//! Expansion of internal entities (`processEntity`,
//! `internalEntityProcessor`, the three open-entity lists of
//! `xmlparse.c` 6688-6823, `storeAttributeValue` 6838-6914 and
//! `callStoreEntityValue` 7300-7364).
//!
//! Like C since 2.7.0, expansion never recurses: an entity reference pushes
//! an [`OpenEntity`] and returns; the replacement text is then processed
//! piecewise by the loop that owns the list (the processor loop of
//! `callProcessor` for content and parameter entities, the loops of
//! [`Parser::store_attribute_value`] and [`Parser::call_store_entity_value`]
//! for attribute values and entity values). Nesting depth therefore costs
//! heap (one [`OpenEntity`] per level), never stack.
//!
//! Recursion (direct `a -> a` and indirect `a -> b -> a`) is detected
//! through [`super::dtd::Entity::open`], which stays set from the reference
//! until the entity and everything opened inside it are closed, exactly as
//! C does.

use alloc::sync::Arc;

use super::accounting::Account;
use super::dtd::EntityRef;
use super::error::{Stop, XmlError};
use super::prolog::PrologDone;
use super::tracker::{OPEN_ENTITY_SIZE, Pool};
use super::{Handler, Parser, Parsing, Processor};
use crate::tok::{Encoding, Normal, internal_encoding, prolog_tok};

/// `enum EntityType`: which list an open entity is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntityType {
    /// `ENTITY_INTERNAL`: content or a parameter entity in the prolog
    /// (`m_openInternalEntities`).
    Internal,
    /// `ENTITY_ATTRIBUTE`: an attribute value (`m_openAttributeEntities`).
    Attribute,
    /// `ENTITY_VALUE`: an entity value (`m_openValueEntities`).
    Value,
}

/// `OPEN_INTERNAL_ENTITY`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenEntity {
    /// `entity`
    pub entity: EntityRef,
    /// `startTagLevel`
    pub start_tag_level: i32,
    /// `betweenDecl`: a parameter entity referenced between declarations.
    pub between_decl: bool,
    /// `internalEventPtr`: offset into the replacement text.
    pub internal_event_ptr: Option<usize>,
    /// `internalEventEndPtr`
    pub internal_event_end_ptr: Option<usize>,
}

impl Parser {
    /// `XmlGetInternalEncoding` (`m_internalEncoding`).
    pub(crate) fn internal_encoding(&self) -> Normal {
        internal_encoding(self.ns)
    }

    /// `parser->m_openInternalEntities != NULL`.
    pub(crate) fn in_internal_entity(&self) -> bool {
        !self.open_internal_entities.is_empty()
    }

    /// `*eventPP = v` in `doContent`, `doCdataSection` and `doProlog`: the
    /// document's event pointer, or the innermost open internal entity's
    /// when `enc` is the internal encoding.
    pub(crate) fn ev<E: Encoding>(&mut self, enc: &E, v: Option<usize>) {
        if enc.is_internal() {
            if let Some(o) = self.open_internal_entities.last_mut() {
                o.internal_event_ptr = v;
            }
        } else {
            self.event_ptr = v;
        }
    }

    /// `*eventEndPP = v`.
    pub(crate) fn ev_end<E: Encoding>(&mut self, enc: &E, v: Option<usize>) {
        if enc.is_internal() {
            if let Some(o) = self.open_internal_entities.last_mut() {
                o.internal_event_end_ptr = v;
            }
        } else {
            self.event_end_ptr = v;
        }
    }

    /// `*eventEndPP`.
    pub(crate) fn ev_end_get<E: Encoding>(&self, enc: &E) -> Option<usize> {
        if enc.is_internal() {
            self.open_internal_entities
                .last()
                .and_then(|o| o.internal_event_end_ptr)
        } else {
            self.event_end_ptr
        }
    }

    /// `if (enc == parser->m_encoding) parser->m_eventPtr = v;`: the
    /// guarded writes of `storeAtts`, `appendAttributeValue` and
    /// `storeEntityValue`.
    pub(crate) fn ev_doc<E: Encoding>(&mut self, enc: &E, v: Option<usize>) {
        if !enc.is_internal() {
            self.event_ptr = v;
        }
    }

    /// `processEntity`: opens `r` on the list of `ty`.
    pub(crate) fn process_entity(&mut self, r: EntityRef, between_decl: bool, ty: EntityType) -> Result<(), Stop> {
        if ty == EntityType::Internal {
            self.processor = Processor::InternalEntity;
        }
        // C reuses a freed OPEN_INTERNAL_ENTITY or allocates one; the port
        // keeps the count of reusable ones (`m_freeEntities`).
        if self.free_entities == 0 {
            let direct = self.direct();
            if !self.tracker.t.alloc(OPEN_ENTITY_SIZE, direct) {
                return Err(XmlError::NoMemory.into());
            }
        }
        self.free_entities = self.free_entities.saturating_sub(1);
        if let Some(e) = self.dtd.entity_mut(r) {
            e.open = true;
            e.has_more = true;
            e.processed = 0;
        }
        let open = OpenEntity {
            entity: r,
            start_tag_level: self.tag_level,
            between_decl,
            internal_event_ptr: None,
            internal_event_end_ptr: None,
        };
        match ty {
            EntityType::Internal => self.open_internal_entities.push(open),
            EntityType::Attribute => self.open_attribute_entities.push(open),
            EntityType::Value => self.open_value_entities.push(open),
        }
        // Only internal entities make use of the reenter flag therefore no
        // need to set it for other entity types.
        if ty == EntityType::Internal {
            self.reenter = true;
        }
        Ok(())
    }

    /// `processEntity(parser, entity, betweenDecl, ENTITY_INTERNAL)` for a
    /// parameter entity referenced in the prolog.
    pub(crate) fn open_param_entity(&mut self, r: EntityRef, between_decl: bool) -> Result<(), Stop> {
        self.process_entity(r, between_decl, EntityType::Internal)
    }

    /// `processEntity(parser, entity, XML_FALSE, ENTITY_INTERNAL)` for a
    /// general entity referenced in content.
    pub(crate) fn open_content_entity(&mut self, r: EntityRef) -> Result<(), Stop> {
        self.process_entity(r, false, EntityType::Internal)
    }

    /// Removes the head of a list (C: `entity->open = false`, unlink, put
    /// the structure on `m_freeEntities`).
    fn close_entity(&mut self, ty: EntityType) -> Option<OpenEntity> {
        let o = match ty {
            EntityType::Internal => self.open_internal_entities.pop(),
            EntityType::Attribute => self.open_attribute_entities.pop(),
            EntityType::Value => self.open_value_entities.pop(),
        }?;
        if let Some(e) = self.dtd.entity_mut(o.entity) {
            e.open = false;
        }
        self.free_entities += 1;
        Some(o)
    }

    /// The text, `processed` offset, `hasMore` and `is_param` of `r`.
    fn entity_progress(&self, r: EntityRef) -> (Arc<[u8]>, usize, bool, bool) {
        match self.dtd.entity(r) {
            Some(e) => (
                e.text.clone().unwrap_or_else(|| Arc::from(&[][..])),
                e.processed,
                e.has_more,
                e.is_param,
            ),
            None => (Arc::from(&[][..]), 0, false, false),
        }
    }

    fn set_processed(&mut self, r: EntityRef, processed: usize) {
        if let Some(e) = self.dtd.entity_mut(r) {
            e.processed = processed;
        }
    }

    fn set_has_more(&mut self, r: EntityRef, has_more: bool) {
        if let Some(e) = self.dtd.entity_mut(r) {
            e.has_more = has_more;
        }
    }

    /// `internalEntityProcessor`: processes the innermost open internal
    /// entity until it opens another one, or closes it.
    pub(crate) fn internal_entity_processor<H: Handler>(&mut self, h: &mut H) -> Result<(), Stop> {
        let Some(open) = self.open_internal_entities.last() else {
            return Err(XmlError::UnexpectedState.into());
        };
        let (r, start_tag_level) = (open.entity, open.start_tag_level);
        let (text, processed, has_more, is_param) = self.entity_progress(r);
        if has_more {
            let enc = self.internal_encoding();
            // Set a safe default value in case 'next' does not get set.
            let mut next = processed;
            if is_param {
                let t = prolog_tok(&enc, &text, processed);
                let tok_next = t.next.unwrap_or(processed);
                let done = self.do_prolog(
                    h,
                    &enc,
                    &text,
                    processed,
                    t.scan,
                    tok_next,
                    &mut next,
                    false,
                    false,
                    Account::EntityExpansion,
                )?;
                // An encoding change needs an XML declaration, whose role
                // the prolog state machine only produces at the start of
                // the document entity.
                if let PrologDone::Rescan(_) = done {
                    return Err(XmlError::UnexpectedState.into());
                }
            } else {
                self.do_content(
                    h,
                    &enc,
                    start_tag_level,
                    &text,
                    processed,
                    &mut next,
                    false,
                    Account::EntityExpansion,
                )?;
            }
            // Check if entity is complete, if not, mark down how much of it
            // is processed.
            if next != text.len()
                && (self.parsing == Parsing::Suspended
                    || (self.parsing == Parsing::Parsing && self.reenter))
            {
                self.set_processed(r, next);
                return Ok(());
            }
            // Entity is complete. We cannot close it here since we need to
            // first process its possible inner entities (which are added to
            // the open internal entities during doProlog or doContent
            // calls above).
            self.set_has_more(r, false);
            if !is_param && start_tag_level != self.tag_level {
                return Err(XmlError::AsyncEntity.into());
            }
            self.reenter = true;
            return Ok(());
        }
        // Remove fully processed openEntity from open entity list.
        self.close_entity(EntityType::Internal);
        if self.open_internal_entities.is_empty() {
            self.processor = if is_param {
                Processor::Prolog
            } else {
                Processor::Content
            };
        }
        self.reenter = true;
        Ok(())
    }

    /// `storeAttributeValue`: the normalised value of `buf[ptr..end]`,
    /// with the replacement text of every referenced internal entity.
    /// `in_prolog` says whether C would store into `dtd->pool` (a default
    /// value in an attribute-list declaration) rather than the temporary
    /// pool (a start tag), which decides how undeclared entities are
    /// checked.
    #[allow(clippy::too_many_arguments)] // reason: mirrors the C signature of storeAttributeValue.
    pub(crate) fn store_attribute_value<E: Encoding>(
        &mut self,
        enc: &E,
        buf: &[u8],
        is_cdata: bool,
        ptr: usize,
        end: usize,
        account: Account,
        in_prolog: bool,
    ) -> Result<alloc::vec::Vec<u8>, Stop> {
        let mut out = alloc::vec::Vec::new();
        self.charged_len = 0;
        let doc = buf.get(..end).unwrap_or(buf);
        let mut next = ptr;
        loop {
            let result = if let Some(open) = self.open_attribute_entities.last() {
                let r = open.entity;
                let (text, processed, has_more, _) = self.entity_progress(r);
                if has_more {
                    let ienc = self.internal_encoding();
                    // Set a safe default value in case 'next' does not get
                    // set.
                    let mut next_in_entity = processed;
                    self.append_attribute_value(
                        &ienc,
                        &text,
                        is_cdata,
                        processed,
                        &mut out,
                        Account::EntityExpansion,
                        in_prolog,
                        &mut next_in_entity,
                    )?;
                    // Check if entity is complete, if not, mark down how
                    // much of it is processed.
                    if next_in_entity < text.len() {
                        self.set_processed(r, next_in_entity);
                    } else {
                        // Entity is complete. We cannot close it here since
                        // we need to first process its possible inner
                        // entities.
                        self.set_has_more(r, false);
                    }
                    continue;
                }
                // Remove fully processed openEntity from open entity list.
                self.close_entity(EntityType::Attribute);
                Ok(())
            } else {
                self.append_attribute_value(enc, doc, is_cdata, next, &mut out, account, in_prolog, &mut next)
            };
            result?;
            // Break if there is nothing left to process.
            if self.open_attribute_entities.is_empty() && next == doc.len() {
                break;
            }
        }
        if !is_cdata && out.last() == Some(&0x20) {
            out.pop();
        }
        // `poolAppendChar(pool, '\0')`, then the caller's `poolFinish`.
        let pool = if in_prolog { Pool::Dtd } else { Pool::Temp };
        self.charge_current(pool, out.len())?;
        self.finish_current(pool, true, true)?;
        Ok(out)
    }

    /// `callStoreEntityValue`: the replacement text of the entity value
    /// `buf[ptr..end]` (`XML_ACCOUNT_NONE` for the document entity), with
    /// the replacement text of every parameter entity it references
    /// (which C only expands inside parameter entities), appended to
    /// `out`.
    pub(crate) fn call_store_entity_value<E: Encoding>(
        &mut self,
        enc: &E,
        buf: &[u8],
        ptr: usize,
        end: usize,
        out: &mut alloc::vec::Vec<u8>,
        account: Account,
    ) -> Result<(), Stop> {
        let doc = buf.get(..end).unwrap_or(buf);
        let mut next = ptr;
        loop {
            let result = if let Some(open) = self.open_value_entities.last() {
                let r = open.entity;
                let (text, processed, has_more, _) = self.entity_progress(r);
                if has_more {
                    let ienc = self.internal_encoding();
                    let mut next_in_entity = processed;
                    self.store_entity_value(
                        &ienc,
                        &text,
                        processed,
                        out,
                        Account::EntityExpansion,
                        &mut next_in_entity,
                    )?;
                    if next_in_entity != text.len() {
                        self.set_processed(r, next_in_entity);
                    } else {
                        self.set_has_more(r, false);
                    }
                    continue;
                }
                self.close_entity(EntityType::Value);
                Ok(())
            } else {
                self.store_entity_value(enc, doc, next, out, account, &mut next)
            };
            result?;
            if self.open_value_entities.is_empty() && next == doc.len() {
                break;
            }
        }
        Ok(())
    }
}
