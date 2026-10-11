//! Differential test harness for the expat port.
//!
//! - [`trace`]: the canonical trace format that defines "equal"
//!   (`docs/ARCHITECTURE.md`), a Rust writer for it, and validation/diff helpers.
//! - [`diff`]: comparison of two [`diff::TraceSource`]s over documents and an
//!   option matrix.
//! - [`vectors`]: test documents extracted from libexpat's test suite.
//! - [`tok`]: differential checks of the tokenizer port.

pub mod diff;
pub mod tok;
pub mod trace;
pub mod vectors;

use expat_oracle::{ParamEntityParsing, ParseOptions};

/// Maps one selector byte (used by fuzz targets) to parse options, so a fuzzer
/// explores options together with documents.
///
/// Bits 0-1: chunking (whole, 1, 3, 64 bytes); bit 2: namespaces with `|`
/// separator; bits 3-5: encoding override (none, UTF-8, UTF-16, UTF-16LE,
/// UTF-16BE, ISO-8859-1, US-ASCII, none); bits 6-7: parameter entity parsing.
pub fn options_from_selector(sel: u8) -> ParseOptions {
    let chunk_size = match sel & 0b11 {
        0 => 0,
        1 => 1,
        2 => 3,
        _ => 64,
    };
    let ns_separator = (sel & 0b100 != 0).then_some(b'|');
    let encoding = match (sel >> 3) & 0b111 {
        1 => Some("UTF-8"),
        2 => Some("UTF-16"),
        3 => Some("UTF-16LE"),
        4 => Some("UTF-16BE"),
        5 => Some("ISO-8859-1"),
        6 => Some("US-ASCII"),
        _ => None,
    }
    .map(String::from);
    let param_entity_parsing = match sel >> 6 {
        1 => ParamEntityParsing::UnlessStandalone,
        2 => ParamEntityParsing::Always,
        _ => ParamEntityParsing::Never,
    };
    ParseOptions {
        encoding,
        ns_separator,
        chunk_size,
        param_entity_parsing,
    }
}
