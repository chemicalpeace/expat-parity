//! Module 10: the port's parser against the C oracle, compared by
//! canonical trace (docs/ARCHITECTURE.md) on the upstream vectors and on
//! generated documents, under many option sets (encodings, namespace
//! separators, chunkings, `XML_Parse` versus the buffer API, reparse
//! deferral on and off).
//!
//! Documents with a document type declaration stop the port with
//! `Stop::DtdNotPorted` until module 11; they are counted, never compared.

use expat_difftest::diff::option_matrix;
use expat_difftest::docgen::{Rng, document};
use expat_difftest::port::{PortTrace, port_trace};
use expat_difftest::trace::first_difference;
use expat_difftest::vectors::upstream;
use expat_oracle::{ParseOptions, parse_trace};

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
    assert_eq!(fully, 398);
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
