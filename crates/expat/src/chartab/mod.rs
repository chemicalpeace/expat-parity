//! Byte classification tables of the tokenizer.
//!
//! Port of the `BT_*` enumeration (`xmltok_impl.h`) and of the 256-entry
//! `type` tables that `xmltok.c` builds for each built-in encoding from
//! `asciitab.h`, `iasciitab.h`, `latin1tab.h` and `utf8tab.h`.
//!
//! The tables are `const` data: no global mutable state, no lazy
//! initialisation. Every lookup is total over `u8`, so no input can make a
//! lookup fail.
//!
//! ```
//! # #[cfg(feature = "internal-api")] {
//! use expat::internal::chartab::{ByteType, Table};
//! assert_eq!(Table::Utf8.byte_type(b'<'), ByteType::Lt);
//! // Without namespace processing the colon is an ordinary name-start char.
//! assert_eq!(Table::Utf8.byte_type(b':'), ByteType::Nmstrt);
//! assert_eq!(Table::Utf8Ns.byte_type(b':'), ByteType::Colon);
//! # }
//! ```

mod data;

/// Lexical class of one byte (or of one UTF-16 code unit below 0x100).
///
/// Mirrors the anonymous `BT_*` enum of `xmltok_impl.h` in the same order, so
/// `ByteType as u8` equals the C value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ByteType {
    /// Not allowed in XML (e.g. a C0 control or a noncharacter).
    NonXml = 0,
    /// Malformed with regard to the encoding.
    Malform,
    /// `<`
    Lt,
    /// `&`
    Amp,
    /// `]`
    Rsqb,
    /// Lead byte of a 2-byte UTF-8 sequence.
    Lead2,
    /// Lead byte of a 3-byte UTF-8 sequence.
    Lead3,
    /// Lead byte of a 4-byte UTF-8 sequence.
    Lead4,
    /// Trailing unit (UTF-8 continuation byte or low surrogate).
    Trail,
    /// Carriage return.
    Cr,
    /// Line feed.
    Lf,
    /// `>`
    Gt,
    /// `"`
    Quot,
    /// `'`
    Apos,
    /// `=`
    Equals,
    /// `?`
    Quest,
    /// `!`
    Excl,
    /// `/`
    Sol,
    /// `;`
    Semi,
    /// `#`
    Num,
    /// `[`
    Lsqb,
    /// White space (tab, space; CR in the internal tables).
    S,
    /// Name-start letter that is not a hex digit, and `_`.
    Nmstrt,
    /// `:` (only in namespace-aware tables).
    Colon,
    /// Hex letter `A-F`, `a-f`.
    Hex,
    /// Digit `0-9`.
    Digit,
    /// `.` and U+00B7 MIDDLE DOT.
    Name,
    /// `-`
    Minus,
    /// Known not to be a name or name-start character.
    Other,
    /// Non-ASCII character that might be a name or name-start character.
    NonAscii,
    /// `%`
    Percnt,
    /// `(`
    Lpar,
    /// `)`
    Rpar,
    /// `*`
    Ast,
    /// `+`
    Plus,
    /// `,`
    Comma,
    /// `|`
    Verbar,
}

/// The built-in encodings' byte tables (`struct normal_encoding::type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Table {
    /// `utf8_encoding` (`xmltok.c`): asciitab + utf8tab, colon is a name-start char.
    Utf8,
    /// `utf8_encoding_ns`: as `Utf8`, colon is [`ByteType::Colon`].
    Utf8Ns,
    /// `internal_utf8_encoding`: iasciitab + utf8tab.
    InternalUtf8,
    /// `internal_utf8_encoding_ns`.
    InternalUtf8Ns,
    /// `latin1_encoding`: asciitab + latin1tab.
    Latin1,
    /// `latin1_encoding_ns`.
    Latin1Ns,
    /// `ascii_encoding`: asciitab, bytes >= 0x80 are [`ByteType::NonXml`].
    Ascii,
    /// `ascii_encoding_ns`.
    AsciiNs,
    /// `little2_encoding` / `big2_encoding`: asciitab + latin1tab, used for
    /// UTF-16 code units below 0x100.
    Little2,
    /// `little2_encoding_ns` / `big2_encoding_ns`.
    Little2Ns,
}

/// Builds one 256-entry table from a lower (ASCII) and an upper half.
///
/// `colon_is_nmstrt` reproduces the `#define BT_COLON BT_NMSTRT` that
/// `xmltok.c` wraps around `asciitab.h` for the non-namespace encodings.
///
/// Only ever evaluated at compile time (it initialises `static`s), so an
/// out-of-bounds index would be a build error, not a runtime panic.
// Indices are bounded by `i < 128` (for `[_; 128]`) and `i + 128 < 256`
// (for `[_; 256]`).
// reason: const-evaluated with bounded indices (see above).
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
const fn build(
    lower: &[ByteType; 128],
    upper: Option<&[ByteType; 128]>,
    colon_is_nmstrt: bool,
) -> [ByteType; 256] {
    let mut t = [ByteType::NonXml; 256];
    let mut i = 0;
    while i < 128 {
        let bt = lower[i];
        t[i] = if colon_is_nmstrt && matches!(bt, ByteType::Colon) {
            ByteType::Nmstrt
        } else {
            bt
        };
        if let Some(up) = upper {
            t[i + 128] = up[i];
        }
        i += 1;
    }
    t
}

static UTF8: [ByteType; 256] = build(&data::ASCII_LOWER, Some(&data::UTF8_UPPER), true);
static UTF8_NS: [ByteType; 256] = build(&data::ASCII_LOWER, Some(&data::UTF8_UPPER), false);
static INTERNAL_UTF8: [ByteType; 256] =
    build(&data::IASCII_LOWER, Some(&data::UTF8_UPPER), true);
static INTERNAL_UTF8_NS: [ByteType; 256] =
    build(&data::IASCII_LOWER, Some(&data::UTF8_UPPER), false);
static LATIN1: [ByteType; 256] = build(&data::ASCII_LOWER, Some(&data::LATIN1_UPPER), true);
static LATIN1_NS: [ByteType; 256] = build(&data::ASCII_LOWER, Some(&data::LATIN1_UPPER), false);
static ASCII: [ByteType; 256] = build(&data::ASCII_LOWER, None, true);
static ASCII_NS: [ByteType; 256] = build(&data::ASCII_LOWER, None, false);

impl Table {
    /// All tables.
    pub const ALL: [Table; 10] = [
        Table::Utf8,
        Table::Utf8Ns,
        Table::InternalUtf8,
        Table::InternalUtf8Ns,
        Table::Latin1,
        Table::Latin1Ns,
        Table::Ascii,
        Table::AsciiNs,
        Table::Little2,
        Table::Little2Ns,
    ];

    /// The full 256-entry table.
    pub fn entries(self) -> &'static [ByteType; 256] {
        match self {
            Table::Utf8 => &UTF8,
            Table::Utf8Ns => &UTF8_NS,
            Table::InternalUtf8 => &INTERNAL_UTF8,
            Table::InternalUtf8Ns => &INTERNAL_UTF8_NS,
            // UTF-16 code units below 0x100 classify exactly like Latin-1.
            Table::Latin1 | Table::Little2 => &LATIN1,
            Table::Latin1Ns | Table::Little2Ns => &LATIN1_NS,
            Table::Ascii => &ASCII,
            Table::AsciiNs => &ASCII_NS,
        }
    }

    /// Class of byte `b` (`SB_BYTE_TYPE` in `xmltok.c`).
    pub fn byte_type(self, b: u8) -> ByteType {
        // A `[_; 256]` indexed by a `u8` cannot be out of bounds; `get` keeps
        // that fact visible to the panic-freedom lints.
        self.entries()
            .get(usize::from(b))
            .copied()
            .unwrap_or(ByteType::NonXml)
    }
}

#[cfg(test)]
// reason: test code; an out-of-bounds index or overflow there is a test failure.
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod tests;
