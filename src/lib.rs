#![no_std]

mod exclusive_cell;
mod lazy;
mod once;
mod once_lock;

pub use lazy::LazyLock;
pub use once::{Once, OnceState, RawOnce};
pub use once_lock::OnceLock;
