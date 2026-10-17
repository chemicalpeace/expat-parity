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
//! - S3 (D-040): `Parser::with_options` with encoding names (known, in
//!   both cases, empty, unknown, with a NUL byte) and namespace separators
//!   (none, URI and non-URI characters, NUL, bytes from 0x80 up), on the
//!   T2 documents and on encoding and namespace documents, in one call, in
//!   random pieces and in T3-style call sequences; the C parser is
//!   `XML_ParserCreate(encoding)` or `XML_ParserCreateNS(encoding, sep)`
//!   (`buffer_run_with`), and `Parser::new` equals `with_options` with
//!   `Options::default()`.
//! - Error texts (D-039): `ErrorCode::description` of every code equals
//!   the bytes of the vendored `XML_ErrorString`, which returns `NULL` for
//!   `XML_ERROR_NONE` and for values outside the enum; `Display for Error`
//!   writes C's line and column.

use expat::{ErrorCode, Options, Parser};
use expat_difftest::docgen::{Rng, document, dtd_document};
use expat_difftest::vectors::upstream;
use expat_oracle::{BufferConfig, BufferOp};

/// The salt `oracle_buffer_run` sets.
const SALT: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

/// The result of one call: status (0 error, 1 ok) and C error code.
type Call = (u8, u32);

/// C's results: one entry per call, then line, column and byte index after
/// the last call.
fn c_run(doc: &[u8], ops: &[BufferOp]) -> (Vec<Call>, (u64, u64, i64)) {
    c_run_cfg(None, doc, ops)
}

/// The parser options of an S3 run (D-040): `None` in the helpers is
/// `Parser::new` against `XML_ParserCreate(NULL)` (slices 1 and 2).
#[derive(Debug, Clone, Copy)]
struct Cfg<'a> {
    encoding: Option<&'a [u8]>,
    sep: Option<u8>,
}

impl Cfg<'_> {
    fn options(&self) -> Options {
        let mut o = Options::new();
        if let Some(e) = self.encoding {
            o = o.encoding(e);
        }
        if let Some(s) = self.sep {
            o = o.namespace_separator(s);
        }
        o
    }

    fn oracle(&self) -> BufferConfig<'_> {
        BufferConfig {
            encoding: self.encoding,
            ns_separator: self.sep,
            reparse_deferral: true,
        }
    }
}

/// The oracle output for `cfg` (see [`Cfg`]).
fn c_out(cfg: Option<&Cfg<'_>>, doc: &[u8], ops: &[BufferOp], error_positions: bool) -> String {
    match (cfg, error_positions) {
        (None, false) => expat_oracle::buffer_run(doc, ops, false, true),
        (None, true) => expat_oracle::buffer_run_with_error_positions(doc, ops, false, true),
        (Some(c), _) => expat_oracle::buffer_run_with(doc, ops, &c.oracle(), error_positions),
    }
}

/// The public parser for `cfg` (see [`Cfg`]).
fn port_parser(cfg: Option<&Cfg<'_>>) -> Parser {
    match cfg {
        None => Parser::new(SALT),
        Some(c) => Parser::with_options(SALT, c.options()),
    }
}

/// [`c_run`] with the parser of `cfg`.
fn c_run_cfg(cfg: Option<&Cfg<'_>>, doc: &[u8], ops: &[BufferOp]) -> (Vec<Call>, (u64, u64, i64)) {
    let out = c_out(cfg, doc, ops, false);
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
fn c_run_error_positions(
    cfg: Option<&Cfg<'_>>,
    doc: &[u8],
    ops: &[BufferOp],
) -> (Vec<CallPos>, (u64, u64, i64)) {
    let out = c_out(cfg, doc, ops, true);
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
fn port_run_error_positions(cfg: Option<&Cfg<'_>>, doc: &[u8], ops: &[BufferOp]) -> Vec<CallPos> {
    let mut p = port_parser(cfg);
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
fn port_run(
    cfg: Option<&Cfg<'_>>,
    doc: &[u8],
    ops: &[BufferOp],
) -> (Vec<Call>, Option<(u64, u64, i64)>) {
    let mut p = port_parser(cfg);
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
    check_all_cfg(id, None, doc, ops)
}

/// [`check_all`] with the parser of `cfg`.
fn check_all_cfg(id: &str, cfg: Option<&Cfg<'_>>, doc: &[u8], ops: &[BufferOp]) -> Vec<Call> {
    let (c_pos_calls, _) = c_run_error_positions(cfg, doc, ops);
    let p_pos_calls = port_run_error_positions(cfg, doc, ops);
    assert_eq!(
        p_pos_calls, c_pos_calls,
        "{id}: results or error positions differ (left port, right C); ops {ops:?}"
    );
    let (c_calls, _) = c_run_cfg(cfg, doc, ops);
    let (p_calls, _) = port_run(cfg, doc, ops);
    assert_eq!(
        p_calls, c_calls,
        "{id}: results differ (left port, right C); ops {ops:?}"
    );
    for &(st, code) in &c_calls {
        assert!(st == 0 || code == 0, "{id}: C returned OK with code {code}");
    }
    if let Some(i) = c_calls.iter().position(|&(st, _)| st == 0) {
        let (_, c_pos) = c_run_cfg(cfg, doc, &ops[..=i]);
        let (_, p_pos) = port_run(cfg, doc, &ops[..=i]);
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
    let positions: Vec<_> = port_run_error_positions(None, doc, &ops)
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
    let (calls, end) = c_run_error_positions(None, doc, &ops);
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
        let (calls, _) = c_run_error_positions(None, &doc, &ops);
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

/// The vendored libexpat header the port is verified against (D-001).
const EXPAT_H: &str = include_str!("../../crates/expat-oracle/vendor/expat/lib/expat.h");

/// The names of `enum XML_Error` in declaration order (C value = index;
/// the enum has no explicit initialisers).
fn c_error_names() -> Vec<&'static str> {
    let start = EXPAT_H.find("enum XML_Error {").unwrap_or(0);
    let body = EXPAT_H.get(start..).unwrap_or("");
    let body = body.get(..body.find("};").unwrap_or(0)).unwrap_or("");
    body.lines()
        .filter_map(|l| {
            let l = l.trim();
            let name = l.strip_prefix("XML_ERROR_")?;
            assert!(!name.contains('='), "unexpected initialiser: {l}");
            Some(name.trim_end_matches(','))
        })
        .collect()
}

/// `NO_MEMORY` -> `NoMemory`.
fn camel(c_name: &str) -> String {
    c_name
        .split('_')
        .map(|w| {
            let mut cs = w.chars();
            cs.next().map_or_else(String::new, |f| {
                let mut s = String::from(f);
                s.extend(cs.flat_map(char::to_lowercase));
                s
            })
        })
        .collect()
}

/// Every code against the vendored `enum XML_Error` (moved here from
/// `crates/expat/src/api.rs` so that the published crate does not read a
/// file outside its package). `there_are_44_error_codes` there checks that
/// the enum has 44 variants, so the one-code-per-value check below also
/// shows that `ALL_CODES` lists every variant.
#[test]
fn error_codes_match_the_c_header() {
    let names = c_error_names();
    assert_eq!(names.len(), 45, "enum XML_Error has 45 codes in 2.9.0");
    assert_eq!(names.first(), Some(&"NONE"));
    for (value, name) in names.iter().enumerate().skip(1) {
        let found: Vec<_> = ALL_CODES
            .iter()
            .filter(|c| c.to_c() as usize == value)
            .collect();
        assert_eq!(found.len(), 1, "XML_ERROR_{name} = {value}");
        let debug = found.first().map(|c| format!("{c:?}"));
        assert_eq!(debug, Some(camel(name)), "value {value}");
    }
}

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

// ---------------------------------------------------------------------------
// S3 (D-040): `Parser::with_options`, encoding names and namespace
// separators. The oracle side is `buffer_run_with`, which creates the C
// parser with `XML_ParserCreate(encoding)` or `XML_ParserCreateNS(encoding,
// sep)`; `check_all_cfg` compares the result and code of every call and
// the position of every error, as T2 and T3 do.

/// The encoding names compared: the six names libexpat knows in both
/// cases, the empty name, unknown names, and names with a NUL byte (C
/// reads them up to the NUL).
const ENCODINGS: [&[u8]; 17] = [
    b"UTF-8",
    b"utf-8",
    b"UTF-16",
    b"utf-16",
    b"UTF-16BE",
    b"UTF-16LE",
    b"utf-16le",
    b"ISO-8859-1",
    b"iso-8859-1",
    b"US-ASCII",
    b"us-ascii",
    b"",
    b"UTF8",
    b"KOI8-R",
    b"UTF-8\0junk",
    b"ISO-8859-1\0",
    b"\0UTF-8",
];

/// The namespace separators compared: URI characters (`:`, `/`), non-URI
/// characters (`|`, space, newline, `!`), NUL, and bytes from 0x80 up.
const SEPARATORS: [u8; 12] = [
    b'|', b':', b'/', b' ', b'\n', b'!', 0, 0x80, 0xa9, 0xc3, 0xe9, 0xff,
];

/// Documents that exercise namespace processing: declarations, unbound
/// and undeclared prefixes, the reserved prefixes and names, namespace
/// names containing separator bytes, duplicate expanded attribute names,
/// and defaults from the DTD.
fn namespace_documents() -> Vec<(String, Vec<u8>)> {
    let docs: [&[u8]; 26] = [
        b"<a/>",
        b"<p:a/>",
        b"<p:a xmlns:p='u'/>",
        b"<a xmlns='u'><b/></a>",
        b"<a xmlns:p='u'><p:b p:x='1'/></a>",
        b"<a b:c='1'/>",
        b"<a xmlns:p='u'><b xmlns:p=''/></a>",
        b"<a xmlns=''/>",
        b"<a xmlns:xml='http://www.w3.org/XML/1998/namespace'/>",
        b"<a xmlns:xml='u'/>",
        b"<a xmlns:xmlns='u'/>",
        b"<a xmlns:p='http://www.w3.org/2000/xmlns/'/>",
        b"<a xmlns:p='http://www.w3.org/XML/1998/namespace'/>",
        b"<a xmlns='u|v'/>",
        b"<a xmlns='u:v'/>",
        b"<a xmlns='u/v'/>",
        b"<a xmlns='u v'/>",
        b"<a xmlns='u&#10;v'/>",
        b"<a xmlns='u!v'/>",
        b"<a xmlns='u\xc2\x80v'/>",
        b"<a xmlns='u\xc3\xa9v'/>",
        b"<a xmlns='u\xc2\xa9'/>",
        b"<a xmlns:p='u' xmlns:q='u' p:x='1' q:x='2'/>",
        b"<!DOCTYPE a [<!ATTLIST a xmlns:p CDATA 'u'>]><p:a/>",
        b"<!DOCTYPE a [<!ATTLIST a xmlns CDATA 'u|v'>]><a/>",
        b"<a:b:c/>",
    ];
    docs.iter()
        .enumerate()
        .map(|(i, d)| (format!("ns doc #{i}"), d.to_vec()))
        .collect()
}

/// Documents for encoding names: Latin-1 and ASCII bytes, UTF-16 with and
/// without a byte order mark, a UTF-8 byte order mark, and XML
/// declarations that name another encoding than the override.
fn encoding_documents() -> Vec<(String, Vec<u8>)> {
    let mut docs: Vec<(String, Vec<u8>)> = Vec::new();
    let mut add = |id: &str, d: Vec<u8>| docs.push((id.to_string(), d));
    add("ascii", b"<a>plain</a>".to_vec());
    add("latin1 byte", b"<a>caf\xe9</a>".to_vec());
    add("utf8 e-acute", b"<a>caf\xc3\xa9</a>".to_vec());
    add("utf8 bom", b"\xef\xbb\xbf<a/>".to_vec());
    add(
        "decl utf-8",
        b"<?xml version='1.0' encoding='UTF-8'?><a/>".to_vec(),
    );
    add(
        "decl latin1 + byte",
        b"<?xml version='1.0' encoding='ISO-8859-1'?><a>\xe9</a>".to_vec(),
    );
    add(
        "decl utf-16 in utf-8",
        b"<?xml version='1.0' encoding='UTF-16'?><a/>".to_vec(),
    );
    add(
        "decl unknown",
        b"<?xml version='1.0' encoding='KOI8-R'?><a/>".to_vec(),
    );
    add(
        "decl us-ascii + byte",
        b"<?xml version='1.0' encoding='US-ASCII'?><a>\xe9</a>".to_vec(),
    );
    for (name, text) in [
        ("plain", "<a>caf\u{e9}</a>"),
        ("decl utf-16", "<?xml version='1.0' encoding='UTF-16'?><a/>"),
        (
            "decl utf-8",
            "<?xml version='1.0' encoding='UTF-8'?><a>x</a>",
        ),
        ("error", "<a></b>"),
    ] {
        let le: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let be: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        add(&format!("utf-16le {name}"), le.clone());
        add(&format!("utf-16be {name}"), be.clone());
        add(
            &format!("utf-16le bom {name}"),
            [&[0xff, 0xfe][..], &le].concat(),
        );
        add(
            &format!("utf-16be bom {name}"),
            [&[0xfe, 0xff][..], &be].concat(),
        );
    }
    docs
}

/// Runs `check_all_cfg` for `cfg` on every document in one call and in
/// random pieces; returns (runs, runs with an error).
fn check_cfg_corpus(r: &mut Rng, cfg: &Cfg<'_>, docs: &[(String, Vec<u8>)]) -> (usize, usize) {
    let (mut n, mut errors) = (0, 0);
    for (id, doc) in docs {
        for ops in [one_call(doc), random_pieces(r, doc)] {
            let id = format!("{id} with {cfg:?}");
            let calls = check_all_cfg(&id, Some(cfg), doc, &ops);
            errors += usize::from(calls.iter().any(|c| c.0 == 0));
            n += 1;
        }
    }
    (n, errors)
}

/// `buffer_run` is `buffer_run_with` with no encoding name and `'|'` (or
/// no namespace processing), and `Some(0)` creates a namespace parser:
/// `<p:a/>` fails with an unbound prefix, as with `'|'`, and not without
/// namespace processing.
#[test]
fn s3_oracle_config() {
    let docs = documents();
    for (id, doc) in docs.iter().take(200) {
        let ops = one_call(doc);
        for ns in [false, true] {
            let cfg = BufferConfig {
                encoding: None,
                ns_separator: ns.then_some(b'|'),
                reparse_deferral: true,
            };
            assert_eq!(
                expat_oracle::buffer_run(doc, &ops, ns, true),
                expat_oracle::buffer_run_with(doc, &ops, &cfg, false),
                "{id} ns {ns}"
            );
            assert_eq!(
                expat_oracle::buffer_run_with_error_positions(doc, &ops, ns, true),
                expat_oracle::buffer_run_with(doc, &ops, &cfg, true),
                "{id} ns {ns}"
            );
        }
    }
    let unbound = ErrorCode::UnboundPrefix.to_c();
    let doc = b"<p:a/>";
    let code = |sep: Option<u8>| {
        let cfg = Cfg {
            encoding: None,
            sep,
        };
        c_run_cfg(Some(&cfg), doc, &one_call(doc)).0[0]
    };
    assert_eq!(code(None), (1, 0));
    assert_eq!(code(Some(b'|')), (0, unbound));
    assert_eq!(code(Some(0)), (0, unbound));
}

/// `Parser::new(salt)` and `Parser::with_options(salt, Options::default())`
/// give the same results, and both are C's `XML_ParserCreate(NULL)`.
#[test]
fn s3_default_options_are_new() {
    let mut r = Rng::new(0x5333_0000_0001);
    let default = Cfg {
        encoding: None,
        sep: None,
    };
    assert_eq!(default.options(), Options::default());
    for (id, doc) in documents() {
        for ops in [one_call(&doc), random_pieces(&mut r, &doc)] {
            assert_eq!(
                port_run_error_positions(None, &doc, &ops),
                port_run_error_positions(Some(&default), &doc, &ops),
                "{id}: new and with_options(default) differ"
            );
            check_all_cfg(&id, Some(&default), &doc, &ops);
        }
    }
}

/// Every encoding name on the upstream vectors, one seed of generated
/// documents and the encoding documents (also without a name), each in
/// one call and in random pieces.
#[test]
fn s3_encoding_names() {
    let mut r = Rng::new(0x5333_0000_0002);
    let mut docs: Vec<(String, Vec<u8>)> = documents().into_iter().take(540 + 300).collect();
    docs.extend(encoding_documents());
    let enc_docs = encoding_documents();
    let (mut runs, mut errors) = check_cfg_corpus(
        &mut r,
        &Cfg {
            encoding: None,
            sep: None,
        },
        &enc_docs,
    );
    for name in ENCODINGS {
        let (n, e) = check_cfg_corpus(
            &mut r,
            &Cfg {
                encoding: Some(name),
                sep: None,
            },
            &docs,
        );
        runs += n;
        errors += e;
    }
    eprintln!("encoding names: {runs} runs, {errors} with an error");
    assert!(errors > 0 && errors < runs);
}

/// Encoding names combined with namespace processing.
#[test]
fn s3_encoding_names_with_namespaces() {
    let mut r = Rng::new(0x5333_0000_0003);
    let mut docs = encoding_documents();
    docs.extend(namespace_documents());
    for name in [
        b"UTF-8".as_slice(),
        b"UTF-16LE",
        b"ISO-8859-1",
        b"",
        b"KOI8-R",
    ] {
        for sep in [b'|', 0] {
            check_cfg_corpus(
                &mut r,
                &Cfg {
                    encoding: Some(name),
                    sep: Some(sep),
                },
                &docs,
            );
        }
    }
}

/// Every separator, and no namespace processing, on the namespace
/// documents, the upstream vectors and both seeds of generated documents,
/// each in one call and in random pieces.
#[test]
fn s3_namespace_separators() {
    let mut r = Rng::new(0x5333_0000_0004);
    let mut docs = namespace_documents();
    docs.extend(documents());
    let mut seps: Vec<Option<u8>> = vec![None];
    seps.extend(SEPARATORS.iter().copied().map(Some));
    let (mut runs, mut errors) = (0, 0);
    for sep in seps {
        let (n, e) = check_cfg_corpus(
            &mut r,
            &Cfg {
                encoding: None,
                sep,
            },
            &docs,
        );
        runs += n;
        errors += e;
    }
    eprintln!("namespace separators: {runs} runs, {errors} with an error");
    assert!(errors > 0 && errors < runs);
}

/// A recorded case: document, encoding name, separator, C's result.
type Recorded<'a> = (&'a [u8], Option<&'a [u8]>, Option<u8>, Call);

/// C's results for the cases D-040 records (NUL in a name, separators from
/// 0x80 up, namespace names containing the separator), each also checked
/// against the port by `check_all_cfg`.
#[test]
fn s3_recorded_c_behaviour() {
    let ok = (1, 0);
    let err = |c: ErrorCode| (0, c.to_c());
    let cases: [Recorded<'_>; 14] = [
        // The name ends at its first NUL.
        (b"<a>caf\xc3\xa9</a>", Some(b"UTF-8\0junk"), None, ok),
        (b"<a>caf\xe9</a>", Some(b"ISO-8859-1\0X"), None, ok),
        (
            b"<a/>",
            Some(b"\0UTF-8"),
            None,
            err(ErrorCode::UnknownEncoding),
        ),
        (b"<a/>", Some(b""), None, err(ErrorCode::UnknownEncoding)),
        (
            b"<a/>",
            Some(b"KOI8-R"),
            None,
            err(ErrorCode::UnknownEncoding),
        ),
        (b"<a>caf\xe9</a>", Some(b"iso-8859-1"), None, ok),
        // The override wins over the declaration.
        (
            b"<?xml version='1.0' encoding='KOI8-R'?><a/>",
            Some(b"UTF-8"),
            None,
            ok,
        ),
        // Separators: compared with the bytes of the namespace name in
        // UTF-8, and only when they are not URI characters.
        (
            b"<a xmlns='u|v'/>",
            None,
            Some(b'|'),
            err(ErrorCode::Syntax),
        ),
        (b"<a xmlns='u:v'/>", None, Some(b':'), ok),
        (
            b"<a xmlns='u\xc2\x80v'/>",
            None,
            Some(0x80),
            err(ErrorCode::Syntax),
        ),
        (
            b"<a xmlns='u\xc3\xa9v'/>",
            None,
            Some(0xc3),
            err(ErrorCode::Syntax),
        ),
        (b"<a xmlns='u\xc3\xa9v'/>", None, Some(0xe9), ok),
        (b"<p:a/>", None, Some(0), err(ErrorCode::UnboundPrefix)),
        (b"<p:a xmlns:p='u'/>", None, Some(0), ok),
    ];
    for (doc, encoding, sep, want) in cases {
        let cfg = Cfg { encoding, sep };
        let id = format!("{doc:?} with {cfg:?}");
        let calls = check_all_cfg(&id, Some(&cfg), doc, &one_call(doc));
        assert_eq!(calls, vec![want], "{id}: C's result");
    }
    // An unknown name fails in the first call, before any byte is read.
    let cfg = Cfg {
        encoding: Some(b"KOI8-R"),
        sep: None,
    };
    let (calls, pos) = c_run_cfg(Some(&cfg), b"<a/>", &one_call(b"<a/>"));
    assert_eq!(
        (calls, pos),
        (vec![err(ErrorCode::UnknownEncoding)], (1, 0, -1))
    );
}

/// T3 with options: calls after an unknown-name error and after a
/// namespace error, and random call sequences.
#[test]
fn s3_calls_after_the_end() {
    let mut r = Rng::new(0x5333_0000_0005);
    let cfgs = [
        Cfg {
            encoding: Some(b"KOI8-R"),
            sep: None,
        },
        Cfg {
            encoding: Some(b"UTF-16LE"),
            sep: Some(b'|'),
        },
        Cfg {
            encoding: None,
            sep: Some(0),
        },
        Cfg {
            encoding: Some(b"ISO-8859-1"),
            sep: Some(0xe9),
        },
    ];
    let docs: [&[u8]; 4] = [
        b"<a/>",
        b"<p:a/>",
        b"<a xmlns='u|v'>x</a>",
        b"<a>caf\xe9</a>",
    ];
    for cfg in &cfgs {
        for doc in docs {
            for ops in [
                vec![parse(4, true), parse(1, false), parse(0, true)],
                vec![
                    parse(2, false),
                    parse(0, false),
                    parse(100, true),
                    parse(0, true),
                ],
                random_pieces(&mut r, doc),
            ] {
                check_all_cfg(&format!("{doc:?} with {cfg:?}"), Some(cfg), doc, &ops);
            }
        }
    }
}
