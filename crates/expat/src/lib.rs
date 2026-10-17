//! Memory-safe Rust port of [libexpat](https://libexpat.github.io/) 2.9.0,
//! a stream-oriented XML 1.0 parser, verified against the vendored C
//! reference by differential testing and fuzzing.
//!
//! **`0.1.0-beta.1` is experimental**: an early-use MVP whose public API
//! covers only part of libexpat's (see [Limitations](#limitations)) and
//! may change before 0.1.0.
//!
//! [`Parser`] parses a document fed in one or more pieces, with the
//! encoding and namespace [`Options`] given, and reports the result, or an
//! [`Error`] with its [`ErrorCode`] and position. [`Parser::parse_with`]
//! reports the element, character data, processing instruction, comment,
//! CDATA section and namespace declaration events to a [`Handler`]; its
//! [`Handler::EVENTS`] ([`Events`]) says which.
//!
//! ```
//! use expat::{Events, Handler, Parser};
//!
//! #[derive(Default)]
//! struct Names(Vec<Vec<u8>>);
//!
//! impl Handler for Names {
//!     const EVENTS: Events = Events::NONE.start_element(true);
//!     fn start_element(&mut self, name: &[u8], _attributes: &[(&[u8], &[u8])], _specified: usize) {
//!         self.0.push(name.to_vec());
//!     }
//! }
//!
//! // Use an unpredictable salt for untrusted documents (see "Hash salt").
//! let salt = [0x5a; 16];
//! let mut names = Names::default();
//! let mut parser = Parser::new(salt);
//! parser.parse_with(&mut names, b"<doc><item/>", false)?;
//! parser.parse_with(&mut names, b"<item/></doc>", true)?;
//! assert_eq!(names.0, [&b"doc"[..], b"item", b"item"]);
//! # Ok::<(), expat::Error>(())
//! ```
//!
//! # Hash salt
//!
//! The salt passed to [`Parser::new`] keys the parser's hash tables. This
//! crate has no entropy source, so the caller provides it. If whoever
//! supplies the document can predict the salt (a constant, as in the
//! examples), they can craft colliding names that make parsing much slower
//! (hash flooding); use bytes from a cryptographic random source for
//! untrusted documents.
//!
//! # Limitations
//!
//! - Only the nine events of [`Events`]: no default handler, no DTD,
//!   entity or XML declaration events, no unknown-encoding handler.
//! - A handler cannot read the position or stop the parse; the position is
//!   reported only with an [`Error`].
//! - External entities and DTD subsets are never loaded: the crate does no
//!   I/O and has no external entity handler, and the parser behaves as
//!   libexpat does when none is set. The internal DTD subset is parsed and
//!   internal entities are expanded.
//! - The amplification limits are libexpat 2.9.0's defaults and cannot be
//!   changed. The parser counts its allocations as libexpat does and
//!   refuses with [`ErrorCode::NoMemory`] or
//!   [`ErrorCode::AmplificationLimitBreach`] where libexpat does (checked
//!   by the differential tests).
//! - When the system allocator itself fails, Rust's allocation-error
//!   handler runs (by default the process aborts); libexpat would return
//!   `XML_ERROR_NO_MEMORY`.
//! - MSRV: Rust 1.99.
//!
//! # Features
//!
//! - `std` (default): `std::error::Error` implementations.
//! - `internal-api`: exposes the internal modules under `internal` for the
//!   differential test crate. Not a stable API.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod api;
mod handler;
pub use api::{Error, ErrorCode, Options, Parser};
pub use handler::{Events, Handler};

/// Declares an internal module that is private by default and public under
/// the `internal-api` feature.
macro_rules! internal_modules {
    ($($(#[$m:meta])* $name:ident),* $(,)?) => {
        /// Unstable internals, exposed for differential testing only.
        #[cfg(feature = "internal-api")]
        pub mod internal {
            $(pub use crate::$name;)*
        }
        $(
            $(#[$m])*
            #[cfg(feature = "internal-api")]
            pub mod $name;
            $(#[$m])*
            #[cfg(not(feature = "internal-api"))]
            mod $name;
        )*
    };
}

internal_modules! {
    // Leaf modules are consumed by the tokenizer port (docs/STATE.md module
    // 5); until then only the tests and `internal-api` use them.
    // reason: unused outside tests until their consumer is ported.
    #[cfg_attr(not(feature = "internal-api"), allow(dead_code))]
    chartab,
    // reason: as above.
    #[cfg_attr(not(feature = "internal-api"), allow(dead_code))]
    naming,
    // reason: as above.
    #[cfg_attr(not(feature = "internal-api"), allow(dead_code))]
    encode,
    // reason: as above (consumed by the hash tables of the parser core).
    #[cfg_attr(not(feature = "internal-api"), allow(dead_code))]
    siphash,
    // The module's re-exports are its API for the parser core.
    // reason: consumed by the parser core (docs/STATE.md modules 9-11).
    #[cfg_attr(not(feature = "internal-api"), allow(dead_code, unused_imports))]
    tok,
    // reason: consumed by the parser core (docs/STATE.md modules 9-11).
    #[cfg_attr(not(feature = "internal-api"), allow(dead_code))]
    role,
    // reason: consumed by the parser core (docs/STATE.md modules 10-11).
    #[cfg_attr(not(feature = "internal-api"), allow(dead_code))]
    heap,
    // reason: as above.
    #[cfg_attr(not(feature = "internal-api"), allow(dead_code))]
    pool,
    // reason: as above.
    #[cfg_attr(not(feature = "internal-api"), allow(dead_code))]
    hash,
    // reason: consumed by the public API (docs/STATE.md module 12).
    #[cfg_attr(not(feature = "internal-api"), allow(dead_code, unused_imports))]
    parse,
}
