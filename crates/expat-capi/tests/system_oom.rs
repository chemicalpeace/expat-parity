//! Failures of the system allocator (D-061).
//!
//! The test lives in this crate because replacing the global allocator
//! takes `unsafe`, which the `expat` crate forbids; it uses only the
//! public API of `expat`.
//!
//! The global allocator of this test binary fails from the n-th
//! allocation (or growing reallocation) on, once armed. For every n up to
//! the number of allocations a document needs, a child process parses the
//! document with the allocator armed after the parser is created, and must
//! end normally: the parse returns `Ok(())` or fails with
//! `ErrorCode::NoMemory`, and the process does not abort.
//!
//! A second test does the same for creating, configuring and resetting
//! parsers through the C interface (D-065): `XML_ParserCreateNS`,
//! `XML_SetEncoding`, `XML_SetBase`, a parse, `XML_ParserReset`,
//! `XML_ExternalEntityParserCreate` and `XML_ParserFree`, with the
//! allocator armed from the start. Each call may fail as libexpat's
//! does (`NULL`, `XML_STATUS_ERROR`, `XML_FALSE`); the process must not
//! abort.
//!
//! Two layouts are exempt, since stable Rust has no fallible `Arc`
//! constructor: the reference-count block of an `Arc<Vec<u8>>` (shared
//! entity text and the input buffer: the counts and the vector header)
//! and that of the parser's shared control. Allocations of exactly these
//! layouts are neither counted nor refused.

// Miri cannot spawn the child processes.
#![cfg(not(miri))]
// reason: a panic is the assertion mechanism in tests (D-011).
#![allow(clippy::expect_used)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use expat::{ErrorCode, Events, Handler, Options, Parser};

/// Receives every event and keeps nothing.
struct Quiet;

impl Handler for Quiet {}

/// Counts allocations while armed and fails from `FAIL_AT` on.
struct Failing;

static ARMED: AtomicBool = AtomicBool::new(false);
static COUNT: AtomicU64 = AtomicU64::new(0);
static FAIL_AT: AtomicU64 = AtomicU64::new(u64::MAX);
/// With `OOM_TRACE` set, a child prints the backtrace of each refusal
/// (to find an allocation that still aborts).
static TRACE: AtomicBool = AtomicBool::new(false);

/// The layout of the block behind an `Arc<Vec<u8>>`.
fn shared_text_block() -> Layout {
    Layout::new::<(AtomicU64, AtomicU64, Vec<u8>)>()
}

impl Failing {
    fn refuse(layout: Layout) -> bool {
        if layout == shared_text_block() || layout == expat::internal::parse::control_block_layout()
        {
            return false;
        }
        let r = ARMED.load(Ordering::Relaxed)
            && COUNT.fetch_add(1, Ordering::Relaxed) >= FAIL_AT.load(Ordering::Relaxed);
        if r && TRACE.load(Ordering::Relaxed) {
            ARMED.store(false, Ordering::Relaxed);
            eprintln!("TRACE {}", std::backtrace::Backtrace::force_capture());
            ARMED.store(true, Ordering::Relaxed);
        }
        r
    }
}

// SAFETY: forwards to `System`, or returns null, which `GlobalAlloc`
// allows for a failed allocation.
unsafe impl GlobalAlloc for Failing {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if Self::refuse(layout) {
            return std::ptr::null_mut();
        }
        // SAFETY: the caller's contract for `alloc`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if Self::refuse(layout) {
            return std::ptr::null_mut();
        }
        // SAFETY: the caller's contract for `alloc_zeroed`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's contract for `dealloc`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if new_size > layout.size() && Self::refuse(layout) {
            return std::ptr::null_mut();
        }
        // SAFETY: the caller's contract for `realloc`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Failing = Failing;

const SALT: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

/// Documents that reach the parser's tables, pools, stacks and buffers.
fn documents() -> Vec<(Options, Vec<u8>)> {
    let ns = || Options::new().namespace_separator(b'|');
    let mut utf16 = vec![0xFF, 0xFE];
    for unit in "<?xml version='1.0' encoding='UTF-16'?><d a='\u{e9}'>x\u{263a}</d>".encode_utf16()
    {
        utf16.extend_from_slice(&unit.to_le_bytes());
    }
    vec![
        (Options::new(), b"<a b='1' c=\"2\">text<b/>&amp;&#x41;<![CDATA[x]]><?pi d?><!--c--></a>".to_vec()),
        (
            Options::new(),
            b"<!DOCTYPE a [<!ELEMENT a (b|c)*><!ELEMENT b EMPTY><!ATTLIST a x CDATA 'd' id ID #IMPLIED>\
<!ENTITY e 'v&f;'><!ENTITY f 'w'><!ENTITY % p '<!ENTITY g \"h\">'>%p;<!NOTATION n SYSTEM 's'>\
<!ENTITY u SYSTEM 'u.bin' NDATA n>]><a>&e;&g;<b/></a>"
                .to_vec(),
        ),
        (
            ns(),
            b"<p:a xmlns:p='urn:p' xmlns='urn:d' p:x='1' y='2'><b xmlns:q='urn:a-long-namespace-name'><q:c q:z='3'/></b></p:a>".to_vec(),
        ),
        (Options::new(), utf16),
        (Options::new(), {
            let mut d = b"<r>".to_vec();
            for i in 0..40 {
                d.extend_from_slice(format!("<e{i} a{i}='v{i}' b='{}'>t</e{i}>", "x".repeat(i * 7)).as_bytes());
            }
            d.extend_from_slice(b"</r>");
            d
        }),
        (ns(), {
            let mut d = b"<r xmlns:p='urn:p'".to_vec();
            for i in 0..40 {
                d.extend_from_slice(format!(" p:a{i}='{i}'").as_bytes());
            }
            d.extend_from_slice(b"/>");
            d
        }),
    ]
}

/// Parses document `doc` in pieces with the allocator armed after creation,
/// with no handler (`events` false) or every handler; the outcome as a
/// code (`None` for success) and the allocations counted.
fn run(doc: usize, events: bool, fail_at: u64) -> (Option<ErrorCode>, u64) {
    let (options, bytes) = documents().swap_remove(doc);
    let mut p = Parser::with_options(SALT, options);
    TRACE.store(std::env::var_os("OOM_TRACE").is_some(), Ordering::Relaxed);
    COUNT.store(0, Ordering::Relaxed);
    FAIL_AT.store(fail_at, Ordering::Relaxed);
    ARMED.store(true, Ordering::Relaxed);
    let mut result = Ok(());
    for (i, piece) in bytes.chunks(7).enumerate() {
        let last = (i + 1) * 7 >= bytes.len();
        result = if events {
            p.parse_with_events(&mut Quiet, Events::ALL, piece, last)
        } else {
            p.parse(piece, last)
        };
        if result.is_err() {
            break;
        }
    }
    ARMED.store(false, Ordering::Relaxed);
    let code = result.err().map(|e| e.code());
    drop(p);
    (code, COUNT.load(Ordering::Relaxed))
}

/// The child side: runs one case named by the environment.
#[test]
fn child_case() {
    let Ok(case) = std::env::var("EXPAT_SYSTEM_OOM_CASE") else {
        return;
    };
    let mut parts = case.split(':');
    let mut next = || {
        parts
            .next()
            .expect("doc:events:n")
            .parse::<u64>()
            .expect("number")
    };
    let (doc, events, at) = (next(), next(), next());
    let (code, _) = run(usize::try_from(doc).expect("doc"), events == 1, at);
    println!("OUTCOME {code:?}");
}

/// Creates, configures, uses, resets and frees parsers through the C
/// interface with the allocator armed from the start; the allocations
/// counted. Any result is accepted; only an abort is a failure.
fn run_creation(fail_at: u64) -> u64 {
    use expat_capi as c;
    let doc = b"<a xmlns:p='urn:p'><p:b x='1'>t</p:b></a>";
    COUNT.store(0, Ordering::Relaxed);
    FAIL_AT.store(fail_at, Ordering::Relaxed);
    TRACE.store(std::env::var_os("OOM_TRACE").is_some(), Ordering::Relaxed);
    ARMED.store(true, Ordering::Relaxed);
    let len = std::ffi::c_int::try_from(doc.len()).expect("length");
    // SAFETY: the strings are NUL-terminated literals, the document is a
    // live slice of `len` bytes, and each handle is used only while live.
    unsafe {
        let p = c::XML_ParserCreateNS(c"UTF-8".as_ptr(), b'|' as c::XML_Char);
        if !p.is_null() {
            let _ = c::XML_SetEncoding(p, c"ISO-8859-1".as_ptr());
            let _ = c::XML_SetBase(p, c"http://example.org/dir/".as_ptr());
            let _ = c::XML_Parse(p, doc.as_ptr().cast(), len, 1);
            if c::XML_ParserReset(p, c"UTF-8".as_ptr()) != 0 {
                let _ = c::XML_SetBase(p, c"http://example.org/other/".as_ptr());
                let e = c::XML_ExternalEntityParserCreate(p, c"e".as_ptr(), c"UTF-8".as_ptr());
                if !e.is_null() {
                    let _ = c::XML_Parse(e, b"<x/>".as_ptr().cast(), 4, 1);
                    c::XML_ParserFree(e);
                }
                let _ = c::XML_Parse(p, doc.as_ptr().cast(), len, 1);
            }
            c::XML_ParserFree(p);
        }
        let q = c::XML_ParserCreate(std::ptr::null());
        if !q.is_null() {
            c::XML_ParserFree(q);
        }
    }
    ARMED.store(false, Ordering::Relaxed);
    COUNT.load(Ordering::Relaxed)
}

/// The child side of the creation test.
#[test]
fn child_creation_case() {
    let Ok(at) = std::env::var("EXPAT_SYSTEM_OOM_CREATION") else {
        return;
    };
    let _ = run_creation(at.parse().expect("number"));
    println!("OUTCOME done");
}

#[test]
fn every_allocation_failure_in_creation_and_reset_returns() {
    if std::env::var_os("EXPAT_SYSTEM_OOM_CASE").is_some()
        || std::env::var_os("EXPAT_SYSTEM_OOM_CREATION").is_some()
    {
        return;
    }
    let exe = std::env::current_exe().expect("test binary");
    let total = run_creation(u64::MAX);
    let mut aborted = Vec::new();
    for at in 0..total {
        let out = Command::new(&exe)
            .args([
                "--exact",
                "child_creation_case",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("EXPAT_SYSTEM_OOM_CREATION", at.to_string())
            .output()
            .expect("child process");
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !(out.status.success() && stdout.contains("OUTCOME done")) {
            aborted.push(at);
        }
    }
    println!("system_oom creation: {total} failure points");
    assert!(
        aborted.is_empty(),
        "{} of {total} failure points did not return: {aborted:?}",
        aborted.len()
    );
}

#[test]
fn every_allocation_failure_ends_the_parse_normally() {
    if std::env::var_os("EXPAT_SYSTEM_OOM_CASE").is_some()
        || std::env::var_os("EXPAT_SYSTEM_OOM_CREATION").is_some()
    {
        return;
    }
    let exe = std::env::current_exe().expect("test binary");
    let mut aborted = Vec::new();
    let mut points = 0;
    for (doc, events) in (0..documents().len()).flat_map(|d| [(d, false), (d, true)]) {
        let (code, total) = run(doc, events, u64::MAX);
        assert_eq!(code, None, "document {doc} parses without failures");
        for at in 0..total {
            points += 1;
            let out = Command::new(&exe)
                .args(["--exact", "child_case", "--nocapture", "--test-threads=1"])
                .env(
                    "EXPAT_SYSTEM_OOM_CASE",
                    format!("{doc}:{}:{at}", u8::from(events)),
                )
                .output()
                .expect("child process");
            let stdout = String::from_utf8_lossy(&out.stdout);
            let normal = out.status.success()
                && (stdout.contains("OUTCOME None") || stdout.contains("OUTCOME Some(NoMemory)"));
            if !normal {
                aborted.push(format!("{doc}:{}:{at}", u8::from(events)));
            }
        }
    }
    println!("system_oom: {points} failure points");
    assert!(
        aborted.is_empty(),
        "{} of {points} failure points did not end normally: {aborted:?}",
        aborted.len()
    );
}
