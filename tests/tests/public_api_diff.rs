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
//!   result and code of every call, and the position of every error, must
//!   equal C's (the same check as T3).
//! - T3: calls after the end (after the final call, after an error,
//!   including deferred calls after an error that C answers with
//!   `XML_STATUS_OK`), on fixed cases and random call sequences; the result
//!   and code of every call, and the position of every error, must equal
//!   C's. The public API reads the position whenever it returns an error,
//!   and each read moves C's position on (`m_positionPtr`), so the C side
//!   is `buffer_run_with_error_positions`, which reads it after every
//!   failed call in the same way; the oracle tests below check that this
//!   mode is the C library's behaviour and leaves the default mode as it
//!   was.
//! - Error texts (D-039): `ErrorCode::description` of every code equals
//!   the bytes of the vendored `XML_ErrorString`, which returns `NULL` for
//!   `XML_ERROR_NONE` and for values outside the enum; `Display for Error`
//!   writes C's line and column.

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

/// A call's result with the position read after it if it failed.
type CallPos = (Call, Option<(u64, u64, i64)>);

/// C's results with the position read after every failed call
/// (`buffer_run_with_error_positions`), and the `end` line.
fn c_run_error_positions(doc: &[u8], ops: &[BufferOp]) -> (Vec<CallPos>, (u64, u64, i64)) {
    let out = expat_oracle::buffer_run_with_error_positions(doc, ops, false, true);
    let mut calls: Vec<CallPos> = Vec::new();
    let mut end = None;
    for line in out.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        let pos = || -> (u64, u64, i64) {
            (
                f[1].parse().expect("line"),
                f[2].parse().expect("column"),
                f[3].parse().expect("byte index"),
            )
        };
        match f[0] {
            "end" => end = Some(pos()),
            "pos" => {
                let last = calls.last_mut().expect("pos follows a call");
                assert_eq!(last.0.0, 0, "pos only after a failed call");
                assert!(last.1.replace(pos()).is_none(), "one pos per call");
            }
            _ => calls.push((
                (f[0].parse().expect("status"), f[1].parse().expect("code")),
                None,
            )),
        }
    }
    for (call, pos) in &calls {
        assert_eq!(call.0 == 0, pos.is_some(), "every failed call has a pos");
    }
    (calls, end.expect("end line"))
}

/// The public API's results for every call, with the position of each
/// error.
fn port_run_error_positions(doc: &[u8], ops: &[BufferOp]) -> Vec<CallPos> {
    let mut p = Parser::new(SALT);
    let mut cur = 0usize;
    let mut calls = Vec::new();
    for op in ops {
        let BufferOp::Parse { n, is_final } = *op else {
            panic!("only XML_Parse operations are compared");
        };
        let k = usize::try_from(n).unwrap_or(0).min(doc.len() - cur);
        let r = p.parse(&doc[cur..cur + k], is_final);
        cur += k;
        calls.push(match r {
            Ok(()) => ((1, 0), None),
            Err(e) => (
                (0, e.code().to_c()),
                Some((e.line(), e.column(), e.byte_index())),
            ),
        });
    }
    calls
}

/// The public API's results for the same calls; the position is that of the last error returned, if the
/// last call made failed.
fn port_run(doc: &[u8], ops: &[BufferOp]) -> (Vec<Call>, Option<(u64, u64, i64)>) {
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
            }
        }
    }
    (calls, last_err)
}

/// Compares the result and code of every call of `ops` and the position of
/// every error, the first one also against a C run that reads no position
/// before it. Returns the C results.
fn check_all(id: &str, doc: &[u8], ops: &[BufferOp]) -> Vec<Call> {
    let (c_pos_calls, _) = c_run_error_positions(doc, ops);
    let p_pos_calls = port_run_error_positions(doc, ops);
    assert_eq!(
        p_pos_calls, c_pos_calls,
        "{id}: results or error positions differ (left port, right C); ops {ops:?}"
    );
    let (c_calls, _) = c_run(doc, ops);
    let (p_calls, _) = port_run(doc, ops);
    assert_eq!(
        p_calls, c_calls,
        "{id}: results differ (left port, right C); ops {ops:?}"
    );
    for &(st, code) in &c_calls {
        assert!(st == 0 || code == 0, "{id}: C returned OK with code {code}");
    }
    if let Some(i) = c_calls.iter().position(|&(st, _)| st == 0) {
        let (_, c_pos) = c_run(doc, &ops[..=i]);
        let (_, p_pos) = port_run(doc, &ops[..=i]);
        assert_eq!(
            p_pos,
            Some(c_pos),
            "{id}: first error position differs (left port, right C); ops {ops:?}"
        );
    }
    c_calls
}

/// [`check_all`] for a T2 run: returns the number of failed calls, whose
/// positions were all compared with C.
fn check_t2(id: &str, doc: &[u8], ops: &[BufferOp]) -> usize {
    check_all(id, doc, ops).iter().filter(|c| c.0 == 0).count()
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
    let (mut n, mut errors, mut positions) = (0, 0, 0);
    for (id, doc) in documents() {
        let failed = check_t2(&id, &doc, &one_call(&doc));
        errors += usize::from(failed > 0);
        positions += failed;
        n += 1;
    }
    eprintln!(
        "one call: {n} documents, {errors} with an error, {positions} error positions compared"
    );
    assert!(errors > 0 && errors < n, "both outcomes are covered");
}

#[test]
fn t2_whole_documents_byte_by_byte() {
    let (mut n, mut errors, mut positions) = (0, 0, 0);
    for (id, doc) in documents() {
        let failed = check_t2(&id, &doc, &byte_by_byte(&doc));
        errors += usize::from(failed > 0);
        positions += failed;
        n += 1;
    }
    eprintln!(
        "byte by byte: {n} documents, {errors} with an error, {positions} error positions compared"
    );
    assert!(errors > 0 && errors < n, "both outcomes are covered");
}

#[test]
fn t2_whole_documents_random_pieces() {
    let mut r = Rng::new(0x7075_626c_6963_0002);
    let (mut n, mut errors, mut positions) = (0, 0, 0);
    for (id, doc) in documents() {
        for _ in 0..2 {
            let ops = random_pieces(&mut r, &doc);
            let failed = check_t2(&id, &doc, &ops);
            errors += usize::from(failed > 0);
            positions += failed;
            n += 1;
        }
    }
    eprintln!(
        "random pieces: {n} runs, {errors} with an error, {positions} error positions compared"
    );
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
    let positions: Vec<_> = port_run_error_positions(doc, &ops)
        .into_iter()
        .map(|(_, pos)| pos)
        .collect();
    assert_eq!(
        positions,
        [Some((1, 5, 5)), Some((1, 10, 5)), Some((1, 15, 5))],
        "the column read after each repeated error advances as in C"
    );
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

/// The oracle's error-position mode against itself and against the C
/// library's documented behaviour, so the expected values of the tests
/// above are C's and not the port's.
#[test]
fn oracle_error_positions_mode() {
    // 1. Repeated reads after repeated errors: `XML_Parse` resets
    //    `m_positionPtr` to the start of each call, and each read advances
    //    the column from there to `m_eventPtr` again (xmlparse.c,
    //    `XML_GetCurrentColumnNumber`), so the column grows by the error's
    //    column at every read while the line and byte index stay.
    let doc = b"<a></b>";
    let ops = [parse(7, true), parse(0, true), parse(0, true)];
    let (calls, end) = c_run_error_positions(doc, &ops);
    let mismatch = ErrorCode::TagMismatch.to_c();
    assert_eq!(
        calls,
        [
            ((0, mismatch), Some((1, 5, 5))),
            ((0, mismatch), Some((1, 10, 5))),
            ((0, mismatch), Some((1, 15, 5))),
        ]
    );
    // The `end` read is one more read without a call in between: it moves
    // nothing, because `m_positionPtr` already equals `m_eventPtr`.
    assert_eq!(end, (1, 15, 5));
    // Without per-call reads, the single `end` read sees one advance.
    assert_eq!(c_run(doc, &ops).1, (1, 5, 5));

    // 2. On many runs: the mode only adds `pos` lines and changes nothing
    //    else a read does not change. Every per-call state line is the
    //    default mode's up to the first failed call; after it, only the
    //    `m_positionPtr` field (the tenth) may differ, since a read moves
    //    it. With no failed call the outputs are identical. The first
    //    `pos` equals the default mode's `end` after a run cut at that
    //    call.
    let mut r = Rng::new(0x7075_626c_6963_0004);
    let (mut runs, mut with_errors, mut multi_errors) = (0, 0, 0);
    for (id, doc) in documents() {
        let mut ops = random_pieces(&mut r, &doc);
        for _ in 0..r.below(4) {
            ops.push(parse(
                i32::try_from(r.below(3)).expect("small"),
                r.one_in(2),
            ));
        }
        let plain = expat_oracle::buffer_run(&doc, &ops, false, true);
        let with = expat_oracle::buffer_run_with_error_positions(&doc, &ops, false, true);
        let strip = |t: &str| -> Vec<String> {
            t.lines()
                .filter(|l| !l.starts_with("pos ") && !l.starts_with("end "))
                .map(str::to_owned)
                .collect()
        };
        let (calls, _) = c_run_error_positions(&doc, &ops);
        let first_error = calls.iter().position(|(c, _)| c.0 == 0);
        let (with_lines, plain_lines) = (strip(&with), strip(&plain));
        assert_eq!(with_lines.len(), plain_lines.len());
        for (k, (w, p)) in with_lines.iter().zip(&plain_lines).enumerate() {
            if first_error.is_none_or(|i| k <= i) {
                assert_eq!(w, p, "{id}: state of call {k} differs");
            } else {
                let without_position_ptr = |l: &str| -> Vec<String> {
                    l.split(' ')
                        .enumerate()
                        .filter(|&(j, _)| j != 9)
                        .map(|(_, f)| f.to_owned())
                        .collect()
                };
                assert_eq!(
                    without_position_ptr(w),
                    without_position_ptr(p),
                    "{id}: state of call {k} differs beyond m_positionPtr"
                );
            }
        }
        let errors = calls.iter().filter(|(c, _)| c.0 == 0).count();
        if errors == 0 {
            assert_eq!(with, plain, "{id}: no failed call, same output");
        } else {
            with_errors += 1;
            multi_errors += usize::from(errors > 1);
            let i = calls
                .iter()
                .position(|(c, _)| c.0 == 0)
                .expect("a failed call");
            assert_eq!(
                calls[i].1,
                Some(c_run(&doc, &ops[..=i]).1),
                "{id}: first pos differs from the default mode"
            );
        }
        runs += 1;
    }
    eprintln!("oracle mode: {runs} runs, {with_errors} with an error, {multi_errors} with several");
    assert!(with_errors > 0 && multi_errors > 0 && with_errors < runs);
}

#[test]
fn t3_random_call_sequences() {
    let mut r = Rng::new(0x7075_626c_6963_0003);
    let (mut runs, mut after_end, mut ok_after_error, mut error_calls) = (0, 0, 0, 0);
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
        error_calls += calls.iter().filter(|c| c.0 == 0).count();
        if calls.windows(2).any(|w| w[0].0 == 0 && w[1].0 == 1) {
            ok_after_error += 1;
        }
        runs += 1;
    }
    eprintln!(
        "random sequences: {runs} runs, {after_end} calls after the end, {ok_after_error} runs with OK after an error, {error_calls} error positions compared"
    );
    assert!(
        after_end > 0 && ok_after_error > 0,
        "both situations are covered"
    );
}

/// Every public error code, written out because `ErrorCode` is
/// `#[non_exhaustive]`; `error_texts_match_xml_error_string` checks that it
/// holds each C value 1..=44 exactly once.
const ALL_CODES: [ErrorCode; 44] = [
    ErrorCode::NoMemory,
    ErrorCode::Syntax,
    ErrorCode::NoElements,
    ErrorCode::InvalidToken,
    ErrorCode::UnclosedToken,
    ErrorCode::PartialChar,
    ErrorCode::TagMismatch,
    ErrorCode::DuplicateAttribute,
    ErrorCode::JunkAfterDocElement,
    ErrorCode::ParamEntityRef,
    ErrorCode::UndefinedEntity,
    ErrorCode::RecursiveEntityRef,
    ErrorCode::AsyncEntity,
    ErrorCode::BadCharRef,
    ErrorCode::BinaryEntityRef,
    ErrorCode::AttributeExternalEntityRef,
    ErrorCode::MisplacedXmlPi,
    ErrorCode::UnknownEncoding,
    ErrorCode::IncorrectEncoding,
    ErrorCode::UnclosedCdataSection,
    ErrorCode::ExternalEntityHandling,
    ErrorCode::NotStandalone,
    ErrorCode::UnexpectedState,
    ErrorCode::EntityDeclaredInPe,
    ErrorCode::FeatureRequiresXmlDtd,
    ErrorCode::CantChangeFeatureOnceParsing,
    ErrorCode::UnboundPrefix,
    ErrorCode::UndeclaringPrefix,
    ErrorCode::IncompletePe,
    ErrorCode::XmlDecl,
    ErrorCode::TextDecl,
    ErrorCode::Publicid,
    ErrorCode::Suspended,
    ErrorCode::NotSuspended,
    ErrorCode::Aborted,
    ErrorCode::Finished,
    ErrorCode::SuspendPe,
    ErrorCode::ReservedPrefixXml,
    ErrorCode::ReservedPrefixXmlns,
    ErrorCode::ReservedNamespaceUri,
    ErrorCode::InvalidArgument,
    ErrorCode::NoBuffer,
    ErrorCode::AmplificationLimitBreach,
    ErrorCode::NotStarted,
];

#[test]
fn error_texts_match_xml_error_string() {
    let mut values: Vec<u32> = ALL_CODES.iter().map(|c| c.to_c()).collect();
    values.sort_unstable();
    assert_eq!(
        values,
        (1..=44).collect::<Vec<u32>>(),
        "one code per C value"
    );
    for c in ALL_CODES {
        let value = i32::try_from(c.to_c()).expect("small");
        let c_text = expat_oracle::error_string(value)
            .unwrap_or_else(|| panic!("XML_ErrorString({value}) is NULL"));
        assert_eq!(
            c.description().as_bytes(),
            c_text.as_slice(),
            "{c:?} = {value}: text differs (left port, right C)"
        );
        assert_eq!(c.to_string().as_bytes(), c_text.as_slice(), "{c:?}");
    }
    // XML_ERROR_NONE and values outside the enum: C returns NULL; the
    // public API has no code for any of them (`ErrorCode` holds 1..=44
    // only, and no error is returned with XML_ERROR_NONE).
    for value in [0, -1, 45, 46, 1000, i32::MAX, i32::MIN] {
        assert_eq!(
            expat_oracle::error_string(value),
            None,
            "XML_ErrorString({value})"
        );
    }
}

#[test]
fn error_display_uses_c_line_and_column() {
    let docs: [&[u8]; 5] = [
        b"<a></b>",
        b"<a>\n</b>",
        b"<a>\r\n  <b></c>",
        b"<doc>&undefined;</doc>",
        b"<?xml version='1.0'?>\n<a x='1' x='2'/>",
    ];
    for doc in docs {
        let ops = one_call(doc);
        let (c_calls, c_pos) = c_run(doc, &ops);
        let &[(0, code)] = c_calls.as_slice() else {
            panic!("{doc:?}: expected one failed call, C gave {c_calls:?}");
        };
        let Err(e) = Parser::new(SALT).parse(doc, true) else {
            panic!("expected an error");
        };
        assert_eq!(e.code().to_c(), code);
        assert_eq!((e.line(), e.column(), e.byte_index()), c_pos);
        let text = expat_oracle::error_string(i32::try_from(code).expect("small")).expect("text");
        let expected = format!(
            "{} at line {}, column {}",
            String::from_utf8(text).expect("ASCII"),
            c_pos.0,
            c_pos.1
        );
        assert_eq!(e.to_string(), expected, "{doc:?}");
    }
}
