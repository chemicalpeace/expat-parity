//! Unit tests of the parser core. The differential tests against C are in
//! `tests/tests/parse_diff.rs` and `tests/tests/buffer_diff.rs`; these pin
//! a few behaviours with expected values taken from libexpat 2.9.0, small
//! enough to run under Miri.

use alloc::vec::Vec;

use super::*;

#[derive(Default)]
struct Events(Vec<Vec<u8>>);

impl Events {
    fn push(&mut self, parts: &[&[u8]]) {
        self.0.push(parts.join(&b' '));
    }
}

impl Handler for Events {
    fn xml_decl(&mut self, version: Option<&[u8]>, encoding: Option<&[u8]>, standalone: i32) {
        let sa = alloc::format!("{standalone}");
        self.push(&[
            b"xmldecl",
            version.unwrap_or(b"~"),
            encoding.unwrap_or(b"~"),
            sa.as_bytes(),
        ]);
    }
    fn start_element(&mut self, name: &[u8], atts: &[(&[u8], &[u8])], specified: i32) {
        let n = alloc::format!("{specified}");
        self.push(&[b"start", name, n.as_bytes()]);
        for (k, v) in atts {
            self.push(&[b"attr", k, v]);
        }
    }
    fn end_element(&mut self, name: &[u8]) {
        self.push(&[b"end", name]);
    }
    fn character_data(&mut self, data: &[u8]) {
        self.push(&[b"text", data]);
    }
    fn processing_instruction(&mut self, target: &[u8], data: &[u8]) {
        self.push(&[b"pi", target, data]);
    }
    fn comment(&mut self, data: &[u8]) {
        self.push(&[b"comment", data]);
    }
    fn start_cdata_section(&mut self) {
        self.push(&[b"cdata"]);
    }
    fn end_cdata_section(&mut self) {
        self.push(&[b"endcdata"]);
    }
    fn start_namespace_decl(&mut self, prefix: Option<&[u8]>, uri: Option<&[u8]>) {
        self.push(&[b"nsstart", prefix.unwrap_or(b"~"), uri.unwrap_or(b"~")]);
    }
    fn end_namespace_decl(&mut self, prefix: Option<&[u8]>) {
        self.push(&[b"nsend", prefix.unwrap_or(b"~")]);
    }
}

fn all() -> HandlerSet {
    HandlerSet {
        xml_decl: true,
        start_element: true,
        end_element: true,
        character_data: true,
        processing_instruction: true,
        comment: true,
        start_cdata_section: true,
        end_cdata_section: true,
        start_namespace_decl: true,
        end_namespace_decl: true,
        skipped_entity: true,
        start_doctype_decl: true,
    }
}

fn parse(doc: &[u8], ns: Option<u8>) -> (Status, Parser, Vec<Vec<u8>>) {
    let mut p = Parser::new(None, ns, [0; 16]);
    p.set_handlers(all());
    let mut ev = Events::default();
    let st = p.parse(&mut ev, doc, true);
    (st, p, ev.0)
}

fn texts(ev: &[Vec<u8>]) -> Vec<&str> {
    ev.iter()
        .map(|e| core::str::from_utf8(e).unwrap_or("<non-utf8>"))
        .collect()
}

#[test]
fn events_of_a_small_document() {
    let (st, mut p, ev) = parse(
        b"<?xml version='1.0'?><d a=' x\ny ' b='&lt;&#65;'>t<![CDATA[c]]><!--k--><?p q?></d>",
        None,
    );
    assert_eq!(st, Status::Ok);
    assert_eq!(
        texts(&ev),
        [
            "xmldecl 1.0 ~ -1",
            "start d 4",
            "attr a  x y ",
            "attr b <A",
            "text t",
            "cdata",
            "text c",
            "endcdata",
            "comment k",
            "pi p q",
            "end d",
        ]
    );
    assert_eq!(p.error_code(), Stop::Error(XmlError::None));
    assert_eq!(p.parsing_status(), Parsing::Finished);
    // C: `result ok 0 2 51 81` (the attribute value spans a line break).
    assert_eq!(
        (
            p.current_line_number(),
            p.current_column_number(),
            p.current_byte_index()
        ),
        (2, 51, 81)
    );
}

#[test]
fn namespaces_are_expanded_and_scoped() {
    let (st, _, ev) = parse(b"<p:a xmlns:p='urn:p' xmlns='urn:d' p:x='1'><b/></p:a>", Some(b'|'));
    assert_eq!(st, Status::Ok);
    assert_eq!(
        texts(&ev),
        [
            "nsstart p urn:p",
            "nsstart ~ urn:d",
            "start urn:p|a 2",
            "attr urn:p|x 1",
            "start urn:d|b 0",
            "end urn:d|b",
            "end urn:p|a",
            "nsend ~",
            "nsend p",
        ]
    );
}

#[test]
fn namespace_errors() {
    for (doc, code) in [
        (&b"<a:b/>"[..], XmlError::UnboundPrefix),
        (b"<a xmlns:p=''/>", XmlError::UndeclaringPrefix),
        (b"<a xmlns:xmlns='u'/>", XmlError::ReservedPrefixXmlns),
        (b"<a xmlns:xml='u'/>", XmlError::ReservedPrefixXml),
        (b"<a xmlns:p='http://www.w3.org/2000/xmlns/'/>", XmlError::ReservedNamespaceUri),
        (b"<a xmlns:p='u' xmlns:q='u' p:x='' q:x=''/>", XmlError::DuplicateAttribute),
        (b"<a xmlns:p='u|v'/>", XmlError::Syntax),
    ] {
        let (st, p, _) = parse(doc, Some(b'|'));
        assert_eq!((st, p.error_code()), (Status::Error, Stop::Error(code)), "{doc:?}");
    }
}

#[test]
fn error_positions() {
    let (st, mut p, _) = parse(b"<a>\n  <b></c></a>", None);
    assert_eq!(st, Status::Error);
    assert_eq!(p.error_code(), Stop::Error(XmlError::TagMismatch));
    assert_eq!(
        (
            p.current_line_number(),
            p.current_column_number(),
            p.current_byte_index()
        ),
        (2, 7, 11)
    );
}

#[test]
fn doctype_is_outside_the_ported_scope() {
    let (st, _, _) = parse(b"<!DOCTYPE d><d/>", None);
    let mut p = Parser::new(None, None, [0; 16]);
    assert_eq!(p.parse(&mut Events::default(), b"<!DOCTYPE d><d/>", true), Status::Error);
    assert_eq!(st, Status::Error);
    assert_eq!(p.error_code(), Stop::DtdNotPorted);
}

#[test]
fn epilog_cr_lf_split_counts_two_lines_like_c() {
    // Q-1 (docs/DIVERGENCES.md): replicated.
    let mut p = Parser::new(None, None, [0; 16]);
    let mut ev = Events::default();
    for b in b"<a/>\r\n" {
        assert_eq!(p.parse(&mut ev, core::slice::from_ref(b), false), Status::Ok);
    }
    assert_eq!(p.parse(&mut ev, b"", true), Status::Ok);
    assert_eq!((p.current_line_number(), p.current_column_number()), (3, 0));
}

#[test]
fn buffer_api() {
    let mut p = Parser::new(None, None, [0; 16]);
    let mut ev = Events::default();
    assert_eq!(p.parse_buffer(&mut ev, 0, false), Status::Error);
    assert_eq!(p.error_code(), Stop::Error(XmlError::NoBuffer));
    assert!(p.get_buffer(-1).is_none());
    assert_eq!(p.error_code(), Stop::Error(XmlError::NoMemory));
    let doc = b"<d/>";
    let Some(b) = p.get_buffer(10) else {
        panic!("buffer refused");
    };
    assert_eq!(b.len(), 10);
    b[..doc.len()].copy_from_slice(doc);
    // The first buffer is twice INIT_BUFFER_SIZE.
    assert_eq!(p.buffer_state().buffer_lim, 2048);
    assert_eq!(p.parse_buffer(&mut ev, 4, true), Status::Ok);
    assert!(p.get_buffer(1).is_none());
    assert_eq!(p.error_code(), Stop::Error(XmlError::Finished));
    assert_eq!(p.parse(&mut ev, b"x", true), Status::Error);
}

#[test]
fn encodings_and_declaration() {
    // UTF-16LE with a byte order mark, then a Latin-1 document.
    let (st, _, ev) = parse(b"\xff\xfe<\0d\0>\0\xe9\0<\0/\0d\0>\0", None);
    assert_eq!(st, Status::Ok);
    assert_eq!(texts(&ev), ["start d 0", "text é", "end d"]);
    let (st, _, ev) = parse(b"<?xml version='1.0' encoding='ISO-8859-1'?><d>\xe9</d>", None);
    assert_eq!(st, Status::Ok);
    assert_eq!(texts(&ev)[2], "text é");
    let (st, p, _) = parse(b"<?xml version='1.0' encoding='x-y'?><d/>", None);
    assert_eq!((st, p.error_code()), (Status::Error, Stop::Error(XmlError::UnknownEncoding)));
}

#[test]
fn accounting_counts_direct_and_expanded_bytes() {
    let (_, p, _) = parse(b"<d>&amp;</d>", None);
    let a = p.accounting();
    assert_eq!((a.count_bytes_direct, a.count_bytes_indirect), (12, 1));
    // Nothing read directly: the factor is (22 + indirect) / 22.
    let mut acc = Accounting {
        activation_threshold_bytes: 0,
        maximum_amplification_factor: 1.0,
        ..Accounting::default()
    };
    let tok = crate::tok::Scan::Token(crate::tok::Tok::DataChars);
    assert!(!acc.diff_tolerated(tok, 1, Account::EntityExpansion));
    assert!(acc.diff_tolerated(tok, 1, Account::None));
    assert!(acc.diff_tolerated(crate::tok::Scan::Partial, 1, Account::Direct));
}
