use std::{
    hint::{self, unreachable_unchecked},
    sync::atomic::{AtomicU8, Ordering},
};

use crate::{once::OnceState, RawOnce};

pub struct RawSpinOnce {
    state: AtomicState,
}

impl RawSpinOnce {
    fn wait_busy(&self) {
        while self.state.load(Ordering::Acquire) == State::Busy {
            hint::spin_loop()
        }
    }
}

unsafe impl RawOnce for RawSpinOnce {
    #[allow(clippy::declare_interior_mutable_const)]
    const COMPLETED: Self = Self {
        state: AtomicState::new(State::Init),
    };
    #[allow(clippy::declare_interior_mutable_const)]
    const INIT: Self = Self {
        state: AtomicState::new(State::Empty),
    };

    #[inline]
    fn is_completed(&self) -> bool {
        self.state.load(Ordering::Acquire) == State::Init
    }

    #[cold]
    #[inline(never)]
    fn call(&self, f: &mut dyn FnMut(&OnceState) -> bool) {
        struct Guard<'a> {
            state: &'a AtomicState,
            new: State,
        }

        impl<'a> Drop for Guard<'a> {
            fn drop(&mut self) {
                self.state.store(self.new, Ordering::Release);
            }
        }

        loop {
            let state = self.state.load(Ordering::Acquire);

            match state {
                State::Busy => {
                    self.wait_busy();
                    continue;
                }
                State::Init => return,
                State::Empty | State::Poison => {}
            }

            if self
                .state
                .compare_exchange(state, State::Busy, Ordering::AcqRel, Ordering::Relaxed)
                .is_err()
            {
                continue;
            }

            let mut state = OnceState::new();
            state.poison();

            let mut guard = Guard {
                new: State::Poison,
                state: &self.state,
            };

            if f(&state) {
                guard.new = State::Init
            }
            return;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Empty,
    Busy,
    Init,
    Poison,
}

impl State {
    #[inline]
    unsafe fn from_u8(byte: u8) -> Self {
        use State::*;

        match byte {
            0 => Empty,
            1 => Busy,
            2 => Init,
            3 => Poison,
            _ => unsafe { unreachable_unchecked() },
        }
    }
}

struct AtomicState(AtomicU8);

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
