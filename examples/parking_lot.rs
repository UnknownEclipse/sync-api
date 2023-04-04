use core::{mem, sync::atomic::Ordering};
use std::{
    hint::unreachable_unchecked,
    sync::{atomic::AtomicU8, Barrier},
    thread,
};

use parking_lot_core::{park, unpark_all, unpark_one, DEFAULT_PARK_TOKEN, DEFAULT_UNPARK_TOKEN};
use sync_api::{OnceLock, OnceState, RawOnce};

fn main() {
    let value = OnceLock::<RawPlOnce, _>::new();
    let barrier = Barrier::new(4);

    thread::scope(|s| {
        s.spawn(|| {
            let s = value.get_or_init(|| String::from("follower"));
            barrier.wait();
            assert_eq!(s, "leader");
        });

        for _ in 1..4 {
            s.spawn(|| {
                barrier.wait();
                let s = value.get_or_init(|| String::from("follower"));
                assert_eq!(s, "leader");
            });
        }
    });
}

pub struct RawPlOnce {
    state: AtomicState,
}

impl RawPlOnce {
    #[inline(never)]
    fn acquire(&self) -> Option<OnceState> {
        loop {
            let state = self.state.load(Ordering::Acquire);

            let once_state = match state {
                State::Running => unsafe {
                    park(
                        key(&self.state),
                        || self.state.load(Ordering::Acquire) == State::Running,
                        || {},
                        |_, _| {},
                        DEFAULT_PARK_TOKEN,
                        None,
                    );
                    continue;
                },
                State::Complete => return None,
                State::Incomplete => OnceState::new(),
                State::Poisoned => OnceState::poisoned(),
            };

            if self
                .state
                .compare_exchange(state, State::Running, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return Some(once_state);
            }
        }
    }
}

unsafe impl RawOnce for RawPlOnce {
    #[allow(clippy::declare_interior_mutable_const)]
    const COMPLETE: Self = RawPlOnce {
        state: AtomicState::new(State::Complete),
    };
    #[allow(clippy::declare_interior_mutable_const)]
    const INCOMPLETE: Self = RawPlOnce {
        state: AtomicState::new(State::Incomplete),
    };

    #[inline]
    fn is_completed(&self) -> bool {
        self.state.load(Ordering::Acquire) == State::Complete
    }

    #[cold]
    fn call<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce(&OnceState) -> Result<(), E>,
    {
        let once_state = match self.acquire() {
            Some(once_state) => once_state,
            None => return Ok(()),
        };

        let guard = Guard { state: &self.state };
        f(&once_state)?;
        mem::forget(guard);
        self.state.store(State::Complete, Ordering::Release);

        unsafe { unpark_all(key(&self.state), DEFAULT_UNPARK_TOKEN) };
        Ok(())
    }
}

struct Guard<'a> {
    state: &'a AtomicState,
}

impl<'a> Drop for Guard<'a> {
    fn drop(&mut self) {
        self.state.store(State::Poisoned, Ordering::Release);
        unsafe { unpark_one(key(self.state), |_| DEFAULT_UNPARK_TOKEN) };
    }
}

fn key(state: *const AtomicState) -> usize {
    state as usize
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Incomplete,
    Running,
    Complete,
    Poisoned,
}

impl State {
    #[inline]
    unsafe fn from_u8(byte: u8) -> Self {
        use State::*;

        match byte {
            0 => Incomplete,
            1 => Running,
            2 => Complete,
            3 => Poisoned,
            _ => unsafe { unreachable_unchecked() },
        }
    }
}

pub(crate) struct AtomicState(AtomicU8);

impl AtomicState {
    #[inline]
    pub const fn new(state: State) -> Self {
        Self(AtomicU8::new(state as u8))
    }

    #[inline]
    pub fn compare_exchange(
        &self,
        current: State,
        new: State,
        success: Ordering,
        failure: Ordering,
    ) -> Result<State, State> {
        match self
            .0
            .compare_exchange(current as u8, new as u8, success, failure)
        {
            Ok(v) => unsafe { Ok(State::from_u8(v)) },
            Err(v) => unsafe { Err(State::from_u8(v)) },
        }
    }

    #[inline]
    pub fn load(&self, order: Ordering) -> State {
        unsafe { State::from_u8(self.0.load(order)) }
    }

    #[inline]
    pub fn store(&self, value: State, order: Ordering) {
        self.0.store(value as u8, order);
    }
}
