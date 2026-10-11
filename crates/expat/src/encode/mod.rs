//! Code point encoders used for character references and conversions.
//!
//! Port of `XmlUtf8Encode` and `XmlUtf16Encode` (`xmltok.c`, `R_2_9_0`
//! lines 1181-1231).
//!
//! Like the C functions these do not reject surrogate code points: callers
//! (`checkCharRefNumber` in the tokenizer) filter them first. Code points
//! above U+10FFFF produce no output (`0`), exactly as in C. The C functions
//! also return `0` for negative input; `u32` makes that unrepresentable.
//!
//! ```
//! # #[cfg(feature = "internal-api")] {
//! use expat::internal::encode::{utf8_encode, utf16_encode};
//! let mut b = [0u8; 4];
//! assert_eq!(utf8_encode(0x20AC, &mut b), 3);
//! assert_eq!(&b[..3], "€".as_bytes());
//! let mut u = [0u16; 2];
//! assert_eq!(utf16_encode(0x1F600, &mut u), 2);
//! assert_eq!(u, [0xD83D, 0xDE00]);
//! assert_eq!(utf8_encode(0x110000, &mut b), 0);
//! # }
//! ```

/// Largest code point either encoder accepts.
pub const MAX_CODE_POINT: u32 = 0x10_FFFF;

/// Writes the UTF-8 form of `c` into `buf` and returns its length (1-4), or
/// `0` if `c > U+10FFFF`. Surrogates are encoded as 3-byte sequences.
pub fn utf8_encode(c: u32, buf: &mut [u8; 4]) -> usize {
    // Each `as u8` below keeps only bits that the shift/mask already limited
    // to the payload of that byte, so no information is lost.
    let cont = |shift: u32| 0x80 | ((c >> shift) & 0x3F) as u8;
    match c {
        0..=0x7F => {
            buf[0] = c as u8;
            1
        }
        0x80..=0x7FF => {
            buf[0] = 0xC0 | (c >> 6) as u8;
            buf[1] = cont(0);
            2
        }
        0x800..=0xFFFF => {
            buf[0] = 0xE0 | (c >> 12) as u8;
            buf[1] = cont(6);
            buf[2] = cont(0);
            3
        }
        0x1_0000..=MAX_CODE_POINT => {
            buf[0] = 0xF0 | (c >> 18) as u8;
            buf[1] = cont(12);
            buf[2] = cont(6);
            buf[3] = cont(0);
            4
        }
        _ => 0,
    }
}

/// Writes the UTF-16 form of `c` into `buf` and returns its length (1-2), or
/// `0` if `c > U+10FFFF`. Surrogate code points are copied as single units.
pub fn utf16_encode(c: u32, buf: &mut [u16; 2]) -> usize {
    match c {
        0..=0xFFFF => {
            buf[0] = c as u16;
            1
        }
        0x1_0000..=MAX_CODE_POINT => {
            // `c >= 0x1_0000` in this arm, so the subtraction cannot wrap.
            let v = c.wrapping_sub(0x1_0000);
            buf[0] = 0xD800 | (v >> 10) as u16;
            buf[1] = 0xDC00 | (v & 0x3FF) as u16;
            2
        }
        _ => 0,
    }
}

#[cfg(test)]
// reason: test code; an out-of-bounds index or overflow there is a test failure.
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod tests;
