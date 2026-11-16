//! A libexpat-compatible C interface (`expat.h`) over the `expat` crate
//! (module 13, slice 1, D-047).
//!
//! The library exports the functions below under their libexpat names, so
//! a C program written against the vendored `expat.h` (libexpat 2.10.0,
//! `XML_Char` = `char`, UTF-8 output) can link against it instead of
//! libexpat. It covers parser creation (`XML_ParserCreate`,
//! `XML_ParserCreateNS`, `XML_ParserCreate_MM`), reset and release, user
//! data, the handlers of [`expat::Events`], `XML_Parse`, `XML_GetBuffer`
//! and `XML_ParseBuffer` (D-049), `XML_StopParser` and `XML_ResumeParser`
//! (D-050), `XML_SetDefaultHandler` and `XML_SetDefaultHandlerExpand`
//! (D-051), `XML_DefaultCurrent` (D-052), the declaration handlers,
//! `XML_FreeContentModel`, `XML_SetBase` and `XML_GetBase` (D-053), the
//! parser settings (`XML_SetEncoding`, `XML_SetHashSalt`,
//! `XML_SetHashSalt16Bytes`, `XML_SetReturnNSTriplet`,
//! `XML_UseForeignDTD`, `XML_SetParamEntityParsing`, the amplification
//! limits, `XML_SetReparseDeferralEnabled` and the property functions),
//! `XML_GetParsingStatus`, the memory functions and `XML_GetFeatureList`
//! (D-054), external entities (`XML_SetExternalEntityRefHandler`,
//! `XML_SetExternalEntityRefHandlerArg`, `XML_ExternalEntityParserCreate`)
//! and `XML_SetNotStandaloneHandler` (D-055), `XML_GetCurrentByteCount`,
//! `XML_GetIdAttributeIndex` and `XML_GetInputContext` (D-056),
//! `XML_SetUnknownEncodingHandler` (D-057), the error code and its text,
//! the position between calls and inside handlers (D-048) and the
//! version. `scripts/build-dropin.sh` builds it as `libexpat.so.1`, and
//! libexpat's own test suite passes against it except for the tests of
//! libexpat internals (`scripts/runtests-check.sh`, D-060). The `release`
//! function of an `XML_Encoding` runs first in `XML_ParserFree` and
//! `XML_ParserReset`, as a handler, as in libexpat.
//!
//! Differences from libexpat:
//!
//! - Inside a handler, `XML_GetCurrentLineNumber`,
//!   `XML_GetCurrentColumnNumber` and `XML_GetCurrentByteIndex` (and their
//!   64-bit forms) return the position of the event, counted at every
//!   event while any handler is set (D-048). libexpat counts only when
//!   asked; the two differ only if a carriage return and the line feed
//!   after it fall on either side of an event boundary.
//! - The set of handlers is read when `XML_Parse` starts: a handler set or
//!   cleared from inside a handler takes effect from the next call. (A
//!   handler replaced by another non-`NULL` handler is used at once, as in
//!   libexpat.)
//! - `XML_GetSpecifiedAttributeCount` and `XML_GetIdAttributeIndex` are
//!   updated only while a start element handler is set, which is where
//!   libexpat documents their use. Before the first start tag, and after
//!   `XML_ParserReset`, `XML_GetIdAttributeIndex` returns -1; libexpat
//!   2.10.0 leaves the value uninitialised there (D-056).
//! - `XML_Parse`, `XML_ParseBuffer`, `XML_GetBuffer`, `XML_ResumeParser`,
//!   `XML_ParserReset` and `XML_ParserFree` called from inside a handler
//!   do nothing (`XML_STATUS_ERROR`, `XML_FALSE` or `NULL`, without an
//!   error code), as in libexpat 2.10.0.
//! - `XML_DefaultCurrent` works only if a default handler was set when
//!   `XML_Parse` started. Inside the default handler it calls, for an
//!   event of a document not in UTF-8 whose text takes more than one
//!   1024-byte piece, the position functions return the position of the
//!   event rather than of each piece.
//! - From inside a handler, `XML_SetBase` returns `XML_STATUS_OK` and the
//!   parser takes the base before it next uses it; a copy refused by the
//!   memory limit then leaves the old base, where libexpat returns
//!   `XML_STATUS_ERROR` to the handler. The billion laughs limits and
//!   `XML_SetReparseDeferralEnabled` set from inside a handler take
//!   effect from the next token (libexpat: at once); the allocation
//!   tracker limits take effect at the next allocation, as in libexpat.
//! - `XML_ParserCreate_MM` with a suite: the parser's data lives in the
//!   Rust allocator, and the suite receives the calls libexpat would make,
//!   in libexpat's order and with libexpat's sizes, on blocks this library
//!   keeps and frees with the parser (D-060). A failing call makes the
//!   parse fail where libexpat's does. After `XML_ParserReset` the parser
//!   keeps its bindings, as libexpat does, but not its other buffers and
//!   tables, so later calls can differ from libexpat's.
//! - An external entity parser created outside the external entity
//!   reference handler (libexpat allows this at any time) keeps its parent
//!   busy until it is freed: meanwhile the parent refuses parsing, reset
//!   and release as from inside a handler, and a second such parser
//!   cannot be created. Inside the handler, one external entity parser can
//!   be open at a time, and it lives at most until that handler returns:
//!   one still open then is closed, and its parsing calls fail with
//!   `XML_ERROR_FINISHED` until `XML_ParserFree`.
//! - After `XML_ParserReset`, the memory limit applies, as in libexpat,
//!   to what is counted after the reset, but the parser counts its
//!   buffers and tables again where libexpat reuses the ones it has.
//! - The hash salt comes from the standard library's `RandomState` keys,
//!   which are seeded from the operating system.
//!
//! A handler must return normally: unwinding or `longjmp` out of a handler
//! through this library is undefined behaviour.
//!
//! This crate is the only place outside the test oracle with `unsafe`
//! code; each block states why it is sound.

// reason: the function, type and constant names are those of expat.h.
#![allow(non_snake_case, non_camel_case_types)]

mod mem;
#[cfg(feature = "testing")]
mod testing;

use std::cell::{Cell, RefCell, UnsafeCell};
use std::ffi::{CStr, CString, c_char, c_int, c_long, c_ulong, c_void};
use std::hash::BuildHasher;
use std::sync::OnceLock;

use expat::{
    ContentModel, ConvertFn, EncodingInfo, ErrorCode, Events, ExternalEntities, ExternalParser,
    Handler, Options, ParamEntityParsing, Parser, ParsingStatus, ReleaseFn, Stopper,
};

/// `XML_Char` without `XML_UNICODE`.
pub type XML_Char = c_char;
/// `XML_Bool`.
pub type XML_Bool = u8;
/// `XML_Size` without `XML_LARGE_SIZE`.
pub type XML_Size = c_ulong;
/// `XML_Index` without `XML_LARGE_SIZE`.
pub type XML_Index = c_long;
/// `enum XML_Status`.
pub type XML_Status = c_int;
/// `enum XML_Error`.
pub type XML_Error = c_int;

/// `XML_STATUS_ERROR`.
pub const XML_STATUS_ERROR: XML_Status = 0;
/// `XML_STATUS_OK`.
pub const XML_STATUS_OK: XML_Status = 1;
/// `XML_STATUS_SUSPENDED`.
pub const XML_STATUS_SUSPENDED: XML_Status = 2;
/// `XML_ERROR_NONE`.
pub const XML_ERROR_NONE: XML_Error = 0;

/// `XML_StartElementHandler`.
pub type XML_StartElementHandler =
    Option<unsafe extern "C" fn(*mut c_void, *const XML_Char, *mut *const XML_Char)>;
/// `XML_EndElementHandler`.
pub type XML_EndElementHandler = Option<unsafe extern "C" fn(*mut c_void, *const XML_Char)>;
/// `XML_CharacterDataHandler`.
pub type XML_CharacterDataHandler =
    Option<unsafe extern "C" fn(*mut c_void, *const XML_Char, c_int)>;
/// `XML_ProcessingInstructionHandler`.
pub type XML_ProcessingInstructionHandler =
    Option<unsafe extern "C" fn(*mut c_void, *const XML_Char, *const XML_Char)>;
/// `XML_CommentHandler`.
pub type XML_CommentHandler = Option<unsafe extern "C" fn(*mut c_void, *const XML_Char)>;
/// `XML_StartCdataSectionHandler`.
pub type XML_StartCdataSectionHandler = Option<unsafe extern "C" fn(*mut c_void)>;
/// `XML_EndCdataSectionHandler`.
pub type XML_EndCdataSectionHandler = Option<unsafe extern "C" fn(*mut c_void)>;
/// `XML_StartNamespaceDeclHandler`.
pub type XML_StartNamespaceDeclHandler =
    Option<unsafe extern "C" fn(*mut c_void, *const XML_Char, *const XML_Char)>;
/// `XML_EndNamespaceDeclHandler`.
pub type XML_EndNamespaceDeclHandler = Option<unsafe extern "C" fn(*mut c_void, *const XML_Char)>;
/// `XML_DefaultHandler`.
pub type XML_DefaultHandler = Option<unsafe extern "C" fn(*mut c_void, *const XML_Char, c_int)>;
/// `XML_XmlDeclHandler`.
pub type XML_XmlDeclHandler =
    Option<unsafe extern "C" fn(*mut c_void, *const XML_Char, *const XML_Char, c_int)>;
/// `XML_StartDoctypeDeclHandler`.
pub type XML_StartDoctypeDeclHandler = Option<
    unsafe extern "C" fn(*mut c_void, *const XML_Char, *const XML_Char, *const XML_Char, c_int),
>;
/// `XML_EndDoctypeDeclHandler`.
pub type XML_EndDoctypeDeclHandler = Option<unsafe extern "C" fn(*mut c_void)>;
/// `XML_EntityDeclHandler`.
pub type XML_EntityDeclHandler = Option<
    unsafe extern "C" fn(
        *mut c_void,
        *const XML_Char,
        c_int,
        *const XML_Char,
        c_int,
        *const XML_Char,
        *const XML_Char,
        *const XML_Char,
        *const XML_Char,
    ),
>;
/// `XML_UnparsedEntityDeclHandler`.
pub type XML_UnparsedEntityDeclHandler = Option<
    unsafe extern "C" fn(
        *mut c_void,
        *const XML_Char,
        *const XML_Char,
        *const XML_Char,
        *const XML_Char,
        *const XML_Char,
    ),
>;
/// `XML_NotationDeclHandler`.
pub type XML_NotationDeclHandler = Option<
    unsafe extern "C" fn(
        *mut c_void,
        *const XML_Char,
        *const XML_Char,
        *const XML_Char,
        *const XML_Char,
    ),
>;
/// `XML_ElementDeclHandler`.
pub type XML_ElementDeclHandler =
    Option<unsafe extern "C" fn(*mut c_void, *const XML_Char, *mut XML_Content)>;
/// `XML_AttlistDeclHandler`.
pub type XML_AttlistDeclHandler = Option<
    unsafe extern "C" fn(
        *mut c_void,
        *const XML_Char,
        *const XML_Char,
        *const XML_Char,
        *const XML_Char,
        c_int,
    ),
>;
/// `XML_SkippedEntityHandler`.
pub type XML_SkippedEntityHandler =
    Option<unsafe extern "C" fn(*mut c_void, *const XML_Char, c_int)>;
/// `XML_ExternalEntityRefHandler`: parser, context, base, system id,
/// public id; returns `XML_STATUS_OK` or `XML_STATUS_ERROR`.
pub type XML_ExternalEntityRefHandler = Option<
    unsafe extern "C" fn(
        XML_Parser,
        *const XML_Char,
        *const XML_Char,
        *const XML_Char,
        *const XML_Char,
    ) -> c_int,
>;
/// `XML_NotStandaloneHandler`: returns 0 to stop the parse with
/// `XML_ERROR_NOT_STANDALONE`.
pub type XML_NotStandaloneHandler = Option<unsafe extern "C" fn(*mut c_void) -> c_int>;

/// `XML_Encoding`: what an unknown-encoding handler fills in. The parser
/// sets every `map` entry to -1 and the three pointers to `NULL` before
/// the call.
#[repr(C)]
#[derive(Debug)]
pub struct XML_Encoding {
    /// For each byte: -1 (malformed), -2 to -4 (first byte of a sequence
    /// of that many bytes, converted by `convert`) or its code point.
    pub map: [c_int; 256],
    /// The first argument of `convert` and `release`.
    pub data: *mut c_void,
    /// The code point of one multi-byte sequence, or -1.
    pub convert: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int>,
    /// Called once the parser no longer needs `data`.
    pub release: Option<unsafe extern "C" fn(*mut c_void)>,
}

/// `XML_UnknownEncodingHandler`: handler data, encoding name, the
/// `XML_Encoding` to fill in; returns `XML_STATUS_OK` to use it.
pub type XML_UnknownEncodingHandler =
    Option<unsafe extern "C" fn(*mut c_void, *const XML_Char, *mut XML_Encoding) -> c_int>;

/// The `data` of an `XML_Encoding`, moved into the converter and release
/// closures the parser keeps.
#[derive(Clone, Copy)]
struct EncodingData(*mut c_void);

// SAFETY: the parser is used from one thread at a time, as libexpat
// requires; the pointer is only passed back to the program's own
// functions, from the thread that is parsing or freeing the parser.
unsafe impl Send for EncodingData {}
// SAFETY: as above.
unsafe impl Sync for EncodingData {}

/// `XML_Content` (`struct XML_cp`): one node of an element declaration's
/// content model. `type` and `quant` hold the values of
/// `enum XML_Content_Type` and `enum XML_Content_Quant`.
#[repr(C)]
#[derive(Debug)]
pub struct XML_Content {
    /// `type`
    pub r#type: c_int,
    /// `quant`
    pub quant: c_int,
    /// `name` (`NULL` unless `type` is `XML_CTYPE_NAME`).
    pub name: *mut XML_Char,
    /// `numchildren`
    pub numchildren: std::ffi::c_uint,
    /// `children`
    pub children: *mut XML_Content,
}

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
}

/// `malloc_fcn` of `XML_Memory_Handling_Suite`.
pub type XML_MallocFcn = unsafe extern "C" fn(usize) -> *mut c_void;
/// `realloc_fcn` of `XML_Memory_Handling_Suite`.
pub type XML_ReallocFcn = unsafe extern "C" fn(*mut c_void, usize) -> *mut c_void;
/// `free_fcn` of `XML_Memory_Handling_Suite`.
pub type XML_FreeFcn = unsafe extern "C" fn(*mut c_void);

/// `XML_Memory_Handling_Suite`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct XML_Memory_Handling_Suite {
    /// `malloc_fcn`
    pub malloc_fcn: Option<XML_MallocFcn>,
    /// `realloc_fcn`
    pub realloc_fcn: Option<XML_ReallocFcn>,
    /// `free_fcn`
    pub free_fcn: Option<XML_FreeFcn>,
}

/// The allocation functions of one parser: those of
/// `XML_ParserCreate_MM`, or the C library's.
#[derive(Debug, Clone, Copy)]
struct Mem {
    malloc: XML_MallocFcn,
    realloc: XML_ReallocFcn,
    free: XML_FreeFcn,
}

impl Default for Mem {
    fn default() -> Self {
        Mem {
            malloc,
            realloc,
            free,
        }
    }
}

/// The C form of `model` in one block from `alloc`, as libexpat's
/// `build_model` lays it out: the nodes in the crate's order (node 0 the
/// root, each node's children contiguous), then their names. `NULL` when
/// the allocation fails.
fn build_model(model: &ContentModel, alloc: impl FnOnce(usize) -> *mut c_void) -> *mut XML_Content {
    let n = model.nodes.len();
    let names: usize = model
        .nodes
        .iter()
        .filter_map(|x| x.name.as_ref())
        .map(|b| b.len() + 1)
        .sum();
    let Some(size) = n
        .checked_mul(std::mem::size_of::<XML_Content>())
        .and_then(|a| a.checked_add(names))
    else {
        return std::ptr::null_mut();
    };
    let block = alloc(size).cast::<XML_Content>();
    if block.is_null() {
        return block;
    }
    // SAFETY: `block` holds `n` nodes followed by `names` bytes; the
    // writes below stay inside it: one node per index below `n`, and each
    // name with its NUL in the space counted for it. `malloc` aligns the
    // block for `XML_Content`, and the names follow the nodes.
    unsafe {
        let mut text = block.add(n).cast::<u8>();
        for (i, src) in model.nodes.iter().enumerate() {
            let name = match &src.name {
                Some(b) => {
                    std::ptr::copy_nonoverlapping(b.as_ptr(), text, b.len());
                    text.add(b.len()).write(0);
                    let p = text.cast::<XML_Char>();
                    text = text.add(b.len() + 1);
                    p
                }
                None => std::ptr::null_mut(),
            };
            let children = if !src.children.is_empty() && src.children.end <= n {
                block.add(src.children.start)
            } else {
                std::ptr::null_mut()
            };
            block.add(i).write(XML_Content {
                r#type: src.kind as c_int,
                quant: src.quant as c_int,
                name,
                numchildren: std::ffi::c_uint::try_from(src.children.len()).unwrap_or(0),
                children,
            });
        }
    }
    block
}

/// `XML_Expat_Version`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XML_Expat_Version {
    /// `major`
    pub major: c_int,
    /// `minor`
    pub minor: c_int,
    /// `micro`
    pub micro: c_int,
}

/// The parser behind an `XML_Parser` handle.
///
/// `user_data` is the first field because `expat.h` defines
/// `XML_GetUserData(parser)` as `(*(void **)(parser))`. Everything a
/// handler may reach through the handle while `XML_Parse` runs is a
/// `Cell`; the Rust parser and the scratch buffers are in `inner`, which
/// only `XML_Parse` and the position functions borrow, and only while
/// `busy` is false.
#[repr(C)]
pub struct XML_ParserStruct {
    user_data: Cell<*mut c_void>,
    handler_arg: Cell<*mut c_void>,
    busy: Cell<bool>,
    /// An external entity parser created outside its parent's handler: its
    /// parent stays `busy` until it is freed.
    lent: Cell<bool>,
    error_code: Cell<XML_Error>,
    specified: Cell<c_int>,
    /// The position of the event being reported (line from 1, column,
    /// byte index), for the position functions inside a handler.
    event_position: Cell<(u64, u64, i64)>,
    /// The input buffer of the event being reported (start, length,
    /// offset of the event or `None` for `NULL`) and its byte count, for
    /// `XML_GetInputContext` and `XML_GetCurrentByteCount` inside a
    /// handler. The buffer is not written while a handler runs.
    event_input: Cell<(*const u8, usize, Option<usize>, u64)>,
    /// `XML_GetIdAttributeIndex`: as libexpat returns it, twice the index
    /// of the attribute; -1 for none.
    id_index: Cell<c_int>,
    start_element: Cell<XML_StartElementHandler>,
    end_element: Cell<XML_EndElementHandler>,
    character_data: Cell<XML_CharacterDataHandler>,
    processing_instruction: Cell<XML_ProcessingInstructionHandler>,
    comment: Cell<XML_CommentHandler>,
    start_cdata_section: Cell<XML_StartCdataSectionHandler>,
    end_cdata_section: Cell<XML_EndCdataSectionHandler>,
    start_namespace_decl: Cell<XML_StartNamespaceDeclHandler>,
    end_namespace_decl: Cell<XML_EndNamespaceDeclHandler>,
    default: Cell<XML_DefaultHandler>,
    xml_decl: Cell<XML_XmlDeclHandler>,
    start_doctype_decl: Cell<XML_StartDoctypeDeclHandler>,
    end_doctype_decl: Cell<XML_EndDoctypeDeclHandler>,
    entity_decl: Cell<XML_EntityDeclHandler>,
    unparsed_entity_decl: Cell<XML_UnparsedEntityDeclHandler>,
    notation_decl: Cell<XML_NotationDeclHandler>,
    element_decl: Cell<XML_ElementDeclHandler>,
    attlist_decl: Cell<XML_AttlistDeclHandler>,
    skipped_entity: Cell<XML_SkippedEntityHandler>,
    external_entity_ref: Cell<XML_ExternalEntityRefHandler>,
    /// `m_externalEntityRefHandlerArg`; `NULL` for the parser itself.
    external_entity_ref_arg: Cell<XML_Parser>,
    not_standalone: Cell<XML_NotStandaloneHandler>,
    /// `m_unknownEncodingHandler` and `m_unknownEncodingHandlerData`.
    unknown_encoding: Cell<(XML_UnknownEncodingHandler, *mut c_void)>,
    /// While the external entity reference handler runs: what creates the
    /// entity's parser (`XML_ExternalEntityParserCreate`); `NULL` otherwise.
    entities: Cell<*mut ExternalEntities<'static>>,
    /// The external entity parser created during the current external
    /// entity reference handler call and not yet freed.
    open_child: Cell<XML_Parser>,
    /// For an external entity parser: the parser it was created from,
    /// while that parser's handler call lasts; `NULL` otherwise.
    parent: Cell<XML_Parser>,
    /// The allocation functions for the memory handed to the program
    /// (content models, `XML_MemMalloc`).
    /// The tracker-driven memory suite of `XML_ParserCreate_MM`, shared
    /// with external entity parsers (D-060).
    suite: std::cell::RefCell<Option<std::sync::Arc<mem::CSuite>>>,
    mem: Cell<Mem>,
    /// A base set by `XML_SetBase` from inside a handler, for the parser
    /// to take ([`Handler::take_base`]).
    pending_base: RefCell<Option<Option<Vec<u8>>>>,
    /// `m_hash_secret_salt_set`.
    salt_set: Cell<bool>,
    /// The billion laughs maximum amplification, as set.
    bl_maximum: Cell<f32>,
    /// The billion laughs activation threshold, as set.
    bl_threshold: Cell<u64>,
    /// The allocation tracker maximum amplification, as set.
    at_maximum: Cell<f32>,
    /// The allocation tracker activation threshold, as set.
    at_threshold: Cell<u64>,
    /// `m_reparseDeferralEnabled`.
    reparse_deferral: Cell<bool>,
    /// A NUL-terminated copy of the base, for `XML_GetBase`.
    base: RefCell<Option<CString>>,
    /// `m_defaultExpandInternalEntities`: cleared by
    /// `XML_SetDefaultHandler`, set by `XML_SetDefaultHandlerExpand`.
    default_expand: Cell<bool>,
    /// The text of the event being reported, as the default handler
    /// would receive it (each piece's end in the text), for
    /// `XML_DefaultCurrent`. Kept only while a default handler is set.
    current: RefCell<(Vec<u8>, Vec<usize>)>,
    /// Stops the parser from inside a handler (`XML_StopParser`).
    stopper: Stopper,
    inner: UnsafeCell<Inner>,
}

/// `XML_Parser`.
pub type XML_Parser = *mut XML_ParserStruct;

/// The Rust parser of a handle.
enum Core {
    /// A parser made by `XML_ParserCreate` and the like.
    Own(Parser),
    /// An external entity parser. Its lifetime is that of the external
    /// entity reference handler call that created it (see
    /// `XML_ExternalEntityParserCreate`); it is dropped by
    /// `XML_ParserFree` or when that call returns.
    Child(ExternalParser<'static>),
    /// An external entity parser whose creating handler call has
    /// returned: every parsing call fails (`XML_ERROR_FINISHED`).
    Closed(Parser),
}

impl Core {
    fn get(&mut self) -> &mut Parser {
        match self {
            Core::Own(p) | Core::Closed(p) => p,
            Core::Child(c) => c,
        }
    }
}

/// The state only the parsing, buffer and position functions use.
struct Inner {
    parser: Core,
    /// NUL-terminated copies of the strings passed to a handler.
    text: Vec<u8>,
    /// The `atts` array of a start element handler.
    atts: Vec<*const XML_Char>,
}

impl XML_ParserStruct {
    /// The state of a root parser.
    // reason: `parser` is changed only with the `testing` feature.
    #[cfg_attr(not(feature = "testing"), allow(unused_mut))]
    fn own(mut parser: Parser) -> Option<Self> {
        #[cfg(feature = "testing")]
        parser.set_reparse_deferral_enabled(testing::reparse_deferral_default());
        Self::with_core(Core::Own(parser))
    }

    /// `None` when the system allocator cannot copy the base (D-065).
    fn with_core(mut core: Core) -> Option<Self> {
        let parser = core.get();
        let base = match parser.base() {
            Some(b) => Some(try_c_string(b)?),
            None => None,
        };
        Some(XML_ParserStruct {
            user_data: Cell::new(std::ptr::null_mut()),
            handler_arg: Cell::new(std::ptr::null_mut()),
            busy: Cell::new(false),
            lent: Cell::new(false),
            error_code: Cell::new(XML_ERROR_NONE),
            specified: Cell::new(0),
            event_position: Cell::new((0, 0, -1)),
            event_input: Cell::new((std::ptr::null(), 0, None, 0)),
            id_index: Cell::new(-1),
            start_element: Cell::new(None),
            end_element: Cell::new(None),
            character_data: Cell::new(None),
            processing_instruction: Cell::new(None),
            comment: Cell::new(None),
            start_cdata_section: Cell::new(None),
            end_cdata_section: Cell::new(None),
            start_namespace_decl: Cell::new(None),
            end_namespace_decl: Cell::new(None),
            default: Cell::new(None),
            xml_decl: Cell::new(None),
            start_doctype_decl: Cell::new(None),
            end_doctype_decl: Cell::new(None),
            entity_decl: Cell::new(None),
            unparsed_entity_decl: Cell::new(None),
            notation_decl: Cell::new(None),
            element_decl: Cell::new(None),
            attlist_decl: Cell::new(None),
            skipped_entity: Cell::new(None),
            external_entity_ref: Cell::new(None),
            external_entity_ref_arg: Cell::new(std::ptr::null_mut()),
            not_standalone: Cell::new(None),
            unknown_encoding: Cell::new((None, std::ptr::null_mut())),
            entities: Cell::new(std::ptr::null_mut()),
            open_child: Cell::new(std::ptr::null_mut()),
            parent: Cell::new(std::ptr::null_mut()),
            suite: std::cell::RefCell::new(None),
            mem: Cell::new(Mem::default()),
            pending_base: RefCell::new(None),
            salt_set: Cell::new(false),
            bl_maximum: Cell::new(parser.billion_laughs_maximum_amplification()),
            bl_threshold: Cell::new(parser.billion_laughs_activation_threshold()),
            at_maximum: Cell::new(parser.alloc_tracker_maximum_amplification()),
            at_threshold: Cell::new(parser.alloc_tracker_activation_threshold()),
            reparse_deferral: Cell::new(parser.reparse_deferral_enabled()),
            base: RefCell::new(base),
            default_expand: Cell::new(true),
            current: RefCell::new((Vec::new(), Vec::new())),
            stopper: parser.stopper(),
            inner: UnsafeCell::new(Inner {
                parser: core,
                text: Vec::new(),
                atts: Vec::new(),
            }),
        })
    }

    /// The events whose handler is set (non-`NULL`), and the position of
    /// each event when any is set.
    fn events(&self) -> Events {
        let events = Events::NONE
            .start_element(self.start_element.get().is_some())
            .end_element(self.end_element.get().is_some())
            .character_data(self.character_data.get().is_some())
            .processing_instruction(self.processing_instruction.get().is_some())
            .comment(self.comment.get().is_some())
            .start_cdata_section(self.start_cdata_section.get().is_some())
            .end_cdata_section(self.end_cdata_section.get().is_some())
            .start_namespace_decl(self.start_namespace_decl.get().is_some())
            .end_namespace_decl(self.end_namespace_decl.get().is_some())
            .xml_decl(self.xml_decl.get().is_some())
            .start_doctype_decl(self.start_doctype_decl.get().is_some())
            .end_doctype_decl(self.end_doctype_decl.get().is_some())
            .entity_decl(self.entity_decl.get().is_some())
            .unparsed_entity_decl(self.unparsed_entity_decl.get().is_some())
            .notation_decl(self.notation_decl.get().is_some())
            .element_decl(self.element_decl.get().is_some())
            .attlist_decl(self.attlist_decl.get().is_some())
            .skipped_entity(self.skipped_entity.get().is_some())
            .default_data(self.default.get().is_some())
            .expand_internal_entities(self.default_expand.get())
            .event_text(self.default.get().is_some())
            .external_entity_ref(self.external_entity_ref.get().is_some())
            .not_standalone(self.not_standalone.get().is_some());
        let id = events.start_element;
        events
            .position(events != Events::NONE)
            .input_context(events != Events::NONE)
            .id_attribute(id)
    }

    /// Runs `f` on the Rust parser between calls, or returns the field of
    /// the event position `g` picks while a handler is running.
    fn position<T>(
        &self,
        f: impl FnOnce(&mut Parser) -> T,
        g: impl FnOnce((u64, u64, i64)) -> T,
    ) -> T {
        match self.with_parser(f) {
            Some(v) => v,
            None => g(self.event_position.get()),
        }
    }

    /// Runs `f` on the Rust parser, or returns `None` while a handler is
    /// running (`XML_Parse` holds the parser then).
    fn with_parser<T>(&self, f: impl FnOnce(&mut Parser) -> T) -> Option<T> {
        if self.busy.get() {
            return None;
        }
        // SAFETY: `inner` is only borrowed by `XML_Parse`, which sets `busy`
        // for the whole borrow, and by this function, which returns before
        // anything else can run on this thread; the handle is not shared
        // between threads (libexpat parsers are not thread-safe either).
        let inner = unsafe { &mut *self.inner.get() };
        match &mut inner.parser {
            Core::Closed(_) => None,
            core => Some(f(core.get())),
        }
    }

    /// `callUnknownEncodingRelease` at the start of `XML_ParserFree` and
    /// `XML_ParserReset`: the release function of an unknown encoding runs
    /// as a handler would (the parser cannot be freed, reset or fed from
    /// it), with the position queries answering the parser's current
    /// position.
    fn run_encoding_release(&self) {
        let Some((release, pos)) = self
            .with_parser(|p| {
                let pos = (p.current_line(), p.current_column(), p.current_byte_index());
                p.take_encoding_release().map(|r| (r, pos))
            })
            .flatten()
        else {
            return;
        };
        let saved = self.event_position.replace(pos);
        self.busy.set(true);
        release();
        self.busy.set(false);
        self.event_position.set(saved);
    }

    /// Drops the Rust parser of an external entity parser whose creating
    /// handler call returns: the state it shares with its parent goes
    /// back to the parent, and later parsing calls fail.
    fn close(&self) {
        self.parent.set(std::ptr::null_mut());
        if self.busy.get() {
            return;
        }
        // SAFETY: as in `with_parser`: no parsing call is running on this
        // handle, so nothing else borrows `inner`.
        let inner = unsafe { &mut *self.inner.get() };
        if matches!(inner.parser, Core::Child(_)) {
            inner.parser = Core::Closed(Parser::new([0; 16]));
        }
    }

    /// Whether this is an external entity parser whose creating handler
    /// call has returned.
    fn closed(&self) -> bool {
        if self.busy.get() {
            return false;
        }
        // SAFETY: as in `with_parser`.
        let inner = unsafe { &*self.inner.get() };
        matches!(inner.parser, Core::Closed(_))
    }

    /// `m_parentParser != NULL`: an external entity parser, open or closed.
    fn is_child(&self) -> bool {
        !self.parent.get().is_null() || self.closed()
    }

    /// `parserBusy`: a parse has started and has not finished.
    fn parser_busy(&self) -> bool {
        matches!(
            self.stopper.parsing_status(),
            ParsingStatus::Parsing | ParsingStatus::Suspended
        )
    }

    /// `setBillionLaughsAttackProtectionMaximumAmplification`.
    fn set_bl_maximum(&self, factor: f32) -> XML_Prop_Error {
        if self.is_child() {
            return XML_PROP_ERROR_PARSER_NOT_ROOT;
        }
        if factor.is_nan() || factor < 1.0 {
            return XML_PROP_ERROR_INVALID_VALUE;
        }
        self.bl_maximum.set(factor);
        if self
            .with_parser(|p| p.set_billion_laughs_maximum_amplification(factor))
            .is_none()
        {
            let _ = self
                .stopper
                .set_billion_laughs_maximum_amplification(factor);
        }
        XML_PROP_ERROR_NONE
    }

    /// `setBillionLaughsAttackProtectionActivationThreshold`.
    fn set_bl_threshold(&self, bytes: u64) -> XML_Prop_Error {
        if self.is_child() {
            return XML_PROP_ERROR_PARSER_NOT_ROOT;
        }
        self.bl_threshold.set(bytes);
        if self
            .with_parser(|p| p.set_billion_laughs_activation_threshold(bytes))
            .is_none()
        {
            self.stopper.set_billion_laughs_activation_threshold(bytes);
        }
        XML_PROP_ERROR_NONE
    }

    /// `setAllocTrackerMaximumAmplification`.
    fn set_at_maximum(&self, factor: f32) -> XML_Prop_Error {
        if self.is_child() {
            return XML_PROP_ERROR_PARSER_NOT_ROOT;
        }
        if factor.is_nan() || factor < 1.0 {
            return XML_PROP_ERROR_INVALID_VALUE;
        }
        self.at_maximum.set(factor);
        if self
            .with_parser(|p| p.set_alloc_tracker_maximum_amplification(factor))
            .is_none()
        {
            let _ = self.stopper.set_alloc_tracker_maximum_amplification(factor);
        }
        XML_PROP_ERROR_NONE
    }

    /// `setAllocTrackerActivationThreshold`.
    fn set_at_threshold(&self, bytes: u64) -> XML_Prop_Error {
        if self.is_child() {
            return XML_PROP_ERROR_PARSER_NOT_ROOT;
        }
        self.at_threshold.set(bytes);
        if self
            .with_parser(|p| p.set_alloc_tracker_activation_threshold(bytes))
            .is_none()
        {
            self.stopper.set_alloc_tracker_activation_threshold(bytes);
        }
        XML_PROP_ERROR_NONE
    }

    /// `setReparseDeferralEnabled`.
    fn set_reparse(&self, enabled: XML_Bool) -> XML_Prop_Error {
        let on = match enabled {
            0 => false,
            1 => true,
            _ => return XML_PROP_ERROR_INVALID_VALUE,
        };
        self.reparse_deferral.set(on);
        if self
            .with_parser(|p| p.set_reparse_deferral_enabled(on))
            .is_none()
        {
            self.stopper.set_reparse_deferral_enabled(on);
        }
        XML_PROP_ERROR_NONE
    }
}

/// A 16-byte hash salt from two `RandomState` hashers.
fn salt() -> [u8; 16] {
    let state = std::collections::hash_map::RandomState::new();
    let a = state.hash_one(0x6578_7061_745f_6361_u64);
    let b = state.hash_one(0x7069_5f73_616c_7400_u64);
    let mut out = [0; 16];
    out[..8].copy_from_slice(&a.to_le_bytes());
    out[8..].copy_from_slice(&b.to_le_bytes());
    out
}

/// Passes the parser's events to the C handlers of one `XML_ParserStruct`.
struct Dispatch<'a> {
    state: &'a XML_ParserStruct,
    text: &'a mut Vec<u8>,
    atts: &'a mut Vec<*const XML_Char>,
}

impl Dispatch<'_> {
    /// Copies `parts` into `text`, each followed by a NUL, and returns the
    /// offset of each copy.
    fn copy(&mut self, parts: &[&[u8]], offsets: &mut Vec<usize>) {
        self.text.clear();
        offsets.clear();
        for part in parts {
            offsets.push(self.text.len());
            self.text.extend_from_slice(part);
            self.text.push(0);
        }
    }

    /// A pointer to the NUL-terminated copy at `offset` in `text`.
    fn at(&self, offset: usize) -> *const XML_Char {
        self.text
            .get(offset..)
            .map_or(std::ptr::null(), |s| s.as_ptr().cast())
    }

    /// NUL-terminated copies of `parts` in `text`, with a pointer to each
    /// (`NULL` for `None`), valid until `text` is next changed.
    fn strs<const N: usize>(&mut self, parts: [Option<&[u8]>; N]) -> [*const XML_Char; N] {
        let present: Vec<&[u8]> = parts.iter().flatten().copied().collect();
        let mut offsets = Vec::with_capacity(N);
        self.copy(&present, &mut offsets);
        let mut it = offsets.into_iter();
        parts.map(|p| {
            p.and_then(|_| it.next())
                .map_or(std::ptr::null(), |o| self.at(o))
        })
    }

    /// One NUL-terminated string (or `NULL` for `None`) passed to `f`.
    fn with_str<T>(&mut self, s: Option<&[u8]>, f: impl FnOnce(*const XML_Char) -> T) -> T {
        match s {
            None => f(std::ptr::null()),
            Some(s) => {
                let mut offsets = Vec::new();
                self.copy(&[s], &mut offsets);
                f(offsets.first().map_or(std::ptr::null(), |&o| self.at(o)))
            }
        }
    }
}

impl Handler for Dispatch<'_> {
    fn take_base(&mut self) -> Option<Option<Vec<u8>>> {
        self.state.pending_base.borrow_mut().take()
    }

    fn external_entity_ref(
        &mut self,
        entities: &mut ExternalEntities<'_>,
        context: Option<&[u8]>,
        base: Option<&[u8]>,
        system_id: Option<&[u8]>,
        public_id: Option<&[u8]>,
    ) -> bool {
        let Some(h) = self.state.external_entity_ref.get() else {
            return false;
        };
        let this: XML_Parser = std::ptr::from_ref(self.state).cast_mut();
        let arg = match self.state.external_entity_ref_arg.get() {
            p if p.is_null() => this,
            p => p,
        };
        let [c, b, sys, public] = self.strs([context, base, system_id, public_id]);
        let entities: *mut ExternalEntities<'_> = entities;
        let previous = self.state.entities.replace(entities.cast());
        // SAFETY: a handler of the C type `expat.h` declares; the strings
        // are NUL-terminated copies that live until `text` is next changed,
        // which nothing does during the call (an external entity parser
        // has its own buffers).
        let status = unsafe { h(arg, c, b, sys, public) };
        // An external entity parser the handler did not free is closed
        // now: it borrows this parser, which goes on after the return.
        let child = self.state.open_child.replace(std::ptr::null_mut());
        // SAFETY: `open_child` only holds a live handle created by
        // `XML_ExternalEntityParserCreate` during this call (it is cleared
        // when that handle is freed).
        if let Some(child) = unsafe { state(child) } {
            child.close();
        }
        self.state.entities.set(previous);
        status != XML_STATUS_ERROR as c_int
    }

    fn unknown_encoding(&mut self, name: &[u8]) -> Option<EncodingInfo> {
        let (Some(h), data) = self.state.unknown_encoding.get() else {
            return None;
        };
        // The NUL-terminated name, on the stack when it is short; a
        // longer one is copied fallibly, and when that fails the encoding
        // is treated as unknown (D-067).
        let mut short = [0u8; 128];
        let mut long = Vec::new();
        let c_name: &[u8] =
            if let Some(dst) = short.get_mut(..name.len()).filter(|_| name.len() < 128) {
                dst.copy_from_slice(name);
                &short
            } else {
                if long.try_reserve_exact(name.len() + 1).is_err() {
                    return None;
                }
                long.extend_from_slice(name);
                long.push(0);
                &long
            };
        let mut info = XML_Encoding {
            map: [-1; 256],
            data: std::ptr::null_mut(),
            convert: None,
            release: None,
        };
        // SAFETY: a handler of the C type `expat.h` declares, called with
        // the data the program set, a NUL-terminated name and a valid
        // `XML_Encoding`.
        let status = unsafe { h(data, c_name.as_ptr().cast(), &raw mut info) };
        let user = EncodingData(info.data);
        let mut out = EncodingInfo::new();
        if let Some(r) = info.release {
            let release = move || {
                let user = user;
                // SAFETY: the release function the handler gave, called
                // once with its data, as libexpat does.
                unsafe { r(user.0) }
            };
            match try_box(release) {
                Some(b) => out.release = Some(b as ReleaseFn),
                None => {
                    // The closure is not boxed; it is called here, and
                    // the encoding is refused (D-067).
                    // SAFETY: as above.
                    unsafe { r(user.0) };
                    return None;
                }
            }
        }
        if status == 0 {
            // `out` is dropped: libexpat calls `release` at once.
            return None;
        }
        out.map = info.map;
        if let Some(f) = info.convert {
            let convert = move |seq: &[u8]| {
                let user = user;
                // SAFETY: the converter the handler gave, called with its
                // data and the bytes of one sequence, as many as the map
                // gives its first byte (libexpat passes a pointer to them).
                unsafe { f(user.0, seq.as_ptr().cast()) }
            };
            // When the closure cannot be boxed, `out` is dropped and its
            // release function runs (D-067).
            out.convert = Some(try_box(convert)? as ConvertFn);
        }
        Some(out)
    }

    fn not_standalone(&mut self) -> bool {
        match self.state.not_standalone.get() {
            // SAFETY: a handler of the C type `expat.h` declares, called
            // with the handler argument the program chose.
            Some(h) => (unsafe { h(self.state.handler_arg.get()) }) != 0,
            None => true,
        }
    }

    fn position(&mut self, line: u64, column: u64, byte_index: i64) {
        self.state.event_position.set((line, column, byte_index));
    }

    fn input_context(&mut self, buffer: &[u8], offset: Option<usize>, byte_count: u64) {
        self.state
            .event_input
            .set((buffer.as_ptr(), buffer.len(), offset, byte_count));
    }

    fn id_attribute(&mut self, index: Option<usize>) {
        let c = index.map_or(-1, |i| {
            c_int::try_from(i.saturating_mul(2)).unwrap_or(c_int::MAX)
        });
        self.state.id_index.set(c);
    }

    fn start_element(&mut self, name: &[u8], attributes: &[(&[u8], &[u8])], specified: usize) {
        let Some(h) = self.state.start_element.get() else {
            return;
        };
        self.state
            .specified
            .set(c_int::try_from(specified.saturating_mul(2)).unwrap_or(c_int::MAX));
        let mut parts: Vec<&[u8]> = Vec::with_capacity(1 + 2 * attributes.len());
        parts.push(name);
        for (n, v) in attributes {
            parts.push(n);
            parts.push(v);
        }
        let mut offsets = Vec::with_capacity(parts.len());
        self.copy(&parts, &mut offsets);
        let mut atts = std::mem::take(self.atts);
        atts.clear();
        atts.extend(offsets.iter().skip(1).map(|&o| self.at(o)));
        atts.push(std::ptr::null());
        let name = offsets.first().map_or(std::ptr::null(), |&o| self.at(o));
        // SAFETY: `h` is a handler the program set; `name` and every entry of
        // `atts` point into `text`, NUL-terminated, and `atts` ends with
        // NULL, as libexpat passes them; both stay valid for the call.
        unsafe { h(self.state.handler_arg.get(), name, atts.as_mut_ptr()) };
        *self.atts = atts;
    }

    fn end_element(&mut self, name: &[u8]) {
        let Some(h) = self.state.end_element.get() else {
            return;
        };
        let arg = self.state.handler_arg.get();
        // SAFETY: `h` is a handler the program set; the name is a
        // NUL-terminated copy valid for the call.
        self.with_str(Some(name), |n| unsafe { h(arg, n) });
    }

    fn character_data(&mut self, data: &[u8]) {
        if let Some(h) = self.state.character_data.get() {
            pieces(h, self.state.handler_arg.get(), data);
        }
    }

    fn default_data(&mut self, data: &[u8]) {
        if let Some(h) = self.state.default.get() {
            pieces(h, self.state.handler_arg.get(), data);
        }
    }

    fn event_text(&mut self, parts: &[&[u8]]) {
        let mut current = self.state.current.borrow_mut();
        let (text, ends) = &mut *current;
        text.clear();
        ends.clear();
        for part in parts {
            text.extend_from_slice(part);
            ends.push(text.len());
        }
    }

    fn processing_instruction(&mut self, target: &[u8], data: &[u8]) {
        let Some(h) = self.state.processing_instruction.get() else {
            return;
        };
        let mut offsets = Vec::with_capacity(2);
        self.copy(&[target, data], &mut offsets);
        let (t, d) = match offsets.as_slice() {
            [t, d] => (self.at(*t), self.at(*d)),
            _ => return,
        };
        // SAFETY: `h` is a handler the program set; both strings are
        // NUL-terminated copies valid for the call.
        unsafe { h(self.state.handler_arg.get(), t, d) };
    }

    fn comment(&mut self, data: &[u8]) {
        let Some(h) = self.state.comment.get() else {
            return;
        };
        let arg = self.state.handler_arg.get();
        // SAFETY: `h` is a handler the program set; the text is a
        // NUL-terminated copy valid for the call.
        self.with_str(Some(data), |d| unsafe { h(arg, d) });
    }

    fn start_cdata_section(&mut self) {
        if let Some(h) = self.state.start_cdata_section.get() {
            // SAFETY: `h` is a handler the program set.
            unsafe { h(self.state.handler_arg.get()) };
        }
    }

    fn end_cdata_section(&mut self) {
        if let Some(h) = self.state.end_cdata_section.get() {
            // SAFETY: `h` is a handler the program set.
            unsafe { h(self.state.handler_arg.get()) };
        }
    }

    fn start_namespace_decl(&mut self, prefix: Option<&[u8]>, uri: Option<&[u8]>) {
        let Some(h) = self.state.start_namespace_decl.get() else {
            return;
        };
        let mut offsets = Vec::with_capacity(2);
        let parts: Vec<&[u8]> = [prefix, uri].into_iter().flatten().collect();
        self.copy(&parts, &mut offsets);
        let mut it = offsets.iter();
        let p = prefix
            .and_then(|_| it.next())
            .map_or(std::ptr::null(), |&o| self.at(o));
        let u = uri
            .and_then(|_| it.next())
            .map_or(std::ptr::null(), |&o| self.at(o));
        // SAFETY: `h` is a handler the program set; each pointer is NULL or
        // a NUL-terminated copy valid for the call, as libexpat passes them.
        unsafe { h(self.state.handler_arg.get(), p, u) };
    }

    fn end_namespace_decl(&mut self, prefix: Option<&[u8]>) {
        let Some(h) = self.state.end_namespace_decl.get() else {
            return;
        };
        let arg = self.state.handler_arg.get();
        // SAFETY: `h` is a handler the program set; the prefix is NULL or a
        // NUL-terminated copy valid for the call.
        self.with_str(prefix, |p| unsafe { h(arg, p) });
    }

    fn xml_decl(
        &mut self,
        version: Option<&[u8]>,
        encoding: Option<&[u8]>,
        standalone: Option<bool>,
    ) {
        let Some(h) = self.state.xml_decl.get() else {
            return;
        };
        let [v, e] = self.strs([version, encoding]);
        let standalone = standalone.map_or(-1, c_int::from);
        // SAFETY: `h` is a handler the program set; each string is NULL or
        // a NUL-terminated copy valid for the call.
        unsafe { h(self.state.handler_arg.get(), v, e, standalone) };
    }

    fn start_doctype_decl(
        &mut self,
        name: &[u8],
        system_id: Option<&[u8]>,
        public_id: Option<&[u8]>,
        has_internal_subset: bool,
    ) {
        let Some(h) = self.state.start_doctype_decl.get() else {
            return;
        };
        let [n, sys, publ] = self.strs([Some(name), system_id, public_id]);
        // SAFETY: as for `xml_decl`.
        unsafe {
            h(
                self.state.handler_arg.get(),
                n,
                sys,
                publ,
                c_int::from(has_internal_subset),
            )
        };
    }

    fn end_doctype_decl(&mut self) {
        if let Some(h) = self.state.end_doctype_decl.get() {
            // SAFETY: `h` is a handler the program set.
            unsafe { h(self.state.handler_arg.get()) };
        }
    }

    fn entity_decl(
        &mut self,
        name: &[u8],
        is_parameter_entity: bool,
        value: Option<&[u8]>,
        base: Option<&[u8]>,
        system_id: Option<&[u8]>,
        public_id: Option<&[u8]>,
        notation: Option<&[u8]>,
    ) {
        let Some(h) = self.state.entity_decl.get() else {
            return;
        };
        let len = value.map_or(0, |v| c_int::try_from(v.len()).unwrap_or(c_int::MAX));
        let [n, v, b, sys, publ, nt] =
            self.strs([Some(name), value, base, system_id, public_id, notation]);
        // SAFETY: as for `xml_decl`; the value is `len` bytes (NUL-terminated
        // here, which libexpat does not promise).
        unsafe {
            h(
                self.state.handler_arg.get(),
                n,
                c_int::from(is_parameter_entity),
                v,
                len,
                b,
                sys,
                publ,
                nt,
            );
        };
    }

    fn unparsed_entity_decl(
        &mut self,
        name: &[u8],
        base: Option<&[u8]>,
        system_id: Option<&[u8]>,
        public_id: Option<&[u8]>,
        notation: &[u8],
    ) {
        let Some(h) = self.state.unparsed_entity_decl.get() else {
            return;
        };
        let [n, b, sys, publ, nt] =
            self.strs([Some(name), base, system_id, public_id, Some(notation)]);
        // SAFETY: as for `xml_decl`.
        unsafe { h(self.state.handler_arg.get(), n, b, sys, publ, nt) };
    }

    fn notation_decl(
        &mut self,
        name: &[u8],
        base: Option<&[u8]>,
        system_id: Option<&[u8]>,
        public_id: Option<&[u8]>,
    ) {
        let Some(h) = self.state.notation_decl.get() else {
            return;
        };
        let [n, b, sys, publ] = self.strs([Some(name), base, system_id, public_id]);
        // SAFETY: as for `xml_decl`.
        unsafe { h(self.state.handler_arg.get(), n, b, sys, publ) };
    }

    fn element_decl(&mut self, name: &[u8], model: &ContentModel) {
        let Some(h) = self.state.element_decl.get() else {
            return;
        };
        let suite = self.state.suite.borrow().clone();
        let malloc = self.state.mem.get().malloc;
        let root = build_model(model, |size| {
            // The block the tracker allocated for this declaration through
            // the suite (D-060), else a new one.
            match suite.as_ref().and_then(|s| s.take_model(size)) {
                Some(p) => p,
                // SAFETY: `malloc` or the program's `malloc_fcn`, which
                // takes any size.
                None => unsafe { malloc(size) },
            }
        });
        if root.is_null() {
            // libexpat ends the parse with `XML_ERROR_NO_MEMORY`; the
            // nearest this interface can do is to abort it.
            let _ = self.state.stopper.stop(false);
            return;
        }
        let arg = self.state.handler_arg.get();
        // SAFETY: `h` is a handler the program set; the name is a
        // NUL-terminated copy valid for the call; `root` is a model the
        // program now owns and frees with `XML_FreeContentModel`.
        self.with_str(Some(name), |n| unsafe { h(arg, n, root) });
    }

    fn attlist_decl(
        &mut self,
        element: &[u8],
        attribute: &[u8],
        attribute_type: &[u8],
        default: Option<&[u8]>,
        is_required: bool,
    ) {
        let Some(h) = self.state.attlist_decl.get() else {
            return;
        };
        let [e, a, t, d] = self.strs([
            Some(element),
            Some(attribute),
            Some(attribute_type),
            default,
        ]);
        // SAFETY: as for `xml_decl`.
        unsafe {
            h(
                self.state.handler_arg.get(),
                e,
                a,
                t,
                d,
                c_int::from(is_required),
            )
        };
    }

    fn skipped_entity(&mut self, name: &[u8], is_parameter_entity: bool) {
        let Some(h) = self.state.skipped_entity.get() else {
            return;
        };
        let arg = self.state.handler_arg.get();
        // SAFETY: `h` is a handler the program set; the name is a
        // NUL-terminated copy valid for the call.
        self.with_str(Some(name), |n| unsafe {
            h(arg, n, c_int::from(is_parameter_entity))
        });
    }
}

/// Calls a handler that takes a length (`XML_CharacterDataHandler`,
/// `XML_DefaultHandler`) with `data`. `XML_Parse` takes an `int` length,
/// so a piece never exceeds it; the split only keeps the conversion total.
/// Empty text is passed as one call with length 0, as libexpat does.
fn pieces(
    h: unsafe extern "C" fn(*mut c_void, *const XML_Char, c_int),
    arg: *mut c_void,
    data: &[u8],
) {
    let max = usize::try_from(c_int::MAX).unwrap_or(usize::MAX);
    let empty: [&[u8]; 1] = [data];
    let chunks: Box<dyn Iterator<Item = &[u8]>> = if data.is_empty() {
        Box::new(empty.into_iter())
    } else {
        Box::new(data.chunks(max))
    };
    for piece in chunks {
        let len = c_int::try_from(piece.len()).unwrap_or(c_int::MAX);
        // SAFETY: `h` is a handler the program set; `piece` is valid for
        // `len` bytes for the call (this text is not NUL-terminated in
        // libexpat either).
        unsafe { h(arg, piece.as_ptr().cast(), len) };
    }
}

/// The parser of a handle, or `None` for `NULL`.
///
/// # Safety
///
/// `parser` is `NULL` or a handle from `XML_ParserCreate` or
/// `XML_ParserCreateNS` that was not freed.
unsafe fn state<'a>(parser: XML_Parser) -> Option<&'a XML_ParserStruct> {
    // SAFETY: by the caller's contract the pointer is NULL or points to a
    // live `XML_ParserStruct` made by `Box::into_raw`; only shared
    // references to it are ever formed.
    unsafe { parser.cast_const().as_ref() }
}

/// The bytes of a C string, or `None` for `NULL`.
///
/// # Safety
///
/// `s` is `NULL` or a NUL-terminated string.
unsafe fn c_bytes<'a>(s: *const XML_Char) -> Option<&'a [u8]> {
    if s.is_null() {
        return None;
    }
    // SAFETY: `s` is non-NULL and NUL-terminated by the caller's contract.
    Some(unsafe { CStr::from_ptr(s) }.to_bytes())
}

fn create(options: Options) -> XML_Parser {
    match Parser::try_with_options(salt(), options) {
        Some(p) => XML_ParserStruct::own(p).map_or(std::ptr::null_mut(), into_handle),
        None => std::ptr::null_mut(),
    }
}

/// `b.to_vec()`, or `None` instead of aborting when the system allocator
/// fails (D-065).
fn try_bytes(b: &[u8]) -> Option<Vec<u8>> {
    let mut v = Vec::new();
    v.try_reserve_exact(b.len().checked_add(1)?).ok()?;
    v.extend_from_slice(b);
    Some(v)
}

/// `CString::new(b)` for a string without NUL bytes, or `None` instead of
/// aborting when the system allocator fails (D-065). The vector has room
/// for the terminator, so `CString::new` does not allocate again.
fn try_c_string(b: &[u8]) -> Option<CString> {
    CString::new(try_bytes(b)?).ok()
}

/// `Box::into_raw(Box::new(state))`, or `NULL` instead of aborting when
/// the system allocator cannot provide the block (D-065), as libexpat's
/// `XML_ParserCreate` returns `NULL` after a failed `malloc`. The block
/// has the layout `Box` uses, so `XML_ParserFree` releases it with
/// `Box::from_raw`.
fn into_handle(state: XML_ParserStruct) -> XML_Parser {
    let layout = std::alloc::Layout::new::<XML_ParserStruct>();
    // SAFETY: `XML_ParserStruct` is not zero-sized, so `layout` is valid
    // for `alloc`.
    let block = unsafe { std::alloc::alloc(layout) }.cast::<XML_ParserStruct>();
    if block.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: `block` is a fresh, aligned allocation for one
    // `XML_ParserStruct`, which this write initialises.
    unsafe { block.write(state) };
    block
}

/// `XML_ParserCreate`: a parser for the encoding named by `encoding`
/// (`NULL` for the default), without namespace processing.
///
/// # Safety
///
/// `encoding` is `NULL` or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_ParserCreate(encoding: *const XML_Char) -> XML_Parser {
    let mut options = Options::new();
    // SAFETY: forwarded from this function's contract.
    if let Some(name) = unsafe { c_bytes(encoding) } {
        let Some(o) = options.try_encoding(name) else {
            return std::ptr::null_mut();
        };
        options = o;
    }
    create(options)
}

/// `XML_ParserCreateNS`: like `XML_ParserCreate`, with namespace
/// processing and `namespace_separator` between the namespace name and the
/// local name.
///
/// # Safety
///
/// `encoding` is `NULL` or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_ParserCreateNS(
    encoding: *const XML_Char,
    namespace_separator: XML_Char,
) -> XML_Parser {
    let mut options = Options::new().namespace_separator(namespace_separator.to_ne_bytes()[0]);
    // SAFETY: forwarded from this function's contract.
    if let Some(name) = unsafe { c_bytes(encoding) } {
        let Some(o) = options.try_encoding(name) else {
            return std::ptr::null_mut();
        };
        options = o;
    }
    create(options)
}

/// `XML_ParserFree`. Does nothing for `NULL` or from inside a handler.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; it is not used after this call
/// (unless the call was made from inside a handler).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_ParserFree(parser: XML_Parser) {
    // SAFETY: forwarded from this function's contract.
    match unsafe { state(parser) } {
        Some(s) if !s.busy.get() => {
            // SAFETY: `parent` is NULL or the live parser whose external
            // entity reference handler is running.
            if let Some(p) = unsafe { state(s.parent.get()) }
                && p.open_child.get() == parser
            {
                p.open_child.set(std::ptr::null_mut());
            }
        }
        _ => return,
    }
    // SAFETY: as above; `run_encoding_release` leaves `busy` false again.
    if let Some(s) = unsafe { state(parser) } {
        s.run_encoding_release();
    }
    // A child created outside a handler lends its parent back once dropped.
    // SAFETY: as above.
    let lender = unsafe { state(parser) }
        .filter(|s| s.lent.get())
        .map(|s| s.parent.get());
    // SAFETY: the handle came from `Box::into_raw` in `create`, is live and
    // no handler is running, so no reference to it is left.
    drop(unsafe { Box::from_raw(parser) });
    // SAFETY: the parent of a lent child outlives it (its `XML_ParserFree`
    // does nothing while `busy`).
    if let Some(p) = lender.and_then(|p| unsafe { state(p) }) {
        p.busy.set(false);
    }
}

/// `XML_SetUserData`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetUserData(parser: XML_Parser, user_data: *mut c_void) {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return;
    };
    if s.handler_arg.get() == s.user_data.get() {
        s.handler_arg.set(user_data);
    }
    s.user_data.set(user_data);
}

/// `XML_UseParserAsHandlerArg`: handlers receive the parser handle instead
/// of the user data.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_UseParserAsHandlerArg(parser: XML_Parser) {
    // SAFETY: forwarded from this function's contract.
    if let Some(s) = unsafe { state(parser) } {
        s.handler_arg.set(parser.cast());
    }
}

macro_rules! setter {
    ($(#[$m:meta])* $name:ident, $field:ident, $ty:ty) => {
        $(#[$m])*
        ///
        /// # Safety
        ///
        /// `parser` is `NULL` or a live handle; the handler is `NULL` or a
        /// function of the C type `expat.h` declares.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(parser: XML_Parser, handler: $ty) {
            // SAFETY: forwarded from this function's contract.
            if let Some(s) = unsafe { state(parser) } {
                s.$field.set(handler);
            }
        }
    };
}

setter!(
    /// `XML_SetStartElementHandler`.
    XML_SetStartElementHandler, start_element, XML_StartElementHandler
);
setter!(
    /// `XML_SetEndElementHandler`.
    XML_SetEndElementHandler, end_element, XML_EndElementHandler
);
setter!(
    /// `XML_SetCharacterDataHandler`.
    XML_SetCharacterDataHandler, character_data, XML_CharacterDataHandler
);
setter!(
    /// `XML_SetProcessingInstructionHandler`.
    XML_SetProcessingInstructionHandler,
    processing_instruction,
    XML_ProcessingInstructionHandler
);
setter!(
    /// `XML_SetCommentHandler`.
    XML_SetCommentHandler, comment, XML_CommentHandler
);
setter!(
    /// `XML_SetStartCdataSectionHandler`.
    XML_SetStartCdataSectionHandler, start_cdata_section, XML_StartCdataSectionHandler
);
setter!(
    /// `XML_SetEndCdataSectionHandler`.
    XML_SetEndCdataSectionHandler, end_cdata_section, XML_EndCdataSectionHandler
);
setter!(
    /// `XML_SetStartNamespaceDeclHandler`.
    XML_SetStartNamespaceDeclHandler, start_namespace_decl, XML_StartNamespaceDeclHandler
);
setter!(
    /// `XML_SetEndNamespaceDeclHandler`.
    XML_SetEndNamespaceDeclHandler, end_namespace_decl, XML_EndNamespaceDeclHandler
);
setter!(
    /// `XML_SetXmlDeclHandler`.
    XML_SetXmlDeclHandler, xml_decl, XML_XmlDeclHandler
);
setter!(
    /// `XML_SetStartDoctypeDeclHandler`.
    XML_SetStartDoctypeDeclHandler, start_doctype_decl, XML_StartDoctypeDeclHandler
);
setter!(
    /// `XML_SetEndDoctypeDeclHandler`.
    XML_SetEndDoctypeDeclHandler, end_doctype_decl, XML_EndDoctypeDeclHandler
);
setter!(
    /// `XML_SetEntityDeclHandler`.
    XML_SetEntityDeclHandler, entity_decl, XML_EntityDeclHandler
);
setter!(
    /// `XML_SetUnparsedEntityDeclHandler`.
    XML_SetUnparsedEntityDeclHandler, unparsed_entity_decl, XML_UnparsedEntityDeclHandler
);
setter!(
    /// `XML_SetNotationDeclHandler`.
    XML_SetNotationDeclHandler, notation_decl, XML_NotationDeclHandler
);
setter!(
    /// `XML_SetElementDeclHandler`. The handler owns the model it receives
    /// and frees it with `XML_FreeContentModel`.
    XML_SetElementDeclHandler, element_decl, XML_ElementDeclHandler
);
setter!(
    /// `XML_SetAttlistDeclHandler`.
    XML_SetAttlistDeclHandler, attlist_decl, XML_AttlistDeclHandler
);
setter!(
    /// `XML_SetSkippedEntityHandler`.
    XML_SetSkippedEntityHandler, skipped_entity, XML_SkippedEntityHandler
);
setter!(
    /// `XML_SetExternalEntityRefHandler`. The handler creates the entity's
    /// parser with `XML_ExternalEntityParserCreate`.
    XML_SetExternalEntityRefHandler, external_entity_ref, XML_ExternalEntityRefHandler
);
setter!(
    /// `XML_SetNotStandaloneHandler`.
    XML_SetNotStandaloneHandler, not_standalone, XML_NotStandaloneHandler
);

/// `XML_SetUnknownEncodingHandler`: the handler asked for an encoding no
/// built-in encoding has, with `data` as its first argument. Kept by
/// `XML_ParserReset`, as in libexpat.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetUnknownEncodingHandler(
    parser: XML_Parser,
    handler: XML_UnknownEncodingHandler,
    data: *mut c_void,
) {
    // SAFETY: forwarded from this function's contract.
    if let Some(s) = unsafe { state(parser) } {
        s.unknown_encoding.set((handler, data));
    }
}

/// `XML_SetExternalEntityRefHandlerArg`: the first argument of the
/// external entity reference handler (`NULL` for the parser itself).
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetExternalEntityRefHandlerArg(parser: XML_Parser, arg: *mut c_void) {
    // SAFETY: forwarded from this function's contract.
    if let Some(s) = unsafe { state(parser) } {
        s.external_entity_ref_arg.set(arg.cast());
    }
}

/// `XML_ExternalEntityParserCreate`: the parser of an external entity of
/// `parser`'s document, with `context` as the external entity reference
/// handler received it (`NULL` for a parameter entity or the external
/// subset) and the encoding `encoding` names (`NULL` for the entity's
/// own). It starts with the handlers, user data and settings `parser`
/// has. `NULL` where libexpat returns `NULL`, and also while another
/// external entity parser of `parser` created in the same handler call,
/// or outside a handler, is still open. Inside the handler, the new
/// parser lives at most until that handler call returns: one the handler
/// did not free is closed then, and its parsing calls fail with
/// `XML_ERROR_FINISHED` until it is freed. Outside a handler, `parser`
/// stays busy (as from inside a handler) until the new parser is freed
/// with `XML_ParserFree` (D-060).
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; `context` and `encoding` are
/// `NULL` or NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_ExternalEntityParserCreate(
    parser: XML_Parser,
    context: *const XML_Char,
    encoding: *const XML_Char,
) -> XML_Parser {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return std::ptr::null_mut();
    };
    let entities = s.entities.get();
    if !s.open_child.get().is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: forwarded from this function's contract.
    let (context, encoding) = unsafe { (c_bytes(context), c_bytes(encoding)) };
    let lent = entities.is_null();
    let child = if lent {
        // Outside the external entity reference handler: the child borrows
        // the parser until it is freed, and the parser refuses parsing,
        // reset and release meanwhile, as from inside a handler.
        s.with_parser(|p| {
            // SAFETY: the child borrows `parser`'s Rust parser; `busy` is
            // set below before anything else can run, so `with_parser` and
            // the parsing functions (the only other ways to reach it)
            // leave it alone until `XML_ParserFree` of the child clears
            // `busy` after dropping it.
            p.external_parser(context, encoding).map(|c| unsafe {
                std::mem::transmute::<ExternalParser<'_>, ExternalParser<'static>>(c)
            })
        })
        .flatten()
    } else {
        // SAFETY: `entities` is set only while the external entity
        // reference handler of `parser` runs (`Dispatch::external_entity_ref`),
        // and points to the `ExternalEntities` that call received; no other
        // reference to it is used meanwhile.
        let entities = unsafe { &mut *entities };
        entities.create(context, encoding).map(|c| {
            // SAFETY: the child borrows `parser`'s Rust parser, which stays
            // borrowed by the running parse (and unused) until the handler
            // call returns. The child is dropped before then: by
            // `XML_ParserFree`, or by `close` when the handler returns
            // (`open_child` holds it until one of the two happens), so the
            // borrow never outlives the call.
            unsafe { std::mem::transmute::<ExternalParser<'_>, ExternalParser<'static>>(c) }
        })
    };
    let Some(child) = child else {
        return std::ptr::null_mut();
    };
    if lent {
        s.busy.set(true);
    }
    let Some(c) = XML_ParserStruct::with_core(Core::Child(child)) else {
        if lent {
            s.busy.set(false);
        }
        return std::ptr::null_mut();
    };
    c.lent.set(lent);
    // `XML_ExternalEntityParserCreate` copies the handlers, the user data
    // and the handler argument.
    c.user_data.set(s.user_data.get());
    if s.handler_arg.get() == s.user_data.get() {
        c.handler_arg.set(s.user_data.get());
    }
    c.start_element.set(s.start_element.get());
    c.end_element.set(s.end_element.get());
    c.character_data.set(s.character_data.get());
    c.processing_instruction.set(s.processing_instruction.get());
    c.comment.set(s.comment.get());
    c.start_cdata_section.set(s.start_cdata_section.get());
    c.end_cdata_section.set(s.end_cdata_section.get());
    c.start_namespace_decl.set(s.start_namespace_decl.get());
    c.end_namespace_decl.set(s.end_namespace_decl.get());
    c.default.set(s.default.get());
    c.xml_decl.set(s.xml_decl.get());
    c.start_doctype_decl.set(s.start_doctype_decl.get());
    c.end_doctype_decl.set(s.end_doctype_decl.get());
    c.entity_decl.set(s.entity_decl.get());
    c.unparsed_entity_decl.set(s.unparsed_entity_decl.get());
    c.notation_decl.set(s.notation_decl.get());
    c.element_decl.set(s.element_decl.get());
    c.attlist_decl.set(s.attlist_decl.get());
    c.skipped_entity.set(s.skipped_entity.get());
    c.external_entity_ref.set(s.external_entity_ref.get());
    if s.external_entity_ref_arg.get() != parser {
        c.external_entity_ref_arg
            .set(s.external_entity_ref_arg.get());
    }
    c.not_standalone.set(s.not_standalone.get());
    c.unknown_encoding.set(s.unknown_encoding.get());
    c.default_expand.set(s.default_expand.get());
    c.mem.set(s.mem.get());
    c.suite.replace(s.suite.borrow().clone());
    c.salt_set.set(s.salt_set.get());
    c.bl_maximum.set(s.bl_maximum.get());
    c.bl_threshold.set(s.bl_threshold.get());
    c.at_maximum.set(s.at_maximum.get());
    c.at_threshold.set(s.at_threshold.get());
    c.reparse_deferral.set(s.reparse_deferral.get());
    c.parent.set(parser);
    let child = into_handle(c);
    if child.is_null() {
        if lent {
            s.busy.set(false);
        }
        return std::ptr::null_mut();
    }
    if s.handler_arg.get() != s.user_data.get() {
        // SAFETY: `child` was just made by `Box::into_raw`.
        if let Some(c) = unsafe { state(child) } {
            c.handler_arg.set(child.cast());
        }
    }
    if !lent {
        s.open_child.set(child);
    }
    child
}

/// `XML_SetDoctypeDeclHandler`: both document type declaration handlers.
///
/// # Safety
///
/// As for `XML_SetStartDoctypeDeclHandler`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetDoctypeDeclHandler(
    parser: XML_Parser,
    start: XML_StartDoctypeDeclHandler,
    end: XML_EndDoctypeDeclHandler,
) {
    // SAFETY: forwarded from this function's contract.
    unsafe {
        XML_SetStartDoctypeDeclHandler(parser, start);
        XML_SetEndDoctypeDeclHandler(parser, end);
    }
}

/// `XML_FreeContentModel`: frees a model passed to the element
/// declaration handler with the parser's free function.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; `model` is `NULL` or a model this
/// parser passed and not yet freed; nothing uses it after the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_FreeContentModel(parser: XML_Parser, model: *mut XML_Content) {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return;
    };
    // SAFETY: `model` came from this parser's `malloc` (`build_model`).
    unsafe { (s.mem.get().free)(model.cast()) };
}

/// `XML_MemMalloc`: memory from the parser's allocation functions, not
/// counted towards its memory limit.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_MemMalloc(parser: XML_Parser, size: usize) -> *mut c_void {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return std::ptr::null_mut();
    };
    // SAFETY: the parser's `malloc` takes any size.
    unsafe { (s.mem.get().malloc)(size) }
}

/// `XML_MemRealloc`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; `ptr` is `NULL` or memory from
/// this parser's allocation functions.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_MemRealloc(
    parser: XML_Parser,
    ptr: *mut c_void,
    size: usize,
) -> *mut c_void {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return std::ptr::null_mut();
    };
    // SAFETY: forwarded from this function's contract.
    unsafe { (s.mem.get().realloc)(ptr, size) }
}

/// `XML_MemFree`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; `ptr` is `NULL` or memory from
/// this parser's allocation functions, not used after the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_MemFree(parser: XML_Parser, ptr: *mut c_void) {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return;
    };
    // SAFETY: forwarded from this function's contract.
    unsafe { (s.mem.get().free)(ptr) }
}

/// `XML_SetBase`: the base passed to the declaration handlers from now
/// on, or none for `NULL`. `XML_STATUS_ERROR` for a `NULL` parser or when
/// the parser's memory runs out. From inside a handler the parser takes
/// the base before it next uses it.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; `base` is `NULL` or a
/// NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetBase(parser: XML_Parser, base: *const XML_Char) -> XML_Status {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return XML_STATUS_ERROR;
    };
    // SAFETY: forwarded from this function's contract.
    let bytes = unsafe { c_bytes(base) };
    // The copy is made first, so that a failure of the system allocator
    // leaves the base as it was (D-065).
    let copy = match bytes {
        Some(b) => match try_c_string(b) {
            Some(c) => Some(c),
            None => return XML_STATUS_ERROR,
        },
        None => None,
    };
    match s.with_parser(|p| p.set_base(bytes)) {
        Some(Err(_)) => return XML_STATUS_ERROR,
        Some(Ok(())) => {}
        None => {
            let pending = match bytes {
                Some(b) => match try_bytes(b) {
                    Some(v) => Some(v),
                    None => return XML_STATUS_ERROR,
                },
                None => None,
            };
            *s.pending_base.borrow_mut() = Some(pending);
        }
    }
    *s.base.borrow_mut() = copy;
    XML_STATUS_OK
}

/// `XML_GetBase`: the base set with `XML_SetBase`, or `NULL`. The string
/// stays valid until the next `XML_SetBase` or `XML_ParserFree`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetBase(parser: XML_Parser) -> *const XML_Char {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return std::ptr::null();
    };
    s.base
        .borrow()
        .as_ref()
        .map_or(std::ptr::null(), |c| c.as_ptr())
}

/// `XML_SetDefaultHandler`: the handler for the text no other handler
/// reports. References to internal general entities in content are then
/// not expanded (even when `handler` is `NULL`, as in libexpat).
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; the handler is `NULL` or a
/// function of the C type `expat.h` declares.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetDefaultHandler(parser: XML_Parser, handler: XML_DefaultHandler) {
    // SAFETY: forwarded from this function's contract.
    if let Some(s) = unsafe { state(parser) } {
        s.default.set(handler);
        s.default_expand.set(false);
    }
}

/// `XML_SetDefaultHandlerExpand`: as `XML_SetDefaultHandler`, but
/// references to internal general entities stay expanded.
///
/// # Safety
///
/// As for `XML_SetDefaultHandler`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetDefaultHandlerExpand(
    parser: XML_Parser,
    handler: XML_DefaultHandler,
) {
    // SAFETY: forwarded from this function's contract.
    if let Some(s) = unsafe { state(parser) } {
        s.default.set(handler);
        s.default_expand.set(true);
    }
}

/// `XML_DefaultCurrent`: passes the event being reported to the default
/// handler, as libexpat would have passed it had the event's own handler
/// not been set. Does nothing outside a handler or without a default
/// handler.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_DefaultCurrent(parser: XML_Parser) {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return;
    };
    if !s.busy.get() {
        return;
    }
    let Some(h) = s.default.get() else {
        return;
    };
    // A copy, so that the handler may call this function again.
    let (text, ends) = s.current.borrow().clone();
    s.stopper.mark_default_current();
    let mut from = 0;
    for end in ends {
        pieces(
            h,
            s.handler_arg.get(),
            text.get(from..end).unwrap_or_default(),
        );
        from = end;
    }
}

/// `XML_SetElementHandler`: both element handlers.
///
/// # Safety
///
/// As for `XML_SetStartElementHandler`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetElementHandler(
    parser: XML_Parser,
    start: XML_StartElementHandler,
    end: XML_EndElementHandler,
) {
    // SAFETY: forwarded from this function's contract.
    unsafe {
        XML_SetStartElementHandler(parser, start);
        XML_SetEndElementHandler(parser, end);
    }
}

/// `XML_SetCdataSectionHandler`: both CDATA section handlers.
///
/// # Safety
///
/// As for `XML_SetStartCdataSectionHandler`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetCdataSectionHandler(
    parser: XML_Parser,
    start: XML_StartCdataSectionHandler,
    end: XML_EndCdataSectionHandler,
) {
    // SAFETY: forwarded from this function's contract.
    unsafe {
        XML_SetStartCdataSectionHandler(parser, start);
        XML_SetEndCdataSectionHandler(parser, end);
    }
}

/// `XML_SetNamespaceDeclHandler`: both namespace declaration handlers.
///
/// # Safety
///
/// As for `XML_SetStartNamespaceDeclHandler`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetNamespaceDeclHandler(
    parser: XML_Parser,
    start: XML_StartNamespaceDeclHandler,
    end: XML_EndNamespaceDeclHandler,
) {
    // SAFETY: forwarded from this function's contract.
    unsafe {
        XML_SetStartNamespaceDeclHandler(parser, start);
        XML_SetEndNamespaceDeclHandler(parser, end);
    }
}

/// `XML_Parse`: parses `len` bytes at `s`; `is_final` non-zero marks the
/// last piece.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; `s` points to `len` readable bytes
/// (or is `NULL` with `len` 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_Parse(
    parser: XML_Parser,
    s: *const c_char,
    len: c_int,
    is_final: c_int,
) -> XML_Status {
    // SAFETY: forwarded from this function's contract.
    let Some(state) = (unsafe { state(parser) }) else {
        return XML_STATUS_ERROR;
    };
    if state.busy.get() {
        return XML_STATUS_ERROR;
    }
    let Ok(n) = usize::try_from(len) else {
        state.error_code.set(error(ErrorCode::InvalidArgument));
        return XML_STATUS_ERROR;
    };
    let data: &[u8] = if n == 0 {
        &[]
    } else if s.is_null() {
        state.error_code.set(error(ErrorCode::InvalidArgument));
        return XML_STATUS_ERROR;
    } else {
        // SAFETY: `s` is non-NULL and points to `len` readable bytes by the
        // caller's contract; the bytes are not modified during the call.
        unsafe { std::slice::from_raw_parts(s.cast::<u8>(), n) }
    };
    run(state, |parser, dispatch, events| {
        parser.parse_with_events(dispatch, events, data, is_final != 0)
    })
}

/// Runs one parsing call `f` on the parser of `state` with the handlers
/// that are set, then records the error code the call left, as libexpat
/// sets `m_errorCode` on every parsing call.
fn run(
    state: &XML_ParserStruct,
    f: impl FnOnce(&mut Parser, &mut Dispatch<'_>, Events) -> Result<(), expat::Error>,
) -> XML_Status {
    if state.closed() {
        state.error_code.set(error(ErrorCode::Finished));
        return XML_STATUS_ERROR;
    }
    let events = state.events();
    state.busy.set(true);
    // SAFETY: the callers checked that `busy` was false, so nothing else
    // borrows `inner`, and it is true until this borrow ends, so a handler
    // calling back into the library cannot borrow it (`with_parser` and
    // the parsing functions check `busy`); handlers only reach the `Cell`
    // fields.
    let inner = unsafe { &mut *state.inner.get() };
    let Inner { parser, text, atts } = inner;
    let parser = parser.get();
    let mut dispatch = Dispatch { state, text, atts };
    #[cfg(feature = "testing")]
    let scanned = parser.bytes_scanned();
    let result = f(parser, &mut dispatch, events);
    #[cfg(feature = "testing")]
    testing::add_bytes_scanned(parser.bytes_scanned().wrapping_sub(scanned));
    state.busy.set(false);
    state
        .error_code
        .set(parser.error_code().map_or(XML_ERROR_NONE, error));
    match result {
        Ok(()) if parser.is_suspended() => XML_STATUS_SUSPENDED,
        Ok(()) => XML_STATUS_OK,
        Err(_) => XML_STATUS_ERROR,
    }
}

/// `XML_StopParser`: stops the parse after the current event, so that
/// `XML_ResumeParser` can continue it when `resumable` is non-zero, or for
/// good otherwise (`XML_ERROR_ABORTED`). Refusals set the error code as in
/// libexpat (`XML_ERROR_NOT_STARTED`, `XML_ERROR_FINISHED`,
/// `XML_ERROR_SUSPENDED`, `XML_ERROR_SUSPEND_PE`).
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_StopParser(parser: XML_Parser, resumable: XML_Bool) -> XML_Status {
    // SAFETY: forwarded from this function's contract.
    let Some(state) = (unsafe { state(parser) }) else {
        return XML_STATUS_ERROR;
    };
    match state.stopper.stop(resumable != 0) {
        Ok(()) => XML_STATUS_OK,
        Err(code) => {
            state.error_code.set(error(code));
            XML_STATUS_ERROR
        }
    }
}

/// `XML_ResumeParser`: continues a suspended parse up to the end of the
/// input given so far (`XML_ERROR_NOT_SUSPENDED` when it is not
/// suspended). Refused without an error code from inside a handler, as in
/// libexpat.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_ResumeParser(parser: XML_Parser) -> XML_Status {
    // SAFETY: forwarded from this function's contract.
    let Some(state) = (unsafe { state(parser) }) else {
        return XML_STATUS_ERROR;
    };
    if state.busy.get() {
        return XML_STATUS_ERROR;
    }
    run(state, |parser, dispatch, events| {
        parser.resume_with_events(dispatch, events)
    })
}

/// `XML_GetBuffer`: space for `len` bytes of input in the parser's buffer,
/// to be filled and then parsed with `XML_ParseBuffer`, or `NULL` with the
/// error code set (`XML_ERROR_NO_MEMORY` for a negative `len`). The space
/// is valid until the next call on the parser.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetBuffer(parser: XML_Parser, len: c_int) -> *mut c_void {
    // SAFETY: forwarded from this function's contract.
    let Some(state) = (unsafe { state(parser) }) else {
        return std::ptr::null_mut();
    };
    // A negative `len` becomes a length the parser refuses with
    // `XML_ERROR_NO_MEMORY`, as libexpat refuses it.
    let n = usize::try_from(len).unwrap_or(usize::MAX);
    let got = state.with_parser(|p| match p.buffer(n) {
        Ok(space) => Ok(space.as_mut_ptr()),
        Err(e) => Err(e.code()),
    });
    match got {
        Some(Ok(ptr)) => ptr.cast(),
        Some(Err(code)) => {
            state.error_code.set(error(code));
            std::ptr::null_mut()
        }
        // Called from inside a handler: refused without an error code, as
        // in libexpat 2.10.0.
        None => std::ptr::null_mut(),
    }
}

/// `XML_ParseBuffer`: parses the first `len` bytes written into the space
/// `XML_GetBuffer` returned; `is_final` non-zero marks the last piece.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_ParseBuffer(
    parser: XML_Parser,
    len: c_int,
    is_final: c_int,
) -> XML_Status {
    // SAFETY: forwarded from this function's contract.
    let Some(state) = (unsafe { state(parser) }) else {
        return XML_STATUS_ERROR;
    };
    if state.busy.get() {
        return XML_STATUS_ERROR;
    }
    // A negative `len` becomes a length the parser refuses with
    // `XML_ERROR_INVALID_ARGUMENT`, as libexpat refuses it.
    let n = usize::try_from(len).unwrap_or(usize::MAX);
    run(state, |parser, dispatch, events| {
        parser.parse_buffer_with_events(dispatch, events, n, is_final != 0)
    })
}

/// The C value of `code`.
fn error(code: ErrorCode) -> XML_Error {
    XML_Error::try_from(code.to_c()).unwrap_or(XML_Error::MAX)
}

/// `XML_GetErrorCode`; `XML_ERROR_INVALID_ARGUMENT` for `NULL`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetErrorCode(parser: XML_Parser) -> XML_Error {
    // SAFETY: forwarded from this function's contract.
    match unsafe { state(parser) } {
        Some(s) => s.error_code.get(),
        None => error(ErrorCode::InvalidArgument),
    }
}

/// `XML_ErrorString`: the libexpat text of `code`, or `NULL` for
/// `XML_ERROR_NONE` and unknown codes.
#[unsafe(no_mangle)]
pub extern "C" fn XML_ErrorString(code: XML_Error) -> *const XML_Char {
    static TEXTS: OnceLock<Vec<(XML_Error, CString)>> = OnceLock::new();
    let texts = TEXTS.get_or_init(|| {
        (1..=64u32)
            .filter_map(ErrorCode::from_c)
            .filter_map(|c| Some((error(c), CString::new(c.description()).ok()?)))
            .collect()
    });
    texts
        .iter()
        .find(|(c, _)| *c == code)
        .map_or(std::ptr::null(), |(_, t)| t.as_ptr())
}

/// `XML_GetCurrentLineNumber64`; 0 for `NULL`. Inside a handler, the
/// line of the event.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetCurrentLineNumber64(parser: XML_Parser) -> u64 {
    // SAFETY: forwarded from this function's contract.
    unsafe { state(parser) }.map_or(0, |s| s.position(|p| p.current_line(), |e| e.0))
}

/// `XML_GetCurrentColumnNumber64`; 0 for `NULL`. Inside a handler, the
/// column of the event.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetCurrentColumnNumber64(parser: XML_Parser) -> u64 {
    // SAFETY: forwarded from this function's contract.
    unsafe { state(parser) }.map_or(0, |s| s.position(|p| p.current_column(), |e| e.1))
}

/// `XML_GetCurrentByteIndex64`; -1 for `NULL`. Inside a handler, the
/// byte index of the event.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetCurrentByteIndex64(parser: XML_Parser) -> i64 {
    // SAFETY: forwarded from this function's contract.
    unsafe { state(parser) }.map_or(-1, |s| s.position(|p| p.current_byte_index(), |e| e.2))
}

/// `XML_GetCurrentLineNumber`: `XML_GetCurrentLineNumber64` converted to
/// `XML_Size` as libexpat does (wrapping where `unsigned long` is 32 bits).
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetCurrentLineNumber(parser: XML_Parser) -> XML_Size {
    // SAFETY: forwarded from this function's contract.
    let v = unsafe { XML_GetCurrentLineNumber64(parser) };
    // reason: C's `(XML_Size)` conversion, which wraps.
    #[allow(clippy::cast_possible_truncation)]
    let r = v as XML_Size;
    r
}

/// `XML_GetCurrentColumnNumber`, converted like `XML_GetCurrentLineNumber`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetCurrentColumnNumber(parser: XML_Parser) -> XML_Size {
    // SAFETY: forwarded from this function's contract.
    let v = unsafe { XML_GetCurrentColumnNumber64(parser) };
    // reason: C's `(XML_Size)` conversion, which wraps.
    #[allow(clippy::cast_possible_truncation)]
    let r = v as XML_Size;
    r
}

/// `XML_GetCurrentByteIndex`: `XML_GetCurrentByteIndex64` converted to
/// `XML_Index` as libexpat does.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetCurrentByteIndex(parser: XML_Parser) -> XML_Index {
    // SAFETY: forwarded from this function's contract.
    let v = unsafe { XML_GetCurrentByteIndex64(parser) };
    // reason: C's `(XML_Index)` conversion, which wraps.
    #[allow(clippy::cast_possible_truncation)]
    let r = v as XML_Index;
    r
}

/// `XML_GetSpecifiedAttributeCount`: twice the number of attributes given
/// in the tag of the last start element handler call; -1 for `NULL`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetSpecifiedAttributeCount(parser: XML_Parser) -> c_int {
    // SAFETY: forwarded from this function's contract.
    unsafe { state(parser) }.map_or(-1, |s| s.specified.get())
}

/// `XML_GetIdAttributeIndex`: the index in the attribute array of the
/// last start element handler call of the attribute declared with type
/// `ID`, counting names and values, or -1 if none was given in the tag;
/// -1 for `NULL`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetIdAttributeIndex(parser: XML_Parser) -> c_int {
    // SAFETY: forwarded from this function's contract.
    unsafe { state(parser) }.map_or(-1, |s| s.id_index.get())
}

/// `XML_GetCurrentByteCount64`: the length in bytes of the current event
/// as written, or 0; 0 for `NULL`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetCurrentByteCount64(parser: XML_Parser) -> u64 {
    // SAFETY: forwarded from this function's contract.
    unsafe { state(parser) }.map_or(0, |s| {
        s.position(|p| p.current_byte_count(), |_| s.event_input.get().3)
    })
}

/// `XML_GetCurrentByteCount`: `XML_GetCurrentByteCount64` converted to
/// `int` as libexpat does.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetCurrentByteCount(parser: XML_Parser) -> c_int {
    // SAFETY: forwarded from this function's contract.
    let v = unsafe { XML_GetCurrentByteCount64(parser) };
    // reason: C's `(int)` conversion, which wraps.
    #[allow(clippy::cast_possible_truncation)]
    let r = v as c_int;
    r
}

/// `XML_GetInputContext64`: the parser's input buffer, with the offset of
/// the current event in it and the length of the input it holds, or
/// `NULL` (leaving `offset` and `size` as they are) when there is no
/// current event; `NULL` for a `NULL` parser. The bytes are valid until
/// the parser next reads input or is freed (inside a handler, until the
/// handler returns).
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; `offset` and `size` are each
/// `NULL` or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetInputContext64(
    parser: XML_Parser,
    offset: *mut i64,
    size: *mut u64,
) -> *const c_char {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return std::ptr::null();
    };
    let context = s.position(
        |p| p.input_context().map(|(b, o)| (b.as_ptr(), b.len(), o)),
        |_| {
            let (ptr, len, offset, _) = s.event_input.get();
            offset.map(|o| (ptr, len, o))
        },
    );
    let Some((ptr, len, at)) = context else {
        return std::ptr::null();
    };
    if !offset.is_null() {
        // SAFETY: the caller passes a pointer valid for a write.
        unsafe { offset.write(i64::try_from(at).unwrap_or(i64::MAX)) };
    }
    if !size.is_null() {
        // SAFETY: as above.
        unsafe { size.write(len as u64) };
    }
    ptr.cast()
}

/// `XML_GetInputContext`: `XML_GetInputContext64` with `offset` and
/// `size` converted to `int` as libexpat does.
///
/// # Safety
///
/// As for [`XML_GetInputContext64`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetInputContext(
    parser: XML_Parser,
    offset: *mut c_int,
    size: *mut c_int,
) -> *const c_char {
    let mut offset64 = 0i64;
    let mut size64 = 0u64;
    // SAFETY: forwarded from this function's contract; the two locals are
    // valid for a write.
    let buffer = unsafe { XML_GetInputContext64(parser, &mut offset64, &mut size64) };
    if buffer.is_null() {
        return buffer;
    }
    // reason: C's `(int)` conversions, which wrap.
    #[allow(clippy::cast_possible_truncation)]
    let (o, z) = (offset64 as c_int, size64 as c_int);
    if !offset.is_null() {
        // SAFETY: the caller passes a pointer valid for a write.
        unsafe { offset.write(o) };
    }
    if !size.is_null() {
        // SAFETY: as above.
        unsafe { size.write(z) };
    }
    buffer
}

/// `XML_ExpatVersion`: `"expat_2.10.0"`, the libexpat version this crate
/// follows.
#[unsafe(no_mangle)]
pub extern "C" fn XML_ExpatVersion() -> *const XML_Char {
    c"expat_2.10.0".as_ptr()
}

/// `XML_ExpatVersionInfo`.
#[unsafe(no_mangle)]
pub extern "C" fn XML_ExpatVersionInfo() -> XML_Expat_Version {
    XML_Expat_Version {
        major: 2,
        minor: 10,
        micro: 0,
    }
}

#[cfg(test)]
mod tests;

/// `enum XML_Prop_Error`.
pub type XML_Prop_Error = c_int;
/// `XML_PROP_ERROR_NONE`.
pub const XML_PROP_ERROR_NONE: XML_Prop_Error = 0;
/// `XML_PROP_ERROR_INVALID_KEY`.
pub const XML_PROP_ERROR_INVALID_KEY: XML_Prop_Error = 1;
/// `XML_PROP_ERROR_INVALID_TYPE`.
pub const XML_PROP_ERROR_INVALID_TYPE: XML_Prop_Error = 2;
/// `XML_PROP_ERROR_INVALID_VALUE`.
pub const XML_PROP_ERROR_INVALID_VALUE: XML_Prop_Error = 3;
/// `XML_PROP_ERROR_PARSER_NULL`.
pub const XML_PROP_ERROR_PARSER_NULL: XML_Prop_Error = 4;
/// `XML_PROP_ERROR_PARSER_NOT_ROOT`.
pub const XML_PROP_ERROR_PARSER_NOT_ROOT: XML_Prop_Error = 5;

/// `enum XML_Parser_Property`.
pub type XML_Parser_Property = c_int;
/// `XML_PROP_ALLOC_TRACKER_ACTIVATION_THRESHOLD` (`uint64_t`).
pub const XML_PROP_ALLOC_TRACKER_ACTIVATION_THRESHOLD: XML_Parser_Property = 1;
/// `XML_PROP_ALLOC_TRACKER_MAXIMUM_AMPLIFICATION` (`double`).
pub const XML_PROP_ALLOC_TRACKER_MAXIMUM_AMPLIFICATION: XML_Parser_Property = 2;
/// `XML_PROP_BILLION_LAUGHS_ACTIVATION_THRESHOLD` (`uint64_t`).
pub const XML_PROP_BILLION_LAUGHS_ACTIVATION_THRESHOLD: XML_Parser_Property = 3;
/// `XML_PROP_BILLION_LAUGHS_MAXIMUM_AMPLIFICATION` (`double`).
pub const XML_PROP_BILLION_LAUGHS_MAXIMUM_AMPLIFICATION: XML_Parser_Property = 4;
/// `XML_PROP_REPARSE_DEFERRAL_ENABLED` (`XML_Bool`).
pub const XML_PROP_REPARSE_DEFERRAL_ENABLED: XML_Parser_Property = 5;

/// The type a property has, if it is known.
fn property_type(property: XML_Parser_Property) -> Option<u8> {
    match property {
        XML_PROP_ALLOC_TRACKER_ACTIVATION_THRESHOLD
        | XML_PROP_BILLION_LAUGHS_ACTIVATION_THRESHOLD => Some(b'u'),
        XML_PROP_ALLOC_TRACKER_MAXIMUM_AMPLIFICATION
        | XML_PROP_BILLION_LAUGHS_MAXIMUM_AMPLIFICATION => Some(b'd'),
        XML_PROP_REPARSE_DEFERRAL_ENABLED => Some(b'b'),
        _ => None,
    }
}

/// `XML_INVALID_KEY` or `XML_INVALID_TYPE` for a property that is not of
/// type `want`, or `None` when it is.
fn property_check(property: XML_Parser_Property, want: u8) -> Option<XML_Prop_Error> {
    match property_type(property) {
        None => Some(XML_PROP_ERROR_INVALID_KEY),
        Some(t) if t != want => Some(XML_PROP_ERROR_INVALID_TYPE),
        Some(_) => None,
    }
}

/// `XML_SetPropertyBool`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetPropertyBool(
    parser: XML_Parser,
    property: XML_Parser_Property,
    value: XML_Bool,
) -> XML_Prop_Error {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return XML_PROP_ERROR_PARSER_NULL;
    };
    if let Some(e) = property_check(property, b'b') {
        return e;
    }
    s.set_reparse(value)
}

/// `XML_SetPropertyDouble`: the value is converted to `float`, as in
/// libexpat.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetPropertyDouble(
    parser: XML_Parser,
    property: XML_Parser_Property,
    value: f64,
) -> XML_Prop_Error {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return XML_PROP_ERROR_PARSER_NULL;
    };
    if let Some(e) = property_check(property, b'd') {
        return e;
    }
    // reason: C's `(float)value`.
    #[allow(clippy::cast_possible_truncation)]
    let f = value as f32;
    if property == XML_PROP_ALLOC_TRACKER_MAXIMUM_AMPLIFICATION {
        s.set_at_maximum(f)
    } else {
        s.set_bl_maximum(f)
    }
}

/// `XML_SetPropertyUInt64`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetPropertyUInt64(
    parser: XML_Parser,
    property: XML_Parser_Property,
    value: u64,
) -> XML_Prop_Error {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return XML_PROP_ERROR_PARSER_NULL;
    };
    if let Some(e) = property_check(property, b'u') {
        return e;
    }
    if property == XML_PROP_ALLOC_TRACKER_ACTIVATION_THRESHOLD {
        s.set_at_threshold(value)
    } else {
        s.set_bl_threshold(value)
    }
}

/// `XML_GetPropertyBool`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; `value` is `NULL` or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetPropertyBool(
    parser: XML_Parser,
    property: XML_Parser_Property,
    value: *mut XML_Bool,
) -> XML_Prop_Error {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return XML_PROP_ERROR_PARSER_NULL;
    };
    if value.is_null() {
        return XML_PROP_ERROR_INVALID_VALUE;
    }
    if let Some(e) = property_check(property, b'b') {
        return e;
    }
    // SAFETY: `value` is non-NULL and writable by the caller's contract.
    unsafe { value.write(XML_Bool::from(s.reparse_deferral.get())) };
    XML_PROP_ERROR_NONE
}

/// `XML_GetPropertyDouble`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; `value` is `NULL` or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetPropertyDouble(
    parser: XML_Parser,
    property: XML_Parser_Property,
    value: *mut f64,
) -> XML_Prop_Error {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return XML_PROP_ERROR_PARSER_NULL;
    };
    if value.is_null() {
        return XML_PROP_ERROR_INVALID_VALUE;
    }
    if let Some(e) = property_check(property, b'd') {
        return e;
    }
    let f = if property == XML_PROP_ALLOC_TRACKER_MAXIMUM_AMPLIFICATION {
        s.at_maximum.get()
    } else {
        s.bl_maximum.get()
    };
    // SAFETY: `value` is non-NULL and writable by the caller's contract.
    unsafe { value.write(f64::from(f)) };
    XML_PROP_ERROR_NONE
}

/// `XML_GetPropertyUInt64`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; `value` is `NULL` or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetPropertyUInt64(
    parser: XML_Parser,
    property: XML_Parser_Property,
    value: *mut u64,
) -> XML_Prop_Error {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return XML_PROP_ERROR_PARSER_NULL;
    };
    if value.is_null() {
        return XML_PROP_ERROR_INVALID_VALUE;
    }
    if let Some(e) = property_check(property, b'u') {
        return e;
    }
    let v = if property == XML_PROP_ALLOC_TRACKER_ACTIVATION_THRESHOLD {
        s.at_threshold.get()
    } else {
        s.bl_threshold.get()
    };
    // SAFETY: `value` is non-NULL and writable by the caller's contract.
    unsafe { value.write(v) };
    XML_PROP_ERROR_NONE
}

/// `XML_SetBillionLaughsAttackProtectionMaximumAmplification`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetBillionLaughsAttackProtectionMaximumAmplification(
    parser: XML_Parser,
    factor: f32,
) -> XML_Bool {
    // SAFETY: forwarded from this function's contract.
    unsafe { state(parser) }.map_or(0, |s| {
        XML_Bool::from(s.set_bl_maximum(factor) == XML_PROP_ERROR_NONE)
    })
}

/// `XML_SetBillionLaughsAttackProtectionActivationThreshold`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetBillionLaughsAttackProtectionActivationThreshold(
    parser: XML_Parser,
    bytes: u64,
) -> XML_Bool {
    // SAFETY: forwarded from this function's contract.
    unsafe { state(parser) }.map_or(0, |s| {
        XML_Bool::from(s.set_bl_threshold(bytes) == XML_PROP_ERROR_NONE)
    })
}

/// `XML_SetAllocTrackerMaximumAmplification`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetAllocTrackerMaximumAmplification(
    parser: XML_Parser,
    factor: f32,
) -> XML_Bool {
    // SAFETY: forwarded from this function's contract.
    unsafe { state(parser) }.map_or(0, |s| {
        XML_Bool::from(s.set_at_maximum(factor) == XML_PROP_ERROR_NONE)
    })
}

/// `XML_SetAllocTrackerActivationThreshold`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetAllocTrackerActivationThreshold(
    parser: XML_Parser,
    bytes: u64,
) -> XML_Bool {
    // SAFETY: forwarded from this function's contract.
    unsafe { state(parser) }.map_or(0, |s| {
        XML_Bool::from(s.set_at_threshold(bytes) == XML_PROP_ERROR_NONE)
    })
}

/// `XML_SetReparseDeferralEnabled`: `XML_FALSE` for a value other than
/// `XML_TRUE` or `XML_FALSE`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetReparseDeferralEnabled(
    parser: XML_Parser,
    enabled: XML_Bool,
) -> XML_Bool {
    // SAFETY: forwarded from this function's contract.
    unsafe { state(parser) }.map_or(0, |s| {
        XML_Bool::from(s.set_reparse(enabled) == XML_PROP_ERROR_NONE)
    })
}

/// `XML_SetHashSalt`: the salt `{0, hash_salt}`; 0 leaves the salt to be
/// chosen at random unless one was set before, as in libexpat. 0 while
/// the parser is busy.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetHashSalt(parser: XML_Parser, hash_salt: c_ulong) -> c_int {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return 0;
    };
    if s.parser_busy() {
        return 0;
    }
    if hash_salt != 0 {
        s.salt_set.set(true);
    }
    if s.salt_set.get() {
        let mut key = [0; 16];
        // reason: `unsigned long` is 32 bits on some targets.
        #[allow(clippy::useless_conversion)]
        let k1 = u64::from(hash_salt);
        key[8..].copy_from_slice(&k1.to_le_bytes());
        let _ = s.with_parser(|p| p.set_salt(key));
    }
    1
}

/// `XML_SetHashSalt16Bytes`. `XML_FALSE` for `NULL` entropy or while the
/// parser is busy.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; `entropy` is `NULL` or points to
/// 16 readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetHashSalt16Bytes(
    parser: XML_Parser,
    entropy: *const u8,
) -> XML_Bool {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return 0;
    };
    if entropy.is_null() || s.parser_busy() {
        return 0;
    }
    let mut key = [0; 16];
    // SAFETY: `entropy` points to 16 readable bytes by the caller's
    // contract.
    key.copy_from_slice(unsafe { std::slice::from_raw_parts(entropy, 16) });
    s.salt_set.set(true);
    let _ = s.with_parser(|p| p.set_salt(key));
    1
}

/// `XML_SetEncoding`: the encoding to read the document in, overriding
/// its declaration, or none for `NULL`. `XML_STATUS_ERROR` while the
/// parser is busy or when the copy of the name fails.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; `encoding` is `NULL` or a
/// NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetEncoding(
    parser: XML_Parser,
    encoding: *const XML_Char,
) -> XML_Status {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return XML_STATUS_ERROR;
    };
    if s.parser_busy() {
        return XML_STATUS_ERROR;
    }
    // SAFETY: forwarded from this function's contract.
    let name = unsafe { c_bytes(encoding) };
    match s.with_parser(|p| p.set_encoding(name)) {
        Some(false) => XML_STATUS_ERROR,
        _ => XML_STATUS_OK,
    }
}

/// `XML_SetReturnNSTriplet`. No effect while the parser is busy.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetReturnNSTriplet(parser: XML_Parser, do_nst: c_int) {
    // SAFETY: forwarded from this function's contract.
    if let Some(s) = unsafe { state(parser) } {
        let _ = s.with_parser(|p| p.set_return_ns_triplet(do_nst != 0));
    }
}

/// `XML_UseForeignDTD`: `XML_ERROR_CANT_CHANGE_FEATURE_ONCE_PARSING`
/// while the parser is busy, `XML_ERROR_INVALID_ARGUMENT` for `NULL`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_UseForeignDTD(parser: XML_Parser, use_dtd: XML_Bool) -> XML_Error {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return error(ErrorCode::InvalidArgument);
    };
    if s.parser_busy() {
        return error(ErrorCode::CantChangeFeatureOnceParsing);
    }
    match s.with_parser(|p| p.use_foreign_dtd(use_dtd != 0)) {
        Some(Err(e)) => error(e),
        _ => XML_ERROR_NONE,
    }
}

/// `enum XML_ParamEntityParsing`.
pub type XML_ParamEntityParsing = c_int;

/// `XML_SetParamEntityParsing`: 0 while the parser is busy.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_SetParamEntityParsing(
    parser: XML_Parser,
    parsing: XML_ParamEntityParsing,
) -> c_int {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return 0;
    };
    if s.parser_busy() {
        return 0;
    }
    // libexpat stores any value; a value outside the enumeration behaves
    // as a non-zero mode other than `XML_PARAM_ENTITY_PARSING_UNLESS_STANDALONE`.
    let mode = match parsing {
        0 => ParamEntityParsing::Never,
        1 => ParamEntityParsing::UnlessStandalone,
        _ => ParamEntityParsing::Always,
    };
    let _ = s.with_parser(|p| p.set_param_entity_parsing(mode));
    1
}

/// `XML_ParsingStatus`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XML_ParsingStatus {
    /// `parsing` (`enum XML_Parsing`: `XML_INITIALIZED` 0,
    /// `XML_PARSING` 1, `XML_FINISHED` 2, `XML_SUSPENDED` 3).
    pub parsing: c_int,
    /// `finalBuffer`
    pub finalBuffer: XML_Bool,
}

/// `XML_GetParsingStatus`.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; `status` is `NULL` or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_GetParsingStatus(parser: XML_Parser, status: *mut XML_ParsingStatus) {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return;
    };
    if status.is_null() {
        return;
    }
    let parsing = match s.stopper.parsing_status() {
        ParsingStatus::Initialized => 0,
        ParsingStatus::Parsing => 1,
        ParsingStatus::Finished => 2,
        ParsingStatus::Suspended => 3,
    };
    // SAFETY: `status` is non-NULL and writable by the caller's contract.
    unsafe {
        status.write(XML_ParsingStatus {
            parsing,
            finalBuffer: XML_Bool::from(s.stopper.is_final_buffer()),
        });
    }
}

/// `XML_ParserReset`: the parser as `XML_ParserCreate(encoding)` (or
/// `XML_ParserCreateNS`) leaves it, keeping what libexpat keeps (the
/// namespace setting, `XML_SetReturnNSTriplet`, the amplification limits
/// and the allocation functions). `XML_FALSE` for `NULL` or from inside a
/// handler. Content models already passed stay the program's.
///
/// # Safety
///
/// `parser` is `NULL` or a live handle; `encoding` is `NULL` or a
/// NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_ParserReset(
    parser: XML_Parser,
    encoding: *const XML_Char,
) -> XML_Bool {
    // SAFETY: forwarded from this function's contract.
    let Some(s) = (unsafe { state(parser) }) else {
        return 0;
    };
    // SAFETY: forwarded from this function's contract.
    let name = unsafe { c_bytes(encoding) };
    if s.busy.get() || !s.parent.get().is_null() || s.closed() {
        return 0;
    }
    s.run_encoding_release();
    if s.with_parser(|p| p.reset(name, salt())) != Some(true) {
        return 0;
    }
    #[cfg(feature = "testing")]
    {
        let on = testing::reparse_deferral_default();
        s.reparse_deferral.set(on);
        let _ = s.with_parser(|p| p.set_reparse_deferral_enabled(on));
    }
    s.user_data.set(std::ptr::null_mut());
    s.handler_arg.set(std::ptr::null_mut());
    s.error_code.set(XML_ERROR_NONE);
    s.specified.set(0);
    s.event_position.set((0, 0, -1));
    s.event_input.set((std::ptr::null(), 0, None, 0));
    s.id_index.set(-1);
    s.start_element.set(None);
    s.end_element.set(None);
    s.character_data.set(None);
    s.processing_instruction.set(None);
    s.comment.set(None);
    s.start_cdata_section.set(None);
    s.end_cdata_section.set(None);
    s.start_namespace_decl.set(None);
    s.end_namespace_decl.set(None);
    s.default.set(None);
    s.xml_decl.set(None);
    s.start_doctype_decl.set(None);
    s.end_doctype_decl.set(None);
    s.entity_decl.set(None);
    s.unparsed_entity_decl.set(None);
    s.notation_decl.set(None);
    s.element_decl.set(None);
    s.attlist_decl.set(None);
    s.skipped_entity.set(None);
    s.external_entity_ref.set(None);
    s.external_entity_ref_arg.set(std::ptr::null_mut());
    s.not_standalone.set(None);
    *s.base.borrow_mut() = None;
    *s.pending_base.borrow_mut() = None;
    s.default_expand.set(true);
    {
        let mut current = s.current.borrow_mut();
        current.0.clear();
        current.1.clear();
    }
    s.salt_set.set(false);
    s.reparse_deferral.set(true);
    1
}

/// `XML_ParserCreate_MM`: a parser whose memory handed to the program
/// (content models, `XML_MemMalloc`, `XML_MemRealloc`, `XML_MemFree`)
/// comes from `memsuite` (the C library's functions for `NULL`), with
/// namespace processing when `namespace_separator` is not `NULL`. With a
/// suite, every call libexpat makes to it is made too, with libexpat's
/// sizes, and a failing call fails the parser where libexpat fails
/// (D-060); the parser's data itself lives in the Rust allocator. `NULL`
/// when a suite lacks one of its functions or a creation call fails.
///
/// # Safety
///
/// `encoding` and `namespace_separator` are `NULL` or NUL-terminated
/// strings; `memsuite` is `NULL` or points to a suite whose functions
/// behave as `malloc`, `realloc` and `free` do.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XML_ParserCreate_MM(
    encoding: *const XML_Char,
    memsuite: *const XML_Memory_Handling_Suite,
    namespace_separator: *const XML_Char,
) -> XML_Parser {
    let mem = if memsuite.is_null() {
        Mem::default()
    } else {
        // SAFETY: `memsuite` is non-NULL and readable by the caller's
        // contract.
        let m = unsafe { *memsuite };
        match (m.malloc_fcn, m.realloc_fcn, m.free_fcn) {
            (Some(malloc), Some(realloc), Some(free)) => Mem {
                malloc,
                realloc,
                free,
            },
            _ => return std::ptr::null_mut(),
        }
    };
    let mut options = Options::new();
    if !namespace_separator.is_null() {
        // SAFETY: non-NULL, so it points to at least its first character.
        let sep = unsafe { *namespace_separator };
        options = options.namespace_separator(sep.to_ne_bytes()[0]);
    }
    // SAFETY: forwarded from this function's contract.
    if let Some(name) = unsafe { c_bytes(encoding) } {
        let Some(o) = options.try_encoding(name) else {
            return std::ptr::null_mut();
        };
        options = o;
    }
    if memsuite.is_null() {
        return create(options);
    }
    let Some(suite) = try_arc(mem::CSuite::new(mem)) else {
        return std::ptr::null_mut();
    };
    let Some(parser) = Parser::with_memory_suite(salt(), options, suite.clone()) else {
        return std::ptr::null_mut();
    };
    let Some(state) = XML_ParserStruct::own(parser) else {
        return std::ptr::null_mut();
    };
    state.mem.set(mem);
    state.suite.replace(Some(suite));
    into_handle(state)
}

/// `XML_FeatureEnum`.
pub type XML_FeatureEnum = c_int;

/// `XML_Feature`.
#[repr(C)]
#[derive(Debug)]
pub struct XML_Feature {
    /// `feature`
    pub feature: XML_FeatureEnum,
    /// `name`
    pub name: *const XML_Char,
    /// `value`
    pub value: c_long,
}

/// The feature list, in a static of its own so that the pointers to the
/// names can be shared between threads.
struct Features([XML_Feature; 11]);

// SAFETY: the list is never written, and the names it points to are
// static string literals.
unsafe impl Sync for Features {}

static FEATURES: Features = Features([
    XML_Feature {
        feature: 6,
        name: c"sizeof(XML_Char)".as_ptr(),
        value: 1,
    },
    XML_Feature {
        feature: 7,
        name: c"sizeof(XML_LChar)".as_ptr(),
        value: 1,
    },
    XML_Feature {
        feature: 3,
        name: c"XML_DTD".as_ptr(),
        value: 0,
    },
    XML_Feature {
        feature: 4,
        name: c"XML_CONTEXT_BYTES".as_ptr(),
        value: 1024,
    },
    XML_Feature {
        feature: 8,
        name: c"XML_NS".as_ptr(),
        value: 0,
    },
    XML_Feature {
        feature: 11,
        name: c"XML_BLAP_MAX_AMP".as_ptr(),
        value: 100,
    },
    XML_Feature {
        feature: 12,
        name: c"XML_BLAP_ACT_THRES".as_ptr(),
        value: 8_388_608,
    },
    XML_Feature {
        feature: 13,
        name: c"XML_GE".as_ptr(),
        value: 0,
    },
    XML_Feature {
        feature: 14,
        name: c"XML_AT_MAX_AMP".as_ptr(),
        value: 100,
    },
    XML_Feature {
        feature: 15,
        name: c"XML_AT_ACT_THRES".as_ptr(),
        value: 67_108_864,
    },
    XML_Feature {
        feature: 0,
        name: std::ptr::null(),
        value: 0,
    },
]);

/// `XML_GetFeatureList`: the features of libexpat 2.10.0 built with
/// `XML_DTD`, `XML_NS`, `XML_GE` and `XML_CONTEXT_BYTES` 1024, ending with
/// `XML_FEATURE_END`.
#[unsafe(no_mangle)]
pub extern "C" fn XML_GetFeatureList() -> *const XML_Feature {
    FEATURES.0.as_ptr()
}

/// `Arc::new(value)`, or `None` when a block of the `Arc`'s size cannot
/// be reserved first (D-067; stable Rust has no fallible `Arc`
/// constructor).
fn try_arc<T>(value: T) -> Option<std::sync::Arc<T>> {
    let counts = std::alloc::Layout::new::<[std::sync::atomic::AtomicUsize; 2]>();
    let size = counts
        .extend(std::alloc::Layout::new::<T>())
        .ok()?
        .0
        .pad_to_align()
        .size();
    let mut probe: Vec<u8> = Vec::new();
    probe.try_reserve_exact(size).ok()?;
    drop(probe);
    Some(std::sync::Arc::new(value))
}

/// `Box::new(value)`, or `None` when a block of its size cannot be
/// reserved first (D-067; stable Rust has no fallible `Box`
/// constructor).
fn try_box<T>(value: T) -> Option<Box<T>> {
    if std::mem::size_of::<T>() > 0 {
        let mut probe: Vec<u8> = Vec::new();
        probe.try_reserve_exact(std::mem::size_of::<T>()).ok()?;
    }
    Some(Box::new(value))
}
