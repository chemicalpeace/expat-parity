//! Allocation accounting: the `MALLOC`/`REALLOC`/`FREE` calls of `xmlparse.c`.
//!
//! libexpat routes every allocation through the parser (`expat_malloc`,
//! `expat_realloc`, `expat_free`), where an application-supplied allocator
//! (`XML_ParserCreate_MM`) or the allocation tracker
//! (`XML_SetAllocTrackerMaximumAmplification`) can refuse it. A refusal is
//! not fatal: the C code reports `XML_ERROR_NO_MEMORY` and leaves its data
//! structures in a defined state.
//!
//! The port allocates with Rust's global allocator, so a refusal cannot come
//! from there. Instead, the data structures of the parser core announce each
//! allocation the C code would make, with the size C would request, to a
//! [`Heap`]; when the heap refuses, they behave exactly like C after a failed
//! `MALLOC` (D-021). Sizes are those of the C structures on the build target
//! ([`POINTER_SIZE`], [`BLOCK_HEADER_SIZE`]).
//!
//! ```
//! # #[cfg(feature = "internal-api")] {
//! use expat::internal::heap::{Heap, Unlimited};
//! let mut heap = Unlimited;
//! assert!(heap.alloc(1036));
//! heap.free(1036);
//! # }
//! ```

/// Size of a C pointer (`sizeof(NAMED *)`): one slot of a hash table.
pub const POINTER_SIZE: usize = core::mem::size_of::<usize>();

/// `offsetof(BLOCK, s)` for `struct block { struct block *next; int size;
/// XML_Char s[]; }` with a one-byte `XML_Char` (D-002): a pointer and a C
/// `int`, without padding before the character array.
pub const BLOCK_HEADER_SIZE: usize = POINTER_SIZE + 4;

/// The allocation failed (C: `MALLOC`/`REALLOC` returned `NULL`; the parser
/// reports `XML_ERROR_NO_MEMORY`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NoMemory;

impl core::fmt::Display for NoMemory {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("out of memory")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NoMemory {}

/// Receives the allocations the C code would make and may refuse them.
///
/// Sizes are the sizes the C code passes to `MALLOC`/`REALLOC` (without the
/// tracker's own header). `free` is called with the size of the allocation
/// being released, as the tracker records it.
pub trait Heap {
    /// `MALLOC(parser, size)`; `false` makes it fail.
    fn alloc(&mut self, size: usize) -> bool;
    /// `REALLOC(parser, p, size)` of an allocation of `old` bytes; `false`
    /// makes it fail (the allocation then keeps its old size).
    fn realloc(&mut self, old: usize, size: usize) -> bool;
    /// `FREE(parser, p)` of an allocation of `size` bytes.
    fn free(&mut self, size: usize);
}

/// A heap that never refuses and keeps no record.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Unlimited;

impl Heap for Unlimited {
    fn alloc(&mut self, _size: usize) -> bool {
        true
    }
    fn realloc(&mut self, _old: usize, _size: usize) -> bool {
        true
    }
    fn free(&mut self, _size: usize) {}
}
