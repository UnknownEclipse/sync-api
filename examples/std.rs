use std::{
    cell::Cell,
    sync::{
        atomic::{AtomicBool, AtomicPtr, Ordering},
        Barrier,
    },
    thread::{self, Thread},
};

use sync_api::{OnceLock, OnceState, RawOnce};

fn main() {
    let value = OnceLock::<RawStdOnce, _>::new();
    let barrier = Barrier::new(4);

    thread::scope(|s| {
        s.spawn(|| {
            let s = value.get_or_init(|| String::from("follower"));
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

pub struct RawStdOnce {
    queue: AtomicPtr<Waiter>,
}

unsafe impl RawOnce for RawStdOnce {
    #[allow(clippy::declare_interior_mutable_const)]
    const COMPLETE: Self = Self {
        queue: AtomicPtr::new(INCOMPLETE_PTR),
    };
    #[allow(clippy::declare_interior_mutable_const)]
    const INCOMPLETE: Self = Self {
        queue: AtomicPtr::new(COMPLETE_PTR),
    };

    #[inline]
    fn is_completed(&self) -> bool {
        self.queue.load(Ordering::Acquire) == COMPLETE_PTR
    }

    #[inline]
    fn call<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce(&OnceState) -> Result<(), E>,
    {
        let mut f = Some(f);
        let mut err = None;

        initialize_or_wait(&self.queue, &mut |once_state| {
            let f = unsafe { f.take().unwrap_unchecked() };
            match f(once_state) {
                Ok(_) => true,
                Err(e) => {
                    err = Some(e);
                    false
                }
            }
        });

        match err {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }
}

// Four states that a Once can be in, encoded into the lower bits of `queue` in
// the Once structure.
const INCOMPLETE: usize = 0x0;
const RUNNING: usize = 0x1;
const COMPLETE: usize = 0x2;
const POISONED: usize = 0x3;
const INCOMPLETE_PTR: *mut Waiter = INCOMPLETE as *mut Waiter;
const COMPLETE_PTR: *mut Waiter = COMPLETE as *mut Waiter;
const POISONED_PTR: *mut Waiter = POISONED as *mut Waiter;

// Mask to learn about the state. All other bits are the queue of waiters if
// this is in the RUNNING state.
const STATE_MASK: usize = 0x3;

/// Representation of a node in the linked list of waiters in the RUNNING state.
/// A waiters is stored on the stack of the waiting threads.
#[repr(align(4))] // Ensure the two lower bits are free to use as state bits.
struct Waiter {
    thread: Cell<Option<Thread>>,
    signaled: AtomicBool,
    next: *mut Waiter,
}

/// Drains and notifies the queue of waiters on drop.
struct Guard<'a> {
    queue: &'a AtomicPtr<Waiter>,
    new_queue: *mut Waiter,
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        let queue = self.queue.swap(self.new_queue, Ordering::AcqRel);

        let state = strict::addr(queue) & STATE_MASK;
        assert_eq!(state, RUNNING);

        unsafe {
            let mut waiter = strict::map_addr(queue, |q| q & !STATE_MASK);
            while !waiter.is_null() {
                let next = (*waiter).next;
                let thread = (*waiter).thread.take().unwrap();
                (*waiter).signaled.store(true, Ordering::Release);
                waiter = next;
                thread.unpark();
            }
        }
    }
}

// Corresponds to `std::sync::Once::call_inner`.
//
// Originally copied from std, but since modified to remove poisoning and to
// support wait.
//
// Note: this is intentionally monomorphic
#[inline(never)]
fn initialize_or_wait(queue: &AtomicPtr<Waiter>, mut init: &mut dyn FnMut(&OnceState) -> bool) {
    let mut curr_queue = queue.load(Ordering::Acquire);

    loop {
        let curr_state = strict::addr(curr_queue) & STATE_MASK;
        match (curr_state, &mut init) {
            (COMPLETE, _) => return,
            (INCOMPLETE | POISONED, init) => {
                let exchange = queue.compare_exchange(
                    curr_queue,
                    strict::map_addr(curr_queue, |q| (q & !STATE_MASK) | RUNNING),
                    Ordering::Acquire,
                    Ordering::Acquire,
                );
                if let Err(new_queue) = exchange {
                    curr_queue = new_queue;
                    continue;
                }
                let mut guard = Guard {
                    queue,
                    new_queue: POISONED_PTR,
                };

                let mut once_state = OnceState::new();
                if curr_state == POISONED {
                    once_state.poison();
                }

                if init(&once_state) {
                    guard.new_queue = COMPLETE_PTR;
                }
                return;
            }
            (RUNNING, _) => {
                wait(queue, curr_queue);
                curr_queue = queue.load(Ordering::Acquire);
            }
            _ => debug_assert!(false),
        }
    }
}

fn wait(queue: &AtomicPtr<Waiter>, mut curr_queue: *mut Waiter) {
    let curr_state = strict::addr(curr_queue) & STATE_MASK;
    loop {
        let node = Waiter {
            thread: Cell::new(Some(thread::current())),
            signaled: AtomicBool::new(false),
            next: strict::map_addr(curr_queue, |q| q & !STATE_MASK),
        };
        let me = &node as *const Waiter as *mut Waiter;

        let exchange = queue.compare_exchange(
            curr_queue,
            strict::map_addr(me, |q| q | curr_state),
            Ordering::Release,
            Ordering::Relaxed,
        );
        if let Err(new_queue) = exchange {
            if strict::addr(new_queue) & STATE_MASK != curr_state {
                return;
            }
            curr_queue = new_queue;
            continue;
        }

        while !node.signaled.load(Ordering::Acquire) {
            thread::park();
        }
        break;
    }
}

// Polyfill of strict provenance from https://crates.io/crates/sptr.
//
// Use free-standing function rather than a trait to keep things simple and
// avoid any potential conflicts with future stabile std API.
mod strict {
    #[must_use]
    #[inline]
    #[allow(clippy::transmutes_expressible_as_ptr_casts)]
    pub(crate) fn addr<T>(ptr: *mut T) -> usize
    where
        T: Sized,
    {
        // FIXME(strict_provenance_magic): I am magic and should be a compiler intrinsic.
        // SAFETY: Pointer-to-integer transmutes are valid (if you are okay with losing the
        // provenance).
        unsafe { core::mem::transmute(ptr) }
    }

    #[must_use]
    #[inline]
    pub(crate) fn with_addr<T>(ptr: *mut T, addr: usize) -> *mut T
    where
        T: Sized,
    {
        // FIXME(strict_provenance_magic): I am magic and should be a compiler intrinsic.
        //
        // In the mean-time, this operation is defined to be "as if" it was
        // a wrapping_offset, so we can emulate it as such. This should properly
        // restore pointer provenance even under today's compiler.
        let self_addr = self::addr(ptr) as isize;
        let dest_addr = addr as isize;
        let offset = dest_addr.wrapping_sub(self_addr);

        // This is the canonical desugarring of this operation,
        // but `pointer::cast` was only stabilized in 1.38.
        // self.cast::<u8>().wrapping_offset(offset).cast::<T>()
        (ptr as *mut u8).wrapping_offset(offset) as *mut T
    }

    #[must_use]
    #[inline]
    pub(crate) fn map_addr<T>(ptr: *mut T, f: impl FnOnce(usize) -> usize) -> *mut T
    where
        T: Sized,
    {
        self::with_addr(ptr, f(addr(ptr)))
    }
}
