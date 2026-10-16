//! Public callbacks (module 12, slice 4, D-041): the [`Handler`] trait and
//! the [`Events`] it receives, used by [`Parser::parse_with`].
//!
//! [`Parser::parse_with`]: crate::Parser::parse_with

use crate::parse;

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
}

impl Events {
    /// Every event.
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

    /// The parser core's handler flags: these nine, every other handler
    /// unset.
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
/// The handler cannot query the parser's position or stop the parse from
/// a callback; see D-041.
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
}

/// Passes the parser core's events to a public [`Handler`]. Only the nine
/// events of [`Events`] are set ([`Events::handler_set`]), so the core
/// never calls the other methods, whose defaults do nothing (and refuse
/// the two that return a value).
pub(crate) struct Adapter<'a, H: Handler>(pub(crate) &'a mut H);

impl<H: Handler> parse::Handler for Adapter<'_, H> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_and_none_map_to_the_nine_handlers_only() {
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
            ..parse::HandlerSet::default()
        };
        assert_eq!(all, expected);
        assert_eq!(Events::NONE.handler_set(), parse::HandlerSet::default());
    }

    #[test]
    fn each_builder_sets_its_own_flag() {
        type Builder = fn(Events, bool) -> Events;
        type Flag = fn(&parse::HandlerSet) -> bool;
        let builders: [(Builder, Flag); 9] = [
            (Events::start_element, |h| h.start_element),
            (Events::end_element, |h| h.end_element),
            (Events::character_data, |h| h.character_data),
            (Events::processing_instruction, |h| h.processing_instruction),
            (Events::comment, |h| h.comment),
            (Events::start_cdata_section, |h| h.start_cdata_section),
            (Events::end_cdata_section, |h| h.end_cdata_section),
            (Events::start_namespace_decl, |h| h.start_namespace_decl),
            (Events::end_namespace_decl, |h| h.end_namespace_decl),
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
