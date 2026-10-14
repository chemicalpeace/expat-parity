//! The public parser API (module 12, first slice, D-038).
//!
//! [`Parser`] wraps the ported parser core (`parse::Parser`) without
//! callbacks: the document is parsed with the default encoding (detected
//! from the input, as `XML_ParserCreate(NULL)` does) and without namespace
//! processing. [`Parser::parse`] is `XML_Parse`, and every call goes to the
//! core, so the results after an error or after the final call are C's
//! (D-023).

use crate::parse::{self, Status, Stop, XmlError};

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
        Parser {
            inner: parse::Parser::new(None, None, salt),
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
}

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
