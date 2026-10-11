//! Raw declarations of `shim/oracle.h`. Keep in sync with that header.

use std::ffi::{c_char, c_int, c_uint, c_ushort, c_void};

/// Mirror of `struct oracle_options`.
#[repr(C)]
pub(crate) struct OracleOptions {
    pub(crate) encoding: *const c_char,
    pub(crate) ns_separator: c_int,
    pub(crate) chunk_size: usize,
    pub(crate) param_entity_parsing: c_int,
}

unsafe extern "C" {
    pub(crate) fn oracle_type_table(id: c_int, out: *mut u8) -> c_int;
    pub(crate) fn oracle_ucs2_naming(which: c_int, hi: c_uint, lo: c_uint) -> c_int;
    pub(crate) fn oracle_utf8_predicate(pred: c_int, p: *const u8, len: usize) -> c_int;
    pub(crate) fn oracle_utf8_encode(c: c_int, out: *mut u8) -> c_int;
    pub(crate) fn oracle_utf16_encode(c: c_int, out: *mut c_ushort) -> c_int;
    pub(crate) fn oracle_parse_trace(
        doc: *const u8,
        len: usize,
        opts: *const OracleOptions,
        out: *mut *mut u8,
        out_len: *mut usize,
    ) -> c_int;
    pub(crate) fn oracle_free(p: *mut c_void);
    pub(crate) fn oracle_expat_version() -> *const c_char;
}
