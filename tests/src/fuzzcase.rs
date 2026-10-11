//! The per-input checks of the whole-parser fuzz targets and their
//! deterministic seed generator.
//!
//! The fuzz targets (`fuzz/fuzz_targets/parse_trace.rs`,
//! `fuzz/fuzz_targets/buffer_diff.rs`) panic when a check returns an error;
//! `tests/tests/fuzz_corpus.rs` replays the committed corpora
//! (`fuzz/corpus/<target>/`) through the same checks on every CI platform.
//! [`seeds`] produces the seed inputs the corpora are regenerated from
//! (`docs/FUZZING.md`); it is a pure function of a fixed seed.

use crate::docgen::{Rng, document, dtd_document, entity_document};
use crate::port::{PortTrace, port_trace};
use crate::trace::{first_difference, is_well_formed_trace};
use crate::vectors::upstream;
use expat_oracle::BufferOp;

/// The `parse_trace` check: the first two bytes select the options
/// ([`crate::options_from_selectors`]), the rest is the document. The C
/// trace must be well formed and, unless the port stopped at a part that is
/// not ported yet, equal to the port's trace. Inputs shorter than two bytes
/// pass.
pub fn parse_trace_case(data: &[u8]) -> Result<(), String> {
    let [sel, sel2, doc @ ..] = data else {
        return Ok(());
    };
    let opts = crate::options_from_selectors(*sel, *sel2);
    let c = expat_oracle::parse_trace(doc, &opts).map_err(|e| format!("oracle shim: {e:?}"))?;
    if !is_well_formed_trace(&c) {
        return Err("malformed C trace".into());
    }
    if let PortTrace::Trace(rust) = port_trace(doc, &opts)
        && let Some((line, l, r)) = first_difference(&c, &rust)
    {
        return Err(format!(
            "trace mismatch at line {line}: C `{l}` port `{r}` ({opts:?})"
        ));
    }
    Ok(())
}

/// Decodes a `buffer_diff` input: one flag byte (bit 0 namespaces, bit 1
/// reparse deferral off, bits 4-7 the number of operations minus one), 4
/// bytes per operation (kind and `isFinal`, `n` as 16-bit little endian,
/// request slack), then the document. `None` when the input is too short.
pub fn decode_buffer_input(data: &[u8]) -> Option<(Vec<BufferOp>, bool, bool, &[u8])> {
    let (&flags, rest) = data.split_first()?;
    let nops = usize::from(flags >> 4) + 1;
    if rest.len() < 4 * nops {
        return None;
    }
    let (op_bytes, doc) = rest.split_at(4 * nops);
    let ops = op_bytes
        .as_chunks::<4>()
        .0
        .iter()
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
    Some((ops, flags & 1 != 0, flags & 2 == 0, doc))
}

/// The `buffer_diff` check: after every operation the buffer state of C
/// and of the port must be equal. Inputs too short to decode pass.
pub fn buffer_diff_case(data: &[u8]) -> Result<(), String> {
    let Some((ops, ns, deferral, doc)) = decode_buffer_input(data) else {
        return Ok(());
    };
    let c = expat_oracle::buffer_run(doc, &ops, ns, deferral);
    let r = crate::port::buffer_run(doc, &ops, ns, deferral);
    if stopped_at_dtd(&r) {
        // The port stopped at a document type declaration
        // (`Stop::DtdNotPorted`, module 11): nothing to compare yet, as
        // in `parse_trace_case`.
        return Ok(());
    }
    if r != c {
        return Err(format!(
            "port and C differ for {ops:?}:\nport:\n{r}\nC:\n{c}"
        ));
    }
    Ok(())
}

/// True when a [`crate::port::buffer_run`] output reports
/// `Stop::DtdNotPorted` (error code 999) after some operation.
fn stopped_at_dtd(port_output: &str) -> bool {
    port_output
        .lines()
        .any(|l| l.split(' ').nth(1) == Some("999"))
}

/// Fixed seed of [`seeds`]; changing it changes every seed input.
pub const SEED: u64 = 0x7365_6564_732d_3130;

/// Selector pairs every upstream vector is paired with in the
/// `parse_trace` seeds: default, 1-byte chunks, 3-byte chunks with `|`
/// namespaces, UTF-16 override, ISO-8859-1 override with 2-byte chunks,
/// buffer API without reparse deferral, parameter entity parsing always,
/// namespaces with triplets.
const VECTOR_SELECTORS: [[u8; 2]; 8] = [
    [0x00, 0x00],
    [0x01, 0x00],
    [0x06, 0x00],
    [0x10, 0x00],
    [0x2B, 0x00],
    [0x01, 0x03],
    [0x80, 0x00],
    [0x04, 0x40],
];

/// The seed inputs of a fuzz target (`parse_trace` or `buffer_diff`), in a
/// fixed order; `None` for a target without generated seeds.
///
/// `parse_trace`: every upstream vector under each of
/// `VECTOR_SELECTORS`, then 2,000 generated documents ([`document`]) with
/// random selector pairs, then 2,000 documents with a document type
/// declaration ([`dtd_document`]), then 1,500 entity documents
/// ([`entity_document`]). `buffer_diff`: every upstream vector, 2,000
/// generated documents, 1,000 documents with a DTD and 500 entity
/// documents, each with a
/// random operation header of 1-16 operations whose sizes cover the
/// document.
pub fn seeds(target: &str) -> Option<Vec<Vec<u8>>> {
    let mut r = Rng::new(SEED);
    let vectors: Vec<Vec<u8>> = upstream().iter().map(|v| v.doc()).collect();
    match target {
        "parse_trace" => {
            let mut out = Vec::new();
            for doc in &vectors {
                for sel in VECTOR_SELECTORS {
                    let mut input = sel.to_vec();
                    input.extend_from_slice(doc);
                    out.push(input);
                }
            }
            for _ in 0..2000 {
                let doc = document(&mut r);
                let mut input = vec![r.next_u64() as u8, r.next_u64() as u8];
                input.extend_from_slice(&doc);
                out.push(input);
            }
            // Module 11: documents with a document type declaration.
            for _ in 0..2000 {
                let doc = dtd_document(&mut r);
                let mut input = vec![r.next_u64() as u8, r.next_u64() as u8];
                input.extend_from_slice(&doc);
                out.push(input);
            }
            // Module 11b: documents built around internal entities.
            for _ in 0..1500 {
                let doc = entity_document(&mut r);
                let mut input = vec![r.next_u64() as u8, r.next_u64() as u8];
                input.extend_from_slice(&doc);
                out.push(input);
            }
            Some(out)
        }
        "buffer_diff" => {
            let mut out = Vec::new();
            let generated: Vec<Vec<u8>> = (0..2000).map(|_| document(&mut r)).collect();
            for doc in vectors.iter().chain(&generated) {
                out.push(buffer_seed(&mut r, doc));
            }
            // Module 11: documents with a document type declaration.
            for _ in 0..1000 {
                let doc = dtd_document(&mut r);
                out.push(buffer_seed(&mut r, &doc));
            }
            // Module 11b: documents built around internal entities.
            for _ in 0..500 {
                let doc = entity_document(&mut r);
                out.push(buffer_seed(&mut r, &doc));
            }
            Some(out)
        }
        _ => None,
    }
}

/// A `buffer_diff` input for `doc`: random flags and operations that feed
/// the document in pieces, the last one final.
fn buffer_seed(r: &mut Rng, doc: &[u8]) -> Vec<u8> {
    let nops = 1 + r.below(16);
    let flags = ((nops - 1) as u8) << 4 | (r.next_u64() as u8 & 0b11);
    let mut input = vec![flags];
    let mut left = doc.len();
    for i in 0..nops {
        let last = i + 1 == nops;
        let n = if last {
            left
        } else {
            r.below(left.min(64) + 1)
        };
        left -= n;
        let n = u16::try_from(n).unwrap_or(u16::MAX);
        let buffer = r.one_in(2);
        let kind = u8::from(last) | if buffer { 2 } else { 0 };
        let slack = if r.one_in(20) {
            *r.pick(&[254u8, 255])
        } else {
            r.below(4) as u8
        };
        let [lo, hi] = n.to_le_bytes();
        input.extend_from_slice(&[kind, lo, hi, slack]);
    }
    input.extend_from_slice(doc);
    input
}
