//! Whole-document differential target. The first two input bytes select
//! the parse options (namespaces and separator, chunk size, encoding
//! override, parameter entity parsing, `XML_Parse` or the buffer API,
//! reparse deferral; `expat_difftest::options_from_selectors`); the rest is
//! the document.
//!
//! The port's parser and the C oracle parse the document and their
//! canonical traces must be equal. Documents with a document type
//! declaration stop the port with `Stop::DtdNotPorted` until module 11;
//! for those only the oracle's trace is checked for well-formedness.
#![no_main]

use expat_difftest::port::{PortTrace, port_trace};
use expat_difftest::trace::{first_difference, is_well_formed_trace};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let [sel, sel2, doc @ ..] = data else {
        return;
    };
    let opts = expat_difftest::options_from_selectors(*sel, *sel2);
    let c = expat_oracle::parse_trace(doc, &opts).expect("oracle shim allocation");
    assert!(is_well_formed_trace(&c), "malformed trace");
    if let PortTrace::Trace(rust) = port_trace(doc, &opts) {
        if let Some((line, l, r)) = first_difference(&c, &rust) {
            panic!("trace mismatch at line {line}: C `{l}` port `{r}` ({opts:?})");
        }
    }
});
