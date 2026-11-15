//! Names by the rules of XML 1.0 Fifth Edition, as an option.
//!
//! libexpat classifies name characters by the Fourth Edition classes
//! (Appendix B), restricted to the BMP; the port does the same by default
//! (see [`crate::naming`]). [`Fifth`] wraps a built-in encoding and
//! changes only the name predicates to productions \[4\] and \[4a\] of the
//! Fifth Edition, which accept every name the Fourth Edition accepts and
//! also characters added to Unicode later, including those outside the
//! BMP. Everything else (byte types, validity checks, conversion) is the
//! wrapped encoding's.
//!
//! Characters below U+0100 are classified alike by both editions, so the
//! byte-type tables are shared; the difference lies in multi-byte UTF-8
//! sequences and in UTF-16 code units of U+0100 and above.
//!
//! ```
//! # #[cfg(feature = "internal-api")] {
//! use expat::internal::tok::{content_tok, Fifth, Normal, Scan, Tok};
//! // U+0221 is a letter in Unicode 4.0, after the Fourth Edition tables.
//! let doc = "<\u{221}/>".as_bytes();
//! assert_eq!(content_tok(&Normal::Utf8, doc, 0).scan, Scan::Invalid);
//! let r = content_tok(&Fifth(Normal::Utf8), doc, 0);
//! assert_eq!(r.scan, Scan::Token(Tok::EmptyElementNoAtts));
//! # }
//! ```

use super::Encoding;
use super::convert::Converted;
use crate::chartab::ByteType;
use crate::naming::{fifth_is_name_char, fifth_is_name_start};

/// A built-in encoding with the name rules of XML 1.0 Fifth Edition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fifth<E>(pub E);

impl<E: Encoding> Encoding for Fifth<E> {
    const MINBPC: usize = E::MINBPC;

    #[inline(always)]
    fn byte_type(&self, buf: &[u8], p: usize) -> ByteType {
        self.0.byte_type(buf, p)
    }

    fn byte_to_ascii(&self, buf: &[u8], p: usize) -> i32 {
        self.0.byte_to_ascii(buf, p)
    }

    fn char_matches(&self, buf: &[u8], p: usize, c: u8) -> bool {
        self.0.char_matches(buf, p, c)
    }

    fn is_name_char_minbpc(&self, buf: &[u8], p: usize) -> bool {
        self.0
            .code_point(buf, p, E::MINBPC)
            .is_some_and(fifth_is_name_char)
    }

    fn is_nmstrt_char_minbpc(&self, buf: &[u8], p: usize) -> bool {
        self.0
            .code_point(buf, p, E::MINBPC)
            .is_some_and(fifth_is_name_start)
    }

    fn is_name_char(&self, buf: &[u8], p: usize, n: usize) -> bool {
        self.0.code_point(buf, p, n).is_some_and(fifth_is_name_char)
    }

    fn is_nmstrt_char(&self, buf: &[u8], p: usize, n: usize) -> bool {
        self.0.code_point(buf, p, n).is_some_and(fifth_is_name_start)
    }

    fn is_invalid_char(&self, buf: &[u8], p: usize, n: usize) -> bool {
        self.0.is_invalid_char(buf, p, n)
    }

    fn code_point(&self, buf: &[u8], p: usize, n: usize) -> Option<u32> {
        self.0.code_point(buf, p, n)
    }

    fn is_utf8(&self) -> bool {
        self.0.is_utf8()
    }

    fn is_internal(&self) -> bool {
        self.0.is_internal()
    }

    fn to_utf8(&self, from: &[u8], to: &mut [u8]) -> Converted {
        self.0.to_utf8(from, to)
    }

    fn to_utf8_until(&self, buf: &[u8], from: usize, lim: usize, to: &mut [u8]) -> Converted {
        self.0.to_utf8_until(buf, from, lim, to)
    }

    fn to_utf16(&self, from: &[u8], to: &mut [u16]) -> Converted {
        self.0.to_utf16(from, to)
    }
}
