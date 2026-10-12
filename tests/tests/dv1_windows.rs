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
//! Every case asserts the security relation (the port never accepts what C
//! refuses) and the exact outcome on each side at the current stage.

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

/// Runs `doc` on both sides and checks the expected outcomes.
fn check(name: &str, doc: &[u8], c_expected: bool, port_expected: bool) {
    let out = compare(name, doc, &base(), WINDOW);
    let c = accepted("C", result_error_code(&out.c), name);
    let p = accepted("port", result_error_code(&out.port), name);
    println!(
        "WINDOW {name:<16} doc {:>7} B | C {} end {:>9} | port {} end {:>9} peak {:>9}",
        doc.len(),
        if c { "accepts" } else { "refuses" },
        out.c_tracker.end,
        if p { "accepts" } else { "refuses" },
        out.port_tracker.end,
        out.port_tracker.peak
    );
    assert!(c || !p, "{name}: the port accepts a document C refuses");
    assert_eq!(
        (c, p),
        (c_expected, port_expected),
        "{name}: (C, port) accepted"
    );
    if c && p {
        assert!(out.equal(), "{name}: traces differ");
    }
}

/// R1: C refuses at both ends of the former reverse window; the port must
/// refuse too.
#[test]
fn r1_attribute_defaults() {
    check("R1 pad 187657", &window_doc(187_657, 10_000), false, false);
    check("R1 pad 188000", &window_doc(188_000, 10_000), false, false);
    check("R1 pad 188322", &window_doc(188_322, 10_000), false, false);
    // Above C's edge the port's creation charge still refuses (the W1
    // window shifted by the defaults' bytes) until pad 188,457.
    check("R1 pad 188323", &window_doc(188_323, 10_000), true, false);
    check("R1 pad 188456", &window_doc(188_456, 10_000), true, false);
    check("R1 pad 188457", &window_doc(188_457, 10_000), true, true);
}

/// W1: the creation-charge window (DV-1).
#[test]
fn w1_creation_charge() {
    check("W1 pad 330745", &window_doc(330_745, 0), false, false);
    check("W1 pad 330746", &window_doc(330_746, 0), true, false);
    check("W1 pad 330878", &window_doc(330_878, 0), true, false);
    check("W1 pad 330879", &window_doc(330_879, 0), true, true);
}

/// W2: the pool-slack window (DV-1).
#[test]
fn w2_pool_slack() {
    check("W2 tail 362", &slack_doc(84_947, 362), true, true);
    check("W2 tail 363", &slack_doc(84_947, 363), true, false);
    check("W2 tail 366", &slack_doc(84_947, 366), true, false);
    check("W2 tail 367", &slack_doc(84_947, 367), false, false);
}
