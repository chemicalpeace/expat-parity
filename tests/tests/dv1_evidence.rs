//! DV-1 evidence (scratch branch `dv1-evidence`, not for main): measures the
//! port's tracker surplus over libexpat per document and searches the
//! default-limit window in which the port refuses (XML_ERROR_NO_MEMORY) a
//! document that libexpat accepts. Run with
//! `cargo test --release -p expat-difftest --test dv1_evidence -- --nocapture --test-threads=1`.

use std::time::Duration;

use expat_difftest::bounded::{Bounds, CountingAlloc, compare};
use expat_difftest::trace::result_error_code;
use expat_difftest::tracker;
use expat_oracle::{ParamEntityParsing, ParseOptions};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

const NO_MEMORY: u32 = 1;

const WIDE: Bounds = Bounds {
    max_input: 2 << 20,
    timeout: Duration::from_secs(100),
    port_heap_cap: 512 << 20,
    c_heap_cap: 512 << 20,
};

fn base() -> ParseOptions {
    ParseOptions {
        param_entity_parsing: ParamEntityParsing::Always,
        ..ParseOptions::default()
    }
}

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

fn extra_documents() -> Vec<(&'static str, Vec<u8>)> {
    let mut v: Vec<(&'static str, Vec<u8>)> = vec![("empty root", b"<r/>".to_vec())];
    let mut long_tags = Vec::new();
    for i in 0..200 {
        long_tags.extend_from_slice(format!("<{}{i}>", "t".repeat(40)).as_bytes());
    }
    for i in (0..200).rev() {
        long_tags.extend_from_slice(format!("</{}{i}>", "t".repeat(40)).as_bytes());
    }
    v.push(("200 nested 42-char tags", long_tags));
    v.push((
        "attribute expansion 395x1000",
        attribute_expansion(0, 395, 1000),
    ));
    v
}

/// `pad` spaces, then an attribute value of `refs` references to an
/// entity of `len` characters.
fn attribute_expansion(pad: usize, len: usize, refs: usize) -> Vec<u8> {
    let mut d = vec![b' '; pad];
    d.extend_from_slice(
        format!("<!DOCTYPE r [<!ENTITY ee '{}'>]><r v='", "x".repeat(len)).as_bytes(),
    );
    for _ in 0..refs {
        d.extend_from_slice(b"&ee;");
    }
    d.extend_from_slice(b"'/>");
    d
}

#[test]
fn surplus_per_document() {
    let mut docs = sweep_documents();
    docs.extend(extra_documents());
    for o in [
        base(),
        ParseOptions {
            ns_separator: Some(b'|'),
            ..base()
        },
    ] {
        for (name, doc) in &docs {
            let r = tracker::run(doc, &o).expect("run");
            let creation = r.port_tracker.created as i64 - r.c_tracker.created as i64;
            let margin = r.port_tracker.end as i64 - r.c_tracker.end as i64;
            println!(
                "SURPLUS ns={} | {name:<28} | C created {:>5} end {:>9} | port created {:>5} end {:>9} | margin {:>6} = creation {:>6} + rest {:>5} | C code {:?} port code {:?}",
                o.ns_separator.is_some(),
                r.c_tracker.created,
                r.c_tracker.end,
                r.port_tracker.created,
                r.port_tracker.end,
                margin,
                creation,
                margin - creation,
                result_error_code(&r.c),
                result_error_code(&r.port)
            );
        }
    }
}

fn outcome(pad: usize) -> (Option<u32>, Option<u32>, u64, u64, u64) {
    let doc = attribute_expansion(pad, 395, 85_000);
    let out = compare(&format!("pad{pad}"), &doc, &base(), WIDE);
    (
        result_error_code(&out.c),
        result_error_code(&out.port),
        out.c_tracker.end,
        out.port_tracker.end,
        out.port_tracker.peak,
    )
}

/// Binary search for the smallest padding at which C accepts the
/// 85,000 x 395 attribute document at the default limits, then the port's
/// outcome across the boundary.
#[test]
fn default_limit_window() {
    let (mut lo, mut hi) = (0usize, 600_000usize); // C refuses at lo, accepts at hi
    let (c, ..) = outcome(lo);
    assert_eq!(c, Some(NO_MEMORY));
    let (c, ..) = outcome(hi);
    assert_eq!(c, Some(0), "C must accept with {hi} bytes of padding");
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        let doc = attribute_expansion(mid, 395, 85_000);
        let mut o = base();
        o.heap_cap = WIDE.c_heap_cap;
        let t = expat_oracle::parse_trace(&doc, &o).expect("oracle");
        if result_error_code(&t) == Some(0) {
            hi = mid
        } else {
            lo = mid
        }
    }
    let direct_doc = attribute_expansion(hi, 395, 85_000).len();
    println!(
        "WINDOW C boundary: refuses at pad {lo}, accepts at pad {hi} (document {direct_doc} bytes)"
    );
    let mut first_port_accept = None;
    for pad in (lo.saturating_sub(2)..hi + 200).step_by(1) {
        let (c, p, ce, pe, pp) = outcome(pad);
        let interesting =
            pad <= hi + 2 || (first_port_accept.is_none() && p == Some(0)) || pad % 25 == 0;
        if interesting {
            println!(
                "WINDOW pad {pad:>6} doc {:>7} B | C {c:?} tracker end {ce:>9} | port {p:?} tracker end {pe:>9} peak {pp:>9}",
                attribute_expansion(pad, 395, 85_000).len()
            );
        }
        if p == Some(0) && first_port_accept.is_none() {
            first_port_accept = Some(pad);
            break;
        }
    }
    println!(
        "WINDOW port first accepts at pad {first_port_accept:?}; window = pads {hi}..{:?}",
        first_port_accept
    );
}

fn distinct_names(k_elems: usize, n_atts: usize, used: bool) -> Vec<u8> {
    let mut names = b"<!DOCTYPE e [".to_vec();
    for k in 0..k_elems {
        names.extend_from_slice(format!("<!ATTLIST e{k}").as_bytes());
        for j in 0..n_atts {
            names.extend_from_slice(format!(" a{k}x{j} CDATA 'v{j}'").as_bytes());
        }
        names.push(b'>');
    }
    names.extend_from_slice(b"]><e>");
    if used {
        for k in 0..k_elems {
            names.extend_from_slice(format!("<e{k} a{k}x0='w'/>").as_bytes());
        }
    }
    names.extend_from_slice(b"</e>");
    names
}

/// Whether the non-creation part of the port's charge can fall below C's
/// and by how much as the DTD grows.
#[test]
fn deficit_scaling() {
    for &(k, n, used) in &[
        (8usize, 60usize, true),
        (8, 60, false),
        (1, 60, false),
        (1, 500, false),
        (1, 2000, false),
        (16, 60, false),
        (64, 60, false),
        (256, 60, false),
        (64, 200, false),
        (64, 200, true),
        (1024, 8, false),
        (4096, 2, false),
        (4096, 2, true),
    ] {
        let doc = distinct_names(k, n, used);
        let r = tracker::run(&doc, &base()).expect("run");
        let creation = r.port_tracker.created as i64 - r.c_tracker.created as i64;
        let margin = r.port_tracker.end as i64 - r.c_tracker.end as i64;
        println!(
            "DEFICIT elems {k:>5} atts {n:>5} used {used} | doc {:>8} B | C end {:>9} port end {:>9} | margin {:>7} rest {:>7} | codes {:?}/{:?}",
            doc.len(),
            r.c_tracker.end,
            r.port_tracker.end,
            margin,
            margin - creation,
            result_error_code(&r.c),
            result_error_code(&r.port)
        );
    }
}

/// C allocation trace (run with EXPAT_MALLOC_DEBUG=2): one document chosen
/// by DV1_DOC = "names:K:N" | "pad:P".
#[test]
fn c_alloc_trace() {
    let Ok(spec) = std::env::var("DV1_DOC") else {
        return;
    };
    let parts: Vec<&str> = spec.split(':').collect();
    let doc = match parts.as_slice() {
        ["names", k, n] => distinct_names(k.parse().unwrap(), n.parse().unwrap(), false),
        ["pad", p] => attribute_expansion(p.parse().unwrap(), 395, 85_000),
        ["empty"] => b"<r/>".to_vec(),
        _ => panic!("bad DV1_DOC"),
    };
    let mut o = base();
    o.heap_cap = WIDE.c_heap_cap;
    let t = expat_oracle::parse_trace(&doc, &o).expect("oracle");
    let tb = expat_oracle::last_tracker_bytes();
    eprintln!(
        "CTRACE_DONE code {:?} created {} end {}",
        result_error_code(&t),
        tb.created,
        tb.end
    );
    if parts.first() == Some(&"pad") || parts.first() == Some(&"empty") {
        let p = expat_difftest::port::port_trace(&doc, &o);
        let pt = expat_difftest::port::last_port_tracker();
        eprintln!(
            "PORT_DONE code {:?} created {} end {} peak {}",
            result_error_code(&p),
            pt.created,
            pt.end,
            pt.peak
        );
    }
}

/// `pad` spaces, a DTD with `ndef` attribute defaults on an element type
/// `z` that never occurs, and the 85,000 x 395 attribute expansion.
fn window_doc(pad: usize, ndef: usize) -> Vec<u8> {
    let mut d = vec![b' '; pad];
    d.extend_from_slice(format!("<!DOCTYPE r [<!ENTITY ee '{}'>", "x".repeat(395)).as_bytes());
    if ndef > 0 {
        d.extend_from_slice(b"<!ATTLIST z");
        for j in 0..ndef {
            d.extend_from_slice(format!(" a{j} CDATA 'v'").as_bytes());
        }
        d.push(b'>');
    }
    d.extend_from_slice(b"]><r v='");
    for _ in 0..85_000 {
        d.extend_from_slice(b"&ee;");
    }
    d.extend_from_slice(b"'/>");
    d
}

fn c_accepts(doc: &[u8]) -> bool {
    let mut o = base();
    o.heap_cap = WIDE.c_heap_cap;
    let t = expat_oracle::parse_trace(doc, &o).expect("oracle");
    match result_error_code(&t) {
        Some(0) => true,
        Some(NO_MEMORY) => false,
        c => panic!("C code {c:?}"),
    }
}

fn port_accepts(doc: &[u8]) -> bool {
    let t = expat_difftest::port::port_trace(doc, &base());
    match result_error_code(&t) {
        Some(0) => true,
        Some(NO_MEMORY) => false,
        c => panic!("port code {c:?}"),
    }
}

/// Smallest padding in lo..hi accepted by `f` (monotone; refuses at lo).
fn boundary(mut lo: usize, mut hi: usize, ndef: usize, f: fn(&[u8]) -> bool) -> usize {
    assert!(!f(&window_doc(lo, ndef)), "accepted at lo {lo}");
    assert!(f(&window_doc(hi, ndef)), "refused at hi {hi}");
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if f(&window_doc(mid, ndef)) {
            hi = mid
        } else {
            lo = mid
        }
    }
    hi
}

/// DV1_NDEF=n (default 0): both boundaries, then a full differential run
/// (traces + tracker counts) at each padding inside the window and at its
/// edges.
#[test]
fn window_both_sides() {
    let Ok(ndef) = std::env::var("DV1_NDEF") else {
        return;
    };
    let ndef: usize = ndef.parse().unwrap();
    let t0 = std::time::Instant::now();
    let cb = boundary(0, 400_000, ndef, c_accepts);
    let pb = boundary(0, 400_000, ndef, port_accepts);
    println!(
        "WIN ndef {ndef}: C accepts from pad {cb}, port accepts from pad {pb} ({:?})",
        t0.elapsed()
    );
    let (a, b) = (cb.min(pb), cb.max(pb));
    let mut pads: Vec<usize> = vec![
        a.saturating_sub(1),
        a,
        a + 1,
        (a + b) / 2,
        b.saturating_sub(1),
        b,
        b + 1,
    ];
    pads.dedup();
    for pad in pads {
        let doc = window_doc(pad, ndef);
        let out = compare(&format!("w{pad}"), &doc, &base(), WIDE);
        println!(
            "WIN pad {pad:>6} doc {:>7} B | C {:?} tracker created {} end {:>9} | port {:?} tracker created {} end {:>9} peak {:>9} | equal traces {}",
            doc.len(),
            result_error_code(&out.c),
            out.c_tracker.created,
            out.c_tracker.end,
            result_error_code(&out.port),
            out.port_tracker.created,
            out.port_tracker.end,
            out.port_tracker.peak,
            out.equal()
        );
    }
}

/// Bindings: `depth` nested elements each declaring a new prefix.
#[test]
fn binding_scaling() {
    for depth in [100usize, 600, 3000, 10000] {
        let mut d = Vec::new();
        for i in 0..depth {
            d.extend_from_slice(format!("<p{i}:e xmlns:p{i}='urn:x'>").as_bytes());
        }
        for i in (0..depth).rev() {
            d.extend_from_slice(format!("</p{i}:e>").as_bytes());
        }
        for ns in [false, true] {
            let o = ParseOptions {
                ns_separator: ns.then_some(b'|'),
                ..base()
            };
            let r = tracker::run(&d, &o).expect("run");
            let margin = r.port_tracker.end as i64 - r.c_tracker.end as i64;
            println!(
                "BINDING depth {depth:>5} ns {ns} | doc {:>7} | C end {:>9} port end {:>9} | margin {:>7} rest {:>7}",
                d.len(),
                r.c_tracker.end,
                r.port_tracker.end,
                margin,
                margin - 13344
            );
        }
    }
}

/// Bindings on one element type: one start tag with `n` prefix declarations.
#[test]
fn binding_one_element() {
    for n in [100usize, 1000, 5000] {
        let mut d = b"<e".to_vec();
        for i in 0..n {
            d.extend_from_slice(format!(" xmlns:p{i}='u'").as_bytes());
        }
        d.extend_from_slice(b"/>");
        let o = ParseOptions {
            ns_separator: Some(b'|'),
            ..base()
        };
        let r = tracker::run(&d, &o).expect("run");
        let margin = r.port_tracker.end as i64 - r.c_tracker.end as i64;
        println!(
            "BIND1 n {n:>5} | doc {:>7} | C end {:>9} port end {:>9} | margin {:>7} rest {:>7}",
            d.len(),
            r.c_tracker.end,
            r.port_tracker.end,
            margin,
            margin - 13344
        );
    }
}

/// Pool slack at a block boundary: an attribute value of `refs` x 395
/// expanded characters plus `tail` literal characters.
fn slack_doc(refs: usize, tail: usize) -> Vec<u8> {
    let ent = if std::env::var("DV1_MB").is_ok() { format!("{}\u{e9}", "x".repeat(393)) } else { "x".repeat(395) };
    let mut d = format!("<!DOCTYPE r [<!ENTITY ee '{ent}'>]><r v='").into_bytes();
    for _ in 0..refs {
        d.extend_from_slice(b"&ee;");
    }
    if std::env::var("DV1_MB").is_ok() { d.extend_from_slice("\u{e9}".as_bytes()); }
    d.extend(std::iter::repeat_n(b'x', tail));
    d.extend_from_slice(b"'/>");
    d
}

#[test]
fn pool_slack_boundary() {
    let Ok(spec) = std::env::var("DV1_SLACK") else { return };
    let mut it = spec.split(':').map(|x| x.parse::<usize>().unwrap());
    let (refs, t0, t1) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
    for tail in t0..=t1 {
        let doc = slack_doc(refs, tail);
        let out = compare(&format!("slack{tail}"), &doc, &base(), WIDE);
        println!(
            "SLACK refs {refs} tail {tail} value {} chars doc {} B | C {:?} end {:>9} | port {:?} end {:>9} peak {:>9}",
            refs * 395 + tail, doc.len(), result_error_code(&out.c), out.c_tracker.end,
            result_error_code(&out.port), out.port_tracker.end, out.port_tracker.peak
        );
    }
}
