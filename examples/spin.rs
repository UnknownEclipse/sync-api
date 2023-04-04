use core::mem;
use std::{
    hint::{self, unreachable_unchecked},
    sync::{
        atomic::{AtomicU8, Ordering},
        Barrier,
    },
    thread,
};

use sync_api::{OnceLock, OnceState, RawOnce};

fn main() {
    let value = SpinOnceLock::new();
    let barrier = Barrier::new(4);

    thread::scope(|s| {
        s.spawn(|| {
            let s = value.get_or_init(|| String::from("leader"));
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

type SpinOnceLock<T> = OnceLock<RawSpinOnce, T>;

struct RawSpinOnce {
    state: AtomicState,
}

impl RawSpinOnce {
    fn wait_while_running(&self) {
        while self.state.load(Ordering::Acquire) == State::Running {
            hint::spin_loop();
        }
    }

    fn try_acquire(&self) -> Option<OnceState> {
        loop {
            let state = self.state.load(Ordering::Acquire);

            let once_state = match state {
                State::Running => {
                    self.wait_while_running();
                    continue;
                }
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

    fn finish_init(&self, guard: Guard<'_>) {
        mem::forget(guard);
        self.state.store(State::Complete, Ordering::Release);
    }
}

unsafe impl RawOnce for RawSpinOnce {
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

    #[cold]
    fn call<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce(&OnceState) -> Result<(), E>,
    {
        let once_state = match self.try_acquire() {
            Some(v) => v,
            None => return Ok(()),
        };

        let guard = Guard { state: &self.state };

        f(&once_state)?;
        self.finish_init(guard);
        Ok(())
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
