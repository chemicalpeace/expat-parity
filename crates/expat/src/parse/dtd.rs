//! The parts of `DTD` that content parsing uses: element types, attribute
//! ids, namespace prefixes and their bindings (`ELEMENT_TYPE`,
//! `ATTRIBUTE_ID`, `PREFIX`, `BINDING`), plus `getAttributeId`,
//! `setElementTypePrefix` and `addBinding`.
//!
//! The tables are the hash tables of module 9 ([`HashTable`]), so their
//! slot order is C's (it becomes observable through `getContext` once
//! external entities are ported). Pointers between the structures are ids:
//! [`EntryId`] for table entries and [`BindingId`] for bindings, which live
//! in an arena with C's free list.

use alloc::vec::Vec;

use crate::hash::{EntryId, HashTable, Lookup};
use crate::heap::{NoMemory, Unlimited};
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
    /// `generalEntities`. Entity declarations are module 11; until then the
    /// table is always empty.
    pub general_entities: HashTable<()>,
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

/// `sizeof(PREFIX)`, `sizeof(ATTRIBUTE_ID)`, `sizeof(ELEMENT_TYPE)`: the
/// `createSize` of a table entry. The parser does not account allocations
/// beyond the pools and tables yet (D-023), so only the tables see these.
const PREFIX_SIZE: usize = 2 * crate::heap::POINTER_SIZE;
const ATTRIBUTE_ID_SIZE: usize = 3 * crate::heap::POINTER_SIZE;
const ELEMENT_TYPE_SIZE: usize = 12 * crate::heap::POINTER_SIZE;

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

    /// `lookup(parser, &dtd->prefixes, name, sizeof(PREFIX))`.
    pub fn lookup_prefix(&mut self, salt: &SipKey, name: &[u8]) -> Result<PrefixId, NoMemory> {
        let (id, _) =
            self.prefixes
                .lookup_or_insert(salt, &mut Unlimited, name, PREFIX_SIZE, Prefix::default)?;
        Ok(PrefixId::Named(id))
    }

    /// `lookup(parser, &dtd->elementTypes, name, 0)`, creating the entry
    /// like `storeAtts` does: with `setElementTypePrefix` when namespace
    /// processing is on.
    pub fn element_type_for_content(
        &mut self,
        salt: &SipKey,
        ns: bool,
        name: &[u8],
    ) -> Result<EntryId, NoMemory> {
        if let Some(id) = self.element_types.find(salt, name) {
            return Ok(id);
        }
        let (id, _) = self.element_types.lookup_or_insert(
            salt,
            &mut Unlimited,
            name,
            ELEMENT_TYPE_SIZE,
            ElementType::default,
        )?;
        if ns {
            self.set_element_type_prefix(salt, id, name)?;
        }
        Ok(id)
    }

    /// `setElementTypePrefix`: the prefix of a new element type is the part
    /// of its name before the first colon.
    fn set_element_type_prefix(
        &mut self,
        salt: &SipKey,
        id: EntryId,
        name: &[u8],
    ) -> Result<(), NoMemory> {
        if let Some(colon) = name.iter().position(|&c| c == b':') {
            let prefix = self.lookup_prefix(salt, name.get(..colon).unwrap_or_default())?;
            if let Some(et) = self.element_types.value_mut(id) {
                et.prefix = Some(prefix);
            }
        }
        Ok(())
    }

    /// `getAttributeId` for the (already converted) name `name`.
    pub fn attribute_id(&mut self, salt: &SipKey, ns: bool, name: &[u8]) -> Result<EntryId, NoMemory> {
        let (id, l) = self.attribute_ids.lookup_or_insert(
            salt,
            &mut Unlimited,
            name,
            ATTRIBUTE_ID_SIZE,
            AttributeId::default,
        )?;
        if l == Lookup::Found || !ns {
            return Ok(id);
        }
        let prefix = if name == b"xmlns" {
            Some((PrefixId::Default, true))
        } else if let Some(rest) = name.strip_prefix(b"xmlns:") {
            Some((self.lookup_prefix(salt, rest)?, true))
        } else if let Some(colon) = name.iter().position(|&c| c == b':') {
            // Attributes without a prefix are *not* in the default namespace.
            Some((self.lookup_prefix(salt, name.get(..colon).unwrap_or_default())?, false))
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

    /// Takes a binding from the free list or allocates a new one.
    pub fn new_binding(&mut self, b: Binding) -> BindingId {
        if let Some(id) = self.free_bindings.pop()
            && let Some(slot) = self.bindings.get_mut(id.0)
        {
            *slot = b;
            return id;
        }
        self.bindings.push(b);
        BindingId(self.bindings.len().wrapping_sub(1))
    }
}
