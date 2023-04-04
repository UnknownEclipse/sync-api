use core::{convert::Infallible, fmt::Debug};

#[derive(Debug)]
pub struct OnceState {
    is_poisoned: bool,
}

impl OnceState {
    pub fn new() -> Self {
        Self { is_poisoned: false }
    }

    pub fn poisoned() -> Self {
        Self { is_poisoned: true }
    }

    pub fn poison(&mut self) -> &mut Self {
        self.is_poisoned = true;
        self
    }

    pub fn is_poisoned(&self) -> bool {
        self.is_poisoned
    }
}

impl Default for OnceState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Once<R> {
    raw: R,
}

impl<R> Once<R>
where
    R: RawOnce,
{
    pub const fn new() -> Self {
        Self { raw: R::INCOMPLETE }
    }

    pub fn call_once<F>(&self, f: F)
    where
        F: FnOnce(),
    {
        self.call_once_force(|state| {
            if state.is_poisoned() {
                panic!("Once poisoned");
            }
            f();
        })
    }

    pub fn call_once_force<F>(&self, f: F)
    where
        F: FnOnce(&OnceState),
    {
        let r = self.try_call_once_force(|once_state| {
            f(once_state);
            Ok::<_, Infallible>(())
        });

        match r {
            Ok(_) => {}
            Err(e) => match e {},
        }
    }

    pub fn try_call_once<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce() -> Result<(), E>,
    {
        self.try_call_once_force(|state| {
            if state.is_poisoned() {
                panic!("Once poisoned")
            }
            f()
        })
    }

    pub fn try_call_once_force<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce(&OnceState) -> Result<(), E>,
    {
        if self.is_completed() {
            Ok(())
        } else {
            self.raw.call(f)
        }
    }

    pub fn is_completed(&self) -> bool {
        self.raw.is_completed()
    }
}

impl<R> Debug for Once<R>
where
    R: RawOnce,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Once")
            .field("completed", &self.is_completed())
            .finish_non_exhaustive()
    }
}

impl<R> Default for Once<R>
where
    R: RawOnce,
{
    fn default() -> Self {
        Self::new()
    }
}

/// The Once primitive responsible for managing all synchronization.
///
/// # Safety
/// The `[try_]call_once` methods must work correctly. Other primitives depend on this
/// contract for their own safety.
pub unsafe trait RawOnce {
    const INCOMPLETE: Self;
    const COMPLETE: Self;

    /// Check if the once has completed successfully.
    fn is_completed(&self) -> bool;

    /// Call a function exactly once.
    ///
    /// Multiple threads may call this function, but only one function will be executed.
    /// If a function panics or returns false, the once will not be marked as completed
    /// and further operations may be called.
    ///
    /// If the called function returns `false`, the once is marked as poisoned. `RawOnce`
    /// implementors should not explicitly panic when they are poisoned. Rather, they
    /// should pass a poisoned once state to the function, and the higher level type
    /// will handle poisoning correctly.
    ///
    /// This is intentionally monomorphic and as flexible as possible
    /// to minimize code size. Think of it as a monomorphized `try_call_once_force`,
    /// if something like it existed in std.
    ///
    /// This is a single function designed to be called in the slow
    /// path, so implementations will likely wish to annotate it with #[cold] and
    /// #[inline(never)]
    fn call<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce(&OnceState) -> Result<(), E>;
}
