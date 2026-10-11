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

use std::ffi::{CString, c_int, c_longlong, c_ulonglong, c_void};
use std::ptr::NonNull;

use crate::ffi;

/// The built-in encodings of `xmltok.c` (`struct normal_encoding`
/// instances), in shim id order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    /// `utf8_encoding`
    Utf8,
    /// `utf8_encoding_ns`
    Utf8Ns,
    /// `internal_utf8_encoding`
    InternalUtf8,
    /// `internal_utf8_encoding_ns`
    InternalUtf8Ns,
    /// `latin1_encoding`
    Latin1,
    /// `latin1_encoding_ns`
    Latin1Ns,
    /// `ascii_encoding`
    Ascii,
    /// `ascii_encoding_ns`
    AsciiNs,
    /// `little2_encoding` (UTF-16LE)
    Little2,
    /// `little2_encoding_ns`
    Little2Ns,
    /// `big2_encoding` (UTF-16BE)
    Big2,
    /// `big2_encoding_ns`
    Big2Ns,
}

impl Builtin {
    /// All built-in encodings.
    pub const ALL: [Builtin; 12] = [
        Builtin::Utf8,
        Builtin::Utf8Ns,
        Builtin::InternalUtf8,
        Builtin::InternalUtf8Ns,
        Builtin::Latin1,
        Builtin::Latin1Ns,
        Builtin::Ascii,
        Builtin::AsciiNs,
        Builtin::Little2,
        Builtin::Little2Ns,
        Builtin::Big2,
        Builtin::Big2Ns,
    ];

    fn id(self) -> c_int {
        match self {
            Builtin::Utf8 => 0,
            Builtin::Utf8Ns => 1,
            Builtin::InternalUtf8 => 2,
            Builtin::InternalUtf8Ns => 3,
            Builtin::Latin1 => 4,
            Builtin::Latin1Ns => 5,
            Builtin::Ascii => 6,
            Builtin::AsciiNs => 7,
            Builtin::Little2 => 8,
            Builtin::Little2Ns => 9,
            Builtin::Big2 => 10,
            Builtin::Big2Ns => 11,
        }
    }
}

/// Number of entries of an unknown-encoding converter table.
pub const CONVERTER_TABLE_LEN: usize = 0x1_0000;

/// An encoding built by `XmlInitUnknownEncoding` / `XmlInitUnknownEncodingNS`.
///
/// Its converter (if any) is the shim's table converter: the code point of
/// a sequence `s` of `n = -map[s[0]]` bytes is
/// `tab[(s[0] << 8) | (s[1] ^ ... ^ s[n-1])]`.
#[derive(Debug)]
pub struct UnknownEncoding {
    handle: NonNull<c_void>,
    map: [i32; 256],
}

impl UnknownEncoding {
    /// Calls the C initialiser. `None` when C rejects the map. `tab`, if
    /// given, must have [`CONVERTER_TABLE_LEN`] entries.
    pub fn new(map: &[i32; 256], tab: Option<&[i32]>, ns: bool) -> Option<Self> {
        if let Some(t) = tab {
            assert_eq!(t.len(), CONVERTER_TABLE_LEN, "converter table length");
        }
        let tab_ptr = tab.map_or(std::ptr::null(), <[i32]>::as_ptr);
        // SAFETY: `map` has 256 entries and `tab` is null or has 65536
        // entries, which is what the shim copies; both are only read.
        let h = unsafe { ffi::oracle_unknown_new(map.as_ptr(), tab_ptr, c_int::from(ns)) };
        NonNull::new(h).map(|handle| UnknownEncoding { handle, map: *map })
    }

    /// Length of the sequence that `b` starts (1 for a single byte).
    pub fn seq_len(&self, b: u8) -> usize {
        match self.map.get(usize::from(b)) {
            Some(-4) => 4,
            Some(-3) => 3,
            Some(-2) => 2,
            _ => 1,
        }
    }

    /// Length of the longest prefix of `buf` that ends on a sequence
    /// boundary.
    pub fn complete_prefix(&self, buf: &[u8]) -> usize {
        let mut p = 0;
        while let Some(&b) = buf.get(p) {
            let n = self.seq_len(b);
            if p + n > buf.len() {
                break;
            }
            p += n;
        }
        p
    }

    fn enc(&self) -> *const c_void {
        // SAFETY: `handle` came from `oracle_unknown_new` and is freed only
        // in `drop`.
        unsafe { ffi::oracle_unknown_enc(self.handle.as_ptr()) }
    }
}

impl Drop for UnknownEncoding {
    fn drop(&mut self) {
        // SAFETY: `handle` came from `oracle_unknown_new` and is freed once.
        unsafe { ffi::oracle_unknown_free(self.handle.as_ptr()) }
    }
}

/// A C encoding: built in, or an [`UnknownEncoding`].
#[derive(Debug, Clone, Copy)]
pub enum Enc<'a> {
    /// A static `struct normal_encoding`.
    Builtin(Builtin),
    /// An application-defined encoding.
    Unknown(&'a UnknownEncoding),
}

impl Enc<'_> {
    fn ptr(self) -> *const c_void {
        match self {
            Enc::Builtin(b) => {
                // SAFETY: plain integer argument; the result is checked.
                let p = unsafe { ffi::oracle_encoding(b.id()) };
                assert!(!p.is_null(), "known encoding id");
                p
            }
            Enc::Unknown(u) => u.enc(),
        }
    }

    /// `minBytesPerChar`.
    pub fn minbpc(self) -> usize {
        match self {
            Enc::Builtin(
                Builtin::Little2 | Builtin::Little2Ns | Builtin::Big2 | Builtin::Big2Ns,
            ) => 2,
            _ => 1,
        }
    }

    /// Appends the ASCII character `c` in this encoding.
    fn push_ascii(self, v: &mut Vec<u8>, c: u8) {
        match self {
            Enc::Builtin(Builtin::Little2 | Builtin::Little2Ns) => v.extend([c, 0]),
            Enc::Builtin(Builtin::Big2 | Builtin::Big2Ns) => v.extend([0, c]),
            _ => v.push(c),
        }
    }
}

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

/// `buf` followed by four stop characters and then `tail`, each ASCII
/// character in the encoding. The stop character is NUL for the built-in
/// encodings (`BT_NONXML` in every table) and `>` for unknown encodings
/// (whose map may make NUL a lead byte but must map `>` to itself); either
/// stops `nameLength`/`skipS` and absorbs a skipped lead byte.
fn padded(enc: Enc<'_>, buf: &[u8], tail: &[u8]) -> Vec<u8> {
    let stop = match enc {
        Enc::Builtin(_) => 0,
        Enc::Unknown(_) => b'>',
    };
    let mut v = buf.to_vec();
    for &c in [stop; 4].iter().chain(tail) {
        enc.push_ascii(&mut v, c);
    }
    v
}

/// Runs scanner `which` on `buf[ptr..]`. Returns the C token and `next`.
pub fn scan(enc: Enc<'_>, which: Scanner, buf: &[u8], ptr: usize) -> (i32, Option<usize>) {
    assert!(ptr <= buf.len(), "ptr within buffer");
    let mut next: c_longlong = -1;
    // SAFETY: `buf` is a live slice of `buf.len()` bytes and `ptr <= len`,
    // the scanners read only inside `[buf + ptr, buf + len)`, and `next` is
    // a valid out-pointer.
    let tok = unsafe {
        ffi::oracle_tok_scan(
            enc.ptr(),
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
pub fn is_public_id(enc: Enc<'_>, buf: &[u8], ptr: usize) -> Result<(), Option<usize>> {
    assert!(ptr <= buf.len(), "ptr within buffer");
    let mut bad: c_longlong = -1;
    // SAFETY: as in `scan`; `isPublicId` reads inside `[buf + ptr, buf + len)`.
    let ok =
        unsafe { ffi::oracle_tok_is_public_id(enc.ptr(), buf.as_ptr(), buf.len(), ptr, &mut bad) };
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
pub fn get_atts(enc: Enc<'_>, buf: &[u8], ptr: usize, atts_max: usize) -> (usize, Vec<Attribute>) {
    assert!(ptr < buf.len() && atts_max <= MAX_ATTS);
    // Ends a tag in every state: `>` outside a value, or closes a "- or
    // '-quoted value and then ends.
    let v = padded(enc, buf, b">\">'>");
    let mut out = vec![0 as c_longlong; 4 * atts_max];
    let max = c_int::try_from(atts_max).expect("<= 64");
    // SAFETY: `v` is terminated as described above, so `getAtts` stops
    // inside it; `out` has room for 4 values per attribute.
    let n = unsafe { ffi::oracle_tok_get_atts(enc.ptr(), v.as_ptr(), ptr, max, out.as_mut_ptr()) };
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
pub fn char_ref_number(enc: Enc<'_>, buf: &[u8], ptr: usize) -> i32 {
    assert!(ptr <= buf.len());
    let v = padded(enc, buf, b";");
    // SAFETY: the padding contains `;`, which ends both loops of
    // `charRefNumber`, and the four NULs before it absorb the `&#x` skip.
    unsafe { ffi::oracle_tok_char_ref_number(enc.ptr(), v.as_ptr(), ptr) }
}

/// `XmlPredefinedEntityName` on the name `buf[ptr..]` (`0` = none).
pub fn predefined_entity_name(enc: Enc<'_>, buf: &[u8], ptr: usize) -> i32 {
    assert!(ptr <= buf.len());
    // SAFETY: reads at most 4 characters, all inside `[buf + ptr, buf + len)`
    // because the switch is on the length of that range.
    unsafe { ffi::oracle_tok_predefined_entity_name(enc.ptr(), buf.as_ptr(), buf.len(), ptr) }
}

/// `XmlNameMatchesAscii` on the name `buf[ptr..]` against `ascii`.
pub fn name_matches_ascii(enc: Enc<'_>, buf: &[u8], ptr: usize, ascii: &[u8]) -> bool {
    assert!(ptr <= buf.len());
    let s = CString::new(ascii).expect("ascii without NUL");
    // SAFETY: `s` is NUL-terminated; the C function checks the remaining
    // length before every read of the name.
    unsafe {
        ffi::oracle_tok_name_matches_ascii(enc.ptr(), buf.as_ptr(), buf.len(), ptr, s.as_ptr()) == 1
    }
}

/// `XmlNameLength` of the name at `buf[ptr]`.
pub fn name_length(enc: Enc<'_>, buf: &[u8], ptr: usize) -> usize {
    assert!(ptr <= buf.len());
    let v = padded(enc, buf, b"");
    // SAFETY: the NUL padding (BT_NONXML) ends the loop inside `v`, also
    // after skipping a lead byte at the end of `buf`.
    off(unsafe { ffi::oracle_tok_name_length(enc.ptr(), v.as_ptr(), ptr) })
}

/// `XmlSkipS` from `buf[ptr]`.
pub fn skip_s(enc: Enc<'_>, buf: &[u8], ptr: usize) -> usize {
    assert!(ptr <= buf.len());
    let v = padded(enc, buf, b"");
    // SAFETY: the NUL padding ends the loop inside `v`.
    off(unsafe { ffi::oracle_tok_skip_s(enc.ptr(), v.as_ptr(), ptr) })
}

/// `XmlUpdatePosition` over `buf[ptr..]` starting from `(line, column)`.
pub fn update_position(enc: Enc<'_>, buf: &[u8], ptr: usize, pos: (u64, u64)) -> (u64, u64) {
    assert!(ptr <= buf.len());
    let mut p: [c_ulonglong; 2] = [pos.0, pos.1];
    // SAFETY: reads only inside `[buf + ptr, buf + len)`; `p` holds 2 values.
    unsafe {
        ffi::oracle_tok_update_position(enc.ptr(), buf.as_ptr(), buf.len(), ptr, p.as_mut_ptr());
    }
    (p[0], p[1])
}

/// Result of a C converter call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Converted {
    /// The `XML_Convert_Result` value.
    pub result: i32,
    /// How far `*fromP` moved, in bytes.
    pub read: usize,
    /// How far `*toP` moved, in output units.
    pub written: usize,
}

fn check_convert_input(enc: Enc<'_>, from: &[u8]) {
    if let Enc::Unknown(u) = enc {
        assert_eq!(
            u.complete_prefix(from),
            from.len(),
            "unknown-encoding converter input must end on a sequence boundary"
        );
    }
}

/// `XmlUtf8Convert(enc, from, to)` with an output of `to_len` bytes. For an
/// unknown encoding `from` must end on a sequence boundary
/// ([`UnknownEncoding::complete_prefix`]); C reads past the end otherwise.
pub fn to_utf8(enc: Enc<'_>, from: &[u8], to_len: usize) -> (Converted, Vec<u8>) {
    check_convert_input(enc, from);
    let mut to = vec![0u8; to_len];
    let (mut read, mut written) = (0usize, 0usize);
    // SAFETY: the converter reads inside `from` (checked above for unknown
    // encodings; the built-in converters check `fromLim`) and writes at most
    // `to_len` bytes into `to`.
    let r = unsafe {
        ffi::oracle_convert(
            enc.ptr(),
            0,
            from.as_ptr(),
            from.len(),
            to.as_mut_ptr().cast(),
            to_len,
            &mut read,
            &mut written,
        )
    };
    to.truncate(written);
    (
        Converted {
            result: r,
            read,
            written,
        },
        to,
    )
}

/// `XmlUtf16Convert(enc, from, to)` with an output of `to_len` units. Same
/// precondition as [`to_utf8`].
pub fn to_utf16(enc: Enc<'_>, from: &[u8], to_len: usize) -> (Converted, Vec<u16>) {
    check_convert_input(enc, from);
    let mut to = vec![0u16; to_len];
    let (mut read, mut written) = (0usize, 0usize);
    // SAFETY: as in `to_utf8`; `to` has room for `to_len` units.
    let r = unsafe {
        ffi::oracle_convert(
            enc.ptr(),
            1,
            from.as_ptr(),
            from.len(),
            to.as_mut_ptr().cast(),
            to_len,
            &mut read,
            &mut written,
        )
    };
    to.truncate(written);
    (
        Converted {
            result: r,
            read,
            written,
        },
        to,
    )
}

/// `_INTERNAL_trim_to_complete_utf8_characters`: the trimmed length.
pub fn trim_utf8(from: &[u8]) -> usize {
    // SAFETY: reads only inside `from`.
    unsafe { ffi::oracle_trim_utf8(from.as_ptr(), from.len()) }
}

impl Builtin {
    fn from_id(id: c_int) -> Option<Builtin> {
        Builtin::ALL.into_iter().find(|b| b.id() == id)
    }
}

/// `initScan` after `XmlInitEncoding(NS)` with `name`: `None` if C rejects
/// the name, else the token, `next`, and the builtin encoding that `*encPtr`
/// points to afterwards (`None`: still the initial encoding). `content`
/// selects `XML_CONTENT_STATE`.
pub fn init_scan(
    name: Option<&[u8]>,
    ns: bool,
    content: bool,
    buf: &[u8],
    ptr: usize,
) -> Option<(i32, Option<usize>, Option<Builtin>)> {
    assert!(ptr <= buf.len());
    let name = name.map(|n| CString::new(n).expect("name without NUL"));
    let mut next: c_longlong = -1;
    let mut selected: c_int = -1;
    // SAFETY: `name` is null or NUL-terminated; initScan reads at most 3
    // bytes and the scanner only `[buf + ptr, buf + len)`; the out-pointers
    // are valid.
    let tok = unsafe {
        ffi::oracle_init_scan(
            name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr()),
            c_int::from(ns),
            c_int::from(content),
            buf.as_ptr(),
            buf.len(),
            ptr,
            &mut next,
            &mut selected,
        )
    };
    if tok == -100 {
        return None;
    }
    let sel = (selected != -1).then(|| Builtin::from_id(selected).expect("builtin id"));
    Some((tok, opt(next), sel))
}

/// `XmlUpdatePosition` of the initial encoding (`initUpdatePosition`).
pub fn init_update_position(buf: &[u8], ptr: usize, pos: (u64, u64)) -> (u64, u64) {
    assert!(ptr <= buf.len());
    let mut p: [c_ulonglong; 2] = [pos.0, pos.1];
    // SAFETY: reads only inside `[buf + ptr, buf + len)`; `p` holds 2 values.
    unsafe { ffi::oracle_init_update_position(buf.as_ptr(), buf.len(), ptr, p.as_mut_ptr()) };
    (p[0], p[1])
}

/// What `findEncoding` returned through `XmlParseXmlDecl`'s `*encoding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundEncoding {
    /// Not written.
    Untouched,
    /// `NULL`: unrecognised name.
    Null,
    /// The input encoding, which is not a builtin.
    Same,
    /// A builtin encoding (possibly the input encoding itself).
    Builtin(Builtin),
}

/// `XmlParseXmlDecl(NS)` results; offsets are `None` when not written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XmlDecl {
    /// Return value.
    pub ok: bool,
    /// `*badPtr`
    pub bad: Option<usize>,
    /// `*versionPtr`
    pub version: Option<usize>,
    /// `*versionEndPtr`
    pub version_end: Option<usize>,
    /// `*encodingName`
    pub encoding_name: Option<usize>,
    /// `*encoding`
    pub encoding: FoundEncoding,
    /// `*standalone`, `None` when not written.
    pub standalone: Option<bool>,
}

/// `XmlParseXmlDecl(NS)` on the declaration `buf[ptr..end]`.
///
/// Requires `end <= buf.len()`, at least 7 characters (`<?xml?>`) and a
/// whole number of characters between `ptr` and `end`; then the C function
/// never moves past `end`. The call runs on a copy of `buf` followed by 8
/// bytes of `>`, because an unknown encoding's converter may read the bytes
/// of a multi-byte sequence that starts just before `end`.
pub fn parse_xml_decl(
    ns: bool,
    is_general_text_entity: bool,
    enc: Enc<'_>,
    buf: &[u8],
    ptr: usize,
    end: usize,
) -> XmlDecl {
    let m = enc.minbpc();
    assert!(end <= buf.len() && ptr + 7 * m <= end && (end - ptr).is_multiple_of(m));
    let mut v = buf.to_vec();
    v.extend([b'>'; 8]);
    let mut out: [c_longlong; 5] = [-1; 5];
    let mut sa: c_int = -7;
    // SAFETY: by the precondition checked above every position the C code
    // reads is below `end`, except converter reads of at most 3 bytes past
    // it, which the padding covers; the out-pointers are valid.
    let ok = unsafe {
        ffi::oracle_parse_xml_decl(
            c_int::from(ns),
            c_int::from(is_general_text_entity),
            enc.ptr(),
            v.as_ptr(),
            ptr,
            end,
            out.as_mut_ptr(),
            &mut sa,
        )
    };
    let encoding = match out[4] {
        -7 => FoundEncoding::Untouched,
        -2 => FoundEncoding::Null,
        -3 => FoundEncoding::Same,
        id => FoundEncoding::Builtin(
            Builtin::from_id(c_int::try_from(id).expect("small")).expect("builtin id"),
        ),
    };
    XmlDecl {
        ok: ok == 1,
        bad: opt(out[0]),
        version: opt(out[1]),
        version_end: opt(out[2]),
        encoding_name: opt(out[3]),
        encoding,
        standalone: match sa {
            -7 => None,
            s => Some(s == 1),
        },
    }
}

/// The prolog state after a token (`PROLOG_STATE`), from the C role machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleState {
    /// Index of the handler in declaration order (`expat::role::Handler::ALL`).
    pub handler: usize,
    /// `level`
    pub level: u32,
    /// `role_none`
    pub role_none: i32,
    /// `includeLevel`
    pub include_level: u32,
    /// `documentEntity`
    pub document_entity: bool,
}

/// One token for [`role_run`]: the C token value and its span in the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleToken {
    /// The `XML_TOK_*` value (any `int`).
    pub tok: i32,
    /// Start of the token.
    pub ptr: usize,
    /// End of the token.
    pub end: usize,
}

/// Feeds `toks` to a fresh C `PROLOG_STATE` (`external`: initialised for an
/// external entity; `init`: then set to this state) and returns the role and
/// the state after each token.
pub fn role_run(
    external: bool,
    init: Option<RoleState>,
    enc: Enc<'_>,
    buf: &[u8],
    toks: &[RoleToken],
) -> Vec<(i32, RoleState)> {
    for t in toks {
        assert!(t.ptr <= t.end && t.end <= buf.len(), "token inside buffer");
    }
    // The handlers compare keywords at `ptr + 2 * MINBPC` without first
    // checking that the token is that long (the read itself is bounded by
    // `end`); the padding keeps even that pointer inside the allocation.
    let mut v = buf.to_vec();
    v.extend([0u8; 8]);
    let n = toks.len();
    let codes: Vec<c_int> = toks.iter().map(|t| t.tok).collect();
    let ptrs: Vec<usize> = toks.iter().map(|t| t.ptr).collect();
    let ends: Vec<usize> = toks.iter().map(|t| t.end).collect();
    let mut roles = vec![0 as c_int; n];
    let mut states = vec![ffi::OracleRoleState::default(); n];
    let init = init.map(|s| ffi::OracleRoleState {
        handler: c_int::try_from(s.handler).expect("small index"),
        level: s.level,
        role_none: s.role_none,
        include_level: s.include_level,
        document_entity: c_int::from(s.document_entity),
    });
    // SAFETY: all arrays have `n` elements; `init` is null or valid; every
    // token lies inside `v` and the handlers read only inside `[ptr, end)`.
    let rc = unsafe {
        ffi::oracle_role_run(
            c_int::from(external),
            init.as_ref().map_or(std::ptr::null(), |i| i),
            enc.ptr(),
            v.as_ptr(),
            n,
            codes.as_ptr(),
            ptrs.as_ptr(),
            ends.as_ptr(),
            roles.as_mut_ptr(),
            states.as_mut_ptr(),
        )
    };
    assert_eq!(rc, 0, "handler index in range");
    roles
        .into_iter()
        .zip(states)
        .map(|(r, s)| {
            (
                r,
                RoleState {
                    handler: usize::try_from(s.handler).expect("known handler"),
                    level: s.level,
                    role_none: s.role_none,
                    include_level: s.include_level,
                    document_entity: s.document_entity != 0,
                },
            )
        })
        .collect()
}
