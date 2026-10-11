//! XML name-character classification and UTF-8 validity checks.
//!
//! Port of `nametab.h` and of the `UCS2_GET_NAMING`, `UTF8_GET_NAMING2/3` and
//! `UTF8_INVALID2/3/4` macros of `xmltok.c` (lines 86-195 at `R_2_9_0`).
//!
//! libexpat classifies names with the XML 1.0 *Fourth Edition* (Appendix B)
//! character classes, restricted to the BMP: 4-byte UTF-8 sequences are
//! never name characters (`utf8_isName4` is `isNever`). The port keeps that
//! behaviour; it is observable (which documents are well-formed).
//!
//! The multi-byte predicates take fixed-size arrays, so a caller can only
//! ask about exactly as many bytes as the C macro reads.
//!
//! ```
//! # #[cfg(feature = "internal-api")] {
//! use expat::internal::naming;
//! assert!(naming::ucs2_is_name_start(u16::from(b'A')));
//! assert!(!naming::ucs2_is_name_start(u16::from(b'1')));
//! assert!(naming::ucs2_is_name_char(u16::from(b'1')));
//! // "é" (U+00E9) as UTF-8:
//! assert!(naming::utf8_is_name_start2([0xC3, 0xA9]));
//! // Overlong encoding of "/" is invalid:
//! assert!(naming::utf8_is_invalid2([0xC0, 0xAF]));
//! # }
//! ```

mod data;

use data::{NAME_PAGES, NAMING_BITMAP, NMSTRT_PAGES};

/// Looks up bit `lo` of the 256-bit page that `pages[hi]` selects.
///
/// C: `namingBitmap[(pages[hi] << 3) + (lo >> 5)] & (1u << (lo & 0x1F))`.
fn naming_bit(pages: &[u8; 256], hi: u8, lo: u8) -> bool {
    let page = pages.get(usize::from(hi)).copied().unwrap_or(0);
    // page <= 255 and lo >> 5 <= 7, so the index is < 2048 and cannot
    // overflow; a page beyond the bitmap would read as "not a name char".
    let index = (usize::from(page) << 3) | usize::from(lo >> 5);
    NAMING_BITMAP
        .get(index)
        .is_some_and(|word| word & (1u32 << (lo & 0x1F)) != 0)
}

/// May the BMP code unit `unit` start an XML name? (`nmstrtPages`)
pub fn ucs2_is_name_start(unit: u16) -> bool {
    let [hi, lo] = unit.to_be_bytes();
    naming_bit(&NMSTRT_PAGES, hi, lo)
}

/// May the BMP code unit `unit` appear inside an XML name? (`namePages`)
pub fn ucs2_is_name_char(unit: u16) -> bool {
    let [hi, lo] = unit.to_be_bytes();
    naming_bit(&NAME_PAGES, hi, lo)
}

/// `UTF8_GET_NAMING2`: the 11 payload bits of a 2-byte sequence, split as
/// page (bits 10..8), word (bits 7..5) and bit (4..0). Does not validate.
fn utf8_naming2(pages: &[u8; 256], b: [u8; 2]) -> bool {
    let [b0, b1] = b;
    let hi = (b0 >> 2) & 7;
    let lo = ((b0 & 3) << 6) | (b1 & 0x3F);
    naming_bit(pages, hi, lo)
}

/// `UTF8_GET_NAMING3`: the 16 payload bits of a 3-byte sequence.
/// Does not validate.
fn utf8_naming3(pages: &[u8; 256], b: [u8; 3]) -> bool {
    let [b0, b1, b2] = b;
    let hi = ((b0 & 0x0F) << 4) | ((b1 >> 2) & 0x0F);
    let lo = ((b1 & 3) << 6) | (b2 & 0x3F);
    naming_bit(pages, hi, lo)
}

/// `utf8_isName2`.
pub fn utf8_is_name_char2(b: [u8; 2]) -> bool {
    utf8_naming2(&NAME_PAGES, b)
}

/// `utf8_isName3`.
pub fn utf8_is_name_char3(b: [u8; 3]) -> bool {
    utf8_naming3(&NAME_PAGES, b)
}

/// `utf8_isNmstrt2`.
pub fn utf8_is_name_start2(b: [u8; 2]) -> bool {
    utf8_naming2(&NMSTRT_PAGES, b)
}

/// `utf8_isNmstrt3`.
pub fn utf8_is_name_start3(b: [u8; 3]) -> bool {
    utf8_naming3(&NMSTRT_PAGES, b)
}

fn is_continuation(b: u8) -> bool {
    b & 0xC0 == 0x80
}

/// `UTF8_INVALID2`: lead must be >= 0xC2 (no overlongs), trail must be a
/// continuation byte. The caller has already classified `b[0]` as a 2-byte
/// lead.
pub fn utf8_is_invalid2(b: [u8; 2]) -> bool {
    let [b0, b1] = b;
    b0 < 0xC2 || !is_continuation(b1)
}

/// `UTF8_INVALID3` (Unicode 3.2 Table 3.1B plus U+FFFE/U+FFFF rejected):
/// E0 needs A0..BF (no overlongs), ED needs 80..9F (no surrogates),
/// EF BF needs 80..BD.
pub fn utf8_is_invalid3(b: [u8; 3]) -> bool {
    let [b0, b1, b2] = b;
    let third_bad = if b0 == 0xEF && b1 == 0xBF {
        // The C macro tests `(p)[2] > 0xBD` here, after `(p)[2] & 0x80 == 0`
        // has already rejected ASCII; together that is "not 80..BD".
        !(0x80..=0xBD).contains(&b2)
    } else {
        !is_continuation(b2)
    };
    let second_bad = match b0 {
        0xE0 => !(0xA0..=0xBF).contains(&b1),
        0xED => !(0x80..=0x9F).contains(&b1),
        _ => !is_continuation(b1),
    };
    third_bad || second_bad
}

/// `UTF8_INVALID4`: F0 needs 90..BF (no overlongs), F4 needs 80..8F (nothing
/// above U+10FFFF), others need a continuation byte; bytes 3 and 4 must be
/// continuation bytes.
pub fn utf8_is_invalid4(b: [u8; 4]) -> bool {
    let [b0, b1, b2, b3] = b;
    let second_bad = match b0 {
        0xF0 => !(0x90..=0xBF).contains(&b1),
        0xF4 => !(0x80..=0x8F).contains(&b1),
        _ => !is_continuation(b1),
    };
    !is_continuation(b3) || !is_continuation(b2) || second_bad
}

#[cfg(test)]
// reason: test code; an out-of-bounds index or overflow there is a test failure.
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod tests;
