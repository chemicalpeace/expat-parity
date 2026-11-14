//! `m_parsingStatus.parsing`, kept where a handler can change it while the
//! parser is running (`XML_StopParser`).

use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

use super::{Parsing, XmlError};

/// The parsing state of one parser, shared with the handles that stop it.
#[derive(Debug)]
pub struct Control {
    parsing: AtomicU8,
    /// `m_isParamEntity`, for the `XML_ERROR_SUSPEND_PE` check.
    param_entity: AtomicBool,
    /// Set by `XML_DefaultCurrent` from inside a handler: the event
    /// reported last went to the default handler.
    default_current: AtomicBool,
    /// `m_parsingStatus.finalBuffer`.
    final_buffer: AtomicBool,
    /// Which settings a handler changed during the parse and the parser
    /// has not applied yet (the `PENDING_*` bits).
    pending: AtomicU8,
    /// The billion laughs maximum amplification (`f32` bits).
    bl_maximum: AtomicU32,
    /// The billion laughs activation threshold.
    bl_threshold: AtomicU64,
    /// The allocation tracker maximum amplification (`f32` bits).
    at_maximum: AtomicU32,
    /// The allocation tracker activation threshold.
    at_threshold: AtomicU64,
    /// `m_reparseDeferralEnabled`.
    reparse_deferral: AtomicBool,
}

const PENDING_BL_MAXIMUM: u8 = 1;
const PENDING_BL_THRESHOLD: u8 = 2;
const PENDING_AT_MAXIMUM: u8 = 4;
const PENDING_AT_THRESHOLD: u8 = 8;
const PENDING_REPARSE: u8 = 16;

/// Settings changed by a handler during a parse, taken by the parser with
/// [`Control::take_pending`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Pending {
    /// The billion laughs maximum amplification.
    pub bl_maximum: Option<f32>,
    /// The billion laughs activation threshold.
    pub bl_threshold: Option<u64>,
    /// The allocation tracker maximum amplification.
    pub at_maximum: Option<f32>,
    /// The allocation tracker activation threshold.
    pub at_threshold: Option<u64>,
    /// Whether reparse deferral is enabled.
    pub reparse_deferral: Option<bool>,
}

impl Default for Control {
    fn default() -> Self {
        Self {
            parsing: AtomicU8::new(encode(Parsing::Initialized)),
            param_entity: AtomicBool::new(false),
            default_current: AtomicBool::new(false),
            final_buffer: AtomicBool::new(false),
            pending: AtomicU8::new(0),
            bl_maximum: AtomicU32::new(0),
            bl_threshold: AtomicU64::new(0),
            at_maximum: AtomicU32::new(0),
            at_threshold: AtomicU64::new(0),
            reparse_deferral: AtomicBool::new(false),
        }
    }
}

const fn encode(p: Parsing) -> u8 {
    match p {
        Parsing::Initialized => 0,
        Parsing::Parsing => 1,
        Parsing::Finished => 2,
        Parsing::Suspended => 3,
    }
}

impl Control {
    /// The current state.
    #[inline]
    pub fn get(&self) -> Parsing {
        match self.parsing.load(Ordering::Relaxed) {
            0 => Parsing::Initialized,
            1 => Parsing::Parsing,
            2 => Parsing::Finished,
            _ => Parsing::Suspended,
        }
    }

    /// Sets the state.
    #[inline]
    pub fn set(&self, p: Parsing) {
        self.parsing.store(encode(p), Ordering::Relaxed);
    }

    pub(crate) fn set_param_entity(&self, on: bool) {
        self.param_entity.store(on, Ordering::Relaxed);
    }

    /// Records that a handler passed the event reported last to the
    /// default handler (`XML_DefaultCurrent`).
    pub fn mark_default_current(&self) {
        self.default_current.store(true, Ordering::Relaxed);
    }

    /// Whether [`Control::mark_default_current`] was called since the
    /// last call of this function.
    pub(crate) fn take_default_current(&self) -> bool {
        self.default_current.swap(false, Ordering::Relaxed)
    }

    /// `m_parsingStatus.finalBuffer`: whether the input of the current (or
    /// last) parse call is the last.
    #[inline]
    pub fn final_buffer(&self) -> bool {
        self.final_buffer.load(Ordering::Relaxed)
    }

    pub(crate) fn set_final_buffer(&self, on: bool) {
        self.final_buffer.store(on, Ordering::Relaxed);
    }

    /// Records a new billion laughs maximum amplification for the parser
    /// to apply (the caller has checked the value).
    pub fn request_bl_maximum(&self, factor: f32) {
        self.bl_maximum.store(factor.to_bits(), Ordering::Relaxed);
        self.pending.fetch_or(PENDING_BL_MAXIMUM, Ordering::Relaxed);
    }

    /// Records a new billion laughs activation threshold.
    pub fn request_bl_threshold(&self, bytes: u64) {
        self.bl_threshold.store(bytes, Ordering::Relaxed);
        self.pending.fetch_or(PENDING_BL_THRESHOLD, Ordering::Relaxed);
    }

    /// Records a new allocation tracker maximum amplification (the caller
    /// has checked the value).
    pub fn request_at_maximum(&self, factor: f32) {
        self.at_maximum.store(factor.to_bits(), Ordering::Relaxed);
        self.pending.fetch_or(PENDING_AT_MAXIMUM, Ordering::Relaxed);
    }

    /// Records a new allocation tracker activation threshold.
    pub fn request_at_threshold(&self, bytes: u64) {
        self.at_threshold.store(bytes, Ordering::Relaxed);
        self.pending.fetch_or(PENDING_AT_THRESHOLD, Ordering::Relaxed);
    }

    /// Records whether reparse deferral is enabled.
    pub fn request_reparse_deferral(&self, on: bool) {
        self.reparse_deferral.store(on, Ordering::Relaxed);
        self.pending.fetch_or(PENDING_REPARSE, Ordering::Relaxed);
    }

    /// Whether a setting is waiting to be applied.
    #[inline]
    pub(crate) fn has_pending(&self) -> bool {
        self.pending.load(Ordering::Relaxed) != 0
    }

    /// The allocation tracker limits recorded and not yet applied
    /// (maximum, threshold), cleared; the other settings stay pending.
    #[inline]
    pub(crate) fn take_tracker_pending(&self) -> Option<(Option<f32>, Option<u64>)> {
        let mask = PENDING_AT_MAXIMUM | PENDING_AT_THRESHOLD;
        if self.pending.load(Ordering::Relaxed) & mask == 0 {
            return None;
        }
        let bits = self.pending.fetch_and(!mask, Ordering::Relaxed);
        Some((
            (bits & PENDING_AT_MAXIMUM != 0).then(|| f32::from_bits(self.at_maximum.load(Ordering::Relaxed))),
            (bits & PENDING_AT_THRESHOLD != 0).then(|| self.at_threshold.load(Ordering::Relaxed)),
        ))
    }

    /// The settings recorded since the last call, cleared.
    pub(crate) fn take_pending(&self) -> Pending {
        let bits = self.pending.swap(0, Ordering::Relaxed);
        let on = |b: u8| bits & b != 0;
        Pending {
            bl_maximum: on(PENDING_BL_MAXIMUM).then(|| f32::from_bits(self.bl_maximum.load(Ordering::Relaxed))),
            bl_threshold: on(PENDING_BL_THRESHOLD).then(|| self.bl_threshold.load(Ordering::Relaxed)),
            at_maximum: on(PENDING_AT_MAXIMUM).then(|| f32::from_bits(self.at_maximum.load(Ordering::Relaxed))),
            at_threshold: on(PENDING_AT_THRESHOLD).then(|| self.at_threshold.load(Ordering::Relaxed)),
            reparse_deferral: on(PENDING_REPARSE).then(|| self.reparse_deferral.load(Ordering::Relaxed)),
        }
    }

    /// Back to the state of a new parser (`XML_ParserReset`).
    pub(crate) fn reset(&self) {
        self.set(Parsing::Initialized);
        self.set_param_entity(false);
        self.default_current.store(false, Ordering::Relaxed);
        self.set_final_buffer(false);
        self.pending.store(0, Ordering::Relaxed);
    }

    /// `XML_StopParser`, without the error code: the caller stores the
    /// returned error where `XML_GetErrorCode` reads it.
    pub fn stop(&self, resumable: bool) -> Result<(), XmlError> {
        match self.get() {
            Parsing::Initialized => Err(XmlError::NotStarted),
            Parsing::Suspended => {
                if resumable {
                    return Err(XmlError::Suspended);
                }
                self.set(Parsing::Finished);
                Ok(())
            }
            Parsing::Finished => Err(XmlError::Finished),
            Parsing::Parsing => {
                if resumable {
                    if self.param_entity.load(Ordering::Relaxed) {
                        return Err(XmlError::SuspendPe);
                    }
                    self.set(Parsing::Suspended);
                } else {
                    self.set(Parsing::Finished);
                }
                Ok(())
            }
        }
    }
}
