//! Differential target for the tokenizer (module 5): every scanner and
//! tokenizer helper of the port against `xmltok_impl.c`, through
//! `expat_difftest::tok`. The first byte selects the encoding; the rest is
//! the buffer. Every offset of short buffers is a start position; longer
//! buffers are additionally walked token by token like the parser does.
#![no_main]

use expat::internal::tok::Normal;
use expat_difftest::tok::{check_buffer, check_token_walk};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&sel, buf)) = data.split_first() else {
        return;
    };
    let enc = Normal::ALL[usize::from(sel) % Normal::ALL.len()];
    check_buffer(enc, buf, 0..=buf.len().min(64));
    check_token_walk(enc, buf);
});
