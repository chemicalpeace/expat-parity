//! Module 11d, step D1: external entities through test handlers, against
//! the C oracle, compared by canonical trace (events, `extref` and
//! `notstandalone` handler calls with all their arguments, error code,
//! line, column, byte index).
//!
//! The library never fetches anything: an external entity is only ever
//! handed to the application's external entity reference handler. The
//! test handler of the shim and of the port traces its arguments (context
//! string from `getContext`, base, system and public identifier) and
//! either fails (`XML_ERROR_EXTERNAL_ENTITY_HANDLING`) or accepts without
//! reading anything. Reading an external entity through an external entity
//! parser is step D2 (not ported yet).
//!
//! Groups (C behaviour, policy and divergences in docs/REPORT.md, module
//! 11d): no handler, failing handler, accepting handler, the
//! not-standalone handler under every parameter-entity-parsing mode and
//! `standalone` value, the foreign DTD (`XML_UseForeignDTD`), context
//! strings with namespaces and open entities, and generated documents
//! under random handler options.

use expat_difftest::docgen::{Rng, entity_document};
use expat_difftest::port::{PortTrace, port_trace};
use expat_difftest::trace::first_difference;
use expat_oracle::{ExtHandler, ParamEntityParsing, ParseOptions, parse_trace};

/// Compares `doc` under `o`; returns the C trace.
fn check(failures: &mut Vec<String>, id: &str, doc: &[u8], o: &ParseOptions) -> Vec<u8> {
    let c = parse_trace(doc, o).expect("oracle");
    match port_trace(doc, o) {
        PortTrace::DtdNotPorted => failures.push(format!("{id} {o:?}: port stopped (DtdNotPorted)")),
        PortTrace::Trace(t) => {
            if let Some((line, l, r)) = first_difference(&c, &t) {
                failures.push(format!("{id} {o:?}: line {line}: C `{l}` port `{r}`"));
            }
        }
    }
    c
}

fn assert_clean(what: &str, pairs: usize, failures: &[String]) {
    eprintln!("{what}: {pairs} pairs compared, {} mismatches", failures.len());
    for f in failures.iter().take(20) {
        eprintln!("{f}");
    }
    assert!(failures.is_empty(), "{what}: {} mismatches", failures.len());
}

fn result_code(trace: &[u8]) -> i64 {
    let s = String::from_utf8_lossy(trace);
    s.lines()
        .rev()
        .find(|l| l.starts_with("result "))
        .and_then(|l| l.split(' ').nth(2))
        .and_then(|c| c.parse().ok())
        .unwrap_or(-1)
}

fn count_lines(trace: &[u8], prefix: &str) -> usize {
    String::from_utf8_lossy(trace)
        .lines()
        .filter(|l| l.starts_with(prefix))
        .count()
}

const PEPS: [ParamEntityParsing; 3] = [
    ParamEntityParsing::Never,
    ParamEntityParsing::UnlessStandalone,
    ParamEntityParsing::Always,
];

/// Every combination of handler options with parameter-entity parsing,
/// namespaces and feeding (whole, 1-byte and 7-byte chunks, buffer API).
fn matrix() -> Vec<ParseOptions> {
    let mut v = Vec::new();
    for ext in [ExtHandler::None, ExtHandler::Fail, ExtHandler::Accept] {
        for not_standalone in [None, Some(true), Some(false)] {
            for pep in PEPS {
                for (i, (chunk, buffer_api, ns)) in [(0, false, None), (1, false, Some(b'|')), (7, true, None)]
                    .into_iter()
                    .enumerate()
                {
                    v.push(ParseOptions {
                        ext_handler: ext,
                        not_standalone,
                        param_entity_parsing: pep,
                        chunk_size: chunk,
                        buffer_api,
                        ns_separator: ns,
                        use_foreign_dtd: i == 2,
                        ..ParseOptions::default()
                    });
                }
            }
        }
    }
    v
}

/// (id, document). External entities of every kind and position.
const CASES: &[(&str, &str)] = &[
    ("no-dtd", "<r>x</r>"),
    ("internal-subset-only", "<!DOCTYPE r [<!ENTITY e 'v'>]><r>&e;</r>"),
    ("external-subset", "<!DOCTYPE r SYSTEM 'r.dtd'><r/>"),
    ("external-subset-public", "<!DOCTYPE r PUBLIC '-//T//X' 'r.dtd'><r/>"),
    ("external-subset-and-internal", "<!DOCTYPE r SYSTEM 'r.dtd' [<!ENTITY e 'v'>]><r>&e;</r>"),
    (
        "external-subset-standalone",
        "<?xml version='1.0' standalone='yes'?><!DOCTYPE r SYSTEM 'r.dtd'><r/>",
    ),
    (
        "external-subset-not-standalone",
        "<?xml version='1.0' standalone='no'?><!DOCTYPE r SYSTEM 'r.dtd'><r/>",
    ),
    ("content-external", "<!DOCTYPE r [<!ENTITY x SYSTEM 'x.xml'>]><r>a&x;b</r>"),
    (
        "content-external-public",
        "<!DOCTYPE r [<!ENTITY x PUBLIC '-//P//E' 'x.xml'>]><r>&x;&x;</r>",
    ),
    (
        "content-external-in-internal",
        "<!DOCTYPE r [<!ENTITY x SYSTEM 'x.xml'><!ENTITY i '<a>&x;</a>'>]><r>&i;</r>",
    ),
    (
        "content-external-nested-open",
        "<!DOCTYPE r [<!ENTITY x SYSTEM 'x.xml'><!ENTITY i '&j;'><!ENTITY j 'J&x;'>]><r>&i;</r>",
    ),
    (
        "content-external-unparsed",
        "<!DOCTYPE r [<!NOTATION n SYSTEM 'n'><!ENTITY u SYSTEM 'u' NDATA n>]><r>&u;</r>",
    ),
    (
        "attribute-external",
        "<!DOCTYPE r [<!ENTITY x SYSTEM 'x.xml'>]><r a='&x;'/>",
    ),
    (
        "pe-external-between-decls",
        "<!DOCTYPE r [<!ENTITY % p SYSTEM 'p.ent'>%p;<!ENTITY e 'after'>]><r>&e;</r>",
    ),
    (
        "pe-external-then-undeclared",
        "<!DOCTYPE r [<!ENTITY % p SYSTEM 'p.ent'>%p;]><r>&undeclared;</r>",
    ),
    (
        "pe-external-standalone",
        "<?xml version='1.0' standalone='yes'?><!DOCTYPE r [<!ENTITY % p SYSTEM 'p.ent'>%p;<!ENTITY e 'v'>]><r>&e;</r>",
    ),
    (
        "pe-external-in-entity-value",
        "<!DOCTYPE r [<!ENTITY % p SYSTEM 'p.ent'><!ENTITY % q '%p;'>%q;<!ENTITY e 'v'>]><r>&e;</r>",
    ),
    (
        "pe-undeclared",
        "<!DOCTYPE r [%p;<!ENTITY e 'v'>]><r>&e;</r>",
    ),
    (
        "pe-undeclared-after-external-subset",
        "<!DOCTYPE r SYSTEM 'r.dtd' [%p;<!ENTITY e 'v'>]><r>&e;&f;</r>",
    ),
    (
        "attlist-after-external-pe",
        "<!DOCTYPE r [<!ENTITY % p SYSTEM 'p.ent'>%p;<!ATTLIST r d CDATA 'dflt'>]><r/>",
    ),
    (
        "namespaces-context",
        "<!DOCTYPE r [<!ENTITY x SYSTEM 'x.xml'>]><r xmlns='urn:d' xmlns:a='urn:a' xmlns:b='urn:b'><a:e xmlns:c='urn:c'>&x;</a:e></r>",
    ),
    (
        "namespaces-unbound-default",
        "<!DOCTYPE r [<!ENTITY x SYSTEM 'x.xml'>]><r xmlns='urn:d'><e xmlns=''>&x;</e></r>",
    ),
    (
        "external-error-position",
        "<!DOCTYPE r [<!ENTITY x SYSTEM 'x.xml'>]>\n<r>\n  <a>text &x; more</a>\n</r>",
    ),
];

/// Every curated case under the whole matrix; the expected handler calls
/// are asserted on C's trace so the group documents C's behaviour.
#[test]
fn curated_external_cases_match_c() {
    let opts = matrix();
    let mut failures = Vec::new();
    let mut pairs = 0;
    let mut extref = 0;
    let mut notstandalone = 0;
    let mut codes = std::collections::BTreeMap::new();
    for (id, doc) in CASES {
        for o in &opts {
            let c = check(&mut failures, id, doc.as_bytes(), o);
            pairs += 1;
            extref += count_lines(&c, "extref");
            notstandalone += count_lines(&c, "notstandalone");
            *codes.entry(result_code(&c)).or_insert(0usize) += 1;
        }
    }
    eprintln!("codes {codes:?}, extref calls {extref}, notstandalone calls {notstandalone}");
    assert_clean("curated external cases", pairs, &failures);
    // The group reaches the handler error (21), the not-standalone error
    // (22), the attribute error (16) and success.
    for code in [0, 16, 21, 22] {
        assert!(codes.contains_key(&code), "no case ended with {code}");
    }
}

/// C's behaviour pinned on single pairs (ParamEntityParsing::Always,
/// whole document): what the handler sees and what the result is.
#[test]
fn pinned_external_behaviour() {
    let always = |ext, ns: Option<u8>, not_standalone| ParseOptions {
        ext_handler: ext,
        ns_separator: ns,
        not_standalone,
        param_entity_parsing: ParamEntityParsing::Always,
        ..ParseOptions::default()
    };
    let doc = |id: &str| CASES.iter().find(|c| c.0 == id).map(|c| c.1.as_bytes()).unwrap();
    let mut failures = Vec::new();
    let mut pin = |id: &str, o: ParseOptions, want_code: i64, want_lines: &[&str]| {
        let c = check(&mut failures, id, doc(id), &o);
        let s = String::from_utf8_lossy(&c).into_owned();
        assert_eq!(result_code(&c), want_code, "{id}: {s}");
        for w in want_lines {
            assert!(s.lines().any(|l| l == *w), "{id}: no line `{w}` in\n{s}");
        }
    };
    // No handler: nothing is read and nothing fails.
    pin("content-external", always(ExtHandler::None, None, None), 0, &[]);
    // A failing handler stops the parse with 21.
    pin(
        "content-external",
        always(ExtHandler::Fail, None, None),
        21,
        &["extref x ~ x.xml ~"],
    );
    // The external subset is offered with a NULL context and the name `#`
    // never appears.
    pin(
        "external-subset-public",
        always(ExtHandler::Accept, None, None),
        0,
        &["extref ~ ~ r.dtd -//T//X"],
    );
    // Namespace context: default namespace first, then the bound prefixes
    // in hash-table slot order, separated by form feeds (%0C).
    pin(
        "namespaces-context",
        always(ExtHandler::Accept, Some(b'|'), None),
        0,
        &["extref =urn:d%0Cb=urn:b%0Cxml=http://www.w3.org/XML/1998/namespace%0Cc=urn:c%0Ca=urn:a%0Cx ~ x.xml ~"],
    );
    // Open entities: every open general entity, in slot order.
    pin(
        "content-external-nested-open",
        always(ExtHandler::Accept, None, None),
        0,
        &["extref i%0Cx%0Cj ~ x.xml ~"],
    );
    // The not-standalone handler refusing after an external PE was offered
    // but not read: not called (paramEntityRead stays false).
    pin(
        "pe-external-between-decls",
        always(ExtHandler::Accept, None, Some(false)),
        0,
        &["extref ~ ~ p.ent ~"],
    );
    // Without parameter entity parsing the not-standalone handler decides
    // at the external subset's system identifier.
    pin(
        "external-subset",
        ParseOptions {
            not_standalone: Some(false),
            ..ParseOptions::default()
        },
        22,
        &["notstandalone"],
    );
    assert_clean("pinned external behaviour", 7, &failures);
}

/// Generated entity documents (which contain external general and
/// parameter entities, unparsed entities and external subsets) under
/// random handler options.
#[test]
fn generated_documents_with_handlers_match_c() {
    let mut r = Rng::new(0x11d_e7e7);
    let mut failures = Vec::new();
    let mut codes = std::collections::BTreeMap::new();
    let mut extref = 0;
    let mut pairs = 0;
    for i in 0..6_000 {
        let doc = entity_document(&mut r);
        let o = ParseOptions {
            ext_handler: [ExtHandler::None, ExtHandler::Fail, ExtHandler::Accept][r.below(3)],
            not_standalone: [None, Some(true), Some(false)][r.below(3)],
            param_entity_parsing: PEPS[r.below(3)],
            use_foreign_dtd: r.one_in(4),
            ns_separator: if r.one_in(3) { Some(b':') } else { None },
            chunk_size: [0, 1, 3, 64][r.below(4)],
            buffer_api: r.one_in(4),
            ..ParseOptions::default()
        };
        let c = check(&mut failures, &format!("gen-{i}"), &doc, &o);
        extref += count_lines(&c, "extref");
        *codes.entry(result_code(&c)).or_insert(0usize) += 1;
        pairs += 1;
    }
    eprintln!("codes {codes:?}, extref calls {extref}");
    assert_clean("generated documents with handlers", pairs, &failures);
    assert!(extref > 300, "the generator must reach the handler");
    for code in [0, 21, 22] {
        assert!(codes.contains_key(&code), "generator never produced error code {code}");
    }
}
