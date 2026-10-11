//! Conversion of encoded input to UTF-8 and UTF-16: the `utf8Convert` and
//! `utf16Convert` members of the C `ENCODING` (`XmlUtf8Convert`,
//! `XmlUtf16Convert`), for the built-in encodings of `xmltok.c`.
//!
//! A converter reads from the start of `from` and writes from the start of
//! `to`; [`Converted`] says how far it got in each, which is the distance the
//! C converter advances `*fromP` and `*toP`. The unknown-encoding converters
//! live with [`super::Unknown`].
//!
//! Like the C functions, the converters do not validate: the parser only
//! hands them input that the tokenizer has accepted, and on other input they
//! produce the same bytes as C (including C's arithmetic on malformed
//! sequences).

// Positions are bounded by the lengths of `from` and `to` (each at most
// `isize::MAX`), and every subtraction is of a smaller position from a larger
// one, checked just before.
// reason: all arithmetic in this module is bounded as stated above.
#![allow(clippy::arithmetic_side_effects)]

use crate::chartab::{ByteType, Table};

/// `enum XML_Convert_Result`. `ConvertResult as i32` is the C value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ConvertResult {
    /// `XML_CONVERT_COMPLETED`: all input was converted.
    Completed = 0,
    /// `XML_CONVERT_INPUT_INCOMPLETE`: the input ends inside a character.
    InputIncomplete = 1,
    /// `XML_CONVERT_OUTPUT_EXHAUSTED`: the output is full, input may remain.
    OutputExhausted = 2,
}

/// Outcome of one converter call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Converted {
    /// The C return value.
    pub result: ConvertResult,
    /// Bytes of input consumed (how far C advances `*fromP`).
    pub read: usize,
    /// Units of output written (how far C advances `*toP`).
    pub written: usize,
}

const fn converted(result: ConvertResult, read: usize, written: usize) -> Converted {
    Converted {
        result,
        read,
        written,
    }
}

/// An output buffer with a write position (the C `*toP`/`toLim` pair).
pub(super) struct Out<'a, T> {
    buf: &'a mut [T],
    pub(super) n: usize,
}

impl<'a, T: Copy> Out<'a, T> {
    pub(super) fn new(buf: &'a mut [T]) -> Self {
        Out { buf, n: 0 }
    }

    /// `toLim - *toP`.
    pub(super) fn room(&self) -> usize {
        self.buf.len() - self.n
    }

    /// `*(*toP)++ = v`. Callers check [`Out::room`] first, as C does; a
    /// write into a full buffer is dropped (C would write out of bounds).
    pub(super) fn push(&mut self, v: T) {
        if let Some(slot) = self.buf.get_mut(self.n) {
            *slot = v;
            self.n += 1;
        }
    }
}

/// `_INTERNAL_trim_to_complete_utf8_characters`: the length of the longest
/// prefix of `from` that does not end inside a UTF-8 sequence, judged by the
/// last lead byte (continuation and invalid bytes are walked over without
/// being checked).
///
/// ```
/// # #[cfg(feature = "internal-api")] {
/// use expat::internal::tok::trim_to_complete_utf8_characters as trim;
/// assert_eq!(trim(b"ab\xc3\xa9"), 4);
/// assert_eq!(trim(b"ab\xe2\x82"), 2);
/// # }
/// ```
pub fn trim_to_complete_utf8_characters(from: &[u8]) -> usize {
    let mut lim = from.len();
    let mut walked: usize = 0;
    while lim > 0 {
        let Some(&prev) = from.get(lim - 1) else {
            break;
        };
        let lead = if prev & 0xF8 == 0xF0 {
            Some(4)
        } else if prev & 0xF0 == 0xE0 {
            Some(3)
        } else if prev & 0xE0 == 0xC0 {
            Some(2)
        } else if prev & 0x80 == 0 {
            // A 1-byte character ends the walk.
            break;
        } else {
            None
        };
        if let Some(n) = lead {
            if walked + 1 >= n {
                lim += n - 1;
                break;
            }
            walked = 0;
        }
        lim -= 1;
        walked += 1;
    }
    lim
}

/// `utf8_toUtf8`.
pub(super) fn utf8_to_utf8(from: &[u8], to: &mut [u8]) -> Converted {
    let mut lim = from.len();
    let output_exhausted = lim > to.len();
    if output_exhausted {
        lim = to.len();
    }
    let src = from.get(..lim).unwrap_or_default();
    let trimmed = trim_to_complete_utf8_characters(src);
    let input_incomplete = trimmed < lim;
    let (src, dst) = (
        src.get(..trimmed).unwrap_or_default(),
        to.get_mut(..trimmed).unwrap_or_default(),
    );
    dst.copy_from_slice(src);
    let result = if output_exhausted {
        ConvertResult::OutputExhausted
    } else if input_incomplete {
        ConvertResult::InputIncomplete
    } else {
        ConvertResult::Completed
    };
    converted(result, trimmed, trimmed)
}

/// `(unsigned short)` of a C `char` (signed on the platforms in CI that
/// define it so; the converters below only reach this for continuation or
/// malformed bytes, where C's sign extension is what the port reproduces).
fn sign_extend(b: u8) -> u16 {
    u16::from_ne_bytes(i16::from(i8::from_ne_bytes([b])).to_ne_bytes())
}

/// `utf8_toUtf16` over the byte types of `table`.
pub(super) fn utf8_to_utf16(table: Table, from: &[u8], to: &mut [u16]) -> Converted {
    let mut out = Out::new(to);
    let mut f = 0;
    let lim = from.len();
    let byte = |i: usize| from.get(i).copied().map_or(0, u16::from);
    while f < lim && out.room() > 0 {
        let b0 = from.get(f).copied().unwrap_or_default();
        match table.byte_type(b0) {
            ByteType::Lead2 => {
                if lim - f < 2 {
                    return converted(ConvertResult::InputIncomplete, f, out.n);
                }
                out.push(((byte(f) & 0x1F) << 6) | (byte(f + 1) & 0x3F));
                f += 2;
            }
            ByteType::Lead3 => {
                if lim - f < 3 {
                    return converted(ConvertResult::InputIncomplete, f, out.n);
                }
                out.push(
                    ((byte(f) & 0xF) << 12) | ((byte(f + 1) & 0x3F) << 6) | (byte(f + 2) & 0x3F),
                );
                f += 3;
            }
            ByteType::Lead4 => {
                if out.room() < 2 {
                    return converted(ConvertResult::OutputExhausted, f, out.n);
                }
                if lim - f < 4 {
                    return converted(ConvertResult::InputIncomplete, f, out.n);
                }
                let b = |i: usize| u32::from(byte(i));
                let n = ((b(f) & 0x7) << 18)
                    | ((b(f + 1) & 0x3F) << 12)
                    | ((b(f + 2) & 0x3F) << 6)
                    | (b(f + 3) & 0x3F);
                // C subtracts in `unsigned long` and keeps the low 16 bits of
                // the shifted value; bits 10..=25 are the same in any width
                // of at least 26 bits, so `u32` wrapping gives C's result
                // also for the overlong forms below 0x10000.
                let n = n.wrapping_sub(0x1_0000);
                let [_, _, hi1, hi0] = (n >> 10).to_be_bytes();
                out.push(u16::from_be_bytes([hi1, hi0]) | 0xD800);
                let [_, _, lo1, lo0] = (n & 0x3FF).to_be_bytes();
                out.push(u16::from_be_bytes([lo1, lo0]) | 0xDC00);
                f += 4;
            }
            _ => {
                out.push(sign_extend(b0));
                f += 1;
            }
        }
    }
    let result = if f < lim {
        ConvertResult::OutputExhausted
    } else {
        ConvertResult::Completed
    };
    converted(result, f, out.n)
}

/// `latin1_toUtf8`.
pub(super) fn latin1_to_utf8(from: &[u8], to: &mut [u8]) -> Converted {
    let mut out = Out::new(to);
    for (f, &c) in from.iter().enumerate() {
        if c & 0x80 != 0 {
            if out.room() < 2 {
                return converted(ConvertResult::OutputExhausted, f, out.n);
            }
            out.push((c >> 6) | 0xC0);
            out.push((c & 0x3F) | 0x80);
        } else {
            if out.room() == 0 {
                return converted(ConvertResult::OutputExhausted, f, out.n);
            }
            out.push(c);
        }
    }
    converted(ConvertResult::Completed, from.len(), out.n)
}

/// The shared shape of `latin1_toUtf16` and `ascii_toUtf8`: one output unit
/// per input byte.
fn widen<T: Copy>(from: &[u8], to: &mut [T], f: impl Fn(u8) -> T) -> Converted {
    let n = from.len().min(to.len());
    for (dst, &src) in to.iter_mut().zip(from) {
        *dst = f(src);
    }
    let result = if n == to.len() && n < from.len() {
        ConvertResult::OutputExhausted
    } else {
        ConvertResult::Completed
    };
    converted(result, n, n)
}

/// `latin1_toUtf16` (also the UTF-16 converter of US-ASCII).
pub(super) fn latin1_to_utf16(from: &[u8], to: &mut [u16]) -> Converted {
    widen(from, to, u16::from)
}

/// `ascii_toUtf8`: copies bytes unchanged.
pub(super) fn ascii_to_utf8(from: &[u8], to: &mut [u8]) -> Converted {
    widen(from, to, |b| b)
}

/// Byte order of a UTF-16 encoding: where the low and high byte of a code
/// unit are (`GET_LO`/`GET_HI`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Order {
    /// `little2_`: low byte first.
    Little,
    /// `big2_`: high byte first.
    Big,
}

impl Order {
    /// `(GET_LO(p), GET_HI(p))`, or `None` if the unit is not in `buf`.
    pub(super) fn unit(self, buf: &[u8], p: usize) -> Option<(u8, u8)> {
        let [a, b] = *buf.get(p..)?.first_chunk::<2>()?;
        Some(match self {
            Order::Little => (a, b),
            Order::Big => (b, a),
        })
    }
}

/// `DEFINE_UTF16_TO_UTF8`.
pub(super) fn utf16_to_utf8(order: Order, from: &[u8], to: &mut [u8]) -> Converted {
    let mut out = Out::new(to);
    // shrink to even
    let lim = from.len() & !1;
    let mut f = 0;
    while f < lim {
        let Some((lo, hi)) = order.unit(from, f) else {
            break;
        };
        match hi {
            0 if lo < 0x80 => {
                if out.room() == 0 {
                    return converted(ConvertResult::OutputExhausted, f, out.n);
                }
                out.push(lo);
            }
            0..=7 => {
                if out.room() < 2 {
                    return converted(ConvertResult::OutputExhausted, f, out.n);
                }
                out.push((lo >> 6) | (hi << 2) | 0xC0);
                out.push((lo & 0x3F) | 0x80);
            }
            0xD8..=0xDB => {
                if out.room() < 4 {
                    return converted(ConvertResult::OutputExhausted, f, out.n);
                }
                if lim - f < 4 {
                    return converted(ConvertResult::InputIncomplete, f, out.n);
                }
                // The second unit is not checked to be a low surrogate; the
                // tokenizer has done that.
                let plane = (((hi & 0x3) << 2) | ((lo >> 6) & 0x3)) + 1;
                out.push((plane >> 2) | 0xF0);
                out.push(((lo >> 2) & 0xF) | ((plane & 0x3) << 4) | 0x80);
                f += 2;
                let (lo2, hi2) = order.unit(from, f).unwrap_or_default();
                out.push(((lo & 0x3) << 4) | ((hi2 & 0x3) << 2) | (lo2 >> 6) | 0x80);
                out.push((lo2 & 0x3F) | 0x80);
            }
            _ => {
                if out.room() < 3 {
                    return converted(ConvertResult::OutputExhausted, f, out.n);
                }
                // 16 bits divided 4, 6, 6 amongst 3 bytes
                out.push((hi >> 4) | 0xE0);
                out.push(((hi & 0xF) << 2) | (lo >> 6) | 0x80);
                out.push((lo & 0x3F) | 0x80);
            }
        }
        f += 2;
    }
    // C ends with `from < fromLim ? INPUT_INCOMPLETE : COMPLETED`, and the
    // loop only exits with `from == fromLim`; an odd final byte is left
    // unread.
    converted(ConvertResult::Completed, f, out.n)
}

/// `DEFINE_UTF16_TO_UTF16`.
pub(super) fn utf16_to_utf16(order: Order, from: &[u8], to: &mut [u16]) -> Converted {
    let mut result = ConvertResult::Completed;
    // shrink to even
    let mut lim = from.len() & !1;
    // Avoid copying the first half (2 bytes) of a surrogate pair.
    if lim / 2 > to.len()
        && order
            .unit(from, lim - 2)
            .is_some_and(|(_, hi)| hi & 0xFC == 0xD8)
    {
        lim -= 2;
        result = ConvertResult::InputIncomplete;
    }
    let mut out = Out::new(to);
    let mut f = 0;
    while f < lim && out.room() > 0 {
        let (lo, hi) = order.unit(from, f).unwrap_or_default();
        out.push(u16::from_be_bytes([hi, lo]));
        f += 2;
    }
    if out.room() == 0 && f < lim {
        result = ConvertResult::OutputExhausted;
    }
    converted(result, f, out.n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_to_utf16_keeps_a_split_surrogate_pair_for_later() {
        // U+10000 as a surrogate pair, with room for only one unit.
        let mut to = [0u16; 1];
        let r = utf16_to_utf16(Order::Big, b"\xd8\x00\xdc\x00", &mut to);
        assert_eq!(r, converted(ConvertResult::OutputExhausted, 2, 1));
        let r = utf16_to_utf16(Order::Little, b"\x41\x00\x00\xd8", &mut to);
        assert_eq!(r, converted(ConvertResult::InputIncomplete, 2, 1));
    }

    #[test]
    fn utf8_to_utf16_sign_extends_stray_bytes() {
        let mut to = [0u16; 2];
        let r = utf8_to_utf16(Table::Utf8, b"\x80a", &mut to);
        assert_eq!(r, converted(ConvertResult::Completed, 2, 2));
        assert_eq!(to, [0xFF80, 0x61]);
    }
}
