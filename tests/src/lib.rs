//! Differential test harness for the expat port.
//!
//! - [`trace`]: the canonical trace format that defines "equal"
//!   (`docs/ARCHITECTURE.md`), a Rust writer for it, and validation/diff helpers.
//! - [`diff`]: comparison of two [`diff::TraceSource`]s over documents and an
//!   option matrix.
//! - [`vectors`]: test documents extracted from libexpat's test suite.
//! - [`tok`]: differential checks of the tokenizer port.
//! - [`role`]: differential checks of the prolog state machine.
//! - [`docgen`]: random XML-ish documents for whole-parser tests.
//! - [`port`]: the trace adapter of the port's parser.
//! - [`fuzzcase`]: the checks of the whole-parser fuzz targets and their
//!   deterministic seed generator.
//! - [`bounded`]: comparisons bounded in input size, wall-clock time and
//!   memory, for the resource-limit tests.
//! - [`core`]: white-box differential checks of the string pool and hash
//!   table of the parser core.

pub mod bounded;
pub mod core;
pub mod diff;
pub mod docgen;
pub mod fuzzcase;
pub mod port;
pub mod role;
pub mod tok;
pub mod trace;
pub mod tracker;
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
        ..ParseOptions::default()
    }
}

/// [`options_from_selector`] extended by a second selector byte for the
/// whole-parser target: bit 0 feeds through `XML_GetBuffer` +
/// `XML_ParseBuffer`, bit 1 disables reparse deferral, bits 2-3 pick the
/// namespace separator used when namespaces are on (`|`, `\n`, `:`, `#`),
/// bits 4-5 replace the chunk size of 64 by 2, 7, 1024 or keep it, bit 6
/// turns on `XML_SetReturnNSTriplet` (namespace triplets), bit 7 sets
/// small billion-laughs limits (module 11: factor `1 + (sel2 & 3)`,
/// activation threshold `32 * ((sel2 >> 2) & 15)` bytes) and traces the
/// accounting counters, so the fuzzer reaches
/// `XML_ERROR_AMPLIFICATION_LIMIT_BREACH` everywhere.
pub fn options_from_selectors(sel: u8, sel2: u8) -> ParseOptions {
    let mut o = options_from_selector(sel);
    o.buffer_api = sel2 & 1 != 0;
    o.no_reparse_deferral = sel2 & 2 != 0;
    if o.ns_separator.is_some() {
        o.ns_separator = Some(b"|\n:#"[usize::from((sel2 >> 2) & 3)]);
    }
    o.ns_triplets = sel2 & 0x40 != 0;
    if o.chunk_size == 64 {
        o.chunk_size = [2, 7, 1024, 64][usize::from((sel2 >> 4) & 3)];
    }
    if sel2 & 0x80 != 0 {
        o.trace_accounting = true;
        o.limits.bl_amplification = Some(expat_oracle::Factor(1.0 + f32::from(sel2 & 3)));
        o.limits.bl_threshold = Some(32 * u64::from((sel2 >> 2) & 15));
    }
    o
}
