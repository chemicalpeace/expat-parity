//! Bounded differential runs for the resource-limit tests (module 11).
//!
//! Inputs that exercise amplification limits can, when either side is
//! wrong, expand without end or allocate without end. Every comparison made
//! through [`compare`] is therefore bounded three ways (limits documented
//! in `docs/REPORT.md` and `docs/DTD-SECURITY.md`):
//!
//! - **Input size:** the caller passes an explicit cap per test; a document
//!   above it is a test bug and panics before anything is parsed.
//! - **Wall clock:** the C and the port parse run on a worker thread; the
//!   test waits at most [`Bounds::timeout`] and fails if the worker has not
//!   finished (the worker is left behind, the test binary still ends).
//! - **Memory:** the port's allocations on the worker thread are counted by
//!   `expat_oracle::counting_alloc::CountingAlloc` (installed as `#[global_allocator]` by each test
//!   binary that uses this module) and refused above
//!   [`Bounds::port_heap_cap`], which aborts the test binary through
//!   `handle_alloc_error` instead of exhausting the machine; libexpat runs
//!   with the oracle's capped allocator (`ParseOptions::heap_cap`), where a
//!   refusal is an ordinary `XML_ERROR_NO_MEMORY` and shows up as a
//!   mismatch. Both peaks are returned so tests can assert on them.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use expat_oracle::ParseOptions;

use expat_oracle::counting_alloc;
pub use expat_oracle::counting_alloc::CountingAlloc;

use crate::port::port_trace;

/// The three bounds of one comparison.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    /// Largest document the test may pass.
    pub max_input: usize,
    /// Wall-clock limit for the C parse plus the port parse.
    pub timeout: Duration,
    /// Live-heap cap of the port parse (worker thread, Rust allocator).
    pub port_heap_cap: usize,
    /// Live-heap cap of the libexpat parser (oracle allocator).
    pub c_heap_cap: usize,
}

impl Bounds {
    /// The bounds of the amplification tests: 256 KiB input, 60 s, 256 MiB
    /// heap per side.
    pub const DEFAULT: Bounds = Bounds {
        max_input: 256 << 10,
        timeout: Duration::from_secs(60),
        port_heap_cap: 256 << 20,
        c_heap_cap: 256 << 20,
    };
}

/// The outcome of one bounded comparison.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// libexpat's trace.
    pub c: Vec<u8>,
    /// The port's trace.
    pub port: Vec<u8>,
    /// Peak live heap of the port parse, bytes.
    pub port_peak: usize,
    /// Peak live heap of the libexpat parser, bytes.
    pub c_peak: usize,
    /// Wall-clock time of both parses.
    pub elapsed: Duration,
    /// libexpat's allocation tracker count (module 11c).
    pub c_tracker: expat_oracle::TrackerBytes,
    /// The port's allocation tracker (module 11c).
    pub port_tracker: crate::port::PortTracker,
}

impl Outcome {
    /// True if both traces are equal (after [`crate::trace::mask_q4`]).
    pub fn equal(&self) -> bool {
        let (c, p) = crate::trace::mask_q4(&self.c, &self.port);
        c == p
    }
}

/// Parses `doc` with libexpat and the port under `opts` within `bounds`
/// (module docs). Panics if the input is above its cap, if the worker does
/// not finish in time, or if the oracle wrapper fails.
pub fn compare(id: &str, doc: &[u8], opts: &ParseOptions, bounds: Bounds) -> Outcome {
    assert!(
        doc.len() <= bounds.max_input,
        "{id}: input of {} bytes is above the test's cap of {} bytes",
        doc.len(),
        bounds.max_input
    );
    let mut o = opts.clone();
    o.heap_cap = bounds.c_heap_cap;
    let doc = doc.to_vec();
    let (tx, rx) = mpsc::channel();
    let name = format!("bounded-{id}");
    let spawned = std::thread::Builder::new().name(name).spawn(move || {
        let start = Instant::now();
        let c = expat_oracle::parse_trace(&doc, &o);
        let c_peak = expat_oracle::last_heap_peak();
        let c_tracker = expat_oracle::last_tracker_bytes();
        counting_alloc::reset(bounds.port_heap_cap);
        let port = port_trace(&doc, &o);
        let port_peak = counting_alloc::peak();
        counting_alloc::reset(usize::MAX);
        let port_tracker = crate::port::last_port_tracker();
        let _ = tx.send((
            c,
            port,
            c_peak,
            port_peak,
            start.elapsed(),
            c_tracker,
            port_tracker,
        ));
    });
    if let Err(e) = spawned {
        panic!("{id}: cannot spawn the worker thread: {e}");
    }
    match rx.recv_timeout(bounds.timeout) {
        Ok((Ok(c), port, c_peak, port_peak, elapsed, c_tracker, port_tracker)) => Outcome {
            c,
            port,
            port_peak,
            c_peak,
            elapsed,
            c_tracker,
            port_tracker,
        },
        Ok((Err(e), ..)) => panic!("{id}: oracle failed: {e}"),
        Err(_) => panic!(
            "{id}: no result within {:?} (wall-clock bound)",
            bounds.timeout
        ),
    }
}
