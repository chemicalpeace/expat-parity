//! Module 10: the port's parser against the C oracle, compared by
//! canonical trace (docs/ARCHITECTURE.md) on the upstream vectors under the
//! option matrix.

use expat_difftest::diff::option_matrix;
use expat_difftest::port::{PortTrace, port_trace};
use expat_difftest::trace::first_difference;
use expat_difftest::vectors::upstream;
use expat_oracle::parse_trace;

#[test]
fn upstream_vectors_match_c() {
    let opts = option_matrix();
    let mut compared = 0usize;
    let mut unported = 0usize;
    let mut docs_fully_compared = 0usize;
    let mut failures = Vec::new();
    for v in upstream() {
        let doc = v.doc();
        let mut all = true;
        for o in &opts {
            let c = parse_trace(&doc, o).expect("oracle");
            match port_trace(&doc, o) {
                PortTrace::DtdNotPorted => {
                    unported += 1;
                    all = false;
                }
                PortTrace::Trace(t) => {
                    compared += 1;
                    if let Some((line, l, r)) = first_difference(&c, &t) {
                        failures.push(format!(
                            "{} {:?}: line {line}: C `{l}` port `{r}`",
                            v.source, o
                        ));
                        all = false;
                    }
                }
            }
        }
        if all {
            docs_fully_compared += 1;
        }
    }
    eprintln!(
        "parse_diff: {compared} (vector, options) pairs equal to C, {unported} with a DTD (module 11), {docs_fully_compared} of 540 vectors compared under every option set"
    );
    for f in failures.iter().take(40) {
        eprintln!("{f}");
    }
    assert!(failures.is_empty(), "{} mismatches", failures.len());
}
