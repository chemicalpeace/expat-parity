//! `XML_UnparsedEntityDeclHandler` against C (`xmlparse.c` doProlog,
//! `XML_ROLE_ENTITY_NOTATION_NAME`): with the handler set, an entity
//! declared with `NDATA` is reported as `unparsed <name> <base> <systemId>
//! <publicId> <notation>` and not to the entity declaration handler;
//! without it, the entity declaration handler gets the notation. Compared
//! by canonical trace (error code, line, column and byte index included)
//! under the option matrix, with and without the handler, in the internal
//! subset and in an external subset read through the test handler.

use expat_difftest::diff::{Oracle, compare_all, option_matrix};
use expat_difftest::port::{Port, port_trace};
use expat_difftest::trace::first_difference;
use expat_oracle::{ExtHandler, ParamEntityParsing, ParseOptions, parse_trace};

/// Unparsed-entity declarations in the internal subset, valid and not.
const DOCS: &[&str] = &[
    "<!DOCTYPE d [<!NOTATION n SYSTEM 'n'><!ENTITY u SYSTEM 'u.bin' NDATA n>]><d/>",
    "<!DOCTYPE d [<!ENTITY u PUBLIC '-//P//EN' 'u.bin' NDATA n>]><d/>",
    "<!DOCTYPE d [<!ENTITY u SYSTEM '' NDATA n>]><d/>",
    // Undeclared notation: not checked by a non-validating parser.
    "<!DOCTYPE d [<!ENTITY u SYSTEM 'u' NDATA undeclared>]><d/>",
    // Duplicate: the first declaration wins, the second is not reported.
    "<!DOCTYPE d [<!ENTITY u SYSTEM 'a' NDATA n><!ENTITY u SYSTEM 'b' NDATA m>]><d/>",
    "<!DOCTYPE d [<!ENTITY u 'text'><!ENTITY u SYSTEM 'b' NDATA m>]><d/>",
    "<!DOCTYPE d [<!ENTITY u SYSTEM 'b' NDATA m><!ENTITY u 'text'>]><d/>",
    // Several, mixed with other declarations.
    "<!DOCTYPE d [<!ENTITY a SYSTEM 'a' NDATA n><!ENTITY b 'x'><!ENTITY c SYSTEM 'c'><!ENTITY e PUBLIC 'p' 'e' NDATA q><!ATTLIST d f ENTITY #IMPLIED>]><d f='a'/>",
    // References to an unparsed entity: content and attribute value.
    "<!DOCTYPE d [<!ENTITY u SYSTEM 'u' NDATA n>]><d>&u;</d>",
    "<!DOCTYPE d [<!ENTITY u SYSTEM 'u' NDATA n>]><d a='&u;'/>",
    "<!DOCTYPE d [<!ENTITY u SYSTEM 'u' NDATA n><!ENTITY v '&u;'>]><d>&v;</d>",
    // Malformed: NDATA on a parameter entity, missing name, internal value.
    "<!DOCTYPE d [<!ENTITY % p SYSTEM 'p' NDATA n>]><d/>",
    "<!DOCTYPE d [<!ENTITY u SYSTEM 'u' NDATA>]><d/>",
    "<!DOCTYPE d [<!ENTITY u 'v' NDATA n>]><d/>",
    "<!DOCTYPE d [<!ENTITY u SYSTEM 'u' NDATA n n>]><d/>",
    "<!DOCTYPE d [<!ENTITY u SYSTEM 'u'NDATA n>]><d/>",
    // Namespaces: names with colons.
    "<!DOCTYPE d [<!ENTITY x:u SYSTEM 'u' NDATA x:n>]><d xmlns:x='urn:x'/>",
    // After a parameter-entity reference the remaining declarations are
    // skipped unless the entity was read (standalone='no').
    "<!DOCTYPE d [<!ENTITY % e SYSTEM 'e.ent'>%e;<!ENTITY u SYSTEM 'u' NDATA n>]><d/>",
    "<?xml version='1.0' standalone='yes'?><!DOCTYPE d [<!ENTITY % e SYSTEM 'e.ent'>%e;<!ENTITY u SYSTEM 'u' NDATA n>]><d/>",
    // Non-ASCII names and identifiers (UTF-8).
    "<!DOCTYPE d [<!ENTITY \u{e9} SYSTEM '\u{e9}.bin' NDATA \u{4e00}>]><d/>",
];

fn with_handler(opts: &[ParseOptions], on: bool) -> Vec<ParseOptions> {
    opts.iter()
        .map(|o| ParseOptions {
            unparsed_handler: on,
            ..o.clone()
        })
        .collect()
}

#[test]
fn internal_subset_matches_c_with_and_without_the_handler() {
    let docs: Vec<(String, &[u8])> = DOCS
        .iter()
        .map(|d| ((*d).to_string(), d.as_bytes()))
        .collect();
    for on in [true, false] {
        let mismatches = compare_all(
            &Oracle,
            &Port,
            docs.clone(),
            &with_handler(&option_matrix(), on),
        );
        assert!(mismatches.is_empty(), "handler {on}: {mismatches:#?}");
    }
    // The handler is really exercised: C reports `unparsed` lines.
    let t = parse_trace(
        DOCS[0].as_bytes(),
        &ParseOptions {
            unparsed_handler: true,
            ..ParseOptions::default()
        },
    )
    .expect("oracle");
    let t = String::from_utf8_lossy(&t);
    assert!(t.lines().any(|l| l.starts_with("unparsed ")), "{t}");
    assert!(!t.lines().any(|l| l.starts_with("entity ")), "{t}");
}

/// Unparsed entities declared in an external subset and in an external
/// parameter entity, read through the test handler, in every
/// parameter-entity parsing mode.
#[test]
fn external_subset_matches_c() {
    let ext: Vec<(Vec<u8>, Vec<u8>)> = vec![
        (b"s.dtd".to_vec(), b"<!NOTATION n SYSTEM 'n'><!ENTITY u SYSTEM 'u' NDATA n><!ENTITY % p SYSTEM 'p.ent'>%p;".to_vec()),
        (b"p.ent".to_vec(), b"<?xml encoding='UTF-8'?><!ENTITY w PUBLIC 'pw' 'w' NDATA n>".to_vec()),
        (b"e.ent".to_vec(), b"<!ENTITY v SYSTEM 'v' NDATA m>".to_vec()),
    ];
    let docs: [&[u8]; 4] = [
        b"<!DOCTYPE d SYSTEM 's.dtd'><d/>",
        b"<!DOCTYPE d SYSTEM 's.dtd' [<!ENTITY u SYSTEM 'first' NDATA i>]><d/>",
        b"<!DOCTYPE d [<!ENTITY % e SYSTEM 'e.ent'>%e;<!ENTITY u SYSTEM 'u' NDATA n>]><d/>",
        b"<?xml version='1.0' standalone='yes'?><!DOCTYPE d SYSTEM 's.dtd'><d/>",
    ];
    let mut compared = 0;
    let mut saw_external = false;
    for base in option_matrix() {
        for pep in [
            ParamEntityParsing::Never,
            ParamEntityParsing::UnlessStandalone,
            ParamEntityParsing::Always,
        ] {
            for on in [true, false] {
                let opts = ParseOptions {
                    unparsed_handler: on,
                    param_entity_parsing: pep,
                    ext_handler: ExtHandler::Read,
                    ext_entities: ext.clone(),
                    ..base.clone()
                };
                for doc in docs {
                    let c = parse_trace(doc, &opts).expect("oracle");
                    let r = port_trace(doc, &opts);
                    assert!(
                        c == r,
                        "{opts:?} {}: {:?}",
                        String::from_utf8_lossy(doc),
                        first_difference(&c, &r)
                    );
                    compared += 1;
                    saw_external |= on && c.windows(11).any(|w| w == b"unparsed w ");
                }
            }
        }
    }
    assert_eq!(compared, option_matrix().len() * 3 * 2 * docs.len());
    // The entity declared in the external parameter entity was reported.
    assert!(saw_external);
}
