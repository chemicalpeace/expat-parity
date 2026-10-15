//! The trace adapter of the public callback API (module 12, slice 4,
//! D-041): runs [`expat::Parser::parse_with`] with a public
//! [`expat::Handler`] and writes the canonical trace of
//! `oracle_parse_trace` (`docs/ARCHITECTURE.md`, "Trace format") through
//! [`TraceWriter`], so it compares with the C oracle run with the
//! matching [`HandlerMask`] ([`oracle_mask`]).
//!
//! It feeds the document like the shim: one final call when the chunk
//! size is 0 or covers the document, otherwise chunks followed by an empty
//! final call, stopping at the first error. Only the public API is used,
//! so two things of the oracle trace are not observable here:
//!
//! - The position after a call that succeeded: the public API reports a
//!   position only in an [`expat::Error`]. The `result` line of a run
//!   without an error therefore has `-` for line, column and byte index;
//!   [`mask_ok_position`] puts the same on the C trace. The position of
//!   every error is compared.
//! - The split points of character data: the trace format coalesces
//!   consecutive `XML_CharacterDataHandler` calls on both sides, so only
//!   the concatenated text between other events is compared.

use crate::port::ORACLE_HASH_SALT;
use crate::trace::{Status, TraceWriter};
use expat::{Events, Handler, Options, Parser};
use expat_oracle::{HandlerMask, ParseOptions};

/// The C handlers `parse_with` sets for `events`: the nine of [`Events`],
/// every other trace handler unset.
pub fn oracle_mask(events: Events) -> HandlerMask {
    HandlerMask {
        start_element: events.start_element,
        end_element: events.end_element,
        character_data: events.character_data,
        processing_instruction: events.processing_instruction,
        comment: events.comment,
        start_cdata_section: events.start_cdata_section,
        end_cdata_section: events.end_cdata_section,
        start_namespace_decl: events.start_namespace_decl,
        end_namespace_decl: events.end_namespace_decl,
        ..HandlerMask::NONE
    }
}

/// The nine events, in [`Events`] field order, by name: the mask with only
/// that event on.
pub fn single_events() -> [(&'static str, Events); 9] {
    let n = Events::NONE;
    [
        ("start_element", n.start_element(true)),
        ("end_element", n.end_element(true)),
        ("character_data", n.character_data(true)),
        ("processing_instruction", n.processing_instruction(true)),
        ("comment", n.comment(true)),
        ("start_cdata_section", n.start_cdata_section(true)),
        ("end_cdata_section", n.end_cdata_section(true)),
        ("start_namespace_decl", n.start_namespace_decl(true)),
        ("end_namespace_decl", n.end_namespace_decl(true)),
    ]
}

/// The masks [`public_trace`] supports: `ALL`, `NONE`, then each event
/// alone ([`single_events`]).
pub fn supported_events() -> Vec<(&'static str, Events)> {
    let mut v = vec![("ALL", Events::ALL), ("NONE", Events::NONE)];
    v.extend(single_events());
    v
}

/// A public handler that writes the trace, with `EVENTS` chosen by `M`:
/// 0 is `ALL`, 1 is `NONE`, 2 + i is the event `i` of [`single_events`]
/// alone. `EVENTS` is an associated constant, so each mask is its own type.
struct Tracer<const M: u8> {
    w: TraceWriter,
}

/// The `EVENTS` of [`Tracer`]`<M>`.
const fn events_of(m: u8) -> Events {
    let n = Events::NONE;
    match m {
        0 => Events::ALL,
        1 => n,
        2 => n.start_element(true),
        3 => n.end_element(true),
        4 => n.character_data(true),
        5 => n.processing_instruction(true),
        6 => n.comment(true),
        7 => n.start_cdata_section(true),
        8 => n.end_cdata_section(true),
        9 => n.start_namespace_decl(true),
        _ => n.end_namespace_decl(true),
    }
}

impl<const M: u8> Handler for Tracer<M> {
    const EVENTS: Events = events_of(M);

    fn start_element(&mut self, name: &[u8], attributes: &[(&[u8], &[u8])], specified: usize) {
        // The trace has C's count, twice the number of given attributes.
        let specified = i32::try_from(specified.saturating_mul(2)).unwrap_or(i32::MAX);
        self.w.start(name, specified, attributes);
    }
    fn end_element(&mut self, name: &[u8]) {
        self.w.end(name);
    }
    fn character_data(&mut self, data: &[u8]) {
        self.w.text(data);
    }
    fn processing_instruction(&mut self, target: &[u8], data: &[u8]) {
        self.w.pi(target, data);
    }
    fn comment(&mut self, data: &[u8]) {
        self.w.comment(data);
    }
    fn start_cdata_section(&mut self) {
        self.w.cdata();
    }
    fn end_cdata_section(&mut self) {
        self.w.end_cdata();
    }
    fn start_namespace_decl(&mut self, prefix: Option<&[u8]>, uri: Option<&[u8]>) {
        self.w.ns_start(prefix, uri);
    }
    fn end_namespace_decl(&mut self, prefix: Option<&[u8]>) {
        self.w.ns_end(prefix);
    }
}

/// The public options for the oracle options' encoding and separator.
fn public_options(opts: &ParseOptions) -> Options {
    let mut o = Options::new();
    if let Some(e) = &opts.encoding {
        o = o.encoding(e.as_bytes());
    }
    if let Some(s) = opts.ns_separator {
        o = o.namespace_separator(s);
    }
    o
}

/// Feeds `doc` to a public parser with [`Tracer`]`<M>`.
fn run<const M: u8>(doc: &[u8], opts: &ParseOptions) -> Vec<u8> {
    let mut p = Parser::with_options(ORACLE_HASH_SALT, public_options(opts));
    let mut t = Tracer::<M> {
        w: TraceWriter::new(),
    };
    let mut r = Ok(());
    if opts.chunk_size == 0 || opts.chunk_size >= doc.len() {
        r = p.parse_with(&mut t, doc, true);
    } else {
        for chunk in doc.chunks(opts.chunk_size) {
            r = p.parse_with(&mut t, chunk, false);
            if r.is_err() {
                break;
            }
        }
        if r.is_ok() {
            r = p.parse_with(&mut t, b"", true);
        }
    }
    match r {
        Ok(()) => {
            let mut out = t.w.finish(Status::Ok, 0, 0, 0, 0);
            mask_result_position(&mut out);
            out
        }
        Err(e) => t.w.finish(
            Status::Error,
            e.code().to_c(),
            e.line(),
            e.column(),
            e.byte_index(),
        ),
    }
}

/// Parses `doc` with the public API, reporting `events` (one of
/// [`supported_events`]), the encoding name and namespace separator of
/// `opts` and its chunk size; every other option must be the default.
/// Compare with [`mask_ok_position`] of the oracle's `parse_trace` with
/// `handler_mask: Some(oracle_mask(events))`.
///
/// # Panics
///
/// If `events` is not one of [`supported_events`].
#[allow(clippy::panic)] // reason: a test helper's precondition.
pub fn public_trace(doc: &[u8], opts: &ParseOptions, events: Events) -> Vec<u8> {
    let Some(m) = (0..=10u8).find(|&m| events_of(m) == events) else {
        panic!("unsupported events {events:?}");
    };
    match m {
        0 => run::<0>(doc, opts),
        1 => run::<1>(doc, opts),
        2 => run::<2>(doc, opts),
        3 => run::<3>(doc, opts),
        4 => run::<4>(doc, opts),
        5 => run::<5>(doc, opts),
        6 => run::<6>(doc, opts),
        7 => run::<7>(doc, opts),
        8 => run::<8>(doc, opts),
        9 => run::<9>(doc, opts),
        _ => run::<10>(doc, opts),
    }
}

/// Replaces line, column and byte index of a final `result ok ...` line by
/// `-`: the position after a successful run, which the public API does
/// not report. A `result error ...` line is left as it is.
fn mask_result_position(trace: &mut Vec<u8>) {
    let text = String::from_utf8_lossy(trace).into_owned();
    let body = text.trim_end_matches('\n');
    let (head, last) = match body.rfind('\n') {
        Some(i) => (&body[..=i], &body[i + 1..]),
        None => ("", body),
    };
    let f: Vec<&str> = last.split(' ').collect();
    if f.len() == 6 && f[0] == "result" && f[1] == "ok" {
        *trace = format!("{head}result ok {} - - -\n", f[2]).into_bytes();
    }
}

/// The masking of the public side's `result ok` line, for a C trace: the oracle side of a
/// comparison with [`public_trace`].
pub fn mask_ok_position(trace: &[u8]) -> Vec<u8> {
    let mut t = trace.to_vec();
    mask_result_position(&mut t);
    t
}
