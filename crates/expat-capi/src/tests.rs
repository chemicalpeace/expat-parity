//! Calls through the C interface from Rust: handler wiring, user data,
//! calls from inside a handler, argument checks. The comparison with
//! libexpat itself is `scripts/capi-check.sh` (a C program built once
//! against libexpat and once against this library).

// reason: a panic is the assertion mechanism in tests (D-011).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

/// Events recorded by the test handlers, reached through the user data.
#[derive(Default)]
struct Log {
    lines: Vec<String>,
    parser: XML_Parser,
    nested_parse: Option<XML_Status>,
}

fn text(s: *const XML_Char) -> String {
    if s.is_null() {
        return "NULL".into();
    }
    // SAFETY: the library passes NUL-terminated strings or NULL.
    unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned()
}

/// # Safety
///
/// `arg` is the `Log` set as user data.
unsafe fn log<'a>(arg: *mut c_void) -> &'a mut Log {
    // SAFETY: every test sets a live `Log` as the handler argument.
    unsafe { &mut *arg.cast::<Log>() }
}

unsafe extern "C" fn on_start(arg: *mut c_void, name: *const XML_Char, atts: *mut *const XML_Char) {
    // SAFETY: test contract.
    let log = unsafe { log(arg) };
    let mut line = format!("start {}", text(name));
    let mut i = 0;
    loop {
        // SAFETY: `atts` is NULL-terminated.
        let a = unsafe { *atts.add(i) };
        if a.is_null() {
            break;
        }
        line.push(' ');
        line.push_str(&text(a));
        i += 1;
    }
    // SAFETY: the parser is live during its own parse.
    let n = unsafe { XML_GetSpecifiedAttributeCount(log.parser) };
    line.push_str(&format!(" [{n}]"));
    log.lines.push(line);
}

unsafe extern "C" fn on_end(arg: *mut c_void, name: *const XML_Char) {
    // SAFETY: test contract.
    unsafe { log(arg) }
        .lines
        .push(format!("end {}", text(name)));
}

unsafe extern "C" fn on_chars(arg: *mut c_void, s: *const XML_Char, len: c_int) {
    // SAFETY: `s` is valid for `len` bytes.
    let bytes = unsafe { std::slice::from_raw_parts(s.cast::<u8>(), len as usize) };
    // SAFETY: test contract.
    let log = unsafe { log(arg) };
    let t = String::from_utf8_lossy(bytes);
    match log.lines.last_mut() {
        Some(l) if l.starts_with("chars ") => l.push_str(&t),
        _ => log.lines.push(format!("chars {t}")),
    }
}

unsafe extern "C" fn on_pi(arg: *mut c_void, target: *const XML_Char, data: *const XML_Char) {
    // SAFETY: test contract.
    unsafe { log(arg) }
        .lines
        .push(format!("pi {} {}", text(target), text(data)));
}

unsafe extern "C" fn on_comment(arg: *mut c_void, data: *const XML_Char) {
    // SAFETY: test contract.
    unsafe { log(arg) }
        .lines
        .push(format!("comment {}", text(data)));
}

unsafe extern "C" fn on_cdata_start(arg: *mut c_void) {
    // SAFETY: test contract.
    unsafe { log(arg) }.lines.push("cdata(".into());
}

unsafe extern "C" fn on_cdata_end(arg: *mut c_void) {
    // SAFETY: test contract.
    unsafe { log(arg) }.lines.push(")cdata".into());
}

unsafe extern "C" fn on_ns_start(arg: *mut c_void, prefix: *const XML_Char, uri: *const XML_Char) {
    // SAFETY: test contract.
    unsafe { log(arg) }
        .lines
        .push(format!("ns {} {}", text(prefix), text(uri)));
}

unsafe extern "C" fn on_ns_end(arg: *mut c_void, prefix: *const XML_Char) {
    // SAFETY: test contract.
    unsafe { log(arg) }
        .lines
        .push(format!("/ns {}", text(prefix)));
}

/// Tries to parse and to free the parser from inside a handler.
unsafe extern "C" fn on_start_nested(
    arg: *mut c_void,
    _: *const XML_Char,
    _: *mut *const XML_Char,
) {
    // SAFETY: test contract.
    let log = unsafe { log(arg) };
    // SAFETY: the parser is live; both calls are refused while it runs.
    unsafe {
        log.nested_parse = Some(XML_Parse(log.parser, c"<x/>".as_ptr(), 4, 1));
        XML_ParserFree(log.parser);
        log.lines.push(format!(
            "inside line={} col={} byte={}",
            XML_GetCurrentLineNumber64(log.parser),
            XML_GetCurrentColumnNumber64(log.parser),
            XML_GetCurrentByteIndex64(log.parser)
        ));
    }
}

fn all_handlers(p: XML_Parser) {
    // SAFETY: `p` is live; the handlers have the expat.h types.
    unsafe {
        XML_SetElementHandler(p, Some(on_start), Some(on_end));
        XML_SetCharacterDataHandler(p, Some(on_chars));
        XML_SetProcessingInstructionHandler(p, Some(on_pi));
        XML_SetCommentHandler(p, Some(on_comment));
        XML_SetCdataSectionHandler(p, Some(on_cdata_start), Some(on_cdata_end));
        XML_SetNamespaceDeclHandler(p, Some(on_ns_start), Some(on_ns_end));
    }
}

fn parse(p: XML_Parser, doc: &[u8], is_final: bool) -> XML_Status {
    let len = c_int::try_from(doc.len()).expect("test documents are small");
    // SAFETY: `p` is live and `doc` is valid for `len` bytes.
    unsafe { XML_Parse(p, doc.as_ptr().cast(), len, c_int::from(is_final)) }
}

#[test]
fn events_reach_the_c_handlers() {
    // SAFETY: NULL selects the default encoding.
    let p = unsafe { XML_ParserCreateNS(std::ptr::null(), b'|' as c_char) };
    let mut log = Log {
        parser: p,
        ..Log::default()
    };
    // SAFETY: `p` is live and `log` outlives the parse.
    unsafe { XML_SetUserData(p, (&raw mut log).cast()) };
    all_handlers(p);
    let doc = b"<?go now?><r xmlns:a='urn:a' a:k='v' n='1'><!--c-->t<![CDATA[x]]></r>";
    assert_eq!(parse(p, doc, true), XML_STATUS_OK);
    assert_eq!(
        log.lines,
        [
            "pi go now",
            "ns a urn:a",
            "start r urn:a|k v n 1 [4]",
            "comment c",
            "chars t",
            "cdata(",
            "chars x",
            ")cdata",
            "end r",
            "/ns a",
        ]
    );
    // SAFETY: `p` is live; the user data macro reads the first field.
    unsafe {
        assert_eq!(*(p as *const *mut c_void), (&raw mut log).cast());
        assert_eq!(XML_GetCurrentLineNumber(p), 1);
        XML_ParserFree(p);
    }
}

#[test]
fn buffer_pieces_reach_the_c_handlers() {
    // SAFETY: NULL selects the default encoding.
    let p = unsafe { XML_ParserCreate(std::ptr::null()) };
    let mut log = Log {
        parser: p,
        ..Log::default()
    };
    // SAFETY: `p` is live and `log` outlives the parse.
    unsafe { XML_SetUserData(p, (&raw mut log).cast()) };
    all_handlers(p);
    let doc = b"<r><!--c-->text<![CDATA[x]]></r>";
    for (i, piece) in doc.chunks(5).enumerate() {
        let len = c_int::try_from(piece.len()).unwrap();
        let last = c_int::from((i + 1) * 5 >= doc.len());
        // SAFETY: `p` is live and not inside a handler.
        let space = unsafe { XML_GetBuffer(p, len) }.cast::<u8>();
        assert!(!space.is_null());
        // SAFETY: the space holds `len` bytes until the next call on `p`.
        unsafe { std::ptr::copy_nonoverlapping(piece.as_ptr(), space, piece.len()) };
        // SAFETY: `p` is live.
        assert_eq!(unsafe { XML_ParseBuffer(p, len, last) }, XML_STATUS_OK);
    }
    assert_eq!(log.lines.first().map(String::as_str), Some("start r [0]"));
    assert_eq!(log.lines.last().map(String::as_str), Some("end r"));
    // SAFETY: `p` is live; a NULL handle is accepted.
    unsafe {
        assert!(XML_GetBuffer(p, 1).is_null());
        assert_eq!(XML_GetErrorCode(p), error(ErrorCode::Finished));
        assert!(XML_GetBuffer(std::ptr::null_mut(), 1).is_null());
        assert_eq!(
            XML_ParseBuffer(std::ptr::null_mut(), 0, 1),
            XML_STATUS_ERROR
        );
        XML_ParserFree(p);
    }
}

unsafe extern "C" fn on_start_suspend(
    arg: *mut c_void,
    name: *const XML_Char,
    _atts: *mut *const XML_Char,
) {
    // SAFETY: test contract.
    let log = unsafe { log(arg) };
    log.lines.push(format!("start {}", text(name)));
    // SAFETY: the parser is live during its own parse.
    unsafe {
        assert_eq!(XML_StopParser(log.parser, 1), XML_STATUS_OK);
        assert_eq!(XML_ResumeParser(log.parser), XML_STATUS_ERROR);
    }
}

#[test]
fn stop_and_resume_through_the_c_functions() {
    // SAFETY: NULL selects the default encoding.
    let p = unsafe { XML_ParserCreate(std::ptr::null()) };
    let mut log = Log {
        parser: p,
        ..Log::default()
    };
    // SAFETY: `p` is live and `log` outlives the parse; a NULL handle is
    // accepted.
    unsafe {
        XML_SetUserData(p, (&raw mut log).cast());
        XML_SetStartElementHandler(p, Some(on_start_suspend));
        assert_eq!(XML_StopParser(p, 1), XML_STATUS_ERROR);
        assert_eq!(XML_GetErrorCode(p), error(ErrorCode::NotStarted));
        let doc = b"<a><b/></a>";
        assert_eq!(
            XML_Parse(p, doc.as_ptr().cast(), 11, 1),
            XML_STATUS_SUSPENDED
        );
        assert_eq!(XML_ResumeParser(p), XML_STATUS_SUSPENDED);
        assert_eq!(XML_ResumeParser(p), XML_STATUS_OK);
        assert_eq!(XML_ResumeParser(p), XML_STATUS_ERROR);
        assert_eq!(XML_GetErrorCode(p), error(ErrorCode::NotSuspended));
        assert_eq!(XML_StopParser(std::ptr::null_mut(), 0), XML_STATUS_ERROR);
        assert_eq!(XML_ResumeParser(std::ptr::null_mut()), XML_STATUS_ERROR);
        XML_ParserFree(p);
    }
    assert_eq!(log.lines, ["start a", "start b"]);
}

#[test]
fn errors_and_positions_match_the_rust_api() {
    // SAFETY: NULL selects the default encoding.
    let p = unsafe { XML_ParserCreate(std::ptr::null()) };
    assert_eq!(parse(p, b"<a>\n  <b></c>", true), XML_STATUS_ERROR);
    let mut q = Parser::new([0; 16]);
    let e = q.parse(b"<a>\n  <b></c>", true).unwrap_err();
    // SAFETY: `p` is live.
    unsafe {
        assert_eq!(XML_GetErrorCode(p), error(e.code()));
        assert_eq!(XML_GetCurrentLineNumber64(p), e.line());
        assert_eq!(XML_GetCurrentColumnNumber64(p), e.column());
        assert_eq!(XML_GetCurrentByteIndex64(p), e.byte_index());
        assert_eq!(text(XML_ErrorString(XML_GetErrorCode(p))), "mismatched tag");
        XML_ParserFree(p);
    }
}

#[test]
fn calls_from_inside_a_handler_are_refused_and_positions_answered() {
    // SAFETY: NULL selects the default encoding.
    let p = unsafe { XML_ParserCreate(std::ptr::null()) };
    let mut log = Log {
        parser: p,
        ..Log::default()
    };
    // SAFETY: `p` is live and `log` outlives the parse.
    unsafe {
        XML_SetUserData(p, (&raw mut log).cast());
        XML_SetStartElementHandler(p, Some(on_start_nested));
    }
    assert_eq!(parse(p, b"<a/>", true), XML_STATUS_OK);
    assert_eq!(log.nested_parse, Some(XML_STATUS_ERROR));
    // The position functions answer with the position of the event.
    assert_eq!(log.lines, ["inside line=1 col=0 byte=0"]);
    // SAFETY: the parser survived the refused free.
    unsafe {
        assert_eq!(XML_GetErrorCode(p), XML_ERROR_NONE);
        XML_ParserFree(p);
    }
}

#[test]
fn parser_as_handler_argument() {
    unsafe extern "C" fn on_end_parser(arg: *mut c_void, _: *const XML_Char) {
        // SAFETY: the argument is the parser, whose user data is a `Log`.
        unsafe { log(XML_GetUserDataForTest(arg.cast())) }
            .lines
            .push("end via parser".into());
    }
    /// `XML_GetUserData` (a macro in expat.h).
    unsafe fn XML_GetUserDataForTest(p: XML_Parser) -> *mut c_void {
        // SAFETY: `user_data` is the first field of the handle.
        unsafe { *(p as *const *mut c_void) }
    }
    // SAFETY: NULL selects the default encoding.
    let p = unsafe { XML_ParserCreate(std::ptr::null()) };
    let mut log = Log::default();
    // SAFETY: `p` is live and `log` outlives the parse.
    unsafe {
        XML_SetUserData(p, (&raw mut log).cast());
        XML_UseParserAsHandlerArg(p);
        XML_SetEndElementHandler(p, Some(on_end_parser));
    }
    assert_eq!(parse(p, b"<a/>", true), XML_STATUS_OK);
    assert_eq!(log.lines, ["end via parser"]);
    // SAFETY: `p` is live.
    unsafe { XML_ParserFree(p) };
}

#[test]
fn argument_checks() {
    // SAFETY: NULL handles and NULL data are accepted by contract.
    unsafe {
        let null = std::ptr::null_mut();
        assert_eq!(XML_Parse(null, std::ptr::null(), 0, 1), XML_STATUS_ERROR);
        assert_eq!(XML_GetErrorCode(null), error(ErrorCode::InvalidArgument));
        assert_eq!(XML_GetSpecifiedAttributeCount(null), -1);
        assert_eq!(XML_GetCurrentByteIndex(null), -1);
        XML_ParserFree(null);

        let p = XML_ParserCreate(std::ptr::null());
        assert_eq!(XML_Parse(p, std::ptr::null(), 1, 0), XML_STATUS_ERROR);
        assert_eq!(XML_GetErrorCode(p), error(ErrorCode::InvalidArgument));
        assert_eq!(XML_Parse(p, c"<a/>".as_ptr(), -1, 0), XML_STATUS_ERROR);
        // The parser itself is unaffected, as in libexpat.
        assert_eq!(XML_Parse(p, std::ptr::null(), 0, 0), XML_STATUS_OK);
        assert_eq!(XML_Parse(p, c"<a/>".as_ptr(), 4, 1), XML_STATUS_OK);
        XML_ParserFree(p);
    }
}

#[test]
fn error_strings_and_version() {
    assert!(XML_ErrorString(XML_ERROR_NONE).is_null());
    assert!(XML_ErrorString(1000).is_null());
    for code in 1..=64u32 {
        if let Some(c) = ErrorCode::from_c(code) {
            assert_eq!(text(XML_ErrorString(error(c))), c.description());
        }
    }
    assert_eq!(text(XML_ExpatVersion()), "expat_2.10.0");
    assert_eq!(
        XML_ExpatVersionInfo(),
        XML_Expat_Version {
            major: 2,
            minor: 10,
            micro: 0
        }
    );
}
