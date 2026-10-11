//! Baseline throughput of the C oracle (including trace generation).
//! Port-vs-C comparisons are added as the Rust parser comes online.

use criterion::Criterion;
use std::hint::black_box;

fn doc(n: usize) -> Vec<u8> {
    let mut d = b"<?xml version='1.0'?><root>".to_vec();
    for i in 0..n {
        d.extend_from_slice(
            format!("<item id='{i}' kind=\"k&amp;v\">text {i} &lt; more</item>").as_bytes(),
        );
    }
    d.extend_from_slice(b"</root>");
    d
}

fn bench(c: &mut Criterion) {
    let input = doc(2000);
    let opts = expat_oracle::ParseOptions::default();
    c.bench_function("oracle_parse_trace_2000_items", |b| {
        b.iter(|| expat_oracle::parse_trace(black_box(&input), &opts))
    });
}

// Hand-written main instead of `criterion_group!`, whose generated function
// has no doc comment and would trip `missing_docs`.
fn main() {
    let mut c = Criterion::default().configure_from_args();
    bench(&mut c);
    c.final_summary();
}
