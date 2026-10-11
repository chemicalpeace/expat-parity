//! Upstream-derived vectors (expat/tests/*.c, see tests/vectors/PROVENANCE.md).

use expat_difftest::diff::option_matrix;
use expat_difftest::trace::{is_well_formed_trace, result_error_code};
use expat_difftest::vectors::upstream;
use expat_oracle::{ParseOptions, parse_trace};
use std::collections::HashMap;

#[test]
fn vector_file_is_complete() {
    let v = upstream();
    assert_eq!(
        v.len(),
        540,
        "re-run scripts/extract_vectors.py and update counts"
    );
    let mut ids: Vec<&str> = v.iter().map(|x| x.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), v.len(), "vector ids are unique");
}

#[test]
fn oracle_traces_are_well_formed_under_option_matrix() {
    let opts = option_matrix();
    for v in upstream() {
        let doc = v.doc();
        for o in &opts {
            let t = parse_trace(&doc, o).expect("oracle");
            assert!(is_well_formed_trace(&t), "{} {:?}", v.source, o);
        }
    }
}

#[test]
fn oracle_matches_committed_golden_traces() {
    // Guards against silent drift of the oracle build or the C trace shim.
    let golden: HashMap<String, String> = include_str!("../vectors/upstream.traces.jsonl")
        .lines()
        .map(|l| {
            let j: serde_json::Value = serde_json::from_str(l).expect("json");
            (
                j["id"].as_str().expect("id").to_owned(),
                j["trace"].as_str().expect("trace").to_owned(),
            )
        })
        .collect();
    let v = upstream();
    assert_eq!(golden.len(), v.len());
    for x in v {
        let t = parse_trace(&x.doc(), &ParseOptions::default()).expect("oracle");
        assert_eq!(
            String::from_utf8_lossy(&t),
            golden[&x.id].as_str(),
            "{}",
            x.source
        );
    }
}

#[test]
fn oracle_satisfies_upstream_error_expectations_for_plain_tests() {
    let mut checked = 0;
    for v in upstream() {
        let (Some(code), true) = (v.expected_error_code, v.plain) else {
            continue;
        };
        let t = parse_trace(&v.doc(), &ParseOptions::default()).expect("oracle");
        assert_eq!(
            result_error_code(&t),
            Some(code),
            "{} ({})",
            v.source,
            v.test
        );
        checked += 1;
    }
    assert_eq!(checked, 7, "number of plain upstream expectations changed");
}

/// Upstream vectors whose C trace depends on how the input is split into
/// `XML_Parse` calls (see docs/DIVERGENCES.md, "Observed C quirks"). The port
/// is always compared to C under the *same* chunking, so these do not weaken
/// the differential tests; this list makes any change in the set visible.
const KNOWN_CHUNK_DEPENDENT: &[(&str, usize)] = &[
    ("expat/tests/basic_tests.c:188", 1),
    ("expat/tests/basic_tests.c:456", 1),
    ("expat/tests/basic_tests.c:456", 2),
    ("expat/tests/basic_tests.c:4615", 1),
    ("expat/tests/misc_tests.c:967", 1),
];

#[test]
fn oracle_chunk_dependence_is_exactly_the_known_set() {
    let mut differing = Vec::new();
    for v in upstream() {
        let doc = v.doc();
        let whole = parse_trace(&doc, &ParseOptions::default()).expect("oracle");
        for chunk in [1, 2, 5] {
            let o = ParseOptions {
                chunk_size: chunk,
                ..ParseOptions::default()
            };
            if parse_trace(&doc, &o).expect("oracle") != whole {
                differing.push((v.source.clone(), chunk));
            }
        }
    }
    let known: Vec<(String, usize)> = KNOWN_CHUNK_DEPENDENT
        .iter()
        .map(|(s, c)| ((*s).to_owned(), *c))
        .collect();
    assert_eq!(differing, known);
}
