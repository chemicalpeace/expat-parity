//! The UTF-16 encodings of `xmltok.c` (`little2_` and `big2_`, `MINBPC` 2),
//! each with and without namespace processing.
//!
//! A code unit below 0x100 is classified by the Latin-1 table; any other unit
//! by `unicode_byte_type`: a high surrogate is a 4-byte lead
//! ([`ByteType::Lead4`]), a low surrogate is [`ByteType::Trail`], U+FFFE and
//! U+FFFF are [`ByteType::NonXml`] and everything else is
//! [`ByteType::NonAscii`], whose name properties come from the BMP naming
//! bitmaps.
//!
//! The internal UTF-16 encodings (`internal_little2_encoding`, ...) are only
//! used by `XML_UNICODE` builds, which are out of scope (D-002).

use super::Encoding;
use super::convert::{self, Converted, Order};
use crate::chartab::{ByteType, Table};
use crate::naming;

/// One of `xmltok.c`'s UTF-16 `struct normal_encoding` instances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Utf16 {
    /// `little2_encoding` (UTF-16LE)
    Little,
    /// `little2_encoding_ns`
    LittleNs,
    /// `big2_encoding` (UTF-16BE)
    Big,
    /// `big2_encoding_ns`
    BigNs,
}

impl Utf16 {
    /// All variants.
    pub const ALL: [Utf16; 4] = [Utf16::Little, Utf16::LittleNs, Utf16::Big, Utf16::BigNs];

    /// The table that classifies code units below 0x100.
    pub fn table(self) -> Table {
        match self {
            Utf16::Little | Utf16::Big => Table::Little2,
            Utf16::LittleNs | Utf16::BigNs => Table::Little2Ns,
        }
    }

    fn order(self) -> Order {
        match self {
            Utf16::Little | Utf16::LittleNs => Order::Little,
            Utf16::Big | Utf16::BigNs => Order::Big,
        }
    }

    /// The code unit at `p` as `(low byte, high byte)`.
    fn unit(self, buf: &[u8], p: usize) -> Option<(u8, u8)> {
        self.order().unit(buf, p)
    }

    /// The code unit at `p` as a `u16`.
    fn code_unit(self, buf: &[u8], p: usize) -> Option<u16> {
        self.unit(buf, p)
            .map(|(lo, hi)| u16::from_be_bytes([hi, lo]))
    }
}

/// `unicode_byte_type(hi, lo)`: the type of a code unit of 0x100 or above.
fn unicode_byte_type(hi: u8, lo: u8) -> ByteType {
    match (hi, lo) {
        // first 16-bit code unit or high surrogate (W1)
        (0xD8..=0xDB, _) => ByteType::Lead4,
        // second 16-bit code unit or low surrogate (W2)
        (0xDC..=0xDF, _) => ByteType::Trail,
        // noncharacters U+FFFE and U+FFFF
        (0xFF, 0xFE | 0xFF) => ByteType::NonXml,
        _ => ByteType::NonAscii,
    }
}

impl Encoding for Utf16 {
    const MINBPC: usize = 2;

    fn byte_type(&self, buf: &[u8], p: usize) -> ByteType {
        // A unit that is not complete in `buf` only occurs where C would
        // read out of bounds; it classifies as "not XML", which stops every
        // loop.
        match self.unit(buf, p) {
            Some((lo, 0)) => self.table().byte_type(lo),
            Some((lo, hi)) => unicode_byte_type(hi, lo),
            None => ByteType::NonXml,
        }
    }

    fn byte_to_ascii(&self, buf: &[u8], p: usize) -> i32 {
        match self.unit(buf, p) {
            Some((lo, 0)) => i32::from(lo),
            _ => -1,
        }
    }

    fn char_matches(&self, buf: &[u8], p: usize, c: u8) -> bool {
        self.unit(buf, p) == Some((c, 0))
    }

    fn is_name_char_minbpc(&self, buf: &[u8], p: usize) -> bool {
        self.code_unit(buf, p).is_some_and(naming::ucs2_is_name_char)
    }

    fn is_nmstrt_char_minbpc(&self, buf: &[u8], p: usize) -> bool {
        self.code_unit(buf, p)
            .is_some_and(naming::ucs2_is_name_start)
    }

    /// `IS_NAME_CHAR` is `0` for UTF-16: characters outside the BMP never
    /// occur in names.
    fn is_name_char(&self, _buf: &[u8], _p: usize, _n: usize) -> bool {
        false
    }

    fn is_nmstrt_char(&self, _buf: &[u8], _p: usize, _n: usize) -> bool {
        false
    }

    /// `LITTLE2_IS_INVALID_CHAR`/`BIG2_IS_INVALID_CHAR`: a surrogate pair is
    /// invalid unless its second unit is a low surrogate.
    fn is_invalid_char(&self, buf: &[u8], p: usize, n: usize) -> bool {
        n == 4
            && self
                .unit(buf, p.saturating_add(2))
                .is_none_or(|(_, hi)| hi & 0xFC != 0xDC)
    }

    fn is_utf8(&self) -> bool {
        false
    }

    fn to_utf8(&self, from: &[u8], to: &mut [u8]) -> Converted {
        convert::utf16_to_utf8(self.order(), from, to)
    }

    fn to_utf16(&self, from: &[u8], to: &mut [u16]) -> Converted {
        convert::utf16_to_utf16(self.order(), from, to)
    }
}
