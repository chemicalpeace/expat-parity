//! `m_parsingStatus.parsing`, kept where a handler can change it while the
//! parser is running (`XML_StopParser`).

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

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
}

impl Default for Control {
    fn default() -> Self {
        Self {
            parsing: AtomicU8::new(encode(Parsing::Initialized)),
            param_entity: AtomicBool::new(false),
            default_current: AtomicBool::new(false),
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
