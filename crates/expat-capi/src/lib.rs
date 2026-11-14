//! A libexpat-compatible C interface (`expat.h`) over the `expat` crate
//! (module 13, slice 1, D-047).
//!
//! The library exports the functions below under their libexpat names, so
//! a C program written against the vendored `expat.h` (libexpat 2.10.0,
//! `XML_Char` = `char`, UTF-8 output) can link against it instead of
//! libexpat. This first slice covers parser creation and release, user
//! data, the nine handlers of [`expat::Events`], `XML_Parse`,
//! `XML_GetBuffer` and `XML_ParseBuffer` (D-049), `XML_StopParser` and
//! `XML_ResumeParser` (D-050), `XML_SetDefaultHandler` and
//! `XML_SetDefaultHandlerExpand` (D-051), `XML_DefaultCurrent` (D-052),
//! the error code and its
//! text, the position between calls and inside handlers (D-048) and the
//! version. Every
//! other `expat.h` function is missing, so a program that uses one fails
//! to link rather than run with different behaviour.
//!
//! Differences from libexpat in this slice:
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
//! - `XML_GetSpecifiedAttributeCount` is updated only while a start
//!   element handler is set, which is where libexpat documents its use.
//! - `XML_Parse`, `XML_ParseBuffer`, `XML_GetBuffer`, `XML_ResumeParser`
//!   and `XML_ParserFree` called from inside a handler do nothing (`XML_STATUS_ERROR` or
//!   `NULL`, without an error code), as in libexpat 2.10.0.
//! - `XML_DefaultCurrent` works only if a default handler was set when
//!   `XML_Parse` started. Inside the default handler it calls, for an
//!   event of a document not in UTF-8 whose text takes more than one
//!   1024-byte piece, the position functions return the position of the
//!   event rather than of each piece.
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

use std::cell::{Cell, RefCell, UnsafeCell};
use std::ffi::{CStr, CString, c_char, c_int, c_long, c_ulong, c_void};
use std::hash::BuildHasher;
use std::sync::OnceLock;

use expat::{ErrorCode, Events, Handler, Options, Parser, Stopper};

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
    error_code: Cell<XML_Error>,
    specified: Cell<c_int>,
    /// The position of the event being reported (line from 1, column,
    /// byte index), for the position functions inside a handler.
    event_position: Cell<(u64, u64, i64)>,
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

/// The state only the parsing, buffer and position functions use.
struct Inner {
    parser: Parser,
    /// NUL-terminated copies of the strings passed to a handler.
    text: Vec<u8>,
    /// The `atts` array of a start element handler.
    atts: Vec<*const XML_Char>,
}

impl XML_ParserStruct {
    fn new(options: Options) -> Self {
        let parser = Parser::with_options(salt(), options);
        XML_ParserStruct {
            user_data: Cell::new(std::ptr::null_mut()),
            handler_arg: Cell::new(std::ptr::null_mut()),
            busy: Cell::new(false),
            error_code: Cell::new(XML_ERROR_NONE),
            specified: Cell::new(0),
            event_position: Cell::new((0, 0, -1)),
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
            default_expand: Cell::new(true),
            current: RefCell::new((Vec::new(), Vec::new())),
            stopper: parser.stopper(),
            inner: UnsafeCell::new(Inner {
                parser,
                text: Vec::new(),
                atts: Vec::new(),
            }),
        }
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
            .default_data(self.default.get().is_some())
            .expand_internal_entities(self.default_expand.get())
            .event_text(self.default.get().is_some());
        events.position(events != Events::NONE)
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
        Some(f(&mut inner.parser))
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
    fn position(&mut self, line: u64, column: u64, byte_index: i64) {
        self.state.event_position.set((line, column, byte_index));
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
    Box::into_raw(Box::new(XML_ParserStruct::new(options)))
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
        options = options.encoding(name);
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
        options = options.encoding(name);
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
        Some(s) if !s.busy.get() => {}
        _ => return,
    }
    // SAFETY: the handle came from `Box::into_raw` in `create`, is live and
    // no handler is running, so no reference to it is left.
    drop(unsafe { Box::from_raw(parser) });
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
    let events = state.events();
    state.busy.set(true);
    // SAFETY: the callers checked that `busy` was false, so nothing else
    // borrows `inner`, and it is true until this borrow ends, so a handler
    // calling back into the library cannot borrow it (`with_parser` and
    // the parsing functions check `busy`); handlers only reach the `Cell`
    // fields.
    let inner = unsafe { &mut *state.inner.get() };
    let Inner { parser, text, atts } = inner;
    let mut dispatch = Dispatch { state, text, atts };
    let result = f(parser, &mut dispatch, events);
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
