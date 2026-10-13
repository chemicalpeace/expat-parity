//! Module 11b: internal entity expansion against the C oracle, compared by
//! canonical trace (events, error code, line, column, byte index).
//!
//! Groups (expected C behaviour and policy in docs/REPORT.md, module 11b):
//! expansion in content, in attribute values, in default attribute values,
//! through parameter entities (between declarations and inside entity
//! values), direct and indirect recursion in every one of these contexts,
//! proper nesting (`XML_ERROR_ASYNC_ENTITY`), reference chains 10,000 deep
//! (expansion is iterative in C and in the port, so depth costs heap, not
//! stack), and random entity graphs.

use expat_difftest::docgen::{Rng, entity_document};
use expat_difftest::port::port_trace;
use expat_difftest::trace::first_difference;
use expat_oracle::{ParamEntityParsing, ParseOptions, parse_trace};

/// Compares `doc` under `o`; returns the C trace.
fn check(failures: &mut Vec<String>, id: &str, doc: &[u8], o: &ParseOptions) -> Vec<u8> {
    let c = parse_trace(doc, o).expect("oracle");
    let t = port_trace(doc, o);
    if let Some((line, l, r)) = first_difference(&c, &t) {
        failures.push(format!("{id} {o:?}: line {line}: C `{l}` port `{r}`"));
    }
    c
}

fn assert_clean(what: &str, pairs: usize, failures: &[String]) {
    eprintln!(
        "{what}: {pairs} pairs compared, {} mismatches",
        failures.len()
    );
    for f in failures.iter().take(20) {
        eprintln!("{f}");
    }
    assert!(failures.is_empty(), "{what}: {} mismatches", failures.len());
}

/// Every parameter-entity-parsing mode, whole and chunked, with and without
/// namespaces, plus the buffer API.
fn options() -> Vec<ParseOptions> {
    let mut v = Vec::new();
    for pe in [
        ParamEntityParsing::Never,
        ParamEntityParsing::UnlessStandalone,
        ParamEntityParsing::Always,
    ] {
        for chunk in [0usize, 1, 7] {
            v.push(ParseOptions {
                param_entity_parsing: pe,
                chunk_size: chunk,
                ..ParseOptions::default()
            });
        }
        v.push(ParseOptions {
            param_entity_parsing: pe,
            ns_separator: Some(b':'),
            chunk_size: 3,
            buffer_api: true,
            ..ParseOptions::default()
        });
    }
    v
}

/// The error code in the `result` line of a trace.
fn result_code(trace: &[u8]) -> i64 {
    let text = String::from_utf8_lossy(trace);
    let line = text.lines().last().unwrap_or_default().to_string();
    line.split(' ')
        .nth(2)
        .and_then(|c| c.parse().ok())
        .unwrap_or(-1)
}

const ALWAYS: ParseOptions = ParseOptions {
    encoding: None,
    ns_separator: None,
    chunk_size: 0,
    param_entity_parsing: ParamEntityParsing::Always,
    buffer_api: false,
    no_reparse_deferral: false,
    ns_triplets: false,
    limits: expat_oracle::Limits::NONE,
    text_digest: false,
    trace_accounting: false,
    heap_cap: 0,
    ext_handler: expat_oracle::ExtHandler::None,
    not_standalone: None,
    use_foreign_dtd: false,
    unparsed_handler: false,
    ext_entities: Vec::new(),
    ext_encoding: None,
};

/// (id, document, expected C error code with parameter entity parsing
/// `Always`, whole document). The expected code pins C's behaviour so the
/// group documents it; the port is compared under every option set.
const CASES: &[(&str, &str, i64)] = &[
    // --- content
    (
        "content-text",
        "<!DOCTYPE r [<!ENTITY e 'x y'>]><r>&e;</r>",
        0,
    ),
    (
        "content-markup",
        "<!DOCTYPE r [<!ENTITY e '<a b=\"1\">t<c/></a><!--k--><?p d?><![CDATA[x<]]>'>]><r>&e;&e;</r>",
        0,
    ),
    (
        "content-nested",
        "<!DOCTYPE r [<!ENTITY a 'A&b;A'><!ENTITY b 'B&c;B'><!ENTITY c 'C'>]><r>&a;</r>",
        0,
    ),
    (
        "content-charref",
        "<!DOCTYPE r [<!ENTITY e '&#38;#60;x&#38;#62;'>]><r>&e;</r>",
        0,
    ),
    (
        "content-predef",
        "<!DOCTYPE r [<!ENTITY e '&lt;&amp;&#65;'>]><r>&e;</r>",
        0,
    ),
    (
        "content-cr",
        "<!DOCTYPE r [<!ENTITY e 'a&#13;&#10;b&#13;c'>]><r>&e;</r>",
        0,
    ),
    (
        "content-sequential-twice",
        "<!DOCTYPE r [<!ENTITY e 'x'><!ENTITY f '&e;&e;'>]><r>&f;&e;&f;</r>",
        0,
    ),
    (
        "content-first-decl-wins",
        "<!DOCTYPE r [<!ENTITY e 'first'><!ENTITY e 'second'>]><r>&e;</r>",
        0,
    ),
    (
        "content-undeclared-in-entity",
        "<!DOCTYPE r [<!ENTITY e '&u;'>]><r>&e;</r>",
        11,
    ),
    (
        "content-undeclared-with-pe",
        "<!DOCTYPE r [<!ENTITY % p ''>%p;<!ENTITY e 'x&u;y'>]><r>&e;</r>",
        0,
    ),
    (
        "content-unparsed",
        "<!DOCTYPE r [<!NOTATION n SYSTEM 'n'><!ENTITY u SYSTEM 'u' NDATA n><!ENTITY e '&u;'>]><r>&e;</r>",
        15,
    ),
    (
        "content-external-in-entity",
        "<!DOCTYPE r [<!ENTITY x SYSTEM 'x'><!ENTITY e 'a&x;b'>]><r>&e;</r>",
        0,
    ),
    (
        "content-bad-markup-in-entity",
        "<!DOCTYPE r [<!ENTITY e '<a'>]><r>&e;</r>",
        5,
    ),
    (
        "content-invalid-char-in-entity",
        "<!DOCTYPE r [<!ENTITY e '&#38;#0;'>]><r>&e;</r>",
        14,
    ),
    (
        "content-entity-in-epilog",
        "<!DOCTYPE r [<!ENTITY e 'x'>]><r/>&e;",
        4,
    ),
    (
        "content-entity-before-root",
        "<!DOCTYPE r [<!ENTITY e '<r/>'>]>&e;",
        4,
    ),
    (
        "content-root-from-entity",
        "<!DOCTYPE r [<!ENTITY e 'x'>]><r>&e;</r>",
        0,
    ),
    (
        "content-empty-entity",
        "<!DOCTYPE r [<!ENTITY e ''>]><r>a&e;b</r>",
        0,
    ),
    (
        "content-error-after-entity",
        "<!DOCTYPE r [<!ENTITY e 'xx'>]><r>&e;<</r>",
        4,
    ),
    (
        "content-mismatch-in-entity",
        "<!DOCTYPE r [<!ENTITY e '<a></b>'>]><r>&e;</r>",
        7,
    ),
    // --- proper nesting
    (
        "async-open",
        "<!DOCTYPE r [<!ENTITY e '<a>'>]><r>&e;</a></r>",
        13,
    ),
    (
        "async-close",
        "<!DOCTYPE r [<!ENTITY e '</a>'>]><r><a>&e;</r>",
        13,
    ),
    (
        "async-nested",
        "<!DOCTYPE r [<!ENTITY e '<b>&f;'><!ENTITY f '</b>'>]><r>&e;</r>",
        13,
    ),
    (
        "async-split-pair",
        "<!DOCTYPE r [<!ENTITY o '<a>'><!ENTITY c '</a>'>]><r>&o;&c;</r>",
        13,
    ),
    // The entity ends right after an inner reference while its start tag
    // is still open: the check runs when the inner entity closes
    // (`processInternalEntity`, `xmlparse.c`; injected bug I2 was only
    // caught by generated documents before these cases).
    (
        "async-open-then-empty-ref",
        "<!DOCTYPE r [<!ENTITY e '<a>&f;'><!ENTITY f ''>]><r>&e;</a></r>",
        13,
    ),
    (
        "async-open-then-text-ref",
        "<!DOCTYPE r [<!ENTITY e '<a>&f;'><!ENTITY f 'x'>]><r>&e;</a></r>",
        13,
    ),
    (
        "async-inner-opens-at-end",
        "<!DOCTYPE r [<!ENTITY e '<i>t</i>&f;'><!ENTITY f '<b><i/>'>]><r>&e;</b></r>",
        13,
    ),
    // --- attribute values
    (
        "attr-text",
        "<!DOCTYPE r [<!ENTITY e 'v'>]><r a='x&e;y'/>",
        0,
    ),
    (
        "attr-nested",
        "<!DOCTYPE r [<!ENTITY e '1&f;2'><!ENTITY f '3&amp;4'>]><r a='&e;&e;'/>",
        0,
    ),
    (
        "attr-whitespace-cdata",
        "<!DOCTYPE r [<!ENTITY e '  a\tb&#10;c  '>]><r a=' &e; '/>",
        0,
    ),
    (
        "attr-whitespace-nmtokens",
        "<!DOCTYPE r [<!ATTLIST r a NMTOKENS #IMPLIED><!ENTITY e '  a  b  '>]><r a=' &e; &e; '/>",
        0,
    ),
    (
        "attr-lt-in-entity",
        "<!DOCTYPE r [<!ENTITY e '&#38;#60;'>]><r a='&e;'/>",
        0,
    ),
    (
        "attr-raw-lt-in-entity",
        "<!DOCTYPE r [<!ENTITY e '<'>]><r a='&e;'/>",
        4,
    ),
    (
        "attr-undeclared-in-entity",
        "<!DOCTYPE r [<!ENTITY e '&u;'>]><r a='&e;'/>",
        11,
    ),
    (
        "attr-undeclared-with-pe",
        "<!DOCTYPE r [%p;<!ENTITY e 'x&u;'>]><r a='&e;'/>",
        0,
    ),
    (
        "attr-external",
        "<!DOCTYPE r [<!ENTITY x SYSTEM 'x'><!ENTITY e '&x;'>]><r a='&e;'/>",
        16,
    ),
    (
        "attr-unparsed",
        "<!DOCTYPE r [<!NOTATION n SYSTEM 'n'><!ENTITY u SYSTEM 'u' NDATA n>]><r a='&u;'/>",
        15,
    ),
    (
        "attr-bad-charref-in-entity",
        "<!DOCTYPE r [<!ENTITY e '&#38;#xD800;'>]><r a='&e;'/>",
        14,
    ),
    (
        "attr-ns-from-entity",
        "<!DOCTYPE r [<!ENTITY u 'urn:x'>]><p:r xmlns:p='&u;' p:a='&u;'/>",
        0,
    ),
    (
        "attr-duplicate-after-entity",
        "<!DOCTYPE r [<!ENTITY e 'v'>]><r a='&e;' a='w'/>",
        8,
    ),
    // --- default attribute values
    (
        "default-text",
        "<!DOCTYPE r [<!ENTITY e 'v'><!ATTLIST r a CDATA '&e;x'>]><r/>",
        0,
    ),
    (
        "default-nested",
        "<!DOCTYPE r [<!ENTITY e '&f;&f;'><!ENTITY f 'w'><!ATTLIST r a CDATA '&e;'>]><r/>",
        0,
    ),
    (
        "default-nmtoken-normalised",
        "<!DOCTYPE r [<!ENTITY e '  a   b '><!ATTLIST r a NMTOKENS ' &e; '>]><r/>",
        0,
    ),
    (
        "default-undeclared",
        "<!DOCTYPE r [<!ATTLIST r a CDATA '&u;'>]><r/>",
        11,
    ),
    (
        "default-declared-later",
        "<!DOCTYPE r [<!ATTLIST r a CDATA '&e;'><!ENTITY e 'x'>]><r/>",
        11,
    ),
    (
        "default-overridden",
        "<!DOCTYPE r [<!ENTITY e 'v'><!ATTLIST r a CDATA '&e;'>]><r a='mine'/>",
        0,
    ),
    (
        "default-fixed",
        "<!DOCTYPE r [<!ENTITY e 'v'><!ATTLIST r a CDATA #FIXED '&e;'>]><r/>",
        0,
    ),
    (
        "default-ns",
        "<!DOCTYPE p:r [<!ENTITY u 'urn:d'><!ATTLIST p:r xmlns:p CDATA '&u;'>]><p:r/>",
        0,
    ),
    // --- parameter entities
    (
        "pe-between-decls",
        "<!DOCTYPE r [<!ENTITY % p '<!ENTITY e \"from-pe\">'>%p;]><r>&e;</r>",
        0,
    ),
    (
        "pe-nested",
        "<!DOCTYPE r [<!ENTITY % q '<!ELEMENT r ANY>'><!ENTITY % p '%q;&#37;q;'>%p;]><r/>",
        10,
    ),
    (
        "pe-nested-charref",
        "<!DOCTYPE r [<!ENTITY % q '<!ELEMENT r ANY>'><!ENTITY % p '&#37;q;'>%p;]><r/>",
        0,
    ),
    (
        "pe-in-entity-value-in-pe",
        "<!DOCTYPE r [<!ENTITY % v 'val'><!ENTITY % p '<!ENTITY e \"[&#37;v;]\">'>%p;]><r>&e;</r>",
        0,
    ),
    (
        "pe-entity-value-undeclared",
        "<!DOCTYPE r [<!ENTITY % p '<!ENTITY e \"&#37;u;\">'>%p;]><r>&e;</r>",
        0,
    ),
    (
        "pe-in-internal-subset-value",
        "<!DOCTYPE r [<!ENTITY % v 'x'><!ENTITY e '%v;'>]><r/>",
        10,
    ),
    (
        "pe-incomplete",
        "<!DOCTYPE r [<!ENTITY % p '<!ELEMENT r'>%p; ANY>]><r/>",
        29,
    ),
    (
        "pe-incomplete-comment",
        "<!DOCTYPE r [<!ENTITY % p '<!--'>%p;-->]><r/>",
        5,
    ),
    (
        "pe-closes-doctype",
        "<!DOCTYPE r [<!ENTITY % p ']>'>%p;<r/>",
        4,
    ),
    (
        "pe-undeclared",
        "<!DOCTYPE r [%u;<!ENTITY e 'x'>]><r>&e;</r>",
        0,
    ),
    (
        "pe-undeclared-standalone",
        "<?xml version='1.0' standalone='yes'?><!DOCTYPE r [%u;]><r/>",
        11,
    ),
    (
        "pe-standalone-declares",
        "<?xml version='1.0' standalone='yes'?><!DOCTYPE r [<!ENTITY % p '<!ENTITY e \"x\">'>%p;]><r>&e;</r>",
        24,
    ),
    (
        "pe-attlist",
        "<!DOCTYPE r [<!ENTITY e 'd'><!ENTITY % p '<!ATTLIST r a CDATA \"&#38;e;\">'>%p;]><r/>",
        0,
    ),
    (
        "pe-declares-pe",
        "<!DOCTYPE r [<!ENTITY % p '<!ENTITY &#37; q \"<!ENTITY e &#39;z&#39;>\">'>%p;%q;]><r>&e;</r>",
        0,
    ),
    (
        "pe-pi-comment",
        "<!DOCTYPE r [<!ENTITY % p '<?pi x?><!--c-->'>%p;]><r/>",
        0,
    ),
    ("pe-empty", "<!DOCTYPE r [<!ENTITY % p ''>%p;%p;]><r/>", 0),
    (
        "pe-external-unread",
        "<!DOCTYPE r [<!ENTITY % x SYSTEM 'x.ent'>%x;<!ENTITY e 'y'>]><r>&e;</r>",
        0,
    ),
    (
        "pe-entity-declared-in-pe-standalone",
        "<?xml version='1.0' standalone='yes'?><!DOCTYPE r [<!ENTITY % p '<!ENTITY e \"x\">'>%p;]><r a='&e;'/>",
        24,
    ),
    // --- recursion: direct
    (
        "rec-content-direct",
        "<!DOCTYPE r [<!ENTITY a '&a;'>]><r>&a;</r>",
        12,
    ),
    (
        "rec-attr-direct",
        "<!DOCTYPE r [<!ENTITY a 'x&a;'>]><r v='&a;'/>",
        12,
    ),
    (
        "rec-default-direct",
        "<!DOCTYPE r [<!ENTITY a '&a;'><!ATTLIST r v CDATA '&a;'>]><r/>",
        12,
    ),
    (
        "rec-pe-direct",
        "<!DOCTYPE r [<!ENTITY % a '&#37;a;'>%a;]><r/>",
        12,
    ),
    (
        "rec-value-direct",
        "<!DOCTYPE r [<!ENTITY % p '<!ENTITY &#37; a \"&#37;a;\">'>%p;]><r/>",
        12,
    ),
    (
        "rec-declared-not-referenced",
        "<!DOCTYPE r [<!ENTITY a '&a;'><!ENTITY b '&c;'><!ENTITY c '&b;'>]><r/>",
        0,
    ),
    // --- recursion: indirect
    (
        "rec-content-indirect-2",
        "<!DOCTYPE r [<!ENTITY a 'x&b;'><!ENTITY b 'y&a;'>]><r>&a;</r>",
        12,
    ),
    (
        "rec-content-indirect-3",
        "<!DOCTYPE r [<!ENTITY a '&b;'><!ENTITY b '<i>&c;</i>'><!ENTITY c 'z&a;'>]><r>&a;</r>",
        12,
    ),
    (
        "rec-content-indirect-late",
        "<!DOCTYPE r [<!ENTITY a 'ok'><!ENTITY b '&a;&c;'><!ENTITY c '&b;'>]><r>&a;&b;</r>",
        12,
    ),
    (
        "rec-attr-indirect",
        "<!DOCTYPE r [<!ENTITY a '1&b;'><!ENTITY b '2&a;'>]><r v='&a;'/>",
        12,
    ),
    (
        "rec-default-indirect",
        "<!DOCTYPE r [<!ENTITY a '&b;'><!ENTITY b '&a;'><!ATTLIST r v CDATA '&b;'>]><r/>",
        12,
    ),
    (
        "rec-pe-indirect",
        "<!DOCTYPE r [<!ENTITY % a '&#37;b;'><!ENTITY % b '&#37;a;'>%a;]><r/>",
        12,
    ),
    (
        "rec-pe-indirect-3",
        "<!DOCTYPE r [<!ENTITY % a '&#37;b;'><!ENTITY % b '<!--x-->&#37;c;'><!ENTITY % c '&#37;a;'>%a;]><r/>",
        12,
    ),
    (
        "rec-value-indirect",
        "<!DOCTYPE r [<!ENTITY % b '&#37;a;'><!ENTITY % p '<!ENTITY &#37; a \"&#37;b;\">'>%p;<!ENTITY % q '<!ENTITY e \"&#37;a;\">'>%q;]><r/>",
        12,
    ),
    (
        "rec-content-via-attr-entity",
        "<!DOCTYPE r [<!ENTITY a '<i v=\"&b;\"/>'><!ENTITY b '&a;'>]><r>&a;</r>",
        12,
    ),
];

#[test]
fn curated_entity_cases_match_c() {
    let opts = options();
    let mut failures = Vec::new();
    let mut pairs = 0;
    let mut wrong_expectation = Vec::new();
    for (id, doc, want) in CASES {
        let c = check(&mut failures, id, doc.as_bytes(), &ALWAYS);
        pairs += 1;
        let got = result_code(&c);
        if got != *want {
            wrong_expectation.push(format!("{id}: C gives {got}, table says {want}"));
        }
        for o in &opts {
            check(&mut failures, id, doc.as_bytes(), o);
            pairs += 1;
        }
    }
    assert!(wrong_expectation.is_empty(), "{wrong_expectation:#?}");
    assert_clean("curated entity cases", pairs, &failures);
}

/// A chain of `n` entities, each referencing the next, in one context.
fn chain(n: usize, kind: &str) -> Vec<u8> {
    let mut d = String::from("<!DOCTYPE r [");
    match kind {
        "content" | "attr" | "default" => {
            for i in 0..n {
                d.push_str(&format!("<!ENTITY e{i} 'x&e{};'>", i + 1));
            }
            d.push_str(&format!("<!ENTITY e{n} 'end'>"));
            match kind {
                "content" => d.push_str("]><r>&e0;</r>"),
                "attr" => d.push_str("]><r a='&e0;'/>"),
                _ => d.push_str("<!ATTLIST r a CDATA '&e0;'>]><r/>"),
            }
        }
        "pe" => {
            for i in 0..n {
                d.push_str(&format!("<!ENTITY % p{i} '&#37;p{};'>", i + 1));
            }
            d.push_str(&format!("<!ENTITY % p{n} '<!ELEMENT r ANY>'>%p0;]><r/>"));
        }
        _ => {
            // Entity values: p0 declares e with a value referencing v0,
            // and v_i references v_{i+1}.
            for i in 0..n {
                d.push_str(&format!("<!ENTITY % v{i} 'v&#37;v{};'>", i + 1));
            }
            d.push_str(&format!(
                "<!ENTITY % v{n} 'end'><!ENTITY % p '<!ENTITY e \"&#37;v0;\">'>%p;]><r>&e;</r>"
            ));
        }
    }
    d.into_bytes()
}

#[test]
fn deep_entity_chains_match_c() {
    // 10,000 nested open entities: C and the port expand iteratively, so
    // this must neither overflow a stack nor differ. Bounded: the
    // documents are < 600 KB and expand to < 50 KB.
    let mut failures = Vec::new();
    let mut pairs = 0;
    for kind in ["content", "attr", "default", "pe", "value"] {
        for n in [1usize, 100, 10_000] {
            let doc = chain(n, kind);
            for o in [
                ALWAYS,
                ParseOptions {
                    chunk_size: 4096,
                    ..ALWAYS
                },
            ] {
                let c = check(&mut failures, &format!("chain-{kind}-{n}"), &doc, &o);
                assert_eq!(result_code(&c), 0, "chain-{kind}-{n}: C should accept");
                pairs += 1;
            }
        }
    }
    assert_clean("deep chains", pairs, &failures);
}

#[test]
fn deep_recursion_cycle_matches_c() {
    // A cycle closed after 5,000 entities: detected at the closing
    // reference in every context.
    let n = 5_000;
    let mut failures = Vec::new();
    for kind in ["content", "attr", "pe"] {
        let mut d = String::from("<!DOCTYPE r [");
        if kind == "pe" {
            for i in 0..n {
                d.push_str(&format!("<!ENTITY % p{i} '&#37;p{};'>", (i + 1) % n));
            }
            d.push_str("%p0;]><r/>");
        } else {
            for i in 0..n {
                d.push_str(&format!("<!ENTITY e{i} '&e{};'>", (i + 1) % n));
            }
            d.push_str(if kind == "content" {
                "]><r>&e0;</r>"
            } else {
                "]><r a='&e0;'/>"
            });
        }
        let c = check(
            &mut failures,
            &format!("cycle-{kind}"),
            d.as_bytes(),
            &ALWAYS,
        );
        assert_eq!(result_code(&c), 12, "cycle-{kind}");
    }
    assert_clean("deep cycles", 3, &failures);
}

#[test]
fn generated_entity_documents_match_c() {
    let mut r = Rng::new(0x11b_e771);
    let opts = options();
    let mut failures = Vec::new();
    let mut codes = std::collections::BTreeMap::new();
    let mut pairs = 0;
    for i in 0..6_000 {
        let doc = entity_document(&mut r);
        let o = &opts[r.below(opts.len())];
        let c = check(&mut failures, &format!("gen-{i}"), &doc, o);
        *codes.entry(result_code(&c)).or_insert(0usize) += 1;
        pairs += 1;
    }
    eprintln!("codes {codes:?}");
    assert_clean("generated entity documents", pairs, &failures);
    // The generator must reach expansion successes, recursion, proper
    // nesting errors, undefined entities and bad markup in replacement
    // text; otherwise it tests less than it claims.
    for code in [0, 4, 11, 12, 13] {
        assert!(
            codes.contains_key(&code),
            "generator never produced error code {code}"
        );
    }
}
