//! The harness must detect a deliberately wrong implementation; otherwise a
//! green differential run would prove nothing.

use expat_difftest::diff::{Oracle, TraceSource, compare_all, option_matrix};
use expat_difftest::vectors::upstream;
use expat_oracle::ParseOptions;

/// The oracle with one injected bug: the column of the final position is off
/// by one whenever the document contains an attribute.
struct BuggyColumn;

impl TraceSource for BuggyColumn {
    fn name(&self) -> &str {
        "oracle-with-column-bug"
    }
    fn trace(&self, doc: &[u8], opts: &ParseOptions) -> Vec<u8> {
        let t = Oracle.trace(doc, opts);
        if !t.windows(5).any(|w| w == b"attr ") {
            return t;
        }
        let s = String::from_utf8(t).expect("ascii trace");
        let (head, last) = s.trim_end().rsplit_once('\n').unwrap_or(("", s.trim_end()));
        let mut f: Vec<String> = last.split(' ').map(String::from).collect();
        let col: u64 = f[4].parse().expect("column");
        f[4] = (col + 1).to_string();
        let prefix = if head.is_empty() {
            String::new()
        } else {
            format!("{head}\n")
        };
        format!("{prefix}{}\n", f.join(" ")).into_bytes()
    }
}

/// The oracle with a dropped event: comments are silently lost.
struct DropsComments;

impl TraceSource for DropsComments {
    fn name(&self) -> &str {
        "oracle-dropping-comments"
    }
    fn trace(&self, doc: &[u8], opts: &ParseOptions) -> Vec<u8> {
        let t = Oracle.trace(doc, opts);
        t.split_inclusive(|&b| b == b'\n')
            .filter(|l| !l.starts_with(b"comment "))
            .flatten()
            .copied()
            .collect()
    }
}

fn docs() -> Vec<(String, Vec<u8>)> {
    upstream()
        .into_iter()
        .map(|v| (v.source.clone(), v.doc()))
        .collect()
}

#[test]
fn identical_sources_agree() {
    let d = docs();
    let m = compare_all(
        &Oracle,
        &Oracle,
        d.iter().map(|(i, x)| (i.clone(), x.as_slice())),
        &option_matrix(),
    );
    assert!(m.is_empty(), "{}", m[0]);
}

#[test]
fn detects_wrong_error_position() {
    let d = docs();
    let m = compare_all(
        &Oracle,
        &BuggyColumn,
        d.iter().map(|(i, x)| (i.clone(), x.as_slice())),
        &[ParseOptions::default()],
    );
    assert!(!m.is_empty());
    assert!(m.iter().all(|x| x.left.starts_with("result ")));
}

#[test]
fn detects_dropped_events() {
    let d = docs();
    let m = compare_all(
        &Oracle,
        &DropsComments,
        d.iter().map(|(i, x)| (i.clone(), x.as_slice())),
        &[ParseOptions::default()],
    );
    assert!(!m.is_empty());
    assert!(m.iter().any(|x| x.left.starts_with("comment ")));
}
