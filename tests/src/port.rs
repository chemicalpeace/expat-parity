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
    self, ContentModel, ExternalEntities, Handler, HandlerSet, ParamEntityParsing as PortPep,
    Parser, Stop,
};
use expat_oracle::{BufferOp, EXT_MAX_DEPTH, ExtHandler, ParamEntityParsing, ParseOptions};

/// The fixed hash salt of the C shim (`ORACLE_HASH_SALT` in
/// `oracle_trace.c`).
pub const ORACLE_HASH_SALT: [u8; 16] = *b"expat-rs-oracle!";

struct Tracer {
    w: TraceWriter,
    /// The shim's test external entity reference handler.
    ext: ExtHandler,
    /// The shim's test not-standalone handler.
    not_standalone: Option<bool>,
    /// The entities [`ExtHandler::Read`] reads, by system identifier.
    entities: Vec<(Vec<u8>, Vec<u8>)>,
    /// Chunk size and buffer API of the document, used for the entities too.
    chunk_size: usize,
    buffer_api: bool,
    /// Nesting depth of external entity parsers.
    depth: usize,
}

impl Tracer {
    /// The shim's `read_external`: reads `body` through an external entity
    /// parser and traces `extend`.
    fn read_external(
        &mut self,
        parsers: &mut ExternalEntities<'_>,
        context: Option<&[u8]>,
        body: &[u8],
    ) -> bool {
        if self.depth >= EXT_MAX_DEPTH {
            self.w.raw_line("extdepth", &[]);
            return false;
        }
        let Some(mut child) = parsers.create(context, None) else {
            self.w.raw_line("extcreate", &[]);
            return false;
        };
        self.depth += 1;
        let mut st = parse::Status::Ok;
        let (chunk, buffer_api) = (self.chunk_size, self.buffer_api);
        if chunk == 0 || chunk >= body.len() {
            st = feed(&mut child, self, body, true, buffer_api);
        } else {
            for piece in body.chunks(chunk) {
                st = feed(&mut child, self, piece, false, buffer_api);
                if st != parse::Status::Ok {
                    break;
                }
            }
            if st == parse::Status::Ok {
                st = feed(&mut child, self, b"", true, buffer_api);
            }
        }
        self.depth -= 1;
        let Stop::Error(e) = child.error_code();
        let code = i64::from(e.to_c());
        let line = child.current_line_number();
        let column = child.current_column_number();
        let index = child.current_byte_index();
        self.w.raw_line(
            "extend",
            &[
                i64::from(st == parse::Status::Ok),
                code,
                i64::try_from(line).unwrap_or(i64::MAX),
                i64::try_from(column).unwrap_or(i64::MAX),
                index,
            ],
        );
        st == parse::Status::Ok
    }
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
    fn external_entity_ref(
        &mut self,
        parsers: &mut ExternalEntities<'_>,
        context: Option<&[u8]>,
        base: Option<&[u8]>,
        system_id: Option<&[u8]>,
        public_id: Option<&[u8]>,
    ) -> bool {
        self.w.extref(context, base, system_id, public_id);
        match self.ext {
            ExtHandler::Read => {
                let body = system_id.and_then(|id| {
                    self.entities
                        .iter()
                        .find(|(k, _)| k.as_slice() == id)
                        .map(|(_, v)| v.clone())
                });
                match body {
                    Some(body) => self.read_external(parsers, context, &body),
                    None => true,
                }
            }
            ExtHandler::Accept => true,
            ExtHandler::None | ExtHandler::Fail => false,
        }
    }
    fn not_standalone(&mut self) -> bool {
        self.w.not_standalone();
        self.not_standalone == Some(true)
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
        external_entity_ref: false,
        not_standalone: false,
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

/// The port's allocation tracker during the calling thread's last
/// [`port_trace`] (module 11c): bytes charged after creation and the limit
/// setters, at the end of the parse, and the peak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PortTracker {
    /// After creation and the setters.
    pub created: u64,
    /// At the end of the parse.
    pub end: u64,
    /// The highest count during the parse.
    pub peak: u64,
}

std::thread_local! {
    static LAST_TRACKER: std::cell::Cell<PortTracker> = const {
        std::cell::Cell::new(PortTracker { created: 0, end: 0, peak: 0 })
    };
}

/// [`PortTracker`] of the calling thread's last [`port_trace`].
pub fn last_port_tracker() -> PortTracker {
    LAST_TRACKER.with(std::cell::Cell::get)
}

/// Parses `doc` with the port under `opts` and returns its trace.
pub fn port_trace(doc: &[u8], opts: &ParseOptions) -> Vec<u8> {
    let mut p = Parser::new(
        opts.encoding.as_deref().map(str::as_bytes),
        opts.ns_separator,
        ORACLE_HASH_SALT,
    );
    p.set_handlers(HandlerSet {
        external_entity_ref: opts.ext_handler != ExtHandler::None,
        not_standalone: opts.not_standalone.is_some(),
        ..traced_handlers()
    });
    if opts.use_foreign_dtd {
        p.use_foreign_dtd(true);
    }
    p.set_param_entity_parsing(match opts.param_entity_parsing {
        ParamEntityParsing::Never => PortPep::Never,
        ParamEntityParsing::UnlessStandalone => PortPep::UnlessStandalone,
        ParamEntityParsing::Always => PortPep::Always,
    });
    let mut t = Tracer {
        w: if opts.text_digest {
            TraceWriter::with_text_digest()
        } else {
            TraceWriter::new()
        },
        ext: opts.ext_handler,
        not_standalone: opts.not_standalone,
        entities: opts.ext_entities.clone(),
        chunk_size: opts.chunk_size,
        buffer_api: opts.buffer_api,
        depth: 0,
    };
    // Same order as the C shim.
    if let Some(f) = opts.limits.bl_amplification {
        let ok = p.set_billion_laughs_maximum_amplification(f.0);
        t.w.limit("bl-amplification", ok);
    }
    if let Some(b) = opts.limits.bl_threshold {
        let ok = p.set_billion_laughs_activation_threshold(b);
        t.w.limit("bl-threshold", ok);
    }
    if let Some(f) = opts.limits.at_amplification {
        let ok = p.set_alloc_tracker_maximum_amplification(f.0);
        t.w.limit("at-amplification", ok);
    }
    if let Some(b) = opts.limits.at_threshold {
        let ok = p.set_alloc_tracker_activation_threshold(b);
        t.w.limit("at-threshold", ok);
    }
    let created = p.alloc_tracker().bytes_allocated;
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
    LAST_TRACKER.with(|c| {
        c.set(PortTracker {
            created,
            end: p.alloc_tracker().bytes_allocated,
            peak: p.alloc_tracker().peak_bytes_allocated,
        });
    });
    let Stop::Error(e) = p.error_code();
    let code = e.to_c();
    // The shim writes `ok` only for XML_STATUS_OK.
    let status = if st == parse::Status::Ok {
        Status::Ok
    } else {
        Status::Error
    };
    if opts.trace_accounting {
        let a = p.accounting();
        t.w.accounting(a.count_bytes_direct, a.count_bytes_indirect);
    }
    let line = p.current_line_number();
    let column = p.current_column_number();
    t.w.finish(status, code, line, column, p.current_byte_index())
}

/// The port as a [`TraceSource`].
#[derive(Debug, Default, Clone, Copy)]
pub struct Port;

impl TraceSource for Port {
    fn name(&self) -> &str {
        "expat-rs"
    }

    fn trace(&self, doc: &[u8], opts: &ParseOptions) -> Vec<u8> {
        port_trace(doc, opts)
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
        let Stop::Error(e) = p.error_code();
        let code = e.to_c();
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
