//! Differential tests of the tokenizer (modules 5 and 6: `xmltok_impl.c` and
//! the encodings and converters of `xmltok.c`) against the C tokenizer. See
//! `expat_difftest::tok` for what each check compares.

use expat::internal::tok::{Normal, Utf16};
use expat_difftest::tok::{
    Subject, UnknownPair, check_buffer, check_convert, check_token_walk, check_trim,
    unknown_from_seed,
};
use expat_difftest::vectors;
use proptest::prelude::*;

/// One representative byte per byte type of the single-byte tables, plus
/// the letters and lead/trail bytes the scanners treat specially.
const ALPHABET: &[u8] = b"\x00\x09\x0a\x0d !\"#%&'()*+,-./09:;<=>?@AFXZ[]_amltx|\x7f\x80\xa9\xb7\xbf\xc0\xc3\xe0\xe2\xed\xef\xf0\xf4\xf8\xff";

/// One representative UTF-16 code unit per byte type and naming class:
/// the ASCII syntax characters, Latin-1 name and non-name characters, a
/// combining mark, CJK, fullwidth letters, both surrogate halves and the
/// noncharacters.
const UNITS: &[u16] = &[
    0x00, 0x09, 0x0A, 0x0D, 0x20, 0x21, 0x22, 0x23, 0x25, 0x26, 0x27, 0x28, 0x2D, 0x2F, 0x30, 0x3A,
    0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x41, 0x5B, 0x5D, 0x5F, 0x61, 0x78, 0x7C, 0xB7, 0xD7, 0xE9,
    0x0300, 0x0E00, 0x4E00, 0xD800, 0xDBFF, 0xDC00, 0xDFFF, 0xFF21, 0xFFFE, 0xFFFF,
];

/// Encodes UTF-16 code units in the byte order of `enc`.
fn units_to_bytes(enc: Utf16, units: impl IntoIterator<Item = u16>) -> Vec<u8> {
    units
        .into_iter()
        .flat_map(|u| match enc {
            Utf16::Little | Utf16::LittleNs => u.to_le_bytes(),
            Utf16::Big | Utf16::BigNs => u.to_be_bytes(),
        })
        .collect()
}

/// Transcodes a byte document to UTF-16 (invalid UTF-8 becomes U+FFFD).
fn to_utf16(enc: Utf16, doc: &[u8]) -> Vec<u8> {
    units_to_bytes(enc, String::from_utf8_lossy(doc).encode_utf16())
}

/// Every string over `alphabet` of up to `max_len` symbols, as `f` encodes it.
fn for_all_strings<T: Copy>(alphabet: &[T], max_len: u32, mut f: impl FnMut(&[T])) {
    let mut s = Vec::with_capacity(max_len as usize);
    for len in 0..=max_len {
        for mut i in 0..alphabet.len().pow(len) {
            s.clear();
            for _ in 0..len {
                s.push(alphabet[i % alphabet.len()]);
                i /= alphabet.len();
            }
            f(&s);
        }
    }
}

#[test]
fn all_strings_up_to_three_bytes() {
    for_all_strings(ALPHABET, 3, |buf| {
        for enc in Normal::ALL {
            check_buffer(&enc, buf, [0]);
        }
    });
}

#[test]
fn utf16_all_strings_up_to_three_units() {
    for_all_strings(UNITS, 3, |units| {
        for enc in Utf16::ALL {
            let buf = units_to_bytes(enc, units.iter().copied());
            check_buffer(&enc, &buf, [0]);
            // A dangling final byte, and a start inside a unit.
            let mut odd = buf.clone();
            odd.push(0x3C);
            check_buffer(&enc, &odd, [0, 1]);
        }
    });
}

#[test]
fn utf8_converters_all_strings_up_to_three_bytes() {
    for_all_strings(ALPHABET, 3, |buf| {
        check_trim(buf);
        // One encoding per converter pair and byte-type table (the NS
        // variants share converters and lead-byte types).
        for enc in [
            Normal::Utf8,
            Normal::InternalUtf8Ns,
            Normal::Latin1,
            Normal::Ascii,
        ] {
            check_convert(&enc, buf, true);
        }
    });
}

#[test]
fn utf16_converters_all_strings_up_to_three_units() {
    for_all_strings(UNITS, 3, |units| {
        for enc in [Utf16::Little, Utf16::Big] {
            check_convert(&enc, &units_to_bytes(enc, units.iter().copied()), true);
        }
    });
}

#[test]
fn trim_all_byte_pairs_and_lead_runs() {
    for a in 0..=255u8 {
        for b in 0..=255u8 {
            check_trim(&[a, b]);
            check_trim(&[b'x', a, 0x80, b]);
        }
    }
}

#[test]
fn upstream_vectors_token_walk() {
    for v in vectors::upstream() {
        let doc = v.doc();
        for enc in Normal::ALL {
            check_token_walk(&enc, &doc);
        }
        for enc in Utf16::ALL {
            check_token_walk(&enc, &doc);
            check_token_walk(&enc, &to_utf16(enc, &doc));
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
            check_buffer(&enc, &doc, 0..=doc.len());
        }
        for enc in [Utf16::LittleNs, Utf16::Big] {
            let d = to_utf16(enc, &doc);
            check_buffer(&enc, &d, 0..=d.len());
            check_convert(&enc, &d, true);
        }
        check_convert(&Normal::Utf8, &doc, true);
    }
}

/// 300 seeded unknown encodings (maps, converter tables, namespace flag),
/// each on the upstream vectors and on every string of up to 2 bytes.
#[test]
fn unknown_encodings_seeded() {
    let docs: Vec<Vec<u8>> = vectors::upstream()
        .iter()
        .map(|v| v.doc())
        .filter(|d| d.len() <= 512)
        .collect();
    let mut accepted = 0;
    let all_bytes: Vec<u8> = (0..=255).collect();
    for seed in 0..300u64 {
        let Some(pair) = unknown_from_seed(seed) else {
            continue;
        };
        accepted += 1;
        for_all_strings(&all_bytes, 1, |buf| check_buffer(&pair, buf, [0]));
        check_convert(&pair, &all_bytes, true);
        for doc in docs
            .iter()
            .skip(usize::try_from(seed % 7).expect("small"))
            .step_by(7)
        {
            check_token_walk(&pair, doc);
            check_buffer(&pair, doc, 0..=doc.len().min(48));
        }
    }
    // The generator must mostly produce maps that C accepts.
    assert!(
        accepted >= 200,
        "only {accepted} of 300 seeded maps accepted"
    );
}

#[test]
fn unknown_encoding_rejections_match() {
    let identity: [i32; 256] = std::array::from_fn(|i| i32::try_from(i).expect("byte"));
    let tab = vec![0x4E00; 0x1_0000];
    // Accepted: identity, with and without converter; NS.
    for ns in [false, true] {
        assert!(UnknownPair::new("identity", &identity, None, ns).is_some());
        assert!(UnknownPair::new("identity", &identity, Some(tab.clone()), ns).is_some());
    }
    let cases: [(usize, i32, bool); 9] = [
        (usize::from(b'<'), 0x3C + 1, false), // syntax char not itself
        (usize::from(b'a'), 0x62, false),     // letter not itself
        (0x24, 0x24, true),                   // `$` (BT_OTHER) may map freely: same value
        (0x24, 0x20AC, true),                 // ... or another character
        (0x01, 0x41, false),                  // a byte mapped onto a syntax character
        (0x80, -5, false),                    // below -4
        (0x80, 0x1_0000, false),              // outside the BMP
        (0x80, -2, false),                    // lead byte without converter
        (0x80, 0xD800, true),                 // surrogate: becomes BT_NONXML
    ];
    for (i, v, ok) in cases {
        let mut map = identity;
        map[i] = v;
        assert_eq!(
            UnknownPair::new("case", &map, None, false).is_some(),
            ok,
            "map[{i:#x}] = {v:#x}"
        );
        let mut map = identity;
        map[i] = v;
        // With a converter, the lead byte is accepted.
        let with = UnknownPair::new("case+conv", &map, Some(tab.clone()), true).is_some();
        assert_eq!(with, ok || v == -2, "map[{i:#x}] = {v:#x} with converter");
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

fn check_all<S: Subject>(s: &S, d: &[u8]) {
    check_buffer(s, d, 0..=d.len());
    check_token_walk(s, d);
    check_convert(s, d, true);
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 3_000,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn random_fragments_every_offset(d in doc(), e in 0usize..8) {
        check_all(&Normal::ALL[e], &d);
    }

    #[test]
    fn utf16_random_fragments_every_offset(d in doc(), e in 0usize..4, raw in any::<bool>()) {
        let enc = Utf16::ALL[e];
        // Either the fragments transcoded (realistic UTF-16) or their raw
        // bytes read as UTF-16 (arbitrary code units, odd lengths).
        let d = if raw { d } else { to_utf16(enc, &d) };
        check_all(&enc, &d);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 300,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn unknown_random_fragments_every_offset(d in doc(), seed in any::<u64>()) {
        if let Some(pair) = unknown_from_seed(seed) {
            check_all(&pair, &d);
        }
    }
}
