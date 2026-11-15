//! The public parser API (module 12, first slice, D-038; error texts and
//! `Display`, D-039; encoding and namespace options, D-040; callbacks,
//! D-041).
//!
//! [`Parser`] wraps the ported parser core (`parse::Parser`) without
//! callbacks. [`Parser::new`] parses with the default encoding (detected
//! from the input, as `XML_ParserCreate(NULL)` does) and without namespace
//! processing; [`Parser::with_options`] takes an encoding name and a
//! namespace separator ([`Options`]), as `XML_ParserCreate(encoding)` and
//! `XML_ParserCreateNS(encoding, sep)` do. [`Parser::parse`] is
//! `XML_Parse`, and every call goes to the core, so the results after an
//! error or after the final call are C's (D-023). [`Parser::parse_with`]
//! is the same call with the [`Handler`] events of D-041.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::handler::{Adapter, Events, Handler};
use crate::parse::{self, Status, Stop, XmlError};

/// How a [`Parser`] is created: the encoding name and the namespace
/// separator given to `XML_ParserCreate` / `XML_ParserCreateNS`.
///
/// `Options::default()` (the same as [`Options::new`]) sets neither: the
/// encoding is detected from the input and namespace processing is off, as
/// with [`Parser::new`]. New options may be added in later versions, so the
/// type is built with its methods:
///
/// ```
/// let options = expat::Options::new()
///     .encoding(b"ISO-8859-1")
///     .namespace_separator(b'|');
/// let mut p = expat::Parser::with_options([7; 16], options);
/// assert_eq!(p.parse(b"<a>caf\xe9</a>", true), Ok(()));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Options {
    encoding: Option<Vec<u8>>,
    namespace_separator: Option<u8>,
}

impl Options {
    /// No encoding name and no namespace processing (`Options::default()`).
    pub fn new() -> Self {
        Self::default()
    }

    /// The encoding name (`encodingName`). libexpat knows `UTF-8`,
    /// `UTF-16`, `UTF-16BE`, `UTF-16LE`, `ISO-8859-1` and `US-ASCII`,
    /// compared ignoring the case of ASCII letters; the input is read in
    /// this encoding (a byte order mark is handled as libexpat handles it)
    /// and the encoding named in its XML declaration is ignored. Any other name, the empty name included, makes the first
    /// [`Parser::parse`] call fail with [`ErrorCode::UnknownEncoding`].
    ///
    /// As with the C string, the name ends at its first NUL byte: only
    /// the bytes before it are kept.
    #[must_use]
    pub fn encoding(mut self, name: &[u8]) -> Self {
        let name = name.split(|&b| b == 0).next().unwrap_or_default();
        self.encoding = Some(name.to_vec());
        self
    }

    /// Turns namespace processing on with `separator`
    /// (`XML_ParserCreateNS`): prefixes must be declared, and the reserved
    /// prefixes and names are checked. `0` is a valid separator, as in C:
    /// namespace processing is on and expanded names have no separator. A
    /// namespace name that contains the separator is a syntax error unless
    /// the separator is a character allowed in URIs (RFC 3986).
    #[must_use]
    pub fn namespace_separator(mut self, separator: u8) -> Self {
        self.namespace_separator = Some(separator);
        self
    }
}

/// An XML parser for one document.
///
/// Each parser parses a single document, fed in one or more pieces with
/// [`Parser::parse`].
///
/// ```
/// let mut p = expat::Parser::new([7; 16]);
/// assert_eq!(p.parse(b"<doc>", false), Ok(()));
/// assert_eq!(p.parse(b"</doc>", true), Ok(()));
///
/// let mut p = expat::Parser::new([7; 16]);
/// let e = p.parse(b"<a></b>", true).unwrap_err();
/// assert_eq!(e.code(), expat::ErrorCode::TagMismatch);
/// assert_eq!((e.line(), e.column(), e.byte_index()), (1, 5, 5));
/// ```
pub struct Parser {
    inner: parse::Parser,
}

/// The handler of a parser without callbacks.
struct NoCallbacks;

impl parse::Handler for NoCallbacks {}

impl Parser {
    /// A parser with the default encoding and without namespace processing
    /// (`XML_ParserCreate(NULL)` followed by `XML_SetHashSalt16Bytes`).
    ///
    /// The salt keys the parser's internal hash tables. This crate has no
    /// entropy source, so the caller provides it; it should be
    /// unpredictable to whoever supplies the document. The examples in this
    /// documentation use a constant salt for brevity; with untrusted input
    /// a predictable salt allows hash flooding (see the crate
    /// documentation, "Hash salt").
    pub fn new(salt: [u8; 16]) -> Self {
        Self::with_options(salt, Options::default())
    }

    /// A parser with the encoding and namespace processing of `options`
    /// (`XML_ParserCreate(encoding)`, or `XML_ParserCreateNS(encoding,
    /// separator)` when a separator is set, followed by
    /// `XML_SetHashSalt16Bytes`). `Parser::with_options(salt,
    /// Options::default())` is [`Parser::new`]`(salt)`.
    ///
    /// ```
    /// use expat::{ErrorCode, Options, Parser};
    ///
    /// // Namespace processing: an unbound prefix is an error.
    /// let mut p = Parser::with_options([7; 16], Options::new().namespace_separator(b'|'));
    /// let e = p.parse(b"<p:a/>", true).unwrap_err();
    /// assert_eq!(e.code(), ErrorCode::UnboundPrefix);
    ///
    /// // Without it, `p:a` is an ordinary name.
    /// let mut p = Parser::new([7; 16]);
    /// assert_eq!(p.parse(b"<p:a/>", true), Ok(()));
    /// ```
    pub fn with_options(salt: [u8; 16], options: Options) -> Self {
        Parser {
            inner: parse::Parser::new(
                options.encoding.as_deref(),
                options.namespace_separator,
                salt,
            ),
        }
    }

    /// Parses the next piece of the document (`XML_Parse`); `is_final`
    /// marks the last piece.
    ///
    /// `Ok(())` means the piece was accepted, and, with `is_final`, that the
    /// document is complete and well-formed. On an error, the returned
    /// [`Error`] holds the error code and the position at that moment.
    ///
    /// Every call is passed to the parser, as in C: after an error, a later
    /// call normally reports the same error, but a call whose input the
    /// reparse-deferral heuristic does not scan returns `Ok(())`; after the
    /// final piece, a call fails with [`ErrorCode::Finished`].
    pub fn parse(&mut self, data: &[u8], is_final: bool) -> Result<(), Error> {
        // Clears the handlers a previous `parse_with` call set, so this call
        // runs with no handler, as every call did before callbacks (D-041).
        self.inner.set_handlers(parse::HandlerSet::default());
        let status = self.inner.parse(&mut NoCallbacks, data, is_final);
        self.result(status)
    }

    /// Parses the next piece of the document like [`Parser::parse`], and
    /// reports its events to `handler` (`XML_Parse` with the handlers of
    /// [`Handler::EVENTS`] set).
    ///
    /// The handler is given to each call: the events of this piece go to
    /// this handler. Before parsing, the call sets the handlers of
    /// `H::EVENTS` and no other (C: the other handlers are `NULL`);
    /// [`Parser::parse`] is `parse_with` with [`Events::NONE`]. Changing
    /// the handler type, and so the events, between the calls of one
    /// document is allowed but not compared with C (D-041).
    ///
    /// The result is that of [`Parser::parse`]: the events reported before
    /// an error are not taken back.
    ///
    /// ```
    /// use expat::{Handler, Parser};
    ///
    /// #[derive(Default)]
    /// struct Text(Vec<u8>);
    ///
    /// impl Handler for Text {
    ///     fn character_data(&mut self, data: &[u8]) {
    ///         self.0.extend_from_slice(data);
    ///     }
    /// }
    ///
    /// let mut text = Text::default();
    /// let mut p = Parser::new([7; 16]);
    /// assert_eq!(p.parse_with(&mut text, b"<a>one <b>two</b>", false), Ok(()));
    /// assert_eq!(p.parse_with(&mut text, b" three</a>", true), Ok(()));
    /// assert_eq!(text.0, b"one two three");
    /// ```
    ///
    /// [`Events::NONE`]: crate::Events::NONE
    pub fn parse_with<H: Handler>(
        &mut self,
        handler: &mut H,
        data: &[u8],
        is_final: bool,
    ) -> Result<(), Error> {
        self.parse_with_events(handler, H::EVENTS, data, is_final)
    }

    /// [`Parser::parse_with`] with the events given for this call instead
    /// of [`Handler::EVENTS`], for a caller whose set of handlers is only
    /// known at run time (C sets each handler pointer separately).
    /// `parse_with(h, data, is_final)` is `parse_with_events(h, H::EVENTS,
    /// data, is_final)`.
    ///
    /// ```
    /// use expat::{Events, Handler, Parser};
    ///
    /// #[derive(Default)]
    /// struct Count(usize);
    ///
    /// impl Handler for Count {
    ///     fn start_element(&mut self, _name: &[u8], _attributes: &[(&[u8], &[u8])], _specified: usize) {
    ///         self.0 += 1;
    ///     }
    /// }
    ///
    /// let mut count = Count::default();
    /// let mut p = Parser::new([7; 16]);
    /// p.parse_with_events(&mut count, Events::NONE, b"<a>", false)?;
    /// p.parse_with_events(&mut count, Events::NONE.start_element(true), b"<b/></a>", true)?;
    /// assert_eq!(count.0, 1);
    /// # Ok::<(), expat::Error>(())
    /// ```
    pub fn parse_with_events<H: Handler>(
        &mut self,
        handler: &mut H,
        events: Events,
        data: &[u8],
        is_final: bool,
    ) -> Result<(), Error> {
        self.inner.set_handlers(events.handler_set());
        let status = self.inner.parse(&mut Adapter(handler), data, is_final);
        self.result(status)
    }

    /// Space for the next `len` bytes of input in the parser's own buffer
    /// (`XML_GetBuffer`), to be filled by the caller and parsed with
    /// [`Parser::parse_buffer`] or [`Parser::parse_buffer_with`]. This
    /// avoids the copy [`Parser::parse`] makes of its input.
    ///
    /// The space is valid until the next call on the parser. Its contents
    /// are unspecified: only the first bytes written and then passed to
    /// `parse_buffer` are read. As in C, the request fails with
    /// [`ErrorCode::NoMemory`] when the buffer would grow past `i32::MAX`
    /// bytes, and with [`ErrorCode::Finished`] after the final piece.
    ///
    /// ```
    /// let mut p = expat::Parser::new([7; 16]);
    /// let doc = b"<a>text</a>";
    /// p.buffer(doc.len())?[..doc.len()].copy_from_slice(doc);
    /// p.parse_buffer(doc.len(), true)?;
    /// # Ok::<(), expat::Error>(())
    /// ```
    pub fn buffer(&mut self, len: usize) -> Result<&mut [u8], Error> {
        let len = c_len(len);
        if !self.inner.reserve_buffer(len) {
            let code = self.error_code().unwrap_or(ErrorCode::UnexpectedState);
            return Err(self.error(code));
        }
        match self.inner.buffer_space(len) {
            Some(space) => Ok(space),
            // `reserve_buffer` made room for `len` bytes, so the space
            // exists; reaching this would be a parser defect.
            None => Err(Error {
                code: ErrorCode::UnexpectedState,
                line: 0,
                column: 0,
                byte_index: -1,
            }),
        }
    }

    /// Parses the first `len` bytes of the space [`Parser::buffer`]
    /// returned (`XML_ParseBuffer`), with no handler; `is_final` marks the
    /// last piece. The result is that of [`Parser::parse`]. Without an
    /// earlier successful `buffer` call the first call fails with
    /// [`ErrorCode::NoBuffer`], and a `len` larger than the space fails
    /// with [`ErrorCode::InvalidArgument`].
    pub fn parse_buffer(&mut self, len: usize, is_final: bool) -> Result<(), Error> {
        self.inner.set_handlers(parse::HandlerSet::default());
        let status = self
            .inner
            .parse_buffer(&mut NoCallbacks, c_len(len), is_final);
        self.result(status)
    }

    /// [`Parser::parse_buffer`] reporting the events to `handler`, as
    /// [`Parser::parse_with`] does for [`Parser::parse`].
    pub fn parse_buffer_with<H: Handler>(
        &mut self,
        handler: &mut H,
        len: usize,
        is_final: bool,
    ) -> Result<(), Error> {
        self.parse_buffer_with_events(handler, H::EVENTS, len, is_final)
    }

    /// [`Parser::parse_buffer_with`] with the events given for this call,
    /// as [`Parser::parse_with_events`] does for [`Parser::parse_with`].
    pub fn parse_buffer_with_events<H: Handler>(
        &mut self,
        handler: &mut H,
        events: Events,
        len: usize,
        is_final: bool,
    ) -> Result<(), Error> {
        self.inner.set_handlers(events.handler_set());
        let status = self
            .inner
            .parse_buffer(&mut Adapter(handler), c_len(len), is_final);
        self.result(status)
    }

    /// The code of the error the last call on the parser set
    /// (`XML_GetErrorCode`), or `None` when it set none. A call that
    /// succeeds clears it, as in C, except a successful [`Parser::buffer`],
    /// which leaves it as it was.
    ///
    /// ```
    /// let mut p = expat::Parser::new([7; 16]);
    /// assert!(p.parse(b"<a></b>", true).is_err());
    /// assert_eq!(p.error_code(), Some(expat::ErrorCode::TagMismatch));
    /// ```
    pub fn error_code(&self) -> Option<ErrorCode> {
        let Stop::Error(e) = self.inner.error_code();
        ErrorCode::from_internal(e)
    }

    /// A handle that stops this parser from inside a handler
    /// (`XML_StopParser`); see [`Stopper`].
    ///
    /// ```
    /// use expat::{Handler, Parser, Stopper};
    ///
    /// struct FirstElement(Stopper, Vec<u8>);
    ///
    /// impl Handler for FirstElement {
    ///     fn start_element(&mut self, name: &[u8], _attributes: &[(&[u8], &[u8])], _specified: usize) {
    ///         self.1 = name.to_vec();
    ///         let _ = self.0.stop(true);
    ///     }
    /// }
    ///
    /// let mut p = Parser::new([7; 16]);
    /// let mut h = FirstElement(p.stopper(), Vec::new());
    /// p.parse_with(&mut h, b"<a><b/></a>", true)?;
    /// assert!(p.is_suspended());
    /// assert_eq!(h.1, b"a");
    /// p.resume_with(&mut h)?;
    /// assert_eq!(h.1, b"b");
    /// p.resume_with(&mut h)?;
    /// assert!(!p.is_suspended());
    /// # Ok::<(), expat::Error>(())
    /// ```
    pub fn stopper(&self) -> Stopper {
        Stopper(Arc::clone(&self.inner.control))
    }

    /// Whether a handler suspended the parse with [`Stopper::stop`]`(true)`
    /// (C: the call returned `XML_STATUS_SUSPENDED`). A suspended parser
    /// takes no input: [`Parser::parse`] and [`Parser::buffer`] fail with
    /// [`ErrorCode::Suspended`] until [`Parser::resume`] continues it.
    pub fn is_suspended(&self) -> bool {
        self.inner.parsing_status() == parse::Parsing::Suspended
    }

    /// Continues a suspended parse with no handler (`XML_ResumeParser`),
    /// from where it stopped, up to the end of the input given so far. The
    /// result is that of [`Parser::parse`]; the parse may be suspended
    /// again. Fails with [`ErrorCode::NotSuspended`] when the parser is not
    /// suspended.
    pub fn resume(&mut self) -> Result<(), Error> {
        self.inner.set_handlers(parse::HandlerSet::default());
        let status = self.inner.resume_parser(&mut NoCallbacks);
        self.result(status)
    }

    /// [`Parser::resume`] reporting the events to `handler`, as
    /// [`Parser::parse_with`] does for [`Parser::parse`].
    pub fn resume_with<H: Handler>(&mut self, handler: &mut H) -> Result<(), Error> {
        self.resume_with_events(handler, H::EVENTS)
    }

    /// [`Parser::resume_with`] with the events given for this call, as
    /// [`Parser::parse_with_events`] does for [`Parser::parse_with`].
    pub fn resume_with_events<H: Handler>(
        &mut self,
        handler: &mut H,
        events: Events,
    ) -> Result<(), Error> {
        self.inner.set_handlers(events.handler_set());
        let status = self.inner.resume_parser(&mut Adapter(handler));
        self.result(status)
    }

    /// The line of the parser's current position, from 1
    /// (`XML_GetCurrentLineNumber64` between calls). After an error it is
    /// the line of [`Error::line`].
    ///
    /// ```
    /// let mut p = expat::Parser::new([7; 16]);
    /// p.parse(b"<a>\n<b/>", false)?;
    /// assert_eq!(p.current_line(), 2);
    /// # Ok::<(), expat::Error>(())
    /// ```
    pub fn current_line(&mut self) -> u64 {
        self.inner.current_line_number()
    }

    /// The column of the parser's current position, from 0
    /// (`XML_GetCurrentColumnNumber64` between calls).
    pub fn current_column(&mut self) -> u64 {
        self.inner.current_column_number()
    }

    /// The byte offset of the parser's current position in the whole
    /// input, or -1 before the first call (`XML_GetCurrentByteIndex64`
    /// between calls).
    pub fn current_byte_index(&self) -> i64 {
        self.inner.current_byte_index()
    }

    /// The public result of a call that returned `status`.
    fn result(&mut self, status: Status) -> Result<(), Error> {
        match status {
            Status::Ok => Ok(()),
            Status::Error => {
                let Stop::Error(e) = self.inner.error_code();
                // `XML_STATUS_ERROR` always sets an error code; `None` here
                // would be a parser defect, not a document error.
                let code = ErrorCode::from_internal(e).unwrap_or(ErrorCode::UnexpectedState);
                Err(self.error(code))
            }
            // A handler stopped the parse with `Stopper::stop(true)`; the
            // caller sees it with `is_suspended` (D-050).
            Status::Suspended => Ok(()),
        }
    }

    /// An [`Error`] with `code` at the parser's current position
    /// (`XML_GetCurrentLineNumber64`, `XML_GetCurrentColumnNumber64`,
    /// `XML_GetCurrentByteIndex64`).
    fn error(&mut self, code: ErrorCode) -> Error {
        Error {
            code,
            line: self.inner.current_line_number(),
            column: self.inner.current_column_number(),
            byte_index: self.inner.current_byte_index(),
        }
    }
}

/// Stops a [`Parser`] while it parses (`XML_StopParser`), from a handler
/// that holds it ([`Parser::stopper`]).
///
/// The parser checks for a stop after each event, as C does, so the events
/// already under way may still be reported. A stopped parse ends the call
/// that was running: with [`Stopper::stop`]`(true)` the call returns
/// `Ok(())` with [`Parser::is_suspended`] set, and [`Parser::resume`]
/// continues it; with `stop(false)` the call fails with
/// [`ErrorCode::Aborted`] and the parser takes no more input. A handle
/// stays tied to its parser and has no effect once the parser is dropped.
#[derive(Debug, Clone)]
pub struct Stopper(Arc<parse::Control>);

impl Stopper {
    /// Stops the parse, resumable or not (`XML_StopParser(parser,
    /// resumable)`). Refused as in C: before the first call
    /// ([`ErrorCode::NotStarted`]), after the final piece or a stop without
    /// resume ([`ErrorCode::Finished`]), a second resumable stop while
    /// suspended ([`ErrorCode::Suspended`]), and a resumable stop inside an
    /// external parameter entity ([`ErrorCode::SuspendPe`]). Unlike C, a
    /// refusal does not change [`Parser::error_code`]. A stop without
    /// resume while suspended ends the parse.
    pub fn stop(&self, resumable: bool) -> Result<(), ErrorCode> {
        self.0
            .stop(resumable)
            .map_err(|e| ErrorCode::from_internal(e).unwrap_or(ErrorCode::UnexpectedState))
    }

    /// Records that a handler passed its event, as
    /// [`Handler::event_text`] gave it, to its own default handler
    /// (`XML_DefaultCurrent`). As in libexpat, when the document's text
    /// is converted to UTF-8 the event then counts as reported: its
    /// position moves to its end, and its text reported again to a later
    /// handler of the same event is empty.
    ///
    /// [`Handler::event_text`]: crate::Handler::event_text
    pub fn mark_default_current(&self) {
        self.0.mark_default_current();
    }
}

/// A length as the C `int` the core takes: lengths above `i32::MAX` become
/// -1, which the core refuses as C refuses a negative length
/// (`XML_GetBuffer`: [`ErrorCode::NoMemory`]; `XML_ParseBuffer`:
/// [`ErrorCode::InvalidArgument`]).
fn c_len(len: usize) -> i32 {
    i32::try_from(len).unwrap_or(-1)
}

/// A parse error: its code and where the parser stood when it occurred.
///
/// `Display` writes the code's text and the line and column
/// (`mismatched tag at line 1, column 5`); with the `std` feature it is a
/// `std::error::Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Error {
    code: ErrorCode,
    line: u64,
    column: u64,
    byte_index: i64,
}

impl Error {
    /// The error code (`XML_GetErrorCode`).
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// The line, counted from 1 (`XML_GetCurrentLineNumber64`).
    pub fn line(&self) -> u64 {
        self.line
    }

    /// The column, counted from 0 (`XML_GetCurrentColumnNumber64`).
    pub fn column(&self) -> u64 {
        self.column
    }

    /// The byte offset in the document, or `-1` when the parser has no
    /// current event (`XML_GetCurrentByteIndex64`).
    pub fn byte_index(&self) -> i64 {
        self.byte_index
    }
}

/// Declares [`ErrorCode`] with one variant per libexpat error code except
/// `XML_ERROR_NONE`, and its conversions from and to the parser's
/// `XmlError`. Both conversions are `match`es without a wildcard, so an
/// error code added to `XmlError` without a variant here does not compile.
macro_rules! error_codes {
    ($($(#[$m:meta])* $name:ident,)*) => {
        /// A libexpat error code (`enum XML_Error` without
        /// `XML_ERROR_NONE`); [`ErrorCode::to_c`] gives its C value.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum ErrorCode {
            $($(#[$m])* $name,)*
        }

        impl ErrorCode {
            /// The code of `e`; `None` for `XML_ERROR_NONE`.
            fn from_internal(e: XmlError) -> Option<Self> {
                match e {
                    XmlError::None => None,
                    $(XmlError::$name => Some(ErrorCode::$name),)*
                }
            }

            /// The parser's code for `self`.
            fn to_internal(self) -> XmlError {
                match self {
                    $(ErrorCode::$name => XmlError::$name,)*
                }
            }

            /// Every code, in declaration order.
            const ALL: &[ErrorCode] = &[$(ErrorCode::$name,)*];
        }
    };
}

error_codes! {
    /// `XML_ERROR_NO_MEMORY`
    NoMemory,
    /// `XML_ERROR_SYNTAX`
    Syntax,
    /// `XML_ERROR_NO_ELEMENTS`
    NoElements,
    /// `XML_ERROR_INVALID_TOKEN`
    InvalidToken,
    /// `XML_ERROR_UNCLOSED_TOKEN`
    UnclosedToken,
    /// `XML_ERROR_PARTIAL_CHAR`
    PartialChar,
    /// `XML_ERROR_TAG_MISMATCH`
    TagMismatch,
    /// `XML_ERROR_DUPLICATE_ATTRIBUTE`
    DuplicateAttribute,
    /// `XML_ERROR_JUNK_AFTER_DOC_ELEMENT`
    JunkAfterDocElement,
    /// `XML_ERROR_PARAM_ENTITY_REF`
    ParamEntityRef,
    /// `XML_ERROR_UNDEFINED_ENTITY`
    UndefinedEntity,
    /// `XML_ERROR_RECURSIVE_ENTITY_REF`
    RecursiveEntityRef,
    /// `XML_ERROR_ASYNC_ENTITY`
    AsyncEntity,
    /// `XML_ERROR_BAD_CHAR_REF`
    BadCharRef,
    /// `XML_ERROR_BINARY_ENTITY_REF`
    BinaryEntityRef,
    /// `XML_ERROR_ATTRIBUTE_EXTERNAL_ENTITY_REF`
    AttributeExternalEntityRef,
    /// `XML_ERROR_MISPLACED_XML_PI`
    MisplacedXmlPi,
    /// `XML_ERROR_UNKNOWN_ENCODING`
    UnknownEncoding,
    /// `XML_ERROR_INCORRECT_ENCODING`
    IncorrectEncoding,
    /// `XML_ERROR_UNCLOSED_CDATA_SECTION`
    UnclosedCdataSection,
    /// `XML_ERROR_EXTERNAL_ENTITY_HANDLING`
    ExternalEntityHandling,
    /// `XML_ERROR_NOT_STANDALONE`
    NotStandalone,
    /// `XML_ERROR_UNEXPECTED_STATE`
    UnexpectedState,
    /// `XML_ERROR_ENTITY_DECLARED_IN_PE`
    EntityDeclaredInPe,
    /// `XML_ERROR_FEATURE_REQUIRES_XML_DTD`
    FeatureRequiresXmlDtd,
    /// `XML_ERROR_CANT_CHANGE_FEATURE_ONCE_PARSING`
    CantChangeFeatureOnceParsing,
    /// `XML_ERROR_UNBOUND_PREFIX`
    UnboundPrefix,
    /// `XML_ERROR_UNDECLARING_PREFIX`
    UndeclaringPrefix,
    /// `XML_ERROR_INCOMPLETE_PE`
    IncompletePe,
    /// `XML_ERROR_XML_DECL`
    XmlDecl,
    /// `XML_ERROR_TEXT_DECL`
    TextDecl,
    /// `XML_ERROR_PUBLICID`
    Publicid,
    /// `XML_ERROR_SUSPENDED`
    Suspended,
    /// `XML_ERROR_NOT_SUSPENDED`
    NotSuspended,
    /// `XML_ERROR_ABORTED`
    Aborted,
    /// `XML_ERROR_FINISHED`
    Finished,
    /// `XML_ERROR_SUSPEND_PE`
    SuspendPe,
    /// `XML_ERROR_RESERVED_PREFIX_XML`
    ReservedPrefixXml,
    /// `XML_ERROR_RESERVED_PREFIX_XMLNS`
    ReservedPrefixXmlns,
    /// `XML_ERROR_RESERVED_NAMESPACE_URI`
    ReservedNamespaceUri,
    /// `XML_ERROR_INVALID_ARGUMENT`
    InvalidArgument,
    /// `XML_ERROR_NO_BUFFER`
    NoBuffer,
    /// `XML_ERROR_AMPLIFICATION_LIMIT_BREACH`
    AmplificationLimitBreach,
    /// `XML_ERROR_NOT_STARTED`
    NotStarted,
}

impl ErrorCode {
    /// The C value of the code (`enum XML_Error`).
    pub fn to_c(self) -> u32 {
        self.to_internal().to_c()
    }

    /// The code whose C value is `value`; `None` for `XML_ERROR_NONE` (0)
    /// and for values libexpat 2.10.0 does not define.
    ///
    /// ```
    /// use expat::ErrorCode;
    ///
    /// assert_eq!(ErrorCode::from_c(ErrorCode::TagMismatch.to_c()), Some(ErrorCode::TagMismatch));
    /// assert_eq!(ErrorCode::from_c(0), None);
    /// ```
    pub fn from_c(value: u32) -> Option<Self> {
        ErrorCode::ALL.iter().copied().find(|c| c.to_c() == value)
    }

    /// The libexpat text of the code (`XML_ErrorString`), the same bytes as
    /// the vendored libexpat 2.10.0 returns for its C value. This is also
    /// what [`Display`](core::fmt::Display) writes.
    ///
    /// ```
    /// assert_eq!(expat::ErrorCode::TagMismatch.description(), "mismatched tag");
    /// ```
    pub fn description(self) -> &'static str {
        match self {
            ErrorCode::NoMemory => "out of memory",
            ErrorCode::Syntax => "syntax error",
            ErrorCode::NoElements => "no element found",
            ErrorCode::InvalidToken => "not well-formed (invalid token)",
            ErrorCode::UnclosedToken => "unclosed token",
            ErrorCode::PartialChar => "partial character",
            ErrorCode::TagMismatch => "mismatched tag",
            ErrorCode::DuplicateAttribute => "duplicate attribute",
            ErrorCode::JunkAfterDocElement => "junk after document element",
            ErrorCode::ParamEntityRef => "illegal parameter entity reference",
            ErrorCode::UndefinedEntity => "undefined entity",
            ErrorCode::RecursiveEntityRef => "recursive entity reference",
            ErrorCode::AsyncEntity => "asynchronous entity",
            ErrorCode::BadCharRef => "reference to invalid character number",
            ErrorCode::BinaryEntityRef => "reference to binary entity",
            ErrorCode::AttributeExternalEntityRef => "reference to external entity in attribute",
            ErrorCode::MisplacedXmlPi => "XML or text declaration not at start of entity",
            ErrorCode::UnknownEncoding => "unknown encoding",
            ErrorCode::IncorrectEncoding => "encoding specified in XML declaration is incorrect",
            ErrorCode::UnclosedCdataSection => "unclosed CDATA section",
            ErrorCode::ExternalEntityHandling => "error in processing external entity reference",
            ErrorCode::NotStandalone => "document is not standalone",
            ErrorCode::UnexpectedState => "unexpected parser state - please send a bug report",
            ErrorCode::EntityDeclaredInPe => "entity declared in parameter entity",
            ErrorCode::FeatureRequiresXmlDtd => {
                "requested feature requires XML_DTD support in Expat"
            }
            ErrorCode::CantChangeFeatureOnceParsing => {
                "cannot change setting once parsing has begun"
            }
            ErrorCode::UnboundPrefix => "unbound prefix",
            ErrorCode::UndeclaringPrefix => "must not undeclare prefix",
            ErrorCode::IncompletePe => "incomplete markup in parameter entity",
            ErrorCode::XmlDecl => "XML declaration not well-formed",
            ErrorCode::TextDecl => "text declaration not well-formed",
            ErrorCode::Publicid => "illegal character(s) in public id",
            ErrorCode::Suspended => "parser suspended",
            ErrorCode::NotSuspended => "parser not suspended",
            ErrorCode::Aborted => "parsing aborted",
            ErrorCode::Finished => "parsing finished",
            ErrorCode::SuspendPe => "cannot suspend in external parameter entity",
            ErrorCode::ReservedPrefixXml => {
                "reserved prefix (xml) must not be undeclared or bound to another namespace name"
            }
            ErrorCode::ReservedPrefixXmlns => {
                "reserved prefix (xmlns) must not be declared or undeclared"
            }
            ErrorCode::ReservedNamespaceUri => {
                "prefix must not be bound to one of the reserved namespace names"
            }
            ErrorCode::InvalidArgument => "invalid argument",
            ErrorCode::NoBuffer => "a successful prior call to function XML_GetBuffer is required",
            ErrorCode::AmplificationLimitBreach => {
                "limit on input amplification factor (from DTD and entities) breached"
            }
            ErrorCode::NotStarted => "parser not started",
        }
    }
}

impl fmt::Display for ErrorCode {
    /// Writes [`ErrorCode::description`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.description())
    }
}

impl fmt::Display for Error {
    /// Writes `<description> at line <line>, column <column>`, for example
    /// `mismatched tag at line 1, column 5`: the line counted from 1 and the
    /// column from 0, as [`Error::line`] and [`Error::column`] return them
    /// (`XML_GetCurrentLineNumber`, `XML_GetCurrentColumnNumber`). The byte
    /// index is not written: it is an offset in the input rather than a
    /// place a reader looks up, and it is `-1` when the parser has no current
    /// event; [`Error::byte_index`] returns it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at line {}, column {}",
            self.code, self.line, self.column
        )
    }
}

/// With the `std` feature: `Error` is a [`std::error::Error`] without a
/// source.
#[cfg(feature = "std")]
impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    /// The comparison of every code with the vendored `enum XML_Error`
    /// is `error_codes_match_the_c_header` in
    /// `tests/tests/public_api_diff.rs`, which runs over a list of the 44
    /// codes; this test checks that the list is complete. (The header is
    /// not part of the published crate, so the comparison cannot live
    /// here.)
    #[test]
    fn there_are_44_error_codes() {
        assert_eq!(ErrorCode::ALL.len(), 44);
    }

    /// Writes `data` into the space of `p.buffer`, or returns its error.
    fn fill(p: &mut Parser, data: &[u8]) -> Result<(), ErrorCode> {
        let space = p.buffer(data.len()).map_err(|e| e.code())?;
        space.copy_from_slice(data);
        Ok(())
    }

    #[test]
    fn buffer_errors_are_those_of_c() {
        let mut p = Parser::new([7; 16]);
        let code = |r: Result<(), Error>| r.map_err(|e| e.code());
        assert_eq!(code(p.parse_buffer(0, false)), Err(ErrorCode::NoBuffer));
        assert_eq!(
            p.buffer(usize::MAX).map(|b| b.len()).map_err(|e| e.code()),
            Err(ErrorCode::NoMemory)
        );
        assert_eq!(p.buffer(4).map(|b| b.len()), Ok(4));
        // A successful request leaves the last error code.
        assert_eq!(p.error_code(), Some(ErrorCode::NoMemory));
        assert_eq!(
            code(p.parse_buffer(1 << 20, false)),
            Err(ErrorCode::InvalidArgument)
        );
        assert_eq!(fill(&mut p, b"<a>"), Ok(()));
        assert_eq!(p.parse_buffer(3, false), Ok(()));
        assert_eq!(p.error_code(), None);
        assert_eq!(p.parse(b"</a>", true), Ok(()));
        assert_eq!(fill(&mut p, b"x"), Err(ErrorCode::Finished));
        assert_eq!(code(p.parse_buffer(0, true)), Err(ErrorCode::Finished));
    }

    #[test]
    fn buffer_pieces_give_the_events_of_one_parse() {
        #[derive(Default)]
        struct Text(Vec<u8>);
        impl Handler for Text {
            fn character_data(&mut self, data: &[u8]) {
                self.0.extend_from_slice(data);
            }
        }
        let doc: Vec<u8> = b"<r>"
            .iter()
            .chain(&[b'x'; 5000])
            .chain(b"</r>")
            .copied()
            .collect();
        let mut text = Text::default();
        let mut p = Parser::new([7; 16]);
        for (i, piece) in doc.chunks(61).enumerate() {
            let last = (i + 1) * 61 >= doc.len();
            assert_eq!(fill(&mut p, piece), Ok(()));
            assert_eq!(p.parse_buffer_with(&mut text, piece.len(), last), Ok(()));
        }
        assert_eq!(text.0, [b'x'; 5000]);
    }

    /// Suspends at every start tag and records the element names.
    struct Suspend(Stopper, Vec<Vec<u8>>);

    impl Handler for Suspend {
        fn start_element(
            &mut self,
            name: &[u8],
            _attributes: &[(&[u8], &[u8])],
            _specified: usize,
        ) {
            self.1.push(name.to_vec());
            assert_eq!(self.0.stop(true), Ok(()));
            assert_eq!(self.0.stop(true), Err(ErrorCode::Suspended));
        }
    }

    #[test]
    fn suspended_parse_resumes_where_it_stopped() {
        let mut p = Parser::new([7; 16]);
        let mut h = Suspend(p.stopper(), Vec::new());
        let code = |r: Result<(), Error>| r.map_err(|e| e.code());
        assert_eq!(h.0.stop(true), Err(ErrorCode::NotStarted));
        assert_eq!(code(p.resume()), Err(ErrorCode::NotSuspended));
        assert_eq!(p.parse_with(&mut h, b"<a><b/><c/>", false), Ok(()));
        assert!(p.is_suspended());
        assert_eq!(code(p.parse(b"</a>", true)), Err(ErrorCode::Suspended));
        assert_eq!(
            p.buffer(1).map(|_| ()).map_err(|e| e.code()),
            Err(ErrorCode::Suspended)
        );
        let mut resumes = 0;
        while p.is_suspended() {
            assert_eq!(p.resume_with(&mut h), Ok(()));
            resumes += 1;
        }
        assert_eq!(resumes, 3);
        assert_eq!(h.1, [b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
        assert_eq!(p.parse_with(&mut h, b"</a>", true), Ok(()));
        assert!(!p.is_suspended());
        assert_eq!(h.0.stop(false), Err(ErrorCode::Finished));
    }

    #[test]
    fn stop_without_resume_aborts() {
        struct Abort(Stopper, usize);
        impl Handler for Abort {
            fn start_element(
                &mut self,
                _name: &[u8],
                _attributes: &[(&[u8], &[u8])],
                _specified: usize,
            ) {
                self.1 += 1;
                assert_eq!(self.0.stop(false), Ok(()));
            }
        }
        let mut p = Parser::new([7; 16]);
        let mut h = Abort(p.stopper(), 0);
        let r = p.parse_with(&mut h, b"<a><b/></a>", true);
        assert_eq!(r.map_err(|e| e.code()), Err(ErrorCode::Aborted));
        assert_eq!(h.1, 1);
        assert!(!p.is_suspended());
        assert_eq!(h.0.stop(true), Err(ErrorCode::Finished));
        assert_eq!(
            p.resume().map_err(|e| e.code()),
            Err(ErrorCode::NotSuspended)
        );
    }

    #[test]
    fn options_keep_the_name_up_to_its_first_nul() {
        assert_eq!(Options::new(), Options::default());
        let o = Options::new()
            .encoding(b"UTF-8\0junk")
            .namespace_separator(0);
        assert_eq!(o.encoding.as_deref(), Some(&b"UTF-8"[..]));
        assert_eq!(o.namespace_separator, Some(0));
        assert_eq!(
            Options::new().encoding(b"\0x").encoding.as_deref(),
            Some(&b""[..])
        );
    }

    #[test]
    fn error_code_display_is_its_description() {
        for &c in ErrorCode::ALL {
            let text = c.description();
            assert!(!text.is_empty() && text.is_ascii(), "{c:?}");
            assert_eq!(format!("{c}"), text);
        }
    }

    #[test]
    fn error_display_writes_text_line_and_column() {
        let e = Error {
            code: ErrorCode::TagMismatch,
            line: 3,
            column: 0,
            byte_index: -1,
        };
        assert_eq!(format!("{e}"), "mismatched tag at line 3, column 0");
    }

    #[test]
    fn error_code_mapping_is_lossless() {
        assert_eq!(ErrorCode::from_internal(XmlError::None), None);
        for (i, &c) in ErrorCode::ALL.iter().enumerate() {
            let e = c.to_internal();
            assert_ne!(e, XmlError::None);
            assert_eq!(ErrorCode::from_internal(e), Some(c));
            assert_eq!(c.to_c(), e.to_c());
            assert!(
                ErrorCode::ALL.iter().take(i).all(|&d| d.to_c() != c.to_c()),
                "{c:?} is not unique"
            );
        }
    }
}
