//! The allocation-tracker relation between the port and libexpat
//! (module 11c, decision D-030).
//!
//! The port charges its tracker at least the bytes libexpat allocates, at
//! the same point of the parse or earlier, so it must be at least as
//! strict: whenever libexpat stops with `XML_ERROR_NO_MEMORY` because its
//! tracker refused an allocation, the port must stop with
//! `XML_ERROR_NO_MEMORY` too. The port may stop earlier or where libexpat
//! does not (it is allowed to be stricter); then its result line must say
//! `XML_ERROR_NO_MEMORY` and everything else must be C's.
//!
//! [`check`] runs one document three ways: with the caller's options
//! (traces equal, and the port's final count at least C's), then with the
//! tracker limits set tight at C's own final count (`at-threshold` = that
//! count, `at-amplification` = 1.0), and at half of it, where libexpat
//! usually fails, so the port must fail there too.

use expat_oracle::{Factor, ParseOptions, TrackerBytes};

use crate::port::{PortTracker, last_port_tracker, port_trace};
use crate::trace::{first_difference, mask_q4, result_error_code};

/// `XML_ERROR_NO_MEMORY`.
pub const NO_MEMORY: u32 = 1;

/// Outcome of one run under the tracker relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    /// Both traces are equal.
    Equal,
    /// libexpat stopped with `XML_ERROR_NO_MEMORY` and so did the port.
    BothNoMemory,
    /// Only the port stopped with `XML_ERROR_NO_MEMORY` (stricter than C).
    PortStricter,
}

/// One parse of `doc` by both sides under `opts`, with both tracker
/// readouts.
#[derive(Debug, Clone)]
pub struct Run {
    /// libexpat's trace.
    pub c: Vec<u8>,
    /// The port's trace.
    pub port: Vec<u8>,
    /// libexpat's tracker counts.
    pub c_tracker: TrackerBytes,
    /// The port's tracker counts.
    pub port_tracker: PortTracker,
}

/// Parses `doc` with both sides.
pub fn run(doc: &[u8], opts: &ParseOptions) -> Result<Run, String> {
    let c = expat_oracle::parse_trace(doc, opts).map_err(|e| format!("oracle shim: {e:?}"))?;
    let c_tracker = expat_oracle::last_tracker_bytes();
    let port = port_trace(doc, opts);
    Ok(Run {
        c,
        port,
        c_tracker,
        port_tracker: last_port_tracker(),
    })
}

/// Classifies a run (`Err`: the relation is violated).
pub fn relation(r: &Run, opts: &ParseOptions) -> Result<Relation, String> {
    let (c, p) = mask_q4(&r.c, &r.port);
    let Some((line, l, rl)) = first_difference(&c, &p) else {
        return Ok(Relation::Equal);
    };
    let c_code = result_error_code(&c);
    let p_code = result_error_code(&p);
    match (c_code, p_code) {
        (Some(NO_MEMORY), Some(NO_MEMORY)) => Ok(Relation::BothNoMemory),
        (Some(NO_MEMORY), _) => Err(format!(
            "C stops with XML_ERROR_NO_MEMORY but the port does not \
             (line {line}: C `{l}` port `{rl}`; C tracker {:?}, port {:?}; {opts:?})",
            r.c_tracker, r.port_tracker
        )),
        (_, Some(NO_MEMORY)) => Ok(Relation::PortStricter),
        _ => Err(format!(
            "trace mismatch at line {line}: C `{l}` port `{rl}` ({opts:?})"
        )),
    }
}

/// Counts of the outcomes of [`check`] over many documents.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    /// Runs compared.
    pub runs: usize,
    /// Tight runs where libexpat failed with `XML_ERROR_NO_MEMORY` (the
    /// port had to fail too).
    pub c_no_memory: usize,
    /// Tight runs where only the port failed.
    pub port_stricter: usize,
    /// Smallest margin `port.end - C.end` over the base runs.
    pub min_margin: Option<i64>,
}

/// The tracker relation for `doc` under `opts` (module docs).
pub fn check(doc: &[u8], opts: &ParseOptions, tally: &mut Tally) -> Result<(), String> {
    let base = run(doc, opts)?;
    check_from(doc, opts, &base, tally)
}

/// [`check`] with the run under the caller's options already made (the
/// fuzz target reuses its own).
pub fn check_from(
    doc: &[u8],
    opts: &ParseOptions,
    base: &Run,
    tally: &mut Tally,
) -> Result<(), String> {
    tally.runs += 1;
    match relation(base, opts)? {
        Relation::Equal => {}
        // The caller's options may already set tracker limits.
        Relation::BothNoMemory => tally.c_no_memory += 1,
        Relation::PortStricter => {
            if opts.limits.at_threshold.is_none() && opts.limits.at_amplification.is_none() {
                return Err(format!(
                    "the port refuses at the default tracker limits where C accepts ({opts:?})"
                ));
            }
            tally.port_stricter += 1;
        }
    }
    if base.port_tracker.created < base.c_tracker.created {
        return Err(format!(
            "port creation charge {} below C's {}",
            base.port_tracker.created, base.c_tracker.created
        ));
    }
    if relation(base, opts)? == Relation::Equal {
        let margin = base.port_tracker.end as i64 - base.c_tracker.end as i64;
        if margin < 0 {
            return Err(format!(
                "port tracker {} below C's {} at the end of the parse ({opts:?})",
                base.port_tracker.end, base.c_tracker.end
            ));
        }
        tally.min_margin = Some(tally.min_margin.map_or(margin, |m| m.min(margin)));
    }
    for threshold in [base.c_tracker.end, base.c_tracker.end / 2] {
        let mut o = opts.clone();
        o.limits.at_threshold = Some(threshold);
        o.limits.at_amplification = Some(Factor(1.0));
        let r = run(doc, &o)?;
        tally.runs += 1;
        match relation(&r, &o)? {
            Relation::BothNoMemory => tally.c_no_memory += 1,
            Relation::PortStricter => tally.port_stricter += 1,
            Relation::Equal if result_error_code(&r.c) == Some(NO_MEMORY) => {
                tally.c_no_memory += 1;
            }
            Relation::Equal => {}
        }
    }
    Ok(())
}
