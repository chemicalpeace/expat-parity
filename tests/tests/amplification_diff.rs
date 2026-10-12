//! Billion-laughs protection (amplification accounting) against the C
//! oracle: `XML_SetBillionLaughsAttackProtectionMaximumAmplification` and
//! `...ActivationThreshold`, the default limits, custom limits, the
//! behaviour at the activation threshold and at the amplification limit,
//! quadratic blow-up, and `XML_ERROR_AMPLIFICATION_LIMIT_BREACH` (43) with
//! its line, column and byte index.
//!
//! Every pair is compared by canonical trace with `trace_accounting` (the
//! direct and indirect byte counters of both parsers must be equal, not
//! only the outcome) and, for large expansions, `text_digest` (character
//! data as length + FNV-1a hash). Every comparison is bounded in input
//! size, wall-clock time and memory (`expat_difftest::bounded`; the limits
//! are documented in docs/REPORT.md and docs/DTD-SECURITY.md).
//!
//! Groups (expected C behaviour and policy in docs/REPORT.md): setters,
//! default limits, custom limits on generated entity documents, threshold
//! sweep, amplification sweep, quadratic blow-up, error positions.

use std::time::Duration;

use expat_difftest::bounded::{Bounds, CountingAlloc, Outcome, compare};
use expat_difftest::docgen::{self, Rng};
use expat_difftest::trace::{first_difference, result_error_code};
use expat_oracle::{Factor, Limits, ParamEntityParsing, ParseOptions};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

/// `XML_ERROR_AMPLIFICATION_LIMIT_BREACH`.
const BREACH: u32 = 43;

/// Bounds of the small-document groups: 16 KiB input, 20 s, 64 MiB heap.
const SMALL: Bounds = Bounds {
    max_input: 16 << 10,
    timeout: Duration::from_secs(20),
    port_heap_cap: 64 << 20,
    c_heap_cap: 64 << 20,
};

/// Bounds of the large-expansion groups (default limits, quadratic
/// blow-up): 256 KiB input, 60 s, 256 MiB heap per side.
const LARGE: Bounds = Bounds::DEFAULT;

fn base() -> ParseOptions {
    ParseOptions {
        param_entity_parsing: ParamEntityParsing::Always,
        trace_accounting: true,
        ..ParseOptions::default()
    }
}

fn with_limits(o: &ParseOptions, amp: Option<f32>, thr: Option<u64>) -> ParseOptions {
    ParseOptions {
        limits: Limits {
            bl_amplification: amp.map(Factor),
            bl_threshold: thr,
            ..Limits::NONE
        },
        ..o.clone()
    }
}

/// Whole, chunked (1, 7 and 4096 bytes), buffer API with namespaces.
fn feeds(o: &ParseOptions) -> Vec<ParseOptions> {
    vec![
        o.clone(),
        ParseOptions {
            chunk_size: 1,
            ..o.clone()
        },
        ParseOptions {
            chunk_size: 7,
            ..o.clone()
        },
        ParseOptions {
            chunk_size: 4096,
            buffer_api: true,
            ns_separator: Some(b'|'),
            ..o.clone()
        },
    ]
}

struct Tally {
    pairs: usize,
    failures: Vec<String>,
    max_port_peak: usize,
    max_c_peak: usize,
    max_elapsed: Duration,
}

impl Tally {
    fn new() -> Self {
        Tally {
            pairs: 0,
            failures: Vec::new(),
            max_port_peak: 0,
            max_c_peak: 0,
            max_elapsed: Duration::ZERO,
        }
    }

    fn run(&mut self, id: &str, doc: &[u8], o: &ParseOptions, b: Bounds) -> Outcome {
        let out = compare(id, doc, o, b);
        self.pairs += 1;
        self.max_port_peak = self.max_port_peak.max(out.port_peak);
        self.max_c_peak = self.max_c_peak.max(out.c_peak);
        self.max_elapsed = self.max_elapsed.max(out.elapsed);
        if !out.equal() {
            let msg = match first_difference(&out.c, &out.port) {
                Some((line, c, r)) => {
                    let cut = |s: &str| s.chars().take(160).collect::<String>();
                    format!(
                        "{id} {o:?}: line {line}: C `{}` port `{}`",
                        cut(&c),
                        cut(&r)
                    )
                }
                None => format!("{id} {o:?}: traces differ"),
            };
            self.failures.push(msg);
        }
        out
    }

    fn finish(&self, what: &str) {
        eprintln!(
            "{what}: {} pairs compared, {} mismatches (max heap: port {} B, C {} B; max time per pair {:?})",
            self.pairs,
            self.failures.len(),
            self.max_port_peak,
            self.max_c_peak,
            self.max_elapsed
        );
        for f in self.failures.iter().take(10) {
            eprintln!("{f}");
        }
        assert!(self.failures.is_empty(), "{what}: mismatches");
    }
}

fn code(out: &Outcome) -> u32 {
    result_error_code(&out.c).unwrap_or(u32::MAX)
}

/// Parses `accounting <direct> <indirect>` from a trace.
fn counters(trace: &[u8]) -> (u64, u64) {
    let text = String::from_utf8_lossy(trace);
    for l in text.lines() {
        if let Some(rest) = l.strip_prefix("accounting ") {
            let mut it = rest.split(' ').map(|x| x.parse::<u64>().unwrap_or(0));
            return (it.next().unwrap_or(0), it.next().unwrap_or(0));
        }
    }
    (0, 0)
}

/// A billion-laughs document: `levels` entities, each referencing the
/// previous one `fanout` times, the innermost being `seed`, referenced once
/// from context `ctx`, with newlines so that positions have lines and
/// columns.
fn laughs(levels: usize, fanout: usize, seed: &str, ctx: &str) -> Vec<u8> {
    let mut d = String::from("<?xml version='1.0'?>\n<!DOCTYPE r [\n");
    let pe = ctx == "pe" || ctx == "value";
    let (decl, rf) = if pe {
        ("<!ENTITY % ", "&#37;")
    } else {
        ("<!ENTITY ", "&")
    };
    d.push_str(&format!("  {decl}l0 '{seed}'>\n"));
    for i in 1..levels {
        let body: String = (0..fanout).map(|_| format!("{rf}l{};", i - 1)).collect();
        d.push_str(&format!("  {decl}l{i} '{body}'>\n"));
    }
    let top = levels - 1;
    match ctx {
        "content" => d.push_str(&format!("]>\n<r>\n  &l{top};\n</r>\n")),
        "attr" => d.push_str(&format!("]>\n<r\n a='&l{top};'/>\n")),
        "default" => d.push_str(&format!("  <!ATTLIST r a CDATA '&l{top};'>\n]>\n<r/>\n")),
        "pe" => d.push_str(&format!("  %l{top};\n]>\n<r/>\n")),
        // A parameter entity whose text declares a general entity: its
        // value is stored with every parameter entity expanded.
        _ => d.push_str(&format!(
            "  <!ENTITY % d '<!ENTITY big \"&#37;l{top};\">'>\n  %d;\n]>\n<r>&big;</r>\n"
        )),
    }
    d.into_bytes()
}

const CONTEXTS: [&str; 5] = ["content", "attr", "default", "pe", "value"];

/// The text of entity `l0` per context: character data in content and
/// attributes, a comment where the replacement text is markup.
fn seed(ctx: &str) -> &'static str {
    match ctx {
        "pe" => "<!--lol-->",
        _ => "lol",
    }
}

#[test]
fn setters_match_c() {
    // Values libexpat must refuse (NaN, < 1.0) and accept (>= 1.0,
    // infinity, any threshold); the `limit` lines carry the XML_Bool.
    let doc = b"<!DOCTYPE r [<!ENTITY e 'x'>]><r>&e;</r>";
    let amps = [
        f32::NAN,
        -f32::NAN,
        f32::NEG_INFINITY,
        -1.0,
        0.0,
        -0.0,
        0.5,
        0.999_999_9,
        1.0,
        1.000_000_1,
        1.5,
        100.0,
        f32::MAX,
        f32::INFINITY,
    ];
    let thresholds = [0u64, 1, 22, 8_388_607, 8_388_608, u64::MAX];
    let mut t = Tally::new();
    for &a in &amps {
        for &th in &thresholds {
            let o = with_limits(&base(), Some(a), Some(th));
            let out = t.run("setter", doc, &o, SMALL);
            let text = String::from_utf8_lossy(&out.c).into_owned();
            let want = if a.is_nan() || a < 1.0 { 0 } else { 1 };
            assert!(
                text.starts_with(&format!(
                    "limit bl-amplification {want}\nlimit bl-threshold 1\n"
                )),
                "C setter results for {a}: {text}"
            );
        }
    }
    t.finish("setters");
}

#[test]
fn default_limits_match_c() {
    // Billion laughs (10 levels x 10 references, 3 * 10^9 bytes if
    // unchecked) under libexpat's defaults (100.0, 8 MiB): stopped with 43
    // shortly after the output passes 8 MiB, in every context.
    let mut t = Tally::new();
    for ctx in CONTEXTS {
        let doc = laughs(10, 10, seed(ctx), ctx);
        for o in feeds(&ParseOptions {
            text_digest: true,
            ..base()
        }) {
            let out = t.run(&format!("laughs-{ctx}"), &doc, &o, LARGE);
            assert_eq!(code(&out), BREACH, "laughs-{ctx}: C must stop with 43");
            let (d, i) = counters(&out.c);
            assert!(
                d + i >= 8_388_608,
                "laughs-{ctx}: stopped below the threshold"
            );
        }
        // Below the activation threshold an amplification of thousands is
        // tolerated: 6 levels of 10 = 3 MB of output.
        let doc = laughs(6, 10, seed(ctx), ctx);
        let o = ParseOptions {
            text_digest: true,
            ..base()
        };
        let out = t.run(&format!("below-threshold-{ctx}"), &doc, &o, LARGE);
        assert_eq!(code(&out), 0, "below-threshold-{ctx}: C accepts");
    }
    // Above the threshold but within the factor: about 195,000 direct
    // bytes first, then about 9.2 MB of expansion (factor ~48).
    let mut d = String::from("<!DOCTYPE r [<!ENTITY a '0123456789'>");
    d.push_str("<!ENTITY b '&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;'>");
    d.push_str("<!ENTITY c '&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;'>");
    d.push_str("<!ENTITY e '&c;&c;&c;&c;&c;&c;&c;&c;&c;&c;'>]><r>");
    while d.len() < 195_000 {
        d.push_str("<p>direct text direct text direct text</p>\n");
    }
    for _ in 0..900 {
        d.push_str("&e;");
    }
    d.push_str("</r>");
    let o = ParseOptions {
        text_digest: true,
        ..base()
    };
    let out = t.run("within-factor", d.as_bytes(), &o, LARGE);
    let (dir, ind) = counters(&out.c);
    assert_eq!(
        code(&out),
        0,
        "within-factor: C accepts (direct {dir}, indirect {ind})"
    );
    assert!(
        dir + ind > 8_388_608,
        "within-factor must pass the threshold"
    );
    t.finish("default limits");
}

#[test]
fn custom_limits_on_generated_documents_match_c() {
    // Random entity documents (module 11b generator) under random small
    // limits, so that 43 is raised at many different places: content,
    // attribute values, default values, parameter entities, entity values.
    let mut r = Rng::new(0x11b2_b1a5);
    let mut t = Tally::new();
    let mut codes = std::collections::BTreeMap::new();
    let amps = [1.0f32, 1.25, 1.5, 2.0, 3.0, 10.0, 100.0];
    let thresholds = [0u64, 1, 16, 64, 256, 1024, 8_388_608];
    let pep = [
        ParamEntityParsing::Never,
        ParamEntityParsing::UnlessStandalone,
        ParamEntityParsing::Always,
    ];
    for i in 0..4_000 {
        let doc = docgen::entity_document(&mut r);
        let mut o = with_limits(&base(), Some(*r.pick(&amps)), Some(*r.pick(&thresholds)));
        o.param_entity_parsing = *r.pick(&pep);
        o.chunk_size = *r.pick(&[0usize, 0, 1, 7]);
        o.buffer_api = r.one_in(4);
        if r.one_in(3) {
            o.ns_separator = Some(b':');
        }
        let out = t.run(&format!("gen-{i}"), &doc, &o, SMALL);
        *codes.entry(code(&out)).or_insert(0usize) += 1;
    }
    eprintln!("codes {codes:?}");
    t.finish("custom limits");
    assert!(
        codes.get(&BREACH).copied().unwrap_or(0) >= 100,
        "the generator must reach 43 often"
    );
    assert!(codes.contains_key(&0), "and also succeed");
}

/// Small documents where every context expands a little.
fn sweep_documents() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        (
            "content",
            b"<!DOCTYPE r [<!ENTITY a 'xy'><!ENTITY b '&a;-&a;'>]>\n<r>t&b;u&a;&lt;&#65;</r>".to_vec(),
        ),
        (
            "attr",
            b"<!DOCTYPE r [<!ENTITY a 'xy'><!ENTITY b '&a;-&a;'>]>\n<r k='&b;&a;&amp;'/>".to_vec(),
        ),
        (
            "default",
            b"<!DOCTYPE r [<!ENTITY a 'xy'><!ATTLIST r k CDATA '&a;&a;'>]>\n<r/>".to_vec(),
        ),
        (
            "pe",
            b"<!DOCTYPE r [<!ENTITY % a '<!--c-->'><!ENTITY % b '&#37;a;&#37;a;'>\n%b;%a;]><r/>".to_vec(),
        ),
        (
            "value",
            b"<!DOCTYPE r [<!ENTITY % a 'xy'><!ENTITY % d '<!ENTITY v \"&#37;a;&#37;a;\">'>%d;]>\n<r>&v;</r>"
                .to_vec(),
        ),
    ]
}

#[test]
fn threshold_sweep_matches_c() {
    // Factor 1.0 (any indirect byte is too much once active) and every
    // activation threshold from 0 to the document's total output + 2: the
    // breach must happen at exactly the same token, position and counter
    // values (`output < threshold` is the activation test).
    let mut t = Tally::new();
    for (ctx, doc) in sweep_documents() {
        let free = t.run(&format!("free-{ctx}"), &doc, &base(), SMALL);
        assert_eq!(code(&free), 0, "{ctx}: C accepts without limits");
        let (d, i) = counters(&free.c);
        let mut breached = 0;
        for th in 0..=d + i + 2 {
            for o in [
                with_limits(&base(), Some(1.0), Some(th)),
                ParseOptions {
                    chunk_size: 3,
                    ..with_limits(&base(), Some(1.0), Some(th))
                },
            ] {
                let out = t.run(&format!("thr-{ctx}-{th}"), &doc, &o, SMALL);
                if code(&out) == BREACH {
                    breached += 1;
                }
            }
        }
        assert!(breached > 0, "{ctx}: no threshold produced 43");
    }
    t.finish("threshold sweep");
}

#[test]
fn amplification_sweep_matches_c() {
    // Threshold 0 (always active) and every factor (direct + indirect) /
    // direct that the counters of the document can take, computed in
    // `float` like `accountingGetCurrentAmplification`, plus the next
    // float above and below: the `<=` of the limit test is hit exactly.
    let mut t = Tally::new();
    for (ctx, doc) in sweep_documents() {
        let free = t.run(&format!("free-{ctx}"), &doc, &base(), SMALL);
        let (d, i) = counters(&free.c);
        let mut factors = std::collections::BTreeSet::new();
        for dd in 1..=d {
            for ii in 0..=i {
                let f = (dd + ii) as f32 / dd as f32;
                for g in [
                    f,
                    f32::from_bits(f.to_bits() - 1),
                    f32::from_bits(f.to_bits() + 1),
                ] {
                    if g >= 1.0 {
                        factors.insert(g.to_bits());
                    }
                }
            }
        }
        let mut seen = std::collections::BTreeMap::new();
        for bits in factors {
            let f = f32::from_bits(bits);
            let o = with_limits(&base(), Some(f), Some(0));
            let out = t.run(&format!("amp-{ctx}-{f}"), &doc, &o, SMALL);
            *seen.entry(code(&out)).or_insert(0usize) += 1;
        }
        assert!(
            seen.contains_key(&BREACH) && seen.contains_key(&0),
            "{ctx}: sweep must reach both outcomes: {seen:?}"
        );
    }
    t.finish("amplification sweep");
}

#[test]
fn quadratic_blowup_matches_c() {
    // One 16 KiB entity referenced many times (no nesting): bounded only by
    // the same amplification test. 1,024 references produce 16 MiB of
    // output from about 20 KB of input (factor ~800): 43 by default,
    // accepted with a factor of 1,000; also in an attribute value, where
    // libexpat builds the whole value in memory before the breach.
    let big = "x".repeat(16 << 10);
    let mut t = Tally::new();
    for ctx in ["content", "attr"] {
        let refs = "&e;".repeat(1024);
        let doc = match ctx {
            "content" => format!("<!DOCTYPE r [<!ENTITY e '{big}'>]>\n<r>\n{refs}</r>"),
            _ => format!("<!DOCTYPE r [<!ENTITY e '{big}'>]>\n<r a='{refs}'/>"),
        };
        let o = ParseOptions {
            text_digest: true,
            ..base()
        };
        for f in feeds(&o) {
            let out = t.run(&format!("quadratic-{ctx}"), doc.as_bytes(), &f, LARGE);
            assert_eq!(code(&out), BREACH, "quadratic-{ctx}: 43 by default");
        }
        let out = t.run(
            &format!("quadratic-{ctx}-1000"),
            doc.as_bytes(),
            &with_limits(&o, Some(1000.0), None),
            LARGE,
        );
        assert_eq!(code(&out), 0, "quadratic-{ctx}: accepted with factor 1000");
        assert!(
            out.port_peak < LARGE.port_heap_cap && out.c_peak < LARGE.c_heap_cap,
            "within the memory bound"
        );
    }
    t.finish("quadratic blow-up");
}

#[test]
fn breach_positions_match_c() {
    // Error 43 inside nested expansions in every context, on multi-line
    // documents, under every feed; the result line (code, line, column,
    // byte index) and the counters must be equal. Small limits keep the
    // documents tiny.
    let mut t = Tally::new();
    let mut breaches = 0;
    for ctx in CONTEXTS {
        let doc = laughs(4, 3, seed(ctx), ctx);
        for (amp, thr) in [(1.0, 0), (2.0, 10), (5.0, 100), (1.5, 64)] {
            for o in feeds(&with_limits(&base(), Some(amp), Some(thr))) {
                let out = t.run(&format!("pos-{ctx}-{amp}-{thr}"), &doc, &o, SMALL);
                assert!(
                    code(&out) == BREACH || code(&out) == 0,
                    "pos-{ctx}: unexpected code {}",
                    code(&out)
                );
                breaches += usize::from(code(&out) == BREACH);
            }
        }
    }
    t.finish("breach positions");
    eprintln!("breach positions: {breaches} pairs end with 43");
    assert!(breaches >= 40, "most position cases must breach");
}

#[test]
fn q4_epilog_name_at_end_of_buffer() {
    // DIVERGENCES.md Q-4: a name cut off by the end of the buffer in the
    // epilog. libexpat computes `NULL - s` for its byte count (undefined
    // behaviour; the counter becomes address-dependent) and then fails with
    // XML_ERROR_JUNK_AFTER_DOC_ELEMENT; the port counts 0 bytes for the
    // token and fails the same way at the same position. Only the
    // `accounting` line differs, which `mask_q4` masks.
    let mut t = Tally::new();
    for (doc, chunk) in [
        (&b"<r/>junk"[..], 0usize),
        (b"<r/>\n  junk", 3),
        (b"<r/>j", 1),
    ] {
        let o = ParseOptions {
            chunk_size: chunk,
            ..base()
        };
        let out = t.run("q4", doc, &o, SMALL);
        assert_eq!(
            code(&out),
            9,
            "C fails with junk after the document element"
        );
        let (c_direct, _) = counters(&out.c);
        let port_direct = counters(&out.port).0;
        eprintln!("q4 {chunk}: C direct counter {c_direct}, port {port_direct}");
        assert!(
            port_direct <= doc.len() as u64,
            "the port's counter stays a byte count"
        );
    }
    t.finish("q4");
}
