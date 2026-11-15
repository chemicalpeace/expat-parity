//! Upstream's unit tests of the entity-size precomputation (libexpat
//! 2.10.0 `tests/entities_tests.c` from #1442 and `test_xcs_nstr` of
//! `tests/xcs_tests.c` from #1439), ported.

use super::*;
use crate::parse::{Handler, Parser, Status};

struct Nop;
impl Handler for Nop {}

/// `test_is_cdata_comment_or_pi_start`.
#[test]
fn is_cdata_comment_or_pi_start_matches_upstream() {
    let cdata = b"<![CDATA[";
    let comment = b"<!--";
    let pi = b"<?";
    assert_eq!(
        is_cdata_comment_or_pi_start(cdata, cdata.len()),
        Some((SectionStart::Cdata, cdata.len()))
    );
    assert_eq!(
        is_cdata_comment_or_pi_start(comment, comment.len()),
        Some((SectionStart::Comment, comment.len()))
    );
    assert_eq!(is_cdata_comment_or_pi_start(pi, pi.len()), Some((SectionStart::Pi, pi.len())));
    assert_eq!(is_cdata_comment_or_pi_start(pi, 0), None);
    assert_eq!(is_cdata_comment_or_pi_start(pi, 1), None);
    // A prefix of the CDATA opening is not one.
    assert_eq!(is_cdata_comment_or_pi_start(b"<![CDATA", 8), None);
    assert_eq!(is_cdata_comment_or_pi_start(b"<!-", 3), None);
    assert_eq!(is_cdata_comment_or_pi_start(b"<a", 2), None);
}

/// `test_is_predefined_entity_name`.
#[test]
fn is_predefined_entity_name_matches_upstream() {
    assert_eq!(is_predefined_entity_name(b"amp"), Some(b'&'));
    assert_eq!(is_predefined_entity_name(b"apos"), Some(b'\''));
    assert_eq!(is_predefined_entity_name(b"gt"), Some(b'>'));
    assert_eq!(is_predefined_entity_name(b"lt"), Some(b'<'));
    assert_eq!(is_predefined_entity_name(b"quot"), Some(b'"'));
    assert_eq!(is_predefined_entity_name(b"am"), None);
    assert_eq!(is_predefined_entity_name(b"Amp"), None);
    assert_eq!(is_predefined_entity_name(b""), None);
}

/// `test_xcs_nstr`.
#[test]
fn xcsnstr_matches_upstream() {
    let haystack = b"haystack";
    let hay = b"hay";
    let stack = b"stack";
    assert_eq!(xcsnstr(haystack, haystack, haystack.len()), Some(0));
    assert_eq!(xcsnstr(haystack, hay, haystack.len()), Some(0));
    assert_eq!(xcsnstr(haystack, hay, hay.len()), Some(0));
    assert_eq!(xcsnstr(haystack, hay, hay.len() - 1), None);
    assert_eq!(xcsnstr(haystack, stack, haystack.len()), Some(hay.len()));
    assert_eq!(xcsnstr(haystack, b"other", haystack.len()), None);
    assert_eq!(xcsnstr(haystack, b"", haystack.len()), Some(0));
    assert_eq!(xcsnstr(hay, haystack, hay.len()), None);
}

/// Scans `text` as a general entity's text with `p`'s entity table;
/// asserts a complete scan to the end and returns the size.
fn expanded_size(p: &Parser, text: &[u8]) -> u64 {
    let mut pos = 0;
    let mut size = 0;
    let r = scan_entity_text(text, false, &mut pos, &mut size, |n| p.scan_lookup(false, n));
    assert_eq!(r, EntityScanResult::Complete, "{:?}", core::str::from_utf8(text));
    assert_eq!(pos, text.len());
    size
}

/// `test_scan_entity_text`.
#[test]
fn scan_entity_text_matches_upstream() {
    let doc = b"<!DOCTYPE doc[\n  <!ENTITY ten '123456789a'>\n]>\n<doc/>";
    let mut p = Parser::new(None, None, [0; 16]);
    assert_eq!(p.parse(&mut Nop, doc, true), Status::Ok);

    let len = |s: &str| s.len() as u64;
    assert_eq!(expanded_size(&p, b""), 0);
    assert_eq!(expanded_size(&p, b"123"), 3);
    let predefined = "&amp;&apos;&gt;&lt;&quot;";
    assert_eq!(expanded_size(&p, predefined.as_bytes()), len(predefined) + 5);
    assert_eq!(expanded_size(&p, b"&undefined;"), len("&undefined;"));

    // "ten" has no size yet: a reference to it blocks the scan, parked at
    // the reference.
    let text = b"referencing &ten;";
    let (mut pos, mut size) = (0, 0);
    let r = scan_entity_text(text, false, &mut pos, &mut size, |n| p.scan_lookup(false, n));
    let blocker = match r {
        EntityScanResult::Blocked(t) => Some(t),
        _ => None,
    };
    assert!(blocker.is_some(), "expected a blocked scan, got {r:?}");
    let Some(ten) = blocker else { return };
    assert_eq!(p.dtd.entity_name(ten), b"ten");
    assert_eq!(size, len("referencing "));
    assert_eq!(pos, "referencing ".len());
    if let Some(e) = p.dtd.entity_mut(ten) {
        // Which makes it stop being a blocker.
        e.expanded_size = 10;
    }

    assert_eq!(expanded_size(&p, b"&ten;"), len("&ten;") + len("123456789a"));
    assert_eq!(
        expanded_size(&p, b"[&ten;&ten;|&ten;]"),
        3 * (len("&ten;") + len("123456789a")) + len("[|]")
    );

    // Cases where no entities should be recognised.
    let quiet = "<![CDATA[&ten;]]><!--&ten;--><?php &ten;?>";
    assert_eq!(expanded_size(&p, quiet.as_bytes()), len(quiet));
    for s in ["<![CDATA[&ten;", "<!--&ten;", "<?php &ten;"] {
        assert_eq!(expanded_size(&p, s.as_bytes()), len(s), "{s}");
    }
}

/// Beyond upstream: the depth-first search over a reference chain, the
/// cycle rule and the up-front refusal in `processEntity`.
#[test]
fn closure_sizes_and_refusal() {
    // b -> a (twice), c -> b, d -> d (cycle, never expanded).
    let doc = b"<!DOCTYPE r [<!ENTITY a 'xy'><!ENTITY b '&a;-&a;'><!ENTITY c '[&b;]'>\
<!ENTITY d '&d;'>]><r>&c;</r>";
    let mut p = Parser::new(None, None, [0; 16]);
    assert_eq!(p.parse(&mut Nop, doc, true), Status::Ok);
    let size = |p: &Parser, name: &[u8]| {
        let id = p.dtd.general_entities.find(&p.salt, name).map(crate::parse::EntityRef::General);
        id.and_then(|r| p.dtd.entity(r)).map(|e| (e.expanded_size, e.being_analyzed))
    };
    // a: 2; b: 2 * (3 + 2) + 1 = 11; c: 1 + (3 + 11) + 1 = 16.
    assert_eq!(size(&p, b"a"), Some((2, false)));
    assert_eq!(size(&p, b"b"), Some((11, false)));
    assert_eq!(size(&p, b"c"), Some((16, false)));
    // Never referenced: never computed.
    assert_eq!(size(&p, b"d"), Some((0, false)));

    // A breach is reported before anything is expanded.
    let doc = b"<!DOCTYPE r [<!ENTITY a 'xxxxxxxxxx'><!ENTITY b '&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;'>]>\
<r>&b;</r>";
    let mut p = Parser::new(None, None, [0; 16]);
    assert!(p.set_billion_laughs_maximum_amplification(1.0));
    assert!(p.set_billion_laughs_activation_threshold(0));
    assert_eq!(p.parse(&mut Nop, doc, true), Status::Error);
    assert_eq!(p.accounting().count_bytes_indirect, 0);
}
