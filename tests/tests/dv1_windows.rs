//! Permanent regressions for the default-limit windows of the allocation
//! tracker (docs/DIVERGENCES.md DV-1 and R1, decision D-033): documents of
//! 340-690 KB whose ~33.5 MB attribute expansion puts both trackers at the
//! 64 MiB activation threshold with an amplification near 100, so a few
//! bytes of difference between the port's and C's counts decide the
//! outcome.
//!
//! - **W1** (creation charge): `pad` spaces before a DTD with a 395-character
//!   entity referenced 85,000 times in one attribute. C accepts from pad
//!   330,746; the port's creation charge (16,384 against C's 3,040) makes
//!   it refuse up to pad 330,878.
//! - **W2** (pool slack): 84,947 references and `tail` literal characters,
//!   no padding. C fills its 32 MiB pool block to the last byte; the port's
//!   `POOL_SLACK` of 4 makes it double the block to 64 MiB for tails
//!   363-366.
//! - **R1** (reverse violation, fixed by D-033): the W1 document with 10,000
//!   attribute defaults on an unused element type. With `BINDING` and
//!   `NAME_AND_DEFAULT_ATTRIBUTE` 8 bytes below C's `sizeof`, the port
//!   accepted documents of 686,993-687,658 bytes that C refuses with
//!   `XML_ERROR_NO_MEMORY`.
//!
//! C's boundary depends on the platform (its creation allocations differ:
//! 3,040 bytes on 64-bit Linux and macOS, 32 fewer on Windows x64), so the
//! tests find both boundaries by bisection on each platform and assert:
//! the port never accepts a document C refuses (on both sides of each
//! boundary), and the width of the window where only C accepts equals the
//! margin between the two trackers at the deciding allocation divided by
//! the maximum amplification (100), within one document. The boundaries
//! and widths are printed for the per-platform report.

use std::time::Duration;

use expat_difftest::bounded::{Bounds, CountingAlloc, compare};
use expat_difftest::trace::result_error_code;
use expat_difftest::tracker::NO_MEMORY;
use expat_oracle::{ParamEntityParsing, ParseOptions};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

/// 1 MiB input (the largest document is ~690 KB), 120 s, 384 MiB heap per
/// side (each side holds one 32 MiB pool block and refuses the 64 MiB one).
const WINDOW: Bounds = Bounds {
    max_input: 1 << 20,
    timeout: Duration::from_secs(120),
    port_heap_cap: 384 << 20,
    c_heap_cap: 384 << 20,
};

fn base() -> ParseOptions {
    ParseOptions {
        param_entity_parsing: ParamEntityParsing::Always,
        ..ParseOptions::default()
    }
}

/// `pad` spaces, a DTD with a 395-character entity `ee` and `ndef`
/// attribute defaults on the unused element type `z`, and a start tag
/// whose attribute references `ee` 85,000 times.
fn window_doc(pad: usize, ndef: usize) -> Vec<u8> {
    let mut d = vec![b' '; pad];
    d.extend_from_slice(format!("<!DOCTYPE r [<!ENTITY ee '{}'>", "x".repeat(395)).as_bytes());
    if ndef > 0 {
        d.extend_from_slice(b"<!ATTLIST z");
        for j in 0..ndef {
            d.extend_from_slice(format!(" a{j} CDATA 'v'").as_bytes());
        }
        d.push(b'>');
    }
    d.extend_from_slice(b"]><r v='");
    for _ in 0..85_000 {
        d.extend_from_slice(b"&ee;");
    }
    d.extend_from_slice(b"'/>");
    d
}

/// An attribute of `refs` x 395 expanded characters plus `tail` literal
/// ones: 84,947 references fill C's 32 MiB block up to 33,554,431
/// characters and the NUL with `tail` 366.
fn slack_doc(refs: usize, tail: usize) -> Vec<u8> {
    let mut d = format!("<!DOCTYPE r [<!ENTITY ee '{}'>]><r v='", "x".repeat(395)).into_bytes();
    for _ in 0..refs {
        d.extend_from_slice(b"&ee;");
    }
    d.extend(std::iter::repeat_n(b'x', tail));
    d.extend_from_slice(b"'/>");
    d
}

/// Accepted (`true`) or refused with `XML_ERROR_NO_MEMORY` (`false`);
/// any other outcome fails the test.
fn accepted(side: &str, code: Option<u32>, name: &str) -> bool {
    match code {
        Some(0) => true,
        Some(NO_MEMORY) => false,
        c => panic!("{name}: {side} ended with {c:?}"),
    }
}

/// Both sides' outcomes for one document.
struct Run {
    c: bool,
    port: bool,
    c_end: u64,
    port_end: u64,
}

fn run(name: &str, doc: &[u8]) -> Run {
    let out = compare(name, doc, &base(), WINDOW);
    let c = accepted("C", result_error_code(&out.c), name);
    let port = accepted("port", result_error_code(&out.port), name);
    assert!(c || !port, "{name}: the port accepts a document C refuses");
    if c && port {
        assert!(out.equal(), "{name}: traces differ");
    }
    Run {
        c,
        port,
        c_end: out.c_tracker.end,
        port_end: out.port_tracker.end,
    }
}

/// The smallest `x` in `lo..=hi` for which `side` of `doc(x)` has outcome
/// `!at_lo`, given that the outcome is monotone in `x` and `at_lo` at `lo`.
fn boundary(
    lo: usize,
    hi: usize,
    doc: &dyn Fn(usize) -> Vec<u8>,
    side: fn(&Run) -> bool,
    name: &str,
) -> usize {
    let at_lo = side(&run(name, &doc(lo)));
    assert_ne!(
        at_lo,
        side(&run(name, &doc(hi))),
        "{name}: no boundary in {lo}..={hi}"
    );
    let (mut a, mut b) = (lo, hi);
    while b - a > 1 {
        let m = a + (b - a) / 2;
        if side(&run(name, &doc(m))) == at_lo {
            a = m;
        } else {
            b = m;
        }
    }
    b
}

fn platform() -> String {
    format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS)
}

/// A family whose documents become acceptable as `x` grows (more direct
/// input): returns (C's first accepted x, the port's first accepted x).
/// Checks the security relation around both boundaries and that the window
/// width matches the margin between the trackers at the deciding pool
/// doubling (both sides' last event is that doubling, so the margin is the
/// difference of their end counts on a document both accept).
fn accept_window(
    name: &str,
    lo: usize,
    hi: usize,
    doc: &dyn Fn(usize) -> Vec<u8>,
) -> (usize, usize) {
    let c_from = boundary(lo, hi, doc, |r| r.c, name);
    let p_from = boundary(lo, hi, doc, |r| r.port, name);
    let below = run(name, &doc(c_from - 1));
    assert!(
        !below.c && !below.port,
        "{name}: below C's boundary both must refuse"
    );
    let both = run(name, &doc(p_from.max(c_from)));
    assert!(
        both.c && both.port,
        "{name}: above both boundaries both must accept"
    );
    let margin = both.port_end as i64 - both.c_end as i64;
    let width = p_from as i64 - c_from as i64;
    println!(
        "WINDOW {} {name}: C accepts from {c_from}, port from {p_from}; width {width}; tracker margin at the deciding doubling {margin} (C end {}, port end {})",
        platform(),
        both.c_end,
        both.port_end
    );
    assert!(width >= 0, "{name}: the port accepts below C's boundary");
    // One document more of direct input raises 100 x direct by 100 bytes.
    assert!(
        (width - margin / 100).abs() <= 1,
        "{name}: window width {width} does not match the margin {margin}"
    );
    (c_from, p_from)
}

/// R1: C refuses at both ends of the former reverse window (686,993 and
/// 687,658 bytes on 64-bit Linux); the port must refuse at least as far.
/// Before D-033 the port accepted from 66,640 bytes of margin below C.
#[test]
fn r1_attribute_defaults() {
    let (c_from, p_from) = accept_window("R1", 187_000, 190_000, &|pad| window_doc(pad, 10_000));
    assert!(p_from >= c_from);
    // The former reverse window lies wholly where C refuses.
    for pad in [187_657, 188_000, c_from - 1] {
        let r = run("R1", &window_doc(pad, 10_000));
        assert!(!r.c && !r.port, "R1 pad {pad}: both must refuse");
    }
    // Pinned at the current stage of D-033: the creation-charge window
    // (W1 shape) is still open, the reverse one is closed.
    assert!(
        p_from > c_from,
        "R1: DV-1 window closed; update the stage pin"
    );
}

/// W1: the creation-charge window (DV-1), pinned open until step 3 of
/// D-033.
#[test]
fn w1_creation_charge() {
    let (c_from, p_from) = accept_window("W1", 330_000, 332_000, &|pad| window_doc(pad, 0));
    assert!(
        p_from > c_from,
        "W1: DV-1 window closed; update the stage pin"
    );
}

/// W2: the pool-slack window (DV-1), pinned open until step 2 of D-033.
/// The tail grows the expanded value, so documents become refused as it
/// grows: C accepts up to the tail that fills its 32 MiB block exactly.
#[test]
fn w2_pool_slack() {
    let doc = |tail| slack_doc(84_947, tail);
    let c_refuses_from = boundary(300, 420, &doc, |r| r.c, "W2");
    let p_refuses_from = boundary(300, 420, &doc, |r| r.port, "W2");
    let last = run("W2", &doc(c_refuses_from - 1));
    println!(
        "WINDOW {} W2: C refuses from tail {c_refuses_from}, port from tail {p_refuses_from}; width {}; C end at tail {} {}, port {}",
        platform(),
        c_refuses_from - p_refuses_from,
        c_refuses_from - 1,
        last.c_end,
        last.port_end
    );
    assert!(
        p_refuses_from <= c_refuses_from,
        "W2: the port accepts what C refuses"
    );
    // C's last accepted tail fills the block exactly; the port's 4-character
    // slack doubles the block 4 tails earlier.
    assert_eq!(
        c_refuses_from - p_refuses_from,
        4,
        "W2: DV-1 slack window changed; update the stage pin"
    );
}
