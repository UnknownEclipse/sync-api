use core::{cell::UnsafeCell, convert::Infallible, fmt::Debug, mem};

use super::once::RawOnce;

pub struct OnceCell<R, T> {
    once: R,
    value: UnsafeCell<Option<T>>,
}

impl<R, T> OnceCell<R, T>
where
    R: RawOnce,
{
    pub const fn new() -> Self {
        Self {
            once: R::INIT,
            value: UnsafeCell::new(None),
        }
    }

    pub const fn with_value(value: T) -> Self {
        Self {
            once: R::COMPLETED,
            value: UnsafeCell::new(Some(value)),
        }
    }

    pub fn get(&self) -> Option<&T> {
        if self.once.is_completed() {
            Some(unsafe { self.get_unchecked() })
        } else {
            None
        }
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.value.get_mut().as_mut()
    }

    /// # Safety
    /// This once cell must be initialized
    pub unsafe fn get_unchecked(&self) -> &T {
        unsafe { (*self.value.get()).as_ref().unwrap_unchecked() }
    }

    pub fn into_inner(self) -> Option<T> {
        self.value.into_inner()
    }

    pub fn take(&mut self) -> Option<T> {
        mem::take(self).into_inner()
    }

    pub fn set(&self, value: T) -> Result<(), T> {
        if self.get().is_some() {
            return Err(value);
        }

        let mut value = Some(value);

        self.once.call(&mut |_| unsafe {
            let value = value.take().unwrap_unchecked();
            *self.value.get() = Some(value);
            true
        });

        match value {
            Some(value) => Err(value),
            None => Ok(()),
        }
    }

    pub fn try_insert(&self, value: T) -> Result<&T, (&T, T)> {
        let result = self.set(value);
        let present = unsafe { self.get_unchecked() };

        match result {
            Ok(_) => Ok(present),
            Err(value) => Err((present, value)),
        }
    }

    pub fn get_or_init<F>(&self, f: F) -> &T
    where
        F: FnOnce() -> T,
    {
        self.get_or_try_init::<_, Infallible>(|| Ok(f())).unwrap()
    }

    pub fn get_or_try_init<F, E>(&self, f: F) -> Result<&T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        if let Some(value) = self.get() {
            Ok(value)
        } else {
            self.get_or_try_init_slow(f)
        }
    }

    #[cold]
    fn get_or_try_init_slow<F, E>(&self, f: F) -> Result<&T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        let mut f = Some(f);
        let mut err = None;

        self.once.call(&mut |_| {
            let f = unsafe { f.take().unwrap_unchecked() };
            let result = f();

            match result {
                Ok(value) => unsafe {
                    *self.value.get() = Some(value);
                    true
                },
                Err(e) => {
                    err = Some(e);
                    false
                }
            }
        });

        if let Some(err) = err {
            Err(err)
        } else {
            Ok(unsafe { self.get_unchecked() })
        }
    }
}

impl<R, T> Clone for OnceCell<R, T>
where
    R: RawOnce,
    T: Clone,
{
    fn clone(&self) -> Self {
        if let Some(v) = self.get() {
            Self::with_value(v.clone())
        } else {
            Self::new()
        }
    }
}

impl<R, T> Debug for OnceCell<R, T>
where
    R: RawOnce,
    T: Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OnceCell")
            .field("value", &self.get())
            .finish()
    }
}

impl<R, T> Default for OnceCell<R, T>
where
    R: RawOnce,
{
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl<R, T> Sync for OnceCell<R, T>
where
    R: Send + Sync,
    T: Send + Sync,
{
}

unsafe impl<R, T> Send for OnceCell<R, T>
where
    R: Send,
    T: Send,
{
}
