//! Encodings the application describes (`handleUnknownEncoding`,
//! `xmlparse.c` 5291-5337): what an unknown-encoding handler returns, and
//! the encoding the parser keeps from it.

use alloc::boxed::Box;
use alloc::sync::Arc;

use super::c_layout::UNKNOWN_ENCODING;
use super::error::{Stop, XmlError};
use super::{Enc, Handler, Parser};
use crate::tok::Unknown;

/// The converter of an [`EncodingInfo`] (`XML_Encoding::convert` with its
/// `data`): receives the bytes of one multi-byte sequence and returns its
/// code point, or a negative value if it is malformed.
pub type ConvertFn = Box<dyn Fn(&[u8]) -> i32 + Send + Sync>;

/// The release function of an [`EncodingInfo`] (`XML_Encoding::release`
/// with its `data`).
pub type ReleaseFn = Box<dyn FnOnce() + Send + Sync>;

/// What an unknown-encoding handler fills in (`XML_Encoding`): the byte
/// map, an optional converter for multi-byte sequences and an optional
/// release function. The release function runs when the value is dropped
/// without being used, or, once the parser uses the encoding, when the
/// parser is reset or dropped, as libexpat calls `release`.
pub struct EncodingInfo {
    /// `map`: for each byte, -1 (malformed), -2 to -4 (first byte of a
    /// sequence of that many bytes, converted by `convert`) or the code
    /// point it stands for.
    pub map: [i32; 256],
    /// `convert`
    pub convert: Option<ConvertFn>,
    /// `release`
    pub release: Option<ReleaseFn>,
}

impl EncodingInfo {
    /// A map where every byte is malformed, as libexpat fills it before
    /// calling the handler.
    #[must_use]
    pub fn new() -> Self {
        EncodingInfo { map: [-1; 256], convert: None, release: None }
    }
}

impl Default for EncodingInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for EncodingInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EncodingInfo")
            .field("convert", &self.convert.is_some())
            .field("release", &self.release.is_some())
            .finish_non_exhaustive()
    }
}

impl Drop for EncodingInfo {
    fn drop(&mut self) {
        if let Some(r) = self.release.take() {
            r();
        }
    }
}

/// Calls a release function when dropped (`callUnknownEncodingRelease`).
struct Release(Option<ReleaseFn>);

impl Drop for Release {
    fn drop(&mut self) {
        if let Some(r) = self.0.take() {
            r();
        }
    }
}

/// The encoding built from an [`EncodingInfo`] (`m_unknownEncodingMem`
/// with `m_unknownEncodingRelease` and `m_unknownEncodingData`).
pub(crate) struct UnknownEnc {
    pub(crate) enc: Unknown<ConvertFn>,
    _release: Release,
}

impl core::fmt::Debug for UnknownEnc {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UnknownEnc").finish_non_exhaustive()
    }
}

impl Parser {
    /// `handleUnknownEncoding`: asks the handler for the encoding `name`
    /// (UTF-8, as the parser stores names). Without an encoding from the
    /// handler, or with a map that `XmlInitUnknownEncoding` rejects, the
    /// parse fails with `XML_ERROR_UNKNOWN_ENCODING`.
    pub(crate) fn handle_unknown_encoding<H: Handler>(&mut self, h: &mut H, name: &[u8]) -> Result<(), Stop> {
        let Some(mut info) = h.unknown_encoding(name) else {
            return Err(XmlError::UnknownEncoding.into());
        };
        let release = Release(info.release.take());
        // `MALLOC(parser, XmlSizeOfUnknownEncoding())`, kept until the
        // parser is reset or freed even when the map is rejected.
        let direct = self.direct();
        if !self.tracker.t.alloc(UNKNOWN_ENCODING, direct) {
            return Err(XmlError::NoMemory.into());
        }
        self.unknown_encoding_mem = true;
        match Unknown::new(&info.map, info.convert.take(), self.ns) {
            Ok(enc) => {
                let enc = if self.fifth_edition_names { enc.with_fifth_edition_names() } else { enc };
                self.unknown = Some(Arc::new(UnknownEnc { enc, _release: release }));
                self.enc = Enc::Unknown;
                Ok(())
            }
            Err(_) => Err(XmlError::UnknownEncoding.into()),
        }
    }

    /// Takes the release function of the unknown encoding out, so that
    /// the caller runs it (libexpat's `callUnknownEncodingRelease` at the
    /// start of `XML_ParserFree` and `XML_ParserReset`, while the parser
    /// can still be queried). `None` without a release function, or while
    /// an external entity parser still shares the encoding (it then runs
    /// when the last parser using it is dropped, as before).
    pub(crate) fn take_encoding_release(&mut self) -> Option<ReleaseFn> {
        let unknown = self.unknown.as_mut()?;
        Arc::get_mut(unknown)?._release.0.take()
    }

    /// The release function runs and `m_unknownEncodingMem` is freed
    /// (`XML_ParserFree`, `XML_ParserReset`).
    pub(crate) fn free_unknown_encoding(&mut self) {
        self.unknown = None;
        if core::mem::take(&mut self.unknown_encoding_mem) {
            self.tracker.t.free(UNKNOWN_ENCODING);
        }
    }
}
