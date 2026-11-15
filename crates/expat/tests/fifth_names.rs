//! Names by XML 1.0 Fifth Edition, an option outside libexpat parity
//! (D-064). With the option off, the parser must behave as libexpat; with
//! it on, names may use the Fifth Edition classes, and nothing else
//! changes.

use expat::{ErrorCode, Parser, ParsingStatus};

fn parse(doc: &[u8], fifth: bool) -> Result<(), ErrorCode> {
    let mut p = Parser::new([7; 16]);
    assert!(p.set_fifth_edition_names(fifth));
    p.parse(doc, true).map_err(|e| e.code())
}

fn utf16(doc: &str, little: bool) -> Vec<u8> {
    let mut out = if little {
        vec![0xFF, 0xFE]
    } else {
        vec![0xFE, 0xFF]
    };
    for u in doc.encode_utf16() {
        out.extend(if little {
            u.to_le_bytes()
        } else {
            u.to_be_bytes()
        });
    }
    out
}

/// Documents that use a character only the Fifth Edition allows, in each
/// kind of name. U+0221 is a letter added in Unicode 4.0; U+10000 lies
/// outside the BMP.
const NEW_NAMES: &[&str] = &[
    "<\u{221}/>",
    "<\u{10000}/>",
    "<a\u{221}b/>",
    "<a \u{221}='1'/>",
    "<a \u{10000}x='1'/>",
    "<?\u{221} data?><a/>",
    "<!DOCTYPE \u{221}><\u{221}/>",
    "<!DOCTYPE a [<!ENTITY \u{221} 'x'>]><a>&\u{221};</a>",
    "<!DOCTYPE a [<!ENTITY \u{221} 'x'>]><a b='&\u{221};'/>",
    "<!DOCTYPE a [<!ENTITY e '<\u{221}/>'>]><a>&e;</a>",
    "<!DOCTYPE a [<!ATTLIST a b (x\u{221}|y) 'y'>]><a/>",
    "<!DOCTYPE a [<!ENTITY % \u{221} '<!ENTITY e \"x\">'> %\u{221};]><a>&e;</a>",
    "<a xmlns:\u{221}='urn:x'><\u{221}:b/></a>",
];

#[test]
fn off_by_default_and_libexpat_rejects_new_names() {
    assert!(!Parser::new([7; 16]).fifth_edition_names());
    for doc in NEW_NAMES {
        assert!(parse(doc.as_bytes(), false).is_err(), "{doc}");
    }
}

#[test]
fn new_names_accepted_with_the_option() {
    for doc in NEW_NAMES {
        assert_eq!(parse(doc.as_bytes(), true), Ok(()), "UTF-8 {doc}");
        assert_eq!(parse(&utf16(doc, true), true), Ok(()), "UTF-16LE {doc}");
        assert_eq!(parse(&utf16(doc, false), true), Ok(()), "UTF-16BE {doc}");
        assert!(
            parse(&utf16(doc, true), false).is_err(),
            "UTF-16LE off {doc}"
        );
    }
}

#[test]
fn non_names_stay_errors() {
    // U+00D7, U+3000 (ideographic space), a digit and U+0300 at the start,
    // U+037E (Greek question mark), U+F0000 (private use).
    for doc in [
        "<\u{D7}/>",
        "<a\u{3000}b/>",
        "<1a/>",
        "<\u{300}a/>",
        "<\u{37E}/>",
        "<\u{F0000}/>",
        "<a>&\u{D7};</a>",
    ] {
        assert!(parse(doc.as_bytes(), true).is_err(), "{doc}");
        assert!(parse(doc.as_bytes(), false).is_err(), "{doc}");
    }
}

#[test]
fn fourth_edition_names_are_unchanged() {
    for doc in [
        "<\u{E9}t\u{E9}/>",
        "<a\u{B7}b/>",
        "<\u{4E00}/>",
        "<a-b.c_d:e/>",
        "<\u{3007}/>",
        "<a\u{300}/>",
    ] {
        assert_eq!(parse(doc.as_bytes(), false), Ok(()), "{doc}");
        assert_eq!(parse(doc.as_bytes(), true), Ok(()), "{doc}");
    }
}

#[test]
fn latin1_is_the_same_in_both_editions() {
    let doc = b"<?xml version='1.0' encoding='ISO-8859-1'?><\xE9\xB7/>";
    assert_eq!(parse(doc, false), Ok(()));
    assert_eq!(parse(doc, true), Ok(()));
    let bad = b"<?xml version='1.0' encoding='ISO-8859-1'?><\xD7/>";
    assert!(parse(bad, true).is_err());
}

#[test]
fn malformed_utf8_stays_an_error() {
    // An overlong encoding of "A" and a lone surrogate.
    for doc in [
        &b"<\xC1\x81/>"[..],
        b"<a\xED\xA0\x80/>",
        b"<\xF4\x90\x80\x80/>",
    ] {
        assert!(parse(doc, true).is_err());
        assert_eq!(parse(doc, true), parse(doc, false));
    }
}

#[test]
fn setting_is_refused_while_busy_and_cleared_by_reset() {
    let mut p = Parser::new([7; 16]);
    assert!(p.set_fifth_edition_names(true));
    assert_eq!(p.parse(b"<a>", false), Ok(()));
    assert_eq!(p.parsing_status(), ParsingStatus::Parsing);
    assert!(!p.set_fifth_edition_names(false));
    assert!(p.fifth_edition_names());
    assert!(p.reset(None, [7; 16]));
    assert!(!p.fifth_edition_names());
}

#[test]
fn names_split_across_buffers() {
    let doc = "<a\u{10000}\u{221}b/>".as_bytes();
    for split in 0..doc.len() {
        let mut p = Parser::new([7; 16]);
        assert!(p.set_fifth_edition_names(true));
        let (x, y) = doc.split_at(split);
        assert_eq!(p.parse(x, false), Ok(()), "split {split}");
        assert_eq!(p.parse(y, true), Ok(()), "split {split}");
    }
}
