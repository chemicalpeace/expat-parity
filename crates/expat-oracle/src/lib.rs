//! Test-only access to the vendored libexpat C reference implementation.
//!
//! This crate is the *oracle* of the differential test strategy described in
//! `docs/ARCHITECTURE.md`: every behaviour of the safe Rust port is compared
//! against what this C build does. It is never a dependency of the safe crate.
//!
//! All `unsafe` code of the project outside `expat-capi` lives in the private `ffi` module and
//! the wrappers below; each block states why it is sound.
//!
//! The vendored sources are libexpat `R_2_9_0`
//! (commit `a76b1174c1ea52abb639a4c69d52b630a8a106a2`), MIT licensed; see
//! `vendor/expat/COPYING` and the repository `NOTICE`.

use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_ushort};

pub mod counting_alloc;
mod ffi;
pub mod tok;

/// Upstream release tag of the vendored sources.
pub const UPSTREAM_TAG: &str = "R_2_9_0";
/// Upstream commit hash of the vendored sources.
pub const UPSTREAM_COMMIT: &str = "a76b1174c1ea52abb639a4c69d52b630a8a106a2";

/// Byte-type tables of the C encodings (`struct normal_encoding::type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeTable {
    /// `utf8_encoding` (colon treated as a name-start character).
    Utf8,
    /// `utf8_encoding_ns` (colon is `BT_COLON`).
    Utf8Ns,
    /// `internal_utf8_encoding`.
    InternalUtf8,
    /// `internal_utf8_encoding_ns`.
    InternalUtf8Ns,
    /// `latin1_encoding`.
    Latin1,
    /// `latin1_encoding_ns`.
    Latin1Ns,
    /// `ascii_encoding`.
    Ascii,
    /// `ascii_encoding_ns`.
    AsciiNs,
    /// `little2_encoding` (table used for UTF-16 code units below 0x100).
    Little2,
    /// `little2_encoding_ns`.
    Little2Ns,
}

impl TypeTable {
    /// Every table, in shim id order.
    pub const ALL: [TypeTable; 10] = [
        TypeTable::Utf8,
        TypeTable::Utf8Ns,
        TypeTable::InternalUtf8,
        TypeTable::InternalUtf8Ns,
        TypeTable::Latin1,
        TypeTable::Latin1Ns,
        TypeTable::Ascii,
        TypeTable::AsciiNs,
        TypeTable::Little2,
        TypeTable::Little2Ns,
    ];

    fn id(self) -> c_int {
        match self {
            TypeTable::Utf8 => 0,
            TypeTable::Utf8Ns => 1,
            TypeTable::InternalUtf8 => 2,
            TypeTable::InternalUtf8Ns => 3,
            TypeTable::Latin1 => 4,
            TypeTable::Latin1Ns => 5,
            TypeTable::Ascii => 6,
            TypeTable::AsciiNs => 7,
            TypeTable::Little2 => 8,
            TypeTable::Little2Ns => 9,
        }
    }
}

/// Returns the 256-entry byte-type table of a C encoding.
pub fn type_table(table: TypeTable) -> [u8; 256] {
    let mut out = [0u8; 256];
    // SAFETY: `out` is a valid, writable 256-byte buffer, which is exactly
    // what `oracle_type_table` writes; the id is one of the known ids.
    let rc = unsafe { ffi::oracle_type_table(table.id(), out.as_mut_ptr()) };
    assert_eq!(rc, 0, "oracle_type_table rejected a known table id");
    out
}

/// Which naming bitmap to query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Naming {
    /// `nmstrtPages`: may start an XML name.
    NameStart,
    /// `namePages`: may appear inside an XML name.
    NameChar,
}

/// `UCS2_GET_NAMING(pages, hi, lo)` for the BMP code unit `unit`.
pub fn ucs2_naming(which: Naming, unit: u16) -> bool {
    let [hi, lo] = unit.to_be_bytes();
    let which = match which {
        Naming::NameStart => 0,
        Naming::NameChar => 1,
    };
    // SAFETY: plain integer arguments within the documented 0..=255 range.
    let rc = unsafe { ffi::oracle_ucs2_naming(which, c_uint::from(hi), c_uint::from(lo)) };
    assert!(rc >= 0, "oracle_ucs2_naming rejected in-range input");
    rc == 1
}

/// The static multi-byte UTF-8 predicates of `xmltok.c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Utf8Predicate {
    /// `utf8_isName2`
    IsName2,
    /// `utf8_isName3`
    IsName3,
    /// `utf8_isNmstrt2`
    IsNmstrt2,
    /// `utf8_isNmstrt3`
    IsNmstrt3,
    /// `utf8_isInvalid2`
    IsInvalid2,
    /// `utf8_isInvalid3`
    IsInvalid3,
    /// `utf8_isInvalid4`
    IsInvalid4,
}

impl Utf8Predicate {
    /// Number of bytes the C predicate reads.
    pub fn width(self) -> usize {
        match self {
            Utf8Predicate::IsName2 | Utf8Predicate::IsNmstrt2 | Utf8Predicate::IsInvalid2 => 2,
            Utf8Predicate::IsName3 | Utf8Predicate::IsNmstrt3 | Utf8Predicate::IsInvalid3 => 3,
            Utf8Predicate::IsInvalid4 => 4,
        }
    }
}

/// Evaluates a C UTF-8 predicate. Returns `None` if `bytes` is shorter than
/// [`Utf8Predicate::width`].
pub fn utf8_predicate(pred: Utf8Predicate, bytes: &[u8]) -> Option<bool> {
    let id = match pred {
        Utf8Predicate::IsName2 => 0,
        Utf8Predicate::IsName3 => 1,
        Utf8Predicate::IsNmstrt2 => 2,
        Utf8Predicate::IsNmstrt3 => 3,
        Utf8Predicate::IsInvalid2 => 4,
        Utf8Predicate::IsInvalid3 => 5,
        Utf8Predicate::IsInvalid4 => 6,
    };
    // SAFETY: the shim reads at most `width()` bytes and only after checking
    // that `len` covers them; the pointer/length pair comes from a live slice.
    let rc = unsafe { ffi::oracle_utf8_predicate(id, bytes.as_ptr(), bytes.len()) };
    match rc {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// `XmlUtf8Encode`: returns the encoded bytes (empty for out-of-range input).
pub fn utf8_encode(c: i32) -> Vec<u8> {
    let mut out = [0u8; 4];
    // SAFETY: `out` provides the 4 writable bytes XmlUtf8Encode may write.
    let n = unsafe { ffi::oracle_utf8_encode(c, out.as_mut_ptr()) };
    out.get(..usize::try_from(n).unwrap_or(0))
        .unwrap_or(&[])
        .to_vec()
}

/// `XmlUtf16Encode`: returns the encoded units (empty for out-of-range input).
pub fn utf16_encode(c: i32) -> Vec<u16> {
    let mut out: [c_ushort; 2] = [0; 2];
    // SAFETY: `out` provides the 2 writable units XmlUtf16Encode may write.
    let n = unsafe { ffi::oracle_utf16_encode(c, out.as_mut_ptr()) };
    out.get(..usize::try_from(n).unwrap_or(0))
        .unwrap_or(&[])
        .to_vec()
}

/// SipHash-2-4 of `data` under `key` as computed by `siphash.h`, feeding
/// `sip24_update` in pieces of `split` bytes (`0` = a single call).
pub fn siphash24(key: &[u8; 16], data: &[u8], split: usize) -> u64 {
    // SAFETY: `key` points to 16 readable bytes; `data` is a live slice and
    // the shim reads exactly `data.len()` bytes from it.
    unsafe { ffi::oracle_siphash24(key.as_ptr(), data.as_ptr(), data.len(), split) }
}

/// `sip24_valid()`: libexpat's own self-test against the reference vectors.
pub fn sip24_valid() -> bool {
    // SAFETY: no arguments; the function only reads static data.
    unsafe { ffi::oracle_sip24_valid() == 1 }
}

/// `XML_ParamEntityParsing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParamEntityParsing {
    /// `XML_PARAM_ENTITY_PARSING_NEVER` (libexpat default).
    #[default]
    Never,
    /// `XML_PARAM_ENTITY_PARSING_UNLESS_STANDALONE`.
    UnlessStandalone,
    /// `XML_PARAM_ENTITY_PARSING_ALWAYS`.
    Always,
}

/// Options of one oracle parse.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParseOptions {
    /// Encoding passed to `XML_ParserCreate`; `None` lets the parser detect it.
    pub encoding: Option<String>,
    /// Namespace separator for `XML_ParserCreateNS`; `None` disables namespaces.
    pub ns_separator: Option<u8>,
    /// Feed the input in chunks of this many bytes (`0` = one call).
    pub chunk_size: usize,
    /// Parameter entity parsing mode.
    pub param_entity_parsing: ParamEntityParsing,
    /// Feed each piece with `XML_GetBuffer(2n + 3)` + `XML_ParseBuffer(n)`
    /// instead of `XML_Parse`.
    pub buffer_api: bool,
    /// `XML_SetReparseDeferralEnabled(parser, XML_FALSE)`.
    pub no_reparse_deferral: bool,
    /// `XML_SetReturnNSTriplet(parser, XML_TRUE)` (only has an effect with
    /// namespaces).
    pub ns_triplets: bool,
    /// Resource-limit setters called before parsing (module 11); their
    /// results appear as `limit` lines at the start of the trace.
    pub limits: Limits,
    /// Trace character data as `textdigest <len> <fnv1a64>` instead of the
    /// text, so the trace of a large expansion stays small.
    pub text_digest: bool,
    /// Add an `accounting <direct> <indirect>` line (the billion-laughs
    /// counters of the root parser) before the `result` line.
    pub trace_accounting: bool,
    /// Non-zero: create the parser with an allocator that refuses requests
    /// taking its live heap above this many bytes (memory cap for bounded
    /// tests); see [`last_heap_peak`].
    pub heap_cap: usize,
    /// External entity reference handler (module 11d, test handler: the
    /// library never fetches anything).
    pub ext_handler: ExtHandler,
    /// `XML_SetNotStandaloneHandler` with a handler returning this value
    /// (`None`: no handler); traced as `notstandalone`.
    pub not_standalone: Option<bool>,
    /// `XML_UseForeignDTD(parser, XML_TRUE)`.
    pub use_foreign_dtd: bool,
    /// The external entities [`ExtHandler::Read`] can read, by system
    /// identifier (in memory: nothing is ever fetched).
    pub ext_entities: Vec<(Vec<u8>, Vec<u8>)>,
}

/// The test external entity reference handler of a parse (module 11d).
/// Every call is traced as `extref <context> <base> <systemId> <publicId>`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ExtHandler {
    /// No handler is set (libexpat's default).
    #[default]
    None,
    /// The handler returns `XML_STATUS_ERROR`
    /// (`XML_ERROR_EXTERNAL_ENTITY_HANDLING`).
    Fail,
    /// The handler returns `XML_STATUS_OK` without reading the entity.
    Accept,
    /// The handler reads the entity whose system identifier is in
    /// [`ParseOptions::ext_entities`] through an external entity parser
    /// (`XML_ExternalEntityParserCreate`), fed like the document, traces
    /// `extend <ok> <code> <line> <column> <byte index>` and returns the
    /// child's status; an unknown identifier is accepted without reading.
    /// At most [`EXT_MAX_DEPTH`] nested children (`extdepth`).
    Read,
}

/// `ORACLE_EXT_MAX_DEPTH`: nesting limit of [`ExtHandler::Read`].
pub const EXT_MAX_DEPTH: usize = 16;

/// Serializes [`ParseOptions::ext_entities`] for the shim (u32 LE length
/// + system id, u32 LE length + bytes per record).
fn ext_records(entities: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (k, v) in entities {
        for part in [k, v] {
            let n = u32::try_from(part.len()).unwrap_or(u32::MAX);
            out.extend_from_slice(&n.to_le_bytes());
            out.extend_from_slice(part);
        }
    }
    out
}

/// A `float` limit passed to libexpat, compared by bit pattern so the
/// options stay `Eq` (NaN is a meaningful test value: libexpat rejects it).
#[derive(Debug, Clone, Copy, Default)]
pub struct Factor(pub f32);

impl PartialEq for Factor {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for Factor {}

/// Resource-limit setters; `None` leaves libexpat's default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    /// `XML_SetBillionLaughsAttackProtectionMaximumAmplification`.
    pub bl_amplification: Option<Factor>,
    /// `XML_SetBillionLaughsAttackProtectionActivationThreshold`.
    pub bl_threshold: Option<u64>,
    /// `XML_SetAllocTrackerMaximumAmplification`.
    pub at_amplification: Option<Factor>,
    /// `XML_SetAllocTrackerActivationThreshold`.
    pub at_threshold: Option<u64>,
}

impl Limits {
    /// No setter is called.
    pub const NONE: Limits = Limits {
        bl_amplification: None,
        bl_threshold: None,
        at_amplification: None,
        at_threshold: None,
    };
}

/// Peak live heap (bytes) of the calling thread's last [`parse_trace`] that
/// ran with a non-zero [`ParseOptions::heap_cap`].
pub fn last_heap_peak() -> usize {
    // SAFETY: reads a thread-local counter of the shim; no arguments.
    unsafe { ffi::oracle_last_heap_peak() }
}

/// The allocation tracker's live byte count
/// (`m_alloc_tracker.bytesAllocated`) during the calling thread's last
/// [`parse_trace`]: after `XML_ParserCreate` and the limit setters, and at
/// the end of the parse (module 11c).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrackerBytes {
    /// After creation and the limit setters.
    pub created: u64,
    /// At the end of the parse, before `XML_ParserFree`.
    pub end: u64,
}

/// [`TrackerBytes`] of the calling thread's last [`parse_trace`].
pub fn last_tracker_bytes() -> TrackerBytes {
    let mut t = TrackerBytes::default();
    // SAFETY: both pointers are valid, distinct out-parameters; the shim
    // only writes two thread-local values through them.
    unsafe { ffi::oracle_last_tracker(&mut t.created, &mut t.end) };
    t
}

/// The C structures whose `sizeof` (`offsetof(BLOCK, s)` for
/// `BLOCK_HEADER`, `sizeof(size_t) + EXPAT_MALLOC_PADDING` for
/// `MALLOC_HEADER`, `sizeof(NAMED *)` for `NAMED_PTR`) [`c_sizes`]
/// returns, in its order (D-034).
pub const ORACLE_C_SIZE_NAMES: [&str; 16] = [
    "TAG",
    "BINDING",
    "PREFIX",
    "ENTITY",
    "CONTENT_SCAFFOLD",
    "BLOCK_HEADER",
    "ATTRIBUTE_ID",
    "DEFAULT_ATTRIBUTE",
    "NAME_AND_DEFAULT_ATTRIBUTE",
    "NS_ATT",
    "ELEMENT_TYPE",
    "OPEN_INTERNAL_ENTITY",
    "ATTRIBUTE",
    "NAMED_PTR",
    "MALLOC_HEADER",
    "INT",
];

/// The C compiler's sizes of the structures named in
/// [`ORACLE_C_SIZE_NAMES`], from the vendored `xmlparse.c` built for the
/// current target.
pub fn c_sizes() -> Vec<(&'static str, usize)> {
    let mut v = [0usize; ORACLE_C_SIZE_NAMES.len()];
    // SAFETY: `v` is a valid buffer of `v.len()` `size_t`s; the shim writes
    // at most that many values and keeps no pointer.
    let n = unsafe { ffi::oracle_c_sizes(v.as_mut_ptr(), v.len()) };
    ORACLE_C_SIZE_NAMES.iter().copied().zip(v).take(n).collect()
}

/// One event of libexpat's allocation tracker (`expat_malloc`,
/// `expat_realloc`, `expat_free`; D-034), as `expat_heap_stat` reports it
/// with `EXPAT_MALLOC_DEBUG=2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackerEvent {
    /// `true` for an increase (`+`: malloc, growing realloc, or a refused
    /// increase), `false` for a decrease (`-`: free, shrinking realloc).
    pub increase: bool,
    /// The increase was refused (`expat_heap_increase_tolerable` returned
    /// false): nothing changed.
    pub refused: bool,
    /// Bytes of the change: `sizeof(size_t) + EXPAT_MALLOC_PADDING + size`
    /// for malloc and free, the size difference for realloc.
    pub size: u64,
    /// `bytesAllocated` after the event.
    pub total: u64,
    /// `peakBytesAllocated` after the event.
    pub peak: u64,
    /// The root parser's `countBytesDirect` at the event.
    pub direct: u64,
    /// The `xmlparse.c` line of the `MALLOC` / `REALLOC` / `FREE`.
    pub line: u32,
}

/// The tracker events of one [`parse_trace_with_tracker_events`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackerEvents {
    /// Every event, from `XML_ParserCreate` to `XML_ParserFree` (external
    /// entity parsers included: they count on the root's tracker).
    pub events: Vec<TrackerEvent>,
    /// Number of events before `XML_ParserFree` of the root parser.
    pub at_end: usize,
}

/// A [`parse_trace`] that also returns every allocation tracker event of
/// the parse. `Err(OutOfMemory)` when the shim dropped events.
pub fn parse_trace_with_tracker_events(
    doc: &[u8],
    opts: &ParseOptions,
) -> Result<(Vec<u8>, TrackerEvents), OracleError> {
    // SAFETY: sets a thread-local flag of the shim.
    unsafe { ffi::oracle_tracker_events_capture(1) };
    let trace = parse_trace(doc, opts);
    let mut raw: *mut ffi::OracleTrackerEvent = std::ptr::null_mut();
    let mut complete: c_int = 0;
    let mut at_end: usize = 0;
    // SAFETY: valid out-pointers; the shim hands over a malloc'd array of
    // `n` events (or NULL with n == 0) and keeps no pointer to it.
    let n = unsafe { ffi::oracle_tracker_events_take(&mut raw, &mut complete, &mut at_end) };
    let events = if raw.is_null() || n == 0 {
        Vec::new()
    } else {
        // SAFETY: `raw` points to `n` initialised events (see above).
        let slice = unsafe { std::slice::from_raw_parts(raw, n) };
        slice
            .iter()
            .map(|e| TrackerEvent {
                // Byte view of the C char, neutral to its signedness (c_char is u8 on aarch64, i8 on x86_64).
                increase: u8::from_ne_bytes(e.op.to_ne_bytes()) == b'+',
                refused: e.refused != 0,
                size: e.size,
                total: e.total,
                peak: e.peak,
                direct: e.direct,
                line: u32::try_from(e.line).unwrap_or(0),
            })
            .collect()
    };
    if !raw.is_null() {
        // SAFETY: allocated by the shim with malloc/realloc, freed once.
        unsafe { ffi::oracle_free(raw.cast()) };
    }
    let trace = trace?;
    if complete == 0 {
        return Err(OracleError::OutOfMemory);
    }
    let at_end = if at_end == 0 { events.len() } else { at_end };
    Ok((trace, TrackerEvents { events, at_end }))
}

/// Errors of the oracle wrapper itself (never of the parse being traced).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OracleError {
    /// The encoding name contained an interior NUL byte.
    EncodingHasNul,
    /// The input is longer than libexpat's `int` length parameter allows.
    InputTooLarge,
    /// The C shim could not allocate.
    OutOfMemory,
}

impl std::fmt::Display for OracleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OracleError::EncodingHasNul => f.write_str("encoding name contains a NUL byte"),
            OracleError::InputTooLarge => f.write_str("input exceeds i32::MAX bytes"),
            OracleError::OutOfMemory => f.write_str("oracle shim allocation failed"),
        }
    }
}

impl std::error::Error for OracleError {}

/// Parses `doc` with libexpat and returns the canonical trace
/// (format: `docs/ARCHITECTURE.md`, "Trace format").
pub fn parse_trace(doc: &[u8], opts: &ParseOptions) -> Result<Vec<u8>, OracleError> {
    if i32::try_from(doc.len()).is_err() {
        return Err(OracleError::InputTooLarge);
    }
    let encoding = match &opts.encoding {
        Some(e) => Some(CString::new(e.as_bytes()).map_err(|_| OracleError::EncodingHasNul)?),
        None => None,
    };
    // Lives until the call returns; the shim only reads it during the parse.
    let records = ext_records(&opts.ext_entities);
    let c_opts = ffi::OracleOptions {
        encoding: encoding.as_ref().map_or(std::ptr::null(), |e| e.as_ptr()),
        ns_separator: opts.ns_separator.map_or(0, c_int::from),
        chunk_size: opts.chunk_size,
        param_entity_parsing: match opts.param_entity_parsing {
            ParamEntityParsing::Never => 0,
            ParamEntityParsing::UnlessStandalone => 1,
            ParamEntityParsing::Always => 2,
        },
        buffer_api: c_int::from(opts.buffer_api),
        no_reparse_deferral: c_int::from(opts.no_reparse_deferral),
        ns_triplets: c_int::from(opts.ns_triplets),
        set_bl_amplification: c_int::from(opts.limits.bl_amplification.is_some()),
        bl_amplification: opts.limits.bl_amplification.map_or(0.0, |f| f.0),
        set_bl_threshold: c_int::from(opts.limits.bl_threshold.is_some()),
        bl_threshold: opts.limits.bl_threshold.unwrap_or(0),
        set_at_amplification: c_int::from(opts.limits.at_amplification.is_some()),
        at_amplification: opts.limits.at_amplification.map_or(0.0, |f| f.0),
        set_at_threshold: c_int::from(opts.limits.at_threshold.is_some()),
        at_threshold: opts.limits.at_threshold.unwrap_or(0),
        text_digest: c_int::from(opts.text_digest),
        trace_accounting: c_int::from(opts.trace_accounting),
        heap_cap: opts.heap_cap,
        ext_handler: match opts.ext_handler {
            ExtHandler::None => 0,
            ExtHandler::Fail => 1,
            ExtHandler::Accept => 2,
            ExtHandler::Read => 3,
        },
        not_standalone: match opts.not_standalone {
            None => 0,
            Some(true) => 1,
            Some(false) => 2,
        },
        use_foreign_dtd: c_int::from(opts.use_foreign_dtd),
        ext_data: records.as_ptr(),
        ext_len: records.len(),
    };
    let mut out: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    // SAFETY: `doc` is a live slice whose length fits in `int` (checked
    // above); `c_opts` and the `encoding` CString it points into outlive the
    // call; `out`/`out_len` are valid out-pointers.
    let rc = unsafe {
        ffi::oracle_parse_trace(doc.as_ptr(), doc.len(), &c_opts, &mut out, &mut out_len)
    };
    if rc != 0 || out.is_null() {
        return Err(OracleError::OutOfMemory);
    }
    // SAFETY: on success the shim returns a malloc'd buffer of exactly
    // `out_len` initialised bytes that we own until `oracle_free`.
    let trace = unsafe { std::slice::from_raw_parts(out, out_len) }.to_vec();
    // SAFETY: `out` came from the shim's allocator and is freed exactly once.
    unsafe { ffi::oracle_free(out.cast()) };
    Ok(trace)
}

/// `ORACLE_CORE_HEAP_CAP`: the harness heap of [`core_run`] refuses any
/// request that would take its total above this many bytes.
pub const CORE_HEAP_CAP: usize = 32 << 20;

/// Serialises [`core_run`]: the shim's allocation hooks use global state.
static CORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Runs a program of string pool or hash table operations against
/// `xmlparse.c` (`shim/oracle_core.c`; program and trace formats in the test
/// crate's `core` module) and returns the text trace, or `None` if the
/// program header is malformed.
pub fn core_run(prog: &[u8]) -> Option<String> {
    let _guard = CORE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut out: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    // SAFETY: `prog` is a live slice, `out`/`out_len` are valid out-pointers,
    // and the lock above serialises access to the shim's global state.
    let rc = unsafe { ffi::oracle_core_run(prog.as_ptr(), prog.len(), &mut out, &mut out_len) };
    if rc != 0 {
        return None;
    }
    let bytes = if out.is_null() {
        Vec::new()
    } else {
        // SAFETY: on success a non-null `out` is a malloc'd buffer of exactly
        // `out_len` initialised bytes that we own until `oracle_free`.
        let v = unsafe { std::slice::from_raw_parts(out, out_len) }.to_vec();
        // SAFETY: `out` came from the shim's allocator and is freed once.
        unsafe { ffi::oracle_free(out.cast()) };
        v
    };
    Some(String::from_utf8(bytes).expect("the trace is ASCII"))
}

/// One input operation of [`buffer_run`]: `XML_Parse` with the next `n`
/// bytes, or `XML_GetBuffer(req)` followed by `XML_ParseBuffer(n)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferOp {
    /// `XML_Parse(next n bytes, is_final)`.
    Parse {
        /// Number of document bytes.
        n: i32,
        /// `isFinal`.
        is_final: bool,
    },
    /// `XML_GetBuffer(req)`, copy `min(n, req)` document bytes, then
    /// `XML_ParseBuffer(n, is_final)` if the buffer was granted.
    Buffer {
        /// The `len` given to `XML_GetBuffer`.
        req: i32,
        /// The `len` given to `XML_ParseBuffer`.
        n: i32,
        /// `isFinal`.
        is_final: bool,
    },
}

/// Feeds `doc` to libexpat with `ops` and returns the white-box trace of
/// its input buffer after each operation (`shim/oracle_core.c`,
/// `oracle_buffer_run`; the format is reproduced by the test crate's
/// `port::buffer_run`).
pub fn buffer_run(doc: &[u8], ops: &[BufferOp], ns: bool, reparse_deferral: bool) -> String {
    let mut flat: Vec<c_int> = Vec::with_capacity(ops.len() * 3);
    for op in ops {
        match *op {
            BufferOp::Parse { n, is_final } => {
                flat.extend_from_slice(&[c_int::MIN, n, c_int::from(is_final)]);
            }
            BufferOp::Buffer { req, n, is_final } => {
                flat.extend_from_slice(&[req, n, c_int::from(is_final)]);
            }
        }
    }
    let mut out: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    // SAFETY: `doc` and `flat` are live slices of the given lengths (`flat`
    // holds `ops.len()` triples); `out`/`out_len` are valid out-pointers.
    // The function uses no global state.
    let rc = unsafe {
        ffi::oracle_buffer_run(
            doc.as_ptr(),
            doc.len(),
            flat.as_ptr(),
            ops.len(),
            c_int::from(ns),
            c_int::from(reparse_deferral),
            &mut out,
            &mut out_len,
        )
    };
    assert!(
        rc == 0 && !out.is_null(),
        "oracle_buffer_run failed to allocate"
    );
    // SAFETY: on success `out` is a malloc'd buffer of exactly `out_len`
    // initialised bytes that we own until `oracle_free`.
    let v = unsafe { std::slice::from_raw_parts(out, out_len) }.to_vec();
    // SAFETY: `out` came from the shim's allocator and is freed once.
    unsafe { ffi::oracle_free(out.cast()) };
    String::from_utf8(v).expect("the trace is ASCII")
}

/// `poolBytesToAllocateFor(block_size)` of `xmlparse.c`.
pub fn pool_bytes_to_allocate_for(block_size: i32) -> usize {
    // SAFETY: pure function of an integer.
    unsafe { ffi::oracle_pool_bytes_to_allocate_for(block_size) }
}

/// `offsetof(BLOCK, s)` in `xmlparse.c`.
pub fn block_header_size() -> usize {
    // SAFETY: returns a constant.
    unsafe { ffi::oracle_block_header_size() }
}

/// `hash(parser, key, len)` of `xmlparse.c` for a parser whose salt was set
/// with `XML_SetHashSalt16Bytes(salt)`, and the bit width of C's `unsigned
/// long` (the type of the result). `key` must not contain NUL.
pub fn parser_hash(salt: &[u8; 16], key: &[u8]) -> (u64, u32) {
    let c = CString::new(key).expect("key without NUL");
    let _guard = CORE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut bits: c_int = 0;
    // SAFETY: `salt` has 16 bytes, `c` holds `key.len()` bytes plus NUL,
    // `bits` is a valid out-pointer.
    let h = unsafe { ffi::oracle_parser_hash(salt.as_ptr(), c.as_ptr(), key.len(), &mut bits) };
    (h, u32::try_from(bits).expect("positive width"))
}

/// The version string reported by the vendored libexpat (`XML_ExpatVersion`).
pub fn expat_version() -> String {
    // SAFETY: XML_ExpatVersion returns a pointer to a static NUL-terminated
    // string.
    let p: *const c_char = unsafe { ffi::oracle_expat_version() };
    // SAFETY: see above; the string is static and valid for the program's life.
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_pin() {
        assert_eq!(expat_version(), "expat_2.9.0");
    }

    #[test]
    fn trace_of_trivial_document() {
        let t = parse_trace(b"<a x='1'>hi</a>", &ParseOptions::default()).expect("oracle trace");
        let t = String::from_utf8(t).expect("trace is ASCII");
        assert_eq!(
            t,
            "start a 2\nattr x 1\ntext hi\nend a\nresult ok 0 1 15 15\n"
        );
    }
}
