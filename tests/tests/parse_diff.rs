//! Module 10: the port's parser against the C oracle, compared by
//! canonical trace (docs/ARCHITECTURE.md) on the upstream vectors and on
//! generated documents, under many option sets (encodings, namespace
//! separators, chunkings, `XML_Parse` versus the buffer API, reparse
//! deferral on and off).
//!
//! Documents with a document type declaration stop the port with
//! `Stop::DtdNotPorted` until module 11; they are counted, never compared.

use expat_difftest::diff::option_matrix;
use expat_difftest::docgen::{Rng, document, dtd_document};
use expat_difftest::port::{PortTrace, port_trace};
use expat_difftest::trace::first_difference;
use expat_difftest::vectors::upstream;
use expat_oracle::{ParamEntityParsing, ParseOptions, parse_trace};

#[derive(Default)]
struct Tally {
    compared: usize,
    unported: usize,
    failures: Vec<String>,
}

impl Tally {
    /// Compares one (document, options) pair; returns false if the port
    /// stopped at a DTD.
    fn check(&mut self, id: &str, doc: &[u8], o: &ParseOptions) -> bool {
        let c = parse_trace(doc, o).expect("oracle");
        match port_trace(doc, o) {
            PortTrace::DtdNotPorted => {
                self.unported += 1;
                false
            }
            PortTrace::Trace(t) => {
                self.compared += 1;
                if let Some((line, l, r)) = first_difference(&c, &t) {
                    self.failures
                        .push(format!("{id} {o:?}: line {line}: C `{l}` port `{r}`"));
                }
                true
            }
        }
    }

    fn assert_clean(&self, what: &str) {
        eprintln!(
            "{what}: {} pairs equal to C, {} stopped at a DTD (module 11)",
            self.compared, self.unported
        );
        for f in self.failures.iter().take(30) {
            eprintln!("{f}");
        }
        assert!(
            self.failures.is_empty(),
            "{} mismatches",
            self.failures.len()
        );
    }
}

/// The option matrix plus the buffer API, reparse deferral and namespace
/// triplet variants.
fn extended_options() -> Vec<ParseOptions> {
    let mut v = option_matrix();
    v.push(ParseOptions {
        chunk_size: 3,
        buffer_api: true,
        ..ParseOptions::default()
    });
    v.push(ParseOptions {
        chunk_size: 1,
        no_reparse_deferral: true,
        ns_separator: Some(b':'),
        ..ParseOptions::default()
    });
    v.push(ParseOptions {
        chunk_size: 2,
        buffer_api: true,
        encoding: Some("UTF-16LE".into()),
        ..ParseOptions::default()
    });
    v.push(ParseOptions {
        chunk_size: 5,
        ns_separator: Some(b'|'),
        ns_triplets: true,
        ..ParseOptions::default()
    });
    v
}

#[test]
fn upstream_vectors_match_c() {
    let opts = extended_options();
    let mut tally = Tally::default();
    let mut fully = 0usize;
    for v in upstream() {
        let doc = v.doc();
        let mut all = true;
        for o in &opts {
            all &= tally.check(&v.source, &doc, o);
        }
        fully += usize::from(all);
    }
    eprintln!("upstream: {fully} of 540 vectors compared under every option set");
    tally.assert_clean("upstream");
    // Every vector without a document type declaration is compared under
    // every option set; the count changes only with the ported scope.
    assert_eq!(fully, 494);
}

/// Random option set for generated documents.
fn random_options(r: &mut Rng) -> ParseOptions {
    ParseOptions {
        encoding: r
            .pick(&[
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some("UTF-8"),
                Some("UTF-16"),
                Some("UTF-16LE"),
                Some("UTF-16BE"),
                Some("ISO-8859-1"),
                Some("US-ASCII"),
                Some("bogus"),
            ])
            .map(String::from),
        ns_separator: *r.pick(&[None, None, Some(b'|'), Some(b'\n'), Some(b':'), Some(b'#')]),
        chunk_size: *r.pick(&[0, 0, 1, 2, 3, 5, 7, 64, 1000, 1500]),
        buffer_api: r.one_in(3),
        no_reparse_deferral: r.one_in(4),
        ns_triplets: r.one_in(3),
        ..ParseOptions::default()
    }
}

#[test]
fn generated_documents_match_c() {
    let mut r = Rng::new(0x6578_7061_742d_7273);
    let mut tally = Tally::default();
    for i in 0..2500 {
        let doc = document(&mut r);
        for _ in 0..3 {
            let o = random_options(&mut r);
            tally.check(&format!("generated #{i}"), &doc, &o);
        }
    }
    tally.assert_clean("generated");
    assert!(tally.compared > 5000, "generator lost coverage");
}

/// Namespace triplets (`XML_SetReturnNSTriplet`): element and attribute
/// names with prefixes, the default namespace, and the duplicate-attribute
/// check for two prefixes bound to one URI, whose C comparison (the stored
/// name, which carries the triplet suffix, starts with the new one) only
/// differs from plain equality when triplets are on.
#[test]
fn namespace_triplets_match_c() {
    let docs: &[&[u8]] = &[
        b"<a:e xmlns:a='u' a:x='1' y='2'><a:f/></a:e>",
        b"<e xmlns='d' xmlns:p='u'><p:f p:x='1'/><g/></e>",
        b"<e xmlns:a='u' xmlns:b='u' a:x='1' b:x='2'/>",
        b"<e xmlns:a='u' xmlns:b='u' a:x='1' b:xy='2'/>",
        b"<e xmlns:a='u' xmlns:b='v' a:x='1' b:x='2'/>",
        b"<e xmlns:a='u' a:x='1' a:x='2'/>",
        b"<e xmlns:a='' a:x='1'/>",
    ];
    let mut tally = Tally::default();
    for (i, doc) in docs.iter().enumerate() {
        for sep in *b"|\n:" {
            for chunk in [0, 1, 4] {
                for triplets in [false, true] {
                    let o = ParseOptions {
                        ns_separator: Some(sep),
                        chunk_size: chunk,
                        ns_triplets: triplets,
                        ..ParseOptions::default()
                    };
                    assert!(tally.check(&format!("triplets #{i}"), doc, &o));
                }
            }
        }
    }
    tally.assert_clean("triplets");
}

/// Q-1 and Q-2 (docs/DIVERGENCES.md): the position after CR LF / CR at the
/// end of the document depends on chunking in C; the port replicates it.
#[test]
fn q1_q2_epilog_line_breaks_replicate_c() {
    let cases: &[(&[u8], usize, &str)] = &[
        (b"<a/>\r\n", 0, "result ok 0 2 0 6"),
        (b"<a/>\r\n", 1, "result ok 0 3 0 6"),
        (b"<a/>\r", 0, "result ok 0 1 4 4"),
        (b"<a/>\r", 1, "result ok 0 2 0 5"),
    ];
    for &(doc, chunk, want) in cases {
        let o = ParseOptions {
            chunk_size: chunk,
            ..ParseOptions::default()
        };
        let c = parse_trace(doc, &o).expect("oracle");
        let PortTrace::Trace(t) = port_trace(doc, &o) else {
            panic!("no DTD here");
        };
        assert_eq!(t, c, "{doc:?} chunk {chunk}");
        let text = String::from_utf8(t).expect("utf-8 trace");
        assert_eq!(
            text.trim_end().rsplit('\n').next(),
            Some(want),
            "{doc:?} chunk {chunk}"
        );
    }
}

/// The generated documents reach the paths they are meant for: every
/// outcome below occurs (so a generator change cannot silently stop
/// testing, say, namespace errors or long documents).
#[test]
fn generated_documents_cover_outcomes() {
    use std::collections::BTreeMap;
    let mut r = Rng::new(0x6578_7061_742d_7273);
    let mut codes: BTreeMap<u32, usize> = BTreeMap::new();
    let mut long_docs = 0;
    let mut events: BTreeMap<String, usize> = BTreeMap::new();
    for _ in 0..2500 {
        let doc = document(&mut r);
        long_docs += usize::from(doc.len() > 2048);
        let o = random_options(&mut r);
        let t = parse_trace(&doc, &o).expect("oracle");
        let text = String::from_utf8_lossy(&t).into_owned();
        for l in text.lines() {
            let tag = l.split(' ').next().unwrap_or("").to_owned();
            *events.entry(tag).or_default() += 1;
        }
        let code = expat_difftest::trace::result_error_code(&t).expect("result");
        *codes.entry(code).or_default() += 1;
    }
    eprintln!("codes {codes:?}\nevents {events:?}\nlong {long_docs}");
    for code in [
        0, 2, 3, 4, 5, 6, 7, 8, 9, 11, 14, 17, 18, 19, 20, 27, 28, 30, 38, 39, 40,
    ] {
        assert!(
            codes.contains_key(&code),
            "error code {code} never produced"
        );
    }
    for tag in [
        "xmldecl", "start", "attr", "end", "text", "pi", "comment", "cdata", "endcdata", "nsstart",
        "nsend",
    ] {
        assert!(events.contains_key(tag), "event {tag} never produced");
    }
    assert!(long_docs > 100);
}

/// Random option set for documents with a DTD: also varies parameter
/// entity parsing.
fn random_dtd_options(r: &mut Rng) -> ParseOptions {
    ParseOptions {
        param_entity_parsing: *r.pick(&[
            ParamEntityParsing::Never,
            ParamEntityParsing::UnlessStandalone,
            ParamEntityParsing::Always,
        ]),
        ..random_options(r)
    }
}

/// Module 11: generated documents with a document type declaration
/// (element, attribute-list, entity and notation declarations, parameter
/// entity references, malformed declarations, entity references in
/// content and attribute values), whole and damaged.
#[test]
fn generated_dtd_documents_match_c() {
    let mut r = Rng::new(0x6474_642d_6765_6e31);
    let mut tally = Tally::default();
    for i in 0..3000 {
        let doc = dtd_document(&mut r);
        for _ in 0..3 {
            let o = random_dtd_options(&mut r);
            tally.check(&format!("dtd #{i}"), &doc, &o);
        }
    }
    tally.assert_clean("generated dtd");
    assert!(tally.compared > 4000, "generator lost coverage");
}

/// The DTD generator reaches the declaration events and the DTD error
/// codes (so a generator change cannot silently stop testing them).
#[test]
fn generated_dtd_documents_cover_outcomes() {
    use std::collections::BTreeMap;
    let mut r = Rng::new(0x6474_642d_6765_6e31);
    let mut codes: BTreeMap<u32, usize> = BTreeMap::new();
    let mut events: BTreeMap<String, usize> = BTreeMap::new();
    for _ in 0..3000 {
        let doc = dtd_document(&mut r);
        let o = random_dtd_options(&mut r);
        let t = parse_trace(&doc, &o).expect("oracle");
        let text = String::from_utf8_lossy(&t).into_owned();
        for l in text.lines() {
            let tag = l.split(' ').next().unwrap_or("").to_owned();
            *events.entry(tag).or_default() += 1;
        }
        let code = expat_difftest::trace::result_error_code(&t).expect("result");
        *codes.entry(code).or_default() += 1;
    }
    eprintln!("codes {codes:?}\nevents {events:?}");
    for code in [0, 2, 4, 10, 11, 14, 15, 16, 32] {
        assert!(
            codes.contains_key(&code),
            "error code {code} never produced"
        );
    }
    for tag in [
        "doctype",
        "enddoctype",
        "entity",
        "notation",
        "element",
        "attlist",
        "skipped",
        "start",
        "attr",
    ] {
        assert!(events.contains_key(tag), "event {tag} never produced");
    }
}

/// Attribute-list declarations whose effect depends on C's rules for
/// repeated definitions: the first definition of an attribute decides its
/// type (CDATA or tokenized normalisation) and default; an ID attribute
/// never gets a default; the first ID attribute is the element's ID.
#[test]
fn repeated_attribute_definitions_match_c() {
    let docs: &[&[u8]] = &[
        b"<!DOCTYPE a [<!ATTLIST a x CDATA #IMPLIED x NMTOKEN #IMPLIED>]><a x='  p  q  '/>",
        b"<!DOCTYPE a [<!ATTLIST a x NMTOKENS #IMPLIED x CDATA #IMPLIED>]><a x='  p  q  '/>",
        b"<!DOCTYPE a [<!ATTLIST a x CDATA 'd1' x CDATA 'd2'>]><a/>",
        b"<!DOCTYPE a [<!ATTLIST a x CDATA #IMPLIED><!ATTLIST a x CDATA 'd'>]><a/>",
        b"<!DOCTYPE a [<!ATTLIST a x ID 'd' y ID #IMPLIED z CDATA 'z'>]><a y='1'/>",
        b"<!DOCTYPE a [<!ATTLIST a x ID #IMPLIED x NMTOKEN ' n '>]><a x=' v  w '/>",
        b"<!DOCTYPE a [<!ATTLIST a xmlns:p CDATA 'urn:p' p:x CDATA 'v'>]><a/>",
        b"<!DOCTYPE a [<!ATTLIST a x (u|v) 'u' y NOTATION (n) #REQUIRED>]><a/>",
    ];
    let mut tally = Tally::default();
    for (i, doc) in docs.iter().enumerate() {
        for o in extended_options() {
            assert!(tally.check(&format!("attdef #{i}"), doc, &o));
        }
    }
    tally.assert_clean("attribute definitions");
}

/// Deeply nested content models: C builds them without recursion
/// (`build_model`); so do the port and its trace writer. Compared with C
/// whole and in chunks.
#[test]
fn deep_content_models_match_c() {
    for depth in [1usize, 31, 32, 33, 1000, 20_000] {
        let mut doc = b"<!DOCTYPE r [<!ELEMENT r ".to_vec();
        doc.extend(std::iter::repeat_n(b'(', depth));
        doc.extend_from_slice(b"a");
        for i in 0..depth {
            doc.extend_from_slice(if i % 2 == 0 { b",b)*" } else { b"|c)" });
        }
        doc.extend_from_slice(b">]><r/>");
        let mut tally = Tally::default();
        for chunk in [0, 1000] {
            let o = ParseOptions {
                chunk_size: chunk,
                ..ParseOptions::default()
            };
            assert!(tally.check(&format!("depth {depth}"), &doc, &o));
        }
        tally.assert_clean("deep models");
    }
}
