//! The parser core of `xmlparse.c`: parser state, the input buffer, and the
//! processors that turn tokens into events.
//!
//! [`Parser`] holds the state of `struct XML_ParserStruct` that the ported
//! processors use. Input arrives through [`Parser::parse`] (`XML_Parse`) or
//! [`Parser::get_buffer`] + [`Parser::parse_buffer`] (`XML_GetBuffer` /
//! `XML_ParseBuffer`); the buffer is managed exactly like C's (initial size,
//! doubling, the 1024 bytes of context kept before the parse position,
//! `XML_CONTEXT_BYTES`), because the reparse-deferral heuristic of
//! `callProcessor` depends on its free space and therefore decides when a
//! partial token is scanned again.
//!
//! Events go to a [`Handler`]. C decides some behaviour by whether a
//! handler is set (for example where the event pointer points after an
//! empty element), so which handlers are "set" is part of the parser
//! configuration ([`HandlerSet`]) rather than of the handler object.
//!
//! Scope (`docs/STATE.md`): the prolog, the document type declaration with
//! entities and the amplification limits (module 11), external entity
//! parsers driven by the external entity reference handler (module 11d),
//! content, attributes, namespaces, CDATA sections and the epilog. The
//! application-facing API (`XML_StopParser`, the default handler,
//! unknown-encoding handlers) is module 12.
//!
//! ```
//! # #[cfg(feature = "internal-api")] {
//! use expat::internal::parse::{Handler, HandlerSet, Parser, Status};
//!
//! #[derive(Default)]
//! struct Names(Vec<Vec<u8>>);
//! impl Handler for Names {
//!     fn start_element(&mut self, name: &[u8], _atts: &[(&[u8], &[u8])], _specified: i32) {
//!         self.0.push(name.to_vec());
//!     }
//! }
//!
//! let mut p = Parser::new(None, None, [0; 16]);
//! p.set_handlers(HandlerSet { start_element: true, ..HandlerSet::default() });
//! let mut names = Names::default();
//! assert_eq!(p.parse(&mut names, b"<doc><a x='1'/></doc>", true), Status::Ok);
//! assert_eq!(names.0, [b"doc".to_vec(), b"a".to_vec()]);
//! # }
//! ```

// Offsets into the parse buffer are bounded by its length, which is at
// most `i32::MAX` like C's `int` buffer sizes; counters that C lets wrap
// use explicit wrapping operations.
// reason: all arithmetic in this module tree is bounded as stated above.
#![allow(clippy::arithmetic_side_effects)]

mod accounting;
pub mod c_layout;
mod content;
mod decl;
mod dtd;
mod entity;
mod error;
mod external;
mod model;
mod prolog;
mod tracker;

#[cfg(test)]
mod tests;

pub use accounting::{Account, Accounting};
pub use tracker::{AllocTracker, PoolShadow, TableShadow, Tracker, TrackerHeap, TRACKER_SIZES};
pub use dtd::{
    AttributeId, Binding, BindingId, DefaultAttribute, Dtd, ElementType, Entity, EntityRef, Prefix,
    PrefixId,
};
pub use model::{ContentModel, ContentNode, ContentQuant, ContentType, Scaffold};
pub use error::{Stop, XmlError};
pub use external::{ExternalEntities, ExternalEntityParser};

use alloc::vec;
use alloc::vec::Vec;

use crate::siphash::SipKey;
use crate::tok::{Attribute, InitEncoding, Known, Position, update_position};

/// `INIT_BUFFER_SIZE`: the first buffer is twice this.
const INIT_BUFFER_SIZE: i32 = 1024;
/// `XML_CONTEXT_BYTES`: bytes kept before the parse position (D-002).
const XML_CONTEXT_BYTES: usize = 1024;
/// `INIT_DATA_BUF_SIZE`: the buffer character data is converted into.
pub(crate) const INIT_DATA_BUF_SIZE: usize = 1024;
/// `INIT_ATTS_SIZE`
pub(crate) const INIT_ATTS_SIZE: usize = 16;

/// `enum XML_Status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    /// `XML_STATUS_ERROR`
    Error,
    /// `XML_STATUS_OK`
    Ok,
    /// `XML_STATUS_SUSPENDED`
    Suspended,
}

/// `enum XML_Parsing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Parsing {
    /// `XML_INITIALIZED`
    Initialized,
    /// `XML_PARSING`
    Parsing,
    /// `XML_FINISHED`
    Finished,
    /// `XML_SUSPENDED`
    Suspended,
}

/// `enum XML_ParamEntityParsing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParamEntityParsing {
    /// `XML_PARAM_ENTITY_PARSING_NEVER`
    Never,
    /// `XML_PARAM_ENTITY_PARSING_UNLESS_STANDALONE`
    UnlessStandalone,
    /// `XML_PARAM_ENTITY_PARSING_ALWAYS`
    Always,
}

/// Which handlers are set (C: which handler pointers are non-`NULL`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct HandlerSet {
    /// `XML_SetXmlDeclHandler`
    pub xml_decl: bool,
    /// `XML_SetStartElementHandler`
    pub start_element: bool,
    /// `XML_SetEndElementHandler`
    pub end_element: bool,
    /// `XML_SetCharacterDataHandler`
    pub character_data: bool,
    /// `XML_SetProcessingInstructionHandler`
    pub processing_instruction: bool,
    /// `XML_SetCommentHandler`
    pub comment: bool,
    /// `XML_SetStartCdataSectionHandler`
    pub start_cdata_section: bool,
    /// `XML_SetEndCdataSectionHandler`
    pub end_cdata_section: bool,
    /// `XML_SetStartNamespaceDeclHandler`
    pub start_namespace_decl: bool,
    /// `XML_SetEndNamespaceDeclHandler`
    pub end_namespace_decl: bool,
    /// `XML_SetSkippedEntityHandler`
    pub skipped_entity: bool,
    /// `XML_SetStartDoctypeDeclHandler`
    pub start_doctype_decl: bool,
    /// `XML_SetEndDoctypeDeclHandler`
    pub end_doctype_decl: bool,
    /// `XML_SetEntityDeclHandler`
    pub entity_decl: bool,
    /// `XML_SetUnparsedEntityDeclHandler`
    pub unparsed_entity_decl: bool,
    /// `XML_SetNotationDeclHandler`
    pub notation_decl: bool,
    /// `XML_SetElementDeclHandler`
    pub element_decl: bool,
    /// `XML_SetAttlistDeclHandler`
    pub attlist_decl: bool,
    /// `XML_SetExternalEntityRefHandler` (module 11d). The library never
    /// reads an external entity itself; the handler decides.
    pub external_entity_ref: bool,
    /// `XML_SetNotStandaloneHandler` (module 11d).
    pub not_standalone: bool,
}

/// Receives the parser's events. Only the methods whose [`HandlerSet`]
/// flag is set are called. Strings are UTF-8 (`XML_Char`, D-002); a C
/// `NULL` string is `None`.
pub trait Handler {
    /// `XML_XmlDeclHandler`: `standalone` is -1 (absent), 0 or 1.
    fn xml_decl(&mut self, _version: Option<&[u8]>, _encoding: Option<&[u8]>, _standalone: i32) {}
    /// `XML_StartElementHandler`: the attributes as (name, value) pairs in
    /// libexpat's order, and `XML_GetSpecifiedAttributeCount` (twice the
    /// number of attributes given in the tag).
    fn start_element(&mut self, _name: &[u8], _atts: &[(&[u8], &[u8])], _specified: i32) {}
    /// `XML_EndElementHandler`.
    fn end_element(&mut self, _name: &[u8]) {}
    /// `XML_CharacterDataHandler`.
    fn character_data(&mut self, _data: &[u8]) {}
    /// `XML_ProcessingInstructionHandler`.
    fn processing_instruction(&mut self, _target: &[u8], _data: &[u8]) {}
    /// `XML_CommentHandler`.
    fn comment(&mut self, _data: &[u8]) {}
    /// `XML_StartCdataSectionHandler`.
    fn start_cdata_section(&mut self) {}
    /// `XML_EndCdataSectionHandler`.
    fn end_cdata_section(&mut self) {}
    /// `XML_StartNamespaceDeclHandler`.
    fn start_namespace_decl(&mut self, _prefix: Option<&[u8]>, _uri: Option<&[u8]>) {}
    /// `XML_EndNamespaceDeclHandler`.
    fn end_namespace_decl(&mut self, _prefix: Option<&[u8]>) {}
    /// `XML_SkippedEntityHandler`.
    fn skipped_entity(&mut self, _name: &[u8], _is_param: bool) {}
    /// `XML_ExternalEntityRefHandler`: `context` is `None` for parameter
    /// entities and the external subset, otherwise C's `getContext`
    /// string. `parsers` creates the external entity parser
    /// (`XML_ExternalEntityParserCreate`) that reads the entity's bytes,
    /// which the handler supplies: the library never fetches anything.
    /// Returns `false` for `XML_STATUS_ERROR`, which stops the parse with
    /// `XML_ERROR_EXTERNAL_ENTITY_HANDLING`. The default refuses.
    fn external_entity_ref(
        &mut self,
        _parsers: &mut ExternalEntities<'_>,
        _context: Option<&[u8]>,
        _base: Option<&[u8]>,
        _system_id: Option<&[u8]>,
        _public_id: Option<&[u8]>,
    ) -> bool {
        false
    }
    /// `XML_NotStandaloneHandler`: `false` stops the parse with
    /// `XML_ERROR_NOT_STANDALONE`. The default refuses.
    fn not_standalone(&mut self) -> bool {
        false
    }
    /// `XML_StartDoctypeDeclHandler`.
    fn start_doctype_decl(
        &mut self,
        _name: &[u8],
        _sysid: Option<&[u8]>,
        _pubid: Option<&[u8]>,
        _has_internal_subset: bool,
    ) {
    }
    /// `XML_EndDoctypeDeclHandler`.
    fn end_doctype_decl(&mut self) {}
    /// `XML_EntityDeclHandler`: `value` is the replacement text of an
    /// internal entity (`None` for an external one).
    #[allow(clippy::too_many_arguments)] // reason: mirrors XML_EntityDeclHandler.
    fn entity_decl(
        &mut self,
        _name: &[u8],
        _is_param: bool,
        _value: Option<&[u8]>,
        _base: Option<&[u8]>,
        _sysid: Option<&[u8]>,
        _pubid: Option<&[u8]>,
        _notation: Option<&[u8]>,
    ) {
    }
    /// `XML_UnparsedEntityDeclHandler`.
    fn unparsed_entity_decl(
        &mut self,
        _name: &[u8],
        _base: Option<&[u8]>,
        _sysid: Option<&[u8]>,
        _pubid: Option<&[u8]>,
        _notation: &[u8],
    ) {
    }
    /// `XML_NotationDeclHandler`.
    fn notation_decl(
        &mut self,
        _name: &[u8],
        _base: Option<&[u8]>,
        _sysid: Option<&[u8]>,
        _pubid: Option<&[u8]>,
    ) {
    }
    /// `XML_ElementDeclHandler`.
    fn element_decl(&mut self, _name: &[u8], _model: &ContentModel) {}
    /// `XML_AttlistDeclHandler`: `dflt` is `None` for `#IMPLIED` and
    /// `#REQUIRED`; `is_required` is true for `#REQUIRED` and `#FIXED`.
    fn attlist_decl(
        &mut self,
        _elname: &[u8],
        _attname: &[u8],
        _att_type: &[u8],
        _dflt: Option<&[u8]>,
        _is_required: bool,
    ) {
    }
}

/// White-box view of the input buffer (offsets from `m_buffer`, -1 for a
/// `NULL` pointer), for the differential tests of buffer management.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferState {
    /// `m_bufferPtr`
    pub buffer_ptr: i64,
    /// `m_bufferEnd`
    pub buffer_end: i64,
    /// `m_bufferLim`
    pub buffer_lim: i64,
    /// `m_partialTokenBytesBefore`
    pub partial_token_bytes_before: usize,
    /// `m_lastBufferRequestSize`
    pub last_buffer_request_size: i32,
    /// `m_parseEndByteIndex`
    pub parse_end_byte_index: u64,
    /// `m_eventPtr`
    pub event_ptr: i64,
    /// `m_positionPtr`
    pub position_ptr: i64,
}

/// The parser's encoding (`m_encoding`): the initial encoding until
/// detection has seen enough bytes, then a built-in encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Enc {
    /// `&m_initEncoding.initEnc`
    Init(InitEncoding),
    /// A built-in encoding.
    Known(Known),
}

/// `m_processor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Processor {
    PrologInit,
    Prolog,
    /// `externalEntityInitProcessor`
    ExternalEntityInit,
    /// `externalEntityInitProcessor2`
    ExternalEntityInit2,
    /// `externalEntityInitProcessor3`
    ExternalEntityInit3,
    /// `externalParEntInitProcessor`
    ExternalParEntInit,
    /// `externalParEntProcessor`
    ExternalParEnt,
    /// `entityValueInitProcessor`
    EntityValueInit,
    /// `entityValueProcessor`
    EntityValue,
    /// `ignoreSectionProcessor`
    IgnoreSection,
    Content,
    CdataSection,
    Epilog,
    /// `internalEntityProcessor`
    InternalEntity,
    Error,
}

/// An open element (`TAG`).
#[derive(Debug, Clone, Default)]
pub(crate) struct Tag {
    /// `rawName[0..rawNameLength]`: the name as it appears in the input.
    pub raw_name: Vec<u8>,
    /// The name as the handlers see it (converted, and expanded with its
    /// namespace URI when namespace processing is on).
    pub name: Vec<u8>,
    /// `bindings`: namespace bindings made by this element.
    pub bindings: Option<BindingId>,
}

/// The parser state (`struct XML_ParserStruct`, the fields modules 9-10
/// use).
#[derive(Debug, Clone)]
pub struct Parser {
    // Input buffer (offsets into `buffer`; C pointers).
    /// `m_buffer[0..bufferLim]`; empty while `m_buffer` is `NULL`.
    pub(crate) buffer: Vec<u8>,
    /// `m_buffer != NULL`
    pub(crate) has_buffer: bool,
    /// `m_bufferPtr`
    pub(crate) buffer_ptr: usize,
    /// `m_bufferEnd`
    pub(crate) buffer_end: usize,
    /// `m_parseEndByteIndex`
    pub(crate) parse_end_byte_index: u64,
    /// `m_parseEndPtr` (`None` = `NULL`)
    pub(crate) parse_end_ptr: Option<usize>,
    /// `m_partialTokenBytesBefore`
    pub(crate) partial_token_bytes_before: usize,
    /// `m_reparseDeferralEnabled`
    pub(crate) reparse_deferral_enabled: bool,
    /// `m_lastBufferRequestSize`
    pub(crate) last_buffer_request_size: i32,
    /// `m_eventPtr`
    pub(crate) event_ptr: Option<usize>,
    /// `m_eventEndPtr`
    pub(crate) event_end_ptr: Option<usize>,
    /// `m_positionPtr`
    pub(crate) position_ptr: Option<usize>,
    /// `m_position`
    pub(crate) position: Position,

    // Configuration.
    /// Which handlers are set.
    pub(crate) handlers: HandlerSet,
    /// `m_protocolEncodingName`
    pub(crate) protocol_encoding_name: Option<Vec<u8>>,
    /// `m_ns`
    pub(crate) ns: bool,
    /// `m_ns_triplets`
    pub(crate) ns_triplets: bool,
    /// `m_namespaceSeparator`
    pub(crate) namespace_separator: u8,
    /// `m_paramEntityParsing`
    pub(crate) param_entity_parsing: ParamEntityParsing,
    /// `m_hash_secret_salt_128`
    pub(crate) salt: SipKey,

    // Parse state.
    /// `m_encoding`
    pub(crate) enc: Enc,
    /// `m_prologState`
    pub(crate) prolog_state: crate::role::PrologState,
    /// `m_processor`
    pub(crate) processor: Processor,
    /// `m_errorCode`
    pub(crate) error_code: Stop,
    /// `m_parsingStatus.parsing`
    pub(crate) parsing: Parsing,
    /// `m_parsingStatus.finalBuffer`
    pub(crate) final_buffer: bool,
    /// `m_reenter`
    pub(crate) reenter: bool,
    /// `m_tagLevel`
    pub(crate) tag_level: i32,
    /// `m_tagStack`, innermost last.
    pub(crate) tag_stack: Vec<Tag>,
    /// `m_inheritedBindings`
    pub(crate) inherited_bindings: Option<BindingId>,
    /// `m_nSpecifiedAtts`
    pub(crate) n_specified_atts: i32,
    /// `m_idAttIndex`
    pub(crate) id_att_index: i32,
    /// `m_atts` (its length is `m_attsSize`).
    pub(crate) atts: Vec<Attribute>,
    /// The attribute list of the last start tag, as the start handler gets
    /// it (C: the `XML_Char *` pairs at the front of `m_atts`).
    pub(crate) app_atts: Vec<(Vec<u8>, Vec<u8>)>,
    /// `m_dtd`
    pub(crate) dtd: Dtd,
    /// `m_accounting`
    pub(crate) accounting: Accounting,

    // Declarations (module 11).
    /// `m_curBase` (`XML_SetBase`, public API).
    pub(crate) cur_base: Option<Vec<u8>>,
    /// `m_doctypeName`
    pub(crate) doctype_name: Option<Vec<u8>>,
    /// `m_doctypeSysid` (`"#"` when no start handler is set).
    pub(crate) doctype_sysid: Option<Vec<u8>>,
    /// `m_doctypePubid`
    pub(crate) doctype_pubid: Option<Vec<u8>>,
    /// `m_useForeignDTD` (`XML_UseForeignDTD`, public API).
    pub(crate) use_foreign_dtd: bool,
    /// `m_isParamEntity`: this parser reads an external parameter entity
    /// (an external entity parser created with a `NULL` context).
    pub(crate) is_param_entity: bool,
    /// `m_parentParser != NULL`: this is an external entity parser. Its
    /// accounting and allocation tracker are the root parser's (moved in
    /// while it runs, [`ExternalEntityParser`]).
    pub(crate) is_child: bool,
    /// The entity value being built when an external parameter entity
    /// referenced inside it is read (C appends to `dtd->entityValuePool`,
    /// which the external entity parser shares); moved into the parameter
    /// entity parser and back.
    pub(crate) pending_value: Vec<u8>,
    /// `m_declEntity`
    pub(crate) decl_entity: Option<EntityRef>,
    /// `m_declNotationName`
    pub(crate) decl_notation_name: Option<Vec<u8>>,
    /// `m_declNotationPublicId`
    pub(crate) decl_notation_public_id: Option<Vec<u8>>,
    /// `m_declElementType`
    pub(crate) decl_element_type: Option<crate::hash::EntryId>,
    /// `m_declAttributeId`
    pub(crate) decl_attribute_id: Option<crate::hash::EntryId>,
    /// `m_declAttributeType`
    pub(crate) decl_attribute_type: Option<Vec<u8>>,
    /// `m_declAttributeIsCdata`
    pub(crate) decl_attribute_is_cdata: bool,
    /// `m_declAttributeIsId`
    pub(crate) decl_attribute_is_id: bool,
    /// `m_groupConnector[0..m_groupSize]`
    pub(crate) group_connector: Vec<u8>,

    // Entity expansion (module 11b).
    /// `m_openInternalEntities`, innermost last.
    pub(crate) open_internal_entities: Vec<entity::OpenEntity>,
    /// `m_openAttributeEntities`, innermost last.
    pub(crate) open_attribute_entities: Vec<entity::OpenEntity>,
    /// `m_openValueEntities`, innermost last.
    pub(crate) open_value_entities: Vec<entity::OpenEntity>,
    /// The length of `m_freeEntities`.
    pub(crate) free_entities: usize,

    // Allocation tracker (module 11c, D-030).
    /// `m_alloc_tracker` and the shadows of C's allocations.
    pub(crate) tracker: Tracker,
    /// Characters of the pool string being built (an attribute value or
    /// entity value) already charged to the tracker.
    pub(crate) charged_len: usize,
}

impl Parser {
    /// `XML_ParserCreate(encoding)` (`ns_separator == None`) or
    /// `XML_ParserCreateNS(encoding, sep)`, followed by
    /// `XML_SetHashSalt16Bytes(salt)`.
    ///
    /// The salt is a parameter because this crate has no entropy source of
    /// its own (it is `no_std`); the public API decides where it comes from
    /// (D-023).
    pub fn new(encoding: Option<&[u8]>, ns_separator: Option<u8>, salt: [u8; 16]) -> Self {
        let ns = ns_separator.is_some();
        Parser {
            buffer: Vec::new(),
            has_buffer: false,
            buffer_ptr: 0,
            buffer_end: 0,
            parse_end_byte_index: 0,
            parse_end_ptr: None,
            partial_token_bytes_before: 0,
            reparse_deferral_enabled: true,
            last_buffer_request_size: 0,
            event_ptr: None,
            event_end_ptr: None,
            position_ptr: None,
            position: Position::default(),
            handlers: HandlerSet::default(),
            protocol_encoding_name: encoding.map(<[u8]>::to_vec),
            ns,
            ns_triplets: false,
            // ASCII_EXCL when namespace processing is off.
            namespace_separator: ns_separator.unwrap_or(b'!'),
            param_entity_parsing: ParamEntityParsing::Never,
            salt: SipKey::from_bytes(salt),
            // `parserInit` initialises the non-namespace encoding without a
            // name, and `initializeEncoding` replaces it before the first scan.
            enc: Enc::Init(InitEncoding::unnamed(false)),
            prolog_state: crate::role::PrologState::new(),
            processor: Processor::PrologInit,
            error_code: Stop::Error(XmlError::None),
            parsing: Parsing::Initialized,
            final_buffer: false,
            reenter: false,
            tag_level: 0,
            tag_stack: Vec::new(),
            inherited_bindings: None,
            n_specified_atts: 0,
            id_att_index: -1,
            atts: vec![Attribute::default(); INIT_ATTS_SIZE],
            app_atts: Vec::new(),
            dtd: Dtd::new(),
            accounting: Accounting::default(),
            cur_base: None,
            doctype_name: None,
            doctype_sysid: None,
            doctype_pubid: None,
            use_foreign_dtd: false,
            is_param_entity: false,
            is_child: false,
            pending_value: Vec::new(),
            decl_entity: None,
            decl_notation_name: None,
            decl_notation_public_id: None,
            decl_element_type: None,
            decl_attribute_id: None,
            decl_attribute_type: None,
            decl_attribute_is_cdata: false,
            decl_attribute_is_id: false,
            group_connector: Vec::new(),
            open_internal_entities: Vec::new(),
            open_attribute_entities: Vec::new(),
            open_value_entities: Vec::new(),
            free_entities: 0,
            tracker: {
                let mut t = Tracker::default();
                t.t.charge_creation(encoding.map(<[u8]>::len));
                t
            },
            charged_len: 0,
        }
    }

    /// Sets which handlers are called (the `XML_Set*Handler` functions).
    pub fn set_handlers(&mut self, handlers: HandlerSet) {
        self.handlers = handlers;
    }

    /// `XML_SetReturnNSTriplet`. Like C, it has no effect once parsing has
    /// started.
    pub fn set_return_ns_triplet(&mut self, on: bool) {
        if self.parsing == Parsing::Initialized {
            self.ns_triplets = on;
        }
    }

    /// `XML_UseForeignDTD`: `false` (C:
    /// `XML_ERROR_CANT_CHANGE_FEATURE_ONCE_PARSING`) once parsing has
    /// started.
    pub fn use_foreign_dtd(&mut self, on: bool) -> bool {
        if matches!(self.parsing, Parsing::Parsing | Parsing::Suspended) {
            return false;
        }
        self.use_foreign_dtd = on;
        true
    }

    /// `XML_SetParamEntityParsing`: `false` once parsing has started.
    pub fn set_param_entity_parsing(&mut self, mode: ParamEntityParsing) -> bool {
        if matches!(self.parsing, Parsing::Parsing | Parsing::Suspended) {
            return false;
        }
        self.param_entity_parsing = mode;
        true
    }

    /// `XML_SetBillionLaughsAttackProtectionMaximumAmplification`
    /// (`xmlparse.c` 3099-3124): `false`, and no change, for NaN or a value
    /// below 1.0. Allowed at any time, like C. (C also refuses it on an
    /// external entity parser, and so does the port.)
    pub fn set_billion_laughs_maximum_amplification(&mut self, factor: f32) -> bool {
        if self.is_child || factor.is_nan() || factor < 1.0 {
            return false;
        }
        self.accounting.maximum_amplification_factor = factor;
        true
    }

    /// `XML_SetBillionLaughsAttackProtectionActivationThreshold`
    /// (`xmlparse.c` 3126-3148): any value, at any time.
    pub fn set_billion_laughs_activation_threshold(&mut self, bytes: u64) -> bool {
        if self.is_child {
            return false;
        }
        self.accounting.activation_threshold_bytes = bytes;
        true
    }

    /// `XML_SetAllocTrackerMaximumAmplification` (`xmlparse.c` 3150-3176):
    /// `false`, and no change, for NaN or a value below 1.0. Allowed at
    /// any time, like C.
    pub fn set_alloc_tracker_maximum_amplification(&mut self, factor: f32) -> bool {
        if self.is_child {
            return false;
        }
        self.tracker.t.set_maximum_amplification(factor)
    }

    /// `XML_SetAllocTrackerActivationThreshold` (`xmlparse.c` 3178-3200):
    /// any value, at any time.
    pub fn set_alloc_tracker_activation_threshold(&mut self, bytes: u64) -> bool {
        if self.is_child {
            return false;
        }
        self.tracker.t.set_activation_threshold(bytes)
    }

    /// The allocation tracker: bytes charged so far (an upper bound of
    /// what libexpat has allocated at the same point, D-030) and the
    /// limits.
    pub fn alloc_tracker(&self) -> &AllocTracker {
        &self.tracker.t
    }

    /// Charges the growth of the current string of pool `pool` to `len`
    /// characters (C appends to the pool as it goes; the port charges after
    /// each token, before the next amplification check).
    pub(crate) fn charge_current(&mut self, pool: tracker::Pool, len: usize) -> Result<(), Stop> {
        if len > self.charged_len {
            let direct = self.direct();
            let (p, mut heap) = self.tracker.pool(pool, direct);
            p.append(&mut heap, len - self.charged_len)
                .map_err(|_| Stop::Error(XmlError::NoMemory))?;
            self.charged_len = len;
        }
        Ok(())
    }

    /// Ends the current string of pool `pool`: its NUL (if `nul`), then
    /// `poolFinish` (`keep`) or `poolDiscard`.
    pub(crate) fn finish_current(&mut self, pool: tracker::Pool, nul: bool, keep: bool) -> Result<(), Stop> {
        let direct = self.direct();
        let (p, mut heap) = self.tracker.pool(pool, direct);
        self.charged_len = 0;
        if nul {
            p.append(&mut heap, 1).map_err(|_| Stop::Error(XmlError::NoMemory))?;
        }
        if keep {
            p.finish();
        } else {
            p.discard();
        }
        Ok(())
    }

    /// `accountingDiffTolerated` for this parser: an external entity
    /// parser's bytes are never direct (`isDirect` needs the root parser),
    /// they count as indirect.
    pub(crate) fn diff_tolerated(&mut self, tok: crate::tok::Scan, bytes: usize, account: Account) -> bool {
        let account = if self.is_child && account == Account::Direct {
            Account::EntityExpansion
        } else {
            account
        };
        self.accounting.diff_tolerated(tok, bytes, account)
    }

    /// `countBytesDirect`, the denominator of both amplification checks.
    pub(crate) fn direct(&self) -> u64 {
        self.accounting.count_bytes_direct
    }

    /// `XML_SetReparseDeferralEnabled`.
    pub fn set_reparse_deferral_enabled(&mut self, enabled: bool) {
        self.reparse_deferral_enabled = enabled;
    }

    /// `XML_GetErrorCode`.
    pub fn error_code(&self) -> Stop {
        self.error_code
    }

    /// `XML_GetSpecifiedAttributeCount`.
    pub fn specified_attribute_count(&self) -> i32 {
        self.n_specified_atts
    }

    /// `XML_GetIdAttributeIndex`.
    pub fn id_attribute_index(&self) -> i32 {
        self.id_att_index
    }

    /// `m_parsingStatus.parsing`.
    pub fn parsing_status(&self) -> Parsing {
        self.parsing
    }

    /// The billion-laughs counters.
    pub fn accounting(&self) -> &Accounting {
        &self.accounting
    }

    /// The state of the input buffer.
    pub fn buffer_state(&self) -> BufferState {
        let off = |p: Option<usize>| match (self.has_buffer, p) {
            (true, Some(p)) => p as i64,
            _ => -1,
        };
        BufferState {
            buffer_ptr: off(Some(self.buffer_ptr)),
            buffer_end: off(Some(self.buffer_end)),
            buffer_lim: off(Some(self.buffer.len())),
            partial_token_bytes_before: self.partial_token_bytes_before,
            last_buffer_request_size: self.last_buffer_request_size,
            parse_end_byte_index: self.parse_end_byte_index,
            event_ptr: off(self.event_ptr),
            position_ptr: off(self.position_ptr),
        }
    }

    /// `XML_GetCurrentByteIndex64`.
    pub fn current_byte_index(&self) -> i64 {
        match (self.event_ptr, self.parse_end_ptr) {
            (Some(ev), Some(pe)) => (self.parse_end_byte_index as i64)
                .wrapping_sub((pe as i64).wrapping_sub(ev as i64)),
            // C computes with a NULL `m_parseEndPtr`, which never happens
            // while `m_eventPtr` is set.
            (Some(ev), None) => (self.parse_end_byte_index as i64).wrapping_add(ev as i64),
            (None, _) => -1,
        }
    }

    /// `XML_GetCurrentByteCount64`.
    pub fn current_byte_count(&self) -> u64 {
        match (self.event_ptr, self.event_end_ptr) {
            (Some(ev), Some(end)) => (end as u64).wrapping_sub(ev as u64),
            _ => 0,
        }
    }

    /// Brings `m_position` up to `m_eventPtr` (the first half of
    /// `XML_GetCurrentLineNumber64` and `XML_GetCurrentColumnNumber64`).
    fn update_position_to_event(&mut self) {
        let (Some(ev), Some(pos)) = (self.event_ptr, self.position_ptr) else {
            return;
        };
        if ev >= pos {
            self.update_position(pos, ev);
            self.position_ptr = Some(ev);
        }
    }

    /// `XmlUpdatePosition(m_encoding, buffer + from, buffer + to, &m_position)`.
    fn update_position(&mut self, from: usize, to: usize) {
        let buf = self.buffer.get(..to).unwrap_or(&self.buffer);
        match self.enc {
            Enc::Init(init) => init.update_position(buf, from, &mut self.position),
            Enc::Known(Known::Normal(e)) => update_position(&e, buf, from, &mut self.position),
            Enc::Known(Known::Utf16(e)) => update_position(&e, buf, from, &mut self.position),
        }
    }

    /// `XML_GetCurrentLineNumber64`.
    pub fn current_line_number(&mut self) -> u64 {
        self.update_position_to_event();
        self.position.line_number.wrapping_add(1)
    }

    /// `XML_GetCurrentColumnNumber64`.
    pub fn current_column_number(&mut self) -> u64 {
        self.update_position_to_event();
        self.position.column_number
    }

    /// `startParsing`: the implicit `xml` prefix binding when namespace
    /// processing is on (the salt is always set, see [`Parser::new`]).
    fn start_parsing<H: Handler>(&mut self, h: &mut H) -> bool {
        // The implicit context is only set for the root parser; external
        // entity parsers inherit it (C skips `startParsing` for them).
        if self.is_child {
            return true;
        }
        if self.ns {
            return self.set_implicit_context(h);
        }
        true
    }

    /// `XML_Parse`.
    pub fn parse<H: Handler>(&mut self, h: &mut H, s: &[u8], is_final: bool) -> Status {
        let Ok(len) = i32::try_from(s.len()) else {
            self.error_code = XmlError::InvalidArgument.into();
            return Status::Error;
        };
        match self.parsing {
            Parsing::Suspended => {
                self.error_code = XmlError::Suspended.into();
                return Status::Error;
            }
            Parsing::Finished => {
                self.error_code = XmlError::Finished.into();
                return Status::Error;
            }
            Parsing::Initialized => {
                if !self.start_parsing(h) {
                    self.error_code = XmlError::NoMemory.into();
                    return Status::Error;
                }
                self.parsing = Parsing::Parsing;
            }
            Parsing::Parsing => {}
        }
        let Some(buf) = self.get_buffer(len) else {
            return Status::Error;
        };
        if let Some(dst) = buf.get_mut(..s.len()) {
            dst.copy_from_slice(s);
        }
        self.parse_buffer(h, len, is_final)
    }

    /// `XML_GetBuffer`: space for `len` more bytes of input at the end of
    /// the buffer, to be committed with [`Parser::parse_buffer`]. `None`
    /// sets the error code.
    pub fn get_buffer(&mut self, len: i32) -> Option<&mut [u8]> {
        if len < 0 {
            self.error_code = XmlError::NoMemory.into();
            return None;
        }
        match self.parsing {
            Parsing::Suspended => {
                self.error_code = XmlError::Suspended.into();
                return None;
            }
            Parsing::Finished => {
                self.error_code = XmlError::Finished.into();
                return None;
            }
            Parsing::Initialized | Parsing::Parsing => {}
        }
        self.last_buffer_request_size = len;
        let lim = self.buffer.len();
        let free = if self.has_buffer {
            lim - self.buffer_end
        } else {
            0
        };
        if len as usize > free || !self.has_buffer {
            let parsing = if self.has_buffer {
                self.buffer_end - self.buffer_ptr
            } else {
                0
            };
            // C: `(int)((unsigned)len + (unsigned)parsing)`, rejected when
            // negative.
            let Ok(needed) = i32::try_from((len as u32).wrapping_add(parsing as u32)) else {
                self.error_code = XmlError::NoMemory.into();
                return None;
            };
            let parsed = if self.has_buffer { self.buffer_ptr } else { 0 };
            let keep = parsed.min(XML_CONTEXT_BYTES);
            let Some(needed) = needed.checked_add(keep as i32) else {
                self.error_code = XmlError::NoMemory.into();
                return None;
            };
            if self.has_buffer && needed as usize <= lim {
                if keep < parsed {
                    let offset = parsed - keep;
                    self.buffer.copy_within(offset..self.buffer_end, 0);
                    self.buffer_end -= offset;
                    self.buffer_ptr -= offset;
                }
            } else {
                let mut size = if lim == 0 { INIT_BUFFER_SIZE } else { lim as i32 };
                loop {
                    size = (2u32.wrapping_mul(size as u32)) as i32;
                    if !(size < needed && size > 0) {
                        break;
                    }
                }
                if size <= 0 {
                    self.error_code = XmlError::NoMemory.into();
                    return None;
                }
                // C allocates the input buffer with the plain `malloc_fcn`,
                // outside the allocation tracker ("to leave limiting the
                // input size to the application", `xmlparse.c` 2601-2603),
                // so the port does not charge it either.
                let mut new_buf = vec![0u8; size as usize];
                if self.has_buffer {
                    let from = self.buffer_ptr - keep;
                    let n = self.buffer_end - from;
                    if let (Some(dst), Some(src)) =
                        (new_buf.get_mut(..n), self.buffer.get(from..self.buffer_end))
                    {
                        dst.copy_from_slice(src);
                    }
                    self.buffer_end = n;
                    self.buffer_ptr = keep;
                } else {
                    self.buffer_end = 0;
                    self.buffer_ptr = 0;
                }
                self.buffer = new_buf;
                self.has_buffer = true;
            }
            self.event_ptr = None;
            self.event_end_ptr = None;
            self.position_ptr = None;
        }
        let end = self.buffer_end;
        self.buffer.get_mut(end..end + len as usize)
    }

    /// `XML_ParseBuffer`: parses `len` bytes written into the space that
    /// [`Parser::get_buffer`] returned.
    pub fn parse_buffer<H: Handler>(&mut self, h: &mut H, len: i32, is_final: bool) -> Status {
        if len < 0 {
            self.error_code = XmlError::InvalidArgument.into();
            return Status::Error;
        }
        match self.parsing {
            Parsing::Suspended => {
                self.error_code = XmlError::Suspended.into();
                return Status::Error;
            }
            Parsing::Finished => {
                self.error_code = XmlError::Finished.into();
                return Status::Error;
            }
            Parsing::Initialized => {
                if !self.has_buffer {
                    self.error_code = XmlError::NoBuffer.into();
                    return Status::Error;
                }
                if !self.start_parsing(h) {
                    self.error_code = XmlError::NoMemory.into();
                    return Status::Error;
                }
                self.parsing = Parsing::Parsing;
            }
            Parsing::Parsing => {}
        }
        let free = if self.has_buffer {
            self.buffer.len() - self.buffer_end
        } else {
            0
        };
        if len as usize > free {
            self.error_code = XmlError::InvalidArgument.into();
            return Status::Error;
        }
        if u64::from(len as u32) > u64::MAX - self.parse_end_byte_index {
            self.error_code = XmlError::NoMemory.into();
            self.event_ptr = None;
            self.event_end_ptr = None;
            self.processor = Processor::Error;
            return Status::Error;
        }
        let start = self.buffer_ptr;
        self.position_ptr = Some(start);
        self.buffer_end += len as usize;
        self.parse_end_ptr = Some(self.buffer_end);
        self.parse_end_byte_index += u64::from(len as u32);
        self.final_buffer = is_final;

        let mut end_ptr = start;
        let result = self.call_processor(h, start, self.buffer_end, &mut end_ptr);
        self.buffer_ptr = end_ptr;
        if let Err(e) = result {
            self.error_code = e;
            self.event_end_ptr = self.event_ptr;
            self.processor = Processor::Error;
            return Status::Error;
        }
        self.error_code = XmlError::None.into();
        let status = match self.parsing {
            Parsing::Suspended => Status::Suspended,
            Parsing::Initialized | Parsing::Parsing => {
                if is_final {
                    self.parsing = Parsing::Finished;
                    return Status::Ok;
                }
                Status::Ok
            }
            Parsing::Finished => Status::Ok,
        };
        if let Some(pos) = self.position_ptr {
            self.update_position(pos, self.buffer_ptr);
        }
        self.position_ptr = Some(self.buffer_ptr);
        status
    }

    /// `callProcessor`: runs the processors on `buffer[start..end]`, with
    /// the reparse-deferral heuristic.
    fn call_processor<H: Handler>(
        &mut self,
        h: &mut H,
        start: usize,
        end: usize,
        end_ptr: &mut usize,
    ) -> Result<(), Stop> {
        let have_now = end - start;
        if self.reparse_deferral_enabled && !self.final_buffer {
            // Heuristic: don't try to parse a partial token again until the
            // amount of available data has increased significantly...
            let had_before = self.partial_token_bytes_before;
            // ...but *do* try anyway if we're close to causing a
            // reallocation.
            let mut available = if self.has_buffer { self.buffer_ptr } else { 0 };
            available -= available.min(XML_CONTEXT_BYTES);
            if self.has_buffer {
                available += self.buffer.len() - self.buffer_end;
            }
            let enough = have_now >= had_before.saturating_mul(2)
                || self.last_buffer_request_size as usize > available;
            if !enough {
                *end_ptr = start;
                return Ok(());
            }
        }
        // The processors read the buffer while they update the parser.
        let buffer = core::mem::take(&mut self.buffer);
        let buf = buffer.get(..end).unwrap_or(&buffer);
        *end_ptr = start;
        let ret = loop {
            let ret = self.run_processor(h, buf, *end_ptr, end_ptr);
            // Make parsing status (and in particular XML_SUSPENDED) take
            // precedence over re-enter flag when they disagree.
            if self.parsing != Parsing::Parsing {
                self.reenter = false;
            }
            if !self.reenter {
                break ret;
            }
            self.reenter = false;
            if ret.is_err() {
                break ret;
            }
        };
        self.buffer = buffer;
        if ret.is_ok() {
            // If we consumed nothing, remember what we had on this parse
            // attempt.
            self.partial_token_bytes_before = if *end_ptr == start { have_now } else { 0 };
        }
        ret
    }

    /// `parser->m_processor(parser, start, end, endPtr)`.
    fn run_processor<H: Handler>(
        &mut self,
        h: &mut H,
        buf: &[u8],
        start: usize,
        end_ptr: &mut usize,
    ) -> Result<(), Stop> {
        match self.processor {
            Processor::PrologInit => self.prolog_init_processor(h, buf, start, end_ptr),
            Processor::Prolog => self.prolog_processor(h, buf, start, end_ptr),
            Processor::ExternalEntityInit => self.external_entity_init_processor(h, buf, start, end_ptr),
            Processor::ExternalEntityInit2 => self.external_entity_init_processor2(h, buf, start, end_ptr),
            Processor::ExternalEntityInit3 => self.external_entity_init_processor3(h, buf, start, end_ptr),
            Processor::ExternalParEntInit => self.external_par_ent_init_processor(h, buf, start, end_ptr),
            Processor::ExternalParEnt => self.external_par_ent_processor(h, buf, start, end_ptr),
            Processor::EntityValueInit => self.entity_value_init_processor(h, buf, start, end_ptr),
            Processor::EntityValue => self.entity_value_processor(h, buf, start, end_ptr),
            Processor::IgnoreSection => self.ignore_section_processor(h, buf, start, end_ptr),
            Processor::Content => self.content_processor(h, buf, start, end_ptr),
            Processor::CdataSection => self.cdata_section_processor(h, buf, start, end_ptr),
            Processor::Epilog => self.epilog_processor(h, buf, start, end_ptr),
            // `internalEntityProcessor` ignores `s`, `end` and `nextPtr`.
            Processor::InternalEntity => self.internal_entity_processor(h),
            // `errorProcessor` returns `m_errorCode`, which a later call
            // that the reparse-deferral heuristic skipped may have reset to
            // XML_ERROR_NONE; then it succeeds, like C.
            Processor::Error => match self.error_code {
                Stop::Error(XmlError::None) => Ok(()),
                e => Err(e),
            },
        }
    }
}
