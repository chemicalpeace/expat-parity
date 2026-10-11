//! The single-byte ("normal") encodings of `xmltok.c`: UTF-8, Latin-1 and
//! US-ASCII, each with and without namespace processing, plus the internal
//! UTF-8 encoding used for parser-generated text.

use super::Encoding;
use super::convert::{self, Converted};
use crate::chartab::{ByteType, Table};
use crate::naming;

/// One of `xmltok.c`'s `struct normal_encoding` instances.
///
/// Latin-1 and ASCII have no lead-byte types in their tables, so their name
/// predicates (C: `NULL_VTABLE`) are never consulted; the UTF-8 predicates
/// are used for all variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Normal {
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
}

impl Normal {
    /// All variants.
    pub const ALL: [Normal; 8] = [
        Normal::Utf8,
        Normal::Utf8Ns,
        Normal::InternalUtf8,
        Normal::InternalUtf8Ns,
        Normal::Latin1,
        Normal::Latin1Ns,
        Normal::Ascii,
        Normal::AsciiNs,
    ];

    /// The byte-type table of this encoding.
    pub fn table(self) -> Table {
        match self {
            Normal::Utf8 => Table::Utf8,
            Normal::Utf8Ns => Table::Utf8Ns,
            Normal::InternalUtf8 => Table::InternalUtf8,
            Normal::InternalUtf8Ns => Table::InternalUtf8Ns,
            Normal::Latin1 => Table::Latin1,
            Normal::Latin1Ns => Table::Latin1Ns,
            Normal::Ascii => Table::Ascii,
            Normal::AsciiNs => Table::AsciiNs,
        }
    }
}

/// `p[0..N]` as an array, or `None` if the bytes are not all in `buf`.
fn bytes<const N: usize>(buf: &[u8], p: usize) -> Option<[u8; N]> {
    buf.get(p..)?.first_chunk::<N>().copied()
}

impl Encoding for Normal {
    const MINBPC: usize = 1;

    fn byte_type(&self, buf: &[u8], p: usize) -> ByteType {
        // Positions past the end only occur where C would read out of
        // bounds; they classify as "not XML", which stops every loop.
        buf.get(p)
            .map_or(ByteType::NonXml, |&b| self.table().byte_type(b))
    }

    fn byte_to_ascii(&self, buf: &[u8], p: usize) -> i32 {
        buf.get(p).map_or(-1, |&b| i32::from(b))
    }

    fn char_matches(&self, buf: &[u8], p: usize, c: u8) -> bool {
        buf.get(p) == Some(&c)
    }

    fn is_name_char_minbpc(&self, _buf: &[u8], _p: usize) -> bool {
        false
    }

    fn is_nmstrt_char_minbpc(&self, _buf: &[u8], _p: usize) -> bool {
        false
    }

    fn is_name_char(&self, buf: &[u8], p: usize, n: usize) -> bool {
        match n {
            2 => bytes(buf, p).is_some_and(naming::utf8_is_name_char2),
            3 => bytes(buf, p).is_some_and(naming::utf8_is_name_char3),
            // `utf8_isName4` is `isNever`: libexpat names are BMP-only.
            _ => false,
        }
    }

    fn is_nmstrt_char(&self, buf: &[u8], p: usize, n: usize) -> bool {
        match n {
            2 => bytes(buf, p).is_some_and(naming::utf8_is_name_start2),
            3 => bytes(buf, p).is_some_and(naming::utf8_is_name_start3),
            _ => false,
        }
    }

    fn is_invalid_char(&self, buf: &[u8], p: usize, n: usize) -> bool {
        match n {
            2 => bytes(buf, p).is_none_or(naming::utf8_is_invalid2),
            3 => bytes(buf, p).is_none_or(naming::utf8_is_invalid3),
            4 => bytes(buf, p).is_none_or(naming::utf8_is_invalid4),
            _ => true,
        }
    }

    fn is_utf8(&self) -> bool {
        !matches!(self, Normal::Latin1 | Normal::Latin1Ns)
    }

    fn to_utf8(&self, from: &[u8], to: &mut [u8]) -> Converted {
        match self {
            Normal::Latin1 | Normal::Latin1Ns => convert::latin1_to_utf8(from, to),
            Normal::Ascii | Normal::AsciiNs => convert::ascii_to_utf8(from, to),
            _ => convert::utf8_to_utf8(from, to),
        }
    }

    fn to_utf16(&self, from: &[u8], to: &mut [u16]) -> Converted {
        match self {
            Normal::Latin1 | Normal::Latin1Ns | Normal::Ascii | Normal::AsciiNs => {
                convert::latin1_to_utf16(from, to)
            }
            _ => convert::utf8_to_utf16(self.table(), from, to),
        }
    }
}
