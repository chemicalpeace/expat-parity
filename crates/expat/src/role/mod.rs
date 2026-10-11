//! The prolog state machine: a port of `xmlrole.c` (`R_2_9_0`).
//!
//! The parser feeds every token it scans in the prolog and in the DTD to
//! [`PrologState::token_role`], which assigns the token its syntactic
//! [`Role`] (doctype name, entity value, attribute type, content-model
//! group, ...) and moves to the next state. A token that is not allowed
//! where it occurs gets [`Role::Error`] and puts the machine into a state
//! that answers [`Role::None`] to everything.
//!
//! C stores the state as a pointer to one of ~55 handler functions; here it
//! is the [`Handler`] enum, and [`PrologState::token_role`] dispatches on it
//! to one method per C handler, each a line-by-line port. The build
//! configuration has `XML_DTD` (D-002), so the external-subset and
//! conditional-section states are included.
//!
//! ```
//! # #[cfg(feature = "internal-api")] {
//! use expat::internal::role::{PrologState, Role};
//! use expat::internal::tok::{Normal, Scan, Tok};
//! let doc = b"<!DOCTYPE doc>";
//! let mut st = PrologState::new();
//! let r = st.token_role(Scan::Token(Tok::DeclOpen), &Normal::Utf8, doc, 0, 9);
//! assert_eq!(r, Role::DoctypeNone);
//! let r = st.token_role(Scan::Token(Tok::Name), &Normal::Utf8, doc, 10, 13);
//! assert_eq!(r, Role::DoctypeName);
//! # }
//! ```

use crate::tok::{Encoding, Scan, Tok, name_matches_ascii};

/// `XML_ROLE_*` (`xmlrole.h`). `Role as i32` is the C value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Role {
    /// `XML_ROLE_ERROR`
    Error = -1,
    /// `XML_ROLE_NONE`
    None = 0,
    /// `XML_ROLE_XML_DECL`
    XmlDecl,
    /// `XML_ROLE_INSTANCE_START`
    InstanceStart,
    /// `XML_ROLE_DOCTYPE_NONE`
    DoctypeNone,
    /// `XML_ROLE_DOCTYPE_NAME`
    DoctypeName,
    /// `XML_ROLE_DOCTYPE_SYSTEM_ID`
    DoctypeSystemId,
    /// `XML_ROLE_DOCTYPE_PUBLIC_ID`
    DoctypePublicId,
    /// `XML_ROLE_DOCTYPE_INTERNAL_SUBSET`
    DoctypeInternalSubset,
    /// `XML_ROLE_DOCTYPE_CLOSE`
    DoctypeClose,
    /// `XML_ROLE_GENERAL_ENTITY_NAME`
    GeneralEntityName,
    /// `XML_ROLE_PARAM_ENTITY_NAME`
    ParamEntityName,
    /// `XML_ROLE_ENTITY_NONE`
    EntityNone,
    /// `XML_ROLE_ENTITY_VALUE`
    EntityValue,
    /// `XML_ROLE_ENTITY_SYSTEM_ID`
    EntitySystemId,
    /// `XML_ROLE_ENTITY_PUBLIC_ID`
    EntityPublicId,
    /// `XML_ROLE_ENTITY_COMPLETE`
    EntityComplete,
    /// `XML_ROLE_ENTITY_NOTATION_NAME`
    EntityNotationName,
    /// `XML_ROLE_NOTATION_NONE`
    NotationNone,
    /// `XML_ROLE_NOTATION_NAME`
    NotationName,
    /// `XML_ROLE_NOTATION_SYSTEM_ID`
    NotationSystemId,
    /// `XML_ROLE_NOTATION_NO_SYSTEM_ID`
    NotationNoSystemId,
    /// `XML_ROLE_NOTATION_PUBLIC_ID`
    NotationPublicId,
    /// `XML_ROLE_ATTRIBUTE_NAME`
    AttributeName,
    /// `XML_ROLE_ATTRIBUTE_TYPE_CDATA`
    AttributeTypeCdata,
    /// `XML_ROLE_ATTRIBUTE_TYPE_ID`
    AttributeTypeId,
    /// `XML_ROLE_ATTRIBUTE_TYPE_IDREF`
    AttributeTypeIdref,
    /// `XML_ROLE_ATTRIBUTE_TYPE_IDREFS`
    AttributeTypeIdrefs,
    /// `XML_ROLE_ATTRIBUTE_TYPE_ENTITY`
    AttributeTypeEntity,
    /// `XML_ROLE_ATTRIBUTE_TYPE_ENTITIES`
    AttributeTypeEntities,
    /// `XML_ROLE_ATTRIBUTE_TYPE_NMTOKEN`
    AttributeTypeNmtoken,
    /// `XML_ROLE_ATTRIBUTE_TYPE_NMTOKENS`
    AttributeTypeNmtokens,
    /// `XML_ROLE_ATTRIBUTE_ENUM_VALUE`
    AttributeEnumValue,
    /// `XML_ROLE_ATTRIBUTE_NOTATION_VALUE`
    AttributeNotationValue,
    /// `XML_ROLE_ATTLIST_NONE`
    AttlistNone,
    /// `XML_ROLE_ATTLIST_ELEMENT_NAME`
    AttlistElementName,
    /// `XML_ROLE_IMPLIED_ATTRIBUTE_VALUE`
    ImpliedAttributeValue,
    /// `XML_ROLE_REQUIRED_ATTRIBUTE_VALUE`
    RequiredAttributeValue,
    /// `XML_ROLE_DEFAULT_ATTRIBUTE_VALUE`
    DefaultAttributeValue,
    /// `XML_ROLE_FIXED_ATTRIBUTE_VALUE`
    FixedAttributeValue,
    /// `XML_ROLE_ELEMENT_NONE`
    ElementNone,
    /// `XML_ROLE_ELEMENT_NAME`
    ElementName,
    /// `XML_ROLE_CONTENT_ANY`
    ContentAny,
    /// `XML_ROLE_CONTENT_EMPTY`
    ContentEmpty,
    /// `XML_ROLE_CONTENT_PCDATA`
    ContentPcdata,
    /// `XML_ROLE_GROUP_OPEN`
    GroupOpen,
    /// `XML_ROLE_GROUP_CLOSE`
    GroupClose,
    /// `XML_ROLE_GROUP_CLOSE_REP`
    GroupCloseRep,
    /// `XML_ROLE_GROUP_CLOSE_OPT`
    GroupCloseOpt,
    /// `XML_ROLE_GROUP_CLOSE_PLUS`
    GroupClosePlus,
    /// `XML_ROLE_GROUP_CHOICE`
    GroupChoice,
    /// `XML_ROLE_GROUP_SEQUENCE`
    GroupSequence,
    /// `XML_ROLE_CONTENT_ELEMENT`
    ContentElement,
    /// `XML_ROLE_CONTENT_ELEMENT_REP`
    ContentElementRep,
    /// `XML_ROLE_CONTENT_ELEMENT_OPT`
    ContentElementOpt,
    /// `XML_ROLE_CONTENT_ELEMENT_PLUS`
    ContentElementPlus,
    /// `XML_ROLE_PI`
    Pi,
    /// `XML_ROLE_COMMENT`
    Comment,
    /// `XML_ROLE_TEXT_DECL`
    TextDecl,
    /// `XML_ROLE_IGNORE_SECT`
    IgnoreSect,
    /// `XML_ROLE_INNER_PARAM_ENTITY_REF`
    InnerParamEntityRef,
    /// `XML_ROLE_PARAM_ENTITY_REF`
    ParamEntityRef,
}

/// The attribute types of `attlist2`, in the order of their roles
/// (`XML_ROLE_ATTRIBUTE_TYPE_CDATA + i`).
const ATTRIBUTE_TYPES: [(&[u8], Role); 8] = [
    (b"CDATA", Role::AttributeTypeCdata),
    (b"ID", Role::AttributeTypeId),
    (b"IDREF", Role::AttributeTypeIdref),
    (b"IDREFS", Role::AttributeTypeIdrefs),
    (b"ENTITY", Role::AttributeTypeEntity),
    (b"ENTITIES", Role::AttributeTypeEntities),
    (b"NMTOKEN", Role::AttributeTypeNmtoken),
    (b"NMTOKENS", Role::AttributeTypeNmtokens),
];

/// The handler functions of `xmlrole.c`; the current one is the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)] // reason: each variant is the C handler of the same name.
pub enum Handler {
    Prolog0,
    Prolog1,
    Prolog2,
    Doctype0,
    Doctype1,
    Doctype2,
    Doctype3,
    Doctype4,
    Doctype5,
    InternalSubset,
    Entity0,
    Entity1,
    Entity2,
    Entity3,
    Entity4,
    Entity5,
    Entity6,
    Entity7,
    Entity8,
    Entity9,
    Entity10,
    Notation0,
    Notation1,
    Notation2,
    Notation3,
    Notation4,
    Attlist0,
    Attlist1,
    Attlist2,
    Attlist3,
    Attlist4,
    Attlist5,
    Attlist6,
    Attlist7,
    Attlist8,
    Attlist9,
    Element0,
    Element1,
    Element2,
    Element3,
    Element4,
    Element5,
    Element6,
    Element7,
    ExternalSubset0,
    ExternalSubset1,
    CondSect0,
    CondSect1,
    CondSect2,
    DeclClose,
    Error,
}

impl Handler {
    /// Every handler, in the order of the C declarations.
    pub const ALL: [Handler; 51] = [
        Handler::Prolog0,
        Handler::Prolog1,
        Handler::Prolog2,
        Handler::Doctype0,
        Handler::Doctype1,
        Handler::Doctype2,
        Handler::Doctype3,
        Handler::Doctype4,
        Handler::Doctype5,
        Handler::InternalSubset,
        Handler::Entity0,
        Handler::Entity1,
        Handler::Entity2,
        Handler::Entity3,
        Handler::Entity4,
        Handler::Entity5,
        Handler::Entity6,
        Handler::Entity7,
        Handler::Entity8,
        Handler::Entity9,
        Handler::Entity10,
        Handler::Notation0,
        Handler::Notation1,
        Handler::Notation2,
        Handler::Notation3,
        Handler::Notation4,
        Handler::Attlist0,
        Handler::Attlist1,
        Handler::Attlist2,
        Handler::Attlist3,
        Handler::Attlist4,
        Handler::Attlist5,
        Handler::Attlist6,
        Handler::Attlist7,
        Handler::Attlist8,
        Handler::Attlist9,
        Handler::Element0,
        Handler::Element1,
        Handler::Element2,
        Handler::Element3,
        Handler::Element4,
        Handler::Element5,
        Handler::Element6,
        Handler::Element7,
        Handler::ExternalSubset0,
        Handler::ExternalSubset1,
        Handler::CondSect0,
        Handler::CondSect1,
        Handler::CondSect2,
        Handler::DeclClose,
        Handler::Error,
    ];
}

/// `PROLOG_STATE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrologState {
    /// `handler`: the current state.
    pub handler: Handler,
    /// `level`: nesting depth of content-model groups.
    pub level: u32,
    /// `role_none`: the role `declClose` gives white space and `>`.
    pub role_none: Role,
    /// `includeLevel`: nesting depth of `INCLUDE` sections.
    pub include_level: u32,
    /// `documentEntity`: false while parsing an external entity.
    pub document_entity: bool,
    /// `inEntityValue`: maintained by the parser, not by this module.
    pub in_entity_value: bool,
}

impl Default for PrologState {
    fn default() -> Self {
        Self::new()
    }
}

/// The token kinds the handlers distinguish: a complete token (C: positive
/// value), `XML_TOK_NONE`, or anything else (which no handler lists).
#[derive(Clone, Copy, PartialEq, Eq)]
enum T {
    Tok(Tok),
    None,
    Other,
}

impl PrologState {
    /// `XmlPrologStateInit`: the state at the start of a document entity.
    /// `level` and `role_none` are not set by C; they are 0 and
    /// [`Role::None`] here and are always written before they are read.
    pub fn new() -> Self {
        PrologState {
            handler: Handler::Prolog0,
            level: 0,
            role_none: Role::None,
            include_level: 0,
            document_entity: true,
            in_entity_value: false,
        }
    }

    /// `XmlPrologStateInitExternalEntity` applied to `self`: start of an
    /// external parameter entity or the external subset. Like C, it leaves
    /// `level`, `role_none` and `in_entity_value` alone.
    pub fn init_external_entity(&mut self) {
        self.handler = Handler::ExternalSubset0;
        self.document_entity = false;
        self.include_level = 0;
    }

    /// `XmlTokenRole(state, tok, ptr, end, enc)`: the role of the token
    /// `buf[ptr..end]` that the scanner returned as `tok`, and the
    /// transition it causes.
    pub fn token_role<E: Encoding>(
        &mut self,
        tok: Scan,
        enc: &E,
        buf: &[u8],
        ptr: usize,
        end: usize,
    ) -> Role {
        let t = match tok {
            Scan::Token(t) => T::Tok(t),
            Scan::None => T::None,
            _ => T::Other,
        };
        let name = Name {
            enc,
            buf: buf.get(..end).unwrap_or_default(),
            ptr,
        };
        self.dispatch(self.handler, t, &name)
    }

    fn dispatch<E: Encoding>(&mut self, h: Handler, t: T, n: &Name<'_, E>) -> Role {
        use Handler as H;
        match h {
            H::Prolog0 => self.prolog0(t, n),
            H::Prolog1 => self.prolog1(t, n),
            H::Prolog2 => self.prolog2(t),
            H::Doctype0 => self.doctype0(t),
            H::Doctype1 => self.doctype1(t, n),
            H::Doctype2 => self.doctype2(t),
            H::Doctype3 => self.doctype3(t),
            H::Doctype4 => self.doctype4(t),
            H::Doctype5 => self.doctype5(t),
            H::InternalSubset => self.internal_subset(t, n),
            H::ExternalSubset0 => self.external_subset0(t, n),
            H::ExternalSubset1 => self.external_subset1(t, n),
            H::Entity0 => self.entity0(t),
            H::Entity1 => self.entity1(t),
            H::Entity2 => self.entity2(t, n),
            H::Entity3 => self.entity3(t),
            H::Entity4 => self.entity4(t),
            H::Entity5 => self.entity5(t, n),
            H::Entity6 => self.entity6(t),
            H::Entity7 => self.entity7(t, n),
            H::Entity8 => self.entity8(t),
            H::Entity9 => self.entity9(t),
            H::Entity10 => self.entity10(t),
            H::Notation0 => self.notation0(t),
            H::Notation1 => self.notation1(t, n),
            H::Notation2 => self.notation2(t),
            H::Notation3 => self.notation3(t),
            H::Notation4 => self.notation4(t),
            H::Attlist0 => self.attlist0(t),
            H::Attlist1 => self.attlist1(t),
            H::Attlist2 => self.attlist2(t, n),
            H::Attlist3 => self.attlist3(t),
            H::Attlist4 => self.attlist4(t),
            H::Attlist5 => self.attlist5(t),
            H::Attlist6 => self.attlist6(t),
            H::Attlist7 => self.attlist7(t),
            H::Attlist8 => self.attlist8(t, n),
            H::Attlist9 => self.attlist9(t),
            H::Element0 => self.element0(t),
            H::Element1 => self.element1(t, n),
            H::Element2 => self.element2(t, n),
            H::Element3 => self.element3(t),
            H::Element4 => self.element4(t),
            H::Element5 => self.element5(t),
            H::Element6 => self.element6(t),
            H::Element7 => self.element7(t),
            H::CondSect0 => self.cond_sect0(t, n),
            H::CondSect1 => self.cond_sect1(t),
            H::CondSect2 => self.cond_sect2(t),
            H::DeclClose => self.decl_close(t),
            // `error`: only reached if the parser failed to stop.
            H::Error => Role::None,
        }
    }

    /// Moves to `h` and returns `r`.
    fn go(&mut self, h: Handler, r: Role) -> Role {
        self.handler = h;
        r
    }

    /// Moves to `declClose`, which answers `role_none`, and returns `r`.
    fn close_with(&mut self, role_none: Role, r: Role) -> Role {
        self.handler = Handler::DeclClose;
        self.role_none = role_none;
        r
    }

    /// `setTopLevel(state)` (with `XML_DTD`), then returns `r`.
    fn top_level(&mut self, r: Role) -> Role {
        self.handler = if self.document_entity {
            Handler::InternalSubset
        } else {
            Handler::ExternalSubset1
        };
        r
    }

    /// `common(state, tok)`.
    fn common(&mut self, t: T) -> Role {
        if !self.document_entity && t == T::Tok(Tok::ParamEntityRef) {
            return Role::InnerParamEntityRef;
        }
        self.handler = Handler::Error;
        Role::Error
    }

    fn prolog0<E: Encoding>(&mut self, t: T, n: &Name<'_, E>) -> Role {
        match t {
            T::Tok(Tok::PrologS) => self.go(Handler::Prolog1, Role::None),
            T::Tok(Tok::XmlDecl) => self.go(Handler::Prolog1, Role::XmlDecl),
            T::Tok(Tok::Pi) => self.go(Handler::Prolog1, Role::Pi),
            T::Tok(Tok::Comment) => self.go(Handler::Prolog1, Role::Comment),
            T::Tok(Tok::Bom) => Role::None,
            T::Tok(Tok::DeclOpen) if n.decl_is(b"DOCTYPE") => {
                self.go(Handler::Doctype0, Role::DoctypeNone)
            }
            T::Tok(Tok::InstanceStart) => self.go(Handler::Error, Role::InstanceStart),
            _ => self.common(t),
        }
    }

    fn prolog1<E: Encoding>(&mut self, t: T, n: &Name<'_, E>) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::None,
            T::Tok(Tok::Pi) => Role::Pi,
            T::Tok(Tok::Comment) => Role::Comment,
            // Cannot arise: the tokenizer rejects a BOM after the first
            // token.
            T::Tok(Tok::Bom) => Role::None,
            T::Tok(Tok::DeclOpen) if n.decl_is(b"DOCTYPE") => {
                self.go(Handler::Doctype0, Role::DoctypeNone)
            }
            T::Tok(Tok::InstanceStart) => self.go(Handler::Error, Role::InstanceStart),
            _ => self.common(t),
        }
    }

    fn prolog2(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::None,
            T::Tok(Tok::Pi) => Role::Pi,
            T::Tok(Tok::Comment) => Role::Comment,
            T::Tok(Tok::InstanceStart) => self.go(Handler::Error, Role::InstanceStart),
            _ => self.common(t),
        }
    }

    fn doctype0(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::DoctypeNone,
            T::Tok(Tok::Name | Tok::PrefixedName) => self.go(Handler::Doctype1, Role::DoctypeName),
            _ => self.common(t),
        }
    }

    fn doctype1<E: Encoding>(&mut self, t: T, n: &Name<'_, E>) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::DoctypeNone,
            T::Tok(Tok::OpenBracket) => {
                self.go(Handler::InternalSubset, Role::DoctypeInternalSubset)
            }
            T::Tok(Tok::DeclClose) => self.go(Handler::Prolog2, Role::DoctypeClose),
            T::Tok(Tok::Name) if n.is(b"SYSTEM") => self.go(Handler::Doctype3, Role::DoctypeNone),
            T::Tok(Tok::Name) if n.is(b"PUBLIC") => self.go(Handler::Doctype2, Role::DoctypeNone),
            _ => self.common(t),
        }
    }

    fn doctype2(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::DoctypeNone,
            T::Tok(Tok::Literal) => self.go(Handler::Doctype3, Role::DoctypePublicId),
            _ => self.common(t),
        }
    }

    fn doctype3(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::DoctypeNone,
            T::Tok(Tok::Literal) => self.go(Handler::Doctype4, Role::DoctypeSystemId),
            _ => self.common(t),
        }
    }

    fn doctype4(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::DoctypeNone,
            T::Tok(Tok::OpenBracket) => {
                self.go(Handler::InternalSubset, Role::DoctypeInternalSubset)
            }
            T::Tok(Tok::DeclClose) => self.go(Handler::Prolog2, Role::DoctypeClose),
            _ => self.common(t),
        }
    }

    fn doctype5(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::DoctypeNone,
            T::Tok(Tok::DeclClose) => self.go(Handler::Prolog2, Role::DoctypeClose),
            _ => self.common(t),
        }
    }

    fn internal_subset<E: Encoding>(&mut self, t: T, n: &Name<'_, E>) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::None,
            T::Tok(Tok::DeclOpen) if n.decl_is(b"ENTITY") => {
                self.go(Handler::Entity0, Role::EntityNone)
            }
            T::Tok(Tok::DeclOpen) if n.decl_is(b"ATTLIST") => {
                self.go(Handler::Attlist0, Role::AttlistNone)
            }
            T::Tok(Tok::DeclOpen) if n.decl_is(b"ELEMENT") => {
                self.go(Handler::Element0, Role::ElementNone)
            }
            T::Tok(Tok::DeclOpen) if n.decl_is(b"NOTATION") => {
                self.go(Handler::Notation0, Role::NotationNone)
            }
            T::Tok(Tok::Pi) => Role::Pi,
            T::Tok(Tok::Comment) => Role::Comment,
            T::Tok(Tok::ParamEntityRef) => Role::ParamEntityRef,
            T::Tok(Tok::CloseBracket) => self.go(Handler::Doctype5, Role::DoctypeNone),
            T::None => Role::None,
            _ => self.common(t),
        }
    }

    fn external_subset0<E: Encoding>(&mut self, t: T, n: &Name<'_, E>) -> Role {
        self.handler = Handler::ExternalSubset1;
        if t == T::Tok(Tok::XmlDecl) {
            return Role::TextDecl;
        }
        self.external_subset1(t, n)
    }

    fn external_subset1<E: Encoding>(&mut self, t: T, n: &Name<'_, E>) -> Role {
        match t {
            T::Tok(Tok::CondSectOpen) => self.go(Handler::CondSect0, Role::None),
            T::Tok(Tok::CondSectClose) if self.include_level != 0 => {
                self.include_level = self.include_level.wrapping_sub(1);
                Role::None
            }
            T::Tok(Tok::CondSectClose) | T::Tok(Tok::CloseBracket) => self.common(t),
            T::Tok(Tok::PrologS) => Role::None,
            T::None if self.include_level != 0 => self.common(t),
            T::None => Role::None,
            _ => self.internal_subset(t, n),
        }
    }

    fn entity0(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::EntityNone,
            T::Tok(Tok::Percent) => self.go(Handler::Entity1, Role::EntityNone),
            T::Tok(Tok::Name) => self.go(Handler::Entity2, Role::GeneralEntityName),
            _ => self.common(t),
        }
    }

    fn entity1(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::EntityNone,
            T::Tok(Tok::Name) => self.go(Handler::Entity7, Role::ParamEntityName),
            _ => self.common(t),
        }
    }

    fn entity2<E: Encoding>(&mut self, t: T, n: &Name<'_, E>) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::EntityNone,
            T::Tok(Tok::Name) if n.is(b"SYSTEM") => self.go(Handler::Entity4, Role::EntityNone),
            T::Tok(Tok::Name) if n.is(b"PUBLIC") => self.go(Handler::Entity3, Role::EntityNone),
            T::Tok(Tok::Literal) => self.close_with(Role::EntityNone, Role::EntityValue),
            _ => self.common(t),
        }
    }

    fn entity3(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::EntityNone,
            T::Tok(Tok::Literal) => self.go(Handler::Entity4, Role::EntityPublicId),
            _ => self.common(t),
        }
    }

    fn entity4(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::EntityNone,
            T::Tok(Tok::Literal) => self.go(Handler::Entity5, Role::EntitySystemId),
            _ => self.common(t),
        }
    }

    fn entity5<E: Encoding>(&mut self, t: T, n: &Name<'_, E>) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::EntityNone,
            T::Tok(Tok::DeclClose) => self.top_level(Role::EntityComplete),
            T::Tok(Tok::Name) if n.is(b"NDATA") => self.go(Handler::Entity6, Role::EntityNone),
            _ => self.common(t),
        }
    }

    fn entity6(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::EntityNone,
            T::Tok(Tok::Name) => self.close_with(Role::EntityNone, Role::EntityNotationName),
            _ => self.common(t),
        }
    }

    fn entity7<E: Encoding>(&mut self, t: T, n: &Name<'_, E>) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::EntityNone,
            T::Tok(Tok::Name) if n.is(b"SYSTEM") => self.go(Handler::Entity9, Role::EntityNone),
            T::Tok(Tok::Name) if n.is(b"PUBLIC") => self.go(Handler::Entity8, Role::EntityNone),
            T::Tok(Tok::Literal) => self.close_with(Role::EntityNone, Role::EntityValue),
            _ => self.common(t),
        }
    }

    fn entity8(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::EntityNone,
            T::Tok(Tok::Literal) => self.go(Handler::Entity9, Role::EntityPublicId),
            _ => self.common(t),
        }
    }

    fn entity9(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::EntityNone,
            T::Tok(Tok::Literal) => self.go(Handler::Entity10, Role::EntitySystemId),
            _ => self.common(t),
        }
    }

    fn entity10(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::EntityNone,
            T::Tok(Tok::DeclClose) => self.top_level(Role::EntityComplete),
            _ => self.common(t),
        }
    }

    fn notation0(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::NotationNone,
            T::Tok(Tok::Name) => self.go(Handler::Notation1, Role::NotationName),
            _ => self.common(t),
        }
    }

    fn notation1<E: Encoding>(&mut self, t: T, n: &Name<'_, E>) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::NotationNone,
            T::Tok(Tok::Name) if n.is(b"SYSTEM") => self.go(Handler::Notation3, Role::NotationNone),
            T::Tok(Tok::Name) if n.is(b"PUBLIC") => self.go(Handler::Notation2, Role::NotationNone),
            _ => self.common(t),
        }
    }

    fn notation2(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::NotationNone,
            T::Tok(Tok::Literal) => self.go(Handler::Notation4, Role::NotationPublicId),
            _ => self.common(t),
        }
    }

    fn notation3(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::NotationNone,
            T::Tok(Tok::Literal) => self.close_with(Role::NotationNone, Role::NotationSystemId),
            _ => self.common(t),
        }
    }

    fn notation4(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::NotationNone,
            T::Tok(Tok::Literal) => self.close_with(Role::NotationNone, Role::NotationSystemId),
            T::Tok(Tok::DeclClose) => self.top_level(Role::NotationNoSystemId),
            _ => self.common(t),
        }
    }

    fn attlist0(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::AttlistNone,
            T::Tok(Tok::Name | Tok::PrefixedName) => {
                self.go(Handler::Attlist1, Role::AttlistElementName)
            }
            _ => self.common(t),
        }
    }

    fn attlist1(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::AttlistNone,
            T::Tok(Tok::DeclClose) => self.top_level(Role::AttlistNone),
            T::Tok(Tok::Name | Tok::PrefixedName) => {
                self.go(Handler::Attlist2, Role::AttributeName)
            }
            _ => self.common(t),
        }
    }

    fn attlist2<E: Encoding>(&mut self, t: T, n: &Name<'_, E>) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::AttlistNone,
            T::Tok(Tok::Name) => {
                if let Some(&(_, role)) = ATTRIBUTE_TYPES.iter().find(|(kw, _)| n.is(kw)) {
                    return self.go(Handler::Attlist8, role);
                }
                if n.is(b"NOTATION") {
                    return self.go(Handler::Attlist5, Role::AttlistNone);
                }
                self.common(t)
            }
            T::Tok(Tok::OpenParen) => self.go(Handler::Attlist3, Role::AttlistNone),
            _ => self.common(t),
        }
    }

    fn attlist3(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::AttlistNone,
            T::Tok(Tok::Nmtoken | Tok::Name | Tok::PrefixedName) => {
                self.go(Handler::Attlist4, Role::AttributeEnumValue)
            }
            _ => self.common(t),
        }
    }

    fn attlist4(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::AttlistNone,
            T::Tok(Tok::CloseParen) => self.go(Handler::Attlist8, Role::AttlistNone),
            T::Tok(Tok::Or) => self.go(Handler::Attlist3, Role::AttlistNone),
            _ => self.common(t),
        }
    }

    fn attlist5(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::AttlistNone,
            T::Tok(Tok::OpenParen) => self.go(Handler::Attlist6, Role::AttlistNone),
            _ => self.common(t),
        }
    }

    fn attlist6(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::AttlistNone,
            T::Tok(Tok::Name) => self.go(Handler::Attlist7, Role::AttributeNotationValue),
            _ => self.common(t),
        }
    }

    fn attlist7(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::AttlistNone,
            T::Tok(Tok::CloseParen) => self.go(Handler::Attlist8, Role::AttlistNone),
            T::Tok(Tok::Or) => self.go(Handler::Attlist6, Role::AttlistNone),
            _ => self.common(t),
        }
    }

    /// default value
    fn attlist8<E: Encoding>(&mut self, t: T, n: &Name<'_, E>) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::AttlistNone,
            T::Tok(Tok::PoundName) if n.pound_is(b"IMPLIED") => {
                self.go(Handler::Attlist1, Role::ImpliedAttributeValue)
            }
            T::Tok(Tok::PoundName) if n.pound_is(b"REQUIRED") => {
                self.go(Handler::Attlist1, Role::RequiredAttributeValue)
            }
            T::Tok(Tok::PoundName) if n.pound_is(b"FIXED") => {
                self.go(Handler::Attlist9, Role::AttlistNone)
            }
            T::Tok(Tok::Literal) => self.go(Handler::Attlist1, Role::DefaultAttributeValue),
            _ => self.common(t),
        }
    }

    fn attlist9(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::AttlistNone,
            T::Tok(Tok::Literal) => self.go(Handler::Attlist1, Role::FixedAttributeValue),
            _ => self.common(t),
        }
    }

    fn element0(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::ElementNone,
            T::Tok(Tok::Name | Tok::PrefixedName) => self.go(Handler::Element1, Role::ElementName),
            _ => self.common(t),
        }
    }

    fn element1<E: Encoding>(&mut self, t: T, n: &Name<'_, E>) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::ElementNone,
            T::Tok(Tok::Name) if n.is(b"EMPTY") => {
                self.close_with(Role::ElementNone, Role::ContentEmpty)
            }
            T::Tok(Tok::Name) if n.is(b"ANY") => self.close_with(Role::ElementNone, Role::ContentAny),
            T::Tok(Tok::OpenParen) => {
                self.level = 1;
                self.go(Handler::Element2, Role::GroupOpen)
            }
            _ => self.common(t),
        }
    }

    fn element2<E: Encoding>(&mut self, t: T, n: &Name<'_, E>) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::ElementNone,
            T::Tok(Tok::PoundName) if n.pound_is(b"PCDATA") => {
                self.go(Handler::Element3, Role::ContentPcdata)
            }
            T::Tok(Tok::OpenParen) => {
                self.level = 2;
                self.go(Handler::Element6, Role::GroupOpen)
            }
            _ => self.content_particle(t),
        }
    }

    /// The name cases shared by `element2` and `element6`.
    fn content_particle(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::Name | Tok::PrefixedName) => self.go(Handler::Element7, Role::ContentElement),
            T::Tok(Tok::NameQuestion) => self.go(Handler::Element7, Role::ContentElementOpt),
            T::Tok(Tok::NameAsterisk) => self.go(Handler::Element7, Role::ContentElementRep),
            T::Tok(Tok::NamePlus) => self.go(Handler::Element7, Role::ContentElementPlus),
            _ => self.common(t),
        }
    }

    fn element3(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::ElementNone,
            T::Tok(Tok::CloseParen) => self.close_with(Role::ElementNone, Role::GroupClose),
            T::Tok(Tok::CloseParenAsterisk) => {
                self.close_with(Role::ElementNone, Role::GroupCloseRep)
            }
            T::Tok(Tok::Or) => self.go(Handler::Element4, Role::ElementNone),
            _ => self.common(t),
        }
    }

    fn element4(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::ElementNone,
            T::Tok(Tok::Name | Tok::PrefixedName) => {
                self.go(Handler::Element5, Role::ContentElement)
            }
            _ => self.common(t),
        }
    }

    fn element5(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::ElementNone,
            T::Tok(Tok::CloseParenAsterisk) => {
                self.close_with(Role::ElementNone, Role::GroupCloseRep)
            }
            T::Tok(Tok::Or) => self.go(Handler::Element4, Role::ElementNone),
            _ => self.common(t),
        }
    }

    fn element6(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::ElementNone,
            T::Tok(Tok::OpenParen) => {
                // Bounded by the number of `(` tokens, i.e. the input length.
                self.level = self.level.wrapping_add(1);
                Role::GroupOpen
            }
            _ => self.content_particle(t),
        }
    }

    /// A `)` variant in `element7`: one level up; at level 0 the content
    /// model is complete.
    fn group_close(&mut self, r: Role) -> Role {
        // `level >= 1` in `element7` (C uses `unsigned` arithmetic).
        self.level = self.level.wrapping_sub(1);
        if self.level == 0 {
            self.handler = Handler::DeclClose;
            self.role_none = Role::ElementNone;
        }
        r
    }

    fn element7(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::ElementNone,
            T::Tok(Tok::CloseParen) => self.group_close(Role::GroupClose),
            T::Tok(Tok::CloseParenAsterisk) => self.group_close(Role::GroupCloseRep),
            T::Tok(Tok::CloseParenQuestion) => self.group_close(Role::GroupCloseOpt),
            T::Tok(Tok::CloseParenPlus) => self.group_close(Role::GroupClosePlus),
            T::Tok(Tok::Comma) => self.go(Handler::Element6, Role::GroupSequence),
            T::Tok(Tok::Or) => self.go(Handler::Element6, Role::GroupChoice),
            _ => self.common(t),
        }
    }

    fn cond_sect0<E: Encoding>(&mut self, t: T, n: &Name<'_, E>) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::None,
            T::Tok(Tok::Name) if n.is(b"INCLUDE") => self.go(Handler::CondSect1, Role::None),
            T::Tok(Tok::Name) if n.is(b"IGNORE") => self.go(Handler::CondSect2, Role::None),
            _ => self.common(t),
        }
    }

    fn cond_sect1(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::None,
            T::Tok(Tok::OpenBracket) => {
                // Bounded by the number of `<![INCLUDE[` sections.
                self.include_level = self.include_level.wrapping_add(1);
                self.go(Handler::ExternalSubset1, Role::None)
            }
            _ => self.common(t),
        }
    }

    fn cond_sect2(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => Role::None,
            T::Tok(Tok::OpenBracket) => self.go(Handler::ExternalSubset1, Role::IgnoreSect),
            _ => self.common(t),
        }
    }

    fn decl_close(&mut self, t: T) -> Role {
        match t {
            T::Tok(Tok::PrologS) => self.role_none,
            T::Tok(Tok::DeclClose) => {
                let r = self.role_none;
                self.top_level(r)
            }
            _ => self.common(t),
        }
    }
}

/// The token text, for the keyword comparisons (`XmlNameMatchesAscii`).
struct Name<'a, E> {
    enc: &'a E,
    /// The buffer up to the end of the token.
    buf: &'a [u8],
    ptr: usize,
}

impl<E: Encoding> Name<'_, E> {
    /// The token is the name `kw`.
    fn is(&self, kw: &[u8]) -> bool {
        name_matches_ascii(self.enc, self.buf, self.ptr, kw)
    }

    /// The token after its first `skip` characters is `kw`.
    fn skip_is(&self, skip: usize, kw: &[u8]) -> bool {
        let p = self.ptr.saturating_add(skip.saturating_mul(E::MINBPC));
        name_matches_ascii(self.enc, self.buf, p, kw)
    }

    /// A `XML_TOK_DECL_OPEN` (`<!` + name) for the keyword `kw`.
    fn decl_is(&self, kw: &[u8]) -> bool {
        self.skip_is(2, kw)
    }

    /// A `XML_TOK_POUND_NAME` (`#` + name) for the keyword `kw`.
    fn pound_is(&self, kw: &[u8]) -> bool {
        self.skip_is(1, kw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tok::Normal;

    #[test]
    fn error_state_answers_none() {
        let mut s = PrologState::new();
        let enc = Normal::Utf8;
        assert_eq!(s.token_role(Scan::Token(Tok::Literal), &enc, b"''", 0, 2), Role::Error);
        assert_eq!(s.handler, Handler::Error);
        assert_eq!(s.token_role(Scan::Token(Tok::PrologS), &enc, b" ", 0, 1), Role::None);
    }

    #[test]
    fn external_entity_inner_param_entity_ref() {
        let mut s = PrologState::new();
        s.init_external_entity();
        let enc = Normal::Utf8;
        // In the external subset a parameter entity reference after a
        // keyword is allowed (common() answers INNER_PARAM_ENTITY_REF).
        let doc = b"<!ENTITY%pe;";
        assert_eq!(s.token_role(Scan::Token(Tok::DeclOpen), &enc, doc, 0, 8), Role::EntityNone);
        assert_eq!(
            s.token_role(Scan::Token(Tok::ParamEntityRef), &enc, doc, 8, 12),
            Role::InnerParamEntityRef
        );
        assert_eq!(s.handler, Handler::Entity0);
    }
}
