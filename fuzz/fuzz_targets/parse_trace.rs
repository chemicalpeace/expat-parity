//! Whole-document differential target. The first input byte selects the
//! parse options (namespaces, chunk size, encoding override, parameter
//! entity parsing); the rest is the document.
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
    let Some((&sel, doc)) = data.split_first() else {
        return;
    };
    let opts = expat_difftest::options_from_selector(sel);
    let c = expat_oracle::parse_trace(doc, &opts).expect("oracle shim allocation");
    assert!(is_well_formed_trace(&c), "malformed trace");
    if let PortTrace::Trace(rust) = port_trace(doc, &opts) {
        if let Some((line, l, r)) = first_difference(&c, &rust) {
            panic!("trace mismatch at line {line}: C `{l}` port `{r}` ({opts:?})");
        }
    }
});
