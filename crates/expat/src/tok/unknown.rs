//! Encodings described by the application (`XmlInitUnknownEncoding`): a
//! byte map built from `XML_Encoding::map` plus an optional converter for
//! multi-byte sequences.
//!
//! Each of the 256 map entries is one of
//!
//! - `-1`: the byte is malformed;
//! - `-2`, `-3`, `-4`: the byte starts a sequence of that many bytes, whose
//!   code point the [`Converter`] computes;
//! - a code point: the byte is that one character.
//!
//! The scanner treats such an encoding like a single-byte one (`MINBPC` 1)
//! whose byte types come from the map.
//!
//! ```
//! # #[cfg(feature = "internal-api")] {
//! use expat::internal::tok::{content_tok, Scan, Tok, Unknown};
//! // Latin-1 with 0xA4 replaced by the euro sign.
//! let mut map = [0i32; 256];
//! for (m, i) in map.iter_mut().zip(0..) {
//!     *m = i;
//! }
//! map[0xA4] = 0x20AC;
//! let enc = Unknown::<fn(&[u8]) -> i32>::new(&map, None, false);
//! assert!(enc.is_ok());
//! let Ok(enc) = enc else { return };
//! let mut out = [0u8; 8];
//! let r = expat::internal::tok::Encoding::to_utf8(&enc, b"\xa4", &mut out);
//! assert_eq!(&out[..r.written], "\u{20ac}".as_bytes());
//! assert_eq!(content_tok(&enc, b"<a/>", 0).scan, Scan::Token(Tok::EmptyElementNoAtts));
//! # }
//! ```

use super::Encoding;
use super::convert::{ConvertResult, Converted, Out};
use super::scan::check_char_ref_number;
use crate::chartab::{ByteType, Table};
use crate::encode::utf8_encode;
use crate::naming;

/// The application's converter for multi-byte sequences (C: `CONVERTER`
/// with its `userData`). It receives exactly the bytes of one sequence, as
/// many as the map entry of the first byte says, and returns the code point
/// or a negative value if the sequence is malformed.
pub trait Converter {
    /// Code point of the sequence `seq`.
    fn convert(&self, seq: &[u8]) -> i32;
}

impl<F: Fn(&[u8]) -> i32> Converter for F {
    fn convert(&self, seq: &[u8]) -> i32 {
        self(seq)
    }
}

/// `struct unknown_encoding`.
#[derive(Debug, Clone)]
pub struct Unknown<C> {
    types: [ByteType; 256],
    utf16: [u16; 256],
    /// Length (0 for a lead byte) and UTF-8 bytes of each single byte.
    utf8: [[u8; 4]; 256],
    convert: Option<C>,
}

/// Why [`Unknown::new`] rejected a map (C: `XmlInitUnknownEncoding` returns
/// `NULL`, the parser reports `XML_ERROR_UNKNOWN_ENCODING`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvalidMap;

impl<C: Converter> Unknown<C> {
    /// `XmlInitUnknownEncoding` (`ns == false`) or
    /// `XmlInitUnknownEncodingNS` (`ns == true`).
    ///
    /// Fails when an ASCII character that XML syntax uses is not mapped to
    /// itself, when an entry is below `-4`, when a multi-byte entry has no
    /// converter, or when a code point is above U+FFFF.
    pub fn new(map: &[i32; 256], convert: Option<C>, ns: bool) -> Result<Self, InvalidMap> {
        let latin1 = |c: u8| Table::Latin1.byte_type(c);
        let syntax = |t: ByteType| t != ByteType::Other && t != ByteType::NonXml;
        for (i, &m) in (0u8..0x80).zip(map.iter()) {
            if syntax(latin1(i)) && m != i32::from(i) {
                return Err(InvalidMap);
            }
        }
        let mut e = Unknown {
            types: [ByteType::NonXml; 256],
            utf16: [0; 256],
            utf8: [[0; 4]; 256],
            convert: None,
        };
        let entries = e.types.iter_mut().zip(e.utf16.iter_mut()).zip(e.utf8.iter_mut());
        for (i, (((ty, u16_), u8_), &c)) in (0u8..=0xFF).zip(entries.zip(map.iter())) {
            if c == -1 {
                *ty = ByteType::Malform;
                // This shouldn't really get used.
                *u16_ = 0xFFFF;
                *u8_ = [1, 0, 0, 0];
            } else if c < 0 {
                // Multi-byte sequences need a converter function.
                let lead = match c {
                    -2 => ByteType::Lead2,
                    -3 => ByteType::Lead3,
                    -4 => ByteType::Lead4,
                    _ => return Err(InvalidMap),
                };
                if convert.is_none() {
                    return Err(InvalidMap);
                }
                *ty = lead;
                *u8_ = [0; 4];
                *u16_ = 0;
            } else if let Ok(a) = u8::try_from(c)
                && a < 0x80
            {
                if syntax(latin1(a)) && a != i {
                    return Err(InvalidMap);
                }
                *ty = latin1(a);
                *u8_ = [1, a, 0, 0];
                *u16_ = if a == 0 { 0xFFFF } else { u16::from(a) };
            } else if check_char_ref_number(c) < 0 {
                *ty = ByteType::NonXml;
                // This shouldn't really get used.
                *u16_ = 0xFFFF;
                *u8_ = [1, 0, 0, 0];
            } else {
                let Ok(unit) = u16::try_from(c) else {
                    return Err(InvalidMap);
                };
                *ty = if naming::ucs2_is_name_start(unit) {
                    ByteType::Nmstrt
                } else if naming::ucs2_is_name_char(unit) {
                    ByteType::Name
                } else {
                    ByteType::Other
                };
                let mut b = [0u8; 4];
                let n = utf8_encode(u32::from(unit), &mut b);
                let [b0, b1, b2, _] = b;
                // `unit >= 0x80`, so `n` is 2 or 3.
                *u8_ = [u8::try_from(n).unwrap_or_default(), b0, b1, b2];
                *u16_ = unit;
            }
        }
        if ns && let Some(t) = e.types.get_mut(usize::from(b':')) {
            *t = ByteType::Colon;
        }
        e.convert = convert;
        Ok(e)
    }

    /// The converter's code point for the sequence at `p`, whose length the
    /// byte type of `buf[p]` gives; `None` if the sequence is not complete in
    /// `buf` or the byte is not a lead byte.
    fn convert_at(&self, buf: &[u8], p: usize) -> Option<i32> {
        let n = self.lead_len(*buf.get(p)?)?;
        let seq = buf.get(p..p.checked_add(n)?)?;
        // Without a converter the map has no lead bytes, so this is never
        // reached; C would call through a null pointer.
        Some(self.convert.as_ref().map_or(0, |c| c.convert(seq)))
    }

    fn lead_len(&self, b: u8) -> Option<usize> {
        match self.types.get(usize::from(b))? {
            ByteType::Lead2 => Some(2),
            ByteType::Lead3 => Some(3),
            ByteType::Lead4 => Some(4),
            _ => None,
        }
    }
}

impl<C: Converter> Encoding for Unknown<C> {
    const MINBPC: usize = 1;

    fn byte_type(&self, buf: &[u8], p: usize) -> ByteType {
        buf.get(p)
            .and_then(|&b| self.types.get(usize::from(b)).copied())
            .unwrap_or(ByteType::NonXml)
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

    /// `unknown_isName`.
    fn is_name_char(&self, buf: &[u8], p: usize, _n: usize) -> bool {
        self.convert_at(buf, p)
            .and_then(|c| u16::try_from(c).ok())
            .is_some_and(naming::ucs2_is_name_char)
    }

    /// `unknown_isNmstrt`.
    fn is_nmstrt_char(&self, buf: &[u8], p: usize, _n: usize) -> bool {
        self.convert_at(buf, p)
            .and_then(|c| u16::try_from(c).ok())
            .is_some_and(naming::ucs2_is_name_start)
    }

    /// `unknown_isInvalid`.
    fn is_invalid_char(&self, buf: &[u8], p: usize, _n: usize) -> bool {
        self.convert_at(buf, p)
            .is_none_or(|c| u16::try_from(c).is_err() || check_char_ref_number(c) < 0)
    }

    fn is_utf8(&self) -> bool {
        false
    }

    /// `unknown_toUtf8` on all of `from`.
    fn to_utf8(&self, from: &[u8], to: &mut [u8]) -> Converted {
        self.to_utf8_until(from, 0, from.len(), to)
    }

    /// `unknown_toUtf8` from `buf[from]` with C's `fromLim` at `lim`. As in
    /// C, the loop stops only when the position equals `lim` exactly, and a
    /// lead byte's whole sequence goes to the converter even when it
    /// extends past `lim` (C `*fromP += type - (BT_LEAD2 - 2)`, after which
    /// the position never equals `lim` and conversion goes on until the
    /// output is full). `toAscii` in `XmlParseXmlDecl` relies on this: it
    /// starts at every byte of the declaration, including trailing bytes
    /// that the map makes lead bytes. Where C would read past the end of
    /// `buf` (a sequence not complete in `buf`), the port stops with
    /// [`ConvertResult::InputIncomplete`] instead (docs/DIVERGENCES.md, Q-5).
    fn to_utf8_until(&self, buf: &[u8], from: usize, lim: usize, to: &mut [u8]) -> Converted {
        let mut out = Out::new(to);
        // Positions relative to `from`; a `lim` before `from` is never
        // reached, as in C.
        let buf = buf.get(from..).unwrap_or_default();
        let lim = lim.checked_sub(from);
        let mut f = 0;
        loop {
            if Some(f) == lim {
                return done(ConvertResult::Completed, f, out.n);
            }
            let Some(&b) = buf.get(f) else {
                return done(ConvertResult::InputIncomplete, f, out.n);
            };
            let [n, b1, b2, b3] = self.utf8.get(usize::from(b)).copied().unwrap_or_default();
            let mut seq = [b1, b2, b3, 0];
            let (n, step) = if n == 0 {
                let Some(c) = self.convert_at(buf, f) else {
                    return done(ConvertResult::InputIncomplete, f, out.n);
                };
                seq = [0; 4];
                let n = u32::try_from(c).map_or(0, |c| utf8_encode(c, &mut seq));
                (n, self.lead_len(b).unwrap_or(1))
            } else {
                (usize::from(n), 1)
            };
            if n > out.room() {
                return done(ConvertResult::OutputExhausted, f, out.n);
            }
            for &x in seq.iter().take(n) {
                out.push(x);
            }
            f = f.saturating_add(step);
        }
    }

    /// `unknown_toUtf16`. An incomplete final sequence is handled as in
    /// [`Unknown::to_utf8`].
    fn to_utf16(&self, from: &[u8], to: &mut [u16]) -> Converted {
        let mut out = Out::new(to);
        let mut f = 0;
        while let Some(&b) = from.get(f) {
            if out.room() == 0 {
                return done(ConvertResult::OutputExhausted, f, out.n);
            }
            let mut c = self.utf16.get(usize::from(b)).copied().unwrap_or_default();
            if c == 0 {
                let Some(v) = self.convert_at(from, f) else {
                    return done(ConvertResult::InputIncomplete, f, out.n);
                };
                // `(unsigned short)` of the converter's `int`.
                let [_, _, hi, lo] = v.to_be_bytes();
                c = u16::from_be_bytes([hi, lo]);
                f = f.saturating_add(self.lead_len(b).unwrap_or(1));
            } else {
                f = f.saturating_add(1);
            }
            out.push(c);
        }
        done(ConvertResult::Completed, f, out.n)
    }
}

const fn done(result: ConvertResult, read: usize, written: usize) -> Converted {
    Converted {
        result,
        read,
        written,
    }
}
