//! Every structure size the port's allocation tracker charges against the
//! C compiler's `sizeof` / `offsetof` of the same structure in the vendored
//! `xmlparse.c`, built by the oracle for the platform the test runs on
//! (decision D-034; runs in the `test` job of every CI platform).
//!
//! A size below C's lets the port accept documents C refuses at the
//! default limits (docs/DIVERGENCES.md R1): it always fails. A size above
//! C's is allowed only for the over-counts listed in [`KNOWN_OVER_COUNTS`],
//! which the size-parity stage of D-034 removes.

use expat::internal::parse::TRACKER_SIZES;

/// Sizes the port still charges above C's on some platform (never below):
/// `ELEMENT_TYPE` (12 words, C 88 bytes on 64-bit targets) and `NS_ATT`
/// (three 8-byte words; C has 4-byte `unsigned long` on Windows).
const KNOWN_OVER_COUNTS: [&str; 2] = ["ELEMENT_TYPE", "NS_ATT"];

#[test]
fn tracker_sizes_match_the_c_compiler() {
    let c = expat_oracle::c_sizes();
    assert_eq!(
        c.len(),
        expat_oracle::ORACLE_C_SIZE_NAMES.len(),
        "the oracle returned fewer sizes than it names"
    );
    let mut port_names: Vec<&str> = TRACKER_SIZES.iter().map(|&(n, _)| n).collect();
    let mut c_names: Vec<&str> = c.iter().map(|&(n, _)| n).collect();
    port_names.sort_unstable();
    c_names.sort_unstable();
    assert_eq!(
        port_names, c_names,
        "the port and the oracle list different structures"
    );

    let target = format!(
        "{}-{}-{}",
        std::env::consts::ARCH,
        std::env::consts::OS,
        std::env::consts::FAMILY
    );
    let mut failures = Vec::new();
    for &(name, c_size) in &c {
        let port = TRACKER_SIZES
            .iter()
            .find(|&&(n, _)| n == name)
            .map(|&(_, s)| s)
            .unwrap_or_default();
        let verdict = if port == c_size {
            "equal"
        } else if port > c_size && KNOWN_OVER_COUNTS.contains(&name) {
            "over (known)"
        } else if port > c_size {
            failures.push(format!(
                "{name}: port {port} > C {c_size} (not a known over-count)"
            ));
            "OVER"
        } else {
            failures.push(format!("{name}: port {port} < C {c_size} (under-count)"));
            "UNDER"
        };
        println!("SIZES {target} {name:<28} C {c_size:>4} port {port:>4} {verdict}");
    }
    assert!(failures.is_empty(), "{target}: {failures:?}");
}
