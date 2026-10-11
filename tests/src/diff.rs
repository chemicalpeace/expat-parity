//! Comparison of two trace sources.

use crate::trace::first_difference;
use expat_oracle::{ParamEntityParsing, ParseOptions};

/// Anything that can produce a canonical trace for a document.
pub trait TraceSource {
    /// Human-readable name used in mismatch reports.
    fn name(&self) -> &str;
    /// Canonical trace of parsing `doc` with `opts`.
    fn trace(&self, doc: &[u8], opts: &ParseOptions) -> Vec<u8>;
}

/// The libexpat C reference.
#[derive(Debug, Default, Clone, Copy)]
pub struct Oracle;

impl TraceSource for Oracle {
    fn name(&self) -> &str {
        "libexpat-2.9.0"
    }

    fn trace(&self, doc: &[u8], opts: &ParseOptions) -> Vec<u8> {
        expat_oracle::parse_trace(doc, opts).expect("oracle shim failed to allocate")
    }
}

/// The option sets every differential document is checked under.
pub fn option_matrix() -> Vec<ParseOptions> {
    let base = ParseOptions::default();
    vec![
        base.clone(),
        ParseOptions {
            chunk_size: 1,
            ..base.clone()
        },
        ParseOptions {
            chunk_size: 7,
            ..base.clone()
        },
        ParseOptions {
            ns_separator: Some(b'|'),
            ..base.clone()
        },
        ParseOptions {
            encoding: Some("ISO-8859-1".into()),
            ..base.clone()
        },
        ParseOptions {
            encoding: Some("UTF-16".into()),
            ..base.clone()
        },
        ParseOptions {
            param_entity_parsing: ParamEntityParsing::Always,
            ns_separator: Some(b'\n'),
            chunk_size: 3,
            ..base
        },
    ]
}

/// One disagreement between two sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    /// Identifier of the document (vector id, generator seed, ...).
    pub doc_id: String,
    /// Options under which the traces differ.
    pub options: ParseOptions,
    /// First differing line number (1-based).
    pub line: usize,
    /// That line in the left trace.
    pub left: String,
    /// That line in the right trace.
    pub right: String,
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {:?}: line {}: left `{}` right `{}`",
            self.doc_id, self.options, self.line, self.left, self.right
        )
    }
}

/// Compares `left` and `right` on every document under every option set.
pub fn compare_all<'a>(
    left: &dyn TraceSource,
    right: &dyn TraceSource,
    docs: impl IntoIterator<Item = (String, &'a [u8])>,
    options: &[ParseOptions],
) -> Vec<Mismatch> {
    let mut out = Vec::new();
    for (id, doc) in docs {
        for opts in options {
            let a = left.trace(doc, opts);
            let b = right.trace(doc, opts);
            if let Some((line, l, r)) = first_difference(&a, &b) {
                out.push(Mismatch {
                    doc_id: id.clone(),
                    options: opts.clone(),
                    line,
                    left: l,
                    right: r,
                });
            }
        }
    }
    out
}
