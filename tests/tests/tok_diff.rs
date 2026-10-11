//! Differential tests of the tokenizer (module 5: `xmltok_impl.c` for the
//! single-byte encodings) against the C tokenizer. See
//! `expat_difftest::tok` for what each check compares.

use expat::internal::tok::Normal;
use expat_difftest::tok::{check_buffer, check_token_walk};
use expat_difftest::vectors;
use proptest::prelude::*;

/// One representative byte per byte type of the single-byte tables, plus
/// the letters and lead/trail bytes the scanners treat specially.
const ALPHABET: &[u8] = b"\x00\x09\x0a\x0d !\"#%&'()*+,-./09:;<=>?@AFXZ[]_amltx|\x7f\x80\xa9\xb7\xbf\xc0\xc3\xe0\xe2\xed\xef\xf0\xf4\xf8\xff";

#[test]
fn all_strings_up_to_three_bytes() {
    let mut buf = Vec::with_capacity(3);
    for len in 0..=3u32 {
        let total = ALPHABET.len().pow(len);
        for mut i in 0..total {
            buf.clear();
            for _ in 0..len {
                buf.push(ALPHABET[i % ALPHABET.len()]);
                i /= ALPHABET.len();
            }
            for enc in Normal::ALL {
                check_buffer(enc, &buf, [0]);
            }
        }
    }
}

#[test]
fn upstream_vectors_token_walk() {
    for v in vectors::upstream() {
        let doc = v.doc();
        for enc in Normal::ALL {
            check_token_walk(enc, &doc);
        }
    }
}

#[test]
fn upstream_vectors_every_offset() {
    for v in vectors::upstream() {
        let doc = v.doc();
        if doc.len() > 2048 {
            continue;
        }
        for enc in [
            Normal::Utf8,
            Normal::Utf8Ns,
            Normal::Latin1Ns,
            Normal::AsciiNs,
        ] {
            check_buffer(enc, &doc, 0..=doc.len());
        }
    }
}

/// XML-ish fragments that the scanners dispatch on.
fn fragment() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        proptest::sample::select(ALPHABET).prop_map(|b| vec![b]),
        proptest::sample::select(vec![
            &b"<!--"[..],
            b"-->",
            b"<![CDATA[",
            b"]]>",
            b"<?",
            b"?>",
            b"<?xml ",
            b"</",
            b"/>",
            b"&#x",
            b"&#",
            b"&amp;",
            b"&lt;",
            b"%pe;",
            b"<!DOCTYPE ",
            b"<!ELEMENT ",
            b"<!ENTITY % ",
            b"(#PCDATA|a)*",
            b"a:b",
            b"='v'",
            b"=\"v\"",
            b"\r\n",
            b"\xc3\xa9",
            b"\xe2\x82\xac",
            b"\xf0\x9f\x98\x80",
            b"\xed\xa0\x80",
            b"\xef\xbf\xbe",
            b"<![",
            b"<![IGNORE[",
        ])
        .prop_map(<[u8]>::to_vec),
    ]
}

fn doc() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(fragment(), 0..24).prop_map(|v| v.concat())
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 3_000,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn random_fragments_every_offset(d in doc(), e in 0usize..8) {
        let enc = Normal::ALL[e];
        check_buffer(enc, &d, 0..=d.len());
        check_token_walk(enc, &d);
    }
}
