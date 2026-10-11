//! White-box differential target for the input buffer (module 10):
//! `XML_Parse`, `XML_GetBuffer`, `XML_ParseBuffer` and reparse deferral.
//!
//! Input: one flag byte (bit 0 namespaces, bit 1 reparse deferral off, bits
//! 4-7 the number of operations), then 4 bytes per operation (kind and
//! `isFinal`, `n` as 16-bit little endian, request slack), then the
//! document (`expat_difftest::fuzzcase::decode_buffer_input`). After every
//! operation the buffer state of C and of the port must be equal
//! (`expat_difftest::fuzzcase::buffer_diff_case`, also run over the
//! committed corpus by `tests/tests/fuzz_corpus.rs`).
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Err(e) = expat_difftest::fuzzcase::buffer_diff_case(data) {
        panic!("{e}");
    }
});
