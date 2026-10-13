//! The parts of `DTD` that content parsing uses: element types, attribute
//! ids, namespace prefixes and their bindings (`ELEMENT_TYPE`,
//! `ATTRIBUTE_ID`, `PREFIX`, `BINDING`), entities (`ENTITY`), the
//! content-model scaffold, plus `getAttributeId`, `getElementType`,
//! `setElementTypePrefix`, `defineAttribute` and `addBinding`.
//!
//! The tables are the hash tables of module 9 ([`HashTable`]), so their
//! slot order is C's (it becomes observable through `getContext` once
//! external entities are ported). Pointers between the structures are ids:
//! [`EntryId`] for table entries and [`BindingId`] for bindings, which live
//! in an arena with C's free list.

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;

use super::model::Scaffold;

use crate::hash::{EntryId, HashTable, Lookup};
use crate::heap::NoMemory;

use super::tracker::{
    DEFAULT_ATTRIBUTE_SIZE, NAME_AND_DEFAULT_ATTRIBUTE_SIZE, Pool, TableShadow, Tracker,
};
use crate::siphash::SipKey;

/// A namespace prefix (C: `PREFIX *`): the default (unnamed) prefix, or an
/// entry of [`Dtd::prefixes`] whose key is the prefix name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrefixId {
    /// `&dtd->defaultPrefix` (name `NULL`).
    Default,
    /// An entry of `dtd->prefixes`.
    Named(EntryId),
}

/// Names a [`Binding`] in [`Dtd::bindings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindingId(pub usize);

/// `PREFIX` (without the name, which is the table key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Prefix {
    /// `binding`: the innermost binding in scope.
    pub binding: Option<BindingId>,
}

/// `ATTRIBUTE_ID` (without the name, which is the table key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttributeId {
    /// `prefix`
    pub prefix: Option<PrefixId>,
    /// `maybeTokenized`: declared with a type other than CDATA.
    pub maybe_tokenized: bool,
    /// `xmlns`: the attribute declares a namespace.
    pub xmlns: bool,
    /// `name[-1]`: 0, or 1 / 2 while `storeAtts` has seen the attribute
    /// (2: it has a prefix that still has to be expanded).
    pub flag: u8,
}

/// `DEFAULT_ATTRIBUTE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultAttribute {
    /// `id`
    pub id: EntryId,
    /// `isCdata`
    pub is_cdata: bool,
    /// `value` (`None`: `#IMPLIED` or `#REQUIRED`).
    pub value: Option<Vec<u8>>,
}

/// `ELEMENT_TYPE` (without the name, which is the table key).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ElementType {
    /// `prefix`
    pub prefix: Option<PrefixId>,
    /// `idAtt`: the attribute declared with type ID.
    pub id_att: Option<EntryId>,
    /// `defaultAtts`, in declaration order.
    pub default_atts: Vec<DefaultAttribute>,
    /// `defaultAttForName`: for each attribute id, the index in
    /// `default_atts` of its first definition. C keeps a hash table whose
    /// slot order is never observable; a map gives the same lookups.
    pub default_att_for_name: BTreeMap<EntryId, usize>,
    /// `allocDefaultAtts` (allocation tracker, D-030).
    pub alloc_default_atts: usize,
    /// The allocations of `defaultAttForName` (allocation tracker).
    pub default_att_table: TableShadow,
}

/// `ENTITY` (without the name, which is the table key).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Entity {
    /// `textPtr[0..textLen]`: the replacement text of an internal entity
    /// (UTF-8); `None` for an external entity. Shared so that the text can
    /// be scanned while the parser state changes.
    pub text: Option<Arc<[u8]>>,
    /// `processed`: bytes of `text` already processed.
    pub processed: usize,
    /// `systemId`
    pub system_id: Option<Vec<u8>>,
    /// `base`
    pub base: Option<Vec<u8>>,
    /// `publicId`
    pub public_id: Option<Vec<u8>>,
    /// `notation`
    pub notation: Option<Vec<u8>>,
    /// `open`: the entity is being expanded.
    pub open: bool,
    /// `hasMore`: the entity has not been completely processed.
    pub has_more: bool,
    /// `is_param`
    pub is_param: bool,
    /// `is_internal`: declared in the internal subset outside a parameter
    /// entity.
    pub is_internal: bool,
}

/// Which entity table an entity is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityRef {
    /// `dtd->generalEntities`
    General(EntryId),
    /// `dtd->paramEntities`
    Param(EntryId),
}

/// `BINDING`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// `prefix`
    pub prefix: PrefixId,
    /// `attId`: the `xmlns` attribute that made the binding (`None` for the
    /// implicit context).
    pub att_id: Option<EntryId>,
    /// `uri[0..uriLen]`: the URI followed by the namespace separator, if
    /// the separator is not NUL.
    pub uri: Vec<u8>,
    /// `nextTagBinding`
    pub next_tag_binding: Option<BindingId>,
    /// `prevPrefixBinding`
    pub prev_prefix_binding: Option<BindingId>,
}

/// The `DTD` structure, as far as content parsing uses it.
#[derive(Debug, Clone)]
pub struct Dtd {
    /// `generalEntities`
    pub general_entities: HashTable<Entity>,
    /// `paramEntities`
    pub param_entities: HashTable<Entity>,
    /// `paramEntityRead`: an external parameter entity has been read.
    pub param_entity_read: bool,
    /// `in_eldecl`: an element declaration's content model is being read.
    pub in_eldecl: bool,
    /// `scaffold[0..scaffCount]`
    pub scaffold: Vec<Scaffold>,
    /// `scaffCount`
    pub scaff_count: usize,
    /// `scaffLevel`
    pub scaff_level: usize,
    /// `scaffIndex`
    pub scaff_index: Vec<usize>,
    /// `contentStringLen`
    pub content_string_len: u32,
    /// `elementTypes`
    pub element_types: HashTable<ElementType>,
    /// `attributeIds`
    pub attribute_ids: HashTable<AttributeId>,
    /// `prefixes`
    pub prefixes: HashTable<Prefix>,
    /// `defaultPrefix.binding`
    pub default_prefix: Prefix,
    /// `keepProcessing`
    pub keep_processing: bool,
    /// `hasParamEntityRefs`
    pub has_param_entity_refs: bool,
    /// `standalone`
    pub standalone: bool,
    /// The bindings (C allocates each one; freed ones go to
    /// `m_freeBindingList`).
    pub bindings: Vec<Binding>,
    /// `m_freeBindingList`, most recently freed last.
    pub free_bindings: Vec<BindingId>,
}

/// `sizeof(PREFIX)`, `sizeof(ATTRIBUTE_ID)`: the `createSize` of a table
/// entry, charged to the allocation tracker (D-030), from the C layout
/// ([`super::c_layout`]).
pub(crate) const PREFIX_SIZE: usize = super::c_layout::PREFIX;
pub(crate) const ATTRIBUTE_ID_SIZE: usize = super::c_layout::ATTRIBUTE_ID;
/// `sizeof(ELEMENT_TYPE)` ([`super::c_layout::ELEMENT_TYPE`]: 88 bytes on
/// 64-bit targets). Before stage 6 of D-034 the port charged twelve
/// pointer-sized words, 8 bytes more than C.
pub(crate) const ELEMENT_TYPE_SIZE: usize = super::c_layout::ELEMENT_TYPE;
/// `sizeof(ENTITY)` ([`super::c_layout::ENTITY`]).
pub(crate) const ENTITY_SIZE: usize = super::c_layout::ENTITY;

impl Default for Dtd {
    fn default() -> Self {
        Self::new()
    }
}

impl Dtd {
    /// `dtdCreate`.
    pub fn new() -> Self {
        Dtd {
            general_entities: HashTable::new(),
            param_entities: HashTable::new(),
            param_entity_read: false,
            in_eldecl: false,
            scaffold: Vec::new(),
            scaff_count: 0,
            scaff_level: 0,
            scaff_index: Vec::new(),
            content_string_len: 0,
            element_types: HashTable::new(),
            attribute_ids: HashTable::new(),
            prefixes: HashTable::new(),
            default_prefix: Prefix::default(),
            keep_processing: true,
            has_param_entity_refs: false,
            standalone: false,
            bindings: Vec::new(),
            free_bindings: Vec::new(),
        }
    }

    /// The name of a prefix (`prefix->name`; `None` for the default prefix).
    pub fn prefix_name(&self, p: PrefixId) -> Option<&[u8]> {
        match p {
            PrefixId::Default => None,
            PrefixId::Named(id) => self.prefixes.key(id),
        }
    }

    /// `prefix->binding`.
    pub fn prefix_binding(&self, p: PrefixId) -> Option<BindingId> {
        match p {
            PrefixId::Default => self.default_prefix.binding,
            PrefixId::Named(id) => self.prefixes.value(id).and_then(|v| v.binding),
        }
    }

    /// Sets `prefix->binding`.
    pub fn set_prefix_binding(&mut self, p: PrefixId, b: Option<BindingId>) {
        match p {
            PrefixId::Default => self.default_prefix.binding = b,
            PrefixId::Named(id) => {
                if let Some(v) = self.prefixes.value_mut(id) {
                    v.binding = b;
                }
            }
        }
    }

    /// The binding `b`.
    pub fn binding(&self, b: BindingId) -> Option<&Binding> {
        self.bindings.get(b.0)
    }

    /// `lookup(parser, &dtd->prefixes, name, sizeof(PREFIX))`, with the
    /// name and a NUL appended to `dtd->pool` one character at a time
    /// first (kept when the prefix is new).
    pub fn lookup_prefix(
        &mut self,
        salt: &SipKey,
        tr: &mut Tracker,
        direct: u64,
        name: &[u8],
    ) -> Result<PrefixId, NoMemory> {
        tr.copy_discard(Pool::Dtd, name.len(), direct)?;
        let (id, l) =
            self.prefixes
                .lookup_or_insert(salt, &mut tr.heap(direct), name, PREFIX_SIZE, Prefix::default)?;
        if l == Lookup::Inserted {
            tr.copy(Pool::Dtd, name.len(), direct)?;
        }
        Ok(PrefixId::Named(id))
    }

    /// `lookup(parser, &dtd->prefixes, name + 6, sizeof(PREFIX))` of an
    /// `xmlns:` attribute id: the prefix's name is the attribute id's own
    /// (nothing in `dtd->pool`).
    fn lookup_xmlns_prefix(
        &mut self,
        salt: &SipKey,
        tr: &mut Tracker,
        direct: u64,
        name: &[u8],
    ) -> Result<PrefixId, NoMemory> {
        let (id, _) = self
            .prefixes
            .lookup_or_insert(salt, &mut tr.heap(direct), name, PREFIX_SIZE, Prefix::default)?;
        Ok(PrefixId::Named(id))
    }

    /// `lookup(parser, &dtd->elementTypes, name, 0)`, creating the entry
    /// like `storeAtts` does: with `setElementTypePrefix` when namespace
    /// processing is on.
    pub fn element_type_for_content(
        &mut self,
        salt: &SipKey,
        tr: &mut Tracker,
        direct: u64,
        ns: bool,
        name: &[u8],
    ) -> Result<EntryId, NoMemory> {
        if let Some(id) = self.element_types.find(salt, name) {
            return Ok(id);
        }
        // `poolCopyString(&dtd->pool, tagNamePtr->str)`
        tr.copy(Pool::Dtd, name.len(), direct)?;
        let (id, _) = self.element_types.lookup_or_insert(
            salt,
            &mut tr.heap(direct),
            name,
            ELEMENT_TYPE_SIZE,
            ElementType::default,
        )?;
        if ns {
            self.set_element_type_prefix(salt, tr, direct, id, name)?;
        }
        Ok(id)
    }

    /// `getElementType`: the element type `name`, created (with
    /// `setElementTypePrefix`, whatever the namespace mode) if new.
    pub fn element_type(
        &mut self,
        salt: &SipKey,
        tr: &mut Tracker,
        direct: u64,
        name: &[u8],
    ) -> Result<EntryId, NoMemory> {
        // `poolStoreString(&dtd->pool, ...)`, discarded for a known name.
        tr.store_text_discard(Pool::Dtd, name, direct)?;
        let (id, l) = self.element_types.lookup_or_insert(
            salt,
            &mut tr.heap(direct),
            name,
            ELEMENT_TYPE_SIZE,
            ElementType::default,
        )?;
        if l == Lookup::Inserted {
            tr.store_text(Pool::Dtd, name, direct)?;
            // Unconditional in C's `getElementType`, also without namespace
            // processing (only `storeAtts` reads the prefix, and only with
            // it); its `dtd->pool` string and `dtd->prefixes` entry are
            // charged either way (D-034 stage 4).
            self.set_element_type_prefix(salt, tr, direct, id, name)?;
        }
        Ok(id)
    }

    /// `lookup(parser, table, name, sizeof(ENTITY))`: the entity `name`
    /// and whether it was created by this call (C: `entity->name == name`).
    pub fn declare_entity(
        &mut self,
        salt: &SipKey,
        tr: &mut Tracker,
        direct: u64,
        is_param: bool,
        name: &[u8],
    ) -> Result<(EntityRef, bool), NoMemory> {
        let table = if is_param {
            &mut self.param_entities
        } else {
            &mut self.general_entities
        };
        // `poolStoreString(&dtd->pool, ...)`, discarded for a known name.
        tr.store_text_discard(Pool::Dtd, name, direct)?;
        let (id, l) = table.lookup_or_insert(salt, &mut tr.heap(direct), name, ENTITY_SIZE, Entity::default)?;
        if l == Lookup::Inserted {
            tr.store_text(Pool::Dtd, name, direct)?;
        }
        let r = if is_param {
            EntityRef::Param(id)
        } else {
            EntityRef::General(id)
        };
        Ok((r, l == Lookup::Inserted))
    }

    /// `lookup(parser, &dtd->paramEntities, externalSubsetName,
    /// sizeof(ENTITY))`: the external subset's entity, whose name is C's
    /// static `"#"` (no copy into `dtd->pool`, unlike [`Dtd::declare_entity`]).
    pub fn external_subset_entity(
        &mut self,
        salt: &SipKey,
        tr: &mut Tracker,
        direct: u64,
    ) -> Result<EntityRef, NoMemory> {
        let (id, _) = self.param_entities.lookup_or_insert(
            salt,
            &mut tr.heap(direct),
            super::decl::EXTERNAL_SUBSET_NAME,
            ENTITY_SIZE,
            Entity::default,
        )?;
        Ok(EntityRef::Param(id))
    }

    /// The entity `r`.
    pub fn entity(&self, r: EntityRef) -> Option<&Entity> {
        match r {
            EntityRef::General(id) => self.general_entities.value(id),
            EntityRef::Param(id) => self.param_entities.value(id),
        }
    }

    /// The entity `r`, mutably.
    pub fn entity_mut(&mut self, r: EntityRef) -> Option<&mut Entity> {
        match r {
            EntityRef::General(id) => self.general_entities.value_mut(id),
            EntityRef::Param(id) => self.param_entities.value_mut(id),
        }
    }

    /// The name of the entity `r`.
    pub fn entity_name(&self, r: EntityRef) -> &[u8] {
        match r {
            EntityRef::General(id) => self.general_entities.key(id),
            EntityRef::Param(id) => self.param_entities.key(id),
        }
        .unwrap_or_default()
    }

    /// `defineAttribute`: adds a declaration of the attribute `att` to the
    /// element type `et` (`value`: its default, `None` for `#IMPLIED` and
    /// `#REQUIRED`).
    /// The arrays it grows are charged to `tr`; `Err` when the tracker
    /// refuses (C: `defineAttribute` returns 0, `XML_ERROR_NO_MEMORY`).
    #[allow(clippy::too_many_arguments)] // reason: mirrors defineAttribute plus the tracker.
    pub fn define_attribute(
        &mut self,
        tr: &mut Tracker,
        direct: u64,
        et: EntryId,
        att: EntryId,
        is_cdata: bool,
        is_id: bool,
        value: Option<Vec<u8>>,
    ) -> Result<(), NoMemory> {
        let att_xmlns = self.attribute_ids.value(att).is_some_and(|a| a.xmlns);
        let Some(t) = self.element_types.value_mut(et) else {
            return Ok(());
        };
        if value.is_some() || is_id {
            // The handling of default attributes gets messed up if we have
            // a default which duplicates a non-default.
            if t.default_att_for_name.contains_key(&att) {
                return Ok(());
            }
            if is_id && t.id_att.is_none() && !att_xmlns {
                t.id_att = Some(att);
            }
        }
        if t.default_atts.len() == t.alloc_default_atts {
            let count = if t.alloc_default_atts == 0 {
                8
            } else {
                t.alloc_default_atts.saturating_mul(2)
            };
            let ok = if t.alloc_default_atts == 0 {
                tr.t.alloc(count * DEFAULT_ATTRIBUTE_SIZE, direct)
            } else {
                tr.t.realloc(
                    t.alloc_default_atts * DEFAULT_ATTRIBUTE_SIZE,
                    count * DEFAULT_ATTRIBUTE_SIZE,
                    direct,
                )
            };
            if !ok {
                return Err(NoMemory);
            }
            t.alloc_default_atts = count;
        }
        if !t.default_att_for_name.contains_key(&att) {
            t.default_att_table
                .insert(&mut tr.heap(direct), NAME_AND_DEFAULT_ATTRIBUTE_SIZE)?;
        }
        let index = t.default_atts.len();
        t.default_atts.push(DefaultAttribute {
            id: att,
            is_cdata,
            value,
        });
        // The first declaration is binding (XML 1.0 3.3).
        t.default_att_for_name.entry(att).or_insert(index);
        if !is_cdata && let Some(a) = self.attribute_ids.value_mut(att) {
            a.maybe_tokenized = true;
        }
        Ok(())
    }

    /// `setElementTypePrefix`: the prefix of a new element type is the part
    /// of its name before the first colon.
    fn set_element_type_prefix(
        &mut self,
        salt: &SipKey,
        tr: &mut Tracker,
        direct: u64,
        id: EntryId,
        name: &[u8],
    ) -> Result<(), NoMemory> {
        if let Some(colon) = name.iter().position(|&c| c == b':') {
            let prefix = self.lookup_prefix(salt, tr, direct, name.get(..colon).unwrap_or_default())?;
            if let Some(et) = self.element_types.value_mut(id) {
                et.prefix = Some(prefix);
            }
        }
        Ok(())
    }

    /// `getAttributeId` for the (already converted) name `name`.
    pub fn attribute_id(
        &mut self,
        salt: &SipKey,
        tr: &mut Tracker,
        direct: u64,
        ns: bool,
        name: &[u8],
    ) -> Result<EntryId, NoMemory> {
        // The flag character (`poolAppendChar`), the converted name and a
        // NUL (`poolStoreString`) in `dtd->pool`, discarded for a known
        // name.
        {
            let (p, mut h) = tr.pool(Pool::Dtd, direct);
            p.append_chars(&mut h, 1)?;
            p.store_text_discard(&mut h, name)?;
        }
        let (id, l) = self.attribute_ids.lookup_or_insert(
            salt,
            &mut tr.heap(direct),
            name,
            ATTRIBUTE_ID_SIZE,
            AttributeId::default,
        )?;
        if l == Lookup::Found {
            return Ok(id);
        }
        {
            let (p, mut h) = tr.pool(Pool::Dtd, direct);
            p.append_chars(&mut h, 1)?;
            p.store_text(&mut h, name)?;
        }
        if !ns {
            return Ok(id);
        }
        let prefix = if name == b"xmlns" {
            Some((PrefixId::Default, true))
        } else if let Some(rest) = name.strip_prefix(b"xmlns:") {
            // C does not check this lookup (xmlparse.c 7650-7652): when the
            // tracker refuses the prefix entry (or the table's growth), the
            // id keeps `prefix == NULL` with `xmlns` set and the parse goes
            // on; `storeAtts` then treats the attribute as an unprefixed
            // one (4262, 4295) and `dtdCopy` copies `xmlns` only with a
            // prefix (8011). The port records the same state (D-034).
            match self.lookup_xmlns_prefix(salt, tr, direct, rest) {
                Ok(p) => Some((p, true)),
                Err(NoMemory) => {
                    if let Some(v) = self.attribute_ids.value_mut(id) {
                        v.xmlns = true;
                    }
                    return Ok(id);
                }
            }
        } else if let Some(colon) = name.iter().position(|&c| c == b':') {
            // Attributes without a prefix are *not* in the default namespace.
            Some((self.lookup_prefix(salt, tr, direct, name.get(..colon).unwrap_or_default())?, false))
        } else {
            None
        };
        if let (Some((p, xmlns)), Some(v)) = (prefix, self.attribute_ids.value_mut(id)) {
            v.prefix = Some(p);
            v.xmlns = xmlns;
        }
        Ok(id)
    }

    /// The flag of an attribute id (`name[-1]`).
    pub fn att_flag(&self, id: EntryId) -> u8 {
        self.attribute_ids.value(id).map_or(0, |v| v.flag)
    }

    /// Sets the flag of an attribute id.
    pub fn set_att_flag(&mut self, id: EntryId, flag: u8) {
        if let Some(v) = self.attribute_ids.value_mut(id) {
            v.flag = flag;
        }
    }

    /// Takes a binding from the free list or allocates a new one; the
    /// flag is true for a binding from the free list.
    pub fn new_binding(&mut self, b: Binding) -> (BindingId, bool) {
        if let Some(id) = self.free_bindings.pop()
            && let Some(slot) = self.bindings.get_mut(id.0)
        {
            *slot = b;
            return (id, true);
        }
        self.bindings.push(b);
        (BindingId(self.bindings.len().wrapping_sub(1)), false)
    }

    /// The binding the next [`Dtd::new_binding`] returns, and whether it
    /// comes from the free list.
    pub fn next_binding(&self) -> (BindingId, bool) {
        match self.free_bindings.last() {
            Some(&id) if id.0 < self.bindings.len() => (id, true),
            _ => (BindingId(self.bindings.len()), false),
        }
    }
}
