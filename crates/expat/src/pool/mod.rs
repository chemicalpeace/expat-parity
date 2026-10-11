//! The string pool of `xmlparse.c` (`STRING_POOL`).
//!
//! A pool builds one string at a time at its end (the *current* string,
//! `start..ptr`) and keeps the strings it has finished (`poolFinish`) until
//! it is cleared. Storage comes in blocks: the newest block holds the
//! current string; when it is full, `poolGrow` reuses a block from the free
//! list, enlarges the newest block in place (only when the current string
//! starts at its beginning, so no finished string moves), or allocates a new
//! one, always copying the current string to the start of its new home.
//! `poolClear` moves every block to the free list.
//!
//! The port keeps that block structure exactly (D-021): block sizes, the
//! order of both lists, which block is reused or enlarged, and every
//! allocation the C code makes are the same, so allocation failures
//! ([`Heap`]) and the allocation tracker see the same sequence of requests.
//! Instead of pointers, strings are named by [`PoolStr`] handles (block,
//! offset, length) and read with [`StringPool::get`]. A handle stays valid
//! exactly as long as the C pointer would: until the pool is cleared, or, for
//! the current string, until the next operation that can grow the pool.
//! Using a stale handle is a logic error but memory-safe: `get` returns
//! `None` or other pool bytes.
//!
//! `XML_Char` is a UTF-8 byte (D-002); text in other encodings is converted
//! on the way in ([`StringPool::append`]).
//!
//! ```
//! # #[cfg(feature = "internal-api")]
//! # fn main() -> Result<(), expat::internal::heap::NoMemory> {
//! use expat::internal::heap::Unlimited;
//! use expat::internal::pool::StringPool;
//! use expat::internal::tok::Normal;
//!
//! let mut pool = StringPool::new();
//! let mut heap = Unlimited;
//! let doc = pool.copy_string(&mut heap, b"doc")?;
//! // Build a second string from Latin-1 input, then finish it.
//! pool.append(&mut heap, &Normal::Latin1, b"caf\xE9")?;
//! assert_eq!(pool.current(), "café".as_bytes());
//! let cafe = pool.start();
//! pool.finish();
//! assert_eq!(pool.get(doc), Some(&b"doc\0"[..]));
//! assert_eq!(pool.get(cafe), Some("café".as_bytes()));
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "internal-api"))]
//! # fn main() {}
//! ```

// Offsets are bounded by the size of the newest block (`start <= ptr <=
// end`, all at most `i32::MAX`), and every subtraction is of a smaller
// offset from a larger one.
// reason: all arithmetic in this module is bounded as stated above.
#![allow(clippy::arithmetic_side_effects)]

#[cfg(test)]
mod tests;

use alloc::vec;
use alloc::vec::Vec;

use crate::heap::{BLOCK_HEADER_SIZE, Heap, NoMemory};
use crate::tok::{ConvertResult, Encoding};

/// `INIT_BLOCK_SIZE`: size of the first block, in `XML_Char`s.
pub const INIT_BLOCK_SIZE: i32 = 1024;

/// A string in a pool: `len` characters at offset `start` of block `block`
/// (blocks are numbered from the oldest in use, which is also how long
/// they keep their number). The C equivalent is the `XML_Char *` the pool
/// functions return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PoolStr {
    /// Index of the block, counted from the oldest block in use.
    pub block: usize,
    /// Offset of the first character in the block.
    pub start: usize,
    /// Number of characters, including a terminating NUL if one was stored.
    pub len: usize,
}

/// One `BLOCK`: `size` characters of storage.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    /// `s`; its length is the C `size` (an `int`, so at most `i32::MAX`).
    s: Vec<u8>,
}

impl Block {
    /// The C `size` field.
    fn size(&self) -> i32 {
        // The length comes from a positive `i32` (see `grow`).
        i32::try_from(self.s.len()).unwrap_or(i32::MAX)
    }
}

/// `STRING_POOL`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StringPool {
    /// `blocks`, oldest first: the last element is the C list head, the block
    /// that holds the current string.
    blocks: Vec<Block>,
    /// `freeBlocks`: the last element is the C list head.
    free_blocks: Vec<Block>,
    /// `start - blocks->s`; meaningful only when `blocks` is not empty (C:
    /// `start`, `ptr` and `end` are all `NULL` exactly when it is empty).
    start: usize,
    /// `ptr - blocks->s`.
    ptr: usize,
}

/// `poolBytesToAllocateFor`: the size of the allocation that holds a block
/// of `block_size` characters, or 0 when it cannot be represented (the C
/// function computes it in `int`).
pub fn bytes_to_allocate_for(block_size: i32) -> usize {
    if block_size <= 0 {
        return 0;
    }
    // `sizeof(XML_Char)` is 1, so `blockSize * stretch` cannot overflow;
    // C adds the header in `size_t` and converts the sum to `int`.
    let Ok(size) = usize::try_from(block_size) else {
        return 0;
    };
    match BLOCK_HEADER_SIZE.checked_add(size) {
        Some(total) if i32::try_from(total).is_ok() => total,
        _ => 0,
    }
}

impl StringPool {
    /// `poolInit`: an empty pool without blocks.
    pub fn new() -> Self {
        Self::default()
    }

    /// `end - blocks->s`: the size of the newest block (0 without blocks).
    fn end(&self) -> usize {
        self.blocks.last().map_or(0, |b| b.s.len())
    }

    /// Whether `start`/`ptr`/`end` are `NULL`.
    fn is_null(&self) -> bool {
        self.blocks.is_empty()
    }

    /// `poolStart`: the current string (`start..ptr`) as a handle.
    pub fn start(&self) -> PoolStr {
        PoolStr {
            block: self.blocks.len().saturating_sub(1),
            start: self.start,
            len: self.len(),
        }
    }

    /// `poolLength`: characters in the current string.
    pub fn len(&self) -> usize {
        self.ptr.saturating_sub(self.start)
    }

    /// Whether the current string is empty (`poolLength(pool) == 0`).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The characters of the current string.
    pub fn current(&self) -> &[u8] {
        self.blocks
            .last()
            .and_then(|b| b.s.get(self.start..self.ptr))
            .unwrap_or(&[])
    }

    /// The characters of `s`, or `None` if the handle does not name pool
    /// storage (only possible for a stale handle).
    pub fn get(&self, s: PoolStr) -> Option<&[u8]> {
        let end = s.start.checked_add(s.len)?;
        self.blocks.get(s.block)?.s.get(s.start..end)
    }

    /// `poolLastChar`: the last character of the current string. C reads
    /// `ptr[-1]` and is only called on a non-empty string; the port returns
    /// `None` for an empty one.
    pub fn last_char(&self) -> Option<u8> {
        let i = self.ptr.checked_sub(1)?;
        if i < self.start {
            return None;
        }
        self.blocks.last()?.s.get(i).copied()
    }

    /// `poolChop`: drops the last character of the current string. C is only
    /// called on a non-empty string; on an empty one the port does nothing.
    pub fn chop(&mut self) {
        if self.ptr > self.start {
            self.ptr -= 1;
        }
    }

    /// `poolDiscard`: empties the current string.
    pub fn discard(&mut self) {
        self.ptr = self.start;
    }

    /// `poolFinish`: keeps the current string and starts a new, empty one.
    pub fn finish(&mut self) {
        self.start = self.ptr;
    }

    /// `poolClear`: forgets every string and moves all blocks to the free
    /// list (the newest block ends up on top if the free list was empty,
    /// otherwise the oldest, as in C).
    pub fn clear(&mut self) {
        if self.free_blocks.is_empty() {
            self.free_blocks = core::mem::take(&mut self.blocks);
        } else {
            self.free_blocks.extend(self.blocks.drain(..).rev());
        }
        self.start = 0;
        self.ptr = 0;
    }

    /// `poolDestroy`: releases every block, the blocks in use first, each list
    /// from its head.
    pub fn destroy<H: Heap + ?Sized>(self, heap: &mut H) {
        for b in self.blocks.iter().rev().chain(self.free_blocks.iter().rev()) {
            heap.free(bytes_to_allocate_for(b.size()));
        }
    }

    /// `poolGrow`: makes room after the current string (see the module
    /// documentation for the three strategies).
    pub fn grow<H: Heap + ?Sized>(&mut self, heap: &mut H) -> Result<(), NoMemory> {
        let used = self.end().saturating_sub(self.start); // `end - start`
        if let Some(free_size) = self.free_blocks.last().map(Block::size) {
            if self.is_null() {
                if let Some(b) = self.free_blocks.pop() {
                    self.blocks.push(b);
                }
                self.start = 0;
                self.ptr = 0;
                return Ok(());
            }
            if i64::try_from(used).unwrap_or(i64::MAX) < i64::from(free_size) {
                if let Some(mut b) = self.free_blocks.pop() {
                    // C copies `end - start` characters (the current string
                    // and whatever follows it in the old block).
                    let old = self.blocks.last().map_or(&[][..], |o| o.s.as_slice());
                    let src = old.get(self.start..).unwrap_or(&[]);
                    let n = src.len().min(b.s.len());
                    if let (Some(dst), Some(src)) = (b.s.get_mut(..n), src.get(..n)) {
                        dst.copy_from_slice(src);
                    }
                    self.ptr -= self.start;
                    self.start = 0;
                    self.blocks.push(b);
                }
                return Ok(());
            }
        }
        if !self.is_null() && self.start == 0 {
            // Enlarge the newest block in place: it holds only the current
            // string. C: `(int)((unsigned)(end - start) * 2U)`.
            let block_size = u32::try_from(used).unwrap_or(u32::MAX).wrapping_mul(2) as i32;
            if block_size < 0 {
                return Err(NoMemory);
            }
            let bytes = bytes_to_allocate_for(block_size);
            if bytes == 0 {
                return Err(NoMemory);
            }
            let Some(head) = self.blocks.last_mut() else {
                return Err(NoMemory);
            };
            if !heap.realloc(bytes_to_allocate_for(head.size()), bytes) {
                return Err(NoMemory);
            }
            head.s.resize(usize::try_from(block_size).unwrap_or(0), 0);
        } else {
            let Ok(mut block_size) = i32::try_from(used) else {
                return Err(NoMemory);
            };
            if block_size < INIT_BLOCK_SIZE {
                block_size = INIT_BLOCK_SIZE;
            } else {
                block_size = block_size.checked_mul(2).ok_or(NoMemory)?;
            }
            let bytes = bytes_to_allocate_for(block_size);
            if bytes == 0 {
                return Err(NoMemory);
            }
            if !heap.alloc(bytes) {
                return Err(NoMemory);
            }
            let mut b = Block {
                s: vec![0; usize::try_from(block_size).unwrap_or(0)],
            };
            let cur = self.current();
            if let Some(dst) = b.s.get_mut(..cur.len()) {
                dst.copy_from_slice(cur);
            }
            self.ptr -= self.start;
            self.start = 0;
            self.blocks.push(b);
        }
        Ok(())
    }

    /// `poolGrowUntil`: grows until at least `needed` characters fit after
    /// the current string.
    pub fn grow_until<H: Heap + ?Sized>(
        &mut self,
        heap: &mut H,
        needed: usize,
    ) -> Result<(), NoMemory> {
        while self.end().saturating_sub(self.ptr) < needed {
            self.grow(heap)?;
        }
        Ok(())
    }

    /// `poolAppendChar`.
    pub fn append_char<H: Heap + ?Sized>(&mut self, heap: &mut H, c: u8) -> Result<(), NoMemory> {
        if self.ptr == self.end() {
            self.grow(heap)?;
        }
        let slot = self
            .blocks
            .last_mut()
            .and_then(|b| b.s.get_mut(self.ptr))
            .ok_or(NoMemory)?;
        *slot = c;
        self.ptr += 1;
        Ok(())
    }

    /// `poolAppendChars`: appends `s` (which may contain NUL).
    pub fn append_chars<H: Heap + ?Sized>(
        &mut self,
        heap: &mut H,
        s: &[u8],
    ) -> Result<(), NoMemory> {
        self.grow_until(heap, s.len())?;
        if s.is_empty() {
            return Ok(());
        }
        let ptr = self.ptr;
        let dst = self
            .blocks
            .last_mut()
            .and_then(|b| b.s.get_mut(ptr..ptr + s.len()))
            .ok_or(NoMemory)?;
        dst.copy_from_slice(s);
        self.ptr += s.len();
        Ok(())
    }

    /// `poolAppend`: converts `input` from `enc` to UTF-8 and appends it to
    /// the current string, growing as needed; an incomplete character at the
    /// end of `input` is dropped. Returns `poolStart`.
    pub fn append<H: Heap + ?Sized, E: Encoding + ?Sized>(
        &mut self,
        heap: &mut H,
        enc: &E,
        input: &[u8],
    ) -> Result<PoolStr, NoMemory> {
        if self.is_null() {
            self.grow(heap)?;
        }
        let mut from = input;
        loop {
            let ptr = self.ptr;
            let to = self
                .blocks
                .last_mut()
                .and_then(|b| b.s.get_mut(ptr..))
                .ok_or(NoMemory)?;
            let r = enc.to_utf8(from, to);
            from = from.get(r.read..).unwrap_or(&[]);
            self.ptr += r.written;
            if matches!(
                r.result,
                ConvertResult::Completed | ConvertResult::InputIncomplete
            ) {
                break;
            }
            self.grow(heap)?;
        }
        Ok(self.start())
    }

    /// `poolStoreString`: [`append`](Self::append), then a terminating NUL.
    /// The string is not finished.
    pub fn store_string<H: Heap + ?Sized, E: Encoding + ?Sized>(
        &mut self,
        heap: &mut H,
        enc: &E,
        input: &[u8],
    ) -> Result<PoolStr, NoMemory> {
        self.append(heap, enc, input)?;
        self.append_char(heap, 0)?;
        Ok(self.start())
    }

    /// `poolCopyString`: appends `s` and a NUL and finishes the string. Like
    /// the C string it copies, `s` must not contain NUL.
    pub fn copy_string<H: Heap + ?Sized>(
        &mut self,
        heap: &mut H,
        s: &[u8],
    ) -> Result<PoolStr, NoMemory> {
        debug_assert!(!s.contains(&0), "C string without interior NUL");
        self.grow_until(heap, s.len().saturating_add(1))?;
        self.append_chars(heap, s)?;
        self.append_char(heap, 0)?;
        let r = self.start();
        self.finish();
        Ok(r)
    }

    /// `poolCopyStringNoFinish`: appends `s` and a NUL one character at a
    /// time, without finishing; on failure the characters appended so far
    /// are removed again. `s` must not contain NUL.
    pub fn copy_string_no_finish<H: Heap + ?Sized>(
        &mut self,
        heap: &mut H,
        s: &[u8],
    ) -> Result<PoolStr, NoMemory> {
        debug_assert!(!s.contains(&0), "C string without interior NUL");
        for (advanced, &c) in s.iter().chain(core::iter::once(&0)).enumerate() {
            if let Err(e) = self.append_char(heap, c) {
                self.ptr -= advanced;
                return Err(e);
            }
        }
        Ok(self.start())
    }

    /// `poolCopyStringN`: appends `s` (no terminator) and finishes the string.
    pub fn copy_string_n<H: Heap + ?Sized>(
        &mut self,
        heap: &mut H,
        s: &[u8],
    ) -> Result<PoolStr, NoMemory> {
        if self.is_null() {
            self.grow(heap)?;
        }
        if !s.is_empty() {
            self.append_chars(heap, s)?;
        }
        let r = self.start();
        self.finish();
        Ok(r)
    }

    /// `poolAppendString`: appends `s` without a terminator; returns
    /// `poolStart`. `s` must not contain NUL.
    pub fn append_string<H: Heap + ?Sized>(
        &mut self,
        heap: &mut H,
        s: &[u8],
    ) -> Result<PoolStr, NoMemory> {
        debug_assert!(!s.contains(&0), "C string without interior NUL");
        self.append_chars(heap, s)?;
        Ok(self.start())
    }

    /// White-box view for the differential tests: sizes of the blocks in
    /// use (oldest first), sizes of the free blocks (C list head last), and
    /// `start`/`ptr` as offsets into the newest block (`None` when C has
    /// `NULL`).
    pub fn layout(&self) -> PoolLayout {
        PoolLayout {
            blocks: self.blocks.iter().map(Block::size).collect(),
            free_blocks: self.free_blocks.iter().map(Block::size).collect(),
            cursor: (!self.is_null()).then_some((self.start, self.ptr)),
        }
    }
}

/// See [`StringPool::layout`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PoolLayout {
    /// Sizes of the blocks in use, oldest first.
    pub blocks: Vec<i32>,
    /// Sizes of the free blocks, the next to be reused last.
    pub free_blocks: Vec<i32>,
    /// `(start, ptr)` in the newest block, `None` for `NULL`.
    pub cursor: Option<(usize, usize)>,
}
