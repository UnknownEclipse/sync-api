// use core::{cell::UnsafeCell, convert::Infallible};

// use crate::RawOnce;

// pub struct ExclusiveCell<R, T> {
//     cell: UnsafeCell<Option<T>>,
//     once: R,
// }

// #[derive(Debug)]
// pub enum TryInitError<E> {
//     AlreadyInitialized,
//     Err(E),
// }

// #[derive(Debug)]
// pub enum TryInitError<E> {
//     AlreadyInitialized,
//     Err(E),
// }

// impl<R, T> ExclusiveCell<R, T>
// where
//     R: RawOnce,
// {
//     pub const fn new() -> Self {
//         Self {
//             cell: UnsafeCell::new(None),
//             once: R::INIT,
//         }
//     }

//     pub fn init<F>(&self, f: F) -> Option<&mut T>
//     where
//         F: FnOnce() -> T,
//     {
//         self.try_init(|| Ok::<_, Infallible>(f())).ok()
//     }

//     pub fn try_init<F, E>(&self, f: F) -> Result<&mut T, TryInitError<E>>
//     where
//         F: FnOnce() -> Result<T, E>,
//     {
//         let mut f = Some(f);
//         let mut error = None;

//         self.once.call(&mut |_| {
//             let f = unsafe { f.take().unwrap_unchecked() };

//             match f() {
//                 Ok(value) => {
//                     unsafe {
//                         *self.cell.get() = Some(value);
//                     }
//                     true
//                 }
//                 Err(err) => {
//                     error = Some(err);
//                     false
//                 }
//             }
//         });

//         if f.is_some() {
//             Err(TryInitError::AlreadyInitialized)
//         } else if let Some(err) = error {
//             Err(TryInitError::Err(err))
//         } else {
//             unsafe {
//                 let value = &mut *self.cell.get();
//                 Ok(value.as_mut().unwrap_unchecked())
//             }
//         }
//     }
// }

// unsafe impl<R, T> Sync for ExclusiveCell<R, T>
// where
//     R: Sync,
//     T: Send,
// {
// }
