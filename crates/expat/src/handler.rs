//! Public callbacks (module 12, slice 4, D-041): the [`Handler`] trait and
//! the [`Events`] it receives, used by [`Parser::parse_with`].
//!
//! [`Parser::parse_with`]: crate::Parser::parse_with

use crate::parse::{self, ContentModel};
use alloc::vec::Vec;

/// Which of the [`Handler`] events a parser reports: the C handler
/// pointers that are set (non-`NULL`).
///
/// An event that is off is not reported, and the parser skips the work it
/// does only for it, as libexpat does for a `NULL` handler: a processing
/// instruction or comment is not copied, and an empty element's position
/// is not updated for its start and end events. Turning events off can
/// therefore change allocations (and so a memory-limit error) and the
/// position the parser reports, exactly as clearing the handler in C does.
/// An empty method body in a [`Handler`] is not the same as an event that
/// is off: the event is still reported and its work is done.
///
/// New events may be added in later versions, so the type is built from
/// [`Events::ALL`] or [`Events::NONE`] with its methods:
///
/// ```
/// use expat::Events;
///
/// let e = Events::NONE.start_element(true).end_element(true);
/// assert!(e.start_element && e.end_element && !e.comment);
/// assert_eq!(Events::ALL.comment(false).comment, false);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[allow(clippy::struct_excessive_bools)] // reason: one flag per C handler pointer.
pub struct Events {
    /// [`Handler::start_element`] (`XML_SetStartElementHandler`).
    pub start_element: bool,
    /// [`Handler::end_element`] (`XML_SetEndElementHandler`).
    pub end_element: bool,
    /// [`Handler::character_data`] (`XML_SetCharacterDataHandler`).
    pub character_data: bool,
    /// [`Handler::processing_instruction`]
    /// (`XML_SetProcessingInstructionHandler`).
    pub processing_instruction: bool,
    /// [`Handler::comment`] (`XML_SetCommentHandler`).
    pub comment: bool,
    /// [`Handler::start_cdata_section`] (`XML_SetStartCdataSectionHandler`).
    pub start_cdata_section: bool,
    /// [`Handler::end_cdata_section`] (`XML_SetEndCdataSectionHandler`).
    pub end_cdata_section: bool,
    /// [`Handler::start_namespace_decl`]
    /// (`XML_SetStartNamespaceDeclHandler`).
    pub start_namespace_decl: bool,
    /// [`Handler::end_namespace_decl`] (`XML_SetEndNamespaceDeclHandler`).
    pub end_namespace_decl: bool,
    /// [`Handler::xml_decl`] (`XML_SetXmlDeclHandler`).
    pub xml_decl: bool,
    /// [`Handler::start_doctype_decl`] (`XML_SetStartDoctypeDeclHandler`).
    pub start_doctype_decl: bool,
    /// [`Handler::end_doctype_decl`] (`XML_SetEndDoctypeDeclHandler`).
    pub end_doctype_decl: bool,
    /// [`Handler::entity_decl`] (`XML_SetEntityDeclHandler`).
    pub entity_decl: bool,
    /// [`Handler::unparsed_entity_decl`] (`XML_SetUnparsedEntityDeclHandler`).
    pub unparsed_entity_decl: bool,
    /// [`Handler::notation_decl`] (`XML_SetNotationDeclHandler`).
    pub notation_decl: bool,
    /// [`Handler::element_decl`] (`XML_SetElementDeclHandler`).
    pub element_decl: bool,
    /// [`Handler::attlist_decl`] (`XML_SetAttlistDeclHandler`).
    pub attlist_decl: bool,
    /// [`Handler::skipped_entity`] (`XML_SetSkippedEntityHandler`).
    pub skipped_entity: bool,
    /// [`Handler::position`] before each reported event: the position
    /// `XML_GetCurrentLineNumber`, `XML_GetCurrentColumnNumber` and
    /// `XML_GetCurrentByteIndex` return inside a C handler. Off in both
    /// [`Events::ALL`] and [`Events::NONE`], because counting lines and
    /// columns at every event costs a second pass over the text.
    pub position: bool,
    /// [`Handler::default_data`] (`XML_SetDefaultHandler`): the text of
    /// the document that no other event that is on reports. Off in both
    /// [`Events::ALL`] and [`Events::NONE`].
    pub default_data: bool,
    /// Whether a reference to an internal general entity in content is
    /// expanded (libexpat's `m_defaultExpandInternalEntities`). On in
    /// both [`Events::ALL`] and [`Events::NONE`], as in libexpat. Off is
    /// what `XML_SetDefaultHandler` sets: the reference is then not
    /// expanded and goes to [`Handler::default_data`] as written;
    /// `XML_SetDefaultHandlerExpand` keeps it on.
    pub expand_internal_entities: bool,
    /// [`Handler::event_text`] before each reported event: the text
    /// `XML_DefaultCurrent` passes to the default handler from inside a C
    /// handler. Off in both [`Events::ALL`] and [`Events::NONE`].
    pub event_text: bool,
}

impl Events {
    /// Every event; [`Events::position`] stays off.
    pub const ALL: Events = Events {
        start_element: true,
        end_element: true,
        character_data: true,
        processing_instruction: true,
        comment: true,
        start_cdata_section: true,
        end_cdata_section: true,
        start_namespace_decl: true,
        end_namespace_decl: true,
        xml_decl: true,
        start_doctype_decl: true,
        end_doctype_decl: true,
        entity_decl: true,
        unparsed_entity_decl: true,
        notation_decl: true,
        element_decl: true,
        attlist_decl: true,
        skipped_entity: true,
        position: false,
        default_data: false,
        expand_internal_entities: true,
        event_text: false,
    };

    /// No event: the parser works as [`Parser::parse`] does.
    ///
    /// [`Parser::parse`]: crate::Parser::parse
    pub const NONE: Events = Events {
        start_element: false,
        end_element: false,
        character_data: false,
        processing_instruction: false,
        comment: false,
        start_cdata_section: false,
        end_cdata_section: false,
        start_namespace_decl: false,
        end_namespace_decl: false,
        xml_decl: false,
        start_doctype_decl: false,
        end_doctype_decl: false,
        entity_decl: false,
        unparsed_entity_decl: false,
        notation_decl: false,
        element_decl: false,
        attlist_decl: false,
        skipped_entity: false,
        position: false,
        default_data: false,
        expand_internal_entities: true,
        event_text: false,
    };

    /// Sets [`Events::start_element`].
    #[must_use]
    pub const fn start_element(mut self, on: bool) -> Self {
        self.start_element = on;
        self
    }

    /// Sets [`Events::end_element`].
    #[must_use]
    pub const fn end_element(mut self, on: bool) -> Self {
        self.end_element = on;
        self
    }

    /// Sets [`Events::character_data`].
    #[must_use]
    pub const fn character_data(mut self, on: bool) -> Self {
        self.character_data = on;
        self
    }

    /// Sets [`Events::processing_instruction`].
    #[must_use]
    pub const fn processing_instruction(mut self, on: bool) -> Self {
        self.processing_instruction = on;
        self
    }

    /// Sets [`Events::comment`].
    #[must_use]
    pub const fn comment(mut self, on: bool) -> Self {
        self.comment = on;
        self
    }

    /// Sets [`Events::start_cdata_section`].
    #[must_use]
    pub const fn start_cdata_section(mut self, on: bool) -> Self {
        self.start_cdata_section = on;
        self
    }

    /// Sets [`Events::end_cdata_section`].
    #[must_use]
    pub const fn end_cdata_section(mut self, on: bool) -> Self {
        self.end_cdata_section = on;
        self
    }

    /// Sets [`Events::start_namespace_decl`].
    #[must_use]
    pub const fn start_namespace_decl(mut self, on: bool) -> Self {
        self.start_namespace_decl = on;
        self
    }

    /// Sets [`Events::end_namespace_decl`].
    #[must_use]
    pub const fn end_namespace_decl(mut self, on: bool) -> Self {
        self.end_namespace_decl = on;
        self
    }

    /// Sets [`Events::xml_decl`].
    #[must_use]
    pub const fn xml_decl(mut self, on: bool) -> Self {
        self.xml_decl = on;
        self
    }

    /// Sets [`Events::start_doctype_decl`].
    #[must_use]
    pub const fn start_doctype_decl(mut self, on: bool) -> Self {
        self.start_doctype_decl = on;
        self
    }

    /// Sets [`Events::end_doctype_decl`].
    #[must_use]
    pub const fn end_doctype_decl(mut self, on: bool) -> Self {
        self.end_doctype_decl = on;
        self
    }

    /// Sets [`Events::entity_decl`].
    #[must_use]
    pub const fn entity_decl(mut self, on: bool) -> Self {
        self.entity_decl = on;
        self
    }

    /// Sets [`Events::unparsed_entity_decl`].
    #[must_use]
    pub const fn unparsed_entity_decl(mut self, on: bool) -> Self {
        self.unparsed_entity_decl = on;
        self
    }

    /// Sets [`Events::notation_decl`].
    #[must_use]
    pub const fn notation_decl(mut self, on: bool) -> Self {
        self.notation_decl = on;
        self
    }

    /// Sets [`Events::element_decl`].
    #[must_use]
    pub const fn element_decl(mut self, on: bool) -> Self {
        self.element_decl = on;
        self
    }

    /// Sets [`Events::attlist_decl`].
    #[must_use]
    pub const fn attlist_decl(mut self, on: bool) -> Self {
        self.attlist_decl = on;
        self
    }

    /// Sets [`Events::skipped_entity`].
    #[must_use]
    pub const fn skipped_entity(mut self, on: bool) -> Self {
        self.skipped_entity = on;
        self
    }

    /// Sets [`Events::position`].
    #[must_use]
    pub const fn position(mut self, on: bool) -> Self {
        self.position = on;
        self
    }

    /// Sets [`Events::default_data`].
    #[must_use]
    pub const fn default_data(mut self, on: bool) -> Self {
        self.default_data = on;
        self
    }

    /// Sets [`Events::expand_internal_entities`].
    #[must_use]
    pub const fn expand_internal_entities(mut self, on: bool) -> Self {
        self.expand_internal_entities = on;
        self
    }

    /// Sets [`Events::event_text`].
    #[must_use]
    pub const fn event_text(mut self, on: bool) -> Self {
        self.event_text = on;
        self
    }

    /// The parser core's handler flags: these events, the position and
    /// the default handler, every other handler unset.
    pub(crate) fn handler_set(self) -> parse::HandlerSet {
        parse::HandlerSet {
            start_element: self.start_element,
            end_element: self.end_element,
            character_data: self.character_data,
            processing_instruction: self.processing_instruction,
            comment: self.comment,
            start_cdata_section: self.start_cdata_section,
            end_cdata_section: self.end_cdata_section,
            start_namespace_decl: self.start_namespace_decl,
            end_namespace_decl: self.end_namespace_decl,
            xml_decl: self.xml_decl,
            start_doctype_decl: self.start_doctype_decl,
            end_doctype_decl: self.end_doctype_decl,
            entity_decl: self.entity_decl,
            unparsed_entity_decl: self.unparsed_entity_decl,
            notation_decl: self.notation_decl,
            element_decl: self.element_decl,
            attlist_decl: self.attlist_decl,
            skipped_entity: self.skipped_entity,
            event_position: self.position,
            default: self.default_data,
            no_default_expand: !self.expand_internal_entities,
            event_text: self.event_text,
            ..parse::HandlerSet::default()
        }
    }
}

/// Receives a document's events from [`Parser::parse_with`].
///
/// Every method has an empty default body; [`Handler::EVENTS`] says which
/// events are reported (all of them by default). Strings are the
/// document's text in UTF-8, whatever its encoding, as `&[u8]`: libexpat
/// does not guarantee that a character data piece ends on a character
/// boundary, and names and values are passed as libexpat builds them.
/// Character data may be split into several calls at points chosen by
/// the parser.
///
/// With namespace processing ([`Options::namespace_separator`]), element
/// and attribute names that have a namespace are expanded: the namespace
/// name, the separator byte and the local name (with separator `0`, no
/// separator byte), as `XML_ParserCreateNS` reports them; namespace
/// declarations are reported to [`Handler::start_namespace_decl`] and
/// [`Handler::end_namespace_decl`] and are not attributes. Without
/// namespace processing, names are as written and the namespace
/// declaration events never occur.
///
/// The handler cannot call the parser or stop the parse from a callback
/// (D-041); with [`Events::position`] it is told the position of each
/// event before the event (D-048).
///
/// ```
/// use expat::{Events, Handler, Parser};
///
/// #[derive(Default)]
/// struct Names(Vec<Vec<u8>>);
///
/// impl Handler for Names {
///     const EVENTS: Events = Events::NONE.start_element(true);
///     fn start_element(&mut self, name: &[u8], _attributes: &[(&[u8], &[u8])], _specified: usize) {
///         self.0.push(name.to_vec());
///     }
/// }
///
/// let mut names = Names::default();
/// let mut p = Parser::new([7; 16]);
/// assert_eq!(p.parse_with(&mut names, b"<a><b/><c x='1'/></a>", true), Ok(()));
/// assert_eq!(names.0, [b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
/// ```
///
/// [`Parser::parse_with`]: crate::Parser::parse_with
/// [`Options::namespace_separator`]: crate::Options::namespace_separator
pub trait Handler {
    /// The events the parser reports to this handler (C: the handlers
    /// that are set); [`Events::ALL`] by default. See [`Events`] for what
    /// an event that is off changes besides the call.
    const EVENTS: Events = Events::ALL;

    /// A start tag or empty-element tag (`XML_StartElementHandler`).
    /// `attributes` are (name, value) pairs in libexpat's order: the
    /// `specified` attributes given in the tag first, then the defaults
    /// from the document type declaration
    /// (`XML_GetSpecifiedAttributeCount` is `2 * specified`).
    fn start_element(&mut self, _name: &[u8], _attributes: &[(&[u8], &[u8])], _specified: usize) {}

    /// An end tag, or the end of an empty-element tag
    /// (`XML_EndElementHandler`).
    fn end_element(&mut self, _name: &[u8]) {}

    /// A piece of character data (`XML_CharacterDataHandler`).
    fn character_data(&mut self, _data: &[u8]) {}

    /// A processing instruction (`XML_ProcessingInstructionHandler`).
    fn processing_instruction(&mut self, _target: &[u8], _data: &[u8]) {}

    /// A comment's text (`XML_CommentHandler`).
    fn comment(&mut self, _data: &[u8]) {}

    /// The start of a CDATA section (`XML_StartCdataSectionHandler`).
    fn start_cdata_section(&mut self) {}

    /// The end of a CDATA section (`XML_EndCdataSectionHandler`).
    fn end_cdata_section(&mut self) {}

    /// A namespace declaration comes into scope, before the start element
    /// event of its element (`XML_StartNamespaceDeclHandler`). `prefix` is
    /// `None` for the default namespace; `uri` is `None` when the default
    /// namespace is undeclared (`xmlns=''`).
    fn start_namespace_decl(&mut self, _prefix: Option<&[u8]>, _uri: Option<&[u8]>) {}

    /// A namespace declaration goes out of scope, after the end element
    /// event of its element (`XML_EndNamespaceDeclHandler`).
    fn end_namespace_decl(&mut self, _prefix: Option<&[u8]>) {}

    /// An XML declaration or text declaration (`XML_XmlDeclHandler`).
    /// `version` is `None` in a text declaration without one, `encoding`
    /// is `None` when absent, and `standalone` is `None` when absent.
    fn xml_decl(
        &mut self,
        _version: Option<&[u8]>,
        _encoding: Option<&[u8]>,
        _standalone: Option<bool>,
    ) {
    }

    /// The start of a document type declaration, after its name and
    /// external identifier (`XML_StartDoctypeDeclHandler`).
    /// `has_internal_subset` is true when the declaration has an
    /// internal subset (`[...]`).
    fn start_doctype_decl(
        &mut self,
        _name: &[u8],
        _system_id: Option<&[u8]>,
        _public_id: Option<&[u8]>,
        _has_internal_subset: bool,
    ) {
    }

    /// The end of a document type declaration, after its internal subset
    /// (`XML_EndDoctypeDeclHandler`).
    fn end_doctype_decl(&mut self) {}

    /// An entity declaration (`XML_EntityDeclHandler`). `value` is the
    /// replacement text of an internal entity and `None` for an external
    /// one; `base` is the base set with [`Parser::set_base`]; `notation`
    /// is the notation of an unparsed entity, which goes here only when
    /// [`Events::unparsed_entity_decl`] is off.
    ///
    /// [`Parser::set_base`]: crate::Parser::set_base
    #[allow(clippy::too_many_arguments)] // reason: mirrors XML_EntityDeclHandler.
    fn entity_decl(
        &mut self,
        _name: &[u8],
        _is_parameter_entity: bool,
        _value: Option<&[u8]>,
        _base: Option<&[u8]>,
        _system_id: Option<&[u8]>,
        _public_id: Option<&[u8]>,
        _notation: Option<&[u8]>,
    ) {
    }

    /// An unparsed entity declaration, one with an `NDATA` notation
    /// (`XML_UnparsedEntityDeclHandler`).
    fn unparsed_entity_decl(
        &mut self,
        _name: &[u8],
        _base: Option<&[u8]>,
        _system_id: Option<&[u8]>,
        _public_id: Option<&[u8]>,
        _notation: &[u8],
    ) {
    }

    /// A notation declaration (`XML_NotationDeclHandler`).
    fn notation_decl(
        &mut self,
        _name: &[u8],
        _base: Option<&[u8]>,
        _system_id: Option<&[u8]>,
        _public_id: Option<&[u8]>,
    ) {
    }

    /// An element type declaration and its content model
    /// (`XML_ElementDeclHandler`).
    fn element_decl(&mut self, _name: &[u8], _model: &ContentModel) {}

    /// One attribute of an attribute-list declaration
    /// (`XML_AttlistDeclHandler`): a declaration with several attributes
    /// is reported once per attribute. `default` is `None` for
    /// `#IMPLIED` and `#REQUIRED`; `is_required` is true for `#REQUIRED`
    /// and for a `#FIXED` default.
    fn attlist_decl(
        &mut self,
        _element: &[u8],
        _attribute: &[u8],
        _attribute_type: &[u8],
        _default: Option<&[u8]>,
        _is_required: bool,
    ) {
    }

    /// A reference to an entity that is not expanded
    /// (`XML_SkippedEntityHandler`): an entity with no declaration read,
    /// where that is not an error (for example after a parameter entity
    /// reference that was not read), or an internal general entity when
    /// [`Events::expand_internal_entities`] is off.
    fn skipped_entity(&mut self, _name: &[u8], _is_parameter_entity: bool) {}

    /// The position of the event reported next, when [`Events::position`]
    /// is on: its line (from 1) and column (from 0), and its byte offset
    /// in the whole input, as `XML_GetCurrentLineNumber`,
    /// `XML_GetCurrentColumnNumber` and `XML_GetCurrentByteIndex` return
    /// them inside that event's C handler.
    ///
    /// ```
    /// use expat::{Events, Handler, Parser};
    ///
    /// #[derive(Default)]
    /// struct Lines(u64, Vec<u64>);
    ///
    /// impl Handler for Lines {
    ///     const EVENTS: Events = Events::NONE.start_element(true).position(true);
    ///     fn position(&mut self, line: u64, _column: u64, _byte_index: i64) {
    ///         self.0 = line;
    ///     }
    ///     fn start_element(&mut self, _name: &[u8], _attributes: &[(&[u8], &[u8])], _specified: usize) {
    ///         self.1.push(self.0);
    ///     }
    /// }
    ///
    /// let mut lines = Lines::default();
    /// let mut p = Parser::new([7; 16]);
    /// p.parse_with(&mut lines, b"<a>\n  <b/>\n<c/></a>", true)?;
    /// assert_eq!(lines.1, [1, 2, 3]);
    /// # Ok::<(), expat::Error>(())
    /// ```
    fn position(&mut self, _line: u64, _column: u64, _byte_index: i64) {}

    /// A piece of the document that no event that is on reports, as
    /// written but converted to UTF-8 (`XML_DefaultHandler`), when
    /// [`Events::default_data`] is on: markup, and character data,
    /// comments, processing instructions or tags whose own event is off.
    /// Line ends are not normalised. Inside the document type
    /// declaration it receives the parts that no declaration event that
    /// is on takes, as in libexpat.
    ///
    /// ```
    /// use expat::{Events, Handler, Parser};
    ///
    /// #[derive(Default)]
    /// struct Copy(Vec<u8>);
    ///
    /// impl Handler for Copy {
    ///     const EVENTS: Events = Events::NONE.default_data(true);
    ///     fn default_data(&mut self, data: &[u8]) {
    ///         self.0.extend_from_slice(data);
    ///     }
    /// }
    ///
    /// let doc = b"<?xml version='1.0'?>\r\n<a x='1'><!-- c -->t&amp;</a>";
    /// let mut copy = Copy::default();
    /// let mut p = Parser::new([7; 16]);
    /// p.parse_with(&mut copy, doc, true)?;
    /// assert_eq!(copy.0, doc);
    /// # Ok::<(), expat::Error>(())
    /// ```
    fn default_data(&mut self, _data: &[u8]) {}

    /// The text of the event reported next, when [`Events::event_text`]
    /// is on: the event as written, converted to UTF-8, in the pieces
    /// that [`Handler::default_data`] would receive for it
    /// (`XML_DefaultCurrent`). Inside an expanded internal entity it is
    /// the text of the entity's current event.
    ///
    /// ```
    /// use expat::{Events, Handler, Parser};
    ///
    /// #[derive(Default)]
    /// struct Tags {
    ///     current: Vec<u8>,
    ///     tags: Vec<Vec<u8>>,
    /// }
    ///
    /// impl Handler for Tags {
    ///     const EVENTS: Events = Events::NONE.start_element(true).event_text(true);
    ///     fn event_text(&mut self, pieces: &[&[u8]]) {
    ///         self.current = pieces.concat();
    ///     }
    ///     fn start_element(&mut self, _name: &[u8], _atts: &[(&[u8], &[u8])], _specified: usize) {
    ///         self.tags.push(self.current.clone());
    ///     }
    /// }
    ///
    /// let mut tags = Tags::default();
    /// let mut p = Parser::new([7; 16]);
    /// p.parse_with(&mut tags, b"<a x = '1'><b/></a>", true)?;
    /// assert_eq!(tags.tags, [&b"<a x = '1'>"[..], b"<b/>"]);
    /// # Ok::<(), expat::Error>(())
    /// ```
    fn event_text(&mut self, _pieces: &[&[u8]]) {}

    /// A new base set from inside a handler (`XML_SetBase` during a
    /// parse), asked for before the parser next uses the base: before
    /// each event, each token of the document type declaration, and at
    /// the end of each call. `Some(Some(base))` sets the base,
    /// `Some(None)` clears it, `None` (the default) keeps it. As with
    /// [`Parser::set_base`], the parser copies the base into its DTD's
    /// string pool; a copy refused by the memory limit leaves the base as
    /// it was.
    ///
    /// ```
    /// use expat::{Events, Handler, Parser};
    ///
    /// #[derive(Default)]
    /// struct Rebase(Option<Option<Vec<u8>>>, Vec<Option<Vec<u8>>>);
    ///
    /// impl Handler for Rebase {
    ///     const EVENTS: Events = Events::NONE.notation_decl(true);
    ///     fn notation_decl(&mut self, _name: &[u8], base: Option<&[u8]>, _system_id: Option<&[u8]>, _public_id: Option<&[u8]>) {
    ///         self.1.push(base.map(<[u8]>::to_vec));
    ///         self.0 = Some(Some(b"second/".to_vec()));
    ///     }
    ///     fn take_base(&mut self) -> Option<Option<Vec<u8>>> {
    ///         self.0.take()
    ///     }
    /// }
    ///
    /// let mut h = Rebase::default();
    /// let mut p = Parser::new([7; 16]);
    /// p.set_base(Some(b"first/")).map_err(|_| "no memory")?;
    /// p.parse_with(&mut h, b"<!DOCTYPE d [<!NOTATION n SYSTEM 'a'><!NOTATION m SYSTEM 'b'>]><d/>", true)?;
    /// assert_eq!(h.1, [Some(b"first/".to_vec()), Some(b"second/".to_vec())]);
    /// assert_eq!(p.base(), Some(&b"second/"[..]));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// [`Parser::set_base`]: crate::Parser::set_base
    fn take_base(&mut self) -> Option<Option<Vec<u8>>> {
        None
    }
}

/// Passes the parser core's events to a public [`Handler`]. Only the
/// events of [`Events`] are set ([`Events::handler_set`]), so the core
/// never calls the other methods, whose defaults do nothing (and refuse
/// the two that return a value).
pub(crate) struct Adapter<'a, H: Handler>(pub(crate) &'a mut H);

impl<H: Handler> parse::Handler for Adapter<'_, H> {
    fn take_base(&mut self) -> Option<Option<Vec<u8>>> {
        self.0.take_base()
    }
    fn event_position(&mut self, line: u64, column: u64, byte_index: i64) {
        self.0.position(line, column, byte_index);
    }
    fn start_element(&mut self, name: &[u8], atts: &[(&[u8], &[u8])], specified: i32) {
        // C's count is twice the number of attributes given in the tag.
        let specified = usize::try_from(specified).unwrap_or(0) / 2;
        self.0.start_element(name, atts, specified);
    }
    fn end_element(&mut self, name: &[u8]) {
        self.0.end_element(name);
    }
    fn character_data(&mut self, data: &[u8]) {
        self.0.character_data(data);
    }
    fn processing_instruction(&mut self, target: &[u8], data: &[u8]) {
        self.0.processing_instruction(target, data);
    }
    fn comment(&mut self, data: &[u8]) {
        self.0.comment(data);
    }
    fn start_cdata_section(&mut self) {
        self.0.start_cdata_section();
    }
    fn end_cdata_section(&mut self) {
        self.0.end_cdata_section();
    }
    fn start_namespace_decl(&mut self, prefix: Option<&[u8]>, uri: Option<&[u8]>) {
        self.0.start_namespace_decl(prefix, uri);
    }
    fn end_namespace_decl(&mut self, prefix: Option<&[u8]>) {
        self.0.end_namespace_decl(prefix);
    }
    fn default_data(&mut self, data: &[u8]) {
        self.0.default_data(data);
    }
    fn xml_decl(&mut self, version: Option<&[u8]>, encoding: Option<&[u8]>, standalone: i32) {
        let standalone = match standalone {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        };
        self.0.xml_decl(version, encoding, standalone);
    }
    fn start_doctype_decl(
        &mut self,
        name: &[u8],
        sysid: Option<&[u8]>,
        pubid: Option<&[u8]>,
        has_internal_subset: bool,
    ) {
        self.0
            .start_doctype_decl(name, sysid, pubid, has_internal_subset);
    }
    fn end_doctype_decl(&mut self) {
        self.0.end_doctype_decl();
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
        self.0
            .entity_decl(name, is_param, value, base, sysid, pubid, notation);
    }
    fn unparsed_entity_decl(
        &mut self,
        name: &[u8],
        base: Option<&[u8]>,
        sysid: Option<&[u8]>,
        pubid: Option<&[u8]>,
        notation: &[u8],
    ) {
        self.0
            .unparsed_entity_decl(name, base, sysid, pubid, notation);
    }
    fn notation_decl(
        &mut self,
        name: &[u8],
        base: Option<&[u8]>,
        sysid: Option<&[u8]>,
        pubid: Option<&[u8]>,
    ) {
        self.0.notation_decl(name, base, sysid, pubid);
    }
    fn element_decl(&mut self, name: &[u8], model: &ContentModel) {
        self.0.element_decl(name, model);
    }
    fn attlist_decl(
        &mut self,
        elname: &[u8],
        attname: &[u8],
        att_type: &[u8],
        dflt: Option<&[u8]>,
        is_required: bool,
    ) {
        self.0
            .attlist_decl(elname, attname, att_type, dflt, is_required);
    }
    fn skipped_entity(&mut self, name: &[u8], is_param: bool) {
        self.0.skipped_entity(name, is_param);
    }
    fn event_text(&mut self, pieces: &[&[u8]]) {
        self.0.event_text(pieces);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn all_and_none_map_to_the_document_handlers_only() {
        let all = Events::ALL.handler_set();
        let expected = parse::HandlerSet {
            start_element: true,
            end_element: true,
            character_data: true,
            processing_instruction: true,
            comment: true,
            start_cdata_section: true,
            end_cdata_section: true,
            start_namespace_decl: true,
            end_namespace_decl: true,
            xml_decl: true,
            start_doctype_decl: true,
            end_doctype_decl: true,
            entity_decl: true,
            unparsed_entity_decl: true,
            notation_decl: true,
            element_decl: true,
            attlist_decl: true,
            skipped_entity: true,
            ..parse::HandlerSet::default()
        };
        assert_eq!(all, expected);
        assert_eq!(Events::NONE.handler_set(), parse::HandlerSet::default());
    }

    #[derive(Default)]
    struct Split<const EXPAND: bool> {
        chars: Vec<u8>,
        other: Vec<u8>,
    }

    impl<const EXPAND: bool> Handler for Split<EXPAND> {
        const EVENTS: Events = Events::NONE
            .character_data(true)
            .default_data(true)
            .expand_internal_entities(EXPAND);
        fn character_data(&mut self, data: &[u8]) {
            self.chars.extend_from_slice(data);
        }
        fn default_data(&mut self, data: &[u8]) {
            self.other.extend_from_slice(data);
        }
    }

    const DOC: &[u8] = b"<!DOCTYPE a [<!ENTITY e 'x&#65;'>]><a>t&e;<b/></a>";

    #[test]
    fn default_data_takes_what_no_other_event_reports() {
        let mut h = Split::<true>::default();
        let mut p = crate::Parser::new([7; 16]);
        assert_eq!(p.parse_with(&mut h, DOC, true), Ok(()));
        assert_eq!(h.chars, b"txA");
        assert_eq!(h.other, b"<!DOCTYPE a [<!ENTITY e 'x&#65;'>]><a><b/></a>");
    }

    #[test]
    fn unexpanded_internal_entities_go_to_default_data() {
        let mut h = Split::<false>::default();
        let mut p = crate::Parser::new([7; 16]);
        assert_eq!(p.parse_with(&mut h, DOC, true), Ok(()));
        assert_eq!(h.chars, b"t");
        assert_eq!(
            h.other,
            b"<!DOCTYPE a [<!ENTITY e 'x&#65;'>]><a>&e;<b/></a>"
        );
    }

    #[test]
    fn default_data_is_converted_to_utf8() {
        let doc: Vec<u8> = "<a>\u{e9}</a>"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let mut h = Split::<true>::default();
        let mut p =
            crate::Parser::with_options([7; 16], crate::Options::new().encoding(b"UTF-16LE"));
        assert_eq!(p.parse_with(&mut h, &doc, true), Ok(()));
        assert_eq!(h.chars, "\u{e9}".as_bytes());
        assert_eq!(h.other, b"<a></a>");
    }

    /// Records the text of each start and end element event, and marks
    /// each start element as passed to the default handler.
    struct Texts {
        stopper: Option<crate::Stopper>,
        current: Vec<u8>,
        seen: Vec<Vec<u8>>,
    }

    impl Handler for Texts {
        const EVENTS: Events = Events::NONE
            .start_element(true)
            .end_element(true)
            .start_namespace_decl(true)
            .event_text(true);
        fn event_text(&mut self, pieces: &[&[u8]]) {
            self.current = pieces.concat();
        }
        fn start_namespace_decl(&mut self, _prefix: Option<&[u8]>, _uri: Option<&[u8]>) {
            self.seen.push(self.current.clone());
            if let Some(s) = &self.stopper {
                s.mark_default_current();
            }
        }
        fn start_element(&mut self, _name: &[u8], _atts: &[(&[u8], &[u8])], _specified: usize) {
            self.seen.push(self.current.clone());
        }
        fn end_element(&mut self, _name: &[u8]) {
            self.seen.push(self.current.clone());
        }
    }

    #[test]
    fn event_text_is_converted_and_moved_after_default_current() {
        let doc: Vec<u8> = "<a xmlns='u'>\u{e9}</a>"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let mut p = crate::Parser::with_options(
            [7; 16],
            crate::Options::new()
                .encoding(b"UTF-16LE")
                .namespace_separator(b'|'),
        );
        let mut h = Texts {
            stopper: None,
            current: Vec::new(),
            seen: Vec::new(),
        };
        assert_eq!(p.parse_with(&mut h, &doc, true), Ok(()));
        assert_eq!(h.seen, [&b"<a xmlns='u'>"[..], b"<a xmlns='u'>", b"</a>"]);

        // Marked in the namespace handler, the start tag counts as reported
        // for the start element handler of the same event.
        let mut p = crate::Parser::with_options(
            [7; 16],
            crate::Options::new()
                .encoding(b"UTF-16LE")
                .namespace_separator(b'|'),
        );
        let mut h = Texts {
            stopper: Some(p.stopper()),
            current: Vec::new(),
            seen: Vec::new(),
        };
        assert_eq!(p.parse_with(&mut h, &doc, true), Ok(()));
        assert_eq!(h.seen, [&b"<a xmlns='u'>"[..], b"", b"</a>"]);
    }

    #[test]
    fn each_builder_sets_its_own_flag() {
        type Builder = fn(Events, bool) -> Events;
        type Flag = fn(&parse::HandlerSet) -> bool;
        let builders: [(Builder, Flag); 18] = [
            (Events::start_element, |h| h.start_element),
            (Events::end_element, |h| h.end_element),
            (Events::character_data, |h| h.character_data),
            (Events::processing_instruction, |h| h.processing_instruction),
            (Events::comment, |h| h.comment),
            (Events::start_cdata_section, |h| h.start_cdata_section),
            (Events::end_cdata_section, |h| h.end_cdata_section),
            (Events::start_namespace_decl, |h| h.start_namespace_decl),
            (Events::end_namespace_decl, |h| h.end_namespace_decl),
            (Events::xml_decl, |h| h.xml_decl),
            (Events::start_doctype_decl, |h| h.start_doctype_decl),
            (Events::end_doctype_decl, |h| h.end_doctype_decl),
            (Events::entity_decl, |h| h.entity_decl),
            (Events::unparsed_entity_decl, |h| h.unparsed_entity_decl),
            (Events::notation_decl, |h| h.notation_decl),
            (Events::element_decl, |h| h.element_decl),
            (Events::attlist_decl, |h| h.attlist_decl),
            (Events::skipped_entity, |h| h.skipped_entity),
        ];
        for (i, (set, get)) in builders.iter().enumerate() {
            let one = set(Events::NONE, true).handler_set();
            assert!(get(&one), "builder {i}");
            for (j, (_, other)) in builders.iter().enumerate() {
                assert_eq!(other(&one), i == j, "builder {i}, flag {j}");
            }
            assert!(!get(&set(Events::ALL, false).handler_set()), "builder {i}");
            assert_eq!(set(set(Events::NONE, true), false), Events::NONE);
        }
    }
}
