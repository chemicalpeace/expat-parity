//! Whole-document target. The first input byte selects the parse options
//! (namespaces, chunk size, encoding override); the rest is the document.
//!
//! Until the Rust parser exists this target exercises the oracle and the
//! trace format (it must never crash and must always end with a `result`
//! line). When `expat::Parser` lands it becomes a differential target.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&sel, doc)) = data.split_first() else {
        return;
    };
    let opts = expat_difftest::options_from_selector(sel);
    let trace = expat_oracle::parse_trace(doc, &opts).expect("oracle shim allocation");
    assert!(
        expat_difftest::trace::is_well_formed_trace(&trace),
        "malformed trace"
    );
});
