use core::{mem, sync::atomic::Ordering};
use std::{
    hint::unreachable_unchecked,
    sync::{atomic::AtomicU8, Barrier},
    thread,
};

use sync_api::{OnceLock, OnceState, RawOnce};

fn main() {
    let value = OnceLock::<RawCsOnce, _>::new();
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

pub struct RawCsOnce {
    state: AtomicState,
}

unsafe impl RawOnce for RawCsOnce {
    #[allow(clippy::declare_interior_mutable_const)]
    const COMPLETE: Self = Self {
        state: AtomicState::new(State::Complete),
    };
    #[allow(clippy::declare_interior_mutable_const)]
    const INCOMPLETE: Self = Self {
        state: AtomicState::new(State::Incomplete),
    };

    #[inline]
    fn is_completed(&self) -> bool {
        self.state.load(Ordering::Acquire) == State::Complete
    }

    fn call<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce(&OnceState) -> Result<(), E>,
    {
        critical_section::with(|_cs| {
            // Acquire might not be entirely necessary, but the perf impact will be
            // minimal.
            let state = self.state.load(Ordering::Acquire);

            let once_state = match state {
                State::Running => panic!("reentrant once call"),
                State::Complete => return Ok(()),
                State::Poisoned => OnceState::poisoned(),
                State::Incomplete => OnceState::new(),
            };

            // The critical section guarantees no other threads will be writing to the state,
            // so relaxed is fine.
            self.state.store(State::Running, Ordering::Relaxed);

            let guard = Guard { state: &self.state };

            f(&once_state)?;
            mem::forget(guard);
            self.state.store(State::Complete, Ordering::Release);
            Ok(())
        })
    }
}

pub(crate) struct Guard<'a> {
    pub state: &'a AtomicState,
}

impl<'a> Drop for Guard<'a> {
    fn drop(&mut self) {
        self.state.store(State::Poisoned, Ordering::Release);
    }
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
    pub fn load(&self, order: Ordering) -> State {
        unsafe { State::from_u8(self.0.load(order)) }
    }

    #[inline]
    pub fn store(&self, value: State, order: Ordering) {
        self.0.store(value as u8, order);
    }
}
