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
    self, Handler, HandlerSet, ParamEntityParsing as PortPep, Parser, Stop,
};
use expat_oracle::{ParamEntityParsing, ParseOptions};

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
    }
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
    let mut st = parse::Status::Ok;
    if opts.chunk_size == 0 || opts.chunk_size >= doc.len() {
        st = p.parse(&mut t, doc, true);
    } else {
        for chunk in doc.chunks(opts.chunk_size) {
            st = p.parse(&mut t, chunk, false);
            if st != parse::Status::Ok {
                break;
            }
        }
        if st == parse::Status::Ok {
            st = p.parse(&mut t, b"", true);
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
