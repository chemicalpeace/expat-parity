//! Differential target for the tokenizer (modules 5 and 6): every scanner,
//! tokenizer helper and converter of the port against `xmltok.c` /
//! `xmltok_impl.c`, through `expat_difftest::tok`.
//!
//! The first byte selects the encoding: the 8 single-byte encodings, the 4
//! UTF-16 encodings, or (for the remaining values) an unknown encoding whose
//! map and converter table are generated from the next 8 bytes as a seed.
//! The rest is the buffer. Every offset of short buffers is a start
//! position; longer buffers are additionally walked token by token like the
//! parser does, and converted with a range of output sizes.
#![no_main]

use expat::internal::tok::{Normal, Utf16};
use expat_difftest::tok::{
    Subject, check_buffer, check_convert, check_token_walk, unknown_from_seed,
};
use libfuzzer_sys::fuzz_target;

fn run<S: Subject>(s: &S, buf: &[u8]) {
    check_buffer(s, buf, 0..=buf.len().min(64));
    check_token_walk(s, buf);
    check_convert(s, buf, buf.len() <= 256);
}

fuzz_target!(|data: &[u8]| {
    let Some((&sel, buf)) = data.split_first() else {
        return;
    };
    let sel = usize::from(sel);
    if let Some(&enc) = Normal::ALL.get(sel) {
        run(&enc, buf);
    } else if let Some(&enc) = Utf16::ALL.get(sel - Normal::ALL.len()) {
        run(&enc, buf);
    } else if let Some((seed, buf)) = buf.split_first_chunk::<8>() {
        if let Some(pair) = unknown_from_seed(u64::from_le_bytes(*seed)) {
            run(&pair, buf);
        }
    }
});
