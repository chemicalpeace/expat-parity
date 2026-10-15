//! The oracle's handler mask (`ParseOptions::handler_mask`, module 12
//! slice 4, PR 1): the C shim sets only the trace handlers in the mask,
//! and the port's trace adapter sets the same `HandlerSet`.
//!
//! A handler that is not set is not only untraced: libexpat skips work
//! done only for it (`reportProcessingInstruction` and `reportComment`
//! store nothing in `m_tempPool` without their handler; an empty element
//! updates the event pointer for its end only when both element handlers
//! are set), so the mask can change allocations and positions. These
//! tests compare C and the port under a matrix of masks:
//!
//! 1. no mask and the explicit full mask give byte-identical C output (and
//!    port output), and identical C tracker streams: the mask leaves the
//!    traces of every existing test unchanged;
//! 2. traces (every callback line, the result, error code, line, column
//!    and byte index) are equal under the mask matrix on the T2 corpus
//!    (540 upstream vectors and 600 generated documents), whole, with
//!    namespaces, and in pieces (1-byte chunks, 7-byte chunks through the
//!    buffer API, 3-byte chunks with parameter entity parsing), and with
//!    external entity parsers (which inherit the handlers);
//! 3. allocation tracker streams are equal event by event (creation,
//!    parse, `XML_ParserFree`), with default and tightened tracker limits;
//!    a sensitivity check shows the mask does change C's stream;
//! 4. bounded runs with `heap_cap` (the C side under a capped allocator,
//!    the port under the counting allocator) stay within the caps and give
//!    equal traces. The port has no allocator-refusal path (D-037:
//!    real allocation failure policy open), so this checks bounds and
//!    trace equality, not refusal parity; refusal parity is checked
//!    through the tracker limits in 3.
//!
//! The mask matrix: full, the nine MVP events of slice 4, none, each MVP
//! event alone, and eight masks drawn from a fixed seed over all 17 bits.

use std::time::Duration;

use expat_difftest::bounded::{self, Bounds, CountingAlloc};
use expat_difftest::docgen::{self, Rng};
use expat_difftest::port::{port_trace, port_trace_with_tracker_events};
use expat_difftest::trace::{first_difference, mask_q4, result_error_code};
use expat_difftest::vectors::upstream;
use expat_oracle::{
    ExtHandler, Factor, HandlerMask, Limits, ParamEntityParsing, ParseOptions, TrackerEvent,
    parse_trace, parse_trace_with_tracker_events,
};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

/// The nine events of the slice 4 MVP.
const MVP: HandlerMask = HandlerMask {
    start_element: true,
    end_element: true,
    character_data: true,
    processing_instruction: true,
    comment: true,
    start_cdata_section: true,
    end_cdata_section: true,
    start_namespace_decl: true,
    end_namespace_decl: true,
    ..HandlerMask::NONE
};

/// Each MVP event alone.
fn singles() -> Vec<(String, HandlerMask)> {
    let n = HandlerMask::NONE;
    vec![
        (
            "start_element",
            HandlerMask {
                start_element: true,
                ..n
            },
        ),
        (
            "end_element",
            HandlerMask {
                end_element: true,
                ..n
            },
        ),
        (
            "character_data",
            HandlerMask {
                character_data: true,
                ..n
            },
        ),
        (
            "processing_instruction",
            HandlerMask {
                processing_instruction: true,
                ..n
            },
        ),
        ("comment", HandlerMask { comment: true, ..n }),
        (
            "start_cdata_section",
            HandlerMask {
                start_cdata_section: true,
                ..n
            },
        ),
        (
            "end_cdata_section",
            HandlerMask {
                end_cdata_section: true,
                ..n
            },
        ),
        (
            "start_namespace_decl",
            HandlerMask {
                start_namespace_decl: true,
                ..n
            },
        ),
        (
            "end_namespace_decl",
            HandlerMask {
                end_namespace_decl: true,
                ..n
            },
        ),
    ]
    .into_iter()
    .map(|(s, m)| (format!("only {s}"), m))
    .collect()
}

/// The whole mask matrix (20 masks: full, MVP, none, 9 singles, 8 random).
fn masks() -> Vec<(String, HandlerMask)> {
    let mut v = vec![
        ("full".to_string(), HandlerMask::FULL),
        ("mvp".to_string(), MVP),
        ("none".to_string(), HandlerMask::NONE),
    ];
    v.extend(singles());
    let mut r = Rng::new(0x6d61_736b_0000_0004);
    let all = (1u64 << HandlerMask::COUNT) - 1;
    for _ in 0..8 {
        let bits = u32::try_from(r.next_u64() & all).unwrap_or(0);
        v.push((format!("random {bits:#07x}"), HandlerMask::from_bits(bits)));
    }
    v
}

/// The T2 corpus of `public_api_diff.rs`: the upstream vectors and 600
/// generated documents.
fn t2() -> Vec<(String, Vec<u8>)> {
    let mut docs: Vec<(String, Vec<u8>)> =
        upstream().iter().map(|v| (v.id.clone(), v.doc())).collect();
    for seed in [0x5eed_7e47_u64, 0x5eed_0001] {
        let mut r = Rng::new(seed);
        for i in 0..300 {
            let doc = if i % 2 == 0 {
                docgen::document(&mut r)
            } else {
                docgen::dtd_document(&mut r)
            };
            docs.push((format!("seed {seed:#x} #{i}"), doc));
        }
    }
    docs
}

/// Documents where the MVP handlers decide allocations or positions:
/// processing instructions and comments (stored in `m_tempPool` only with
/// their handler), empty elements (end event pointer), CDATA sections,
/// namespace declarations, and a pool-block boundary in a comment and a
/// processing instruction.
fn mask_sensitive_docs() -> Vec<(String, Vec<u8>)> {
    let long = "x".repeat(1100);
    let docs = [
        "<?pi data?><r/>".to_string(),
        "<r><!-- c --><?t d?><e/><![CDATA[x]]></r>".to_string(),
        "<r xmlns='urn:a' xmlns:p='urn:p'><p:e/><e a='1'/></r>".to_string(),
        format!("<r><!--{long}--><?pi {long}?></r>"),
        format!("<r><?pi {long}?><e/><!--{long}--></r junk>"),
        "<r><e/><e/><!-- unterminated".to_string(),
        "<!DOCTYPE r [<!ENTITY e '<?p d?><!--c--><x/>'>]><r>&e;</r>".to_string(),
        "<r>\r\n<?pi a\r\nb?>\r\n<!--a\r\nb-->\u{e9}</r>".to_string(),
    ];
    docs.into_iter()
        .enumerate()
        .map(|(i, d)| (format!("sensitive #{i}"), d.into_bytes()))
        .collect()
}

#[derive(Default)]
struct Tally {
    compared: usize,
    failures: Vec<String>,
}

impl Tally {
    fn check(&mut self, id: &str, mask: &str, doc: &[u8], o: &ParseOptions) {
        let c = parse_trace(doc, o).expect("oracle");
        let t = port_trace(doc, o);
        self.compared += 1;
        if let Some((line, l, r)) = first_difference(&c, &t) {
            self.failures.push(format!(
                "{id} [{mask}] chunk {} ns {:?}: line {line}: C `{l}` port `{r}`",
                o.chunk_size, o.ns_separator
            ));
        }
    }

    fn assert_clean(&self, what: &str) {
        eprintln!("{what}: {} runs compared", self.compared);
        for f in self.failures.iter().take(30) {
            eprintln!("{f}");
        }
        assert!(
            self.failures.is_empty(),
            "{what}: {} mismatches",
            self.failures.len()
        );
    }
}

fn with_mask(o: &ParseOptions, m: HandlerMask) -> ParseOptions {
    ParseOptions {
        handler_mask: Some(m),
        ..o.clone()
    }
}

/// Part 1 of the module docs: No mask and the explicit full mask: byte-identical output on both
/// sides and identical C tracker streams.
#[test]
fn no_mask_equals_explicit_full_mask() {
    let opts = [
        ParseOptions::default(),
        ParseOptions {
            ns_separator: Some(b'|'),
            ..ParseOptions::default()
        },
        ParseOptions {
            chunk_size: 7,
            buffer_api: true,
            ..ParseOptions::default()
        },
        ParseOptions {
            param_entity_parsing: ParamEntityParsing::Always,
            trace_accounting: true,
            ..ParseOptions::default()
        },
    ];
    let mut docs = t2();
    docs.extend(mask_sensitive_docs());
    let (mut n, mut streams, mut q4) = (0usize, 0usize, 0usize);
    let mut failures = Vec::new();
    for (id, doc) in &docs {
        for o in &opts {
            let full = with_mask(o, HandlerMask::FULL);
            let c_none = parse_trace(doc, o).expect("oracle");
            let c_full = parse_trace(doc, &full).expect("oracle");
            // Q-4 (`trace::mask_q4`): C's direct byte counter for one
            // token is address-dependent, so two C runs of the same input
            // can print different `accounting` lines; only that line is
            // masked, and only when the C trace shows the Q-4 signature.
            let (cn, cf) = mask_q4(&c_none, &c_full);
            if cn != cf {
                failures.push(format!("{id} {o:?}: C output differs"));
            }
            q4 += usize::from(cn != c_none);
            let p_none = port_trace(doc, o);
            let p_full = port_trace(doc, &full);
            if p_none != p_full {
                failures.push(format!("{id} {o:?}: port output differs"));
            }
            n += 1;
        }
    }
    // C tracker streams on the upstream vectors and the sensitive set.
    for (id, doc) in docs.iter().take(540).chain(&docs[docs.len() - 8..]) {
        let o = ParseOptions::default();
        let (t0, e0) = parse_trace_with_tracker_events(doc, &o).expect("oracle");
        let (t1, e1) = parse_trace_with_tracker_events(doc, &with_mask(&o, HandlerMask::FULL))
            .expect("oracle");
        if t0 != t1 || !same_c_events(&t0, &e0.events, &e1.events) || e0.at_end != e1.at_end {
            failures.push(format!("{id}: C tracker stream differs"));
        }
        streams += 1;
    }
    eprintln!(
        "none = full: {n} document/option runs ({q4} with the Q-4 accounting line masked), \
         {streams} C tracker streams"
    );
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(HandlerMask::FULL.bits(), (1 << HandlerMask::COUNT) - 1);
    for bits in [0, 1, 0x1_5555, 0xaaaa, 0x1_ffff] {
        assert_eq!(HandlerMask::from_bits(bits).bits(), bits);
    }
}

/// C tracker events compared field by field, every field included. The
/// one exception is Q-4 (`trace::mask_q4`, docs/DIVERGENCES.md): when the
/// trace ends with `XML_ERROR_JUNK_AFTER_DOC_ELEMENT` (9), an event's
/// `direct` counter of at least 2^62 on either side is the
/// address-dependent value of that quirk and can differ between two runs
/// of the same build, so `direct` alone is not compared for that event;
/// `increase`, `refused`, `size`, `total`, `peak` and `line` still are.
fn same_c_events(trace: &[u8], a: &[TrackerEvent], b: &[TrackerEvent]) -> bool {
    let q4 = result_error_code(trace) == Some(9);
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            let garbage = q4 && (x.direct >= 1 << 62 || y.direct >= 1 << 62);
            (x.increase, x.refused, x.size, x.total, x.peak, x.line)
                == (y.increase, y.refused, y.size, y.total, y.peak, y.line)
                && (garbage || x.direct == y.direct)
        })
}

/// Part 2a of the module docs: The mask matrix on T2, whole and with namespaces.
#[test]
fn mask_matrix_whole_documents_match_c() {
    let docs = t2();
    let opts = [
        ParseOptions::default(),
        ParseOptions {
            ns_separator: Some(b'|'),
            ..ParseOptions::default()
        },
    ];
    let mut t = Tally::default();
    // The matrix is not vacuous: every mask other than the full one
    // changes C's trace of some document.
    let full: Vec<Vec<u8>> = docs
        .iter()
        .map(|(_, d)| parse_trace(d, &with_mask(&opts[0], HandlerMask::FULL)).expect("oracle"))
        .collect();
    let mut unchanged = Vec::new();
    for (name, m) in masks() {
        let mut changed = 0usize;
        for (k, o) in opts.iter().enumerate() {
            let o = with_mask(o, m);
            for (i, (id, doc)) in docs.iter().enumerate() {
                t.check(id, &name, doc, &o);
                if k == 0 && parse_trace(doc, &o).expect("oracle") != full[i] {
                    changed += 1;
                }
            }
        }
        if m != HandlerMask::FULL && changed == 0 {
            unchanged.push(name);
        }
    }
    t.assert_clean("mask matrix, whole");
    assert!(
        unchanged.is_empty(),
        "masks that change no C trace: {unchanged:?}"
    );
}

/// Part 2b of the module docs: The mask matrix on T2 in pieces.
#[test]
fn mask_matrix_pieces_match_c() {
    let docs = t2();
    let opts = [
        ParseOptions {
            chunk_size: 1,
            ns_separator: Some(b'|'),
            ..ParseOptions::default()
        },
        ParseOptions {
            chunk_size: 7,
            buffer_api: true,
            ..ParseOptions::default()
        },
        ParseOptions {
            chunk_size: 3,
            param_entity_parsing: ParamEntityParsing::Always,
            no_reparse_deferral: true,
            ..ParseOptions::default()
        },
    ];
    let mut t = Tally::default();
    for (name, m) in masks() {
        for o in &opts {
            let o = with_mask(o, m);
            for (id, doc) in &docs {
                t.check(id, &name, doc, &o);
            }
        }
    }
    t.assert_clean("mask matrix, pieces");
}

/// Part 2c of the module docs: The mask-sensitive documents and external entity parsers (which
/// inherit the handlers of their parent) under the mask matrix.
#[test]
fn mask_matrix_sensitive_and_external_match_c() {
    let mut t = Tally::default();
    let base = ParseOptions {
        param_entity_parsing: ParamEntityParsing::Always,
        ..ParseOptions::default()
    };
    let mut r = Rng::new(0x6d61_736b_6578_7401);
    let externals: Vec<_> = (0..100).map(|_| docgen::external_case(&mut r)).collect();
    for (name, m) in masks() {
        for (id, doc) in mask_sensitive_docs() {
            for chunk in [0, 1, 5] {
                for ns in [None, Some(b'|')] {
                    let o = ParseOptions {
                        chunk_size: chunk,
                        ns_separator: ns,
                        ..with_mask(&base, m)
                    };
                    t.check(&id, &name, &doc, &o);
                }
            }
        }
        for (i, (doc, ents)) in externals.iter().enumerate() {
            for chunk in [0, 3] {
                let o = ParseOptions {
                    ext_handler: ExtHandler::Read,
                    ext_entities: ents.clone(),
                    chunk_size: chunk,
                    ..with_mask(&base, m)
                };
                t.check(&format!("external #{i}"), &name, doc, &o);
            }
        }
    }
    t.assert_clean("mask matrix, sensitive and external");
}

fn first_unequal(
    c: &[TrackerEvent],
    port: &[expat::internal::parse::TrackerEvent],
) -> Option<usize> {
    let n = c.len().max(port.len());
    (0..n).find(|&i| match (c.get(i), port.get(i)) {
        (Some(a), Some(b)) => {
            (a.increase, a.refused, a.size, a.total, a.peak)
                != (b.increase, b.refused, b.size, b.total, b.peak)
        }
        _ => true,
    })
}

/// Part 3 of the module docs: Tracker streams event by event under the mask matrix, with default
/// and tightened tracker limits (activation threshold 0: every allocation
/// is checked, so a refusal point depends on what the mask stores).
#[test]
fn mask_matrix_tracker_streams_match_c() {
    let mut docs: Vec<(String, Vec<u8>)> =
        upstream().iter().map(|v| (v.id.clone(), v.doc())).collect();
    let mut r = Rng::new(0x6d61_736b_7472_6b01);
    for i in 0..120 {
        docs.push((
            format!("gen #{i}"),
            match i % 3 {
                0 => docgen::document(&mut r),
                1 => docgen::dtd_document(&mut r),
                _ => docgen::entity_document(&mut r),
            },
        ));
    }
    docs.extend(mask_sensitive_docs());
    let base = ParseOptions {
        param_entity_parsing: ParamEntityParsing::Always,
        ..ParseOptions::default()
    };
    let mut opts = vec![base.clone()];
    for f in [8.0f32, 45.0] {
        opts.push(ParseOptions {
            limits: Limits {
                at_amplification: Some(Factor(f)),
                at_threshold: Some(0),
                ..Limits::NONE
            },
            ..base.clone()
        });
    }
    let (mut runs, mut refusals) = (0usize, 0usize);
    let mut failures = Vec::new();
    for (name, m) in masks() {
        for o in &opts {
            let o = with_mask(o, m);
            for (id, doc) in &docs {
                let (ct, c) = parse_trace_with_tracker_events(doc, &o).expect("oracle");
                let (pt, p, p_at_end) = port_trace_with_tracker_events(doc, &o);
                runs += 1;
                refusals += usize::from(c.events.iter().any(|e| e.refused));
                if let Some((line, l, rr)) = first_difference(&ct, &pt) {
                    failures.push(format!(
                        "{id} [{name}] {:?}: trace line {line}: C `{l}` port `{rr}`",
                        o.limits
                    ));
                }
                if c.at_end != p_at_end {
                    failures.push(format!(
                        "{id} [{name}]: {} events to the end in C, {p_at_end} in the port",
                        c.at_end
                    ));
                }
                if let Some(k) = first_unequal(&c.events, &p) {
                    failures.push(format!(
                        "{id} [{name}] {:?}: streams differ at event {k}",
                        o.limits
                    ));
                }
            }
        }
    }
    eprintln!("tracker streams: {runs} runs, {refusals} with a refusal in C");
    for f in failures.iter().take(30) {
        eprintln!("{f}");
    }
    assert!(failures.is_empty(), "{} mismatches", failures.len());
    assert!(refusals > 0, "the tightened limits refuse somewhere");
}

/// Part 3 of the module docs (sensitivity): The mask changes C's tracker stream for a document
/// with a processing instruction and a comment, and the port follows.
#[test]
fn mask_changes_allocations_in_c_and_port() {
    let doc = format!("<r><!--{0}--><?pi {0}?></r>", "x".repeat(1100)).into_bytes();
    let full = with_mask(&ParseOptions::default(), HandlerMask::FULL);
    let none = with_mask(&ParseOptions::default(), HandlerMask::NONE);
    let (_, cf) = parse_trace_with_tracker_events(&doc, &full).expect("oracle");
    let (_, cn) = parse_trace_with_tracker_events(&doc, &none).expect("oracle");
    let (_, pf, _) = port_trace_with_tracker_events(&doc, &full);
    let (_, pn, _) = port_trace_with_tracker_events(&doc, &none);
    assert_ne!(
        cf.events, cn.events,
        "C: the mask changes the tracker stream"
    );
    assert!(
        first_unequal(&cf.events, &pf).is_none(),
        "full mask: streams equal"
    );
    assert!(
        first_unequal(&cn.events, &pn).is_none(),
        "no handlers: streams equal"
    );
    let peak = |e: &[TrackerEvent]| e.iter().map(|e| e.peak).max().unwrap_or(0);
    assert!(
        peak(&cf.events) > peak(&cn.events),
        "storing the PI and comment raises the peak"
    );
}

/// Part 4 of the module docs: Bounded runs with `heap_cap` under the mask matrix.
#[test]
fn mask_matrix_bounded_heap_cap() {
    const BOUNDS: Bounds = Bounds {
        max_input: 64 << 10,
        timeout: Duration::from_secs(60),
        port_heap_cap: 64 << 20,
        c_heap_cap: 16 << 20,
    };
    let big = format!(
        "<r>{}</r>",
        format!(
            "<!--{0}--><?pi {0}?><e a='1'/><![CDATA[{0}]]>",
            "y".repeat(2000)
        )
        .repeat(4)
    );
    let mut docs = mask_sensitive_docs();
    docs.push(("big".to_string(), big.into_bytes()));
    let mut c_peaks = std::collections::BTreeMap::new();
    let mut failures = Vec::new();
    let mut runs = 0usize;
    for (name, m) in masks() {
        for (id, doc) in &docs {
            for chunk in [0, 7] {
                let o = ParseOptions {
                    chunk_size: chunk,
                    ..with_mask(&ParseOptions::default(), m)
                };
                let out = bounded::compare(&format!("{id}-{name}"), doc, &o, BOUNDS);
                runs += 1;
                if !out.equal() {
                    failures.push(format!("{id} [{name}] chunk {chunk}"));
                }
                if id == "big" && chunk == 0 {
                    c_peaks.insert(name.clone(), out.c_peak);
                }
            }
        }
    }
    eprintln!(
        "bounded: {runs} runs; C peak live heap on `big`: full {:?}, none {:?}",
        c_peaks.get("full"),
        c_peaks.get("none")
    );
    assert!(failures.is_empty(), "{failures:?}");
    assert!(
        c_peaks.get("full") > c_peaks.get("none"),
        "the full mask needs more of C's heap than no handlers"
    );
}
