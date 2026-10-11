//! Differential tests of the prolog state machine (module 8: `xmlrole.c`)
//! against C. See `expat_difftest::role` for what is compared.

use expat::internal::tok::{Normal, Utf16};
use expat_difftest::role::{check_all_single_steps, check_document, check_roles, synthetic};
use expat_difftest::tok::{Subject, unknown_from_seed};
use expat_difftest::vectors;
use proptest::prelude::*;

#[test]
fn every_handler_token_and_keyword() {
    check_all_single_steps(&Normal::Utf8);
    check_all_single_steps(&Utf16::Big);
}

#[test]
fn upstream_vectors_as_document_and_external_entity() {
    for v in vectors::upstream() {
        let doc = v.doc();
        for enc in [Normal::Utf8, Normal::Latin1Ns] {
            check_document(&enc, &doc);
        }
        for enc in Utf16::ALL {
            check_document(&enc, &doc);
        }
    }
}

/// Declarations that drive the machine through every DTD construct,
/// including conditional sections (external subset only).
const DTD: &str = r#"<?xml version="1.0"?>
<!ENTITY % pe "x"> <!ENTITY ge SYSTEM "a" NDATA n> <!ENTITY e2 PUBLIC "p" "s">
<!ENTITY % pe2 SYSTEM "s"> <!ENTITY % pe3 PUBLIC "p" "s">
<!NOTATION n SYSTEM "s"> <!NOTATION n2 PUBLIC "p"> <!NOTATION n3 PUBLIC "p" "s">
<!ATTLIST a b CDATA #IMPLIED c ID #REQUIRED d (x|y) "x" e NOTATION (n|n2) #FIXED "n"
  f IDREF 'v' g IDREFS #IMPLIED h ENTITY #IMPLIED i ENTITIES #IMPLIED j NMTOKEN #IMPLIED k NMTOKENS #IMPLIED>
<!ELEMENT a EMPTY> <!ELEMENT b ANY> <!ELEMENT c (#PCDATA)> <!ELEMENT d (#PCDATA|a|b)*>
<!ELEMENT e ((a,b)|(c?,d*)+)?> <!ELEMENT f (a|b)+> <!ELEMENT g (a)*>
<![INCLUDE[ <!ELEMENT h EMPTY> <![ IGNORE [ junk ]]> ]]> %pe; <?pi x?> <!-- c -->
"#;

#[test]
fn dtd_constructs() {
    let doc = format!("<!DOCTYPE d [{DTD}]><d/>");
    for enc in Normal::ALL {
        check_document(&enc, doc.as_bytes());
        check_document(&enc, DTD.as_bytes());
    }
    for enc in Utf16::ALL {
        let b: Vec<u8> = DTD
            .encode_utf16()
            .flat_map(|u| match enc {
                Utf16::Little | Utf16::LittleNs => u.to_le_bytes(),
                Utf16::Big | Utf16::BigNs => u.to_be_bytes(),
            })
            .collect();
        check_document(&enc, &b);
    }
}

fn check_synthetic<S: Subject>(s: &S, picks: &[(usize, usize)], external: bool) {
    let (buf, toks) = synthetic(s, picks);
    check_roles(s, external, &buf, &toks);
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 2_000,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn synthetic_token_sequences(
        picks in proptest::collection::vec((0usize..64, 0usize..64), 0..40),
        external in any::<bool>(),
        e in 0usize..12,
        seed in any::<u64>(),
    ) {
        if let Some(&enc) = Normal::ALL.get(e) {
            check_synthetic(&enc, &picks, external);
        } else {
            check_synthetic(&Utf16::ALL[e - 8], &picks, external);
        }
        if let Some(pair) = unknown_from_seed(seed) {
            check_synthetic(&pair, &picks, external);
        }
    }
}
