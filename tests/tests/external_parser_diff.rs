//! Module 11d, step D2: external entities read through external entity
//! parsers (`XML_ExternalEntityParserCreate`), against the C oracle,
//! compared by canonical trace.
//!
//! The library never fetches anything. The test handler of the shim and of
//! the port (`ExtHandler::Read`) looks the system identifier up in an
//! in-memory map, reads the bytes through an external entity parser fed
//! like the document (whole, 1-byte or 7-byte chunks, `XML_Parse` or the
//! buffer API) and traces `extend <ok> <code> <line> <column> <index>`
//! for the child; the parent continues or stops with
//! `XML_ERROR_EXTERNAL_ENTITY_HANDLING`.
//!
//! Groups (C behaviour, policy and divergences in docs/REPORT.md, module
//! 11d step D2): curated general entities (text declarations, encodings,
//! namespaces inherited through the context string, nesting, recursion,
//! tag balance), external subsets and parameter entities (declarations,
//! conditional sections INCLUDE/IGNORE, text declarations, parameter
//! entities in entity values), the parameter-entity parsing modes NEVER /
//! UNLESS_STANDALONE / ALWAYS with the not-standalone handler, the nesting
//! cap, generated documents with random entity bodies, billion laughs
//! across nested external entities (bounded), and the allocation tracker
//! relation across nested parsers.

use expat_difftest::bounded::{Bounds, CountingAlloc, compare};
use expat_difftest::docgen::{Rng, external_case};
use expat_difftest::port::port_trace;
use expat_difftest::trace::first_difference;
use expat_oracle::{ExtHandler, Factor, ParamEntityParsing, ParseOptions, parse_trace};

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

type Entities = Vec<(Vec<u8>, Vec<u8>)>;

/// The entity map of a case. A body starting with `u16:` is encoded as
/// UTF-16LE with a byte order mark; `trunc:` appends the first byte of a
/// two-byte UTF-8 character (a partial character at the end).
fn entities(list: &[(&str, &str)]) -> Entities {
    list.iter()
        .map(|(k, v)| {
            let body = if let Some(t) = v.strip_prefix("u16:") {
                let mut b = vec![0xFF, 0xFE];
                b.extend(t.encode_utf16().flat_map(u16::to_le_bytes));
                b
            } else if let Some(t) = v.strip_prefix("trunc:") {
                let mut b = t.as_bytes().to_vec();
                b.push(0xC3);
                b
            } else {
                v.as_bytes().to_vec()
            };
            (k.as_bytes().to_vec(), body)
        })
        .collect()
}

/// Compares `doc` under `o`; returns the C trace.
fn check(failures: &mut Vec<String>, id: &str, doc: &[u8], o: &ParseOptions) -> Vec<u8> {
    let c = parse_trace(doc, o).expect("oracle");
    let t = port_trace(doc, o);
    if let Some((line, l, r)) = first_difference(&c, &t) {
        failures.push(format!("{id}: line {line}: C `{l}` port `{r}` ({o:?})"));
    }
    c
}

fn assert_clean(what: &str, pairs: usize, failures: &[String]) {
    eprintln!(
        "{what}: {pairs} pairs compared, {} mismatches",
        failures.len()
    );
    for f in failures.iter().take(10) {
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

fn lines_with(trace: &[u8], prefix: &str) -> Vec<String> {
    String::from_utf8_lossy(trace)
        .lines()
        .filter(|l| l.starts_with(prefix))
        .map(str::to_string)
        .collect()
}

const PEPS: [ParamEntityParsing; 3] = [
    ParamEntityParsing::Never,
    ParamEntityParsing::UnlessStandalone,
    ParamEntityParsing::Always,
];

/// Parameter-entity parsing x not-standalone handler x feeding (whole;
/// 1-byte chunks with namespaces; 7-byte chunks through the buffer API).
fn matrix(ents: &Entities) -> Vec<ParseOptions> {
    let mut v = Vec::new();
    for not_standalone in [None, Some(true), Some(false)] {
        for pep in PEPS {
            for (chunk, buffer_api, ns) in
                [(0, false, None), (1, false, Some(b'|')), (7, true, None)]
            {
                v.push(ParseOptions {
                    ext_handler: ExtHandler::Read,
                    ext_entities: ents.clone(),
                    not_standalone,
                    param_entity_parsing: pep,
                    chunk_size: chunk,
                    buffer_api,
                    ns_separator: ns,
                    ..ParseOptions::default()
                });
            }
        }
    }
    v
}

type Case = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

/// (id, document, entities). The documents are also compared under the
/// whole matrix.
const CASES: &[Case] = &[
    // --- general entities in content
    (
        "g-text",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>a&g;b</r>",
        &[("g.xml", "text")],
    ),
    (
        "g-elements",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "<a x='1'>t<b/></a><c/>")],
    ),
    (
        "g-empty",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "")],
    ),
    (
        "g-bom-only",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "\u{FEFF}")],
    ),
    (
        "g-bom-text",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "\u{FEFF}<a/>")],
    ),
    (
        "g-textdecl",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "<?xml encoding='UTF-8'?><a/>")],
    ),
    (
        "g-textdecl-version",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "<?xml version='1.0' encoding='US-ASCII'?>x")],
    ),
    (
        "g-textdecl-no-encoding",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "<?xml version='1.0'?>x")],
    ),
    (
        "g-textdecl-standalone",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[(
            "g.xml",
            "<?xml version='1.0' encoding='UTF-8' standalone='yes'?>x",
        )],
    ),
    (
        "g-textdecl-latin1",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "<?xml encoding='ISO-8859-1'?>caf\u{e9}")],
    ),
    (
        "g-textdecl-unknown-encoding",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "<?xml encoding='x-unknown'?>x")],
    ),
    (
        "g-textdecl-late",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "x<?xml encoding='UTF-8'?>")],
    ),
    (
        "g-unclosed-tag",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "<a>")],
    ),
    (
        "g-closes-parent",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "</r>")],
    ),
    (
        "g-mismatch",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "<a></b>")],
    ),
    (
        "g-invalid",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "a < b")],
    ),
    (
        "g-partial-char",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "trunc:ab")],
    ),
    (
        "g-internal-from-parent",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'><!ENTITY e 'int<i/>'>]><r>&g;</r>",
        &[("g.xml", "[&e;]")],
    ),
    (
        "g-undeclared",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "&nope;")],
    ),
    (
        "g-attribute-default",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'><!ATTLIST a d CDATA 'dflt'>]><r>&g;</r>",
        &[("g.xml", "<a/>")],
    ),
    (
        "g-nested",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'><!ENTITY h SYSTEM 'h.xml'>]><r>&g;</r>",
        &[("g.xml", "<a>&h;</a>"), ("h.xml", "<b>deep</b>")],
    ),
    (
        "g-self-recursive",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "x&g;")],
    ),
    (
        "g-mutual-recursive",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'><!ENTITY h SYSTEM 'h.xml'>]><r>&g;</r>",
        &[("g.xml", "&h;"), ("h.xml", "&g;")],
    ),
    (
        "g-through-internal",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'><!ENTITY i '<k>&g;</k>'>]><r>&i;</r>",
        &[("g.xml", "&i;")],
    ),
    (
        "g-twice",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;&g;</r>",
        &[("g.xml", "<a/>")],
    ),
    (
        "g-namespaces",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r xmlns='urn:d' xmlns:p='urn:p'>&g;</r>",
        &[(
            "g.xml",
            "<a/><p:b q='1' p:r='2'/><c xmlns:p='urn:other'><p:d/></c>",
        )],
    ),
    (
        "g-namespaces-undeclared-prefix",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r xmlns:p='urn:p'>&g;</r>",
        &[("g.xml", "<z:a/>")],
    ),
    (
        "g-namespaces-default-unbound",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r xmlns='urn:d'><e xmlns=''>&g;</e></r>",
        &[("g.xml", "<a/>")],
    ),
    (
        "g-cdata-pi-comment",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "<![CDATA[x]]><?pi d?><!--c-->&#x41;\r\n")],
    ),
    (
        "g-unparsed",
        "<!DOCTYPE r [<!NOTATION n SYSTEM 'n'><!ENTITY u SYSTEM 'u' NDATA n><!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "&u;")],
    ),
    (
        "g-in-attribute",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r a='&g;'/>",
        &[("g.xml", "x")],
    ),
    (
        "g-missing",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[],
    ),
    (
        "g-utf16",
        "<!DOCTYPE r [<!ENTITY g SYSTEM 'g.xml'>]><r>&g;</r>",
        &[("g.xml", "u16:<a>u16</a>")],
    ),
    // --- external subset and parameter entities
    (
        "sub-decls",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&x;</r>",
        &[(
            "r.dtd",
            "<!ENTITY x 'from-subset'><!ELEMENT r ANY><!ATTLIST r a CDATA 'd'>",
        )],
    ),
    (
        "sub-textdecl",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&x;</r>",
        &[("r.dtd", "<?xml encoding='UTF-8'?><!ENTITY x 'v'>")],
    ),
    (
        "sub-textdecl-no-encoding",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r/>",
        &[("r.dtd", "<?xml version='1.0'?><!ENTITY x 'v'>")],
    ),
    (
        "sub-bom",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&x;</r>",
        &[("r.dtd", "\u{FEFF}<!ENTITY x 'v'>")],
    ),
    (
        "sub-include",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&i;</r>",
        &[("r.dtd", "<![INCLUDE[ <!ENTITY i 'inc'> ]]>")],
    ),
    (
        "sub-ignore",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&i;</r>",
        &[(
            "r.dtd",
            "<![IGNORE[ <!ENTITY i 'ign'> <![ nested [ ]]> x ]]><!ENTITY i 'after'>",
        )],
    ),
    (
        "sub-nested-cond",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&i;&j;</r>",
        &[(
            "r.dtd",
            "<![INCLUDE[<![IGNORE[<!ENTITY i 'no'>]]><!ENTITY i 'yes'><![INCLUDE[<!ENTITY j 'j'>]]>]]>",
        )],
    ),
    (
        "sub-cond-pe-keyword",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&i;</r>",
        &[(
            "r.dtd",
            "<!ENTITY % k 'INCLUDE'><![%k;[<!ENTITY i 'via-pe'>]]>",
        )],
    ),
    (
        "sub-ignore-unclosed",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r/>",
        &[("r.dtd", "<![IGNORE[ never closed")],
    ),
    (
        "sub-include-unclosed",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r/>",
        &[("r.dtd", "<![INCLUDE[ <!ENTITY i 'x'>")],
    ),
    (
        "sub-stray-close",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r/>",
        &[("r.dtd", "]]>")],
    ),
    (
        "sub-incomplete-decl",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r/>",
        &[("r.dtd", "<!ENTITY x")],
    ),
    (
        "sub-instance",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r/>",
        &[("r.dtd", "<r/>")],
    ),
    (
        "sub-pe-internal",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&x;</r>",
        &[("r.dtd", "<!ENTITY % d '<!ENTITY x \"pe\">'>%d;")],
    ),
    (
        "sub-pe-inside-decl",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r a='1'/>",
        &[("r.dtd", "<!ENTITY % t 'CDATA'><!ATTLIST r a %t; #IMPLIED>")],
    ),
    (
        "sub-pe-external",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&x;</r>",
        &[
            ("r.dtd", "<!ENTITY % p SYSTEM 'p.ent'>%p;"),
            ("p.ent", "<!ENTITY x 'from-p'>"),
        ],
    ),
    (
        "sub-pe-external-textdecl",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&x;</r>",
        &[
            ("r.dtd", "<!ENTITY % p SYSTEM 'p.ent'>%p;"),
            ("p.ent", "<?xml encoding='UTF-8'?><!ENTITY x 'from-p'>"),
        ],
    ),
    (
        "sub-pe-recursive",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r/>",
        &[
            ("r.dtd", "<!ENTITY % p SYSTEM 'p.ent'>%p;"),
            ("p.ent", "%p;"),
        ],
    ),
    (
        "sub-pe-in-entity-value",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&v;</r>",
        &[
            ("r.dtd", "<!ENTITY % p SYSTEM 'p.ent'><!ENTITY v 'a%p;b'>"),
            ("p.ent", "VAL"),
        ],
    ),
    (
        "sub-pe-in-entity-value-charref",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&v;</r>",
        &[
            ("r.dtd", "<!ENTITY % p SYSTEM 'p.ent'><!ENTITY v 'a%p;b'>"),
            ("p.ent", "VAL&#33;"),
        ],
    ),
    (
        "sub-pe-in-entity-value-textdecl",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&v;</r>",
        &[
            ("r.dtd", "<!ENTITY % p SYSTEM 'p.ent'><!ENTITY v '[%p;]'>"),
            ("p.ent", "<?xml version='1.0' encoding='UTF-8'?>tv"),
        ],
    ),
    (
        "sub-pe-in-entity-value-textdecl-no-version",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&v;</r>",
        &[
            ("r.dtd", "<!ENTITY % p SYSTEM 'p.ent'><!ENTITY v '[%p;]'>"),
            ("p.ent", "<?xml encoding='UTF-8'?>tv"),
        ],
    ),
    (
        "sub-pe-in-entity-value-tag",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&v;</r>",
        &[
            ("r.dtd", "<!ENTITY % p SYSTEM 'p.ent'><!ENTITY v '[%p;]'>"),
            ("p.ent", "a<b"),
        ],
    ),
    (
        "sub-pe-in-entity-value-internal-pe",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&v;&w;</r>",
        &[
            (
                "r.dtd",
                "<!ENTITY % q 'Q'><!ENTITY % p SYSTEM 'p.ent'><!ENTITY v '[%p;]'><!ENTITY w '%p;'>",
            ),
            ("p.ent", "x%q;y"),
        ],
    ),
    (
        "sub-pe-in-entity-value-nested",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&v;</r>",
        &[
            (
                "r.dtd",
                "<!ENTITY % p SYSTEM 'p.ent'><!ENTITY % s SYSTEM 's.ent'><!ENTITY v '%p;'>",
            ),
            ("p.ent", "1%s;2"),
            ("s.ent", "S"),
        ],
    ),
    (
        "sub-standalone-yes",
        "<?xml version='1.0' standalone='yes'?><!DOCTYPE r SYSTEM 'r.dtd'><r>&x;</r>",
        &[("r.dtd", "<!ENTITY x 'v'>")],
    ),
    (
        "sub-standalone-no",
        "<?xml version='1.0' standalone='no'?><!DOCTYPE r SYSTEM 'r.dtd'><r>&x;</r>",
        &[("r.dtd", "<!ENTITY x 'v'>")],
    ),
    (
        "sub-and-internal",
        "<!DOCTYPE r SYSTEM 'r.dtd' [<!ENTITY x 'internal-first'>]><r>&x;&y;</r>",
        &[("r.dtd", "<!ENTITY x 'subset'><!ENTITY y 'y'>")],
    ),
    (
        "sub-declares-general-external",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&g;</r>",
        &[
            ("r.dtd", "<!ENTITY g SYSTEM 'g.xml'>"),
            ("g.xml", "<from-g/>"),
        ],
    ),
    ("sub-missing", "<!DOCTYPE r SYSTEM 'r.dtd'><r>&x;</r>", &[]),
    (
        "sub-utf16",
        "<!DOCTYPE r SYSTEM 'r.dtd'><r>&x;</r>",
        &[("r.dtd", "u16:<!ENTITY x 'v16'>")],
    ),
    (
        "internal-pe-external",
        "<!DOCTYPE r [<!ENTITY % p SYSTEM 'p.ent'>%p;<!ENTITY y 'after'>]><r>&x;&y;</r>",
        &[("p.ent", "<!ENTITY x 'from-p'>")],
    ),
    (
        "internal-pe-external-standalone",
        "<?xml version='1.0' standalone='yes'?><!DOCTYPE r [<!ENTITY % p SYSTEM 'p.ent'>%p;]><r>&x;</r>",
        &[("p.ent", "<!ENTITY x 'from-p'>")],
    ),
    (
        "internal-pe-external-cond",
        "<!DOCTYPE r [<!ENTITY % p SYSTEM 'p.ent'>%p;]><r>&x;</r>",
        &[("p.ent", "<![INCLUDE[<!ENTITY x 'c'>]]>")],
    ),
    (
        "internal-pe-external-partial",
        "<!DOCTYPE r [<!ENTITY % p SYSTEM 'p.ent'>%p;]><r/>",
        &[("p.ent", "<!ENTITY x 'unterminated")],
    ),
    ("foreign-dtd", "<r>&x;</r>", &[]),
    (
        "entity-declared-in-pe-standalone",
        "<?xml version='1.0' standalone='yes'?><!DOCTYPE r [<!ENTITY % p SYSTEM 'p.ent'>%p;]><r>&x;</r>",
        &[("p.ent", "<!ENTITY x 'v'>")],
    ),
];

/// Every curated case under the whole matrix (27 option sets), with the
/// foreign DTD for the case that needs it.
#[test]
fn curated_external_parser_cases_match_c() {
    let mut failures = Vec::new();
    let (mut pairs, mut reads) = (0, 0);
    let mut codes = std::collections::BTreeMap::new();
    let mut child_codes = std::collections::BTreeMap::new();
    for (id, doc, ents) in CASES {
        let ents = entities(ents);
        for mut o in matrix(&ents) {
            // The foreign DTD has no system identifier, so the handler is
            // called but reads nothing.
            o.use_foreign_dtd = *id == "foreign-dtd";
            let c = check(&mut failures, id, doc.as_bytes(), &o);
            pairs += 1;
            for l in lines_with(&c, "extend ") {
                reads += 1;
                let code: i64 = l
                    .split(' ')
                    .nth(2)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(-1);
                *child_codes.entry(code).or_insert(0usize) += 1;
            }
            *codes.entry(result_code(&c)).or_insert(0usize) += 1;
        }
    }
    eprintln!("result codes {codes:?}");
    eprintln!("child parses {reads}, child codes {child_codes:?}");
    assert_clean("curated external parser cases", pairs, &failures);
    // Children end successfully, with a text declaration error (31), an
    // asynchronous entity (13), an incomplete parameter entity (29) and a
    // recursive reference (12).
    for code in [0, 12, 13, 29, 31] {
        assert!(
            child_codes.contains_key(&code),
            "no child ended with {code}"
        );
    }
}

/// C's behaviour pinned on single pairs (ParamEntityParsing::Always,
/// whole document).
#[test]
fn pinned_external_parser_behaviour() {
    let mut failures = Vec::new();
    let mut pin = |id: &str, pep: ParamEntityParsing, want_code: i64, want: &[&str]| {
        let (_, doc, ents) = CASES.iter().find(|c| c.0 == id).expect("case");
        let o = ParseOptions {
            ext_handler: ExtHandler::Read,
            ext_entities: entities(ents),
            param_entity_parsing: pep,
            ..ParseOptions::default()
        };
        let c = check(&mut failures, id, doc.as_bytes(), &o);
        let s = String::from_utf8_lossy(&c).into_owned();
        assert_eq!(result_code(&c), want_code, "{id}: {s}");
        for w in want {
            assert!(s.lines().any(|l| l == *w), "{id}: no line `{w}` in\n{s}");
        }
    };
    let always = ParamEntityParsing::Always;
    // Content of the entity appears in place; the child succeeds.
    pin(
        "g-elements",
        always,
        0,
        &["start a 2", "extend 1 0 1 22 22"],
    );
    // Recursion through the context string: the child sees g open.
    pin("g-self-recursive", always, 21, &["extend 0 12 1 1 1"]);
    // An end tag of the parent inside the entity.
    pin("g-closes-parent", always, 21, &["extend 0 13 1 0 0"]);
    // A text declaration needs an encoding declaration.
    pin(
        "g-textdecl-no-encoding",
        always,
        21,
        &["extend 0 31 1 19 19"],
    );
    // Declarations of the external subset are used by the document.
    pin("sub-decls", always, 0, &["text from-subset"]);
    // Ignore sections are skipped, nested brackets included.
    pin("sub-ignore", always, 0, &["text after"]);
    // A parameter entity in an entity value inside the external subset.
    pin("sub-pe-in-entity-value", always, 0, &["text aVALb"]);
    // C scans that text with the prolog tokenizer before storing it: a
    // character reference is an invalid token there (the declaration
    // handler still sees the value stored so far).
    pin(
        "sub-pe-in-entity-value-charref",
        always,
        21,
        &["extend 0 4 1 0 0", "entity v 0 a ~ ~ ~ ~"],
    );
    // Without parameter entity parsing nothing is read and the undeclared
    // entity is skipped (the document has an external subset).
    pin("sub-decls", ParamEntityParsing::Never, 0, &["skipped x 0"]);
    assert_clean("pinned external parser behaviour", 9, &failures);
}

/// The nesting cap of the test handler (`EXT_MAX_DEPTH` children): a chain
/// of 20 general entities.
#[test]
fn nesting_cap_matches_c() {
    let mut doc = String::from("<!DOCTYPE r [");
    let mut ents = Vec::new();
    for i in 0..20 {
        doc.push_str(&format!("<!ENTITY g{i} SYSTEM 'g{i}.xml'>"));
        ents.push((
            format!("g{i}.xml").into_bytes(),
            format!("<l{i}>&g{};</l{i}>", i + 1).into_bytes(),
        ));
    }
    doc.push_str("<!ENTITY g20 'end'>]><r>&g0;</r>");
    let mut failures = Vec::new();
    let mut pairs = 0;
    for pep in PEPS {
        for ns in [None, Some(b'|')] {
            let o = ParseOptions {
                ext_handler: ExtHandler::Read,
                ext_entities: ents.clone(),
                param_entity_parsing: pep,
                ns_separator: ns,
                ..ParseOptions::default()
            };
            let c = check(&mut failures, "chain-20", doc.as_bytes(), &o);
            pairs += 1;
            assert_eq!(lines_with(&c, "extdepth").len(), 1);
            assert_eq!(lines_with(&c, "extend ").len(), 16);
        }
    }
    assert_clean("nesting cap", pairs, &failures);
}

/// Generated documents with external entities and random bodies
/// (`docgen::external_case`) under random options.
#[test]
fn generated_external_documents_match_c() {
    let mut r = Rng::new(0x11d2_e7e7);
    let mut failures = Vec::new();
    let mut codes = std::collections::BTreeMap::new();
    let (mut pairs, mut reads) = (0, 0);
    for i in 0..6_000 {
        let (doc, ents) = external_case(&mut r);
        let o = ParseOptions {
            ext_handler: if r.one_in(8) {
                ExtHandler::Accept
            } else {
                ExtHandler::Read
            },
            ext_entities: ents,
            not_standalone: [None, Some(true), Some(false)][r.below(3)],
            param_entity_parsing: PEPS[r.below(3)],
            use_foreign_dtd: r.one_in(8),
            ns_separator: if r.one_in(3) { Some(b'|') } else { None },
            chunk_size: [0, 1, 3, 64][r.below(4)],
            buffer_api: r.one_in(4),
            ..ParseOptions::default()
        };
        let c = check(&mut failures, &format!("generated-{i}"), &doc, &o);
        pairs += 1;
        reads += lines_with(&c, "extend ").len();
        *codes.entry(result_code(&c)).or_insert(0usize) += 1;
    }
    eprintln!("result codes {codes:?}, child parses {reads}");
    assert_clean("generated external documents", pairs, &failures);
}

/// Billion laughs across nested external entities: each external entity
/// references the next one `fan` times, so the expansion is `fan^depth`
/// child parsers whose bytes all count as indirect for the root parser.
/// Bounded (input, heap, time); with the default limits and with a lowered
/// activation threshold.
#[test]
fn billion_laughs_across_external_entities() {
    let bounds = Bounds {
        max_input: 64 << 10,
        timeout: std::time::Duration::from_secs(60),
        port_heap_cap: 256 << 20,
        c_heap_cap: 256 << 20,
    };
    let mut summary = Vec::new();
    for (kind, fan, depth) in [
        ("general", 10usize, 5usize),
        ("general", 4, 9),
        ("param", 10, 5),
        ("param", 3, 10),
    ] {
        let mut doc = String::from("<!DOCTYPE r [");
        let mut ents = Vec::new();
        for i in 0..depth {
            let refs = if kind == "general" {
                format!("&x{};", i + 1).repeat(fan)
            } else {
                format!("%x{};", i + 1).repeat(fan)
            };
            if kind == "general" {
                doc.push_str(&format!("<!ENTITY x{i} SYSTEM 'x{i}.xml'>"));
                ents.push((
                    format!("x{i}.xml").into_bytes(),
                    format!("<a>{refs}</a>").into_bytes(),
                ));
            } else {
                doc.push_str(&format!("<!ENTITY % x{i} SYSTEM 'x{i}.ent'>"));
                ents.push((
                    format!("x{i}.ent").into_bytes(),
                    format!("<!-- {i} -->{refs}").into_bytes(),
                ));
            }
        }
        if kind == "general" {
            doc.push_str(&format!("<!ENTITY x{depth} 'leaf text'>]><r>&x0;</r>"));
        } else {
            doc.push_str(&format!("<!ENTITY % x{depth} '<!-- leaf -->'>%x0;]><r/>"));
        }
        for threshold in [None, Some(4096u64)] {
            let mut o = ParseOptions {
                ext_handler: ExtHandler::Read,
                ext_entities: ents.clone(),
                param_entity_parsing: ParamEntityParsing::Always,
                text_digest: true,
                trace_accounting: true,
                ..ParseOptions::default()
            };
            o.limits.bl_threshold = threshold;
            if threshold.is_some() {
                o.limits.bl_amplification = Some(Factor(100.0));
            }
            let out = compare(&format!("{kind}-{fan}^{depth}"), doc.as_bytes(), &o, bounds);
            assert!(
                out.equal(),
                "{kind} {fan}^{depth} {threshold:?}: traces differ: {:?}",
                first_difference(&out.c, &out.port)
            );
            let code = result_code(&out.c);
            let breaches = lines_with(&out.c, "extend 0 43").len();
            summary.push(format!(
                "{kind} fan {fan} depth {depth} threshold {threshold:?}: result {code}, children {} (breach {breaches}), {:?}, heap C {} port {}",
                lines_with(&out.c, "extend ").len(),
                out.elapsed,
                out.c_peak,
                out.port_peak
            ));
            if threshold.is_some() {
                // The lowered threshold stops the expansion inside a child.
                assert!(breaches >= 1 || code == 43, "{kind}: no amplification stop");
            }
        }
    }
    for s in &summary {
        eprintln!("{s}");
    }
}

/// The allocation tracker relation across nested external entity parsers
/// (`expat_difftest::tracker`): whenever C refuses with tight tracker
/// limits, the port refuses too; at the default limits the port never
/// refuses a document C accepts.
#[test]
fn tracker_relation_across_external_parsers() {
    let mut r = Rng::new(0x11d2_7ac4);
    let mut tally = expat_difftest::tracker::Tally::default();
    let mut errors = Vec::new();
    for (id, doc, ents) in CASES {
        let o = ParseOptions {
            ext_handler: ExtHandler::Read,
            ext_entities: entities(ents),
            param_entity_parsing: ParamEntityParsing::Always,
            ..ParseOptions::default()
        };
        if let Err(e) = expat_difftest::tracker::check(doc.as_bytes(), &o, &mut tally) {
            errors.push(format!("{id}: {e}"));
        }
    }
    for i in 0..1_500 {
        let (doc, ents) = external_case(&mut r);
        let o = ParseOptions {
            ext_handler: ExtHandler::Read,
            ext_entities: ents,
            param_entity_parsing: PEPS[r.below(3)],
            ns_separator: if r.one_in(3) { Some(b'|') } else { None },
            ..ParseOptions::default()
        };
        if let Err(e) = expat_difftest::tracker::check(&doc, &o, &mut tally) {
            errors.push(format!("generated-{i}: {e}"));
        }
    }
    eprintln!(
        "tracker relation: {} runs, C refused {} tight runs, port-only refusals {}, smallest margin {:?}",
        tally.runs, tally.c_no_memory, tally.port_stricter, tally.min_margin
    );
    for e in errors.iter().take(10) {
        eprintln!("{e}");
    }
    assert!(errors.is_empty(), "{} relation violations", errors.len());
}
