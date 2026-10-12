//! `XmlParseXmlDecl(NS)` with unknown encodings against C: the error
//! position when `toAscii` starts inside a multi-byte sequence (Q-5,
//! docs/DIVERGENCES.md, D-035).
//!
//! `parsePseudoAttribute` and `checkXmlDeclVersionNum` (`xmltok.c`) step by
//! `minBytesPerChar`, i.e. by one byte with an unknown encoding, and call
//! `toAscii(enc, ptr, end)` at every byte, including trailing bytes of a
//! multi-byte character. When the map makes such a byte a lead byte,
//! `unknown_toUtf8` passes the converter the whole sequence even if it
//! extends past `end` (the declaration end before `?>`), and C goes on
//! converting until the one-byte output is full. The random test
//! `init_diff::xml_decl_in_every_builtin_encoding` found the port stopping
//! at `end` instead (fixed by `Encoding::to_utf8_until`):
//!
//! - seed `0x52f43d23123766f6`, document `<?xml \xC3\xA9?><a/>`
//!   (`\xC3` lead of 2, `\xA9` lead of 4): `bad` was 7 in the port, 8 in C;
//! - seed `0xfe8d9b29cbd0ff22`, document `<?xml no\xC3\xA9 ?><a/>`
//!   (`\xC3` lead of 3, `\xA9` lead of 4): port 9, C 11.
//!
//! `bad` is `processXmlDecl`'s `&m_eventPtr`, so it is the error's byte
//! index (`XML_GetCurrentByteIndex` for a document parsed from offset 0);
//! `check_xml_decl` also compares the line and column `XmlUpdatePosition`
//! gives for it, and `XML_ERROR_XML_DECL` / `XML_ERROR_TEXT_DECL` follows
//! from `ok` and the text-declaration flag. Whole parses with an unknown
//! encoding need the unknown-encoding handler (module 12); whole parses of
//! the same declarations in the built-in encodings are compared here
//! through the trace (error code, line, column, byte index).

use expat::internal::tok::{self as rs, Normal, Utf16};
use expat_difftest::diff::{Oracle, compare_all, option_matrix};
use expat_difftest::port::Port;
use expat_difftest::tok::{
    Subject, UnknownPair, check_scan, check_token_walk, check_xml_decl, unknown_from_seed,
};
use expat_oracle::tok::{self as c, CONVERTER_TABLE_LEN, Scanner};

/// The two inputs of the random test, verbatim, with the end of the
/// declaration token and C's `bad` for them.
const SEEDS: [(u64, &[u8], usize, usize); 2] = [
    (0x52f4_3d23_1237_66f6, b"<?xml \xC3\xA9?><a/>", 10, 8),
    (0xfe8d_9b29_cbd0_ff22, b"<?xml no\xC3\xA9 ?><a/>", 13, 11),
];

#[test]
fn random_test_seeds_match_c() {
    for (seed, doc, decl_end, c_bad) in SEEDS {
        let p = unknown_from_seed(seed).expect("both seeds give a valid map");
        for ns in [false, true] {
            for gte in [false, true] {
                let cd = c::parse_xml_decl(ns, gte, p.c(), doc, 0, decl_end);
                assert_eq!((cd.ok, cd.bad), (false, Some(c_bad)), "C, seed {seed:#x}");
                let r = rs::parse_xml_decl(gte, p.rs(), ns, doc, 0, decl_end);
                assert_eq!((r.ok, r.bad), (false, Some(c_bad)), "port, seed {seed:#x}");
                check_xml_decl(&p, ns, gte, doc, 0, decl_end);
            }
        }
        // The path the random test takes: the scanners accept the token
        // and compare the declaration.
        check_scan(&p, Scanner::Prolog, doc, 0);
        check_scan(&p, Scanner::Content, doc, 0);
        check_token_walk(&p, doc);
    }
}

/// What the map makes of a byte.
#[derive(Debug, Clone, Copy)]
enum Kind {
    Lead(i32),
    Malformed,
    NonXml,
    Bmp(i32),
    Ascii(u8),
    /// A map value C rejects (below `-4`).
    Invalid,
}

const KINDS: [Kind; 10] = [
    Kind::Lead(2),
    Kind::Lead(3),
    Kind::Lead(4),
    Kind::Malformed,
    Kind::NonXml,
    Kind::Bmp(0xE9),
    Kind::Bmp(0x4E00),
    Kind::Ascii(b'^'),
    // C rejects a non-ASCII byte mapped to an ASCII character XML syntax uses.
    Kind::Ascii(b'Z'),
    Kind::Invalid,
];

/// What the converter returns for every sequence: ASCII (letter, digit,
/// white space, `=`, both quotes, NUL), 2-, 3- and 4-byte UTF-8, a
/// surrogate, past Unicode, and negative (no output, so C converts the
/// next character).
const CONVERTED: [i32; 13] = [
    b'a' as i32,
    b'1' as i32,
    b' ' as i32,
    b'=' as i32,
    b'"' as i32,
    b'\'' as i32,
    0xE9,
    0x4E00,
    0x1_F600,
    0xD800,
    0x11_0000,
    -1,
    0,
];

/// Declarations with the two bytes `X` and `Y` (in that order) at
/// different places: in a pseudo-attribute name, in a value, after `=`,
/// before `?>`, between attributes, in the encoding name and the
/// standalone value, and alone.
const TEMPLATES: [&str; 14] = [
    "<?xml XY?>",
    "<?xml noXY ?>",
    "<?xml vXYersion='1.0'?>",
    "<?xml version='1.XY'?>",
    "<?xml version=XY'1.0'?>",
    "<?xml version='1.0'XY?>",
    "<?xml version='1.0' XY?>",
    "<?xml version='1.0'XY encoding='UTF-8'?>",
    "<?xml version='1.0' encoding='aXY'?>",
    "<?xml version='1.0' encoding='XY'?>",
    "<?xml version='1.0' standalone='XY'?>",
    "<?xml version=\"1.0\" encoding=\"x\" standalone=\"yesXY\"?>",
    "<?xml version='1.0' XY='1.0'?>",
    "<?xml X version='1.0'Y?>",
];

fn map_with(x: Kind, y: Kind) -> [i32; 256] {
    let mut map = [0i32; 256];
    for (i, m) in map.iter_mut().enumerate() {
        *m = if i < 0x80 { i as i32 } else { 0xC0 };
    }
    for (b, k) in [(0xC3usize, x), (0xA9usize, y)] {
        map[b] = match k {
            Kind::Lead(n) => -n,
            Kind::Malformed => -1,
            Kind::NonXml => 0xFFFE,
            Kind::Bmp(c) => c,
            Kind::Ascii(c) => i32::from(c),
            Kind::Invalid => -5,
        };
    }
    map
}

fn instantiate(template: &str) -> Vec<u8> {
    template
        .bytes()
        .map(|b| match b {
            b'X' => 0xC3,
            b'Y' => 0xA9,
            b => b,
        })
        .collect()
}

/// Compares the declaration `decl` followed by `tail` for both namespace
/// modes and both declaration kinds, and the scanners on it.
fn check_decl<S: Subject>(s: &S, decl: &[u8], tail: &[u8]) {
    let doc = [decl, tail].concat();
    for ns in [false, true] {
        for gte in [false, true] {
            check_xml_decl(s, ns, gte, &doc, 0, decl.len());
        }
    }
    check_scan(s, Scanner::Prolog, &doc, 0);
    check_scan(s, Scanner::Content, &doc, 0);
}

#[test]
fn multibyte_sequences_at_every_pseudo_attribute_position() {
    let mut compared = 0usize;
    let mut rejected = 0usize;
    for x in KINDS {
        for y in KINDS {
            for (i, &cv) in CONVERTED.iter().enumerate() {
                let tab = vec![cv; CONVERTER_TABLE_LEN];
                let map = map_with(x, y);
                let label = format!("{x:?} {y:?} -> {cv:#x}");
                let Some(p) = UnknownPair::new(&label, &map, Some(tab), i % 2 == 0) else {
                    rejected += 1;
                    continue;
                };
                for t in TEMPLATES {
                    let decl = instantiate(t);
                    // Followed by a document, by nothing but the `>` padding
                    // the C wrapper adds, and by more lead bytes.
                    for tail in [&b"<a/>"[..], b"", b"\xC3\xA9\xC3\xA9"] {
                        check_decl(&p, &decl, tail);
                        compared += 1;
                    }
                }
            }
        }
    }
    // Pairs with `Ascii('Z')` or `Invalid` are rejected (by both sides,
    // which `UnknownPair::new` asserts): 100 - 8 * 8 = 36 pairs.
    assert_eq!(rejected, 36 * CONVERTED.len());
    // 64 accepted pairs x 13 converters x 14 templates x 3 tails.
    assert_eq!(compared, 64 * CONVERTED.len() * TEMPLATES.len() * 3);
    // Printed so that CI logs (`--nocapture`) show the count on every
    // platform.
    eprintln!(
        "xmldecl_unknown_diff: {compared} declarations compared, {rejected} rejected pairs checked"
    );
}

/// The same declarations in every built-in encoding (Latin-1 reads the
/// bytes as `Ã` and `©`, UTF-8 as `é`).
#[test]
fn same_declarations_in_every_builtin_encoding() {
    for t in TEMPLATES {
        let decl = instantiate(t);
        for enc in Normal::ALL {
            check_decl(&enc, &decl, b"<a/>");
            check_token_walk(&enc, &[&decl[..], b"<a/>"].concat());
        }
        let text: String = decl.iter().map(|&b| char::from(b)).collect();
        for enc in Utf16::ALL {
            let mut d: Vec<u8> = Vec::new();
            for u in text.encode_utf16() {
                d.extend(match enc {
                    Utf16::Little | Utf16::LittleNs => u.to_le_bytes(),
                    Utf16::Big | Utf16::BigNs => u.to_be_bytes(),
                });
            }
            let tail: &[u8] = match enc {
                Utf16::Little | Utf16::LittleNs => b"<\0a\0/\0>\0",
                Utf16::Big | Utf16::BigNs => b"\0<\0a\0/\0>",
            };
            check_decl(&enc, &d, tail);
        }
    }
}

/// Whole parses of the declarations (built-in encodings, every option set
/// of the differential suite, including chunked input and the protocol
/// encodings ISO-8859-1 and UTF-16): error code, line, column, byte index.
#[test]
fn same_declarations_whole_parse_match_c() {
    let mut docs: Vec<(String, Vec<u8>)> = Vec::new();
    for t in TEMPLATES {
        let d = instantiate(t);
        docs.push((format!("{t} + <a/>"), [&d[..], b"<a/>"].concat()));
        docs.push((
            format!("UTF-8 BOM + {t} + <a/>"),
            [&b"\xEF\xBB\xBF"[..], &d, b"<a/>"].concat(),
        ));
        docs.push((format!("{t} alone"), d));
    }
    for (seed, doc, _, _) in SEEDS {
        docs.push((format!("seed {seed:#x}"), doc.to_vec()));
    }
    let mismatches = compare_all(
        &Oracle,
        &Port,
        docs.iter().map(|(id, d)| (id.clone(), d.as_slice())),
        &option_matrix(),
    );
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}

/// Q-5: where C would read past the end of the buffer the port does not
/// read and `toAscii` gives `-1`. Not comparable with C (it reads outside
/// the buffer; the C wrapper pads with `>`), so the port is pinned. In the
/// map of the first seed `\xA9` is a lead byte of 4 and the converter
/// gives a negative value for `\xA9 ? > <`: no output, so C goes on with
/// the byte after that sequence, two bytes past the declaration token.
#[test]
fn q5_sequence_past_the_buffer_is_not_read() {
    let p = unknown_from_seed(SEEDS[0].0).expect("valid map");
    let decl = b"<?xml \xC3\xA9?>";
    for (tail, ascii, bad) in [
        // The sequence itself is not complete in the buffer.
        (&b""[..], -1, 7),
        // Complete, but its conversion is empty and the next byte is past
        // the buffer.
        (b"<", -1, 7),
        // Like C: the next character is converted (`a`).
        (b"<a", i32::from(b'a'), 8),
        (b"<a/>", i32::from(b'a'), 8),
    ] {
        let buf = [&decl[..], tail].concat();
        assert_eq!(rs::to_ascii(p.rs(), &buf, 7, 8), ascii, "tail {tail:?}");
        let r = rs::parse_xml_decl(false, p.rs(), false, &buf, 0, decl.len());
        assert_eq!((r.ok, r.bad), (false, Some(bad)), "tail {tail:?}");
    }
}
