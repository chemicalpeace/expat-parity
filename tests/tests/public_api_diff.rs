//! Module 12, first slice (D-038): the public `expat::Parser` against the C
//! oracle.
//!
//! The oracle side is `expat_oracle::buffer_run` with `XML_Parse`
//! operations only: `XML_ParserCreate(NULL)` (default encoding, no
//! namespaces), no handlers, salt 1..=16, reparse deferral on (C's default,
//! which the public API cannot change). After every call it reports the
//! status and `XML_GetErrorCode`, and after the last call the line, column
//! and byte index. The public side makes the same calls with the same
//! pieces of the same document.
//!
//! - T2: whole documents (540 upstream vectors and 2 x 300 generated
//!   documents) fed in one call, byte by byte, and in random pieces; the
//!   result of every call up to the first error, and the code and position
//!   of that error, must equal C's.
//! - T3: calls after the end (after the final call, after an error,
//!   including deferred calls after an error that C answers with
//!   `XML_STATUS_OK`), on fixed cases and random call sequences; the result
//!   and code of every call, and the position of the first error, must
//!   equal C's.

use expat::{ErrorCode, Parser};
use expat_difftest::docgen::{Rng, document, dtd_document};
use expat_difftest::vectors::upstream;
use expat_oracle::BufferOp;

/// The salt `oracle_buffer_run` sets.
const SALT: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

/// The result of one call: status (0 error, 1 ok) and C error code.
type Call = (u8, u32);

/// C's results: one entry per call, then line, column and byte index after
/// the last call.
fn c_run(doc: &[u8], ops: &[BufferOp]) -> (Vec<Call>, (u64, u64, i64)) {
    let out = expat_oracle::buffer_run(doc, ops, false, true);
    let mut calls = Vec::new();
    let mut end = None;
    for line in out.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        if f[0] == "end" {
            end = Some((
                f[1].parse().expect("line"),
                f[2].parse().expect("column"),
                f[3].parse().expect("byte index"),
            ));
        } else {
            calls.push((f[0].parse().expect("status"), f[1].parse().expect("code")));
        }
    }
    (calls, end.expect("end line"))
}

/// The public API's results for the same calls (all of them, or up to the
/// first error); the position is that of the last error returned, if the
/// last call made failed.
fn port_run(
    doc: &[u8],
    ops: &[BufferOp],
    stop_at_first_error: bool,
) -> (Vec<Call>, Option<(u64, u64, i64)>) {
    let mut p = Parser::new(SALT);
    let mut calls = Vec::new();
    let mut last_err = None;
    let mut cur = 0usize;
    for op in ops {
        let BufferOp::Parse { n, is_final } = *op else {
            panic!("only XML_Parse operations are compared");
        };
        let k = usize::try_from(n).unwrap_or(0).min(doc.len() - cur);
        let r = p.parse(&doc[cur..cur + k], is_final);
        cur += k;
        match r {
            Ok(()) => {
                calls.push((1, 0));
                last_err = None;
            }
            Err(e) => {
                assert_ne!(e.code().to_c(), 0, "an error never has code NONE");
                calls.push((0, e.code().to_c()));
                last_err = Some((e.line(), e.column(), e.byte_index()));
                if stop_at_first_error {
                    break;
                }
            }
        }
    }
    (calls, last_err)
}

/// Compares the calls of `ops` up to and including the first error, and
/// that error's position. Returns whether an error occurred.
fn check_until_first_error(id: &str, doc: &[u8], ops: &[BufferOp]) -> bool {
    let (c_calls, _) = c_run(doc, ops);
    let (p_calls, p_pos) = port_run(doc, ops, true);
    let first = c_calls.iter().position(|&(st, _)| st == 0);
    let n = first.map_or(c_calls.len(), |i| i + 1);
    assert_eq!(
        p_calls,
        c_calls[..n],
        "{id}: results differ (left port, right C); ops {ops:?}"
    );
    if let Some(i) = first {
        // C's position right after the failing call.
        let (_, c_pos) = c_run(doc, &ops[..=i]);
        assert_eq!(
            p_pos,
            Some(c_pos),
            "{id}: error position differs (left port, right C); ops {ops:?}"
        );
        true
    } else {
        false
    }
}

/// Compares the result and code of every call of `ops`, and the position
/// of the first error. Returns the C results.
///
/// Only the first error's position is compared: the public API reads the
/// position at every error, and in C (as in the port) each read after a
/// further failing call advances the column again from `m_positionPtr`
/// (which `XML_Parse` resets to the start of the call) to `m_eventPtr`,
/// while `oracle_buffer_run` reads it only once, after the last call. The
/// two sides read the position at the same points only up to the first
/// error.
fn check_all(id: &str, doc: &[u8], ops: &[BufferOp]) -> Vec<Call> {
    let (c_calls, _) = c_run(doc, ops);
    let (p_calls, _) = port_run(doc, ops, false);
    assert_eq!(
        p_calls, c_calls,
        "{id}: results differ (left port, right C); ops {ops:?}"
    );
    for &(st, code) in &c_calls {
        assert!(st == 0 || code == 0, "{id}: C returned OK with code {code}");
    }
    if let Some(i) = c_calls.iter().position(|&(st, _)| st == 0) {
        let (_, c_pos) = c_run(doc, &ops[..=i]);
        let (_, p_pos) = port_run(doc, &ops[..=i], false);
        assert_eq!(
            p_pos,
            Some(c_pos),
            "{id}: first error position differs (left port, right C); ops {ops:?}"
        );
    }
    c_calls
}

fn parse(n: i32, is_final: bool) -> BufferOp {
    BufferOp::Parse { n, is_final }
}

fn one_call(doc: &[u8]) -> Vec<BufferOp> {
    vec![parse(
        i32::try_from(doc.len()).expect("small document"),
        true,
    )]
}

fn byte_by_byte(doc: &[u8]) -> Vec<BufferOp> {
    let mut ops = vec![parse(1, false); doc.len()];
    ops.push(parse(0, true));
    ops
}

/// Random pieces of `doc`; the last piece is final, or an empty final call
/// follows it.
fn random_pieces(r: &mut Rng, doc: &[u8]) -> Vec<BufferOp> {
    let mut ops = Vec::new();
    let mut left = doc.len();
    while left > 0 {
        let n = match r.below(4) {
            0 => 1 + r.below(4),
            1 => 1 + r.below(64),
            2 => 1 + r.below(1500),
            _ => left,
        }
        .min(left);
        left -= n;
        ops.push(parse(
            i32::try_from(n).expect("small piece"),
            left == 0 && r.one_in(2),
        ));
    }
    if !ops
        .last()
        .is_some_and(|o| matches!(o, BufferOp::Parse { is_final: true, .. }))
    {
        ops.push(parse(0, true));
    }
    ops
}

/// The 540 upstream vectors and 2 x 300 generated documents (alternately
/// without and with a document type declaration) from the seeds of the
/// D-037 experiment.
fn documents() -> Vec<(String, Vec<u8>)> {
    let mut docs: Vec<(String, Vec<u8>)> = upstream()
        .iter()
        .enumerate()
        .map(|(i, v)| (format!("upstream #{i}"), v.doc()))
        .collect();
    assert_eq!(docs.len(), 540);
    for seed in [0x5eed_7e47_u64, 0x5eed_0001] {
        let mut r = Rng::new(seed);
        for i in 0..300 {
            let doc = if i % 2 == 0 {
                document(&mut r)
            } else {
                dtd_document(&mut r)
            };
            docs.push((format!("seed {seed:#x} #{i}"), doc));
        }
    }
    docs
}

#[test]
fn t2_whole_documents_one_call() {
    let (mut n, mut errors) = (0, 0);
    for (id, doc) in documents() {
        errors += usize::from(check_until_first_error(&id, &doc, &one_call(&doc)));
        n += 1;
    }
    eprintln!("one call: {n} documents, {errors} ended in an error");
    assert!(errors > 0 && errors < n, "both outcomes are covered");
}

#[test]
fn t2_whole_documents_byte_by_byte() {
    let (mut n, mut errors) = (0, 0);
    for (id, doc) in documents() {
        errors += usize::from(check_until_first_error(&id, &doc, &byte_by_byte(&doc)));
        n += 1;
    }
    eprintln!("byte by byte: {n} documents, {errors} ended in an error");
    assert!(errors > 0 && errors < n, "both outcomes are covered");
}

#[test]
fn t2_whole_documents_random_pieces() {
    let mut r = Rng::new(0x7075_626c_6963_0002);
    let (mut n, mut errors) = (0, 0);
    for (id, doc) in documents() {
        for _ in 0..2 {
            let ops = random_pieces(&mut r, &doc);
            errors += usize::from(check_until_first_error(&id, &doc, &ops));
            n += 1;
        }
    }
    eprintln!("random pieces: {n} runs, {errors} ended in an error");
    assert!(errors > 0 && errors < n, "both outcomes are covered");
}

#[test]
fn t3_call_after_the_final_call() {
    let doc = b"<doc>text</doc>";
    let ops = [parse(15, true), parse(0, true), parse(0, false)];
    let calls = check_all("after final", doc, &ops);
    let finished = ErrorCode::Finished.to_c();
    assert_eq!(calls, [(1, 0), (0, finished), (0, finished)]);
}

#[test]
fn t3_final_call_after_an_error() {
    let doc = b"<a></b>";
    let ops = [parse(7, true), parse(0, true), parse(0, true)];
    let calls = check_all("after error", doc, &ops);
    let mismatch = ErrorCode::TagMismatch.to_c();
    assert_eq!(calls, [(0, mismatch), (0, mismatch), (0, mismatch)]);
}

/// After an error in a non-final call, later calls that the
/// reparse-deferral heuristic does not scan reset the error code and return
/// `XML_STATUS_OK` in C (D-023), even the final one here; the public API
/// must do the same. The case (UTF-16LE with a BOM, junk after the document
/// element) was found by a search over generated documents.
#[test]
fn t3_deferred_call_after_an_error_is_ok() {
    let text = "\n<p:e id = '&lt;&gt;&quot;&apos;'>]]<![CDATA[\r\n]]>\t</p:e>junk\r\n";
    let mut doc = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        doc.extend_from_slice(&unit.to_le_bytes());
    }
    let ops = [
        parse(10, false),
        parse(20, false),
        parse(98, false),
        parse(0, false),
        parse(0, true),
    ];
    let calls = check_all("deferred after error", &doc, &ops);
    let junk = ErrorCode::JunkAfterDocElement.to_c();
    assert_eq!(calls, [(1, 0), (1, 0), (0, junk), (1, 0), (1, 0)]);
}

#[test]
fn t3_random_call_sequences() {
    let mut r = Rng::new(0x7075_626c_6963_0003);
    let (mut runs, mut after_end, mut ok_after_error) = (0, 0, 0);
    for (id, doc) in &documents() {
        let mut ops = Vec::new();
        let mut left = doc.len();
        let steps = 1 + r.below(10);
        for i in 0..steps {
            let last = i + 1 == steps;
            let n = if last {
                left
            } else {
                match r.below(4) {
                    0 => r.below(3),
                    1 => r.below(40),
                    2 => r.below(1200),
                    _ => left / 2,
                }
                .min(left)
            };
            left -= n;
            ops.push(parse(
                i32::try_from(n).expect("small"),
                last || r.one_in(12),
            ));
        }
        // Calls after the end: final and non-final, empty and not.
        for _ in 0..r.below(4) {
            ops.push(parse(
                i32::try_from(r.below(3)).expect("small"),
                r.one_in(2),
            ));
            after_end += 1;
        }
        let calls = check_all(id, doc, &ops);
        if calls.windows(2).any(|w| w[0].0 == 0 && w[1].0 == 1) {
            ok_after_error += 1;
        }
        runs += 1;
    }
    eprintln!(
        "random sequences: {runs} runs, {after_end} calls after the end, {ok_after_error} runs with OK after an error"
    );
    assert!(
        after_end > 0 && ok_after_error > 0,
        "both situations are covered"
    );
}
