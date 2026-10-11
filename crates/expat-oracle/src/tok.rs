//! The C tokenizer (`xmltok_impl.c` through the `ENCODING` vtable).
//!
//! Positions are byte offsets into the buffer. A `next`/`bad` of `None`
//! means the C function did not write that pointer.
//!
//! The C functions without an end pointer (`nameLength`, `skipS`, `getAtts`,
//! `charRefNumber`) read until they find a terminator. Their wrappers here
//! run them on a copy of the buffer followed by a padding that terminates
//! every one of them, so they are safe to call on any input; the result is
//! the C result on that padded input. On input that meets the C precondition
//! (a complete, already-tokenized construct) the padding is never reached.

use std::ffi::{CString, c_int, c_longlong, c_ulonglong};

use crate::{TypeTable, ffi};

/// The scanners of `struct encoding` (`scanners` and `literalScanners`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scanner {
    /// `XmlPrologTok`
    Prolog,
    /// `XmlContentTok`
    Content,
    /// `XmlCdataSectionTok`
    CdataSection,
    /// `XmlIgnoreSectionTok`
    IgnoreSection,
    /// `XmlAttributeValueTok`
    AttributeValue,
    /// `XmlEntityValueTok`
    EntityValue,
}

impl Scanner {
    /// All scanners.
    pub const ALL: [Scanner; 6] = [
        Scanner::Prolog,
        Scanner::Content,
        Scanner::CdataSection,
        Scanner::IgnoreSection,
        Scanner::AttributeValue,
        Scanner::EntityValue,
    ];

    fn id(self) -> c_int {
        match self {
            Scanner::Prolog => 0,
            Scanner::Content => 1,
            Scanner::CdataSection => 2,
            Scanner::IgnoreSection => 3,
            Scanner::AttributeValue => 4,
            Scanner::EntityValue => 5,
        }
    }
}

fn opt(v: c_longlong) -> Option<usize> {
    usize::try_from(v).ok()
}

fn off(v: c_longlong) -> usize {
    usize::try_from(v).expect("C offset is non-negative")
}

/// Bytes per character unit of a table's encoding.
fn minbpc(table: TypeTable) -> usize {
    match table {
        TypeTable::Little2 | TypeTable::Little2Ns => 2,
        _ => 1,
    }
}

/// `buf` followed by four NUL characters (which classify as `BT_NONXML` in
/// every table and so stop `nameLength`/`skipS`, and absorb a skipped lead
/// byte) and then `tail`, each ASCII character widened to the encoding.
fn padded(table: TypeTable, buf: &[u8], tail: &[u8]) -> Vec<u8> {
    let w = minbpc(table);
    let mut v = buf.to_vec();
    for &c in [0u8; 4].iter().chain(tail) {
        v.push(c);
        v.extend(std::iter::repeat_n(0, w - 1));
    }
    v
}

/// Runs scanner `which` on `buf[ptr..]`. Returns the C token and `next`.
pub fn scan(table: TypeTable, which: Scanner, buf: &[u8], ptr: usize) -> (i32, Option<usize>) {
    assert!(ptr <= buf.len(), "ptr within buffer");
    let mut next: c_longlong = -1;
    // SAFETY: `buf` is a live slice of `buf.len()` bytes and `ptr <= len`;
    // the scanners read only inside `[buf + ptr, buf + len)`. `next` is a
    // valid out-pointer.
    let tok = unsafe {
        ffi::oracle_tok_scan(
            table.id(),
            which.id(),
            buf.as_ptr(),
            buf.len(),
            ptr,
            &mut next,
        )
    };
    (tok, opt(next))
}

/// `XmlIsPublicId` on the literal `buf[ptr..]`. `Err(bad)` is the C `0`.
pub fn is_public_id(table: TypeTable, buf: &[u8], ptr: usize) -> Result<(), Option<usize>> {
    assert!(ptr <= buf.len(), "ptr within buffer");
    let mut bad: c_longlong = -1;
    // SAFETY: as in `scan`; `isPublicId` reads inside `[buf + ptr, buf + len)`.
    let ok =
        unsafe { ffi::oracle_tok_is_public_id(table.id(), buf.as_ptr(), buf.len(), ptr, &mut bad) };
    if ok == 1 { Ok(()) } else { Err(opt(bad)) }
}

/// One attribute as reported by `getAtts`, with unwritten fields at 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attribute {
    /// `name`
    pub name: usize,
    /// `valuePtr`
    pub value_ptr: usize,
    /// `valueEnd`
    pub value_end: usize,
    /// `normalized`
    pub normalized: bool,
}

/// Largest `atts_max` accepted by [`get_atts`].
pub const MAX_ATTS: usize = 64;

/// `XmlGetAttributes` on the tag starting at `buf[ptr]`, with room for
/// `atts_max` (at most [`MAX_ATTS`]) attributes. Returns the count and the
/// `atts_max` entries.
pub fn get_atts(
    table: TypeTable,
    buf: &[u8],
    ptr: usize,
    atts_max: usize,
) -> (usize, Vec<Attribute>) {
    assert!(ptr < buf.len() && atts_max <= MAX_ATTS);
    // Ends a tag in every state: `>` outside a value, or closes a "- or
    // '-quoted value and then ends.
    let v = padded(table, buf, b">\">'>");
    let mut out = vec![0 as c_longlong; 4 * atts_max];
    let max = c_int::try_from(atts_max).expect("<= 64");
    // SAFETY: `v` is terminated as described above, so `getAtts` stops
    // inside it; `out` has room for 4 values per attribute.
    let n = unsafe { ffi::oracle_tok_get_atts(table.id(), v.as_ptr(), ptr, max, out.as_mut_ptr()) };
    let atts = out
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&[name, value_ptr, value_end, normalized]| Attribute {
            name: off(name),
            value_ptr: off(value_ptr),
            value_end: off(value_end),
            normalized: normalized != 0,
        })
        .collect();
    (usize::try_from(n).expect("non-negative count"), atts)
}

/// `XmlCharRefNumber` on the reference at `buf[ptr]` (`&#`).
pub fn char_ref_number(table: TypeTable, buf: &[u8], ptr: usize) -> i32 {
    assert!(ptr <= buf.len());
    let v = padded(table, buf, b";");
    // SAFETY: the padding contains `;`, which ends both loops of
    // `charRefNumber`, and the four NULs before it absorb the `&#x` skip.
    unsafe { ffi::oracle_tok_char_ref_number(table.id(), v.as_ptr(), ptr) }
}

/// `XmlPredefinedEntityName` on the name `buf[ptr..]` (`0` = none).
pub fn predefined_entity_name(table: TypeTable, buf: &[u8], ptr: usize) -> i32 {
    assert!(ptr <= buf.len());
    // SAFETY: reads at most 4 characters, all inside `[buf + ptr, buf + len)`
    // because the switch is on the length of that range.
    unsafe { ffi::oracle_tok_predefined_entity_name(table.id(), buf.as_ptr(), buf.len(), ptr) }
}

/// `XmlNameMatchesAscii` on the name `buf[ptr..]` against `ascii`.
pub fn name_matches_ascii(table: TypeTable, buf: &[u8], ptr: usize, ascii: &[u8]) -> bool {
    assert!(ptr <= buf.len());
    let s = CString::new(ascii).expect("ascii without NUL");
    // SAFETY: `s` is NUL-terminated; the C function checks the remaining
    // length before every read of the name.
    unsafe {
        ffi::oracle_tok_name_matches_ascii(table.id(), buf.as_ptr(), buf.len(), ptr, s.as_ptr())
            == 1
    }
}

/// `XmlNameLength` of the name at `buf[ptr]`.
pub fn name_length(table: TypeTable, buf: &[u8], ptr: usize) -> usize {
    assert!(ptr <= buf.len());
    let v = padded(table, buf, b"");
    // SAFETY: the NUL padding (BT_NONXML) ends the loop inside `v`, also
    // after skipping a lead byte at the end of `buf`.
    off(unsafe { ffi::oracle_tok_name_length(table.id(), v.as_ptr(), ptr) })
}

/// `XmlSkipS` from `buf[ptr]`.
pub fn skip_s(table: TypeTable, buf: &[u8], ptr: usize) -> usize {
    assert!(ptr <= buf.len());
    let v = padded(table, buf, b"");
    // SAFETY: the NUL padding ends the loop inside `v`.
    off(unsafe { ffi::oracle_tok_skip_s(table.id(), v.as_ptr(), ptr) })
}

/// `XmlUpdatePosition` over `buf[ptr..]` starting from `(line, column)`.
pub fn update_position(table: TypeTable, buf: &[u8], ptr: usize, pos: (u64, u64)) -> (u64, u64) {
    assert!(ptr <= buf.len());
    let mut p: [c_ulonglong; 2] = [pos.0, pos.1];
    // SAFETY: reads only inside `[buf + ptr, buf + len)`; `p` holds 2 values.
    unsafe {
        ffi::oracle_tok_update_position(table.id(), buf.as_ptr(), buf.len(), ptr, p.as_mut_ptr());
    }
    (p[0], p[1])
}
