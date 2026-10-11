//! Whole-document differential target. The first two input bytes select
//! the parse options (namespaces and separator, chunk size, encoding
//! override, parameter entity parsing, `XML_Parse` or the buffer API,
//! reparse deferral; `expat_difftest::options_from_selectors`); the rest is
//! the document.
//!
//! The port's parser and the C oracle parse the document and their
//! canonical traces must be equal (`expat_difftest::fuzzcase::parse_trace_case`,
//! also run over the committed corpus by `tests/tests/fuzz_corpus.rs`).
//! Documents with a document type declaration stop the port with
//! `Stop::DtdNotPorted` until module 11; for those only the oracle's trace
//! is checked for well-formedness.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Err(e) = expat_difftest::fuzzcase::parse_trace_case(data) {
        panic!("{e}");
    }
});
