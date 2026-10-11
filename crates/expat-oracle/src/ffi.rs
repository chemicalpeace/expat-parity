//! Raw declarations of `shim/oracle.h`. Keep in sync with that header.

use std::ffi::{c_char, c_int, c_longlong, c_uint, c_ulonglong, c_ushort, c_void};

/// Mirror of `struct oracle_options`.
#[repr(C)]
pub(crate) struct OracleOptions {
    pub(crate) encoding: *const c_char,
    pub(crate) ns_separator: c_int,
    pub(crate) chunk_size: usize,
    pub(crate) param_entity_parsing: c_int,
}

/// Mirror of `struct oracle_role_state`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OracleRoleState {
    pub(crate) handler: c_int,
    pub(crate) level: c_uint,
    pub(crate) role_none: c_int,
    pub(crate) include_level: c_uint,
    pub(crate) document_entity: c_int,
}

unsafe extern "C" {
    pub(crate) fn oracle_type_table(id: c_int, out: *mut u8) -> c_int;
    pub(crate) fn oracle_ucs2_naming(which: c_int, hi: c_uint, lo: c_uint) -> c_int;
    pub(crate) fn oracle_utf8_predicate(pred: c_int, p: *const u8, len: usize) -> c_int;
    pub(crate) fn oracle_utf8_encode(c: c_int, out: *mut u8) -> c_int;
    pub(crate) fn oracle_utf16_encode(c: c_int, out: *mut c_ushort) -> c_int;
    pub(crate) fn oracle_siphash24(
        key: *const u8,
        data: *const u8,
        len: usize,
        split: usize,
    ) -> u64;
    pub(crate) fn oracle_sip24_valid() -> c_int;
    pub(crate) fn oracle_tok_scan(
        enc: *const c_void,
        which: c_int,
        buf: *const u8,
        len: usize,
        ptr: usize,
        next: *mut c_longlong,
    ) -> c_int;
    pub(crate) fn oracle_tok_is_public_id(
        enc: *const c_void,
        buf: *const u8,
        len: usize,
        ptr: usize,
        bad: *mut c_longlong,
    ) -> c_int;
    pub(crate) fn oracle_tok_get_atts(
        enc: *const c_void,
        buf: *const u8,
        ptr: usize,
        atts_max: c_int,
        out: *mut c_longlong,
    ) -> c_int;
    pub(crate) fn oracle_tok_char_ref_number(
        enc: *const c_void,
        buf: *const u8,
        ptr: usize,
    ) -> c_int;
    pub(crate) fn oracle_tok_predefined_entity_name(
        enc: *const c_void,
        buf: *const u8,
        len: usize,
        ptr: usize,
    ) -> c_int;
    pub(crate) fn oracle_tok_name_matches_ascii(
        enc: *const c_void,
        buf: *const u8,
        len: usize,
        ptr: usize,
        ascii: *const c_char,
    ) -> c_int;
    pub(crate) fn oracle_tok_name_length(
        enc: *const c_void,
        buf: *const u8,
        ptr: usize,
    ) -> c_longlong;
    pub(crate) fn oracle_tok_skip_s(enc: *const c_void, buf: *const u8, ptr: usize) -> c_longlong;
    pub(crate) fn oracle_tok_update_position(
        enc: *const c_void,
        buf: *const u8,
        len: usize,
        ptr: usize,
        pos: *mut c_ulonglong,
    );
    pub(crate) fn oracle_encoding(id: c_int) -> *const c_void;
    pub(crate) fn oracle_convert(
        enc: *const c_void,
        to_utf16: c_int,
        from: *const u8,
        from_len: usize,
        to: *mut c_void,
        to_len: usize,
        read: *mut usize,
        written: *mut usize,
    ) -> c_int;
    pub(crate) fn oracle_trim_utf8(from: *const u8, len: usize) -> usize;
    pub(crate) fn oracle_unknown_new(
        map: *const c_int,
        tab: *const c_int,
        ns: c_int,
    ) -> *mut c_void;
    pub(crate) fn oracle_unknown_enc(h: *const c_void) -> *const c_void;
    pub(crate) fn oracle_unknown_free(h: *mut c_void);
    pub(crate) fn oracle_init_scan(
        name: *const c_char,
        ns: c_int,
        state: c_int,
        buf: *const u8,
        len: usize,
        ptr: usize,
        next: *mut c_longlong,
        selected: *mut c_int,
    ) -> c_int;
    pub(crate) fn oracle_init_update_position(
        buf: *const u8,
        len: usize,
        ptr: usize,
        pos: *mut c_ulonglong,
    );
    pub(crate) fn oracle_parse_xml_decl(
        ns: c_int,
        is_general_text_entity: c_int,
        enc: *const c_void,
        buf: *const u8,
        ptr: usize,
        end: usize,
        out: *mut c_longlong,
        standalone: *mut c_int,
    ) -> c_int;
    pub(crate) fn oracle_role_run(
        external: c_int,
        init: *const OracleRoleState,
        enc: *const c_void,
        buf: *const u8,
        n: usize,
        toks: *const c_int,
        ptrs: *const usize,
        ends: *const usize,
        roles: *mut c_int,
        states: *mut OracleRoleState,
    ) -> c_int;
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
