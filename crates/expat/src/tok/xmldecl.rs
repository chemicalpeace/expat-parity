//! The XML and text declaration: `XmlParseXmlDecl(NS)` (`doParseXmlDecl`,
//! `parsePseudoAttribute`, `checkXmlDeclVersionNum`, `NS(findEncoding)`).
//!
//! The parser calls this on a token that the scanner returned as
//! `XML_TOK_XML_DECL`, i.e. `<?xml ... ?>` in the document's encoding. The
//! declaration is read with [`toAscii`](to_ascii): one character at a time,
//! converted to UTF-8, of which only ASCII matters.
//!
//! ```
//! # #[cfg(feature = "internal-api")] {
//! use expat::internal::tok::{parse_xml_decl, Normal};
//! let d = br#"<?xml version="1.0" encoding="UTF-8" standalone='yes'?>"#;
//! let r = parse_xml_decl(false, &Normal::Utf8, false, d, 0, d.len());
//! assert!(r.ok);
//! assert_eq!(r.version, Some(15));
//! assert_eq!(r.standalone, Some(true));
//! # }
//! ```

// Positions only move forward by one character at a time from `ptr` and stop
// at `end <= buf.len()`, and `end` is lowered by at most two characters after
// checking it is large enough.
// reason: all arithmetic in this module is bounded as stated above.
#![allow(clippy::arithmetic_side_effects)]

use super::init::{Known, get_encoding_index};
use super::scan::name_matches_ascii;
use super::Encoding;

/// `toAscii(enc, ptr, end)`: the first UTF-8 byte of the character at `ptr`
/// if it converts to exactly one byte, else `-1`. The byte is returned as a
/// C `char`, i.e. sign-extended (D-017); converted declaration characters
/// never produce a byte that makes this matter.
pub fn to_ascii<E: Encoding>(enc: &E, buf: &[u8], ptr: usize, end: usize) -> i32 {
    let mut out = [0u8; 1];
    // `XmlUtf8Convert(enc, &ptr, end, ...)`: with an unknown encoding a
    // lead byte just before `end` is converted with the bytes after it
    // (Q-5).
    if enc.to_utf8_until(buf, ptr, end, &mut out).written == 0 {
        -1
    } else {
        let [b] = out;
        i32::from(i8::from_ne_bytes([b]))
    }
}

fn is_space(c: i32) -> bool {
    matches!(c, 0x20 | 0xD | 0xA | 0x9)
}

/// Output of `parsePseudoAttribute`.
struct Pseudo {
    /// `*namePtr`: `None` is C's `NULL` (only white space was left).
    name: Option<usize>,
    name_end: usize,
    val: usize,
}

/// `parsePseudoAttribute`. `Err(next)` is the C `0` with `*nextTokPtr`.
/// On success `ptr` is moved past the closing quote.
fn parse_pseudo_attribute<E: Encoding>(
    enc: &E,
    buf: &[u8],
    ptr: &mut usize,
    end: usize,
) -> Result<Pseudo, usize> {
    let m = E::MINBPC;
    let asc = |p: usize| to_ascii(enc, buf, p, end);
    let none = Pseudo {
        name: None,
        name_end: 0,
        val: 0,
    };
    let mut p = *ptr;
    if p == end {
        return Ok(none);
    }
    if !is_space(asc(p)) {
        return Err(p);
    }
    loop {
        p += m;
        if !is_space(asc(p)) {
            break;
        }
    }
    if p == end {
        // C returns without writing `*nextTokPtr`.
        return Ok(none);
    }
    let name = p;
    let name_end;
    loop {
        let c = asc(p);
        if c == -1 {
            return Err(p);
        }
        if c == i32::from(b'=') {
            name_end = p;
            break;
        }
        if is_space(c) {
            name_end = p;
            let mut c;
            loop {
                p += m;
                c = asc(p);
                if !is_space(c) {
                    break;
                }
            }
            if c != i32::from(b'=') {
                return Err(p);
            }
            break;
        }
        p += m;
    }
    if p == name {
        return Err(p);
    }
    p += m;
    let mut c = asc(p);
    while is_space(c) {
        p += m;
        c = asc(p);
    }
    if c != i32::from(b'"') && c != i32::from(b'\'') {
        return Err(p);
    }
    let open = c;
    p += m;
    let val = p;
    loop {
        let c = asc(p);
        if c == open {
            break;
        }
        let ok = u8::try_from(c)
            .is_ok_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));
        if !ok {
            return Err(p);
        }
        p += m;
    }
    *ptr = p + m;
    Ok(Pseudo {
        name: Some(name),
        name_end,
        val,
    })
}

/// `checkXmlDeclVersionNum`: the value `buf[val..val_end]` is `1.` followed
/// by one or more digits.
fn check_version_num<E: Encoding>(enc: &E, buf: &[u8], val: usize, val_end: usize) -> bool {
    let m = E::MINBPC;
    let start = buf.get(..val + 2 * m).unwrap_or_default();
    if val_end < val || val_end - val < 2 * m || !name_matches_ascii(enc, start, val, b"1.") {
        return false;
    }
    let mut v = val + 2 * m;
    if v == val_end {
        return false;
    }
    while v != val_end {
        let c = to_ascii(enc, buf, v, val_end);
        if !(i32::from(b'0')..=i32::from(b'9')).contains(&c) {
            return false;
        }
        v += m;
    }
    true
}

/// What `NS(findEncoding)` returned for the declared encoding name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FoundEncoding {
    /// The document's own encoding (C returns `enc`): the name is `UTF-16`
    /// and the document is read as UTF-16.
    Input,
    /// A built-in encoding.
    Known(Known),
    /// The name is not a built-in encoding (C: `NULL`); the parser asks the
    /// application's unknown-encoding handler.
    Unrecognized,
}

/// `NS(findEncoding)` on the encoding name `buf[ptr..end]`.
fn find_encoding<E: Encoding>(enc: &E, ns: bool, buf: &[u8], ptr: usize, end: usize) -> FoundEncoding {
    const ENCODING_MAX: usize = 128;
    let from = buf.get(ptr..end).unwrap_or_default();
    let mut name = [0u8; ENCODING_MAX - 1];
    let r = enc.to_utf8(from, &mut name);
    if r.read != from.len() {
        return FoundEncoding::Unrecognized;
    }
    let name = name.get(..r.written).unwrap_or_default();
    let index = get_encoding_index(Some(name));
    if E::MINBPC == 2 && index == Some(super::EncodingIndex::Utf16) {
        return FoundEncoding::Input;
    }
    match index {
        Some(i) => FoundEncoding::Known(Known::from_index(i, ns)),
        None => FoundEncoding::Unrecognized,
    }
}

/// Result of [`parse_xml_decl`]: the C return value and every out-parameter
/// as C leaves it (`None` = not written), as offsets into the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct XmlDecl {
    /// The C return value (`1` = well-formed declaration).
    pub ok: bool,
    /// `*badPtr`: where the declaration is malformed.
    pub bad: Option<usize>,
    /// `*versionPtr`: start of the version value.
    pub version: Option<usize>,
    /// `*versionEndPtr`: just past the closing quote of the version value.
    pub version_end: Option<usize>,
    /// `*encodingName`: start of the encoding name.
    pub encoding_name: Option<usize>,
    /// `*encoding`: the encoding that the name selects.
    pub encoding: Option<FoundEncoding>,
    /// `*standalone`.
    pub standalone: Option<bool>,
}

/// `XmlParseXmlDecl` (`ns == false`) / `XmlParseXmlDeclNS` on the
/// declaration token `buf[ptr..end]` (`<?xml` ... `?>`).
/// `is_general_text_entity` selects the text-declaration rules of an
/// external parsed entity.
pub fn parse_xml_decl<E: Encoding>(
    is_general_text_entity: bool,
    enc: &E,
    ns: bool,
    buf: &[u8],
    ptr: usize,
    end: usize,
) -> XmlDecl {
    let mut d = XmlDecl {
        ok: false,
        bad: None,
        version: None,
        version_end: None,
        encoding_name: None,
        encoding: None,
        standalone: None,
    };
    let fail = |mut d: XmlDecl, bad: usize| {
        d.bad = Some(bad);
        d
    };
    let m = E::MINBPC;
    let mut ptr = ptr + 5 * m;
    // The token has at least `<?xml` and `?>`.
    let Some(end) = end.checked_sub(2 * m).filter(|&e| e >= ptr) else {
        return fail(d, ptr);
    };
    let mut attr = match parse_pseudo_attribute(enc, buf, &mut ptr, end) {
        Ok(a) if a.name.is_some() => a,
        Ok(_) => return fail(d, ptr),
        Err(next) => return fail(d, next),
    };
    let named = |a: &Pseudo, kw: &[u8]| {
        a.name.is_some_and(|n| {
            let s = buf.get(..a.name_end).unwrap_or_default();
            name_matches_ascii(enc, s, n, kw)
        })
    };
    let name_of = |a: &Pseudo| a.name.unwrap_or_default();
    if !named(&attr, b"version") {
        if !is_general_text_entity {
            return fail(d, name_of(&attr));
        }
    } else {
        d.version = Some(attr.val);
        d.version_end = Some(ptr);
        // The version number must not be empty.
        if attr.val == ptr - m {
            return fail(d, attr.val);
        }
        if !check_version_num(enc, buf, attr.val, ptr - m) {
            return fail(d, attr.val);
        }
        attr = match parse_pseudo_attribute(enc, buf, &mut ptr, end) {
            Ok(a) => a,
            Err(next) => return fail(d, next),
        };
        if attr.name.is_none() {
            if is_general_text_entity {
                // a TextDecl must have an EncodingDecl
                return fail(d, ptr);
            }
            d.ok = true;
            return d;
        }
    }
    if named(&attr, b"encoding") {
        let c = to_ascii(enc, buf, attr.val, end);
        if !u8::try_from(c).is_ok_and(|b| b.is_ascii_alphabetic()) {
            return fail(d, attr.val);
        }
        d.encoding_name = Some(attr.val);
        d.encoding = Some(find_encoding(enc, ns, buf, attr.val, ptr - m));
        attr = match parse_pseudo_attribute(enc, buf, &mut ptr, end) {
            Ok(a) => a,
            Err(next) => return fail(d, next),
        };
        if attr.name.is_none() {
            d.ok = true;
            return d;
        }
    }
    if !named(&attr, b"standalone") || is_general_text_entity {
        return fail(d, name_of(&attr));
    }
    let value = buf.get(..ptr - m).unwrap_or_default();
    if name_matches_ascii(enc, value, attr.val, b"yes") {
        d.standalone = Some(true);
    } else if name_matches_ascii(enc, value, attr.val, b"no") {
        d.standalone = Some(false);
    } else {
        return fail(d, attr.val);
    }
    while is_space(to_ascii(enc, buf, ptr, end)) {
        ptr += m;
    }
    if ptr != end {
        return fail(d, ptr);
    }
    d.ok = true;
    d
}
