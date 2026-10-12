//! A counting global allocator for bounded tests (module 11).
//!
//! It lives in this crate because it needs `unsafe` (D-011 allows `unsafe`
//! only in the oracle and the C API crates). Test binaries install it with
//! `#[global_allocator]`; it then counts the live bytes allocated by each
//! thread and refuses requests above that thread's cap, so a runaway
//! allocation in a test aborts the test binary (through
//! `handle_alloc_error`) instead of exhausting the machine.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static LIVE: Cell<usize> = const { Cell::new(0) };
    static PEAK: Cell<usize> = const { Cell::new(0) };
    static CAP: Cell<usize> = const { Cell::new(usize::MAX) };
}

/// Global allocator that counts the live bytes allocated by each thread and
/// refuses requests above that thread's cap (`usize::MAX` unless set by
/// [`reset`]). Frees on another thread than the allocation only lower
/// that thread's count (saturating), which can make a cap looser, never
/// stricter.
#[derive(Debug)]
pub struct CountingAlloc;

fn charge(n: usize) -> bool {
    LIVE.try_with(|live| {
        let cap = CAP.try_with(Cell::get).unwrap_or(usize::MAX);
        let Some(new) = live.get().checked_add(n) else {
            return false;
        };
        if new > cap {
            return false;
        }
        live.set(new);
        let _ = PEAK.try_with(|p| {
            if new > p.get() {
                p.set(new);
            }
        });
        true
    })
    .unwrap_or(true)
}

fn release(n: usize) {
    let _ = LIVE.try_with(|live| live.set(live.get().saturating_sub(n)));
}

// SAFETY: every method forwards to `System` with the caller's layout and
// only adds bookkeeping in thread-local `Cell`s with const initialisers
// (no allocation, no destructor), so the `GlobalAlloc` contract is the
// one `System` fulfils.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !charge(layout.size()) {
            return std::ptr::null_mut();
        }
        // SAFETY: forwarded unchanged.
        let p = unsafe { System.alloc(layout) };
        if p.is_null() {
            release(layout.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if !charge(layout.size()) {
            return std::ptr::null_mut();
        }
        // SAFETY: forwarded unchanged.
        let p = unsafe { System.alloc_zeroed(layout) };
        if p.is_null() {
            release(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        release(layout.size());
        // SAFETY: forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old = layout.size();
        if new_size > old && !charge(new_size - old) {
            return std::ptr::null_mut();
        }
        // SAFETY: forwarded unchanged.
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if p.is_null() {
            if new_size > old {
                release(new_size - old);
            }
        } else if new_size < old {
            release(old - new_size);
        }
        p
    }
}

/// Sets the calling thread's live count and peak to 0 and its cap to
/// `cap` bytes (`usize::MAX`: no cap).
pub fn reset(cap: usize) {
    LIVE.with(|l| l.set(0));
    PEAK.with(|p| p.set(0));
    CAP.with(|c| c.set(cap));
}

/// The calling thread's peak live bytes since [`reset`].
pub fn peak() -> usize {
    PEAK.with(Cell::get)
}
