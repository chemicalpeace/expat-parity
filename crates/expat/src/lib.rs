//! Memory-safe Rust port of [libexpat](https://libexpat.github.io/), a
//! stream-oriented XML 1.0 parser.
//!
//! The port is built bottom-up and verified module by module against the
//! vendored C reference (see `docs/ARCHITECTURE.md`). The public parser API
//! does not exist yet; see `docs/STATE.md` for progress.
#![no_std]
#![forbid(unsafe_code)]

#[cfg(feature = "std")]
extern crate std;
