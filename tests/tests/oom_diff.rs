//! Allocation failure injection against the C oracle.
//!
//! libexpat's own tests make the memory suite passed to
//! `XML_ParserCreate_MM` fail from the n-th call on and check that the
//! parser reports `XML_ERROR_NO_MEMORY` and cleans up. The oracle does the
//! same (`ParseOptions::fail_alloc_after`); the port has no allocator calls
//! of its own at these points, so it fails at the same C allocation
//! through the tracker that replays C's allocations (D-034,
//! `AllocTracker::fail_allocations_after`). For every document and option
//! set, every failure point from the first allocator call after creation
//! to the point where the parse no longer fails is compared: the traces,
//! including the events before the failure and the error, must be equal.

use expat_difftest::docgen::{self, Rng};
use expat_difftest::port::port_trace;
use expat_difftest::trace::first_difference;
use expat_difftest::vectors::upstream;
use expat_oracle::{ExtHandler, ParamEntityParsing, ParseOptions};

/// Upper bound of failure points per document and option set.
const MAX_POINTS: u64 = 4000;

fn option_sets() -> Vec<ParseOptions> {
    let base = ParseOptions::default();
    vec![
        base.clone(),
        ParseOptions {
            ns_separator: Some(b'|'),
            ns_triplets: true,
            ..base.clone()
        },
        ParseOptions {
            chunk_size: 7,
            ..base.clone()
        },
        ParseOptions {
            chunk_size: 5,
            buffer_api: true,
            param_entity_parsing: ParamEntityParsing::Always,
            ..base
        },
    ]
}

/// Compares every failure point of `doc` under `opts`; returns the number
/// of points compared, or the first difference.
fn sweep(id: &str, doc: &[u8], opts: &ParseOptions) -> Result<u64, String> {
    let full = expat_oracle::parse_trace(doc, opts).expect("oracle");
    for n in 0..MAX_POINTS {
        let o = ParseOptions {
            fail_alloc_after: Some(n),
            ..opts.clone()
        };
        let c = expat_oracle::parse_trace(doc, &o).expect("oracle");
        let p = port_trace(doc, &o);
        if let Some((line, l, r)) = first_difference(&c, &p) {
            return Err(format!(
                "{id} {opts:?} fail after {n}: line {line}: C `{l}` port `{r}`"
            ));
        }
        if c == full {
            return Ok(n + 1);
        }
    }
    Err(format!(
        "{id} {opts:?}: still failing after {MAX_POINTS} points"
    ))
}

#[test]
fn upstream_vectors_fail_at_every_allocation_like_c() {
    let mut points = 0u64;
    let mut failures = Vec::new();
    for v in upstream() {
        let doc = v.doc();
        for opts in option_sets() {
            match sweep(&v.id, &doc, &opts) {
                Ok(n) => points += n,
                Err(e) => failures.push(e),
            }
        }
    }
    for f in failures.iter().take(20) {
        eprintln!("{f}");
    }
    eprintln!(
        "{points} failure points compared, {} differences",
        failures.len()
    );
    assert!(failures.is_empty());
}

/// Generated documents (content, DTD and entity generators) under random
/// option sets.
#[test]
fn generated_documents_fail_at_every_allocation_like_c() {
    let mut r = Rng::new(0x00f0_0d0e);
    let mut points = 0u64;
    let mut failures = Vec::new();
    for i in 0..1_500 {
        let doc = match i % 3 {
            0 => docgen::document(&mut r),
            1 => docgen::dtd_document(&mut r),
            _ => docgen::entity_document(&mut r),
        };
        let opts = ParseOptions {
            ns_separator: if r.one_in(3) { Some(b'|') } else { None },
            ns_triplets: r.one_in(2),
            chunk_size: [0, 1, 3, 64][r.below(4)],
            buffer_api: r.one_in(4),
            param_entity_parsing: [
                ParamEntityParsing::Never,
                ParamEntityParsing::UnlessStandalone,
                ParamEntityParsing::Always,
            ][r.below(3)],
            ..ParseOptions::default()
        };
        match sweep(&format!("gen-{i}"), &doc, &opts) {
            Ok(n) => points += n,
            Err(e) => failures.push(e),
        }
    }
    for f in failures.iter().take(20) {
        eprintln!("{f}");
    }
    eprintln!(
        "{points} failure points compared, {} differences",
        failures.len()
    );
    assert!(failures.is_empty());
}

/// External entity parsers share the root parser's memory suite: a failure
/// inside a child (its creation, its buffer, its parse) is compared too.
#[test]
fn external_entities_fail_at_every_allocation_like_c() {
    let mut r = Rng::new(0x0e7e_0f0d);
    let mut points = 0u64;
    let mut failures = Vec::new();
    for i in 0..1_500 {
        let (doc, ents) = docgen::external_case(&mut r);
        let opts = ParseOptions {
            ext_handler: if r.one_in(8) {
                ExtHandler::Accept
            } else {
                ExtHandler::Read
            },
            ext_entities: ents,
            not_standalone: [None, Some(true), Some(false)][r.below(3)],
            param_entity_parsing: [
                ParamEntityParsing::Never,
                ParamEntityParsing::UnlessStandalone,
                ParamEntityParsing::Always,
            ][r.below(3)],
            use_foreign_dtd: r.one_in(8),
            ns_separator: if r.one_in(3) { Some(b'|') } else { None },
            chunk_size: [0, 1, 3, 64][r.below(4)],
            buffer_api: r.one_in(4),
            ..ParseOptions::default()
        };
        match sweep(&format!("ext-{i}"), &doc, &opts) {
            Ok(n) => points += n,
            Err(e) => failures.push(e),
        }
    }
    for f in failures.iter().take(20) {
        eprintln!("{f}");
    }
    eprintln!(
        "{points} failure points compared, {} differences",
        failures.len()
    );
    assert!(failures.is_empty());
}
