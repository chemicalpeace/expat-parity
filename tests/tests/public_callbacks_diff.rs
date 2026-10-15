//! Module 12, slice 4 (D-041): the public callback API
//! (`expat::Parser::parse_with` with an `expat::Handler`) against the C
//! oracle.
//!
//! The public side is `expat_difftest::public::public_trace`: a public
//! handler whose `EVENTS` is the mask under test writes the canonical
//! trace. The C side is `expat_oracle::parse_trace` with
//! `handler_mask: Some(oracle_mask(events))`, the PR 1 mask (D-041): the
//! nine handlers of `Events` as the mask says, every other trace handler
//! unset, as `parse_with` sets them. Both use the oracle's hash salt and
//! the same feeding (one final call, or fixed-size chunks and an empty
//! final call, stopping at the first error).
//!
//! Compared, byte for byte: every event line (start with C's specified
//! count and the attributes, end, text, pi, comment, cdata, endcdata,
//! nsstart, nsend), the result, the error code, and the line, column and
//! byte index of every error. Not compared, because the public API cannot
//! observe them (documented in `public.rs` and D-041):
//!
//! - the position after a run without an error (`result ok` lines carry
//!   `-` on both sides; the core-level mask matrix of PR 1 compares it);
//! - where character data is split: the trace format coalesces adjacent
//!   `character_data` calls on both sides;
//! - changing the events between the calls of one document (or mixing
//!   `parse` and `parse_with`): the oracle sets the mask once per
//!   document.
//!
//! The tests:
//!
//! - T2 (540 upstream vectors and 600 generated documents, the D-037
//!   seeds) under the masks `ALL`, `NONE` and each event alone, fed in one
//!   call, byte by byte, and in 7- and 64-byte chunks;
//! - S3: the same masks with namespace processing off, with separator `|`
//!   and with separator NUL, under nine encoding settings (none, six known
//!   names in mixed case, the empty name and an unknown name), on T2 and
//!   on namespace and encoding documents, in one call and in 7-byte
//!   chunks;
//! - `parse` unchanged: on T2, every call of `Parser::parse` returns what
//!   `parse_with` with `Events::NONE` returns, in one call and byte by
//!   byte, and the C trace with mask `NONE` has no event line.

use expat::{Events, Handler, Parser};
use expat_difftest::docgen::{self, Rng};
use expat_difftest::port::ORACLE_HASH_SALT;
use expat_difftest::public::{mask_ok_position, oracle_mask, public_trace, supported_events};
use expat_difftest::trace::first_difference;
use expat_difftest::vectors::upstream;
use expat_oracle::{ParseOptions, parse_trace};

/// The T2 corpus: the 540 upstream vectors and 2 x 300 generated documents
/// (alternately without and with a document type declaration).
fn t2() -> Vec<(String, Vec<u8>)> {
    let mut docs: Vec<(String, Vec<u8>)> =
        upstream().iter().map(|v| (v.id.clone(), v.doc())).collect();
    assert_eq!(docs.len(), 540);
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

/// Documents for the nine events and namespace processing: every event,
/// defaulted attributes, namespace declarations (prefixed, default,
/// undeclared, nested, separator bytes in names), errors after events,
/// and the documents of `crates/expat/tests/public_callbacks.rs`.
fn event_documents() -> Vec<(String, Vec<u8>)> {
    let docs: [&[u8]; 24] = [
        b"<?pi x?><a k='v'>t<!--c--><![CDATA[<d>]]><e/></a>",
        b"<!DOCTYPE a [<!ATTLIST a d CDATA 'x'>]><a s='1'/>",
        b"<p:a xmlns:p='u' xmlns='v' p:x='1'><b xmlns=''/></p:a>",
        b"<p:a xmlns:p='u'/>",
        b"<a>x<!--c--></a>",
        b"<a><b></a>",
        b"<a><?p d?><!--c--></a>",
        b"<a></b>",
        b"<a>&x;</a>",
        b"<a/>",
        b"<p:a/>",
        b"<a xmlns='u'><b/><c xmlns='w'>t</c></a>",
        b"<a xmlns:p='u'><p:b p:x='1' x='2'/></a>",
        b"<a xmlns:p='u'><b xmlns:p=''/></a>",
        b"<a xmlns='u|v'/>",
        b"<a xmlns:p='u' xmlns:q='u' p:x='1' q:x='2'/>",
        b"<!DOCTYPE a [<!ATTLIST a xmlns:p CDATA 'u'>]><p:a/>",
        b"<a><![CDATA[]]><![CDATA[x]]>y<?t?></a>",
        b"<a>&lt;&#233;&#x10000;</a>",
        b"<a>\r\nline\rtwo\n</a>",
        b"<?xml version='1.0'?><!--before--><a/><!--after--><?after?>",
        b"<a><!--x--></a><b/>",
        b"<a b='1' b='2'/>",
        b"<a><![CDATA[x</a>",
    ];
    docs.iter()
        .enumerate()
        .map(|(i, d)| (format!("event doc #{i}"), d.to_vec()))
        .collect()
}

/// Documents for encoding settings: Latin-1 and ASCII bytes, UTF-8 with
/// and without a byte order mark, UTF-16LE/BE with and without a byte
/// order mark, and XML declarations naming another encoding.
fn encoding_documents() -> Vec<(String, Vec<u8>)> {
    let mut docs: Vec<(String, Vec<u8>)> = vec![
        ("ascii".into(), b"<a>plain</a>".to_vec()),
        ("latin1 byte".into(), b"<a k='\xe9'>caf\xe9</a>".to_vec()),
        ("utf8 e-acute".into(), b"<a>caf\xc3\xa9</a>".to_vec()),
        ("utf8 bom".into(), b"\xef\xbb\xbf<a>x</a>".to_vec()),
        (
            "decl latin1".into(),
            b"<?xml version='1.0' encoding='ISO-8859-1'?><a>\xe9</a>".to_vec(),
        ),
        (
            "decl utf-16 in utf-8".into(),
            b"<?xml version='1.0' encoding='UTF-16'?><a/>".to_vec(),
        ),
        (
            "decl unknown".into(),
            b"<?xml version='1.0' encoding='KOI8-R'?><a/>".to_vec(),
        ),
    ];
    for (name, text) in [
        (
            "plain",
            "<a k='\u{e9}'><!--c\u{e9}--><?p \u{e9}?>caf\u{e9}</a>",
        ),
        ("ns", "<p:a xmlns:p='u\u{e9}'><![CDATA[\u{10000}]]></p:a>"),
        ("error", "<a></b>"),
    ] {
        let le: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let be: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        docs.push((
            format!("utf-16le bom {name}"),
            [&[0xff, 0xfe][..], &le].concat(),
        ));
        docs.push((
            format!("utf-16be bom {name}"),
            [&[0xfe, 0xff][..], &be].concat(),
        ));
        docs.push((format!("utf-16le {name}"), le));
        docs.push((format!("utf-16be {name}"), be));
    }
    docs
}

/// Counts of one comparison loop.
#[derive(Debug, Default)]
struct Counts {
    runs: usize,
    errors: usize,
    event_lines: usize,
    failures: Vec<String>,
}

impl Counts {
    /// Compares the public trace and the C trace of `doc` under `opts`
    /// and `events`.
    fn compare(&mut self, id: &str, doc: &[u8], opts: &ParseOptions, name: &str, events: Events) {
        let c = parse_trace(
            doc,
            &ParseOptions {
                handler_mask: Some(oracle_mask(events)),
                ..opts.clone()
            },
        )
        .expect("oracle");
        let c = mask_ok_position(&c);
        let rust = public_trace(doc, opts, events);
        self.runs += 1;
        let text = String::from_utf8_lossy(&c);
        if text
            .lines()
            .last()
            .is_some_and(|l| l.starts_with("result error"))
        {
            self.errors += 1;
        }
        self.event_lines += text.lines().count().saturating_sub(1);
        if c != rust {
            let d = first_difference(&c, &rust);
            if self.failures.len() < 20 {
                self.failures.push(format!(
                    "{id} [{name}] enc={:?} ns={:?} chunk={}: {d:?}",
                    opts.encoding, opts.ns_separator, opts.chunk_size
                ));
            }
        }
    }

    fn check(&self, what: &str) {
        eprintln!(
            "{what}: {} runs ({} with an error), {} event lines compared",
            self.runs, self.errors, self.event_lines
        );
        assert!(
            self.failures.is_empty(),
            "{what}: {} differences, first: {:#?}",
            self.failures.len(),
            self.failures
        );
        // The comparisons are not vacuous: both outcomes and events occur.
        assert!(self.errors > 0 && self.errors < self.runs, "{what}");
        assert!(self.event_lines > 0, "{what}");
    }
}

/// T2 under every supported mask, with the input fed in `chunk`-byte
/// pieces (0: one call).
fn t2_with_chunk(chunk: usize) -> Counts {
    let mut n = Counts::default();
    let mut docs = t2();
    docs.extend(event_documents());
    let opts = ParseOptions {
        chunk_size: chunk,
        ..ParseOptions::default()
    };
    for (id, doc) in &docs {
        for (name, events) in supported_events() {
            n.compare(id, doc, &opts, name, events);
        }
    }
    n
}

#[test]
fn t2_masks_one_call() {
    t2_with_chunk(0).check("T2, one call");
}

#[test]
fn t2_masks_byte_by_byte() {
    t2_with_chunk(1).check("T2, byte by byte");
}

#[test]
fn t2_masks_7_byte_chunks() {
    t2_with_chunk(7).check("T2, 7-byte chunks");
}

#[test]
fn t2_masks_64_byte_chunks() {
    t2_with_chunk(64).check("T2, 64-byte chunks");
}

/// The encoding settings of the S3 matrix: none, the six names libexpat
/// knows (in mixed case), the empty name and an unknown name.
const ENCODINGS: [Option<&str>; 9] = [
    None,
    Some("UTF-8"),
    Some("utf-16"),
    Some("UTF-16LE"),
    Some("UTF-16BE"),
    Some("iso-8859-1"),
    Some("US-ASCII"),
    Some(""),
    Some("KOI8-R"),
];

/// The S3 matrix for one namespace setting: every encoding setting and
/// every supported mask on T2, the event documents and the encoding
/// documents, in one call and in 7-byte chunks.
fn s3_with_separator(sep: Option<u8>) -> Counts {
    let mut docs = t2();
    docs.extend(event_documents());
    docs.extend(encoding_documents());
    let mut n = Counts::default();
    for encoding in ENCODINGS {
        for chunk in [0, 7] {
            let opts = ParseOptions {
                encoding: encoding.map(String::from),
                ns_separator: sep,
                chunk_size: chunk,
                ..ParseOptions::default()
            };
            for (id, doc) in &docs {
                for (name, events) in supported_events() {
                    n.compare(id, doc, &opts, name, events);
                }
            }
        }
    }
    n
}

#[test]
fn s3_namespaces_off() {
    s3_with_separator(None).check("S3, namespaces off");
}

#[test]
fn s3_namespace_separator_bar() {
    let n = s3_with_separator(Some(b'|'));
    n.check("S3, separator '|'");
}

#[test]
fn s3_namespace_separator_nul() {
    s3_with_separator(Some(0)).check("S3, separator NUL");
}

/// Namespace declaration lines occur under namespace processing, so the
/// namespace events are really compared (and never occur without it).
#[test]
fn namespace_events_occur_only_with_namespaces() {
    let docs = event_documents();
    let count = |sep: Option<u8>| -> usize {
        let opts = ParseOptions {
            ns_separator: sep,
            ..ParseOptions::default()
        };
        docs.iter()
            .map(|(_, d)| {
                let t = public_trace(d, &opts, Events::ALL);
                String::from_utf8_lossy(&t)
                    .lines()
                    .filter(|l| l.starts_with("nsstart") || l.starts_with("nsend"))
                    .count()
            })
            .sum()
    };
    assert_eq!(count(None), 0);
    assert!(count(Some(b'|')) > 0);
    assert!(count(Some(0)) > 0);
}

struct Nothing;

impl Handler for Nothing {
    const EVENTS: Events = Events::NONE;
}

/// `parse` is unchanged by this slice: on T2 and the event documents,
/// each call of `Parser::parse` returns what the same call of `parse_with`
/// with `Events::NONE` returns (in one call and byte by byte), and C sets
/// no handler for the mask `NONE` (its trace is only the result line). The
/// results of `parse` themselves are compared with C by
/// `public_api_diff.rs`, which this slice does not change.
#[test]
fn parse_equals_parse_with_none() {
    let mut docs = t2();
    docs.extend(event_documents());
    let mut calls = 0;
    for (id, doc) in &docs {
        let mut a = Parser::new(ORACLE_HASH_SALT);
        let mut b = Parser::new(ORACLE_HASH_SALT);
        assert_eq!(
            a.parse(doc, true),
            b.parse_with(&mut Nothing, doc, true),
            "{id}"
        );
        let mut a = Parser::new(ORACLE_HASH_SALT);
        let mut b = Parser::new(ORACLE_HASH_SALT);
        for byte in doc.chunks(1) {
            let (ra, rb) = (
                a.parse(byte, false),
                b.parse_with(&mut Nothing, byte, false),
            );
            assert_eq!(ra, rb, "{id}");
            calls += 1;
            if ra.is_err() {
                break;
            }
        }
        assert_eq!(
            a.parse(b"", true),
            b.parse_with(&mut Nothing, b"", true),
            "{id}"
        );
        calls += 2;
        let c = parse_trace(
            doc,
            &ParseOptions {
                handler_mask: Some(oracle_mask(Events::NONE)),
                ..ParseOptions::default()
            },
        )
        .expect("oracle");
        assert_eq!(String::from_utf8_lossy(&c).lines().count(), 1, "{id}");
    }
    eprintln!(
        "parse = parse_with(NONE): {calls} calls on {} documents",
        docs.len()
    );
}

/// The comparison is sensitive: a mask that differs from what the public
/// handler sets is detected (one event alone on each side, swapped).
#[test]
fn comparison_detects_a_wrong_mask() {
    let doc = b"<?pi x?><a k='v'>t<!--c--><![CDATA[<d>]]><e/></a>";
    let opts = ParseOptions::default();
    let comment = Events::NONE.comment(true);
    let pi = Events::NONE.processing_instruction(true);
    let c = mask_ok_position(
        &parse_trace(
            doc,
            &ParseOptions {
                handler_mask: Some(oracle_mask(pi)),
                ..opts.clone()
            },
        )
        .expect("oracle"),
    );
    assert_ne!(c, public_trace(doc, &opts, comment));
    assert_eq!(c, public_trace(doc, &opts, pi));
}

/// The oracle's `parse_trace` creates a namespace parser for `Some(0)`
/// (`XML_ParserCreateNS(enc, '\0')`, D-041; before this slice it read
/// `Some(0)` as no namespace processing): `<p:a/>` fails with an unbound
/// prefix as with `'|'`, and not without namespaces; and the port's
/// internal trace adapter, which always created a namespace parser for
/// `Some(0)`, equals C on T2 with separator NUL (every line, the
/// position after a successful parse included).
#[test]
fn oracle_separator_nul_is_a_namespace_parser() {
    let trace = |sep: Option<u8>, doc: &[u8]| {
        let o = ParseOptions {
            ns_separator: sep,
            ..ParseOptions::default()
        };
        String::from_utf8(parse_trace(doc, &o).expect("oracle")).expect("utf-8")
    };
    // XML_ERROR_UNBOUND_PREFIX is 27.
    assert_eq!(trace(Some(0), b"<p:a/>"), trace(Some(b'|'), b"<p:a/>"));
    assert!(trace(Some(0), b"<p:a/>").starts_with("result error 27 "));
    assert!(trace(None, b"<p:a/>").contains("result ok "));
    assert!(trace(Some(0), b"<p:a xmlns:p='u'/>").contains("start ua 0"));
    let mut docs = t2();
    docs.extend(event_documents());
    let opts = ParseOptions {
        ns_separator: Some(0),
        ..ParseOptions::default()
    };
    for (id, doc) in &docs {
        let c = parse_trace(doc, &opts).expect("oracle");
        let rust = expat_difftest::port::port_trace(doc, &opts);
        assert_eq!(c, rust, "{id}: {:?}", first_difference(&c, &rust));
    }
}
