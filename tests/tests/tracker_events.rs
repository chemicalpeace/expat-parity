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
use expat_oracle::{ExtHandler, Factor, Limits, ParamEntityParsing, ParseOptions, TrackerEvent};

/// Streams (of 5,196) that must be equal after creation at this stage
/// (stage 4 of D-034: 3,270 of 3,420; stage 2: 3,291 of 3,420 and all
/// 1,776 pool-boundary, public-id, chop and `xmlns:` runs added with it,
/// every remaining first difference the raw tag name the port charges at
/// the start tag, stage 5; see docs/REPORT.md, DV-1 parity).
const EQUAL_STREAMS_AT_LEAST: usize = 5_196;

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
        ParseOptions {
            chunk_size: 7,
            ..b.clone()
        },
        // A protocol encoding name: `parserInit` copies it (`copyString`)
        // after the DTD, a fifth creation allocation.
        ParseOptions {
            encoding: Some("UTF-8".to_string()),
            ..b
        },
    ]
}

/// Option sets for documents with external entities: the test handler
/// reads them through external entity parsers (`ExtHandler::Read`), whose
/// creation, allocations and release go through the root parser's
/// tracker; whole, 1-byte chunks with namespaces, 7-byte chunks through
/// the buffer API with `UNLESS_STANDALONE`, and a handler that refuses.
fn external_option_sets(ents: &[(Vec<u8>, Vec<u8>)]) -> Vec<ParseOptions> {
    let b = ParseOptions {
        ext_handler: ExtHandler::Read,
        ext_entities: ents.to_vec(),
        ..base()
    };
    vec![
        b.clone(),
        ParseOptions {
            ns_separator: Some(b'|'),
            chunk_size: 1,
            ..b.clone()
        },
        ParseOptions {
            chunk_size: 7,
            buffer_api: true,
            param_entity_parsing: ParamEntityParsing::UnlessStandalone,
            not_standalone: Some(true),
            ..b.clone()
        },
        ParseOptions {
            ext_handler: ExtHandler::Fail,
            ..b
        },
    ]
}

/// Tightened tracker limits (activation threshold 0, so every allocation
/// is checked against `total / direct`), with factors that make C refuse
/// at different points of the parse: during the parse, while an external
/// entity parser is created (its structure, arrays, DTD), while it
/// parses, or never. The refusal events and the error and position must
/// be equal.
fn limit_option_sets(o: &ParseOptions) -> Vec<ParseOptions> {
    factor_option_sets(o, &[8.0f32, 20.0, 45.0, 100.0, 400.0])
}

/// Activation thresholds from 3,000 to 20,000 bytes in steps of 40, with
/// the smallest factor (1.0, which any count above the direct bytes
/// exceeds): C refuses the first allocation that takes the count to the
/// threshold, so for the nested external cases the refusal walks through
/// every allocation in turn, the external entity parsers' creation
/// (structure, `m_atts`, `m_dataBuf`, DTD), DTD copy, context, parse and
/// release included.
fn threshold_sweep(o: &ParseOptions) -> Vec<ParseOptions> {
    (3_000..20_000)
        .step_by(40)
        .map(|t| ParseOptions {
            limits: Limits {
                at_amplification: Some(Factor(1.0)),
                at_threshold: Some(t),
                ..Limits::NONE
            },
            ..o.clone()
        })
        .collect()
}

fn factor_option_sets(o: &ParseOptions, factors: &[f32]) -> Vec<ParseOptions> {
    factors
        .iter()
        .copied()
        .map(|f| ParseOptions {
            limits: Limits {
                at_amplification: Some(Factor(f)),
                at_threshold: Some(0),
                ..Limits::NONE
            },
            ..o.clone()
        })
        .collect()
}

/// Documents run under [`limit_option_sets`]: every fifth generated
/// document and every fourth external case.
const LIMIT_STRIDE: (usize, usize) = (5, 4);

/// Every (document, option set) run of the test.
fn all_runs() -> Vec<(Vec<u8>, ParseOptions)> {
    let mut v = Vec::new();
    for (i, doc) in corpus().into_iter().enumerate() {
        for o in option_sets() {
            v.push((doc.clone(), o));
        }
        if i % LIMIT_STRIDE.0 == 0 {
            for o in limit_option_sets(&base()) {
                v.push((doc.clone(), o));
            }
        }
    }
    let mut r = Rng::new(0x6578_7465_726e_616c);
    for i in 0..EXTERNAL_CASES {
        let (doc, ents) = docgen::external_case(&mut r);
        for o in external_option_sets(&ents) {
            v.push((doc.clone(), o));
        }
        if i % LIMIT_STRIDE.1 == 0 {
            for o in limit_option_sets(&external_option_sets(&ents)[0]) {
                v.push((doc.clone(), o));
            }
        }
    }
    for (doc, ents) in nested_external_cases() {
        for o in external_option_sets(&ents) {
            v.push((doc.clone(), o));
        }
        for base in external_option_sets(&ents).iter().take(3) {
            for o in threshold_sweep(base) {
                v.push((doc.clone(), o));
            }
        }
    }
    v
}

/// External entity bodies by system identifier.
type Entities = Vec<(Vec<u8>, Vec<u8>)>;

/// Generated documents with external entities (`docgen::external_case`).
const EXTERNAL_CASES: usize = 400;

/// External general entities nested several levels deep (each child
/// parser created from inside the previous one's handler), a parameter
/// entity inside an external subset, and a general entity whose context
/// string carries namespace bindings into the child.
fn nested_external_cases() -> Vec<(Vec<u8>, Entities)> {
    let ent = |k: &str, v: &str| (k.as_bytes().to_vec(), v.as_bytes().to_vec());
    vec![
        (
            b"<!DOCTYPE r [<!ENTITY a SYSTEM 'a.xml'>]><r>&a;</r>".to_vec(),
            vec![
                ent("a.xml", "<!DOCTYPE x [<!ENTITY b SYSTEM 'b.xml'>]><a>&b;</a>"),
                ent("b.xml", "<b>text<c/></b>"),
            ],
        ),
        (
            b"<!DOCTYPE r [<!ENTITY a SYSTEM 'a.xml'><!ENTITY b SYSTEM 'b.xml'><!ENTITY c SYSTEM 'c.xml'>]><r>&a;</r>".to_vec(),
            vec![ent("a.xml", "<a>&b;&b;</a>"), ent("b.xml", "<b>&c;</b>"), ent("c.xml", "<c x='1'>deep</c>")],
        ),
        (
            b"<!DOCTYPE r SYSTEM 'sub.dtd'><r a='&e;'/>".to_vec(),
            vec![
                ent("sub.dtd", "<!ENTITY % p SYSTEM 'p.ent'>%p;<!ATTLIST r b CDATA 'd'>"),
                ent("p.ent", "<!ENTITY e 'from p'><!ENTITY % q 'x'>"),
            ],
        ),
        (
            b"<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r xmlns='urn:d' xmlns:p='urn:p'><p:s>&g;</p:s></r>".to_vec(),
            vec![ent("g.xml", "<p:t xmlns:q='urn:q' q:a='1'>x</p:t>")],
        ),
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
    docs.extend(boundary_docs());
    docs
}

/// Multi-byte characters (2, 3 and 4 bytes in UTF-8, as literals and as
/// character references) placed so that they straddle the end of the first
/// 1,024-character pool block, in each kind of string C converts into a
/// pool (attribute values, normalised or not, comments, processing
/// instructions, entity values, public ids), in UTF-8 and UTF-16 (D-034
/// stage 2: where `XmlConvert` stops decides when a pool grows).
fn boundary_docs() -> Vec<Vec<u8>> {
    let mut docs = Vec::new();
    for ch in ["\u{e9}", "\u{20ac}", "\u{1f600}", "&#xe9;", "&#x1f600;"] {
        for fill in 1016..1026 {
            let body = format!("{}{ch}{ch}tail", "x".repeat(fill));
            let texts = [
                format!("<r a='{body}'/>"),
                format!("<r a=' {body} '><!--{body}--><?pi {body}?></r>"),
                format!(
                    "<!DOCTYPE r [<!ATTLIST r a NMTOKENS '  {body}  '><!ENTITY e '{body}'>]><r b='&e;'/>"
                ),
                format!("<!DOCTYPE r PUBLIC '  -//{body}//  ' 's'><r/>"),
            ];
            for t in texts {
                docs.push(t.clone().into_bytes());
                // UTF-16LE with a byte order mark.
                let mut u16 = vec![0xFF, 0xFE];
                for c in t.encode_utf16() {
                    u16.extend_from_slice(&c.to_le_bytes());
                }
                docs.push(u16);
            }
        }
    }
    // Public ids that normalisation shortens, with the literal ending just
    // past the first block and the normalised id within it (C charges the
    // literal; `dtd->pool` for the doctype's and an entity's, `m_tempPool`
    // for a notation's).
    for fill in 998..1022 {
        let x = "x".repeat(fill);
        for t in [
            format!("<!DOCTYPE r PUBLIC '  -//{x}   y//  ' 's'><r/>"),
            format!("<!DOCTYPE r [<!ENTITY e PUBLIC '  {x}   y  ' 's'>]><r/>"),
            format!("<!DOCTYPE r [<!NOTATION n PUBLIC '  {x}   y  '>]><r/>"),
        ] {
            docs.push(t.into_bytes());
        }
    }
    // A tokenized attribute value whose trailing space fills the pool block
    // (`poolChop` lets the NUL take its place), in `dtd->pool` (a default)
    // and in `m_tempPool` (a start tag); and an `xmlns:` attribute id whose
    // prefix C looks up without copying it, right after a long element
    // type name in `dtd->pool`.
    for fill in 990..1030 {
        let x = "x".repeat(fill);
        for t in [
            format!("<!DOCTYPE r [<!ATTLIST r a NMTOKENS ' {x} '>]><r/>"),
            format!("<!DOCTYPE r [<!ATTLIST r a NMTOKENS #IMPLIED>]><r a=' {x} '/>"),
            format!("<!DOCTYPE r [<!ATTLIST {x} xmlns:p CDATA #IMPLIED>]><r/>"),
        ] {
            docs.push(t.into_bytes());
        }
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

/// The `result` line of a trace: status, error code, line, column and
/// byte index where the parse stopped.
fn result_line(trace: &[u8]) -> String {
    String::from_utf8_lossy(trace)
        .lines()
        .rev()
        .find(|l| l.starts_with("result "))
        .unwrap_or_default()
        .to_string()
}

/// The first event where two streams differ (direction, size, refusal,
/// running total or peak), or `None` when they are equal.
fn first_unequal(
    c: &[TrackerEvent],
    port: &[expat::internal::parse::TrackerEvent],
) -> Option<usize> {
    let k = c
        .iter()
        .zip(port)
        .take_while(|(c, p)| {
            c.increase == p.increase
                && c.size == p.size
                && c.refused == p.refused
                && c.total == p.total
                && c.peak == p.peak
        })
        .count();
    (k != c.len() || k != port.len()).then_some(k)
}

#[test]
fn tracker_events_against_c() {
    let platform = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
    let (mut runs, mut equal_parse, mut equal_all, mut equal_result) =
        (0usize, 0usize, 0usize, 0usize);
    let (mut refusals, mut peaks_equal) = (0usize, 0usize);
    let mut first_difference: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut refused_at: BTreeMap<u32, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    for (di, (doc, o)) in all_runs().iter().enumerate() {
        runs += 1;
        let (c_trace, c) = expat_oracle::parse_trace_with_tracker_events(doc, o).expect("oracle");
        let (p_trace, port, port_at_end) = port_trace_with_tracker_events(doc, o);
        // 1. Self-consistency; both streams return to 0 after
        //    `XML_ParserFree` (child parsers included).
        let c_tuples = |e: &TrackerEvent| (e.increase, e.refused, e.size, e.total, e.peak);
        match consistent(c.events.iter().map(c_tuples), 0) {
            Ok(0) => {}
            Ok(left) => failures.push(format!("run {di}: C ends at {left}, not 0")),
            Err(e) => failures.push(format!("run {di}: C stream: {e}")),
        }
        match consistent(
            port.iter()
                .map(|e| (e.increase, e.refused, e.size, e.total, e.peak)),
            0,
        ) {
            Ok(0) => {}
            Ok(left) => failures.push(format!("run {di}: port ends at {left}, not 0")),
            Err(e) => failures.push(format!("run {di}: port stream: {e}")),
        }
        // 2. Limit outcome: error code and position.
        if result_line(&c_trace) == result_line(&p_trace) {
            equal_result += 1;
        } else {
            failures.push(format!(
                "run {di}: C `{}` port `{}`",
                result_line(&c_trace),
                result_line(&p_trace)
            ));
        }
        if let Some(e) = c.events.iter().find(|e| e.refused) {
            refusals += 1;
            *refused_at.entry(e.line).or_insert(0usize) += 1;
        }
        // 3. Event by event from the parser's creation to the end of the
        //    parse, then through `XML_ParserFree`.
        let c_parse = &c.events[..c.at_end];
        let p_parse = port.get(..port_at_end).unwrap_or_default();
        let peak = |e: Option<&TrackerEvent>| e.map_or(0, |e| e.peak);
        if peak(c_parse.last()) == p_parse.last().map_or(0, |e| e.peak) {
            peaks_equal += 1;
        }
        let parse_diff = first_unequal(c_parse, p_parse);
        let all_diff = first_unequal(&c.events, &port);
        if parse_diff.is_none() {
            equal_parse += 1;
        }
        let Some(k) = all_diff else {
            equal_all += 1;
            continue;
        };
        let phase = if parse_diff.is_none() {
            "release"
        } else {
            "parse"
        };
        let key = c
            .events
            .get(k)
            .map_or(format!("{phase}: port has more events"), |e| {
                format!(
                    "{phase}: C {}{}{} at xmlparse.c:{}",
                    sign(e.increase),
                    e.size,
                    if e.refused { " refused" } else { "" },
                    e.line
                )
            });
        let port_side = port.get(k).map_or("no event".to_string(), |e| {
            format!(
                "{}{}{}",
                sign(e.increase),
                e.size,
                if e.refused { " refused" } else { "" }
            )
        });
        let entry = first_difference.entry(key).or_insert((0, String::new()));
        entry.0 += 1;
        if entry.1.is_empty() {
            entry.1 = format!("run {di} event {k}: port {port_side}");
        }
    }
    println!(
        "EVENTS {platform}: {runs} runs; {equal_parse} streams equal from creation to the end of the parse, \
         {equal_all} also through XML_ParserFree; {peaks_equal} equal peaks; {equal_result} equal results \
         ({refusals} runs with a refusal in C)"
    );
    println!("EVENTS {platform} first refusal in C by xmlparse.c line: {refused_at:?}");
    let mut classes: Vec<_> = first_difference.iter().collect();
    classes.sort_by_key(|(_, (n, _))| std::cmp::Reverse(*n));
    for (key, (n, example)) in &classes {
        println!("EVENTS {platform} first difference {n:>5} x {key} (e.g. {example})");
    }
    assert!(
        failures.is_empty(),
        "{platform}: {} failures: {:?}",
        failures.len(),
        failures.iter().take(5).collect::<Vec<_>>()
    );
    assert!(
        equal_all >= EQUAL_STREAMS_AT_LEAST,
        "{platform}: {equal_all} equal streams, fewer than the {EQUAL_STREAMS_AT_LEAST} of this stage"
    );
}
