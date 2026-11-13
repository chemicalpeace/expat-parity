//! libexpat 2.10.0 security fixes (2026-11-02) checked against the port of
//! libexpat 2.9.0 (`expat` 0.1.0-beta.1). One group per CVE; the verdicts
//! and measurements are in the check report (`expat-parity-cve-2-10-0.md`).
//!
//! - CVE-2026-113351 (libexpat#1431): `XML_ParserReset` kept the
//!   allocation-tracker total and reset the billion-laughs settings. The
//!   port has no reset (neither the public nor the internal `Parser`); the
//!   tests pin the upstream reset-neutrality documents on fresh parsers.
//! - CVE-2026-115506 (#1433 #1436 #1438 #1439 #1442): billion laughs
//!   detected only after up to `factor x direct bytes` of expansion work.
//!   The port replicates 2.9.0's accounting exactly, so it is affected:
//!   `cve_2026_115506_port_still_expands_before_breach` pins today's
//!   (vulnerable) behaviour and must be inverted when the fix lands; the
//!   strict 2.10.0 check runs with `EXPAT_CVE_STRICT=1` and fails today.
//!   `bl_measure_*` print port and C timings (`--nocapture`;
//!   `EXPAT_CVE_TIMING=1` for the scaling run).
//! - CVE-2026-117610 (#1444): an empty, non-final first parse call of an
//!   external entity parser.
//! - CVE-2026-118370 (#1450): `attIndex` / `nPrefixes` signed overflow in
//!   `storeAtts`. The port counts in `usize`; not reachable in a test
//!   (needs > 2^30 attributes in one start tag).
//!
//! Run the timing groups in release:
//! `EXPAT_CVE_TIMING=1 cargo test --release -p expat-difftest --test cve_2_10_0 -- --nocapture`.

use std::time::Instant;

use expat::internal::parse::{
    ExternalEntities, Handler, HandlerSet, ParamEntityParsing, Parser, Status, Stop,
};
use expat_difftest::trace::result_error_code;
use expat_oracle::{HandlerMask, ParseOptions};

const SALT: [u8; 16] = [0x5a; 16];
/// `XML_ERROR_AMPLIFICATION_LIMIT_BREACH`.
const BREACH: u32 = 43;

struct Nop;
impl Handler for Nop {}

// ---------------------------------------------------------------------------
// Shared: one port parse and one C parse, timed.

#[derive(Debug, Clone, Copy)]
struct Run {
    code: u32,
    direct: u64,
    indirect: u64,
    secs: f64,
}

fn code_of(p: &Parser) -> u32 {
    match p.error_code() {
        Stop::Error(e) => e.to_c(),
    }
}

fn port_run(doc: &[u8], pe: bool) -> Run {
    let mut p = Parser::new(None, None, SALT);
    if pe {
        assert!(p.set_param_entity_parsing(ParamEntityParsing::Always));
    }
    let t = Instant::now();
    let st = p.parse(&mut Nop, doc, true);
    let secs = t.elapsed().as_secs_f64();
    let code = if st == Status::Ok { 0 } else { code_of(&p) };
    let a = p.accounting();
    Run {
        code,
        direct: a.count_bytes_direct,
        indirect: a.count_bytes_indirect,
        secs,
    }
}

fn c_run(doc: &[u8], pe: bool) -> Run {
    let opts = ParseOptions {
        param_entity_parsing: if pe {
            expat_oracle::ParamEntityParsing::Always
        } else {
            expat_oracle::ParamEntityParsing::Never
        },
        trace_accounting: true,
        handler_mask: Some(HandlerMask::default()),
        ..ParseOptions::default()
    };
    let t = Instant::now();
    let trace = expat_oracle::parse_trace(doc, &opts).unwrap_or_default();
    let secs = t.elapsed().as_secs_f64();
    let text = String::from_utf8_lossy(&trace);
    let (direct, indirect) = text
        .lines()
        .find_map(|l| l.strip_prefix("accounting "))
        .and_then(|r| {
            let mut it = r.split(' ').map(|n| n.parse::<u64>().unwrap_or(0));
            Some((it.next()?, it.next()?))
        })
        .unwrap_or((0, 0));
    Run {
        code: result_error_code(&trace).unwrap_or(u32::MAX),
        direct,
        indirect,
        secs,
    }
}

// ---------------------------------------------------------------------------
// CVE-2026-113351 (#1431): XML_ParserReset.

/// The API does not exist: a new document needs a new `Parser`, which
/// starts with zeroed accounting and tracker totals and default limits
/// (C: `parserCreate`, not `XML_ParserReset`). Upstream's
/// `test_amplification_reset_neutrality` document on two fresh parsers
/// with the same settings breaches both times.
#[test]
fn cve_2026_113351_amplification_settings_per_parser() {
    let doc = b"<!DOCTYPE r [<!ENTITY e 'general entity'>]><r>&e;</r>";
    for _ in 0..2 {
        let mut p = Parser::new(None, None, SALT);
        assert!(p.set_billion_laughs_maximum_amplification(1.0));
        assert!(p.set_billion_laughs_activation_threshold(0));
        assert_eq!(p.parse(&mut Nop, doc, true), Status::Error);
        assert_eq!(code_of(&p), BREACH);
    }
}

/// Upstream's `test_alloc_tracker_reset_neutrality`: a document of 1.1x
/// the allocation-tracker activation threshold, then `<hello/>`. Without
/// reset, the second document is parsed by a fresh parser and succeeds.
#[test]
fn cve_2026_113351_alloc_tracker_fresh_parser_after_big_document() {
    const TRACKER_THRESHOLD: usize = 64 << 20; // EXPAT_ALLOC_TRACKER_ACTIVATION_THRESHOLD_DEFAULT
    let size = TRACKER_THRESHOLD + TRACKER_THRESHOLD / 10;
    let mut big = vec![b'X'; size];
    big[..8].copy_from_slice(b"<doc a='");
    let n = big.len();
    big[n - 3..].copy_from_slice(b"'/>");
    let mut p = Parser::new(None, None, SALT);
    assert_eq!(p.parse(&mut Nop, &big, true), Status::Ok);
    drop(p);
    let mut p = Parser::new(None, None, SALT);
    assert_eq!(p.parse(&mut Nop, b"<hello/>", true), Status::Ok);
}

// ---------------------------------------------------------------------------
// CVE-2026-115506 (#1433 ...): billion laughs detected late.

/// A lololol document (github.com/hannob/lololol): root text of `len`
/// bytes, `depth` levels of `multi` references each, `count` references
/// at the top, `pad` bytes of comment in the internal subset first
/// (raises the direct byte count and with it the expansion allowed before
/// the 100x limit applies), declarations reversed when `flip`.
fn lol(
    pe: bool,
    len: usize,
    multi: usize,
    depth: usize,
    count: usize,
    pad: usize,
    flip: bool,
) -> Vec<u8> {
    let mut decls = Vec::new();
    if pe {
        let s = if len >= 7 {
            format!("<!--{}-->", "x".repeat(len - 7))
        } else {
            " ".repeat(len)
        };
        decls.push(format!("<!ENTITY % a0 \"{s}\">\n"));
        for j in 0..depth {
            decls.push(format!(
                "<!ENTITY % a{} \"{}\">",
                j + 1,
                format!("&#37;a{j};").repeat(multi)
            ));
        }
    } else {
        decls.push(format!("<!ENTITY a0 \"{}\">\n", "l".repeat(len)));
        for j in 0..depth {
            decls.push(format!(
                "<!ENTITY a{} \"{}\">",
                j + 1,
                format!("&a{j};").repeat(multi)
            ));
        }
    }
    if flip {
        decls.reverse();
    }
    let mut s = String::from("<!DOCTYPE billion [\n");
    if pad > 0 {
        s.push_str("<!--");
        s.push_str(&"p".repeat(pad));
        s.push_str("-->\n");
    }
    s.push_str(&decls.join("\n"));
    if pe {
        s.push('\n');
        s.push_str(&format!("%a{depth};").repeat(count));
        s.push_str("\n]><x/>\n");
    } else {
        s.push_str("]><x>");
        s.push_str(&format!("&a{depth};").repeat(count));
        s.push_str("</x>\n");
    }
    s.into_bytes()
}

/// Shapes measured: (name, pe, len, multi, depth, count, flip). Each
/// expands to far more than 100x its size, so both parsers must breach.
const SHAPES: &[(&str, bool, usize, usize, usize, usize, bool)] = &[
    ("ge-classic-lol", false, 3, 10, 9, 1, false),
    ("ge-empty-root", false, 0, 10, 9, 1, false),
    ("ge-flip", false, 3, 10, 9, 1, true),
    ("pe-comment", true, 20, 10, 8, 1, false),
    ("pe-empty", true, 0, 10, 8, 1, false),
];

/// Port and C reach the same verdict and the same counters at the breach
/// (the port replicates 2.9.0), and the indirect count at the breach is
/// ~100x the direct count: the expansion work done before rejection grows
/// with the document size. Prints the timings.
#[test]
fn bl_measure_shapes_port_vs_c() {
    for &(name, pe, len, multi, depth, count, flip) in SHAPES {
        let pads: &[usize] = if std::env::var_os("EXPAT_CVE_TIMING").is_some() {
            &[0, 256 << 10, 1 << 20]
        } else {
            &[0, 256 << 10]
        };
        for &pad in pads {
            let doc = lol(pe, len, multi, depth, count, pad, flip);
            let r = port_run(&doc, pe);
            let c = c_run(&doc, pe);
            eprintln!(
                "cve-115506: {name:<15} pad={pad:>8} doc={:>8}B port: code={} {:.3}s direct={} indirect={} ({:.1}x) | C 2.9.0: code={} {:.3}s indirect={}",
                doc.len(),
                r.code,
                r.secs,
                r.direct,
                r.indirect,
                r.indirect as f64 / r.direct.max(1) as f64,
                c.code,
                c.secs,
                c.indirect
            );
            assert_eq!(r.code, BREACH, "{name} pad={pad}: port must breach");
            assert_eq!(c.code, BREACH, "{name} pad={pad}: C must breach");
            assert_eq!(
                (r.direct, r.indirect),
                (c.direct, c.indirect),
                "{name} pad={pad}: accounting differs from C"
            );
        }
    }
}

/// Time to breach as the padding grows (2.9.0 behaviour: linear in the
/// padding with a ~100x factor; 2.10.0: the parse of the padding only).
/// Release build recommended. `EXPAT_CVE_PAD_MIB` overrides the largest
/// padding (default 4 MiB); `EXPAT_CVE_SHAPE` selects one shape by name.
#[test]
fn bl_measure_time_scaling() {
    if std::env::var_os("EXPAT_CVE_TIMING").is_none() {
        eprintln!("cve-115506-scaling: skipped (set EXPAT_CVE_TIMING=1, release build)");
        return;
    }
    let max_mib: usize = std::env::var("EXPAT_CVE_PAD_MIB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let only = std::env::var("EXPAT_CVE_SHAPE").ok();
    let mut pad_mib = 1;
    while pad_mib <= max_mib {
        for &(name, pe, len, multi, depth, count, flip) in SHAPES {
            if only.as_deref().is_some_and(|o| o != name) {
                continue;
            }
            let doc = lol(pe, len, multi, depth, count, pad_mib << 20, flip);
            let r = port_run(&doc, pe);
            let c = c_run(&doc, pe);
            eprintln!(
                "cve-115506-scaling: {name:<15} pad={pad_mib}MiB port {:.2}s (indirect {:.0} MB) | C 2.9.0 {:.2}s (indirect {:.0} MB) | port/C {:.2}",
                r.secs,
                r.indirect as f64 / 1e6,
                c.secs,
                c.indirect as f64 / 1e6,
                r.secs / c.secs.max(1e-9)
            );
            assert_eq!(r.code, BREACH);
            assert_eq!(c.code, BREACH);
        }
        pad_mib *= 2;
    }
}

/// Pins the port's current, vulnerable behaviour (CVE-2026-115506): at
/// the breach the indirect count is ~99x the direct count, i.e. the
/// expansion ran until the 100x limit. When 2.10.0's precomputed
/// expansion sizes (#1433 + #1439) are ported this test fails by design:
/// replace it with `cve_2026_115506_rejected_before_expansion` unconditionally.
#[test]
fn cve_2026_115506_port_still_expands_before_breach() {
    let (name, pe, len, multi, depth, count, flip) = SHAPES[0];
    let doc = lol(pe, len, multi, depth, count, 256 << 10, flip);
    let r = port_run(&doc, pe);
    assert_eq!(r.code, BREACH, "{name}");
    let ratio = r.indirect as f64 / r.direct as f64;
    assert!(
        ratio > 90.0,
        "{name}: ratio {ratio:.1}: fixed? see the doc comment"
    );
}

/// 2.10.0 behaviour (strict): an entity reference whose precomputed
/// expansion would breach the limit is rejected before it is expanded,
/// so the indirect bytes at the breach stay below the direct bytes
/// instead of reaching ~100x of them. Fails on the port (and on C 2.9.0)
/// until the fix lands; runs with `EXPAT_CVE_STRICT=1`.
#[test]
fn cve_2026_115506_rejected_before_expansion() {
    if std::env::var_os("EXPAT_CVE_STRICT").is_none() {
        eprintln!(
            "cve-115506 strict: skipped (set EXPAT_CVE_STRICT=1; fails until the fix is ported)"
        );
        return;
    }
    for &(name, pe, len, multi, depth, count, flip) in SHAPES {
        let doc = lol(pe, len, multi, depth, count, 1 << 20, flip);
        let r = port_run(&doc, pe);
        assert_eq!(r.code, BREACH, "{name}");
        assert!(
            r.indirect < r.direct,
            "{name}: {} indirect bytes processed before the breach (direct {}), 2.10.0 rejects at the reference",
            r.indirect,
            r.direct
        );
    }
}

/// #1439: references inside CDATA sections, comments and PIs in entity
/// text are not expanded, so 2.10.0's precomputation must not count them
/// (a backport without #1439 rejects this valid document). Passes on the
/// port today; keeps a future port of the precomputation honest.
#[test]
fn cve_2026_115506_pr1439_markup_in_entity_text_not_counted() {
    let mut s = String::from("<!DOCTYPE r [\n<!ENTITY a0 'lol'>\n");
    for j in 0..9 {
        s.push_str(&format!(
            "<!ENTITY a{} '{}'>\n",
            j + 1,
            format!("&a{j};").repeat(10)
        ));
    }
    s.push_str(
        "<!ENTITY safe '<![CDATA[&a9;&a9;]]><!--&a9;--><?pi &a9;?>'>\n]><r>&safe;&safe;</r>",
    );
    let doc = s.as_bytes();
    let r = port_run(doc, false);
    let c = c_run(doc, false);
    assert_eq!(r.code, 0, "port must accept");
    assert_eq!(c.code, 0, "C must accept");
}

// ---------------------------------------------------------------------------
// CVE-2026-117610 (#1444): empty non-final first parse of an external
// entity parser.

#[derive(Default)]
struct Sub {
    /// The chunks the external entity parser is fed with.
    chunks: Vec<(Vec<u8>, bool)>,
    statuses: Vec<Status>,
    child_codes: Vec<u32>,
    pis: Vec<(Vec<u8>, Vec<u8>)>,
    chars: Vec<u8>,
}

impl Handler for Sub {
    fn processing_instruction(&mut self, target: &[u8], data: &[u8]) {
        self.pis.push((target.to_vec(), data.to_vec()));
    }
    fn character_data(&mut self, data: &[u8]) {
        self.chars.extend_from_slice(data);
    }
    fn external_entity_ref(
        &mut self,
        parsers: &mut ExternalEntities<'_>,
        context: Option<&[u8]>,
        _base: Option<&[u8]>,
        _system_id: Option<&[u8]>,
        _public_id: Option<&[u8]>,
    ) -> bool {
        let Some(mut child) = parsers.create(context, None) else {
            return false;
        };
        let chunks = core::mem::take(&mut self.chunks);
        let mut ok = true;
        for (bytes, last) in &chunks {
            let st = child.parse(self, bytes, *last);
            self.statuses.push(st);
            if st != Status::Ok {
                self.child_codes.push(code_of(&child));
                ok = false;
                break;
            }
        }
        ok
    }
}

const SUB_DOC: &[u8] =
    b"<!DOCTYPE root [\n  <!ENTITY ext SYSTEM 'external.dtd'>\n]>\n<root>&ext;</root>\n";

fn run_sub(chunks: &[(&[u8], bool)]) -> (Status, u32, Sub) {
    let mut p = Parser::new(None, None, SALT);
    p.set_handlers(HandlerSet {
        processing_instruction: true,
        character_data: true,
        external_entity_ref: true,
        ..HandlerSet::default()
    });
    let mut h = Sub {
        chunks: chunks.iter().map(|(b, f)| (b.to_vec(), *f)).collect(),
        ..Sub::default()
    };
    let st = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let st = p.parse(&mut h, SUB_DOC, true);
        (st, code_of(&p))
    }));
    let (st, code) = st.expect("port panicked");
    (st, code, h)
}

/// Upstream `test_misc_empty_nonfinal_subparser_parse`: `""` non-final,
/// then `<?target data?>` final. C 2.9.0 crashes; 2.10.0 and the port
/// return OK and report the processing instruction.
#[test]
fn cve_2026_117610_upstream_test() {
    let (st, code, h) = run_sub(&[(b"", false), (b"<?target data?>", true)]);
    assert_eq!(
        h.statuses,
        [Status::Ok, Status::Ok],
        "child codes {:?}",
        h.child_codes
    );
    assert_eq!((st, code), (Status::Ok, 0));
    assert_eq!(h.pis, [(b"target".to_vec(), b"data".to_vec())]);
}

/// More empty non-final calls, then empty final, then content only.
#[test]
fn cve_2026_117610_variants_no_panic() {
    let cases: &[&[(&[u8], bool)]] = &[
        &[(b"", false), (b"", false), (b"", true)],
        &[(b"", false), (b"", true)],
        &[(b"", false), (b"text", true)],
        &[(b"", false), (b"<a/>", false), (b"", true)],
        &[(b"", false), (b"\xEF\xBB\xBFtext", true)],
        &[(b"", false), (b"\xFF\xFEt\x00", true)],
    ];
    for c in cases {
        let (st, code, h) = run_sub(c);
        eprintln!(
            "cve-117610 variant {:?}: parent {st:?} code {code} child {:?} {:?} chars {:?}",
            c,
            h.statuses,
            h.child_codes,
            String::from_utf8_lossy(&h.chars)
        );
        assert_eq!(
            st,
            Status::Ok,
            "{c:?}: parent code {code}, child {:?}",
            h.child_codes
        );
    }
    // A byte order mark after the empty call is skipped, not reported as
    // U+FEFF character data (2.10.0; the port before #1444 reported it).
    for (chunk, want) in [
        (&b"\xEF\xBB\xBFtext"[..], "text"),
        (&b"\xFF\xFEt\x00"[..], "t"),
    ] {
        let (st, _, h) = run_sub(&[(b"", false), (chunk, true)]);
        assert_eq!(st, Status::Ok);
        assert_eq!(String::from_utf8_lossy(&h.chars), want, "{chunk:?}");
    }
}

/// A text declaration after the empty non-final call. 2.10.0 stays in
/// `externalEntityInitProcessor2` and accepts it as the text declaration.
/// Without the #1444 change the port (2.9.0's control flow) had moved on
/// to content and reported `XML_ERROR_MISPLACED_XML_PI` (17) for the
/// child and `XML_ERROR_EXTERNAL_ENTITY_HANDLING` (21) for the parent.
#[test]
fn cve_2026_117610_text_decl_after_empty_nonfinal() {
    let (st, code, h) = run_sub(&[
        (b"", false),
        (
            b"<?xml version='1.0' encoding='utf-8'?><?target data?>",
            true,
        ),
    ]);
    assert_eq!(
        (st, code),
        (Status::Ok, 0),
        "child {:?} {:?}",
        h.statuses,
        h.child_codes
    );
    assert_eq!(h.pis, [(b"target".to_vec(), b"data".to_vec())]);
}

// ---------------------------------------------------------------------------
// CVE-2026-118370 (#1450): attIndex / nPrefixes overflow in storeAtts.

/// The port counts attributes and prefixes in `usize` (`app.len()`,
/// `n_prefixes`), so no signed overflow exists to trigger; the specified
/// count is still narrowed to C's `int`. The overflow needs > 2^30
/// attributes in one tag (tens of GiB), so this test pins the count on a
/// large but feasible tag (200,000 prefixed attributes) against C instead.
#[test]
fn cve_2026_118370_many_prefixed_attributes_match_c() {
    let n = 200_000usize;
    let mut s = String::from("<r xmlns:p='u'");
    for i in 0..n {
        s.push_str(&format!(" p:a{i}=''"));
    }
    s.push_str("/>");
    let doc = s.as_bytes();

    #[derive(Default)]
    struct Count(Option<i32>, usize);
    impl Handler for Count {
        fn start_element(&mut self, _n: &[u8], atts: &[(&[u8], &[u8])], specified: i32) {
            self.0 = Some(specified);
            self.1 = atts.len();
        }
    }
    let mut p = Parser::new(None, Some(b'|'), SALT);
    p.set_handlers(HandlerSet {
        start_element: true,
        ..HandlerSet::default()
    });
    let mut h = Count::default();
    assert_eq!(p.parse(&mut h, doc, true), Status::Ok);
    assert_eq!(h.0, Some(i32::try_from(2 * n).unwrap_or(-1)));
    assert_eq!(h.1, n);
    assert_eq!(
        p.specified_attribute_count(),
        i32::try_from(2 * n).unwrap_or(-1)
    );

    let c = expat_oracle::parse_trace(
        doc,
        &ParseOptions {
            ns_separator: Some(b'|'),
            handler_mask: Some(HandlerMask::default()),
            ..ParseOptions::default()
        },
    )
    .unwrap_or_default();
    assert_eq!(result_error_code(&c), Some(0));
}
