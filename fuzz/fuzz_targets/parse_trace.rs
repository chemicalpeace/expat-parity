//! Whole-document differential target. The first two input bytes select
//! the parse options (namespaces and separator, chunk size, encoding
//! override, parameter entity parsing, `XML_Parse` or the buffer API,
//! reparse deferral; `expat_difftest::options_from_selectors`); the rest is
//! the document.
//!
//! The port's parser and the C oracle parse the document and their
//! canonical traces must be equal (`expat_difftest::fuzzcase::parse_trace_case`,
//! also run over the committed corpus by `tests/tests/fuzz_corpus.rs`).
//! A document containing `expat_difftest::docgen::EXTERNAL_MARKER` carries
//! the bodies of its external entities, which the test handler reads
//! through external entity parsers (module 11d).
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Err(e) = expat_difftest::fuzzcase::parse_trace_case(data) {
        panic!("{e}");
    }
});
