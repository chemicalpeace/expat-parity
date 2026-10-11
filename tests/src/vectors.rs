//! Test documents extracted from libexpat's C test suite by
//! `scripts/extract_vectors.py` (provenance in every record).

use serde::Deserialize;

/// One extracted document.
#[derive(Debug, Clone, Deserialize)]
pub struct Vector {
    /// `<file>:<line>` of the literal in `expat/tests/`.
    pub id: String,
    /// Upstream path and line.
    pub source: String,
    /// Upstream `START_TEST` name.
    pub test: String,
    /// True when the upstream test does not configure the parser, so its
    /// expectation applies to a default parser fed the whole document.
    pub plain: bool,
    /// `XML_ERROR_*` name the upstream test expects, when it states one.
    pub expected_error: Option<String>,
    /// Numeric value of `expected_error` in libexpat 2.9.0.
    pub expected_error_code: Option<u32>,
    /// Document bytes, hex encoded (byte-exact, may contain NULs).
    pub doc_hex: String,
    /// How many further identical literals upstream contains.
    pub dups: u32,
}

impl Vector {
    /// The document bytes.
    pub fn doc(&self) -> Vec<u8> {
        decode_hex(&self.doc_hex).expect("vector doc_hex is valid hex")
    }
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Raw contents of `tests/vectors/upstream.jsonl`.
pub const UPSTREAM_JSONL: &str = include_str!("../vectors/upstream.jsonl");

/// All upstream vectors.
pub fn upstream() -> Vec<Vector> {
    UPSTREAM_JSONL
        .lines()
        .map(|l| serde_json::from_str(l).expect("valid vector record"))
        .collect()
}
