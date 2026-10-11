//! White-box differential target for the input buffer (module 10):
//! `XML_Parse`, `XML_GetBuffer`, `XML_ParseBuffer` and reparse deferral.
//!
//! Input: one flag byte (bit 0 namespaces, bit 1 reparse deferral off, bits
//! 4-7 the number of operations), then 4 bytes per operation (kind and
//! `isFinal`, `n` as 16-bit little endian, request slack), then the
//! document. After every operation the buffer state of C and of the port
//! must be equal (`expat_oracle::buffer_run`, `port::buffer_run`).
#![no_main]

use expat_oracle::BufferOp;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&flags, rest)) = data.split_first() else {
        return;
    };
    let nops = usize::from(flags >> 4) + 1;
    if rest.len() < 4 * nops {
        return;
    }
    let (op_bytes, doc) = rest.split_at(4 * nops);
    let ops: Vec<BufferOp> = op_bytes
        .chunks_exact(4)
        .map(|b| {
            let is_final = b[0] & 1 != 0;
            let n = i32::from(u16::from_le_bytes([b[1], b[2]]));
            if b[0] & 2 == 0 {
                BufferOp::Parse { n, is_final }
            } else {
                // Keep n <= req so no unwritten byte is parsed, or make the
                // request fail without allocating.
                let req = match b[3] {
                    255 => i32::MAX - i32::from(b[0] >> 2),
                    254 => -1,
                    s => n + i32::from(s) * 12,
                };
                BufferOp::Buffer { req, n, is_final }
            }
        })
        .collect();
    let ns = flags & 1 != 0;
    let deferral = flags & 2 == 0;
    let c = expat_oracle::buffer_run(doc, &ops, ns, deferral);
    let r = expat_difftest::port::buffer_run(doc, &ops, ns, deferral);
    assert_eq!(r, c, "port (left) and C (right) differ for {ops:?}");
});
