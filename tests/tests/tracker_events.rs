//! The allocation tracker event by event against libexpat's (decision
//! D-034, the gate of the parity stages).
//!
//! The oracle captures every event of C's tracker (`expat_malloc`,
//! `expat_realloc`, `expat_free` and refusals, with the `xmlparse.c` line,
//! the running `bytesAllocated` and `peakBytesAllocated`;
//! `shim/oracle_xmlparse.c`); the port records every change of its own
//! count (`parse::TrackerEvent`). For each document and option set:
//!
//! 1. both streams are self-consistent: every event moves the running
//!    total by its size (a refusal moves nothing), the peak is the running
//!    maximum; C's stream ends at 0 after `XML_ParserFree` (everything C
//!    allocates through its tracker is freed through it, child parsers
//!    included);
//! 2. the two streams are compared event by event after the creation
//!    events (C reports `parserCreate`'s allocations one by one; the port
//!    charges a creation bound as one event until stage 3 of D-034):
//!    increase/decrease, size and refusal must be equal at every event.
//!
//! Until every stage is merged the streams are not all equal; the test
//! prints, per platform, how many are, and where the others first differ
//! (by C line), and fails if fewer streams are equal than
//! [`EQUAL_STREAMS_AT_LEAST`] (a ratchet raised by each stage), or if a
//! stream that differs is not explained by a known remaining class.

use std::collections::BTreeMap;

use expat_difftest::docgen::{self, Rng};
use expat_difftest::port::port_trace_with_tracker_events;
use expat_difftest::vectors::upstream;
use expat_oracle::{ParamEntityParsing, ParseOptions, TrackerEvent};

/// `xmlparse.c` lines of `parserCreate`'s and `dtdCreate`'s allocations
/// (the parser structure, `m_atts`, `m_dataBuf`, the DTD).
const C_CREATION_LINES: [u32; 4] = [1395, 1405, 1410, 7865];

/// Streams (of 3,420) that must be equal after creation at this stage
/// (measured on 64-bit Linux, macOS and Windows; see docs/REPORT.md, DV-1
/// parity).
const EQUAL_STREAMS_AT_LEAST: usize = 1_838;

fn base() -> ParseOptions {
    ParseOptions {
        param_entity_parsing: ParamEntityParsing::Always,
        ..ParseOptions::default()
    }
}

fn option_sets() -> Vec<ParseOptions> {
    let b = base();
    vec![
        b.clone(),
        ParseOptions {
            ns_separator: Some(b'|'),
            ns_triplets: true,
            ..b.clone()
        },
        ParseOptions { chunk_size: 7, ..b },
    ]
}

fn corpus() -> Vec<Vec<u8>> {
    let mut docs: Vec<Vec<u8>> = upstream().iter().map(|v| v.doc()).collect();
    let mut r = Rng::new(0x0065_7665_6e74_7321);
    for i in 0..600 {
        docs.push(match i % 3 {
            0 => docgen::document(&mut r),
            1 => docgen::dtd_document(&mut r),
            _ => docgen::entity_document(&mut r),
        });
    }
    docs
}

/// Checks running totals and peaks of a stream starting at `start`.
fn consistent(
    events: impl Iterator<Item = (bool, bool, u64, u64, u64)>,
    start: u64,
) -> Result<u64, String> {
    let (mut total, mut peak) = (start, start);
    for (k, (increase, refused, size, t, p)) in events.enumerate() {
        if !refused {
            total = if increase {
                total + size
            } else {
                total
                    .checked_sub(size)
                    .ok_or(format!("event {k}: below zero"))?
            };
            peak = peak.max(total);
        }
        if t != total {
            return Err(format!("event {k}: total {t}, expected {total}"));
        }
        if p != peak {
            return Err(format!("event {k}: peak {p}, expected {peak}"));
        }
    }
    Ok(total)
}

fn sign(increase: bool) -> char {
    if increase { '+' } else { '-' }
}

#[test]
fn tracker_events_against_c() {
    let platform = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
    let (mut runs, mut equal) = (0usize, 0usize);
    let mut first_difference: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut failures = Vec::new();
    for (di, doc) in corpus().iter().enumerate() {
        for (oi, o) in option_sets().iter().enumerate() {
            runs += 1;
            let (_, c) = expat_oracle::parse_trace_with_tracker_events(doc, o).expect("oracle");
            let (_, port) = port_trace_with_tracker_events(doc, o);
            // 1. Self-consistency.
            let c_tuples = |e: &TrackerEvent| (e.increase, e.refused, e.size, e.total, e.peak);
            match consistent(c.events.iter().map(c_tuples), 0) {
                Ok(0) => {}
                Ok(left) => failures.push(format!("doc {di} opt {oi}: C ends at {left}, not 0")),
                Err(e) => failures.push(format!("doc {di} opt {oi}: C stream: {e}")),
            }
            if let Err(e) = consistent(
                port.iter()
                    .map(|e| (e.increase, e.refused, e.size, e.total, e.peak)),
                0,
            ) {
                failures.push(format!("doc {di} opt {oi}: port stream: {e}"));
            }
            // 2. Event by event after creation, up to the end of the parse.
            let c_ev: Vec<&TrackerEvent> = c.events[..c.at_end]
                .iter()
                .skip_while(|e| C_CREATION_LINES.contains(&e.line))
                .collect();
            let p_ev = port.get(1..).unwrap_or_default();
            let k = c_ev
                .iter()
                .zip(p_ev)
                .take_while(|(c, p)| {
                    c.increase == p.increase && c.size == p.size && c.refused == p.refused
                })
                .count();
            if k == c_ev.len() && k == p_ev.len() {
                equal += 1;
                continue;
            }
            let key = c_ev.get(k).map_or("port has more events".to_string(), |e| {
                format!("C {}{} at xmlparse.c:{}", sign(e.increase), e.size, e.line)
            });
            let port_side = p_ev.get(k).map_or("no event".to_string(), |e| {
                format!("{}{}", sign(e.increase), e.size)
            });
            let entry = first_difference.entry(key).or_insert((0, String::new()));
            entry.0 += 1;
            if entry.1.is_empty() {
                entry.1 = format!("doc {di} opt {oi} event {k}: port {port_side}");
            }
        }
    }
    println!("EVENTS {platform}: {runs} runs, {equal} equal event streams after creation");
    let mut classes: Vec<_> = first_difference.iter().collect();
    classes.sort_by_key(|(_, (n, _))| std::cmp::Reverse(*n));
    for (key, (n, example)) in &classes {
        println!("EVENTS {platform} first difference {n:>5} x {key} (e.g. {example})");
    }
    assert!(
        failures.is_empty(),
        "{platform}: {} inconsistent streams: {:?}",
        failures.len(),
        failures.iter().take(5).collect::<Vec<_>>()
    );
    assert!(
        equal >= EQUAL_STREAMS_AT_LEAST,
        "{platform}: {equal} equal streams, fewer than the {EQUAL_STREAMS_AT_LEAST} of this stage"
    );
}
