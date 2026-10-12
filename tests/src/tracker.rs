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

/// Whether a trace shows a refusal by the allocation tracker: the parse
/// ended with `XML_ERROR_NO_MEMORY`, or an external entity parser did
/// (`extend 0 1 ...`; the root then reports
/// `XML_ERROR_EXTERNAL_ENTITY_HANDLING`), or one could not be created
/// (`extcreate`, module 11d).
pub fn refused(trace: &[u8]) -> bool {
    result_error_code(trace) == Some(NO_MEMORY)
        || trace
            .split(|&b| b == b'\n')
            .any(|l| l.starts_with(b"extend 0 1 ") || l == b"extcreate")
}

/// Classifies a run (`Err`: the relation is violated).
pub fn relation(r: &Run, opts: &ParseOptions) -> Result<Relation, String> {
    let (c, p) = mask_q4(&r.c, &r.port);
    let Some((line, l, rl)) = first_difference(&c, &p) else {
        return Ok(Relation::Equal);
    };
    let c_code = refused(&c).then_some(NO_MEMORY).or(result_error_code(&c));
    let p_code = refused(&p).then_some(NO_MEMORY).or(result_error_code(&p));
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

/// Parses `doc` under `opts` with both sides recording every tracker
/// event and requires exact parity (D-034): equal traces (so equal limit
/// outcomes and positions) and equal event streams from the parser's
/// creation through `XML_ParserFree` (direction, size, refusal, running
/// total and peak at every event). Returns whether C refused an
/// allocation.
pub fn compare_events(doc: &[u8], opts: &ParseOptions) -> Result<bool, String> {
    let (c_trace, c) = expat_oracle::parse_trace_with_tracker_events(doc, opts)
        .map_err(|e| format!("oracle shim: {e:?}"))?;
    let (p_trace, port, port_at_end) = crate::port::port_trace_with_tracker_events(doc, opts);
    let (cm, pm) = mask_q4(&c_trace, &p_trace);
    if let Some((line, l, r)) = first_difference(&cm, &pm) {
        let side = match (refused(&cm), refused(&pm)) {
            (false, true) => "the port refuses where C accepts; ",
            (true, false) => "C refuses where the port accepts; ",
            _ => "",
        };
        return Err(format!(
            "{side}trace mismatch at line {line}: C `{l}` port `{r}` ({opts:?})"
        ));
    }
    let same = |a: &expat_oracle::TrackerEvent, b: &expat::internal::parse::TrackerEvent| {
        a.increase == b.increase
            && a.refused == b.refused
            && a.size == b.size
            && a.total == b.total
            && a.peak == b.peak
    };
    let k = c
        .events
        .iter()
        .zip(&port)
        .take_while(|(a, b)| same(a, b))
        .count();
    if k != c.events.len() || k != port.len() || c.at_end != port_at_end {
        let show = |inc: bool, refd: bool, size: u64, total: u64, peak: u64| {
            format!(
                "{}{size}{} total {total} peak {peak}",
                if inc { '+' } else { '-' },
                if refd { " refused" } else { "" }
            )
        };
        let ce = c.events.get(k).map_or("no event".into(), |e| {
            format!(
                "{} (xmlparse.c:{})",
                show(e.increase, e.refused, e.size, e.total, e.peak),
                e.line
            )
        });
        let pe = port.get(k).map_or("no event".into(), |e| {
            show(e.increase, e.refused, e.size, e.total, e.peak)
        });
        return Err(format!(
            "tracker events differ at event {k} (C {} events, end of parse at {}; port {}, at {}): \
             C {ce}, port {pe} ({opts:?})",
            c.events.len(),
            c.at_end,
            port.len(),
            port_at_end
        ));
    }
    Ok(c.events.iter().any(|e| e.refused))
}

/// [`check`] with the run under the caller's options already made (the
/// fuzz target reuses its own). Exact parity (D-034): the traces must be
/// equal (the port never refuses where C accepts, nor the reverse), the
/// counts after creation and at the end of the parse equal, and two runs
/// with tight limits (activation threshold at C's final count and at half
/// of it, amplification factor 1.0) must agree event by event
/// ([`compare_events`]).
pub fn check_from(
    doc: &[u8],
    opts: &ParseOptions,
    base: &Run,
    tally: &mut Tally,
) -> Result<(), String> {
    tally.runs += 1;
    match relation(base, opts)? {
        Relation::Equal => {
            if refused(&base.c) {
                tally.c_no_memory += 1;
            }
        }
        Relation::BothNoMemory => {
            return Err(format!(
                "C and the port both refuse but stop differently ({opts:?})"
            ));
        }
        Relation::PortStricter => {
            tally.port_stricter += 1;
            return Err(format!("the port refuses where C accepts ({opts:?})"));
        }
    }
    if base.port_tracker.created != base.c_tracker.created {
        return Err(format!(
            "port creation charge {} differs from C's {}",
            base.port_tracker.created, base.c_tracker.created
        ));
    }
    let margin = base.port_tracker.end as i64 - base.c_tracker.end as i64;
    if margin != 0 {
        return Err(format!(
            "port tracker {} differs from C's {} at the end of the parse ({opts:?})",
            base.port_tracker.end, base.c_tracker.end
        ));
    }
    tally.min_margin = Some(tally.min_margin.map_or(margin, |m| m.min(margin)));
    for threshold in [base.c_tracker.end, base.c_tracker.end / 2] {
        let mut o = opts.clone();
        o.limits.at_threshold = Some(threshold);
        o.limits.at_amplification = Some(Factor(1.0));
        tally.runs += 1;
        if compare_events(doc, &o)? {
            tally.c_no_memory += 1;
        }
    }
    Ok(())
}
