use core::{
    cell::Cell,
    ops::{Deref, DerefMut},
};

use crate::{OnceLock, RawOnce};

pub struct LazyLock<R, T, F = fn() -> T> {
    cell: OnceLock<R, T>,
    init: Cell<Option<F>>,
}

impl<R, T, F> LazyLock<R, T, F>
where
    R: RawOnce,
{
    pub const fn new(init: F) -> Self {
        Self {
            cell: OnceLock::new(),
            init: Cell::new(Some(init)),
        }
    }
}

impl<R, T, F> LazyLock<R, T, F>
where
    R: RawOnce,
    F: FnOnce() -> T,
{
    pub fn force(this: &Self) -> &T {
        this.cell.get_or_init(|| {
            let init = unsafe { this.init.take().unwrap_unchecked() };
            init()
        })
    }

    pub fn force_mut(this: &mut Self) -> &mut T {
        if this.cell.get_mut().is_none() {
            let init = unsafe { this.init.take().unwrap_unchecked() };
            this.cell = OnceLock::with_value(init());
        }
        unsafe { this.cell.get_mut().unwrap_unchecked() }
    }
}

impl<R, T, F> Deref for LazyLock<R, T, F>
where
    R: RawOnce,
    F: FnOnce() -> T,
{
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        Self::force(self)
    }
}

impl<R, T, F> DerefMut for LazyLock<R, T, F>
where
    R: RawOnce,
    F: FnOnce() -> T,
{
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        Self::force_mut(self)
    }
}

impl<R, T> Default for LazyLock<R, T>
where
    R: RawOnce,
    T: Default,
{
    fn default() -> Self {
        Self::new(Default::default)
    }
}

unsafe impl<R, T, F> Sync for LazyLock<R, T, F>
where
    T: Sync + Send,
    R: Sync + Send,
    F: Send,
{
}
