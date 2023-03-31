#![no_std]

mod lazy;
mod once;
mod once_cell;
// mod raw_spin;
// mod raw_std;

pub use lazy::Lazy;
pub use once::{Once, OnceState, RawOnce};
pub use once_cell::OnceCell;
