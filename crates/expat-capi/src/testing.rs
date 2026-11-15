//! libexpat's `XML_TESTING` hooks, for running libexpat's own test suite
//! (`tests/runtests`) against this library (`scripts/runtests-check.sh`,
//! D-060). Built only with the `testing` feature; not part of the
//! interface.

// reason: the names are those of libexpat's test build.
#![allow(non_upper_case_globals, non_snake_case)]

use std::ffi::c_ulonglong;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

use crate::{XML_Parser, state};

/// `g_reparseDeferralEnabledDefault`: the reparse deferral setting of new
/// and reset parsers (an `XML_Bool` the test suite writes).
#[unsafe(no_mangle)]
pub static g_reparseDeferralEnabledDefault: AtomicU8 = AtomicU8::new(1);

/// `g_bytesScanned`: the bytes handed to the processors by every parser
/// (an `unsigned int` the test suite resets and reads).
#[unsafe(no_mangle)]
pub static g_bytesScanned: AtomicU32 = AtomicU32::new(0);

/// The value of `g_reparseDeferralEnabledDefault`.
pub(crate) fn reparse_deferral_default() -> bool {
    g_reparseDeferralEnabledDefault.load(Ordering::Relaxed) != 0
}

/// Adds the bytes one parsing call scanned to `g_bytesScanned` (libexpat
/// counts in `unsigned int` and wraps).
pub(crate) fn add_bytes_scanned(n: u64) {
    // reason: C's `(unsigned)have_now`, wrapping.
    #[allow(clippy::cast_possible_truncation)]
    g_bytesScanned.fetch_add(n as u32, Ordering::Relaxed);
}

/// `testingAccountingGetCountBytesDirect`: the bytes read directly so far
/// (0 for `NULL` or while the parser is busy).
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn testingAccountingGetCountBytesDirect(parser: XML_Parser) -> c_ulonglong {
    // SAFETY: forwarded from this function's contract.
    unsafe { state(parser) }
        .and_then(|s| s.with_parser(|p| p.amplification_counts().0))
        .unwrap_or(0)
}

/// `testingAccountingGetCountBytesIndirect`: the bytes read from entity
/// expansions so far (0 for `NULL` or while the parser is busy).
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn testingAccountingGetCountBytesIndirect(parser: XML_Parser) -> c_ulonglong {
    // SAFETY: forwarded from this function's contract.
    unsafe { state(parser) }
        .and_then(|s| s.with_parser(|p| p.amplification_counts().1))
        .unwrap_or(0)
}
