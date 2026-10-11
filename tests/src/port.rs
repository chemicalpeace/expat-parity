//! The trace adapter of the port: runs [`expat::internal::parse::Parser`]
//! with the handlers the C shim sets and writes the same canonical trace
//! (`docs/ARCHITECTURE.md`, "Trace format") through [`TraceWriter`].
//!
//! It feeds the document exactly like `oracle_parse_trace`: one final
//! `XML_Parse` call when the chunk size is 0 or covers the document,
//! otherwise chunks followed by an empty final call, stopping at the first
//! error.

use crate::diff::TraceSource;
use crate::trace::{Status, TraceWriter};
use expat::internal::parse::{
    self, ContentModel, Handler, HandlerSet, ParamEntityParsing as PortPep, Parser, Stop,
};
use expat_oracle::{BufferOp, ParamEntityParsing, ParseOptions};

/// The fixed hash salt of the C shim (`ORACLE_HASH_SALT` in
/// `oracle_trace.c`).
pub const ORACLE_HASH_SALT: [u8; 16] = *b"expat-rs-oracle!";

/// Result of a port parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortTrace {
    /// The canonical trace.
    Trace(Vec<u8>),
    /// The document needs a part of the parser that is not ported yet
    /// (module 11, [`Stop::DtdNotPorted`]); there is no trace to compare.
    DtdNotPorted,
}

struct Tracer {
    w: TraceWriter,
}

impl Handler for Tracer {
    fn xml_decl(&mut self, version: Option<&[u8]>, encoding: Option<&[u8]>, standalone: i32) {
        self.w.xml_decl(version, encoding, standalone);
    }
    fn start_element(&mut self, name: &[u8], atts: &[(&[u8], &[u8])], specified: i32) {
        self.w.start(name, specified, atts);
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
    fn skipped_entity(&mut self, name: &[u8], is_param: bool) {
        self.w.skipped(name, is_param);
    }
    fn start_doctype_decl(
        &mut self,
        name: &[u8],
        sysid: Option<&[u8]>,
        pubid: Option<&[u8]>,
        has_internal_subset: bool,
    ) {
        self.w.doctype(name, sysid, pubid, has_internal_subset);
    }
    fn end_doctype_decl(&mut self) {
        self.w.end_doctype();
    }
    fn entity_decl(
        &mut self,
        name: &[u8],
        is_param: bool,
        value: Option<&[u8]>,
        base: Option<&[u8]>,
        sysid: Option<&[u8]>,
        pubid: Option<&[u8]>,
        notation: Option<&[u8]>,
    ) {
        self.w
            .entity(name, is_param, value, base, sysid, pubid, notation);
    }
    fn notation_decl(
        &mut self,
        name: &[u8],
        base: Option<&[u8]>,
        sysid: Option<&[u8]>,
        pubid: Option<&[u8]>,
    ) {
        self.w.notation(name, base, sysid, pubid);
    }
    fn element_decl(&mut self, name: &[u8], model: &ContentModel) {
        self.w.element(name, model);
    }
    fn attlist_decl(
        &mut self,
        elname: &[u8],
        attname: &[u8],
        att_type: &[u8],
        dflt: Option<&[u8]>,
        is_required: bool,
    ) {
        self.w
            .attlist(elname, attname, Some(att_type), dflt, is_required);
    }
}

/// The handlers `oracle_parse_trace` sets, as far as the port has them.
fn traced_handlers() -> HandlerSet {
    HandlerSet {
        xml_decl: true,
        start_element: true,
        end_element: true,
        character_data: true,
        processing_instruction: true,
        comment: true,
        start_cdata_section: true,
        end_cdata_section: true,
        start_namespace_decl: true,
        end_namespace_decl: true,
        skipped_entity: true,
        start_doctype_decl: true,
        end_doctype_decl: true,
        entity_decl: true,
        // The shim sets no unparsed-entity handler, so unparsed entities
        // reach the entity declaration handler.
        unparsed_entity_decl: false,
        notation_decl: true,
        element_decl: true,
        attlist_decl: true,
    }
}

/// One piece of input, like the shim's `feed`: [`Parser::parse`], or
/// [`Parser::get_buffer`] for `2n + 3` bytes and [`Parser::parse_buffer`]
/// for `n`.
fn feed(
    p: &mut Parser,
    t: &mut Tracer,
    s: &[u8],
    is_final: bool,
    buffer_api: bool,
) -> parse::Status {
    if !buffer_api {
        return p.parse(t, s, is_final);
    }
    let n = s.len();
    let req = if n <= (0x7fff_ffff - 3) / 2 {
        2 * n + 3
    } else {
        n
    };
    let (Ok(req), Ok(len)) = (i32::try_from(req), i32::try_from(n)) else {
        return parse::Status::Error;
    };
    let Some(b) = p.get_buffer(req) else {
        return parse::Status::Error;
    };
    if let Some(dst) = b.get_mut(..n) {
        dst.copy_from_slice(s);
    }
    p.parse_buffer(t, len, is_final)
}

/// Parses `doc` with the port under `opts` and returns its trace.
pub fn port_trace(doc: &[u8], opts: &ParseOptions) -> PortTrace {
    let mut p = Parser::new(
        opts.encoding.as_deref().map(str::as_bytes),
        opts.ns_separator,
        ORACLE_HASH_SALT,
    );
    p.set_handlers(traced_handlers());
    p.set_param_entity_parsing(match opts.param_entity_parsing {
        ParamEntityParsing::Never => PortPep::Never,
        ParamEntityParsing::UnlessStandalone => PortPep::UnlessStandalone,
        ParamEntityParsing::Always => PortPep::Always,
    });
    let mut t = Tracer {
        w: TraceWriter::new(),
    };
    if opts.no_reparse_deferral {
        p.set_reparse_deferral_enabled(false);
    }
    if opts.ns_triplets {
        p.set_return_ns_triplet(true);
    }
    let mut st = parse::Status::Ok;
    if opts.chunk_size == 0 || opts.chunk_size >= doc.len() {
        st = feed(&mut p, &mut t, doc, true, opts.buffer_api);
    } else {
        for chunk in doc.chunks(opts.chunk_size) {
            st = feed(&mut p, &mut t, chunk, false, opts.buffer_api);
            if st != parse::Status::Ok {
                break;
            }
        }
        if st == parse::Status::Ok {
            st = feed(&mut p, &mut t, b"", true, opts.buffer_api);
        }
    }
    let code = match p.error_code() {
        Stop::Error(e) => e.to_c(),
        Stop::DtdNotPorted => return PortTrace::DtdNotPorted,
    };
    // The shim writes `ok` only for XML_STATUS_OK.
    let status = if st == parse::Status::Ok {
        Status::Ok
    } else {
        Status::Error
    };
    let line = p.current_line_number();
    let column = p.current_column_number();
    PortTrace::Trace(t.w.finish(status, code, line, column, p.current_byte_index()))
}

/// The port as a [`TraceSource`]. A document outside the ported scope
/// yields the line `unported dtd`, which never equals a C trace.
#[derive(Debug, Default, Clone, Copy)]
pub struct Port;

impl TraceSource for Port {
    fn name(&self) -> &str {
        "expat-rs"
    }

    fn trace(&self, doc: &[u8], opts: &ParseOptions) -> Vec<u8> {
        match port_trace(doc, opts) {
            PortTrace::Trace(t) => t,
            PortTrace::DtdNotPorted => b"unported dtd\n".to_vec(),
        }
    }
}

/// No-op handler for [`buffer_run`].
struct Quiet;
impl Handler for Quiet {}

/// The port's version of [`expat_oracle::buffer_run`]: the same operations
/// and the same trace lines.
pub fn buffer_run(doc: &[u8], ops: &[BufferOp], ns: bool, reparse_deferral: bool) -> String {
    use std::fmt::Write as _;
    let salt: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    let mut p = Parser::new(None, ns.then_some(b'|'), salt);
    p.set_reparse_deferral_enabled(reparse_deferral);
    let mut out = String::new();
    let mut cur = 0usize;
    for op in ops {
        let mut got = -2i64;
        let st = match *op {
            BufferOp::Parse { n, is_final } => {
                let k = usize::try_from(n).unwrap_or(0).min(doc.len() - cur);
                let st = p.parse(&mut Quiet, &doc[cur..cur + k], is_final);
                cur += k;
                st
            }
            BufferOp::Buffer { req, n, is_final } => match p.get_buffer(req) {
                None => {
                    got = -1;
                    parse::Status::Error
                }
                Some(b) => {
                    let k = usize::try_from(n)
                        .unwrap_or(0)
                        .min(usize::try_from(req).unwrap_or(0));
                    let d = k.min(doc.len() - cur);
                    b[..d].copy_from_slice(&doc[cur..cur + d]);
                    b[d..k].fill(b' ');
                    cur += d;
                    got = p.buffer_state().buffer_end;
                    p.parse_buffer(&mut Quiet, n, is_final)
                }
            },
        };
        let st = match st {
            parse::Status::Error => 0,
            parse::Status::Ok => 1,
            parse::Status::Suspended => 2,
        };
        let code = match p.error_code() {
            Stop::Error(e) => e.to_c(),
            Stop::DtdNotPorted => 999,
        };
        let b = p.buffer_state();
        let _ = writeln!(
            out,
            "{st} {code} {} {} {} {} {} {} {} {} {got}",
            b.buffer_ptr,
            b.buffer_end,
            b.buffer_lim,
            b.partial_token_bytes_before,
            b.last_buffer_request_size,
            b.parse_end_byte_index,
            b.event_ptr,
            b.position_ptr
        );
    }
    let line = p.current_line_number();
    let col = p.current_column_number();
    let _ = writeln!(out, "end {line} {col} {}", p.current_byte_index());
    out
}
