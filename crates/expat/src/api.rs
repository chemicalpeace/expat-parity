//! The public parser API (module 12, first slice, D-038; error texts and
//! `Display`, D-039; encoding and namespace options, D-040).
//!
//! [`Parser`] wraps the ported parser core (`parse::Parser`) without
//! callbacks. [`Parser::new`] parses with the default encoding (detected
//! from the input, as `XML_ParserCreate(NULL)` does) and without namespace
//! processing; [`Parser::with_options`] takes an encoding name and a
//! namespace separator ([`Options`]), as `XML_ParserCreate(encoding)` and
//! `XML_ParserCreateNS(encoding, sep)` do. [`Parser::parse`] is
//! `XML_Parse`, and every call goes to the core, so the results after an
//! error or after the final call are C's (D-023).

use alloc::vec::Vec;
use core::fmt;

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
    /// unpredictable to whoever supplies the document.
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
        match self.inner.parse(&mut NoCallbacks, data, is_final) {
            Status::Ok => Ok(()),
            Status::Error => {
                let Stop::Error(e) = self.inner.error_code();
                // `XML_STATUS_ERROR` always sets an error code; `None` here
                // would be a parser defect, not a document error.
                let code = ErrorCode::from_internal(e).unwrap_or(ErrorCode::UnexpectedState);
                Err(self.error(code))
            }
            // Only `XML_StopParser` suspends a parse, and this API cannot
            // call it.
            Status::Suspended => Err(self.error(ErrorCode::UnexpectedState)),
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
            #[cfg(test)]
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

    /// The libexpat text of the code (`XML_ErrorString`), the same bytes as
    /// the vendored libexpat 2.9.0 returns for its C value. This is also
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
    use alloc::string::String;
    use alloc::vec::Vec;

    /// The vendored libexpat header the port is verified against (D-001).
    const EXPAT_H: &str = include_str!("../../expat-oracle/vendor/expat/lib/expat.h");

    /// The names of `enum XML_Error` in declaration order (C value = index;
    /// the enum has no explicit initialisers).
    fn c_error_names() -> Vec<&'static str> {
        let start = EXPAT_H.find("enum XML_Error {").unwrap_or(0);
        let body = EXPAT_H.get(start..).unwrap_or("");
        let body = body.get(..body.find("};").unwrap_or(0)).unwrap_or("");
        body.lines()
            .filter_map(|l| {
                let l = l.trim();
                let name = l.strip_prefix("XML_ERROR_")?;
                assert!(!name.contains('='), "unexpected initialiser: {l}");
                Some(name.trim_end_matches(','))
            })
            .collect()
    }

    /// `NO_MEMORY` -> `NoMemory`.
    fn camel(c_name: &str) -> String {
        c_name
            .split('_')
            .map(|w| {
                let mut cs = w.chars();
                cs.next().map_or_else(String::new, |f| {
                    let mut s = String::from(f);
                    s.extend(cs.flat_map(char::to_lowercase));
                    s
                })
            })
            .collect()
    }

    #[test]
    fn error_codes_match_the_c_header() {
        let names = c_error_names();
        assert_eq!(names.len(), 45, "enum XML_Error has 45 codes in 2.9.0");
        assert_eq!(names.first(), Some(&"NONE"));
        assert_eq!(ErrorCode::ALL.len(), 44);
        for (value, name) in names.iter().enumerate().skip(1) {
            let found: Vec<_> = ErrorCode::ALL
                .iter()
                .filter(|c| c.to_c() as usize == value)
                .collect();
            assert_eq!(found.len(), 1, "XML_ERROR_{name} = {value}");
            let debug = found.first().map(|c| format!("{c:?}"));
            assert_eq!(debug, Some(camel(name)), "value {value}");
        }
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
