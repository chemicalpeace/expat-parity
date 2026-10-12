//! Pins the trace format: the C shim and the Rust `TraceWriter` must both
//! produce these hand-written traces (spec: docs/ARCHITECTURE.md).

use expat_difftest::trace::{Status, TraceWriter, is_well_formed_trace};
use expat_oracle::{ParseOptions, parse_trace};

const RICH: &[u8] = include_bytes!("../vectors/handwritten-rich.xml");

const RICH_TRACE: &str = "xmldecl 1.0 UTF-8 1
doctype r ~ ~ 1
entity e 0 vAl ~ ~ ~ ~
entity p 1 x ~ ~ ~ ~
entity ext 0 ~ ~ ext.xml ~ ~
notation n ~ ~ pub
element r 52 ~(40 a(),40 b())
element a 30 ~()
attlist r d CDATA def 0
attlist r q %28x|y%29 ~ 1
enddoctype
start r 4
attr q x
attr xmlns:p urn:p
attr d def
comment %20c%20
text vAl
pi t d%20
cdata
text <&
endcdata
start p:a 2
attr p:z 1
text t%C3%A9%7E%28%2C%29%25
end p:a
end r
result ok 0 12 0 370
";

#[test]
fn oracle_produces_handwritten_rich_trace() {
    let t = parse_trace(RICH, &ParseOptions::default()).expect("oracle");
    assert_eq!(String::from_utf8_lossy(&t), RICH_TRACE);
}

#[test]
fn oracle_rich_trace_with_namespaces() {
    let opts = ParseOptions {
        ns_separator: Some(b'|'),
        ..ParseOptions::default()
    };
    let t = String::from_utf8(parse_trace(RICH, &opts).expect("oracle")).expect("ascii");
    assert!(t.contains("nsstart p urn:p\nstart r 2\nattr q x\nattr d def\n"));
    assert!(t.contains("start urn:p|a 2\nattr urn:p|z 1\n"));
    assert!(t.ends_with("end r\nnsend p\nresult ok 0 12 0 370\n"));
}

#[test]
fn writer_reproduces_content_part_of_rich_trace() {
    // The writer has no DTD declaration methods yet (they arrive with the DTD
    // port); check the element/content/result portion it can express.
    let mut w = TraceWriter::new();
    w.start(
        b"r",
        4,
        &[(b"q", b"x"), (b"xmlns:p", b"urn:p"), (b"d", b"def")],
    );
    w.comment(b" c ");
    w.text(b"v");
    w.text(b"Al");
    w.pi(b"t", b"d ");
    w.cdata();
    w.text(b"<&");
    w.end_cdata();
    w.start(b"p:a", 2, &[(b"p:z", b"1")]);
    w.text("té~(,)%".as_bytes());
    w.end(b"p:a");
    w.end(b"r");
    let got = String::from_utf8(w.finish(Status::Ok, 0, 12, 0, 370)).expect("ascii");
    let want = RICH_TRACE
        .split_once("enddoctype\n")
        .map(|(_, rest)| rest)
        .expect("marker present");
    assert_eq!(got, want);
}

#[test]
fn error_and_skipped_entity_traces() {
    let t = parse_trace(b"<a>&undef;</a>", &ParseOptions::default()).expect("oracle");
    assert_eq!(t, b"start a 0\nresult error 11 1 3 3\n");
    let t = parse_trace(
        b"<!DOCTYPE a SYSTEM \"x\"><a>&undef;</a>",
        &ParseOptions::default(),
    )
    .expect("oracle");
    let mut w = TraceWriter::new();
    w.doctype(b"a", Some(b"x"), None, false);
    w.end_doctype();
    w.start(b"a", 0, &[]);
    w.skipped(b"undef", false);
    w.end(b"a");
    assert_eq!(t, w.finish(Status::Ok, 0, 1, 37, 37));
}

#[test]
fn null_versus_tilde_are_distinct() {
    let mut w = TraceWriter::new();
    w.ns_start(None, Some(b"~"));
    let t = w.finish(Status::Error, 3, 1, 0, 0);
    assert_eq!(t, b"nsstart ~ %7E\nresult error 3 1 0 0\n");
    assert!(is_well_formed_trace(&t));
}

#[test]
fn well_formedness_checker_rejects_garbage() {
    assert!(!is_well_formed_trace(b""));
    assert!(!is_well_formed_trace(b"start a 0\n"));
    assert!(!is_well_formed_trace(b"bogus x\nresult ok 0 1 0 0\n"));
    assert!(!is_well_formed_trace(b"result ok 0 1 0\n"));
    assert!(is_well_formed_trace(b"result ok 0 1 0 0\n"));
}

#[test]
fn limit_digest_and_accounting_lines() {
    // Module 11 options: setter results first, character data as
    // `textdigest <len> <fnv1a64>`, counters before the result line. The
    // C shim and the writer must agree; the digest of "ab" is pinned.
    let doc = b"<r>ab</r>";
    let opts = ParseOptions {
        limits: expat_oracle::Limits {
            bl_amplification: Some(expat_oracle::Factor(0.5)),
            bl_threshold: Some(7),
            ..expat_oracle::Limits::NONE
        },
        text_digest: true,
        trace_accounting: true,
        ..ParseOptions::default()
    };
    let expected = "limit bl-amplification 0
limit bl-threshold 1
start r 0
textdigest 2 089c4407b545986a
end r
accounting 9 0
result ok 0 1 9 9
";
    let c = parse_trace(doc, &opts).expect("oracle");
    assert_eq!(String::from_utf8_lossy(&c), expected);
    assert!(is_well_formed_trace(&c));
    let mut w = TraceWriter::with_text_digest();
    w.limit("bl-amplification", false);
    w.limit("bl-threshold", true);
    w.start(b"r", 0, &[]);
    w.text(b"a");
    w.text(b"b");
    w.end(b"r");
    w.accounting(9, 0);
    let r = w.finish(Status::Ok, 0, 1, 9, 9);
    assert_eq!(String::from_utf8_lossy(&r), expected);
}
