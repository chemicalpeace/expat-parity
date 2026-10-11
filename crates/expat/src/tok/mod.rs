//! The tokenizer: splits a byte buffer into XML tokens.
//!
//! Port of `xmltok_impl.c` (the scanner template that `xmltok.c` instantiates
//! once per encoding family) and of the per-encoding glue in `xmltok.c`.
//!
//! The C code is a template parameterised by macros (`MINBPC`, `BYTE_TYPE`,
//! `IS_NAME_CHAR`, ...). Here those macros are the methods of the
//! [`Encoding`] trait and the template is the set of generic functions in
//! [`scan`]. Every function is a line-by-line port; the differences are
//! listed in `docs/ARCHITECTURE.md` ("Tokenizer") and are all about
//! representation, never about which token is produced:
//!
//! - Pointers are byte offsets into a slice. The end of the scan is the end
//!   of the slice.
//! - The C out-parameter `nextTokPtr` is the `next` field of [`Scanned`]. It
//!   is `None` exactly when the C code leaves `*nextTokPtr` untouched.
//! - The C token `int` is the typed [`Scan`]; [`Scan::to_c`] gives the C
//!   value back.
//! - Functions whose C version relies on well-formed input to stop
//!   (`nameLength`, `skipS`, `getAtts`, `charRefNumber`) additionally stop at
//!   the end of the slice. On input that satisfies the C precondition the
//!   result is identical; on other input C reads out of bounds while the port
//!   returns a defined value.
//!
//! ```
//! # #[cfg(feature = "internal-api")] {
//! use expat::internal::tok::{content_tok, Normal, Scan, Tok};
//! let enc = Normal::Utf8;
//! let r = content_tok(&enc, b"<doc a='1'/>rest", 0);
//! assert_eq!(r.scan, Scan::Token(Tok::EmptyElementWithAtts));
//! assert_eq!(r.next, Some(12));
//! # }
//! ```

mod normal;
pub mod scan;

pub use normal::Normal;
pub use scan::{
    Attribute, Position, attribute_value_tok, cdata_section_tok, char_ref_number, content_tok,
    entity_value_tok, get_atts, ignore_section_tok, is_public_id, name_length,
    name_matches_ascii, predefined_entity_name, prolog_tok, skip_s, update_position,
};

use crate::chartab::ByteType;

/// The encoding-specific primitives of the scanner template.
///
/// These are the macros that `xmltok.c` defines before each inclusion of
/// `xmltok_impl.c`. Positions are offsets into `buf`. Callers guarantee that
/// `p + Self::MINBPC <= buf.len()` (respectively `p + n`) for every call the
/// C code would make; implementations must still be total and return a
/// fixed value for positions outside `buf`.
pub trait Encoding {
    /// `MINBPC`: minimum bytes per character (1, or 2 for UTF-16).
    const MINBPC: usize;

    /// `BYTE_TYPE(enc, p)`.
    fn byte_type(&self, buf: &[u8], p: usize) -> ByteType;

    /// `BYTE_TO_ASCII(enc, p)`. Only compared against ASCII values.
    fn byte_to_ascii(&self, buf: &[u8], p: usize) -> i32;

    /// `CHAR_MATCHES(enc, p, c)` for an ASCII character `c`.
    fn char_matches(&self, buf: &[u8], p: usize, c: u8) -> bool;

    /// `IS_NAME_CHAR_MINBPC(enc, p)`, used for [`ByteType::NonAscii`].
    fn is_name_char_minbpc(&self, buf: &[u8], p: usize) -> bool;

    /// `IS_NMSTRT_CHAR_MINBPC(enc, p)`, used for [`ByteType::NonAscii`].
    fn is_nmstrt_char_minbpc(&self, buf: &[u8], p: usize) -> bool;

    /// `IS_NAME_CHAR(enc, p, n)` for a lead byte of an `n`-byte sequence.
    fn is_name_char(&self, buf: &[u8], p: usize, n: usize) -> bool;

    /// `IS_NMSTRT_CHAR(enc, p, n)`.
    fn is_nmstrt_char(&self, buf: &[u8], p: usize, n: usize) -> bool;

    /// `IS_INVALID_CHAR(enc, p, n)`.
    fn is_invalid_char(&self, buf: &[u8], p: usize, n: usize) -> bool;
}

/// A positive `XML_TOK_*` token (`xmltok.h`). `Tok as i32` is the C value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Tok {
    /// `XML_TOK_START_TAG_WITH_ATTS`
    StartTagWithAtts = 1,
    /// `XML_TOK_START_TAG_NO_ATTS`
    StartTagNoAtts = 2,
    /// `XML_TOK_EMPTY_ELEMENT_WITH_ATTS`
    EmptyElementWithAtts = 3,
    /// `XML_TOK_EMPTY_ELEMENT_NO_ATTS`
    EmptyElementNoAtts = 4,
    /// `XML_TOK_END_TAG`
    EndTag = 5,
    /// `XML_TOK_DATA_CHARS`
    DataChars = 6,
    /// `XML_TOK_DATA_NEWLINE`
    DataNewline = 7,
    /// `XML_TOK_CDATA_SECT_OPEN`
    CdataSectOpen = 8,
    /// `XML_TOK_ENTITY_REF`
    EntityRef = 9,
    /// `XML_TOK_CHAR_REF`
    CharRef = 10,
    /// `XML_TOK_PI`
    Pi = 11,
    /// `XML_TOK_XML_DECL`
    XmlDecl = 12,
    /// `XML_TOK_COMMENT`
    Comment = 13,
    /// `XML_TOK_BOM`
    Bom = 14,
    /// `XML_TOK_PROLOG_S`
    PrologS = 15,
    /// `XML_TOK_DECL_OPEN`
    DeclOpen = 16,
    /// `XML_TOK_DECL_CLOSE`
    DeclClose = 17,
    /// `XML_TOK_NAME`
    Name = 18,
    /// `XML_TOK_NMTOKEN`
    Nmtoken = 19,
    /// `XML_TOK_POUND_NAME`
    PoundName = 20,
    /// `XML_TOK_OR`
    Or = 21,
    /// `XML_TOK_PERCENT`
    Percent = 22,
    /// `XML_TOK_OPEN_PAREN`
    OpenParen = 23,
    /// `XML_TOK_CLOSE_PAREN`
    CloseParen = 24,
    /// `XML_TOK_OPEN_BRACKET`
    OpenBracket = 25,
    /// `XML_TOK_CLOSE_BRACKET`
    CloseBracket = 26,
    /// `XML_TOK_LITERAL`
    Literal = 27,
    /// `XML_TOK_PARAM_ENTITY_REF`
    ParamEntityRef = 28,
    /// `XML_TOK_INSTANCE_START`
    InstanceStart = 29,
    /// `XML_TOK_NAME_QUESTION`
    NameQuestion = 30,
    /// `XML_TOK_NAME_ASTERISK`
    NameAsterisk = 31,
    /// `XML_TOK_NAME_PLUS`
    NamePlus = 32,
    /// `XML_TOK_COND_SECT_OPEN`
    CondSectOpen = 33,
    /// `XML_TOK_COND_SECT_CLOSE`
    CondSectClose = 34,
    /// `XML_TOK_CLOSE_PAREN_QUESTION`
    CloseParenQuestion = 35,
    /// `XML_TOK_CLOSE_PAREN_ASTERISK`
    CloseParenAsterisk = 36,
    /// `XML_TOK_CLOSE_PAREN_PLUS`
    CloseParenPlus = 37,
    /// `XML_TOK_COMMA`
    Comma = 38,
    /// `XML_TOK_ATTRIBUTE_VALUE_S`
    AttributeValueS = 39,
    /// `XML_TOK_CDATA_SECT_CLOSE`
    CdataSectClose = 40,
    /// `XML_TOK_PREFIXED_NAME`
    PrefixedName = 41,
    /// `XML_TOK_IGNORE_SECT`
    IgnoreSect = 42,
}

/// Result class of one scanner call (the C `int` return value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scan {
    /// A complete token (C: the positive token value).
    Token(Tok),
    /// A token that may continue past the end of the buffer (C: the negated
    /// token value, e.g. `-XML_TOK_PROLOG_S`).
    PartialToken(Tok),
    /// `XML_TOK_INVALID` (0).
    Invalid,
    /// `XML_TOK_PARTIAL` (-1): only part of a token.
    Partial,
    /// `XML_TOK_PARTIAL_CHAR` (-2): only part of a multi-byte character.
    PartialChar,
    /// `XML_TOK_TRAILING_CR` (-3).
    TrailingCr,
    /// `XML_TOK_NONE` (-4): nothing to scan.
    None,
    /// `XML_TOK_TRAILING_RSQB` (-5).
    TrailingRsqb,
}

impl Scan {
    /// The C `int` value.
    pub fn to_c(self) -> i32 {
        match self {
            Scan::Token(t) => t as i32,
            Scan::PartialToken(t) => -(t as i32),
            Scan::Invalid => 0,
            Scan::Partial => -1,
            Scan::PartialChar => -2,
            Scan::TrailingCr => -3,
            Scan::None => -4,
            Scan::TrailingRsqb => -5,
        }
    }
}

/// Return value of a scanner: the token and the C `*nextTokPtr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Scanned {
    /// What was found.
    pub scan: Scan,
    /// Offset where the next token starts (or where the error is). `None`
    /// when the C scanner does not write `*nextTokPtr`.
    pub next: Option<usize>,
}

#[cfg(test)]
// reason: test code; an out-of-bounds index or overflow there is a test failure.
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod tests;
