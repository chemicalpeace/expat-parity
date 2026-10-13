//! External entity bodies that end at token boundaries, against C: the
//! error branches of the external entity processors
//! (`externalEntityInitProcessor2/3`, `externalParEntInitProcessor`,
//! `entityValueInitProcessor`, `entityValueProcessor`,
//! `externalParEntProcessor`; `parse/external.rs`), which the generated
//! cases of `external_parser_diff` rarely end in.
//!
//! Every body (empty, BOM only, text declaration complete or cut, unclosed
//! comment / tag / PI, partial and invalid characters, UTF-16 with and
//! without BOM, a start tag where only declarations are allowed) is read
//! through the test handler (`ExtHandler::Read`, in memory) in five places:
//! an external parameter entity inside an entity value and as an entity
//! value (declared in the external subset, where that is allowed), the
//! external subset itself, an
//! external parameter entity referenced in the internal subset, and an
//! external general entity in content. Compared by canonical trace (events,
//! the child's `extend <ok> <code> <line> <column> <index>`, the parent's
//! result) under the option matrix (whole, 1- and 7-byte chunks, buffer
//! API, namespaces) and in every parameter-entity parsing mode.

use expat_difftest::diff::option_matrix;
use expat_difftest::port::port_trace;
use expat_difftest::trace::first_difference;
use expat_oracle::{ExtHandler, ParamEntityParsing, ParseOptions, parse_trace};

const BODIES: &[&[u8]] = &[
    b"",
    b"\xEF\xBB\xBF",
    b"\xEF\xBB\xBFabc",
    b"\xEF\xBB",
    b"<?xml encoding='UTF-8'?>",
    b"<?xml encoding='UTF-8'?>abc",
    b"<?xml encoding='UTF-8'",
    b"<?xml encoding='UTF-8'?",
    b"<?xml version='1.0'?>",
    b"<?xml encoding='ISO-8859-1'?>\xE9",
    b"<?xml encoding='x-unknown'?>abc",
    b"\xEF\xBB\xBF<?xml encoding='UTF-8'?>abc",
    b"<!--",
    b"<?xml encoding='UTF-8'?><!--",
    b"<?pi",
    b"<a",
    b"<a/>",
    b"<?xml encoding='UTF-8'?><a",
    b"<?xml encoding='UTF-8'?><a/>",
    b"abc<",
    b"\xC3",
    b"abc\xC3",
    b"<?xml encoding='UTF-8'?>\xC3",
    b"\xFF",
    b"abc\xFF",
    b"\x00",
    b"<",
    b"<!",
    b"<!ENTITY",
    b"<!ENTITY y 'v'>",
    b"<!ENTITY y 'v'",
    b"]]>",
    b"<![INCLUDE[",
    b"<![IGNORE[ x ]]>",
    b"%p;",
    b"&amp;",
    b"&#",
    b"\r",
    b"\r\n",
    b"x\r",
    b"\xFF\xFE<\x00a\x00/\x00>\x00",
    b"\xFE\xFF\x00<\x00a\x00/\x00>",
    b"\xFF\xFE<\x00",
    b"<\x00?\x00x\x00m\x00l\x00 \x00e\x00n\x00c\x00o\x00d\x00i\x00n\x00g\x00=\x00'\x00U\x00T\x00F\x00-\x001\x006\x00'\x00?\x00>\x00",
    b"\x00<\x00a",
    b"\xFF\xFE",
    b"\xFE",
    b"'abc'",
    b"<?xml encoding='UTF-8'?>'abc'",
    b"<?xml encoding='UTF-8'?>'abc",
    b"\xEF\xBB\xBF'abc'",
    b"'a<b'",
    b"'abc' x",
    // In an entity value the parser reads an XML declaration, not a text
    // declaration (`processXmlDecl(parser, 0, ..)`): `version` is required
    // and `standalone` allowed. After it, `entityValueProcessor` tokenizes
    // the rest: complete, invalid, unclosed, a partial character, the start
    // of an element, and (in chunked parses) a token cut by the buffer end.
    b"<?xml version='1.0'?>'abc'",
    b"<?xml version='1.0'?>abc",
    b"<?xml version='1.0'?>'a' 'b' <!-- c -->",
    b"<?xml version='1.0'?>\xFF",
    b"<?xml version='1.0'?>'a'\xFF",
    b"<?xml version='1.0'?><!--",
    b"<?xml version='1.0'?>'abc",
    b"<?xml version='1.0'?>\xC3",
    b"<?xml version='1.0'?>'a'\xE4\xB8",
    b"<?xml version='1.0'?><a",
    b"<?xml version='1.0'?>'a'<a/>",
    b"<?xml version='1.0' encoding='UTF-8'?>'abc'",
    b"<?xml version='1.0' encoding='ISO-8859-1'?>'\xE9'",
    b"<?xml version='1.0' encoding='x-unknown'?>'abc'",
    b"<?xml version='1.0' standalone='yes'?>'abc'",
    b"<?xml version='1.0' standalone='no'?>'abc'",
    b"\xEF\xBB\xBF<?xml version='1.0'?>'abc'",
    b"<?xml version='1.0'?><?xml version='1.0'?>",
    b"<?xml version='1.0'?>&amp;%p;",
];

/// Documents reading the body (system identifier `b`): `s.dtd` references
/// it inside an entity value literal (the child parser starts in
/// `entityValueInitProcessor`, `inEntityValue` being set by
/// `storeEntityValue`), `v.dtd` in place of the entity value (an inner
/// parameter-entity reference: `externalParEntProcessor`); the external subset; an
/// external parameter entity in the internal subset; an external general
/// entity in content (once and twice).
const DOCS: &[&[u8]] = &[
    b"<!DOCTYPE d SYSTEM 's.dtd'><d>&x;</d>",
    b"<!DOCTYPE d SYSTEM 'v.dtd'><d>&x;</d>",
    b"<!DOCTYPE d SYSTEM 'b'><d/>",
    b"<!DOCTYPE d [<!ENTITY % p SYSTEM 'b'>%p;]><d/>",
    b"<!DOCTYPE d [<!ENTITY e SYSTEM 'b'>]><d>&e;</d>",
    b"<!DOCTYPE d [<!ENTITY e SYSTEM 'b'>]><d>x&e;y&e;</d>",
];

#[test]
fn external_entity_bodies_match_c() {
    let mut compared = 0usize;
    let mut child_errors = 0usize;
    for body in BODIES {
        let ext = vec![
            (b"b".to_vec(), body.to_vec()),
            (
                b"s.dtd".to_vec(),
                b"<!ENTITY % p SYSTEM 'b'><!ENTITY x '[%p;]'>".to_vec(),
            ),
            // The parameter entity stands where the entity value would
            // (`XML_ROLE_INNER_PARAM_ENTITY_REF`): not `inEntityValue`.
            (
                b"v.dtd".to_vec(),
                b"<!ENTITY % p SYSTEM 'b'><!ENTITY x %p;>".to_vec(),
            ),
        ];
        for base in option_matrix() {
            for pep in [
                ParamEntityParsing::Never,
                ParamEntityParsing::UnlessStandalone,
                ParamEntityParsing::Always,
            ] {
                let opts = ParseOptions {
                    param_entity_parsing: pep,
                    ext_handler: ExtHandler::Read,
                    ext_entities: ext.clone(),
                    ..base.clone()
                };
                for doc in DOCS {
                    let c = parse_trace(doc, &opts).expect("oracle");
                    let r = port_trace(doc, &opts);
                    assert!(
                        c == r,
                        "body {:?} doc {} {opts:?}: {:?}",
                        String::from_utf8_lossy(body),
                        String::from_utf8_lossy(doc),
                        first_difference(&c, &r)
                    );
                    compared += 1;
                    child_errors += c.windows(9).filter(|w| w == b"extend 0 ").count();
                }
            }
        }
    }
    assert_eq!(
        compared,
        BODIES.len() * option_matrix().len() * 3 * DOCS.len()
    );
    // The point of the bodies: many children end in an error.
    assert!(child_errors > compared / 4, "{child_errors} of {compared}");
}
