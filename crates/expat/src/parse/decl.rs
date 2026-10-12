//! The roles of a document type declaration in `doProlog`: the doctype
//! itself, element declarations with their content models, attribute-list
//! declarations with default values (`defineAttribute`), entity
//! declarations with their values (`storeEntityValue`), notations and
//! parameter entity references.

use alloc::vec::Vec;

use super::accounting::Account;
use super::dtd::EntityRef;
use super::entity::EntityType;
use super::error::{Stop, XmlError};
use super::model::{ContentModel, ContentQuant, ContentType};
use super::prolog::to_utf8;
use super::{Handler, ParamEntityParsing, Parser};
use crate::encode::utf8_encode;
use crate::role::Role;
use crate::tok::{
    Encoding, Scan, Tok, char_ref_number, entity_value_tok, is_public_id, predefined_entity_name,
};

/// `externalSubsetName`: the name of the parameter entity that stands for
/// the external subset.
pub(crate) const EXTERNAL_SUBSET_NAME: &[u8] = b"#";

/// `normalizePublicId`: white space (space, CR, LF) runs become one
/// space; leading and trailing white space is removed.
pub(crate) fn normalize_public_id(s: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    for &c in s {
        match c {
            0x20 | 0xD | 0xA => {
                if !out.is_empty() && out.last() != Some(&0x20) {
                    out.push(0x20);
                }
            }
            _ => out.push(c),
        }
    }
    if out.last() == Some(&0x20) {
        out.pop();
    }
    out
}

/// The bytes `buf[from..to]` (empty if out of range).
fn bytes(buf: &[u8], from: usize, to: usize) -> &[u8] {
    buf.get(from..to).unwrap_or_default()
}

/// Does the attribute type need a closing parenthesis (enumerated or
/// `NOTATION` type)?
fn is_enumerated_type(t: &[u8]) -> bool {
    t.first() == Some(&b'(') || t.starts_with(b"NO")
}

/// The fixed attribute type names of `doProlog`.
fn attribute_type_name(role: Role) -> Option<&'static [u8]> {
    Some(match role {
        Role::AttributeTypeCdata => b"CDATA",
        Role::AttributeTypeId => b"ID",
        Role::AttributeTypeIdref => b"IDREF",
        Role::AttributeTypeIdrefs => b"IDREFS",
        Role::AttributeTypeEntity => b"ENTITY",
        Role::AttributeTypeEntities => b"ENTITIES",
        Role::AttributeTypeNmtoken => b"NMTOKEN",
        Role::AttributeTypeNmtokens => b"NMTOKENS",
        _ => return None,
    })
}

/// The quantifier of a content-model role.
fn quant_of(role: Role) -> ContentQuant {
    match role {
        Role::ContentElementOpt | Role::GroupCloseOpt => ContentQuant::Opt,
        Role::ContentElementRep | Role::GroupCloseRep => ContentQuant::Rep,
        Role::ContentElementPlus | Role::GroupClosePlus => ContentQuant::Plus,
        _ => ContentQuant::None,
    }
}

impl Parser {
    /// `getElementType` for `buf[s..end]`.
    fn get_element_type<E: Encoding>(
        &mut self,
        enc: &E,
        buf: &[u8],
        s: usize,
        end: usize,
    ) -> Result<crate::hash::EntryId, Stop> {
        let name = to_utf8(enc, bytes(buf, s, end));
        let salt = self.salt;
        self.dtd
            .element_type(&salt, self.ns, &name)
            .map_err(|_| XmlError::NoMemory.into())
    }

    /// The literal `buf[s..next]` without its quotes, converted.
    fn literal<E: Encoding>(enc: &E, buf: &[u8], s: usize, next: usize) -> Vec<u8> {
        let m = E::MINBPC;
        to_utf8(enc, bytes(buf, s + m, next.saturating_sub(m)))
    }

    /// `XmlIsPublicId`; on failure `m_eventPtr` points at the bad
    /// character.
    fn check_public_id<E: Encoding>(
        &mut self,
        enc: &E,
        buf: &[u8],
        s: usize,
        next: usize,
    ) -> Result<(), Stop> {
        if let Err(bad) = is_public_id(enc, bytes(buf, 0, next), s) {
            self.ev(enc, Some(bad));
            return Err(XmlError::Publicid.into());
        }
        Ok(())
    }

    /// The name of the element type being declared.
    fn decl_element_name(&self) -> Vec<u8> {
        self.decl_element_type
            .and_then(|id| self.dtd.element_types.key(id))
            .unwrap_or_default()
            .to_vec()
    }

    /// Calls the attribute-list declaration handler for the attribute
    /// being declared (C: the common tail of the four value roles).
    fn report_attlist<H: Handler>(&mut self, h: &mut H, dflt: Option<&[u8]>, is_required: bool) {
        let Some(t) = self.decl_attribute_type.as_mut() else {
            return;
        };
        if is_enumerated_type(t) {
            t.push(b')');
        }
        let elname = self.decl_element_name();
        let attname = self
            .decl_attribute_id
            .and_then(|id| self.dtd.attribute_ids.key(id))
            .unwrap_or_default()
            .to_vec();
        let att_type = self.decl_attribute_type.clone().unwrap_or_default();
        h.attlist_decl(&elname, &attname, &att_type, dflt, is_required);
    }

    /// Handles a role of a document type declaration (the cases of
    /// `doProlog`'s switch from `XML_ROLE_DOCTYPE_NAME` on). `s..next` is
    /// the token, `t` its kind.
    #[allow(clippy::too_many_arguments)] // reason: the state doProlog passes to each case.
    pub(crate) fn dtd_role<E: Encoding, H: Handler>(
        &mut self,
        h: &mut H,
        enc: &E,
        buf: &[u8],
        role: Role,
        t: Tok,
        s: usize,
        next: usize,
        allow_closing_doctype: bool,
    ) -> Result<(), Stop> {
        let m = E::MINBPC;
        let salt = self.salt;
        let no_mem = |_| Stop::Error(XmlError::NoMemory);
        match role {
            Role::DoctypeName => {
                if self.handlers.start_doctype_decl {
                    self.doctype_name = Some(to_utf8(enc, bytes(buf, s, next)));
                    self.doctype_pubid = None;
                }
                // Always initialize to NULL.
                self.doctype_sysid = None;
            }
            Role::DoctypeInternalSubset => {
                if self.handlers.start_doctype_decl {
                    let name = self.doctype_name.take().unwrap_or_default();
                    h.start_doctype_decl(
                        &name,
                        self.doctype_sysid.as_deref(),
                        self.doctype_pubid.as_deref(),
                        true,
                    );
                    // C clears the temporary pool here; the system id
                    // pointer stays non-NULL and is only tested later.
                }
            }
            Role::DoctypePublicId | Role::EntityPublicId => {
                if role == Role::DoctypePublicId {
                    self.use_foreign_dtd = false;
                    let (r, _) = self
                        .dtd
                        .declare_entity(&salt, true, EXTERNAL_SUBSET_NAME)
                        .map_err(no_mem)?;
                    self.decl_entity = Some(r);
                    self.dtd.has_param_entity_refs = true;
                }
                self.check_public_id(enc, buf, s, next)?;
                if role == Role::DoctypePublicId && self.handlers.start_doctype_decl {
                    self.doctype_pubid =
                        Some(normalize_public_id(&Self::literal(enc, buf, s, next)));
                }
                if self.dtd.keep_processing
                    && let Some(r) = self.decl_entity
                {
                    let id = normalize_public_id(&Self::literal(enc, buf, s, next));
                    if let Some(e) = self.dtd.entity_mut(r) {
                        e.public_id = Some(id);
                    }
                }
            }
            Role::DoctypeClose => {
                if !allow_closing_doctype {
                    // Must not close doctype from within expanded
                    // parameter entities.
                    return Err(XmlError::InvalidToken.into());
                }
                if let Some(name) = self.doctype_name.take() {
                    h.start_doctype_decl(
                        &name,
                        self.doctype_sysid.as_deref(),
                        self.doctype_pubid.as_deref(),
                        false,
                    );
                }
                if self.doctype_sysid.is_some() || self.use_foreign_dtd {
                    // Reading the external subset needs an external
                    // entity reference handler (public API, module 12),
                    // without one C only records the reference.
                    self.dtd.has_param_entity_refs = true;
                    self.use_foreign_dtd = false;
                }
                if self.handlers.end_doctype_decl {
                    h.end_doctype_decl();
                }
            }
            Role::AttlistElementName => {
                self.decl_element_type = Some(self.get_element_type(enc, buf, s, next)?);
            }
            Role::AttributeName => {
                let name = to_utf8(enc, bytes(buf, s, next));
                self.decl_attribute_id =
                    Some(self.dtd.attribute_id(&salt, self.ns, &name).map_err(no_mem)?);
                self.decl_attribute_is_cdata = false;
                self.decl_attribute_type = None;
                self.decl_attribute_is_id = false;
            }
            Role::AttributeTypeCdata
            | Role::AttributeTypeId
            | Role::AttributeTypeIdref
            | Role::AttributeTypeIdrefs
            | Role::AttributeTypeEntity
            | Role::AttributeTypeEntities
            | Role::AttributeTypeNmtoken
            | Role::AttributeTypeNmtokens => {
                match role {
                    Role::AttributeTypeCdata => self.decl_attribute_is_cdata = true,
                    Role::AttributeTypeId => self.decl_attribute_is_id = true,
                    _ => {}
                }
                self.decl_attribute_type = attribute_type_name(role).map(<[u8]>::to_vec);
            }
            Role::AttributeEnumValue | Role::AttributeNotationValue => {
                if self.dtd.keep_processing && self.handlers.attlist_decl {
                    let prefix: &[u8] = if self.decl_attribute_type.is_some() {
                        b"|"
                    } else if role == Role::AttributeNotationValue {
                        b"NOTATION("
                    } else {
                        b"("
                    };
                    let value = to_utf8(enc, bytes(buf, s, next));
                    let t = self.decl_attribute_type.get_or_insert_with(Vec::new);
                    t.extend_from_slice(prefix);
                    t.extend_from_slice(&value);
                }
            }
            Role::ImpliedAttributeValue | Role::RequiredAttributeValue => {
                if self.dtd.keep_processing {
                    if let (Some(et), Some(att)) = (self.decl_element_type, self.decl_attribute_id) {
                        self.dtd.define_attribute(
                            et,
                            att,
                            self.decl_attribute_is_cdata,
                            self.decl_attribute_is_id,
                            None,
                        );
                    }
                    if self.handlers.attlist_decl && self.decl_attribute_type.is_some() {
                        self.ev_end(enc, Some(s));
                        self.report_attlist(h, None, role == Role::RequiredAttributeValue);
                    }
                }
            }
            Role::DefaultAttributeValue | Role::FixedAttributeValue => {
                if self.dtd.keep_processing {
                    let value = self.store_attribute_value(
                        enc,
                        buf,
                        self.decl_attribute_is_cdata,
                        s + m,
                        next - m,
                        Account::None,
                        true,
                    )?;
                    // ID attributes aren't allowed to have a default.
                    if let (Some(et), Some(att)) = (self.decl_element_type, self.decl_attribute_id) {
                        self.dtd.define_attribute(
                            et,
                            att,
                            self.decl_attribute_is_cdata,
                            false,
                            Some(value.clone()),
                        );
                    }
                    if self.handlers.attlist_decl && self.decl_attribute_type.is_some() {
                        self.ev_end(enc, Some(s));
                        self.report_attlist(h, Some(&value), role == Role::FixedAttributeValue);
                    }
                }
            }
            Role::EntityValue => {
                if self.dtd.keep_processing {
                    // This stores the replacement text of m_declEntity.
                    let mut text = Vec::new();
                    let result = self.call_store_entity_value(enc, buf, s + m, next - m, &mut text, Account::None);
                    if let Some(r) = self.decl_entity {
                        if i32::try_from(text.len()).is_err() {
                            return Err(XmlError::NoMemory.into());
                        }
                        let text: alloc::sync::Arc<[u8]> = text.into();
                        let mut info = None;
                        if let Some(e) = self.dtd.entity_mut(r) {
                            e.text = Some(text.clone());
                            info = Some(e.is_param);
                        }
                        if self.handlers.entity_decl
                            && let Some(is_param) = info
                        {
                            self.ev_end(enc, Some(s));
                            let name = self.dtd.entity_name(r).to_vec();
                            h.entity_decl(
                                &name,
                                is_param,
                                Some(&text),
                                self.cur_base.as_deref(),
                                None,
                                None,
                                None,
                            );
                        }
                    }
                    result?;
                }
            }
            Role::DoctypeSystemId | Role::EntitySystemId => {
                if role == Role::DoctypeSystemId {
                    self.use_foreign_dtd = false;
                    self.dtd.has_param_entity_refs = true;
                    self.doctype_sysid = Some(if self.handlers.start_doctype_decl {
                        Self::literal(enc, buf, s, next)
                    } else {
                        // Makes m_doctypeSysid non-NULL when no start
                        // handler is set.
                        EXTERNAL_SUBSET_NAME.to_vec()
                    });
                    // The not-standalone handler (public API, module 12)
                    // is not set.
                    if self.decl_entity.is_none() {
                        let (r, _) = self
                            .dtd
                            .declare_entity(&salt, true, EXTERNAL_SUBSET_NAME)
                            .map_err(no_mem)?;
                        if let Some(e) = self.dtd.entity_mut(r) {
                            e.public_id = None;
                        }
                        self.decl_entity = Some(r);
                    }
                }
                if self.dtd.keep_processing
                    && let Some(r) = self.decl_entity
                {
                    let sysid = Self::literal(enc, buf, s, next);
                    let base = self.cur_base.clone();
                    if let Some(e) = self.dtd.entity_mut(r) {
                        e.system_id = Some(sysid);
                        e.base = base;
                    }
                }
            }
            Role::EntityComplete => {
                if self.dtd.keep_processing
                    && let Some(r) = self.decl_entity
                    && self.handlers.entity_decl
                {
                    self.ev_end(enc, Some(s));
                    let name = self.dtd.entity_name(r).to_vec();
                    if let Some(e) = self.dtd.entity(r) {
                        h.entity_decl(
                            &name,
                            e.is_param,
                            None,
                            e.base.as_deref(),
                            e.system_id.as_deref(),
                            e.public_id.as_deref(),
                            None,
                        );
                    }
                }
            }
            Role::EntityNotationName => {
                if self.dtd.keep_processing
                    && let Some(r) = self.decl_entity
                {
                    let notation = to_utf8(enc, bytes(buf, s, next));
                    if let Some(e) = self.dtd.entity_mut(r) {
                        e.notation = Some(notation);
                    }
                    let name = self.dtd.entity_name(r).to_vec();
                    if self.handlers.unparsed_entity_decl || self.handlers.entity_decl {
                        self.ev_end(enc, Some(s));
                    }
                    if let Some(e) = self.dtd.entity(r) {
                        if self.handlers.unparsed_entity_decl {
                            h.unparsed_entity_decl(
                                &name,
                                e.base.as_deref(),
                                e.system_id.as_deref(),
                                e.public_id.as_deref(),
                                e.notation.as_deref().unwrap_or_default(),
                            );
                        } else if self.handlers.entity_decl {
                            h.entity_decl(
                                &name,
                                false,
                                None,
                                e.base.as_deref(),
                                e.system_id.as_deref(),
                                e.public_id.as_deref(),
                                e.notation.as_deref(),
                            );
                        }
                    }
                }
            }
            Role::GeneralEntityName | Role::ParamEntityName => {
                let is_param = role == Role::ParamEntityName;
                if !is_param && predefined_entity_name(enc, bytes(buf, 0, next), s).is_some() {
                    self.decl_entity = None;
                } else if self.dtd.keep_processing {
                    let name = to_utf8(enc, bytes(buf, s, next));
                    let (r, created) = self
                        .dtd
                        .declare_entity(&salt, is_param, &name)
                        .map_err(no_mem)?;
                    if created {
                        // If we have a parent parser or are reading an
                        // internal parameter entity, then the entity
                        // declaration is not considered "internal".
                        let is_internal = !self.in_internal_entity();
                        if let Some(e) = self.dtd.entity_mut(r) {
                            e.public_id = None;
                            e.is_param = is_param;
                            e.is_internal = is_internal;
                        }
                        self.decl_entity = Some(r);
                    } else {
                        self.decl_entity = None;
                    }
                } else {
                    self.decl_entity = None;
                }
            }
            Role::NotationName => {
                self.decl_notation_public_id = None;
                self.decl_notation_name = None;
                if self.handlers.notation_decl {
                    self.decl_notation_name = Some(to_utf8(enc, bytes(buf, s, next)));
                }
            }
            Role::NotationPublicId => {
                self.check_public_id(enc, buf, s, next)?;
                // A name is only stored when the handler is set.
                if self.decl_notation_name.is_some() {
                    self.decl_notation_public_id =
                        Some(normalize_public_id(&Self::literal(enc, buf, s, next)));
                }
            }
            Role::NotationSystemId => {
                if let Some(name) = self.decl_notation_name.clone()
                    && self.handlers.notation_decl
                {
                    let sysid = Self::literal(enc, buf, s, next);
                    self.ev_end(enc, Some(s));
                    h.notation_decl(
                        &name,
                        self.cur_base.as_deref(),
                        Some(&sysid),
                        self.decl_notation_public_id.as_deref(),
                    );
                }
            }
            Role::NotationNoSystemId => {
                if let Some(pubid) = self.decl_notation_public_id.clone()
                    && self.handlers.notation_decl
                {
                    let name = self.decl_notation_name.clone().unwrap_or_default();
                    self.ev_end(enc, Some(s));
                    h.notation_decl(&name, self.cur_base.as_deref(), None, Some(&pubid));
                }
            }
            Role::GroupOpen => {
                let level = self.prolog_state.level as usize;
                if self.group_connector.len() <= level {
                    self.group_connector.resize(level + 1, 0);
                }
                if let Some(c) = self.group_connector.get_mut(level) {
                    *c = 0;
                }
                if self.dtd.in_eldecl {
                    let myindex = self.dtd.next_scaffold_part();
                    let lvl = self.dtd.scaff_level;
                    if self.dtd.scaff_index.len() <= lvl {
                        self.dtd.scaff_index.resize(lvl + 1, 0);
                    }
                    if let Some(i) = self.dtd.scaff_index.get_mut(lvl) {
                        *i = myindex;
                    }
                    self.dtd.scaff_level += 1;
                    if let Some(sc) = self.dtd.scaffold.get_mut(myindex) {
                        sc.kind = ContentType::Seq;
                    }
                }
            }
            Role::GroupSequence | Role::GroupChoice => {
                let level = self.prolog_state.level as usize;
                let cur = self.group_connector.get(level).copied().unwrap_or(0);
                if role == Role::GroupSequence {
                    if cur == b'|' {
                        return Err(XmlError::Syntax.into());
                    }
                } else {
                    if cur == b',' {
                        return Err(XmlError::Syntax.into());
                    }
                    if self.dtd.in_eldecl && cur == 0 {
                        let top = self.current_scaffold_group();
                        if let Some(sc) = self.dtd.scaffold.get_mut(top)
                            && sc.kind != ContentType::Mixed
                        {
                            sc.kind = ContentType::Choice;
                        }
                    }
                }
                if let Some(c) = self.group_connector.get_mut(level) {
                    *c = if role == Role::GroupSequence { b',' } else { b'|' };
                }
            }
            Role::ParamEntityRef | Role::InnerParamEntityRef => {
                self.param_entity_ref(h, enc, buf, role, s, next)?;
            }
            Role::ElementName => {
                if self.handlers.element_decl {
                    self.decl_element_type = Some(self.get_element_type(enc, buf, s, next)?);
                    self.dtd.scaff_level = 0;
                    self.dtd.scaff_count = 0;
                    self.dtd.in_eldecl = true;
                }
            }
            Role::ContentAny | Role::ContentEmpty => {
                if self.dtd.in_eldecl {
                    if self.handlers.element_decl {
                        let kind = if role == Role::ContentAny {
                            ContentType::Any
                        } else {
                            ContentType::Empty
                        };
                        self.ev_end(enc, Some(s));
                        let name = self.decl_element_name();
                        h.element_decl(&name, &ContentModel::leaf(kind));
                    }
                    self.dtd.in_eldecl = false;
                }
            }
            Role::ContentPcdata => {
                if self.dtd.in_eldecl {
                    let top = self.current_scaffold_group();
                    if let Some(sc) = self.dtd.scaffold.get_mut(top) {
                        sc.kind = ContentType::Mixed;
                    }
                }
            }
            Role::ContentElement
            | Role::ContentElementOpt
            | Role::ContentElementRep
            | Role::ContentElementPlus => {
                if self.dtd.in_eldecl {
                    let quant = quant_of(role);
                    let nxt = if quant == ContentQuant::None { next } else { next - m };
                    let myindex = self.dtd.next_scaffold_part();
                    let el = self.get_element_type(enc, buf, s, nxt)?;
                    let name = self.dtd.element_types.key(el).unwrap_or_default().to_vec();
                    // Detect and prevent integer overflow.
                    let name_len = u32::try_from(name.len() + 1).unwrap_or(u32::MAX);
                    if name_len > u32::MAX - self.dtd.content_string_len {
                        return Err(XmlError::NoMemory.into());
                    }
                    self.dtd.content_string_len += name_len;
                    if let Some(sc) = self.dtd.scaffold.get_mut(myindex) {
                        sc.kind = ContentType::Name;
                        sc.quant = quant;
                        sc.name = Some(name);
                    }
                }
            }
            Role::GroupClose | Role::GroupCloseOpt | Role::GroupCloseRep | Role::GroupClosePlus => {
                if self.dtd.in_eldecl {
                    self.dtd.scaff_level = self.dtd.scaff_level.saturating_sub(1);
                    let idx = self
                        .dtd
                        .scaff_index
                        .get(self.dtd.scaff_level)
                        .copied()
                        .unwrap_or(0);
                    if let Some(sc) = self.dtd.scaffold.get_mut(idx) {
                        sc.quant = quant_of(role);
                    }
                    if self.dtd.scaff_level == 0 {
                        if self.handlers.element_decl {
                            let model = self.dtd.build_model();
                            self.ev_end(enc, Some(s));
                            let name = self.decl_element_name();
                            h.element_decl(&name, &model);
                        }
                        self.dtd.in_eldecl = false;
                        self.dtd.content_string_len = 0;
                    }
                }
            }
            // Only the default handler (public API) sees these.
            Role::EntityNone | Role::NotationNone | Role::AttlistNone | Role::ElementNone => {}
            // Conditional sections and text declarations exist only in
            // external parameter entities and the external subset, which
            // are read through an external entity parser (module 12).
            _ => {
                let _ = t;
                return Err(Stop::DtdNotPorted);
            }
        }
        Ok(())
    }

    /// `scaffold[scaffIndex[scaffLevel - 1]]`: the innermost open group.
    fn current_scaffold_group(&self) -> usize {
        self.dtd
            .scaff_level
            .checked_sub(1)
            .and_then(|l| self.dtd.scaff_index.get(l))
            .copied()
            .unwrap_or(0)
    }

    /// `XML_ROLE_PARAM_ENTITY_REF` / `XML_ROLE_INNER_PARAM_ENTITY_REF`.
    fn param_entity_ref<E: Encoding, H: Handler>(
        &mut self,
        h: &mut H,
        enc: &E,
        buf: &[u8],
        role: Role,
        s: usize,
        next: usize,
    ) -> Result<(), Stop> {
        self.dtd.has_param_entity_refs = true;
        if self.param_entity_parsing == ParamEntityParsing::Never {
            self.dtd.keep_processing = self.dtd.standalone;
            return Ok(());
        }
        let name = Self::literal(enc, buf, s, next);
        let entity = self.dtd.param_entities.find(&self.salt, &name);
        // First, determine if a check for an existing declaration is
        // needed; if yes, check that the entity exists, and that it is
        // internal, otherwise call the skipped entity handler.
        let check = self.prolog_state.document_entity
            && if self.dtd.standalone {
                !self.in_internal_entity()
            } else {
                !self.dtd.has_param_entity_refs
            };
        let Some(id) = entity else {
            if check {
                return Err(XmlError::UndefinedEntity.into());
            }
            self.dtd.keep_processing = self.dtd.standalone;
            // Cannot report skipped entities in declarations.
            if role == Role::ParamEntityRef && self.handlers.skipped_entity {
                h.skipped_entity(&name, true);
            }
            return Ok(());
        };
        let r = EntityRef::Param(id);
        let (open, has_text, is_internal) = match self.dtd.entity(r) {
            Some(e) => (e.open, e.text.is_some(), e.is_internal),
            None => (false, false, true),
        };
        if check && !is_internal {
            return Err(XmlError::EntityDeclaredInPe.into());
        }
        if open {
            return Err(XmlError::RecursiveEntityRef.into());
        }
        if has_text {
            return self.open_param_entity(r, role == Role::ParamEntityRef);
        }
        // An external parameter entity: without an external entity
        // reference handler (public API, module 12) it is not read.
        self.dtd.keep_processing = self.dtd.standalone;
        Ok(())
    }

    /// `storeEntityValue` on `buf[ptr..]` (the slice ends at the end of the
    /// literal or replacement text), appending the replacement text to
    /// `out`. Returns at the end, or after opening a parameter entity
    /// (`*next_ptr` is then where to continue).
    pub(crate) fn store_entity_value<E: Encoding>(
        &mut self,
        enc: &E,
        buf: &[u8],
        ptr: usize,
        out: &mut Vec<u8>,
        account: Account,
        next_ptr: &mut usize,
    ) -> Result<(), Stop> {
        let old_in_entity_value = self.prolog_state.in_entity_value;
        self.prolog_state.in_entity_value = true;
        let mut next = ptr;
        let r = self.store_entity_value_inner(enc, buf, ptr, out, account, &mut next);
        self.prolog_state.in_entity_value = old_in_entity_value;
        // If 'nextPtr' is given, it should be updated during the
        // processing.
        *next_ptr = next;
        r
    }

    fn store_entity_value_inner<E: Encoding>(
        &mut self,
        enc: &E,
        buf: &[u8],
        ptr: usize,
        out: &mut Vec<u8>,
        account: Account,
        next_out: &mut usize,
    ) -> Result<(), Stop> {
        let m = E::MINBPC;
        let mut ptr = ptr;
        // Nothing to tokenize.
        if ptr >= buf.len() {
            return Ok(());
        }
        loop {
            let r = entity_value_tok(enc, buf, ptr);
            // XmlEntityValueTok doesn't always set the last arg.
            let mut next = r.next.unwrap_or(ptr);
            *next_out = next;
            if !self
                .accounting
                .diff_tolerated(r.scan, next.saturating_sub(ptr), account)
            {
                return Err(XmlError::AmplificationLimitBreach.into());
            }
            match r.scan {
                Scan::Token(Tok::ParamEntityRef) => {
                    if self.is_param_entity || enc.is_internal() {
                        let name = to_utf8(enc, bytes(buf, ptr + m, next - m));
                        let Some(id) = self.dtd.param_entities.find(&self.salt, &name) else {
                            // Not a well-formedness error - see XML 1.0:
                            // WFC Entity Declared (C cannot report a
                            // skipped entity here).
                            self.dtd.keep_processing = self.dtd.standalone;
                            return Ok(());
                        };
                        let r = EntityRef::Param(id);
                        let (open, external) = match self.dtd.entity(r) {
                            Some(e) => (e.open, e.system_id.is_some()),
                            None => (false, false),
                        };
                        if open || self.decl_entity == Some(r) {
                            self.ev_doc(enc, Some(ptr));
                            return Err(XmlError::RecursiveEntityRef.into());
                        }
                        if external {
                            // Read only by an external entity reference
                            // handler (public API); without one it is
                            // skipped.
                            self.dtd.keep_processing = self.dtd.standalone;
                            ptr = next;
                            continue;
                        }
                        return self.process_entity(r, false, EntityType::Value);
                    }
                    // In the internal subset, PE references are not legal
                    // within markup declarations, e.g entity values in
                    // this case.
                    self.ev_doc(enc, Some(ptr));
                    return Err(XmlError::ParamEntityRef.into());
                }
                Scan::None => return Ok(()),
                Scan::Token(Tok::EntityRef | Tok::DataChars) => {
                    out.extend_from_slice(&to_utf8(enc, bytes(buf, ptr, next)));
                }
                Scan::TrailingCr | Scan::Token(Tok::DataNewline) => {
                    if r.scan == Scan::TrailingCr {
                        next = ptr + m;
                    }
                    out.push(0xA);
                }
                Scan::Token(Tok::CharRef) => {
                    let n = char_ref_number(enc, buf, ptr);
                    let Ok(c) = u32::try_from(n) else {
                        self.ev_doc(enc, Some(ptr));
                        return Err(XmlError::BadCharRef.into());
                    };
                    let mut tmp = [0u8; 4];
                    let k = utf8_encode(c, &mut tmp);
                    out.extend_from_slice(tmp.get(..k).unwrap_or_default());
                }
                Scan::Partial => {
                    self.ev_doc(enc, Some(ptr));
                    return Err(XmlError::InvalidToken.into());
                }
                Scan::Invalid => {
                    self.ev_doc(enc, Some(next));
                    return Err(XmlError::InvalidToken.into());
                }
                _ => {
                    self.ev_doc(enc, Some(ptr));
                    return Err(XmlError::UnexpectedState.into());
                }
            }
            ptr = next;
        }
    }
}
