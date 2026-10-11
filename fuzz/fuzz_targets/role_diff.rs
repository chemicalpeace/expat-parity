//! Differential target for the prolog state machine (module 8): the port's
//! `expat::role` against `xmlrole.c`, through `expat_difftest::role`.
//!
//! The first byte selects the encoding (low 4 bits, modulo the 12 built-in
//! encodings) and the mode: bit 6 set reads the rest as synthetic
//! `(token kind, keyword text)` byte pairs, otherwise the rest is a document
//! that is tokenized in the prolog state; bit 7 starts the machine as an
//! external entity (in document mode both starts are always compared).
#![no_main]

use expat::internal::tok::{Normal, Utf16};
use expat_difftest::role::{check_document, check_roles, synthetic};
use expat_difftest::tok::Subject;
use libfuzzer_sys::fuzz_target;

fn run<S: Subject>(s: &S, synthetic_mode: bool, external: bool, rest: &[u8]) {
    if synthetic_mode {
        let picks: Vec<(usize, usize)> = rest
            .chunks_exact(2)
            .map(|p| (usize::from(p[0]), usize::from(p[1])))
            .collect();
        let (buf, toks) = synthetic(s, &picks);
        check_roles(s, external, &buf, &toks);
    } else {
        check_document(s, rest);
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((&sel, rest)) = data.split_first() else {
        return;
    };
    let synthetic_mode = sel & 0x40 != 0;
    let external = sel & 0x80 != 0;
    let e = usize::from(sel & 0x0F) % 12;
    if let Some(&enc) = Normal::ALL.get(e) {
        run(&enc, synthetic_mode, external, rest);
    } else {
        run(&Utf16::ALL[e - 8], synthetic_mode, external, rest);
    }
});
