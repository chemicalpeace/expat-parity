//! Differential tests of module 7 against C: encoding detection
//! (`XmlInitEncoding(NS)` + `initScan`) and the XML/text declaration
//! (`XmlParseXmlDecl(NS)`). The declaration is compared through
//! `expat_difftest::tok::check_scan`, which runs it on every
//! `XML_TOK_XML_DECL` token a scanner accepts.

use expat::internal::tok::{Normal, Utf16};
use expat_difftest::tok::{Subject, check_init, check_scan, check_token_walk, unknown_from_seed};
use expat_difftest::vectors;
use expat_oracle::tok::Scanner;
use proptest::prelude::*;

/// The bytes `initScan` dispatches on, plus ordinary ones.
const INIT_ALPHABET: &[u8] = b"\x00\x3c\xfe\xff\xef\xbb\xbf ?x\n";

#[test]
fn init_scan_all_strings_up_to_four_bytes() {
    let mut buf = Vec::with_capacity(4);
    for len in 0..=4u32 {
        for mut i in 0..INIT_ALPHABET.len().pow(len) {
            buf.clear();
            for _ in 0..len {
                buf.push(INIT_ALPHABET[i % INIT_ALPHABET.len()]);
                i /= INIT_ALPHABET.len();
            }
            check_init(&buf, 0);
            if !buf.is_empty() {
                check_init(&buf, 1);
            }
        }
    }
}

#[test]
fn init_scan_upstream_vectors() {
    for v in vectors::upstream() {
        let doc = v.doc();
        check_init(&doc, 0);
    }
}

/// Q-3: a one-byte first chunk of BOM-less UTF-16LE is scanned as the
/// default (UTF-8) encoding, two bytes select UTF-16LE. The port replicates
/// C (docs/DIVERGENCES.md, Q-3).
#[test]
fn q3_single_byte_detection_matches_c() {
    let doc = b" \0<\0e\0/\0>\0";
    check_init(&doc[..1], 0);
    check_init(&doc[..2], 0);
    check_init(doc, 0);
}

/// Pieces of an XML declaration, valid and invalid.
fn decl_piece() -> impl Strategy<Value = &'static str> {
    proptest::sample::select(vec![
        " ",
        "  ",
        "\t",
        "\n",
        "\r",
        "version",
        "encoding",
        "standalone",
        "Version",
        "=",
        "'",
        "\"",
        "1.0",
        "1.",
        "1.10",
        "2.0",
        "1.x",
        "UTF-8",
        "utf-16",
        "UTF-16LE",
        "ISO-8859-1",
        "us-ascii",
        "x-custom",
        "9bad",
        "yes",
        "no",
        "Yes",
        "\u{e9}",
        "\u{4e00}",
        "a",
        "_.-",
    ])
}

/// A declaration from pieces, biased towards the well-formed shape.
fn decl() -> impl Strategy<Value = String> {
    let attr = |name: &'static str, values: Vec<&'static str>| {
        (
            proptest::sample::select(vec![" ", "  ", "\n"]),
            proptest::sample::select(vec!["=", " = ", "\t="]),
            proptest::sample::select(vec!["'", "\""]),
            proptest::sample::select(values),
        )
            .prop_map(move |(ws, eq, q, v)| format!("{ws}{name}{eq}{q}{v}{q}"))
    };
    let shaped = (
        proptest::option::of(attr("version", vec!["1.0", "1.1", "1.", "", "2.0", "1.0a"])),
        proptest::option::of(attr(
            "encoding",
            vec![
                "UTF-8",
                "utf-16",
                "UTF-16BE",
                "ISO-8859-1",
                "x-custom",
                "1x",
                "",
            ],
        )),
        proptest::option::of(attr("standalone", vec!["yes", "no", "maybe"])),
        proptest::sample::select(vec!["", " ", "\n "]),
    )
        .prop_map(|(v, e, s, tail)| {
            format!(
                "<?xml{}{}{}{tail}?>",
                v.unwrap_or_default(),
                e.unwrap_or_default(),
                s.unwrap_or_default()
            )
        });
    let random = proptest::collection::vec(decl_piece(), 0..12)
        .prop_map(|v| format!("<?xml {}?>", v.concat()));
    prop_oneof![3 => shaped, 1 => random]
}

/// Scans `doc` from the start in the prolog and content states (which
/// compares the declaration whenever one is accepted), and walks it.
fn check_doc<S: Subject>(s: &S, doc: &[u8]) {
    check_scan(s, Scanner::Prolog, doc, 0);
    check_scan(s, Scanner::Content, doc, 0);
    check_token_walk(s, doc);
}

fn utf16(enc: Utf16, s: &str) -> Vec<u8> {
    s.encode_utf16()
        .flat_map(|u| match enc {
            Utf16::Little | Utf16::LittleNs => u.to_le_bytes(),
            Utf16::Big | Utf16::BigNs => u.to_be_bytes(),
        })
        .collect()
}

proptest! {
    // 4,000 cases by default; long local runs set PROPTEST_CASES (and
    // PROPTEST_RNG_SEED for a reproducible stream), e.g. the 100,000-case
    // runs of docs/REPORT.md (Q-5).
    #![proptest_config(ProptestConfig {
        cases: std::env::var("PROPTEST_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(4_000),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn xml_decl_in_every_builtin_encoding(d in decl(), seed in any::<u64>()) {
        let doc = format!("{d}<a/>");
        for enc in Normal::ALL {
            check_doc(&enc, doc.as_bytes());
        }
        // Latin-1 bytes for the Latin-1 characters.
        let latin1: Vec<u8> = doc.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')).collect();
        check_doc(&Normal::Latin1Ns, &latin1);
        for enc in Utf16::ALL {
            check_doc(&enc, &utf16(enc, &doc));
        }
        if let Some(pair) = unknown_from_seed(seed) {
            check_doc(&pair, doc.as_bytes());
            check_doc(&pair, &latin1);
        }
    }
}
