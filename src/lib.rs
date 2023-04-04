#![no_std]

mod exclusive_cell;
mod lazy;
mod once;
mod once_lock;

use core::convert::Infallible;

pub use lazy::LazyLock;
pub use once::{Once, OnceState, RawOnce};
pub use once_lock::OnceLock;

fn into_ok<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(err) => match err {},
    }
}
