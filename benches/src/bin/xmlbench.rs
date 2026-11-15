//! Throughput harness for the port, the twin of `benches/filc/xmlbench.c`
//! (same arguments, same output line, so event counts can be compared with
//! C libexpat built by a plain C compiler and by Fil-C; see
//! `benches/filc/README.md`).
//!
//! ```text
//! xmlbench FILE MODE CHUNK NS RUNS
//! ```
//!
//! `MODE` is `none` (no handler, [`expat::Parser::parse`]) or `count`
//! (start/end element and character data counted through
//! [`expat::Parser::parse_with`]); `CHUNK` 0 parses the whole document in one
//! call, otherwise in calls of `CHUNK` bytes (the last one final); `NS` 1
//! enables namespace processing with separator `|`; `RUNS` timed runs follow
//! one untimed warm-up run.

use expat::{Events, Handler, Options, Parser};
use std::process::ExitCode;
use std::time::Instant;

/// Counts the events the C harness counts.
#[derive(Default)]
struct Counts {
    starts: u64,
    ends: u64,
    chars: u64,
    charbytes: u64,
}

impl Handler for Counts {
    const EVENTS: Events = Events::NONE
        .start_element(true)
        .end_element(true)
        .character_data(true);

    fn start_element(&mut self, _name: &[u8], _attributes: &[(&[u8], &[u8])], _specified: usize) {
        self.starts += 1;
    }
    fn end_element(&mut self, _name: &[u8]) {
        self.ends += 1;
    }
    fn character_data(&mut self, data: &[u8]) {
        self.chars += 1;
        self.charbytes += data.len() as u64;
    }
}

/// What one run parses and how.
struct Job<'a> {
    doc: &'a [u8],
    count: bool,
    chunk: usize,
    ns: bool,
}

/// Parses the document once with a fresh parser.
fn run_once(job: &Job<'_>, c: &mut Counts) -> Result<(), expat::Error> {
    let options = if job.ns {
        Options::new().namespace_separator(b'|')
    } else {
        Options::new()
    };
    let mut p = Parser::with_options([7; 16], options);
    *c = Counts::default();
    let pieces: Vec<&[u8]> = if job.chunk == 0 {
        vec![job.doc]
    } else {
        job.doc.chunks(job.chunk).collect()
    };
    let last = pieces.len().saturating_sub(1);
    for (i, piece) in pieces.iter().enumerate() {
        if job.count {
            p.parse_with(c, piece, i == last)?;
        } else {
            p.parse(piece, i == last)?;
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let [_, file, mode, chunk, ns, runs] = args.as_slice() else {
        eprintln!("usage: xmlbench FILE none|count CHUNK NS RUNS");
        return ExitCode::from(2);
    };
    let Ok(doc) = std::fs::read(file) else {
        eprintln!("cannot read {file}");
        return ExitCode::from(2);
    };
    let (Ok(chunk), Ok(runs)) = (chunk.parse::<usize>(), runs.parse::<usize>()) else {
        eprintln!("CHUNK and RUNS must be numbers");
        return ExitCode::from(2);
    };
    if runs == 0 {
        return ExitCode::from(2);
    }
    let job = Job {
        doc: &doc,
        count: mode == "count",
        chunk,
        ns: ns == "1",
    };
    let mut c = Counts::default();
    let mut times = Vec::with_capacity(runs);
    for i in 0..=runs {
        let t0 = Instant::now();
        if let Err(e) = run_once(&job, &mut c) {
            eprintln!("parse error: {e}");
            return ExitCode::from(1);
        }
        if i > 0 {
            times.push(t0.elapsed().as_nanos());
        }
    }
    times.sort_unstable();
    let mid = runs / 2;
    let median = match (times.get(mid), times.get(mid.wrapping_sub(1))) {
        (Some(&m), Some(&below)) if runs % 2 == 0 => (m + below) / 2,
        (Some(&m), _) => m,
        _ => 0,
    };
    let min = times.first().copied().unwrap_or(0);
    let mbps = doc.len() as f64 / 1_048_576.0 / (median as f64 / 1e9);
    println!(
        "impl=rust bytes={} runs={runs} median_ns={median} min_ns={min} mbps={mbps:.1} \
         starts={} ends={} chars={} charbytes={}",
        doc.len(),
        c.starts,
        c.ends,
        c.chars,
        c.charbytes
    );
    ExitCode::SUCCESS
}
