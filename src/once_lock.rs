use core::{cell::UnsafeCell, convert::Infallible, fmt::Debug, mem};

use super::once::RawOnce;
use crate::into_ok;

pub struct OnceLock<R, T> {
    once: R,
    value: UnsafeCell<Option<T>>,
}

impl<R, T> OnceLock<R, T>
where
    R: RawOnce,
{
    pub const fn new() -> Self {
        Self {
            once: R::INCOMPLETE,
            value: UnsafeCell::new(None),
        }
    }

    pub const fn with_value(value: T) -> Self {
        Self {
            once: R::COMPLETE,
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

        let res = self.once.call(|_| unsafe {
            *self.value.get() = value.take();
            Ok::<_, Infallible>(())
        });
        into_ok(res);

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
        into_ok(self.get_or_try_init::<_, Infallible>(|| Ok(f())))
    }

    pub fn get_or_try_init<F, E>(&self, f: F) -> Result<&T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        if let Some(value) = self.get() {
            Ok(value)
        } else {
            self.once.call(|_| {
                let value = f()?;
                unsafe {
                    *self.value.get() = Some(value);
                }
                Ok(())
            })?;
            Ok(unsafe { self.get_unchecked() })
        }
    }

    // #[cold]
    // fn initialize<F, E>(&self, f: F) -> Result<(), E>
    // where
    //     F: FnOnce() -> Result<T, E>,
    // {
    //     let mut result = Ok(());
    //     let slot = self.value.get();

    //     self.once.call(|once| match f() {
    //         Ok(value) => unsafe {
    //             (*slot) = Some(value);
    //         },
    //         Err(err) => {
    //             result = Err(err);
    //         }
    //     });
    // }
}

impl<R, T> Clone for OnceLock<R, T>
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

impl<R, T> Debug for OnceLock<R, T>
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

impl<R, T> Default for OnceLock<R, T>
where
    R: RawOnce,
{
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl<R, T> Sync for OnceLock<R, T>
where
    R: Send + Sync,
    T: Send + Sync,
{
}

unsafe impl<R, T> Send for OnceLock<R, T>
where
    R: Send,
    T: Send,
{
}
