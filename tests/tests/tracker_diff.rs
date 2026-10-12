//! The allocation tracker (`XML_SetAllocTrackerMaximumAmplification`,
//! `XML_SetAllocTrackerActivationThreshold`) against the C oracle
//! (module 11c, decision D-030).
//!
//! The port does not allocate libexpat's data structures, so it cannot
//! have libexpat's byte counts; it charges at least as much, no later
//! (docs/DTD-SECURITY.md section 5). These tests check the relation that
//! matters for security: **whenever libexpat stops with
//! `XML_ERROR_NO_MEMORY` because of its tracker, the port stops with
//! `XML_ERROR_NO_MEMORY` too** (`expat_difftest::tracker`), and at the
//! default limits the port never refuses a document libexpat accepts.
//!
//! Groups (C behaviour, policy and divergences in docs/REPORT.md):
//! setters, creation charge, corpus relation (upstream vectors under four
//! option sets), generated documents, threshold and amplification
//! boundary sweeps, large expansions (the ~340 KB attribute-expansion
//! document that only the tracker stops), and the content-expansion
//! control. Large documents run through `expat_difftest::bounded` (input,
//! wall-clock and heap bounds).

use std::time::Duration;

use expat_difftest::bounded::{Bounds, CountingAlloc, compare};
use expat_difftest::docgen::{self, Rng};
use expat_difftest::trace::{first_difference, result_error_code};
use expat_difftest::tracker::{self, NO_MEMORY, Relation, Tally};
use expat_difftest::vectors::upstream;
use expat_oracle::{Factor, Limits, ParamEntityParsing, ParseOptions};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

/// `XML_ERROR_AMPLIFICATION_LIMIT_BREACH`.
const BREACH: u32 = 43;

/// Bounds of the large-expansion group: 512 KiB input (the document that
/// only the tracker stops needs ~340 KB of direct input, see
/// [`tracker_only_attribute`]), 60 s, 384 MiB heap per side.
const LARGE: Bounds = Bounds {
    max_input: 512 << 10,
    timeout: Duration::from_secs(60),
    port_heap_cap: 384 << 20,
    c_heap_cap: 384 << 20,
};

fn base() -> ParseOptions {
    ParseOptions {
        param_entity_parsing: ParamEntityParsing::Always,
        ..ParseOptions::default()
    }
}

fn at(o: &ParseOptions, amp: Option<f32>, thr: Option<u64>) -> ParseOptions {
    ParseOptions {
        limits: Limits {
            at_amplification: amp.map(Factor),
            at_threshold: thr,
            ..o.limits
        },
        ..o.clone()
    }
}

/// The option sets of the corpus groups.
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
        ParseOptions {
            chunk_size: 64,
            buffer_api: true,
            ns_separator: Some(b'|'),
            ..b
        },
    ]
}

fn finish(what: &str, t: &Tally, failures: &[String]) {
    eprintln!(
        "{what}: {} runs, C no-memory {}, port stricter {}, min end margin {:?}",
        t.runs, t.c_no_memory, t.port_stricter, t.min_margin
    );
    assert!(
        failures.is_empty(),
        "{what}: {} failures, first: {}",
        failures.len(),
        failures.first().map_or("", String::as_str)
    );
}

#[test]
fn setters_match_c() {
    let doc = b"<r a='1'>text</r>";
    let cases: [(Option<f32>, Option<u64>); 9] = [
        (Some(f32::NAN), None),
        (Some(0.0), None),
        (Some(0.999_999), None),
        (Some(1.0), None),
        (Some(f32::INFINITY), None),
        (Some(-1.0), None),
        (None, Some(0)),
        (None, Some(u64::MAX)),
        (Some(1.0), Some(0)),
    ];
    for (amp, thr) in cases {
        let o = at(&base(), amp, thr);
        let r = tracker::run(doc, &o).unwrap();
        let port = r.port.clone();
        // The setters' results are trace lines (`limit at-...`), so equal
        // traces mean equal acceptance; `(1.0, 0)` makes both fail at the
        // first allocation of the parse (nothing read directly yet).
        assert_eq!(first_difference(&r.c, &port), None, "{amp:?} {thr:?}");
        if (amp, thr) == (Some(1.0), Some(0)) {
            assert_eq!(result_error_code(&r.c), Some(NO_MEMORY));
        }
    }
}

#[test]
fn creation_charge_covers_c() {
    for enc in [None, Some("UTF-8"), Some("ISO-8859-1"), Some("UTF-16LE")] {
        for ns in [None, Some(b'|')] {
            let o = ParseOptions {
                encoding: enc.map(str::to_string),
                ns_separator: ns,
                ..base()
            };
            let r = tracker::run(b"<r/>", &o).unwrap();
            assert!(
                r.port_tracker.created >= r.c_tracker.created,
                "{enc:?} {ns:?}: port {} < C {}",
                r.port_tracker.created,
                r.c_tracker.created
            );
            eprintln!(
                "creation {enc:?} ns={}: C {} port {}",
                ns.is_some(),
                r.c_tracker.created,
                r.port_tracker.created
            );
        }
    }
}

#[test]
fn upstream_vectors_tracker_relation() {
    let mut t = Tally::default();
    let mut failures = Vec::new();
    for v in upstream() {
        let doc = v.doc();
        for o in option_sets() {
            if let Err(e) = tracker::check(&doc, &o, &mut t) {
                failures.push(format!("{}: {e}", v.id));
            }
        }
    }
    finish("upstream vectors", &t, &failures);
    assert!(
        t.c_no_memory > 1000,
        "the tight runs must make C fail often"
    );
}

#[test]
fn generated_documents_tracker_relation() {
    let mut t = Tally::default();
    let mut failures = Vec::new();
    let mut r = Rng::new(0x0074_7261_636b_6572);
    let sets = option_sets();
    for i in 0..1500 {
        let doc = match i % 3 {
            0 => docgen::document(&mut r),
            1 => docgen::dtd_document(&mut r),
            _ => docgen::entity_document(&mut r),
        };
        let o = sets.get(i % sets.len()).cloned().unwrap_or_else(base);
        if let Err(e) = tracker::check(&doc, &o, &mut t) {
            failures.push(format!("generated {i}: {e}"));
        }
    }
    finish("generated documents", &t, &failures);
}

/// Documents for the boundary sweeps: each exercises one charged
/// structure.
fn sweep_documents() -> Vec<(&'static str, Vec<u8>)> {
    let mut v: Vec<(&'static str, Vec<u8>)> = Vec::new();
    let mut deep = Vec::new();
    for i in 0..300 {
        deep.extend_from_slice(format!("<n{} a='{i}'>", i % 7).as_bytes());
    }
    for i in (0..300).rev() {
        deep.extend_from_slice(format!("</n{}>", i % 7).as_bytes());
    }
    v.push(("deep nesting", deep));
    let mut long = b"<r>".to_vec();
    for i in 0..40 {
        long.extend_from_slice(format!("<{} x='1'/>", "q".repeat(50 + 37 * i)).as_bytes());
    }
    long.extend_from_slice(b"</r>");
    v.push(("long names", long));
    let mut ns = b"<r xmlns='urn:default'>".to_vec();
    for i in 0..60 {
        ns.extend_from_slice(
            format!(
                "<p{i}:e xmlns:p{i}='urn:{}' p{i}:a='1' p{i}:b='2'/>",
                "u".repeat(i * 11)
            )
            .as_bytes(),
        );
    }
    ns.extend_from_slice(b"</r>");
    v.push(("namespaces", ns));
    let mut atts = b"<!DOCTYPE r [<!ATTLIST r".to_vec();
    for i in 0..40 {
        atts.extend_from_slice(format!(" d{i} CDATA 'default {i}'").as_bytes());
    }
    atts.extend_from_slice(b">]><r");
    for i in 0..30 {
        atts.extend_from_slice(format!(" a{i}='&amp;{i}  x'").as_bytes());
    }
    atts.extend_from_slice(b"/>");
    v.push(("attributes and defaults", atts));
    let mut ents = b"<!DOCTYPE r [<!ENTITY a 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'>".to_vec();
    ents.extend_from_slice(
        b"<!ENTITY b '&a;&a;&a;&a;&a;&a;&a;&a;'><!ENTITY c '&b;&b;&b;&b;&b;&b;&b;&b;'>",
    );
    ents.extend_from_slice(b"]><r v='&c;&c;&c;'>&c;&c;<x w='&b;'/></r>");
    v.push(("entity expansion", ents));
    let mut model = b"<!DOCTYPE r [".to_vec();
    for i in 0..20 {
        model.extend_from_slice(
            format!(
                "<!ELEMENT e{i} ((a|b|c)*,(d,(e|f)+)?,{}g{})>",
                "(".repeat(i),
                ")".repeat(i)
            )
            .as_bytes(),
        );
    }
    model.extend_from_slice(b"]><r/>");
    v.push(("content models", model));
    let mut misc = b"<?xml version='1.0' encoding='utf-8'?><!-- c --><r>".to_vec();
    for i in 0..50 {
        misc.extend_from_slice(
            format!(
                "<?pi{i} {}?><!--{}-->",
                "d".repeat(i * 13),
                "c".repeat(i * 17)
            )
            .as_bytes(),
        );
    }
    misc.extend_from_slice(b"</r>");
    v.push(("comments and PIs", misc));
    // Targeted documents (added after injected bugs K4 and K7 went
    // undetected by the documents above): one new binding per nesting
    // level (`addBinding`), and many distinct attribute names with defaults
    // on several element types (`defineAttribute`'s arrays, attribute ids).
    let mut scopes = Vec::new();
    for i in 0..600 {
        scopes.extend_from_slice(
            format!("<p{i}:e xmlns:p{i}='urn:{}{i}'>", "n".repeat(50)).as_bytes(),
        );
    }
    for i in (0..600).rev() {
        scopes.extend_from_slice(format!("</p{i}:e>").as_bytes());
    }
    v.push(("nested namespace scopes", scopes));
    let mut names = b"<!DOCTYPE e [".to_vec();
    for k in 0..8 {
        names.extend_from_slice(format!("<!ATTLIST e{k}").as_bytes());
        for j in 0..60 {
            names.extend_from_slice(format!(" a{k}x{j} CDATA 'v{j}'").as_bytes());
        }
        names.push(b'>');
    }
    names.extend_from_slice(b"]><e>");
    for k in 0..8 {
        names.extend_from_slice(format!("<e{k} a{k}x0='w'/>").as_bytes());
    }
    names.extend_from_slice(b"</e>");
    v.push(("distinct attribute names", names));
    v
}

/// For every sweep document and option set: every threshold at which C
/// fails (amplification 1.0) makes the port fail. The thresholds are a
/// geometric grid plus a binary search for C's boundary.
#[test]
fn threshold_boundary_sweep() {
    let mut failures = Vec::new();
    let mut boundary_hits = 0;
    let mut stricter = 0;
    for (name, doc) in sweep_documents() {
        for o in option_sets() {
            let probe = |thr: u64, failures: &mut Vec<String>| -> Option<bool> {
                let oo = at(&o, Some(1.0), Some(thr));
                let r = tracker::run(&doc, &oo).ok()?;
                match tracker::relation(&r, &oo) {
                    Ok(Relation::BothNoMemory) => Some(true),
                    Ok(Relation::PortStricter) => Some(false),
                    Ok(_) => Some(result_error_code(&r.c) == Some(NO_MEMORY)),
                    Err(e) => {
                        failures.push(format!("{name} thr={thr}: {e}"));
                        None
                    }
                }
            };
            let mut thr = 1024u64;
            while thr < 1 << 26 {
                probe(thr, &mut failures);
                thr = thr * 5 / 4;
            }
            // Binary search for the smallest threshold at which C passes
            // (its tracked peak), then probe just below it.
            let (mut lo, mut hi) = (0u64, 1u64 << 27);
            while hi - lo > 1 {
                let mid = lo + (hi - lo) / 2;
                let oo = at(&o, Some(1.0), Some(mid));
                let c = expat_oracle::parse_trace(&doc, &oo).unwrap();
                if result_error_code(&c) == Some(NO_MEMORY) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            for thr in [lo.saturating_sub(64), lo.saturating_sub(1), lo] {
                if probe(thr, &mut failures) == Some(true) {
                    boundary_hits += 1;
                }
            }
            // At C's passing threshold the port may still fail (it is stricter),
            // count it, it is the margin of D-030.
            let oo = at(&o, Some(1.0), Some(hi));
            if let Ok(r) = tracker::run(&doc, &oo)
                && tracker::relation(&r, &oo) == Ok(Relation::PortStricter)
            {
                stricter += 1;
            }
        }
    }
    eprintln!(
        "threshold sweep: {boundary_hits} boundary probes where C failed, port stricter at C's boundary in {stricter}"
    );
    assert!(
        failures.is_empty(),
        "{} failures, first: {}",
        failures.len(),
        failures[0]
    );
    assert!(boundary_hits > 50);
    // Every sweep document and option set: at C's exact passing threshold
    // the port still refuses (the creation charge alone is 16 KiB above
    // C's), the documented divergence under tightened limits.
    assert_eq!(stricter, sweep_documents().len() * option_sets().len());
}

/// With the threshold at 0, the amplification factor decides: every
/// factor at which C fails makes the port fail.
#[test]
fn amplification_boundary_sweep() {
    let mut failures = Vec::new();
    let mut c_fail = 0;
    for (name, doc) in sweep_documents() {
        for o in option_sets() {
            let mut f = 1.0f32;
            while f < 4000.0 {
                let oo = at(&o, Some(f), Some(0));
                match tracker::run(&doc, &oo).map(|r| (tracker::relation(&r, &oo), r)) {
                    Ok((Ok(rel), r)) => {
                        if rel == Relation::BothNoMemory
                            || (rel == Relation::Equal
                                && result_error_code(&r.c) == Some(NO_MEMORY))
                        {
                            c_fail += 1;
                        }
                    }
                    Ok((Err(e), _)) | Err(e) => failures.push(format!("{name} f={f}: {e}")),
                }
                f *= 1.3;
            }
        }
    }
    eprintln!("amplification sweep: C failed in {c_fail} probes");
    assert!(
        failures.is_empty(),
        "{} failures, first: {}",
        failures.len(),
        failures[0]
    );
    assert!(c_fail > 100);
}

/// An attribute value of `refs` references to an entity of `len`
/// characters: the billion-laughs factor stays at `(4 + len) / 4`, while
/// the value lands in one pool string of `refs * len` characters, whose
/// block C doubles.
fn attribute_expansion(len: usize, refs: usize) -> Vec<u8> {
    let mut d = format!("<!DOCTYPE r [<!ENTITY ee '{}'>]><r v='", "x".repeat(len)).into_bytes();
    for _ in 0..refs {
        d.extend_from_slice(b"&ee;");
    }
    d.extend_from_slice(b"'/>");
    d
}

/// The document only the tracker stops: 85,000 references to a
/// 395-character entity in one attribute value (~340 KB of input, 33.6 MB
/// of expansion, billion-laughs factor 99.75 < 100). libexpat's pool block
/// for the value doubles to 64 MiB: 67 MB allocated per 340 KB read is a
/// factor of ~197 above the 64 MiB threshold, so C reports
/// `XML_ERROR_NO_MEMORY`. Without a tracker the port would accept it.
#[test]
fn tracker_only_attribute() {
    let doc = attribute_expansion(395, 85_000);
    let o = base();
    let out = compare("tracker-only", &doc, &o, LARGE);
    let c_code = result_error_code(&out.c);
    let p_code = result_error_code(&out.port);
    eprintln!(
        "tracker-only attribute: input {} B, C code {c_code:?} tracker end {} heap peak {}, port code {p_code:?} tracker peak {} heap peak {}, {:?}",
        doc.len(),
        out.c_tracker.end,
        out.c_peak,
        out.port_tracker.peak,
        out.port_peak,
        out.elapsed
    );
    assert_eq!(c_code, Some(NO_MEMORY), "C must refuse through its tracker");
    assert_eq!(p_code, Some(NO_MEMORY), "the port must refuse too");
    // The billion-laughs check alone accepts it: with the tracker factor
    // raised, both parse it completely and equally.
    let relaxed = at(&o, Some(1000.0), None);
    let out = compare("tracker-relaxed", &doc, &relaxed, LARGE);
    assert!(out.equal(), "{:?}", first_difference(&out.c, &out.port));
    assert_eq!(result_error_code(&out.c), Some(0));
    assert!(out.port_tracker.end >= out.c_tracker.end);
    // A threshold above the pool block: accepted by both.
    let high = at(&o, None, Some(128 << 20));
    let out = compare("tracker-high-threshold", &doc, &high, LARGE);
    assert!(out.equal());
    assert_eq!(result_error_code(&out.c), Some(0));
}

/// The same shape around the boundary: values whose pool block stays at
/// 32 MiB are accepted by both, values that need 64 MiB are refused by
/// both, and a value whose billion-laughs factor exceeds 100 stops with
/// error 43 on both sides before the tracker matters.
#[test]
fn attribute_expansion_sizes() {
    let o = base();
    for (len, refs, expect) in [
        (395usize, 40_000usize, Some(0)),
        (395, 84_000, Some(0)),
        (395, 85_500, Some(NO_MEMORY)),
        (300, 120_000, Some(NO_MEMORY)),
        (2000, 20_000, Some(BREACH)),
    ] {
        let doc = attribute_expansion(len, refs);
        let out = compare(&format!("attr-{len}x{refs}"), &doc, &o, LARGE);
        let c = result_error_code(&out.c);
        let p = result_error_code(&out.port);
        eprintln!(
            "attr {len}x{refs}: C {c:?} (tracker end {}), port {p:?} (tracker peak {})",
            out.c_tracker.end, out.port_tracker.peak
        );
        assert_eq!(c, expect, "C outcome of {len}x{refs}");
        if c == Some(NO_MEMORY) {
            assert_eq!(p, Some(NO_MEMORY));
        } else {
            assert!(out.equal(), "{len}x{refs}");
        }
    }
}

/// Control: the same expansion in content is streamed through the data
/// buffer, so the tracker does not grow with it (C accepts 33 MB of
/// expanded character data per 340 KB read); the port must accept it too.
#[test]
fn content_expansion_is_not_tracked() {
    let mut d = format!("<!DOCTYPE r [<!ENTITY ee '{}'>]><r>", "x".repeat(395)).into_bytes();
    for _ in 0..85_000 {
        d.extend_from_slice(b"&ee;");
    }
    d.extend_from_slice(b"</r>");
    let o = ParseOptions {
        text_digest: true,
        ..base()
    };
    let out = compare("content-expansion", &d, &o, LARGE);
    eprintln!(
        "content expansion: C tracker end {}, port tracker peak {}",
        out.c_tracker.end, out.port_tracker.peak
    );
    assert!(out.equal());
    assert_eq!(result_error_code(&out.c), Some(0));
}
