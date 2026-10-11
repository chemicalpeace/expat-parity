//! `enum XML_Error` (`expat.h`) and the parser's own scope boundary.

/// An error code of libexpat (`enum XML_Error`). `XmlError as u32` is the C
/// value; [`XmlError::None`] is `XML_ERROR_NONE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum XmlError {
    /// `XML_ERROR_NONE`
    None = 0,
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

impl XmlError {
    /// The C value of the code.
    pub fn to_c(self) -> u32 {
        self as u32
    }
}

/// Why the parser stopped: a libexpat error, or input that needs a part of
/// the parser that is not ported yet.
///
/// [`Stop::DtdNotPorted`] is the documented boundary of Phase 3 module 10
/// (`docs/STATE.md`): the processors of the document type declaration
/// (`doProlog`'s DTD roles, entities, the external subset) are module 11.
/// Until then the parser stops at the doctype name of a document entity
/// instead of guessing; the differential harness counts such documents
/// separately, so they are never compared as if they were supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stop {
    /// A libexpat error code.
    Error(XmlError),
    /// The document has a document type declaration (module 11).
    DtdNotPorted,
}

impl From<XmlError> for Stop {
    fn from(e: XmlError) -> Self {
        Stop::Error(e)
    }
}
