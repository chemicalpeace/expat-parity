//! Growth of the namespaced-attribute hash table (`m_nsAtts`,
//! `xmlparse.c` storeAtts: the table doubles while
//! `(nPrefixes << 1) >> m_nsAttsPower` is non-zero, and is reused across
//! start tags through `m_nsAttsVersion`) against C.
//!
//! Start tags with 1 to 1,100 prefixed attributes, around every power of
//! two the table passes, are compared by canonical trace under the option
//! matrix and with namespace processing (with and without triplets): the
//! attributes' expanded names, their order, and the error, line, column
//! and byte index of a duplicate expanded name (two prefixes bound to one
//! URI) placed first, in the middle or last, which is only found through
//! the table. A sequence of start tags that grow and shrink checks that a
//! reused table does not report stale entries. The allocation tracker
//! charges the table's growth (`tracker.rs` `ns_atts`): a threshold sweep
//! checks that wherever C stops with `XML_ERROR_NO_MEMORY`, the port does
//! too, at the same point and with the same trace (exact parity, D-034,
//! `expat_difftest::tracker`).

use expat_difftest::diff::option_matrix;
use expat_difftest::port::port_trace;
use expat_difftest::trace::{first_difference, result_error_code};
use expat_difftest::tracker::{self, NO_MEMORY, Relation};
use expat_oracle::{Factor, Limits, ParseOptions, parse_trace};

/// Numbers of prefixed attributes: below, at and above each power of two
/// up to 1,024.
fn sizes() -> Vec<usize> {
    let mut v = vec![1, 2, 3];
    let mut p = 4;
    while p <= 1024 {
        v.extend([p - 1, p, p + 1]);
        p *= 2;
    }
    v.push(1100);
    v.sort_unstable();
    v.dedup();
    v
}

/// Where a duplicate expanded name goes.
#[derive(Clone, Copy, Debug)]
enum Dup {
    None,
    First,
    Middle,
    Last,
}

/// `<d xmlns:p0='urn:0' .. xmlns:p{k-1}='urn:{k-1}' xmlns:q='urn:0'
/// p{i%k}:a{i}='v{i}' ..>`, `n` prefixed attributes over `k` prefixes, and
/// a duplicate of `p0:a0` written as `q:a0` (same URI) at `dup`.
fn start_tag(n: usize, k: usize, dup: Dup, out: &mut Vec<u8>) {
    out.extend_from_slice(b"<d");
    for j in 0..k {
        out.extend_from_slice(format!(" xmlns:p{j}='urn:{j}'").as_bytes());
    }
    out.extend_from_slice(b" xmlns:q='urn:0'");
    // A few unprefixed attributes, which do not enter the table.
    out.extend_from_slice(b" u='1' w='2'");
    let dup_at = match dup {
        Dup::None => usize::MAX,
        Dup::First => 1,
        Dup::Middle => n / 2,
        Dup::Last => n,
    };
    for i in 0..n {
        if i == dup_at {
            out.extend_from_slice(b"\n q:a0='dup'");
        }
        out.extend_from_slice(format!(" p{}:a{i}='v{i}'", i % k).as_bytes());
    }
    if dup_at == n {
        out.extend_from_slice(b"\n q:a0='dup'");
    }
    out.push(b'>');
}

fn documents() -> Vec<(String, Vec<u8>)> {
    let mut v = Vec::new();
    for n in sizes() {
        for k in [1usize, 3, 17] {
            for dup in [Dup::None, Dup::First, Dup::Middle, Dup::Last] {
                if matches!(dup, Dup::First | Dup::Middle) && n < 3 {
                    continue;
                }
                let mut doc = Vec::new();
                start_tag(n, k, dup, &mut doc);
                doc.extend_from_slice(b"</d>");
                v.push((format!("n={n} k={k} {dup:?}"), doc));
            }
        }
    }
    // Growing then shrinking start tags in one document: the table keeps
    // its size and is cleared by version, never reporting a name of an
    // earlier tag as a duplicate; the last tag has a real duplicate.
    let mut seq = b"<r>".to_vec();
    for n in [1usize, 40, 300, 2, 700, 5, 129, 0, 64] {
        start_tag(n, 5, Dup::None, &mut seq);
        seq.extend_from_slice(b"</d>");
    }
    start_tag(200, 5, Dup::Middle, &mut seq);
    seq.extend_from_slice(b"</d></r>");
    v.push(("grow and shrink".into(), seq));
    // Nested: the same names on parent and child are not duplicates.
    let mut nested = Vec::new();
    start_tag(150, 4, Dup::None, &mut nested);
    start_tag(150, 4, Dup::None, &mut nested);
    nested.extend_from_slice(b"</d></d>");
    v.push(("nested".into(), nested));
    v
}

fn option_sets() -> Vec<ParseOptions> {
    let mut sets = option_matrix();
    let b = ParseOptions::default();
    sets.push(ParseOptions {
        ns_separator: Some(b'|'),
        ns_triplets: true,
        chunk_size: 64,
        ..b.clone()
    });
    sets.push(ParseOptions {
        ns_separator: Some(b'|'),
        buffer_api: true,
        chunk_size: 7,
        ..b
    });
    sets
}

#[test]
fn ns_atts_growth_matches_c() {
    let docs = documents();
    let sets = option_sets();
    let mut compared = 0usize;
    let mut duplicates = 0usize;
    for (id, doc) in &docs {
        for o in &sets {
            let c = parse_trace(doc, o).expect("oracle");
            let r = port_trace(doc, o);
            assert!(c == r, "{id} {o:?}: {:?}", first_difference(&c, &r));
            compared += 1;
            if o.ns_separator.is_some() && result_error_code(&c) == Some(8) {
                duplicates += 1;
            }
        }
    }
    // Every document with a duplicate fails with
    // XML_ERROR_DUPLICATE_ATTRIBUTE under each namespace option set.
    let ns_sets = sets.iter().filter(|o| o.ns_separator.is_some()).count();
    let with_dup = docs
        .iter()
        .filter(|(id, _)| id.contains("First") || id.contains("Middle") || id.contains("Last"))
        .count()
        + 1;
    assert_eq!(duplicates, with_dup * ns_sets, "{compared} compared");
    eprintln!(
        "{} documents x {} option sets = {compared} comparisons, {duplicates} duplicate errors",
        docs.len(),
        sets.len()
    );
}

/// Threshold sweep (amplification 1.0) on the largest tags: at every
/// threshold the port's trace equals C's, including where C refuses; plus
/// the threshold just below C's tracked peak, found by binary search.
#[test]
fn ns_atts_tracker_relation() {
    let mut failures = Vec::new();
    let mut c_refusals = 0usize;
    let docs: Vec<(String, Vec<u8>)> = documents()
        .into_iter()
        .filter(|(id, _)| {
            id == "grow and shrink"
                || id.starts_with("n=1025 k=17 ")
                || id.starts_with("n=513 k=3 ")
        })
        .collect();
    assert_eq!(docs.len(), 9);
    for (id, doc) in &docs {
        for sep in [Some(b'|'), Some(b'\n')] {
            let o = ParseOptions {
                ns_separator: sep,
                ..ParseOptions::default()
            };
            let with = |thr: u64| ParseOptions {
                limits: Limits {
                    at_amplification: Some(Factor(1.0)),
                    at_threshold: Some(thr),
                    ..Limits::NONE
                },
                ..o.clone()
            };
            // Exact parity (D-034): a refusal shows as equal traces, so C's
            // refusals are counted from its trace, and both other relations
            // are failures (as in `tracker::check_from`).
            let mut probe = |thr: u64| {
                let oo = with(thr);
                match tracker::run(doc, &oo).and_then(|r| Ok((tracker::relation(&r, &oo)?, r))) {
                    Ok((Relation::Equal, r)) => {
                        if tracker::refused(&r.c) {
                            c_refusals += 1;
                        }
                    }
                    Ok((Relation::BothNoMemory, _)) => failures.push(format!(
                        "{id} thr={thr}: C and the port both refuse but stop differently"
                    )),
                    Ok((Relation::PortStricter, _)) => failures.push(format!(
                        "{id} thr={thr}: the port refuses where C accepts"
                    )),
                    Err(e) => failures.push(format!("{id} thr={thr}: {e}")),
                }
            };
            let mut thr = 1024u64;
            while thr < 1 << 22 {
                probe(thr);
                thr = thr * 9 / 8;
            }
            let (mut lo, mut hi) = (0u64, 1u64 << 24);
            while hi - lo > 1 {
                let mid = lo + (hi - lo) / 2;
                let c = parse_trace(doc, &with(mid)).expect("oracle");
                if result_error_code(&c) == Some(NO_MEMORY) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            probe(lo);
            probe(hi);
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures, first: {}",
        failures.len(),
        failures.first().map_or("", String::as_str)
    );
    eprintln!("tracker relation: C refused {c_refusals} times, the port each time");
    assert!(c_refusals > 50, "C refused only {c_refusals} times");
}

/// The duplicate check stays near-linear in the number of prefixed
/// attributes, like C's hash table (the port's first version compared each
/// name with every earlier one: 1.5 s for an 80,000 attribute, 1 MB start
/// tag, against 47 ms in C). Measured against C on the same machine: going
/// from 10,000 to 160,000 attributes (16 times) may cost the port at most
/// three times C's growth factor; C and the fixed port both grow ~25 times,
/// a quadratic check ~200 times. Relative to C, so it holds on slow, loaded
/// and instrumented builds; best of three runs each.
///
/// The measurement is reported on success too: one `ns-scaling` line on
/// stderr (visible with `--nocapture`) and, when `NS_SCALING_REPORT` names
/// a file, the same values as JSON (C and port times, growth factors,
/// ratio, input sizes, runner OS / architecture / CPU / parallelism, build
/// profile).
#[test]
fn ns_duplicate_check_scales_like_c() {
    let doc = |n: usize| {
        let mut d = b"<d xmlns:p='urn:p'".to_vec();
        for i in 0..n {
            d.extend_from_slice(format!(" p:a{i}='v'").as_bytes());
        }
        d.extend_from_slice(b"/>");
        d
    };
    let o = ParseOptions {
        ns_separator: Some(b'|'),
        ..ParseOptions::default()
    };
    const RUNS: usize = 3;
    // The port may grow at most this many times C's growth factor; the
    // same constant is reported, so the report cannot drift from the check.
    const LIMIT_FACTOR: f64 = 3.0;
    let best = |d: &[u8], parse: &dyn Fn(&[u8], &ParseOptions) -> Vec<u8>| {
        (0..RUNS)
            .map(|_| {
                let t = std::time::Instant::now();
                let r = parse(d, &o);
                let e = t.elapsed();
                assert!(r.windows(10).any(|w| w == b"result ok "), "parse failed");
                e
            })
            .min()
            .unwrap_or_default()
            .as_secs_f64()
            .max(1e-6)
    };
    let (n_small, n_large) = (10_000usize, 160_000usize);
    let (small, large) = (doc(n_small), doc(n_large));
    let c_trace = |d: &[u8], o: &ParseOptions| parse_trace(d, o).expect("oracle");
    let (port_small, port_large) = (best(&small, &port_trace), best(&large, &port_trace));
    let (c_small, c_large) = (best(&small, &c_trace), best(&large, &c_trace));
    let port = port_large / port_small;
    let c = c_large / c_small;
    let limit = LIMIT_FACTOR * c;
    let passed = port < limit;
    let runner = runner_info();
    eprintln!(
        "ns-scaling: attributes {n_small} -> {n_large} ({} -> {} bytes); \
         port {:.3} ms -> {:.3} ms (x{port:.1}); C {:.3} ms -> {:.3} ms (x{c:.1}); \
         relative {:.2} (limit {LIMIT_FACTOR:.2}); runner {runner}",
        small.len(),
        large.len(),
        port_small * 1e3,
        port_large * 1e3,
        c_small * 1e3,
        c_large * 1e3,
        port / c
    );
    // A failed report write must not hide a failed measurement: the write
    // result is kept and reported together with the assertion below.
    let mut write_error = None;
    if let Some(path) = std::env::var_os("NS_SCALING_REPORT") {
        let report = serde_json::json!({
            "test": "ns_atts_diff::ns_duplicate_check_scales_like_c",
            "attributes": { "small": n_small, "large": n_large },
            "input_bytes": { "small": small.len(), "large": large.len() },
            "runs_per_measurement": RUNS,
            "statistic": "minimum wall-clock seconds of the runs",
            "port_seconds": { "small": port_small, "large": port_large },
            "c_seconds": { "small": c_small, "large": c_large },
            "growth": { "port": port, "c": c, "port_over_c": port / c, "limit_port_over_c": LIMIT_FACTOR },
            "passed": passed,
            "runner": {
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "family": std::env::consts::FAMILY,
                "cpu": cpu_model(),
                "available_parallelism": std::thread::available_parallelism().map_or(0, usize::from),
                "debug_assertions": cfg!(debug_assertions),
            },
        });
        let text = serde_json::to_string_pretty(&report).expect("json");
        if let Err(e) = std::fs::write(&path, text) {
            write_error = Some(format!(
                "write NS_SCALING_REPORT {}: {e}",
                std::path::Path::new(&path).display()
            ));
        }
    }
    let note = write_error
        .as_ref()
        .map_or(String::new(), |e| format!(" (also: {e})"));
    assert!(
        passed,
        "port grows x{port:.1}, C x{c:.1}: quadratic duplicate check?{note}"
    );
    if let Some(e) = write_error {
        panic!("{e}");
    }
}

/// The CPU model as the OS reports it (`/proc/cpuinfo` on Linux,
/// `sysctl -n machdep.cpu.brand_string` on macOS, `PROCESSOR_IDENTIFIER`
/// on Windows); "unknown" otherwise. Informational only.
fn cpu_model() -> String {
    let linux = || {
        let info = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        let field = |name: &str| {
            info.lines()
                .filter_map(|l| l.split_once(':'))
                .find(|(k, _)| k.trim() == name)
                .map(|(_, v)| v.trim().to_string())
        };
        // x86 reports a model name; arm64 only implementer and part ids.
        field("model name").or_else(|| {
            Some(format!(
                "implementer {} part {}",
                field("CPU implementer")?,
                field("CPU part")?
            ))
        })
    };
    let macos = || {
        let out = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()?;
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
    };
    let windows = || std::env::var("PROCESSOR_IDENTIFIER").ok();
    match std::env::consts::OS {
        "linux" => linux(),
        "macos" => macos(),
        "windows" => windows(),
        _ => None,
    }
    .unwrap_or_else(|| "unknown".to_string())
}

/// `os/arch, cpu, N threads, debug|release` for the stderr line.
fn runner_info() -> String {
    format!(
        "{}/{}, {}, {} threads, {}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        cpu_model(),
        std::thread::available_parallelism().map_or(0, usize::from),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    )
}
