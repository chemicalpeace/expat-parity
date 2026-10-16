//! Module 12, slice 4 (D-041): the public callback API on its own.
//!
//! Like `public_api.rs`, this file uses only the public API, never
//! `expat::internal`, so `cargo test -p expat` and `cargo test -p expat
//! --no-default-features` run it with the `internal-api` feature off. The
//! expected events are libexpat 2.9.0's for the same input;
//! `tests/tests/public_callbacks_diff.rs` compares the same API with the C
//! oracle on many documents and every mask it supports, these documents
//! included.

use expat::{ErrorCode, Events, Handler, Options, Parser};

const SALT: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

/// One event, with owned data.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ev {
    Start(Vec<u8>, Vec<(Vec<u8>, Vec<u8>)>, usize),
    End(Vec<u8>),
    Text(Vec<u8>),
    Pi(Vec<u8>, Vec<u8>),
    Comment(Vec<u8>),
    CdataStart,
    CdataEnd,
    NsStart(Option<Vec<u8>>, Option<Vec<u8>>),
    NsEnd(Option<Vec<u8>>),
}

/// Records every event, merging adjacent character data.
#[derive(Default)]
struct Log(Vec<Ev>);

impl Log {
    fn push(&mut self, e: Ev) {
        self.0.push(e);
    }
}

impl Handler for Log {
    fn start_element(&mut self, name: &[u8], attributes: &[(&[u8], &[u8])], specified: usize) {
        let atts = attributes
            .iter()
            .map(|(n, v)| (n.to_vec(), v.to_vec()))
            .collect();
        self.push(Ev::Start(name.to_vec(), atts, specified));
    }
    fn end_element(&mut self, name: &[u8]) {
        self.push(Ev::End(name.to_vec()));
    }
    fn character_data(&mut self, data: &[u8]) {
        if let Some(Ev::Text(t)) = self.0.last_mut() {
            t.extend_from_slice(data);
        } else {
            self.push(Ev::Text(data.to_vec()));
        }
    }
    fn processing_instruction(&mut self, target: &[u8], data: &[u8]) {
        self.push(Ev::Pi(target.to_vec(), data.to_vec()));
    }
    fn comment(&mut self, data: &[u8]) {
        self.push(Ev::Comment(data.to_vec()));
    }
    fn start_cdata_section(&mut self) {
        self.push(Ev::CdataStart);
    }
    fn end_cdata_section(&mut self) {
        self.push(Ev::CdataEnd);
    }
    fn start_namespace_decl(&mut self, prefix: Option<&[u8]>, uri: Option<&[u8]>) {
        self.push(Ev::NsStart(
            prefix.map(<[u8]>::to_vec),
            uri.map(<[u8]>::to_vec),
        ));
    }
    fn end_namespace_decl(&mut self, prefix: Option<&[u8]>) {
        self.push(Ev::NsEnd(prefix.map(<[u8]>::to_vec)));
    }
}

fn b(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

fn start(name: &str, atts: &[(&str, &str)], specified: usize) -> Ev {
    Ev::Start(
        b(name),
        atts.iter().map(|(n, v)| (b(n), b(v))).collect(),
        specified,
    )
}

fn parse_all(p: &mut Parser, doc: &[u8]) -> (Vec<Ev>, Result<(), expat::Error>) {
    let mut log = Log::default();
    let r = p.parse_with(&mut log, doc, true);
    (log.0, r)
}

#[test]
fn content_events_in_order() {
    let doc = b"<?pi x?><a k='v'>t<!--c--><![CDATA[<d>]]><e/></a>";
    let (events, r) = parse_all(&mut Parser::new(SALT), doc);
    assert_eq!(r, Ok(()));
    assert_eq!(
        events,
        [
            Ev::Pi(b("pi"), b("x")),
            start("a", &[("k", "v")], 1),
            Ev::Text(b("t")),
            Ev::Comment(b("c")),
            Ev::CdataStart,
            Ev::Text(b("<d>")),
            Ev::CdataEnd,
            start("e", &[], 0),
            Ev::End(b("e")),
            Ev::End(b("a")),
        ]
    );
}

#[test]
fn defaulted_attributes_follow_the_specified_ones() {
    let doc = b"<!DOCTYPE a [<!ATTLIST a d CDATA 'x'>]><a s='1'/>";
    let (events, r) = parse_all(&mut Parser::new(SALT), doc);
    assert_eq!(r, Ok(()));
    assert_eq!(
        events,
        [start("a", &[("s", "1"), ("d", "x")], 1), Ev::End(b("a"))]
    );
}

#[test]
fn namespace_declarations_and_expanded_names() {
    let doc = b"<p:a xmlns:p='u' xmlns='v' p:x='1'><b xmlns=''/></p:a>";
    let mut p = Parser::with_options(SALT, Options::new().namespace_separator(b'|'));
    let (events, r) = parse_all(&mut p, doc);
    assert_eq!(r, Ok(()));
    assert_eq!(
        events,
        [
            Ev::NsStart(Some(b("p")), Some(b("u"))),
            Ev::NsStart(None, Some(b("v"))),
            start("u|a", &[("u|x", "1")], 1),
            Ev::NsStart(None, None),
            start("b", &[], 0),
            Ev::End(b("b")),
            Ev::NsEnd(None),
            Ev::End(b("u|a")),
            Ev::NsEnd(None),
            Ev::NsEnd(Some(b("p"))),
        ]
    );
    // Separator 0: namespace name and local name are joined directly.
    let mut p = Parser::with_options(SALT, Options::new().namespace_separator(0));
    let (events, _) = parse_all(&mut p, b"<p:a xmlns:p='u'/>");
    assert_eq!(events.get(1), Some(&start("ua", &[], 0)));
    // Without namespace processing the declaration is an attribute.
    let (events, _) = parse_all(&mut Parser::new(SALT), b"<p:a xmlns:p='u'/>");
    assert_eq!(
        events,
        [start("p:a", &[("xmlns:p", "u")], 1), Ev::End(b("p:a"))]
    );
}

#[test]
fn events_mask_selects_the_calls() {
    struct Only(Log);
    impl Handler for Only {
        const EVENTS: Events = Events::NONE.end_element(true).comment(true);
        fn start_element(&mut self, n: &[u8], a: &[(&[u8], &[u8])], s: usize) {
            self.0.start_element(n, a, s);
        }
        fn end_element(&mut self, n: &[u8]) {
            self.0.end_element(n);
        }
        fn character_data(&mut self, d: &[u8]) {
            self.0.character_data(d);
        }
        fn comment(&mut self, d: &[u8]) {
            self.0.comment(d);
        }
    }
    let mut h = Only(Log::default());
    let mut p = Parser::new(SALT);
    assert_eq!(p.parse_with(&mut h, b"<a>x<!--c--></a>", true), Ok(()));
    assert_eq!(h.0.0, [Ev::Comment(b("c")), Ev::End(b("a"))]);
}

#[test]
fn events_across_calls_and_an_error() {
    let mut p = Parser::new(SALT);
    let mut log = Log::default();
    assert_eq!(p.parse_with(&mut log, b"<a><b>", false), Ok(()));
    let r = p.parse_with(&mut log, b"</a>", true);
    assert_eq!(
        r.map_err(|e| (e.code(), e.line(), e.column(), e.byte_index())),
        Err((ErrorCode::TagMismatch, 1, 8, 8))
    );
    // The events before the error stay reported.
    assert_eq!(log.0, [start("a", &[], 0), start("b", &[], 0)]);
}

#[test]
fn parse_and_parse_with_none_agree() {
    struct Nothing;
    impl Handler for Nothing {
        const EVENTS: Events = Events::NONE;
    }
    for doc in [&b"<a><?p d?><!--c--></a>"[..], b"<a></b>", b"<a>&x;</a>"] {
        let r1 = Parser::new(SALT).parse(doc, true);
        let r2 = Parser::new(SALT).parse_with(&mut Nothing, doc, true);
        assert_eq!(r1, r2);
    }
}

#[test]
fn utf16_input_is_reported_in_utf8() {
    let mut doc = vec![0xFF, 0xFE];
    for unit in "<d>\u{e9}</d>".encode_utf16() {
        doc.extend_from_slice(&unit.to_le_bytes());
    }
    let (events, r) = parse_all(&mut Parser::new(SALT), &doc);
    assert_eq!(r, Ok(()));
    assert_eq!(events.get(1), Some(&Ev::Text(b("\u{e9}"))));
}
