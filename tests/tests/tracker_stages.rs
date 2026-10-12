//! One named regression test per fix of the allocation-tracker parity work
//! (decision D-034, DV-1 option B), each on documents aimed at that fix:
//! the C and port tracker streams must be equal event by event (direction,
//! size, refusal, running total, peak) from the parser's creation through
//! `XML_ParserFree`, both must end at 0, and the traces must be equal (the
//! parse results, error code and position, and every callback before
//! them). `tracker_events.rs` runs the broad corpus;
//! these tests name the stage a regression belongs to.

use expat_difftest::port::port_trace_with_tracker_events;
use expat_difftest::trace::{first_difference, mask_q4};
use expat_oracle::{ExtHandler, Factor, Limits, ParamEntityParsing, ParseOptions, TrackerEvent};

type Entities = Vec<(Vec<u8>, Vec<u8>)>;

fn base() -> ParseOptions {
    ParseOptions {
        param_entity_parsing: ParamEntityParsing::Always,
        ..ParseOptions::default()
    }
}

fn result_line(trace: &[u8]) -> String {
    String::from_utf8_lossy(trace)
        .lines()
        .rev()
        .find(|l| l.starts_with("result "))
        .unwrap_or_default()
        .to_string()
}

fn utf16le(text: &str) -> Vec<u8> {
    let mut v = vec![0xFF, 0xFE];
    v.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    v
}

/// Asserts that C and the port agree on `doc` under `o` (see the module
/// documentation) and returns C's events.
fn assert_equal_streams(what: &str, doc: &[u8], o: &ParseOptions) -> Vec<TrackerEvent> {
    let (c_trace, c) = expat_oracle::parse_trace_with_tracker_events(doc, o).expect("oracle");
    let (p_trace, port, port_at_end) = port_trace_with_tracker_events(doc, o);
    assert_eq!(
        result_line(&c_trace),
        result_line(&p_trace),
        "{what}: parse results differ"
    );
    let (cm, pm) = mask_q4(&c_trace, &p_trace);
    if let Some((line, l, r)) = first_difference(&cm, &pm) {
        panic!("{what}: traces differ at line {line}: C `{l}` port `{r}`");
    }
    assert_eq!(
        c.at_end, port_at_end,
        "{what}: event counts to the end of the parse differ"
    );
    assert_eq!(c.events.len(), port.len(), "{what}: event counts differ");
    for (k, (ce, pe)) in c.events.iter().zip(&port).enumerate() {
        assert!(
            ce.increase == pe.increase
                && ce.size == pe.size
                && ce.refused == pe.refused
                && ce.total == pe.total
                && ce.peak == pe.peak,
            "{what}: event {k} differs: C {ce:?}, port {pe:?}"
        );
    }
    assert_eq!(
        c.events.last().map(|e| e.total),
        Some(0),
        "{what}: C does not end at 0"
    );
    c.events
}

fn ent(k: &str, v: &str) -> (Vec<u8>, Vec<u8>) {
    (k.as_bytes().to_vec(), v.as_bytes().to_vec())
}

fn read_options(ents: Entities) -> ParseOptions {
    ParseOptions {
        ext_handler: ExtHandler::Read,
        ext_entities: ents,
        ..base()
    }
}

/// Activation thresholds that walk a refusal through every allocation of
/// the first ~20 KB (factor 1.0: any count above the direct bytes is
/// refused once the threshold is reached).
fn threshold_sweep(o: &ParseOptions) -> impl Iterator<Item = ParseOptions> + '_ {
    (3_000..20_000).step_by(40).map(move |t| ParseOptions {
        limits: Limits {
            at_amplification: Some(Factor(1.0)),
            at_threshold: Some(t),
            ..Limits::NONE
        },
        ..o.clone()
    })
}

/// Stage 2: the pool model is exact at a block boundary (`XmlConvert`
/// writes whole characters; a pool grows when the rest does not fit).
#[test]
fn stage2_pool_growth_at_block_boundaries() {
    let mut grows = 0;
    for ch in ["\u{e9}", "\u{20ac}", "\u{1f600}", "&#x1f600;"] {
        for fill in 1019..1026 {
            let text = format!(
                "<r a='{}{ch}{ch}'><!--{}{ch}--></r>",
                "x".repeat(fill),
                "y".repeat(fill)
            );
            for doc in [text.clone().into_bytes(), utf16le(&text)] {
                let ev = assert_equal_streams("stage 2", &doc, &base());
                grows += ev.iter().filter(|e| e.increase && e.line == 8608).count();
            }
        }
    }
    assert!(grows > 0, "stage 2: no pool block was reallocated");
}

/// Stage 4: the implicit namespace context (`setContext` with its
/// `m_tempPool` block), the external subset's entity under C's static name,
/// and `setElementTypePrefix` without namespace processing.
#[test]
fn stage4_allocations_the_creation_charge_used_to_cover() {
    let ns = ParseOptions {
        ns_separator: Some(b'|'),
        ..base()
    };
    assert_equal_streams("stage 4 implicit context", b"<r/>", &ns);
    assert_equal_streams(
        "stage 4 implicit context",
        b"<p:r xmlns:p='u'><q/></p:r>",
        &ns,
    );
    let accept = ParseOptions {
        ext_handler: ExtHandler::Accept,
        ..base()
    };
    assert_equal_streams(
        "stage 4 external subset",
        b"<!DOCTYPE r SYSTEM 'x.dtd'><r/>",
        &accept,
    );
    assert_equal_streams(
        "stage 4 element type prefix",
        b"<!DOCTYPE p:r [<!ELEMENT p:r ANY><!ATTLIST p:r a CDATA 'd'>]><p:r/>",
        &base(),
    );
}

/// Stage 4b: `storeEntityValue` gives `dtd->entityValuePool` its first
/// block before anything else, so an empty value allocates one too.
#[test]
fn stage4b_entity_value_pool_block_for_an_empty_value() {
    for doc in [
        &b"<!DOCTYPE r [<!ENTITY e ''>]><r>&e;</r>"[..],
        b"<!DOCTYPE r [<!ENTITY % p ''>%p;]><r/>",
        b"<!DOCTYPE r [<!ENTITY e ''><!ENTITY f 'x'>]><r a='&e;&f;'/>",
    ] {
        let ev = assert_equal_streams("stage 4b", doc, &base());
        // Each document allocates at least one pool block after creation.
        assert!(
            ev.iter().any(|e| e.increase && e.line == 8648),
            "stage 4b: no pool block"
        );
    }
}

/// Stage 5: the raw tag name is copied into the tag buffer by
/// `storeRawNames` after each successful `doContent` (so it depends on
/// where the input is split), and `doContent` doubles the buffer only
/// while it is smaller than the name length plus one.
#[test]
fn stage5_raw_tag_names_and_tag_buffer_growth() {
    for len in [30, 31, 32, 33, 63, 64, 65] {
        let name = "n".repeat(len);
        let text = format!("<{name}><{name}2 a='1'><x/>text</{name}2></{name}>");
        for chunk in [0, 1, 3, 7] {
            let o = ParseOptions {
                chunk_size: chunk,
                ..base()
            };
            assert_equal_streams("stage 5", text.as_bytes(), &o);
            assert_equal_streams("stage 5 utf-16", &utf16le(&text), &o);
        }
    }
    // Unclosed tags at the end of the input keep raw names in their
    // buffers; nested deep.
    let deep: String = (0..40)
        .map(|i| format!("<element-with-a-long-name-{i}>"))
        .collect();
    for chunk in [0, 5] {
        let o = ParseOptions {
            chunk_size: chunk,
            ..base()
        };
        assert_equal_streams("stage 5 unclosed", deep.as_bytes(), &o);
    }
}

/// Stage 6: `ELEMENT_TYPE` (and `NS_ATT`) at C's size.
#[test]
fn stage6_element_type_and_ns_att_sizes() {
    let mut doc = String::from("<!DOCTYPE r [");
    for i in 0..300 {
        doc.push_str(&format!("<!ELEMENT e{i} ANY>"));
    }
    doc.push_str("]><r xmlns:p='u' p:a='1' p:b='2' p:c='3'/>");
    let ns = ParseOptions {
        ns_separator: Some(b'|'),
        ..base()
    };
    assert_equal_streams("stage 6", doc.as_bytes(), &base());
    assert_equal_streams("stage 6 ns", doc.as_bytes(), &ns);
}

/// Stage 3: the root parser's creation is C's allocations (parser
/// structure, `m_atts`, `m_dataBuf`, DTD, and the protocol encoding name)
/// with this platform's sizes; their sum is `CREATION_CHARGE`.
#[test]
fn stage3_root_creation_allocations() {
    let ev = assert_equal_streams("stage 3", b"<r/>", &base());
    let created: u64 = ev.iter().take(4).map(|e| e.size).sum();
    assert_eq!(
        ev.iter().take(4).map(|e| e.line).collect::<Vec<_>>(),
        [1395, 1405, 1410, 7865],
        "stage 3: C's creation events"
    );
    assert_eq!(
        created,
        expat::internal::parse::CREATION_CHARGE,
        "stage 3: CREATION_CHARGE is not C's creation on this platform"
    );
    let enc = ParseOptions {
        encoding: Some("ISO-8859-1".to_string()),
        ..base()
    };
    let ev = assert_equal_streams("stage 3 encoding name", b"<r/>", &enc);
    assert_eq!(
        ev.get(4).map(|e| e.size),
        Some(8 + 11),
        "stage 3: encoding name copy"
    );
}

/// Stage 3: an external entity parser's creation is checked and charged
/// allocation by allocation; a refusal at any of them (the structure,
/// `xmlparse.c` 1310; `m_atts`, 1405; `m_dataBuf`, 1410; the DTD, 7865)
/// frees what was allocated, like C.
#[test]
fn stage3_child_creation_refusals() {
    let o = read_options(vec![
        ent(
            "a.xml",
            "<!DOCTYPE x [<!ENTITY b SYSTEM 'b.xml'>]><a>&b;</a>",
        ),
        ent("b.xml", "<b>text<c/></b>"),
    ]);
    let doc = b"<!DOCTYPE r [<!ENTITY a SYSTEM 'a.xml'><!ATTLIST r d CDATA 'v'>]><r>&a;</r>";
    let mut lines = std::collections::BTreeSet::new();
    for o in threshold_sweep(&o) {
        let ev = assert_equal_streams("stage 3 child creation", doc, &o);
        if let Some(e) = ev.iter().find(|e| e.refused) {
            lines.insert(e.line);
        }
    }
    for line in [1310, 1405, 1410, 7865] {
        assert!(
            lines.contains(&line),
            "stage 3: no refusal at xmlparse.c:{line} ({lines:?})"
        );
    }
}

/// Stage 7: `XML_ParserFree` releases in C's order (tags and their
/// bindings, open entities, free and inherited bindings, the temporary
/// pools, the DTD's tables, pools and scaffold, the arrays, the parser),
/// for the root parser and for every external entity parser.
#[test]
fn stage7_release_streams() {
    let ns = ParseOptions {
        ns_separator: Some(b'|'),
        ..base()
    };
    for doc in [
        &b"<a xmlns:p='urn:p'><b xmlns='urn:d'><p:c/>"[..],
        b"<a xmlns:p='urn:p'><b xmlns:p='urn:q'/><b xmlns:p='urn:longer-uri-than-before-0123456789'/></a>",
        b"<!DOCTYPE r [<!ELEMENT r (a|b)*><!ELEMENT a (#PCDATA)><!ATTLIST r x CDATA 'd' y CDATA 'e'><!ENTITY e 'v'><!ENTITY % p 'q'>]><r x='&e;'><a/></r>",
        b"<!DOCTYPE r [<!ENTITY e '<i>&f;</i>'><!ENTITY f 'x'>]><r>&e;<unclosed>",
    ] {
        assert_equal_streams("stage 7", doc, &base());
        assert_equal_streams("stage 7 ns", doc, &ns);
    }
    let ents = vec![
        ent("a.xml", "<a xmlns:q='urn:q'>&b;<q:x/></a>"),
        ent("b.xml", "<b>&c;</b>"),
        ent("c.xml", "<c x='1'>deep"),
        ent(
            "s.dtd",
            "<!ENTITY % p SYSTEM 'p.ent'>%p;<!ATTLIST r b CDATA 'd'>",
        ),
        ent("p.ent", "<!ENTITY e 'from p'><!ELEMENT q ANY>"),
    ];
    let doc = b"<!DOCTYPE r SYSTEM 's.dtd' [<!ELEMENT r (a|b|q)*><!ELEMENT a (#PCDATA|q)*><!ENTITY a SYSTEM 'a.xml'><!ENTITY b SYSTEM 'b.xml'><!ENTITY c SYSTEM 'c.xml'>]><r xmlns:p='urn:p' b='&e;'>&a;</r>";
    assert_equal_streams("stage 7 nested", doc, &read_options(ents.clone()));
    let o = ParseOptions {
        ns_separator: Some(b'|'),
        chunk_size: 3,
        ..read_options(ents)
    };
    assert_equal_streams("stage 7 nested ns", doc, &o);
}

/// Stage 7: a child whose DTD copy or context fails part way is freed by
/// `XML_ParserFree` with what it had copied so far (the port keeps the
/// partial copy for that).
#[test]
fn stage7_partial_dtd_copy_released() {
    let mut subset = String::new();
    for i in 0..12 {
        subset.push_str(&format!(
            "<!ELEMENT e{i} ANY><!ATTLIST e{i} a{i} CDATA 'v{i}' b{i} NMTOKEN 'n'><!ENTITY g{i} 'text {i}'>"
        ));
    }
    let doc = format!(
        "<!DOCTYPE r [{subset}<!ENTITY x SYSTEM 'x.xml'>]><r xmlns:p='urn:p' xmlns='urn:d'>&x;</r>"
    );
    let o = ParseOptions {
        ns_separator: Some(b'|'),
        ..read_options(vec![ent("x.xml", "<e1/>")])
    };
    let mut refused = 0;
    for o in threshold_sweep(&o) {
        let ev = assert_equal_streams("stage 7 partial copy", doc.as_bytes(), &o);
        refused += usize::from(ev.iter().any(|e| e.refused));
    }
    assert!(refused > 0, "stage 7: no refusal in the sweep");
    // Default values long enough that copying each into the child's
    // `dtd->pool` takes a new block: the sweep refuses at the second and
    // third value, after the first attribute's `defaultAttForName` entry
    // (and table) were allocated, which `XML_ParserFree` must release.
    let v = |c: &str| c.repeat(1500);
    let doc = format!(
        "<!DOCTYPE r [<!ATTLIST e a CDATA '{}' b CDATA '{}' c CDATA '{}'><!ENTITY x SYSTEM 'x.xml'>]><r>&x;</r>",
        v("a"),
        v("b"),
        v("c")
    );
    let o = read_options(vec![ent("x.xml", "<e/>")]);
    let mut at_copy = 0;
    for o in (3_000..40_000).step_by(40).map(|t| ParseOptions {
        limits: Limits {
            at_amplification: Some(Factor(1.0)),
            at_threshold: Some(t),
            ..Limits::NONE
        },
        ..o.clone()
    }) {
        let ev = assert_equal_streams("stage 7 partial default values", doc.as_bytes(), &o);
        // A refusal after the child parser's creation (its DTD, 7865).
        if let Some(k) = ev.iter().position(|e| e.refused)
            && ev[..k].iter().filter(|e| e.line == 7865).count() == 2
        {
            at_copy += 1;
        }
    }
    assert!(
        at_copy > 0,
        "stage 7: no refusal during the child's DTD copy"
    );
}

/// Activation thresholds `from..to` one byte apart, amplification 1.0.
fn fine_sweep(o: &ParseOptions, from: u64, to: u64) -> impl Iterator<Item = ParseOptions> + '_ {
    (from..to).map(move |t| ParseOptions {
        limits: Limits {
            at_amplification: Some(Factor(1.0)),
            at_threshold: Some(t),
            ..Limits::NONE
        },
        ..o.clone()
    })
}

/// Refusals C tolerates: `getAttributeId` does not check the `lookup` of
/// an `xmlns:` prefix (`xmlparse.c` 7650-7652), so when the tracker
/// refuses that `PREFIX` (or the table's growth) the attribute id keeps a
/// NULL prefix with `xmlns` set and the parse goes on; `storeAtts` then
/// treats the attribute as an unprefixed one (4262, 4295) and `dtdCopy`
/// copies `xmlns` only with a prefix (8011). The port stopped with
/// `XML_ERROR_NO_MEMORY` there (found by the tightened fuzz check: the
/// port refused where C accepted).
#[test]
fn refusal_of_an_xmlns_prefix_is_tolerated() {
    let ns = ParseOptions {
        ns_separator: Some(b':'),
        ..base()
    };
    let comment = format!("<!--{}-->", "c".repeat(30_000));
    let docs = [
        // In a DTD the next allocation (the attribute's
        // `defaultAttForName` entry, 32 bytes) is refused too.
        "<!DOCTYPE p:e [<!ATTLIST q:e a CDATA #IMPLIED xmlns:z CDATA #REQUIRED>]>".to_string(),
        // In a start tag (`storeAtts` creates the attribute id) the parse
        // goes on: `xmlns:z` is then an ordinary attribute and binds
        // nothing, so `z:x` has an unbound prefix.
        "<r xmlns:z='urn:z' z:x='1'/>".to_string(),
        format!("<r xmlns:z='urn:z'>{comment}<z:x/></r>"),
        "<r xmlns:z='urn:z'><s xmlns:z='urn:y'/></r>".to_string(),
    ];
    let (mut tolerated, mut continued) = (0, 0);
    for doc in &docs {
        // Thresholds one byte apart with amplification 2.0: the refusal
        // falls on each allocation of the declarations in turn.
        for t in 5_000..8_000 {
            let o = ParseOptions {
                limits: Limits {
                    at_amplification: Some(Factor(2.0)),
                    at_threshold: Some(t),
                    ..Limits::NONE
                },
                ..ns.clone()
            };
            let ev = assert_equal_streams("xmlns prefix refusal", doc.as_bytes(), &o);
            let (c_trace, _) =
                expat_oracle::parse_trace_with_tracker_events(doc.as_bytes(), &o).expect("oracle");
            let c_trace = String::from_utf8_lossy(&c_trace).into_owned();
            // A refused `PREFIX` (24 bytes) or prefix table (520 bytes at
            // 8260) that C went on from.
            if ev
                .iter()
                .any(|e| e.refused && ((e.size == 24 && e.line == 8331) || e.line == 8260))
                && !result_line(c_trace.as_bytes()).starts_with("result error 1 ")
            {
                tolerated += 1;
                if c_trace.lines().any(|l| l.starts_with("start ")) {
                    continued += 1;
                }
            }
        }
    }
    assert!(
        tolerated > 0,
        "no run where C tolerated a refused xmlns prefix"
    );
    assert!(
        continued > 0,
        "no run where C reached the content after the refusal"
    );
}

/// What C hands the entity declaration handler after a refused
/// allocation: `XML_ROLE_ENTITY_VALUE` reports `dtd->entityValuePool`
/// before returning `storeEntityValue`'s error (`xmlparse.c` 5935-5958):
/// NULL when the pool's first block was refused (7126-7129), otherwise the
/// characters written before the refused growth (`poolAppend` writes the
/// whole characters that fit; `poolAppendChars` for a character reference
/// writes nothing when it cannot grow).
/// The port reported an empty value and the whole last token.
#[test]
fn refused_entity_value_reports_what_c_stored() {
    let (mut null_values, mut partial_values) = (0, 0);
    for piece in [
        "x",
        "\u{e9}",
        "\u{20ac}",
        "\u{1f600}",
        "&#x20AC;",
        "&#233;",
        "ab&#x1F600;c\u{e9}",
    ] {
        let value = piece.repeat(700);
        let text = format!("<!DOCTYPE r [<!ENTITY e '{value}'>]><r/>");
        for doc in [text.clone().into_bytes(), utf16le(&text)] {
            for o in threshold_sweep(&base()).step_by(3) {
                let ev = assert_equal_streams("entity value refusal", &doc, &o);
                let (c_trace, _) =
                    expat_oracle::parse_trace_with_tracker_events(&doc, &o).expect("oracle");
                let c_trace = String::from_utf8_lossy(&c_trace).into_owned();
                if !ev.iter().any(|e| e.refused) {
                    continue;
                }
                if c_trace.contains("entity e 0 ~ ") {
                    null_values += 1;
                } else if c_trace.lines().any(|l| l.starts_with("entity e 0 ")) {
                    partial_values += 1;
                }
            }
        }
    }
    assert!(null_values > 0, "no run with a NULL entity value");
    assert!(
        partial_values > 0,
        "no run with a partly stored entity value"
    );
}

/// A start tag whose buffer cannot grow is on `m_tagStack` already
/// (`xmlparse.c` 3764-3766, 3790-3792), so `XML_ParserFree` releases it
/// before its parent (1874-1887); the port put it on the free list.
#[test]
fn refused_tag_buffer_growth_released_first() {
    let mut refused = 0;
    for len in [40, 80, 200] {
        let text = format!("<a><{0}>x</{0}></a>", "b".repeat(len));
        for doc in [text.clone().into_bytes(), utf16le(&text)] {
            for o in fine_sweep(&base(), 3_100, 7_000).step_by(2) {
                let ev = assert_equal_streams("tag buffer refusal", &doc, &o);
                if ev.iter().any(|e| e.refused && e.line == 3790) {
                    refused += 1;
                }
            }
        }
    }
    assert!(refused > 0, "no refused tag buffer growth");
}
