//! Memory-safe Rust port of [libexpat](https://libexpat.github.io/), a
//! stream-oriented XML 1.0 parser.
//!
//! The port is built bottom-up and verified module by module against the
//! vendored C reference (see `docs/ARCHITECTURE.md`). The public parser API
//! does not exist yet; see `docs/STATE.md` for progress.
//!
//! # Features
//!
//! - `std` (default): `std::error::Error` implementations.
//! - `internal-api`: exposes the internal modules under [`internal`] for the
//!   differential test crate. Not a stable API.
#![no_std]
#![forbid(unsafe_code)]

#[cfg(feature = "std")]
extern crate std;

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
}
