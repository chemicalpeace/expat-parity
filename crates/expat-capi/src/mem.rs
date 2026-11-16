//! The memory functions of `XML_ParserCreate_MM` driven by the parser's
//! allocation tracker (D-060): every call libexpat makes to `malloc_fcn`,
//! `realloc_fcn` and `free_fcn` is made with libexpat's size, on blocks
//! this module keeps. The parser's data lives in Rust collections; the
//! blocks only carry the program's view of the allocations, so that an
//! allocator that fails, counts or checks for leaks behaves as with
//! libexpat.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;

use expat::MemorySuite;

use crate::Mem;

/// A block the suite returned. Raw pointers are neither `Send` nor `Sync`;
/// the blocks are only handed back to the same suite.
#[derive(Debug, Clone, Copy)]
struct Block(*mut c_void);

// SAFETY: a block is an address the program's allocator returned; this
// module only passes it back to that allocator, from whichever thread uses
// the parser (libexpat parsers are used from one thread at a time).
unsafe impl Send for Block {}

/// The program's memory functions with the blocks allocated through them,
/// by size, and the content model allocated for the next element
/// declaration.
#[derive(Debug)]
pub(crate) struct CSuite {
    mem: Mem,
    blocks: Mutex<HashMap<usize, Vec<Block>>>,
    model: Mutex<Option<(Block, usize)>>,
}

impl CSuite {
    pub(crate) fn new(mem: Mem) -> Self {
        CSuite {
            mem,
            blocks: Mutex::new(HashMap::new()),
            model: Mutex::new(None),
        }
    }

    /// The last block of `size` bytes kept. The size's entry stays, with
    /// its capacity, so that putting the block back cannot allocate.
    fn take(&self, size: usize) -> Option<Block> {
        self.blocks.lock().ok()?.get_mut(&size)?.pop()
    }

    /// Room to keep one more block of `size` bytes, reserved before the
    /// program's allocator is called; `false` when the system allocator
    /// cannot provide it, which the suite reports as a failed call
    /// (D-067).
    fn room(&self, size: usize) -> bool {
        let Ok(mut blocks) = self.blocks.lock() else {
            return false;
        };
        if !blocks.contains_key(&size) {
            if blocks.try_reserve(1).is_err() {
                return false;
            }
            blocks.insert(size, Vec::new());
        }
        blocks
            .get_mut(&size)
            .is_some_and(|v| v.try_reserve(1).is_ok())
    }

    /// Keeps a block, in room [`CSuite::room`] reserved or [`CSuite::take`]
    /// left.
    fn keep(&self, size: usize, b: Block) {
        if let Ok(mut blocks) = self.blocks.lock()
            && let Some(v) = blocks.get_mut(&size)
        {
            v.push(b);
        }
    }

    /// The content model block of `size` bytes allocated for the element
    /// declaration being reported, if any; the program owns it now.
    pub(crate) fn take_model(&self, size: usize) -> Option<*mut c_void> {
        let (b, n) = self.model.lock().ok()?.take()?;
        if n == size {
            return Some(b.0);
        }
        // SAFETY: a block `malloc_fcn` returned, not handed out.
        unsafe { (self.mem.free)(b.0) };
        None
    }
}

impl MemorySuite for CSuite {
    fn malloc(&self, size: usize) -> bool {
        if !self.room(size) {
            return false;
        }
        // SAFETY: the program's `malloc_fcn`, which takes any size.
        let p = unsafe { (self.mem.malloc)(size) };
        if p.is_null() {
            return false;
        }
        self.keep(size, Block(p));
        true
    }

    fn realloc(&self, old: usize, size: usize) -> bool {
        let Some(b) = self.take(old) else {
            // No block of that size (a reset parser's creation): a new one.
            return self.malloc(size);
        };
        if !self.room(size) {
            self.keep(old, b);
            return false;
        }
        // SAFETY: a block the program's allocator returned and this suite
        // kept; on failure it stays valid and is kept again.
        let p = unsafe { (self.mem.realloc)(b.0, size) };
        if p.is_null() {
            self.keep(old, b);
            return false;
        }
        self.keep(size, Block(p));
        true
    }

    fn free(&self, size: usize) {
        if let Some(b) = self.take(size) {
            // SAFETY: a block the program's allocator returned, freed once.
            unsafe { (self.mem.free)(b.0) };
        }
    }

    fn content_model(&self, size: usize) -> bool {
        // SAFETY: the program's `malloc_fcn`, which takes any size.
        let p = unsafe { (self.mem.malloc)(size) };
        if p.is_null() {
            return false;
        }
        if let Ok(mut m) = self.model.lock()
            && let Some((old, _)) = m.replace((Block(p), size))
        {
            // SAFETY: an earlier model block no handler took.
            unsafe { (self.mem.free)(old.0) };
        }
        true
    }
}

impl Drop for CSuite {
    /// `XML_ParserFree`: the blocks still allocated are freed.
    fn drop(&mut self) {
        let blocks = std::mem::take(self.blocks.get_mut().unwrap_or_else(|e| e.into_inner()));
        for b in blocks.into_values().flatten() {
            // SAFETY: blocks the program's allocator returned, freed once.
            unsafe { (self.mem.free)(b.0) };
        }
        if let Some((b, _)) = self.model.get_mut().ok().and_then(Option::take) {
            // SAFETY: as above.
            unsafe { (self.mem.free)(b.0) };
        }
    }
}
