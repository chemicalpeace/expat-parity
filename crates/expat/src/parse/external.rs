//! External entity parsers (`XML_ExternalEntityParserCreate`,
//! `xmlparse.c` 1680-1863) and the processors that run in them:
//! `externalEntityInitProcessor` 1-3 (general entities: text declaration,
//! then content at tag level 1), `externalParEntInitProcessor`,
//! `externalParEntProcessor`, `entityValueInitProcessor`,
//! `entityValueProcessor` (parameter entities and the external subset)
//! and `ignoreSectionProcessor` / `doIgnoreSection` (conditional sections).
//!
//! The library never fetches anything. The application's external entity
//! reference handler receives an [`ExternalEntities`] and may create an
//! [`ExternalEntityParser`] for the entity, feed it bytes it obtained
//! itself, and drop it before returning.
//!
//! Sharing with the parent (D-032): a parameter entity parser shares the
//! parent's `DTD` (C: the same pointer); a general entity parser gets a copy
//! (`dtdCopy`) and the parent's namespace bindings and open entities from
//! the context string (`setContext`). Every parser charges the root
//! parser's allocation tracker and billion-laughs accounting (C:
//! `getRootParserOf`). The port moves these shared parts into the child
//! when it is created and back when it is dropped; the parent cannot run in
//! between, because it is suspended inside its handler.
//!
//! ```
//! # #[cfg(feature = "internal-api")] {
//! use expat::internal::parse::{ExternalEntities, Handler, HandlerSet, Parser, Status};
//!
//! /// Supplies `<b/>` for every external general entity.
//! #[derive(Default)]
//! struct Resolver(Vec<Vec<u8>>);
//! impl Handler for Resolver {
//!     fn start_element(&mut self, name: &[u8], _atts: &[(&[u8], &[u8])], _specified: i32) {
//!         self.0.push(name.to_vec());
//!     }
//!     fn external_entity_ref(
//!         &mut self,
//!         parsers: &mut ExternalEntities<'_>,
//!         context: Option<&[u8]>,
//!         _base: Option<&[u8]>,
//!         _system_id: Option<&[u8]>,
//!         _public_id: Option<&[u8]>,
//!     ) -> bool {
//!         let Some(mut child) = parsers.create(context, None) else { return false };
//!         child.parse(self, b"<b/>", true) == Status::Ok
//!     }
//! }
//!
//! let mut p = Parser::new(None, None, [0; 16]);
//! p.set_handlers(HandlerSet { start_element: true, external_entity_ref: true, ..HandlerSet::default() });
//! let mut r = Resolver::default();
//! let doc = b"<!DOCTYPE a [<!ENTITY e SYSTEM 'e.xml'>]><a>&e;</a>";
//! assert_eq!(p.parse(&mut r, doc, true), Status::Ok);
//! assert_eq!(r.0, [b"a".to_vec(), b"b".to_vec()]);
//! # }
//! ```

use alloc::vec::Vec;
use core::ops::{Deref, DerefMut};

use super::accounting::Account;
use super::dtd::{Dtd, PrefixId};
use super::error::{Stop, XmlError};
use super::prolog::{PrologDone, with_known};
use super::tracker::Pool;
use super::{Enc, Handler, Parser, Parsing, Processor};
use crate::tok::{Encoding, InitEncoding, Normal, Scan, Scanned, State, Tok, ignore_section_tok, init_scan};

/// `CONTEXT_SEP`: separates the items of a context string.
const CONTEXT_SEP: u8 = 0x0C;

/// Creates external entity parsers for the entity whose reference the
/// handler is called for (the `XML_Parser` argument of C's
/// `XML_ExternalEntityRefHandler`, restricted to
/// `XML_ExternalEntityParserCreate`).
#[derive(Debug)]
pub struct ExternalEntities<'a> {
    parent: &'a mut Parser,
}

impl<'a> ExternalEntities<'a> {
    pub(crate) fn new(parent: &'a mut Parser) -> Self {
        ExternalEntities { parent }
    }

    /// `XML_ExternalEntityParserCreate(parser, context, encoding)`: `context`
    /// as the handler received it (`None` for a parameter entity or the
    /// external subset). `None` when C returns `NULL`: the allocation
    /// tracker refuses the new parser, or the context cannot be set up.
    pub fn create(&mut self, context: Option<&[u8]>, encoding: Option<&[u8]>) -> Option<ExternalEntityParser<'_>> {
        let (child, bytes_before) = self.parent.external_entity_parser_create(context, encoding)?;
        Some(ExternalEntityParser {
            parent: &mut *self.parent,
            child,
            bytes_before,
        })
    }
}

/// An external entity parser. It dereferences to its [`Parser`] (feed it
/// with [`Parser::parse`] or the buffer API); dropping it is
/// `XML_ParserFree`, which hands the shared state back to the parent.
#[derive(Debug)]
pub struct ExternalEntityParser<'p> {
    parent: &'p mut Parser,
    child: Parser,
    /// The root tracker's byte count before the child was created.
    bytes_before: u64,
}

impl Deref for ExternalEntityParser<'_> {
    type Target = Parser;
    fn deref(&self) -> &Parser {
        &self.child
    }
}

impl DerefMut for ExternalEntityParser<'_> {
    fn deref_mut(&mut self) -> &mut Parser {
        &mut self.child
    }
}

impl Drop for ExternalEntityParser<'_> {
    fn drop(&mut self) {
        self.parent.external_entity_parser_free(&mut self.child, self.bytes_before);
    }
}

/// Handler for `setContext`, whose `addBinding` calls never report a
/// namespace declaration (no attribute id).
struct NoEvents;
impl Handler for NoEvents {}

impl Dtd {
    /// `dtdCopy` (`xmlparse.c` 7969-8119) with `copyEntityTable`
    /// (8121-8188): a new DTD with copies of every prefix, attribute id,
    /// element type (with its default attributes) and entity of `self`,
    /// created in slot order of the old tables as C does, so the new
    /// tables' slot order and the new `dtd->pool` blocks are C's. The
    /// allocations are charged to `tr` (whose `dtd_pool` must be the new,
    /// empty pool). The scaffold is shared in C (not deep-copied); the
    /// port copies its values.
    pub(crate) fn copy_for_child(
        &self,
        salt: &crate::siphash::SipKey,
        tr: &mut super::Tracker,
        direct: u64,
    ) -> Result<Dtd, crate::heap::NoMemory> {
        use super::dtd::{AttributeId, ElementType, Entity, Prefix};
        use super::tracker::{DEFAULT_ATTRIBUTE_SIZE, NAME_AND_DEFAULT_ATTRIBUTE_SIZE};
        let mut n = Dtd::new();
        // Copy the prefix table.
        for id in self.prefixes.iter() {
            let name = self.prefixes.key(id).unwrap_or_default();
            tr.store(Pool::Dtd, name.len(), direct)?;
            n.prefixes
                .lookup_or_insert(salt, &mut tr.heap(direct), name, super::dtd::PREFIX_SIZE, Prefix::default)?;
        }
        let map_prefix = |n: &Dtd, p: Option<PrefixId>| match p {
            None => None,
            Some(PrefixId::Default) => Some(PrefixId::Default),
            Some(PrefixId::Named(old)) => n
                .prefixes
                .find(salt, self.prefixes.key(old).unwrap_or_default())
                .map(PrefixId::Named),
        };
        // Copy the attribute id table (with the scratch byte before each
        // name).
        for id in self.attribute_ids.iter() {
            let name = self.attribute_ids.key(id).unwrap_or_default();
            let old = self.attribute_ids.value(id).copied().unwrap_or_default();
            {
                let (p, mut h) = tr.pool(Pool::Dtd, direct);
                p.append(&mut h, 1)?;
                p.append(&mut h, name.len() + 1)?;
                p.finish();
            }
            let (nid, _) = n.attribute_ids.lookup_or_insert(
                salt,
                &mut tr.heap(direct),
                name,
                super::dtd::ATTRIBUTE_ID_SIZE,
                AttributeId::default,
            )?;
            let prefix = map_prefix(&n, old.prefix);
            if let Some(a) = n.attribute_ids.value_mut(nid) {
                a.maybe_tokenized = old.maybe_tokenized;
                if old.prefix.is_some() {
                    a.xmlns = old.xmlns;
                    a.prefix = prefix;
                }
            }
        }
        let map_att = |n: &Dtd, old: crate::hash::EntryId| {
            n.attribute_ids
                .find(salt, self.attribute_ids.key(old).unwrap_or_default())
        };
        // Copy the element type table.
        for id in self.element_types.iter() {
            let name = self.element_types.key(id).unwrap_or_default();
            let Some(old) = self.element_types.value(id) else { continue };
            tr.store(Pool::Dtd, name.len(), direct)?;
            let (nid, _) = n.element_types.lookup_or_insert(
                salt,
                &mut tr.heap(direct),
                name,
                super::dtd::ELEMENT_TYPE_SIZE,
                ElementType::default,
            )?;
            let count = old.default_atts.len();
            if count > 0 {
                let bytes = count.checked_mul(DEFAULT_ATTRIBUTE_SIZE).ok_or(crate::heap::NoMemory)?;
                if !tr.t.alloc(bytes, direct) {
                    return Err(crate::heap::NoMemory);
                }
            }
            let id_att = old.id_att.and_then(|a| map_att(&n, a));
            let prefix = map_prefix(&n, old.prefix);
            let mut default_atts = Vec::with_capacity(count);
            let mut table = super::tracker::TableShadow::default();
            let mut for_name = alloc::collections::BTreeMap::new();
            for (i, d) in old.default_atts.iter().enumerate() {
                let Some(att) = map_att(&n, d.id) else { continue };
                if let Some(v) = &d.value {
                    tr.store(Pool::Dtd, v.len(), direct)?;
                }
                if let alloc::collections::btree_map::Entry::Vacant(e) = for_name.entry(att) {
                    table.insert(&mut tr.heap(direct), NAME_AND_DEFAULT_ATTRIBUTE_SIZE)?;
                    e.insert(i);
                }
                default_atts.push(super::dtd::DefaultAttribute {
                    id: att,
                    is_cdata: d.is_cdata,
                    value: d.value.clone(),
                });
            }
            if let Some(e) = n.element_types.value_mut(nid) {
                e.id_att = id_att;
                e.prefix = prefix;
                e.default_atts = default_atts;
                e.default_att_for_name = for_name;
                e.alloc_default_atts = count;
                e.default_att_table = table;
            }
        }
        // Copy the entity tables.
        for is_param in [false, true] {
            let (old_table, new_table) = if is_param {
                (&self.param_entities, &mut n.param_entities)
            } else {
                (&self.general_entities, &mut n.general_entities)
            };
            for id in old_table.iter() {
                let name = old_table.key(id).unwrap_or_default();
                let Some(e) = old_table.value(id) else { continue };
                tr.store(Pool::Dtd, name.len(), direct)?;
                let (nid, _) = new_table.lookup_or_insert(
                    salt,
                    &mut tr.heap(direct),
                    name,
                    super::dtd::ENTITY_SIZE,
                    Entity::default,
                )?;
                if let Some(sys) = &e.system_id {
                    tr.store(Pool::Dtd, sys.len(), direct)?;
                    // C copies a base once per run of entities sharing the
                    // same pointer; the port copies it every time (at
                    // least C's bytes).
                    if let Some(b) = &e.base {
                        tr.store(Pool::Dtd, b.len(), direct)?;
                    }
                    if let Some(p) = &e.public_id {
                        tr.store(Pool::Dtd, p.len(), direct)?;
                    }
                } else {
                    // `poolCopyStringN`: the text without a NUL.
                    let (p, mut h) = tr.pool(Pool::Dtd, direct);
                    p.append(&mut h, e.text.as_ref().map_or(0, |t| t.len()))?;
                    p.finish();
                }
                if let Some(nt) = &e.notation {
                    tr.store(Pool::Dtd, nt.len(), direct)?;
                }
                if let Some(ne) = new_table.value_mut(nid) {
                    *ne = Entity {
                        text: if e.system_id.is_some() { None } else { e.text.clone() },
                        processed: 0,
                        system_id: e.system_id.clone(),
                        base: if e.system_id.is_some() { e.base.clone() } else { None },
                        public_id: if e.system_id.is_some() { e.public_id.clone() } else { None },
                        notation: e.notation.clone(),
                        open: false,
                        has_more: false,
                        is_param: e.is_param,
                        is_internal: e.is_internal,
                    };
                }
            }
        }
        n.param_entity_read = self.param_entity_read;
        n.keep_processing = self.keep_processing;
        n.has_param_entity_refs = self.has_param_entity_refs;
        n.standalone = self.standalone;
        // Don't want deep copying for scaffolding (C shares the pointer).
        n.in_eldecl = self.in_eldecl;
        n.scaffold = self.scaffold.clone();
        n.content_string_len = self.content_string_len;
        n.scaff_count = self.scaff_count;
        n.scaff_level = self.scaff_level;
        n.scaff_index = self.scaff_index.clone();
        Ok(n)
    }
}

impl Parser {
    /// `XML_ExternalEntityParserCreate`. Returns the child and the root
    /// tracker's byte count before it was created.
    fn external_entity_parser_create(&mut self, context: Option<&[u8]>, encoding: Option<&[u8]>) -> Option<(Parser, u64)> {
        let bytes_before = self.tracker.t.bytes_allocated;
        let direct = self.direct();
        let mut t = self.tracker.t;
        // C checks the new parser structure against the root parser's
        // tracker and then allocates its arrays, the DTD (general
        // entities) and the encoding name through it; the port charges its
        // creation bound with one check.
        if !t.charge_child_creation(encoding.map(<[u8]>::len), direct) {
            return None;
        }
        let ns_sep = self.ns.then_some(self.namespace_separator);
        let mut child = Parser::new(encoding, ns_sep, [0; 16]);
        child.tracker.t = t;
        child.salt = self.salt;
        child.handlers = self.handlers;
        child.ns_triplets = self.ns_triplets;
        child.reparse_deferral_enabled = self.reparse_deferral_enabled;
        child.param_entity_parsing = self.param_entity_parsing;
        child.prolog_state.in_entity_value = self.prolog_state.in_entity_value;
        child.decl_element_type = self.decl_element_type;
        child.accounting = self.accounting;
        child.is_child = true;
        match context {
            Some(context) => {
                let salt = self.salt;
                let dtd = self.dtd.copy_for_child(&salt, &mut child.tracker, direct).ok()?;
                child.dtd = dtd;
                child.tracker.scaff_size = self.tracker.scaff_size;
                child.tracker.scaff_index_size = self.tracker.scaff_index_size;
                if !child.set_context(context) {
                    return None;
                }
                child.processor = Processor::ExternalEntityInit;
            }
            None => {
                // The DTD instance is shared between the document's root
                // parser and external PE parsers.
                child.dtd = core::mem::take(&mut self.dtd);
                child.tracker.dtd_pool = core::mem::take(&mut self.tracker.dtd_pool);
                child.tracker.entity_value_pool = core::mem::take(&mut self.tracker.entity_value_pool);
                child.tracker.scaff_size = self.tracker.scaff_size;
                child.tracker.scaff_index_size = self.tracker.scaff_index_size;
                child.pending_value = core::mem::take(&mut self.pending_value);
                child.charged_len = self.charged_len;
                child.is_param_entity = true;
                child.prolog_state.init_external_entity();
                child.processor = Processor::ExternalParEntInit;
            }
        }
        Some((child, bytes_before))
    }

    /// `XML_ParserFree` of an external entity parser: the shared state goes
    /// back to the parent. A general entity parser's allocations are all
    /// released (C frees its DTD copy too), so the root tracker returns to
    /// its count before the creation. A parameter entity parser's DTD
    /// growth stays (shared); the port releases only its creation charge,
    /// keeping the child's other buffers charged (at least C's count).
    fn external_entity_parser_free(&mut self, child: &mut Parser, bytes_before: u64) {
        let mut t = child.tracker.t;
        if child.is_param_entity {
            self.dtd = core::mem::take(&mut child.dtd);
            self.tracker.dtd_pool = core::mem::take(&mut child.tracker.dtd_pool);
            self.tracker.entity_value_pool = core::mem::take(&mut child.tracker.entity_value_pool);
            self.tracker.scaff_size = child.tracker.scaff_size;
            self.tracker.scaff_index_size = child.tracker.scaff_index_size;
            self.pending_value = core::mem::take(&mut child.pending_value);
            self.charged_len = child.charged_len;
            t.release_child_creation(child.protocol_encoding_name.as_ref().map(Vec::len));
        } else {
            t.bytes_allocated = bytes_before;
        }
        self.tracker.t = t;
        self.accounting = child.accounting;
    }

    /// `setContext` (`xmlparse.c` 7771-7840): binds the prefixes of the
    /// context string (`prefix=uri`, `=uri` for the default namespace) as
    /// inherited bindings and marks the named general entities open.
    fn set_context(&mut self, context: &[u8]) -> bool {
        let salt = self.salt;
        let len = context.len();
        // `s` scans, `ctx` is the start of the current item, `temp` is
        // `m_tempPool`'s current string.
        let (mut s, mut ctx) = (0usize, 0usize);
        let mut temp: Vec<u8> = Vec::new();
        while ctx < len {
            let c = context.get(s).copied();
            let direct = self.direct();
            if c.is_none() || c == Some(CONTEXT_SEP) {
                if self.tracker.store_discard(Pool::Temp, temp.len(), direct).is_err() {
                    return false;
                }
                if let Some(id) = self.dtd.general_entities.find(&salt, &temp)
                    && let Some(e) = self.dtd.general_entities.value_mut(id)
                {
                    e.open = true;
                }
                if c.is_some() {
                    s += 1;
                }
                ctx = s;
                temp.clear();
            } else if c == Some(b'=') {
                let prefix = if temp.is_empty() {
                    PrefixId::Default
                } else {
                    if self.tracker.store_discard(Pool::Temp, temp.len(), direct).is_err() {
                        return false;
                    }
                    match self.dtd.lookup_prefix(&salt, &mut self.tracker, direct, &temp) {
                        Ok(p) => p,
                        Err(_) => return false,
                    }
                };
                temp.clear();
                let mut j = s + 1;
                while j < len && context.get(j) != Some(&CONTEXT_SEP) {
                    j += 1;
                }
                let uri = context.get(s + 1..j).unwrap_or_default();
                let direct = self.direct();
                if self.tracker.store_discard(Pool::Temp, uri.len(), direct).is_err() {
                    return false;
                }
                let mut inherited = self.inherited_bindings;
                let ok = self.add_binding(&mut NoEvents, prefix, None, uri, &mut inherited).is_ok();
                self.inherited_bindings = inherited;
                if !ok {
                    return false;
                }
                if j < len {
                    j += 1;
                }
                ctx = j;
                s = j;
            } else {
                temp.push(c.unwrap_or(0));
                s += 1;
            }
        }
        true
    }

    /// Scans one token in `state`, detecting the encoding first while it is
    /// still the initial encoding (C: `XmlContentTok` / `XmlPrologTok` on
    /// `m_encoding`, which `initScan` replaces).
    fn scan_any(&mut self, state: State, buf: &[u8], s: usize) -> Scanned {
        match self.enc {
            Enc::Init(init) => {
                let r = init_scan(&init, state, buf, s);
                if let Some(k) = r.encoding {
                    self.enc = Enc::Known(k);
                }
                r.scanned
            }
            Enc::Known(k) => k.tok(state, buf, s),
        }
    }

    /// `externalEntityInitProcessor`.
    pub(crate) fn external_entity_init_processor<H: Handler>(
        &mut self,
        h: &mut H,
        buf: &[u8],
        start: usize,
        end_ptr: &mut usize,
    ) -> Result<(), Stop> {
        self.initialize_encoding()?;
        self.processor = Processor::ExternalEntityInit2;
        self.external_entity_init_processor2(h, buf, start, end_ptr)
    }

    /// `externalEntityInitProcessor2`: skips a byte order mark.
    pub(crate) fn external_entity_init_processor2<H: Handler>(
        &mut self,
        h: &mut H,
        buf: &[u8],
        start: usize,
        end_ptr: &mut usize,
    ) -> Result<(), Stop> {
        let mut start = start;
        let r = self.scan_any(State::Content, buf, start);
        // XmlContentTok doesn't always set the last arg.
        let next = r.next.unwrap_or(start);
        match r.scan {
            Scan::Token(Tok::Bom) => {
                if !self.diff_tolerated(r.scan, next - start, Account::Direct) {
                    return Err(XmlError::AmplificationLimitBreach.into());
                }
                // If we are at the end of the buffer, this would cause the
                // next stage to pass control directly to doContent without
                // processing any text declaration.
                if next == buf.len() && !self.final_buffer {
                    *end_ptr = next;
                    return Ok(());
                }
                start = next;
            }
            Scan::Partial => {
                if !self.final_buffer {
                    *end_ptr = start;
                    return Ok(());
                }
                self.event_ptr = Some(start);
                return Err(XmlError::UnclosedToken.into());
            }
            Scan::PartialChar => {
                if !self.final_buffer {
                    *end_ptr = start;
                    return Ok(());
                }
                self.event_ptr = Some(start);
                return Err(XmlError::PartialChar.into());
            }
            _ => {}
        }
        self.processor = Processor::ExternalEntityInit3;
        self.external_entity_init_processor3(h, buf, start, end_ptr)
    }

    /// `externalEntityInitProcessor3`: the text declaration, then content
    /// at tag level 1.
    pub(crate) fn external_entity_init_processor3<H: Handler>(
        &mut self,
        h: &mut H,
        buf: &[u8],
        start: usize,
        end_ptr: &mut usize,
    ) -> Result<(), Stop> {
        let mut start = start;
        self.event_ptr = Some(start);
        let r = self.scan_any(State::Content, buf, start);
        let next = r.next.unwrap_or(start);
        // These bytes are accounted later in processXmlDecl or
        // externalEntityContentProcessor.
        self.event_end_ptr = Some(next);
        match r.scan {
            Scan::Token(Tok::XmlDecl) => {
                let Enc::Known(k) = self.enc else {
                    return Err(XmlError::UnexpectedState.into());
                };
                // A change of encoding takes effect for the following
                // tokens (C: `m_encoding` is replaced).
                let _ = with_known!(k, e => self.process_xml_decl(h, e, buf, start, next, true))?;
                match self.parsing {
                    Parsing::Suspended => {
                        *end_ptr = next;
                        return Ok(());
                    }
                    Parsing::Finished => return Err(XmlError::Aborted.into()),
                    Parsing::Parsing if self.reenter => return Err(XmlError::UnexpectedState.into()),
                    _ => start = next,
                }
            }
            Scan::Partial => {
                if !self.final_buffer {
                    *end_ptr = start;
                    return Ok(());
                }
                return Err(XmlError::UnclosedToken.into());
            }
            Scan::PartialChar => {
                if !self.final_buffer {
                    *end_ptr = start;
                    return Ok(());
                }
                return Err(XmlError::PartialChar.into());
            }
            _ => {}
        }
        self.processor = Processor::Content;
        self.tag_level = 1;
        self.content_processor(h, buf, start, end_ptr)
    }

    /// `doContent` while the encoding of an external entity is still the
    /// initial one: `initScanContent` returned no token (too few bytes:
    /// XML_TOK_NONE or XML_TOK_PARTIAL), so only these cases of
    /// `doContent` apply.
    #[allow(clippy::too_many_arguments)] // reason: mirrors the C signature of doContent.
    pub(crate) fn content_undecided<H: Handler>(
        &mut self,
        h: &mut H,
        init: InitEncoding,
        buf: &[u8],
        s: usize,
        next_ptr: &mut usize,
        have_more: bool,
        start_tag_level: i32,
    ) -> Result<(), Stop> {
        self.event_ptr = Some(s);
        let r = init_scan(&init, State::Content, buf, s);
        if let Some(k) = r.encoding {
            // Detected after all: the regular path.
            self.enc = Enc::Known(k);
            return with_known!(k, e => self.do_content(h, e, start_tag_level, buf, s, next_ptr, have_more, Account::Direct));
        }
        let next = r.scanned.next.unwrap_or(s);
        self.event_end_ptr = Some(next);
        match r.scanned.scan {
            Scan::None => {
                if have_more {
                    *next_ptr = s;
                    return Ok(());
                }
                if start_tag_level > 0 {
                    if self.tag_level != start_tag_level {
                        return Err(XmlError::AsyncEntity.into());
                    }
                    *next_ptr = s;
                    return Ok(());
                }
                Err(XmlError::NoElements.into())
            }
            Scan::Partial => {
                if have_more {
                    *next_ptr = s;
                    return Ok(());
                }
                Err(XmlError::UnclosedToken.into())
            }
            // `initScan` leaves the encoding undecided only with
            // XML_TOK_NONE or XML_TOK_PARTIAL.
            _ => Err(XmlError::UnexpectedState.into()),
        }
    }

    /// `externalParEntInitProcessor`.
    pub(crate) fn external_par_ent_init_processor<H: Handler>(
        &mut self,
        h: &mut H,
        buf: &[u8],
        s: usize,
        next_ptr: &mut usize,
    ) -> Result<(), Stop> {
        self.initialize_encoding()?;
        // We know now that XML_Parse(Buffer) has been called, so we
        // consider the external parameter entity read.
        self.dtd.param_entity_read = true;
        if self.prolog_state.in_entity_value {
            self.processor = Processor::EntityValueInit;
            self.entity_value_init_processor(h, buf, s, next_ptr)
        } else {
            self.processor = Processor::ExternalParEnt;
            self.external_par_ent_processor(h, buf, s, next_ptr)
        }
    }

    /// `entityValueInitProcessor`: an external parameter entity referenced
    /// inside an entity value. Scans prolog tokens up to the end of the
    /// input (only to find a text declaration), then stores the whole text
    /// as part of the entity value.
    pub(crate) fn entity_value_init_processor<H: Handler>(
        &mut self,
        h: &mut H,
        buf: &[u8],
        s: usize,
        next_ptr: &mut usize,
    ) -> Result<(), Stop> {
        let mut s = s;
        let mut start = s;
        self.event_ptr = Some(start);
        loop {
            let r = self.scan_any(State::Prolog, buf, start);
            let next = r.next.unwrap_or(start);
            // Except for a BOM, these bytes are accounted later in
            // storeEntityValue or processXmlDecl.
            self.event_end_ptr = Some(next);
            if r.scan.to_c() <= 0 {
                if !self.final_buffer && r.scan != Scan::Invalid {
                    *next_ptr = s;
                    return Ok(());
                }
                match r.scan {
                    Scan::Invalid => return Err(XmlError::InvalidToken.into()),
                    Scan::Partial => return Err(XmlError::UnclosedToken.into()),
                    Scan::PartialChar => return Err(XmlError::PartialChar.into()),
                    _ => {}
                }
                // Found end of entity value - can store it now.
                return self.store_external_value(h, buf, s);
            }
            match r.scan {
                Scan::Token(Tok::XmlDecl) => {
                    let Enc::Known(k) = self.enc else {
                        return Err(XmlError::UnexpectedState.into());
                    };
                    let _ = with_known!(k, e => self.process_xml_decl(h, e, buf, start, next, false))?;
                    if self.parsing == Parsing::Finished {
                        return Err(XmlError::Aborted.into());
                    }
                    *next_ptr = next;
                    // Stop scanning for text declaration - we found one.
                    self.processor = Processor::EntityValue;
                    return self.entity_value_processor(h, buf, next, next_ptr);
                }
                Scan::Token(Tok::Bom) => {
                    if !self.diff_tolerated(r.scan, next - s, Account::Direct) {
                        return Err(XmlError::AmplificationLimitBreach.into());
                    }
                    *next_ptr = next;
                    s = next;
                }
                // The start of what might be a normal tag, which is not
                // legal in a DTD context.
                Scan::Token(Tok::InstanceStart) => {
                    *next_ptr = next;
                    return Err(XmlError::Syntax.into());
                }
                _ => {}
            }
            start = next;
            self.event_ptr = Some(start);
        }
    }

    /// `entityValueProcessor`: like [`Parser::entity_value_init_processor`]
    /// after the text declaration.
    pub(crate) fn entity_value_processor<H: Handler>(
        &mut self,
        h: &mut H,
        buf: &[u8],
        s: usize,
        next_ptr: &mut usize,
    ) -> Result<(), Stop> {
        let mut start = s;
        loop {
            let r = self.scan_any(State::Prolog, buf, start);
            let next = r.next.unwrap_or(start);
            if r.scan.to_c() <= 0 {
                if !self.final_buffer && r.scan != Scan::Invalid {
                    *next_ptr = s;
                    return Ok(());
                }
                match r.scan {
                    Scan::Invalid => return Err(XmlError::InvalidToken.into()),
                    Scan::Partial => return Err(XmlError::UnclosedToken.into()),
                    Scan::PartialChar => return Err(XmlError::PartialChar.into()),
                    _ => {}
                }
                return self.store_external_value(h, buf, s);
            }
            if r.scan == Scan::Token(Tok::InstanceStart) {
                *next_ptr = next;
                return Err(XmlError::Syntax.into());
            }
            start = next;
        }
    }

    /// `storeEntityValue(parser, m_encoding, s, end, XML_ACCOUNT_DIRECT,
    /// NULL)` appending to the value the parent is building.
    fn store_external_value<H: Handler>(&mut self, h: &mut H, buf: &[u8], s: usize) -> Result<(), Stop> {
        let mut out = core::mem::take(&mut self.pending_value);
        let mut next = s;
        let r = match self.enc {
            Enc::Known(k) => {
                with_known!(k, e => self.store_entity_value(h, e, buf, s, &mut out, Account::Direct, &mut next))
            }
            // Nothing was scanned in a known encoding: the text is empty
            // (C tokenizes nothing).
            Enc::Init(_) => {
                let e = Normal::Utf8;
                self.store_entity_value(h, &e, buf, buf.len(), &mut out, Account::Direct, &mut next)
            }
        };
        self.pending_value = out;
        r
    }

    /// `externalParEntProcessor`: skips a byte order mark, then the
    /// external subset or parameter entity is parsed by `doProlog`.
    pub(crate) fn external_par_ent_processor<H: Handler>(
        &mut self,
        h: &mut H,
        buf: &[u8],
        s: usize,
        next_ptr: &mut usize,
    ) -> Result<(), Stop> {
        let mut s = s;
        let mut r = self.scan_any(State::Prolog, buf, s);
        if r.scan.to_c() <= 0 {
            if !self.final_buffer && r.scan != Scan::Invalid {
                *next_ptr = s;
                return Ok(());
            }
            match r.scan {
                Scan::Invalid => return Err(XmlError::InvalidToken.into()),
                Scan::Partial => return Err(XmlError::UnclosedToken.into()),
                Scan::PartialChar => return Err(XmlError::PartialChar.into()),
                _ => {}
            }
        } else if r.scan == Scan::Token(Tok::Bom) {
            // doProlog does not accept a BOM in an external subset: skip it
            // and account for its bytes.
            let next = r.next.unwrap_or(s);
            if !self.diff_tolerated(r.scan, next - s, Account::Direct) {
                return Err(XmlError::AmplificationLimitBreach.into());
            }
            s = next;
            r = self.scan_any(State::Prolog, buf, s);
        }
        self.processor = Processor::Prolog;
        let next = r.next.unwrap_or(s);
        let have_more = !self.final_buffer;
        let done = match self.enc {
            Enc::Known(k) => {
                with_known!(k, e => self.do_prolog(h, e, buf, s, r.scan, next, next_ptr, have_more, true, Account::Direct))?
            }
            // An empty entity: C runs doProlog with XML_TOK_NONE in the
            // initial encoding, whose only use is the role of the end of a
            // parameter entity.
            Enc::Init(_) => {
                let e = Normal::Utf8;
                self.do_prolog(h, &e, buf, s, r.scan, next, next_ptr, have_more, true, Account::Direct)?
            }
        };
        match done {
            PrologDone::Done => Ok(()),
            PrologDone::Rescan(at) => self.prolog_processor(h, buf, at, next_ptr),
        }
    }

    /// `ignoreSectionProcessor`.
    pub(crate) fn ignore_section_processor<H: Handler>(
        &mut self,
        h: &mut H,
        buf: &[u8],
        s: usize,
        next_ptr: &mut usize,
    ) -> Result<(), Stop> {
        let Enc::Known(k) = self.enc else {
            return Err(XmlError::UnexpectedState.into());
        };
        let have_more = !self.final_buffer;
        let mut start = Some(s);
        with_known!(k, e => self.do_ignore_section(e, buf, &mut start, next_ptr, have_more))?;
        if let Some(s) = start {
            self.processor = Processor::Prolog;
            return self.prolog_processor(h, buf, s, next_ptr);
        }
        Ok(())
    }

    /// `doIgnoreSection`: `*start` becomes `None` if the section is not yet
    /// closed, else the position after it.
    pub(crate) fn do_ignore_section<E: Encoding>(
        &mut self,
        enc: &E,
        buf: &[u8],
        start: &mut Option<usize>,
        next_ptr: &mut usize,
        have_more: bool,
    ) -> Result<(), Stop> {
        let Some(s) = *start else {
            return Err(XmlError::UnexpectedState.into());
        };
        self.ev(enc, Some(s));
        *start = None;
        let r = ignore_section_tok(enc, buf, s);
        let next = r.next.unwrap_or(s);
        if !self.diff_tolerated(r.scan, next.saturating_sub(s), Account::Direct) {
            return Err(XmlError::AmplificationLimitBreach.into());
        }
        self.ev_end(enc, Some(next));
        match r.scan {
            Scan::Token(Tok::IgnoreSect) => {
                *start = Some(next);
                *next_ptr = next;
                if self.parsing == Parsing::Finished {
                    return Err(XmlError::Aborted.into());
                }
                Ok(())
            }
            Scan::Invalid => {
                self.ev(enc, Some(next));
                Err(XmlError::InvalidToken.into())
            }
            Scan::PartialChar => {
                if have_more {
                    *next_ptr = s;
                    return Ok(());
                }
                Err(XmlError::PartialChar.into())
            }
            Scan::Partial | Scan::None => {
                if have_more {
                    *next_ptr = s;
                    return Ok(());
                }
                // XML_ERROR_UNCLOSED_IGNORE_SECTION
                Err(XmlError::Syntax.into())
            }
            _ => {
                self.ev(enc, Some(next));
                Err(XmlError::UnexpectedState.into())
            }
        }
    }
}
