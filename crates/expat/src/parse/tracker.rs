//! The allocation tracker (`MALLOC_TRACKER`, `expat_heap_increase_tolerable`,
//! `XML_SetAllocTrackerMaximumAmplification`,
//! `XML_SetAllocTrackerActivationThreshold`; `xmlparse.c` 860-1060,
//! 3150-3200).
//!
//! libexpat counts every byte its parser allocates through `MALLOC` and
//! `REALLOC` (`sizeof(size_t) + EXPAT_MALLOC_PADDING + size` each, minus
//! what it frees; the input buffer and the content models handed to the
//! element declaration handler are allocated outside the tracker) and refuses an allocation, reporting `XML_ERROR_NO_MEMORY`, when
//! the new total is at or above the activation threshold (64 MiB) *and*
//! `total / countBytesDirect` exceeds the maximum amplification (100.0).
//!
//! The port does not allocate the C data structures, so it cannot count
//! C's allocations directly. Instead it *charges* the tracker, at the same
//! point of the parse or earlier, at least the bytes C allocates (decision
//! D-030, `docs/DTD-SECURITY.md` section 5):
//!
//! - exactly C's sizes where the port keeps C's structure: the attribute
//!   array, the hash tables (through [`Heap`]),
//!   the string pools (simulated by [`PoolShadow`], which replays C's block
//!   strategy on lengths only), tag buffers and their free list, namespace
//!   bindings, open-entity records, default-attribute arrays and their
//!   per-element hash tables, the content-model scaffold;
//! - an upper bound for the parser creation and for the small fixed
//!   allocations (data buffer, group connector, XML declaration strings):
//!   [`CREATION_CHARGE`].
//!
//! Charging earlier or more than C makes the port at least as strict: the
//! amplification `total / direct` can only be higher at any point of the
//! parse, because both sides count the same direct bytes
//! ([`super::Accounting`]). The differential tests check that the port
//! fails with `XML_ERROR_NO_MEMORY` whenever C does (`tracker_diff.rs`).
//!
//! ```
//! # #[cfg(feature = "internal-api")] {
//! use expat::internal::parse::AllocTracker;
//! let mut t = AllocTracker::default();
//! assert!(t.alloc(1 << 20, 1)); // below the 64 MiB activation threshold
//! t.set_activation_threshold(0);
//! assert!(!t.alloc(1 << 20, 1)); // now the amplification counts
//! # }
//! ```

use super::c_layout;
use crate::heap::{BLOCK_HEADER_SIZE, Heap, NoMemory, POINTER_SIZE};

/// `sizeof(size_t) + EXPAT_MALLOC_PADDING`: the tracker's header per
/// allocation. `EXPAT_MALLOC_ALIGNMENT` is `sizeof(union { long long l;
/// void *p; })`, 8 on every supported target, and the padding fills the
/// header up to it.
pub const MALLOC_HEADER: u64 = c_layout::MALLOC_HEADER as u64;

/// `EXPAT_ALLOC_TRACKER_MAXIMUM_AMPLIFICATION_DEFAULT`.
pub const ALLOC_MAXIMUM_AMPLIFICATION_DEFAULT: f32 = 100.0;

/// `EXPAT_ALLOC_TRACKER_ACTIVATION_THRESHOLD_DEFAULT` (64 MiB).
pub const ALLOC_ACTIVATION_THRESHOLD_DEFAULT: u64 = 67_108_864;

/// Charged when the parser is created: an upper bound of what
/// `parserCreate` allocates (the parser structure, `m_atts`, `m_dataBuf`,
/// the DTD; 3,040 bytes on 64-bit Linux, measured through the oracle).
/// Everything C allocates after creation is replayed allocation by
/// allocation (since stage 4 of D-034 also the implicit namespace
/// context's `m_tempPool` block); the excess over C's creation is the
/// margin stage 3 removes. `tracker_diff.rs` checks it against C's count
/// after creation on every CI platform.
pub const CREATION_CHARGE: u64 = 16_384;

/// `INIT_BLOCK_SIZE` of the string pools.
const INIT_BLOCK_SIZE: i64 = 1024;


/// One change of the tracker's count, for the per-event comparison with
/// libexpat's tracker (D-034): what `expat_heap_stat` reports in C.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackerEvent {
    /// An increase (charge) rather than a decrease (release).
    pub increase: bool,
    /// The increase was refused; the count did not change.
    pub refused: bool,
    /// Bytes of the change.
    pub size: u64,
    /// `bytes_allocated` after the event.
    pub total: u64,
    /// `peak_bytes_allocated` after the event.
    pub peak: u64,
}

/// `MALLOC_TRACKER` of the root parser.
#[derive(Debug, Clone, PartialEq)]
pub struct AllocTracker {
    /// `bytesAllocated`: what the port has charged and not released.
    pub bytes_allocated: u64,
    /// The highest `bytes_allocated` so far.
    pub peak_bytes_allocated: u64,
    /// `maximumAmplificationFactor` (>= 1.0)
    pub maximum_amplification_factor: f32,
    /// `activationThresholdBytes`
    pub activation_threshold_bytes: u64,
    /// Every change of the count while enabled ([`AllocTracker::log_events`]),
    /// for the per-event comparison with C (tests only; `None` normally).
    pub events: Option<alloc::vec::Vec<TrackerEvent>>,
}

impl Default for AllocTracker {
    fn default() -> Self {
        AllocTracker {
            bytes_allocated: 0,
            peak_bytes_allocated: 0,
            maximum_amplification_factor: ALLOC_MAXIMUM_AMPLIFICATION_DEFAULT,
            activation_threshold_bytes: ALLOC_ACTIVATION_THRESHOLD_DEFAULT,
            events: None,
        }
    }
}

impl AllocTracker {
    /// Starts (or stops, discarding them) recording every change of the
    /// count as a [`TrackerEvent`].
    /// The count so far is recorded as one first event (the parser's
    /// creation charge, which C reports as several allocations).
    pub fn log_events(&mut self, on: bool) {
        self.events = on.then(alloc::vec::Vec::new);
        if on && self.bytes_allocated > 0 {
            self.record(true, false, self.bytes_allocated);
        }
    }

    fn record(&mut self, increase: bool, refused: bool, size: u64) {
        let (total, peak) = (self.bytes_allocated, self.peak_bytes_allocated);
        if let Some(e) = self.events.as_mut() {
            e.push(TrackerEvent {
                increase,
                refused,
                size,
                total,
                peak,
            });
        }
    }

    /// Lowers the count by `size` (a release), recorded as an event.
    pub fn release(&mut self, size: u64) {
        self.bytes_allocated = self.bytes_allocated.saturating_sub(size);
        if size > 0 {
            self.record(false, false, size);
        }
    }

    /// `increase_tolerable` that records a refusal.
    fn tolerable(&mut self, increase: u64, direct: u64) -> bool {
        let ok = self.increase_tolerable(increase, direct);
        if !ok {
            self.record(true, true, increase);
        }
        ok
    }

    /// `setAllocTrackerMaximumAmplification` on a root parser: `false`,
    /// and no change, for NaN or a value below 1.0.
    pub fn set_maximum_amplification(&mut self, factor: f32) -> bool {
        if factor.is_nan() || factor < 1.0 {
            return false;
        }
        self.maximum_amplification_factor = factor;
        true
    }

    /// `setAllocTrackerActivationThreshold` on a root parser: any value.
    pub fn set_activation_threshold(&mut self, bytes: u64) -> bool {
        self.activation_threshold_bytes = bytes;
        true
    }

    /// `expat_heap_increase_tolerable` (`xmlparse.c` 860-894) with the root
    /// parser's direct byte count `direct`.
    pub fn increase_tolerable(&self, increase: u64, direct: u64) -> bool {
        let Some(new_total) = self.bytes_allocated.checked_add(increase) else {
            return false;
        };
        if new_total >= self.activation_threshold_bytes {
            // Like C: `(float)newTotal / (float)countBytesDirect`, which is
            // +infinity when nothing was read directly.
            let amplification = new_total as f32 / direct as f32;
            if amplification > self.maximum_amplification_factor {
                return false;
            }
        }
        true
    }

    fn add(&mut self, increase: u64) {
        self.bytes_allocated = self.bytes_allocated.saturating_add(increase);
        if self.bytes_allocated > self.peak_bytes_allocated {
            self.peak_bytes_allocated = self.bytes_allocated;
        }
        self.record(true, false, increase);
    }

    /// Charges `MALLOC(size)`; `false` = C's `expat_malloc` returns
    /// `NULL` (nothing is charged).
    pub fn alloc(&mut self, size: usize, direct: u64) -> bool {
        let Some(increase) = u64::try_from(size)
            .ok()
            .and_then(|s| s.checked_add(MALLOC_HEADER))
        else {
            return false;
        };
        if !self.tolerable(increase, direct) {
            return false;
        }
        self.add(increase);
        true
    }

    /// Charges `REALLOC` of an allocation of `old` bytes to `size` bytes;
    /// `false` = refused (the allocation keeps its size). A decrease is
    /// always granted.
    pub fn realloc(&mut self, old: usize, size: usize, direct: u64) -> bool {
        let (old, size) = (old as u64, size as u64);
        if size > old {
            if !self.tolerable(size - old, direct) {
                return false;
            }
            self.add(size - old);
        } else {
            self.release(old - size);
        }
        true
    }

    /// Releases a charged allocation of `size` bytes (`FREE`).
    pub fn free(&mut self, size: usize) {
        self.release((size as u64).saturating_add(MALLOC_HEADER));
    }

    /// Charges the creation of an external entity parser (the same bound
    /// as a root parser's creation, `CREATION_CHARGE`, plus the encoding
    /// name), checked against the root parser's limits like C's
    /// `parserCreate` with a parent; `false` = refused, nothing charged.
    pub fn charge_child_creation(&mut self, encoding_name_len: Option<usize>, direct: u64) -> bool {
        let increase = CREATION_CHARGE
            .saturating_add(encoding_name_len.map_or(0, |n| (n as u64).saturating_add(1 + MALLOC_HEADER)));
        if !self.tolerable(increase, direct) {
            return false;
        }
        self.add(increase);
        true
    }

    /// Releases what [`AllocTracker::charge_child_creation`] charged
    /// (`XML_ParserFree` of the external entity parser).
    pub fn release_child_creation(&mut self, encoding_name_len: Option<usize>) {
        let charge = CREATION_CHARGE
            .saturating_add(encoding_name_len.map_or(0, |n| (n as u64).saturating_add(1 + MALLOC_HEADER)));
        self.release(charge);
    }

    /// Charges the parser creation (`CREATION_CHARGE` plus the copy of
    /// the protocol encoding name, `copyString`). Never refused: C checks
    /// nothing for a root parser's own structure.
    pub fn charge_creation(&mut self, encoding_name_len: Option<usize>) {
        self.add(CREATION_CHARGE);
        if let Some(n) = encoding_name_len {
            self.add((n as u64).saturating_add(1 + MALLOC_HEADER));
        }
    }
}

/// The tracker as a [`Heap`] for the port's hash tables and pool shadows:
/// every request is checked against the root parser's direct byte count.
#[derive(Debug)]
pub struct TrackerHeap<'a> {
    /// The tracker charged.
    pub tracker: &'a mut AllocTracker,
    /// `countBytesDirect` of the root parser at this point.
    pub direct: u64,
}

impl Heap for TrackerHeap<'_> {
    fn alloc(&mut self, size: usize) -> bool {
        self.tracker.alloc(size, self.direct)
    }
    fn realloc(&mut self, old: usize, size: usize) -> bool {
        self.tracker.realloc(old, size, self.direct)
    }
    fn free(&mut self, size: usize) {
        self.tracker.free(size);
    }
}

/// `poolBytesToAllocateFor`: `offsetof(BLOCK, s) + blockSize`.
fn block_bytes(size: i64) -> usize {
    usize::try_from(size).unwrap_or(0).saturating_add(BLOCK_HEADER_SIZE)
}

/// The block structure of one C `STRING_POOL` without its characters:
/// sizes of the blocks in use (newest last) and of the free blocks (C list
/// head last), and the current string as offsets into the newest block.
/// `grow` is `poolGrow` (`xmlparse.c` 8560-8662) on these numbers, so the
/// allocations it charges are C's (D-021 verifies the same strategy with
/// characters, `crate::pool::StringPool`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PoolShadow {
    blocks: alloc::vec::Vec<i64>,
    free_blocks: alloc::vec::Vec<i64>,
    /// `start - blocks->s` (meaningful when `blocks` is not empty).
    start: i64,
    /// `ptr - blocks->s`
    ptr: i64,
}

impl PoolShadow {
    fn end(&self) -> i64 {
        self.blocks.last().copied().unwrap_or(0)
    }

    /// Bytes of all blocks in use and free (for tests).
    pub fn total_block_bytes(&self) -> usize {
        self.blocks
            .iter()
            .chain(self.free_blocks.iter())
            .map(|&s| block_bytes(s))
            .sum()
    }

    /// Bytes charged for the blocks in use and free: each block's size
    /// plus its `MALLOC_HEADER` (what `poolDestroy` releases).
    pub fn charged_bytes(&self) -> u64 {
        let blocks = (self.blocks.len() + self.free_blocks.len()) as u64;
        (self.total_block_bytes() as u64).saturating_add(blocks.saturating_mul(MALLOC_HEADER))
    }

    /// `poolGrow`.
    fn grow<H: Heap + ?Sized>(&mut self, heap: &mut H) -> Result<(), NoMemory> {
        let used = self.end() - self.start;
        if let Some(&free_size) = self.free_blocks.last() {
            if self.blocks.is_empty() {
                // `pool->start == NULL`
                self.free_blocks.pop();
                self.blocks.push(free_size);
                self.start = 0;
                self.ptr = 0;
                return Ok(());
            }
            if used < free_size {
                self.free_blocks.pop();
                self.blocks.push(free_size);
                self.ptr -= self.start;
                self.start = 0;
                return Ok(());
            }
        }
        if !self.blocks.is_empty() && self.start == 0 {
            let block_size = used.checked_mul(2).filter(|&s| s <= i64::from(i32::MAX)).ok_or(NoMemory)?;
            let old = self.end();
            if !heap.realloc(block_bytes(old), block_bytes(block_size)) {
                return Err(NoMemory);
            }
            if let Some(b) = self.blocks.last_mut() {
                *b = block_size;
            }
        } else {
            let block_size = if used < INIT_BLOCK_SIZE {
                INIT_BLOCK_SIZE
            } else {
                used.checked_mul(2).filter(|&s| s <= i64::from(i32::MAX)).ok_or(NoMemory)?
            };
            if !heap.alloc(block_bytes(block_size)) {
                return Err(NoMemory);
            }
            self.ptr -= self.start;
            self.start = 0;
            self.blocks.push(block_size);
        }
        Ok(())
    }

    /// `poolAppend` of a converted text of `text.len()` UTF-8 bytes: an
    /// empty pool grows first, even for an empty text (`! pool->ptr`), then
    /// the pool grows while the rest of the text does not fit.
    ///
    /// `XmlConvert` writes whole characters only and stops up to three
    /// bytes before the end of a block when the next character does not
    /// fit, but that changes no allocation: it reports
    /// `XML_CONVERT_OUTPUT_EXHAUSTED` exactly when the rest of the text is
    /// longer than the room left (`utf8_toUtf8` compares the lengths; the
    /// Latin-1, ASCII and UTF-16 converters stop at the first character
    /// that does not fit, which exists exactly then), and `poolGrow` sizes
    /// the new block from `end - start`, not from where the conversion
    /// stopped; the characters left over are written after the growth.
    /// The shadow therefore needs only the length (D-034 stage 2; the unit
    /// test checks it against the character-level pool on multi-byte
    /// texts).
    pub fn append_text<H: Heap + ?Sized>(&mut self, heap: &mut H, text: &[u8]) -> Result<(), NoMemory> {
        if self.blocks.is_empty() {
            self.grow(heap)?;
        }
        self.append_chars(heap, text.len())
    }

    /// `poolAppendChars` (`poolGrowUntil`, then the copy) of `n`
    /// characters; the same allocations as `n` calls of `poolAppendChar`.
    pub fn append_chars<H: Heap + ?Sized>(&mut self, heap: &mut H, n: usize) -> Result<(), NoMemory> {
        let n = i64::try_from(n).map_err(|_| NoMemory)?;
        while self.end() - self.ptr < n {
            self.grow(heap)?;
        }
        self.ptr += n;
        Ok(())
    }

    /// `poolChop`: drops the last character of the current string.
    pub fn chop(&mut self) {
        if self.ptr > self.start {
            self.ptr -= 1;
        }
    }

    /// `poolFinish`.
    pub fn finish(&mut self) {
        self.start = self.ptr;
    }

    /// `poolDiscard`.
    pub fn discard(&mut self) {
        self.ptr = self.start;
    }

    /// `poolStoreString` + `poolFinish`: the converted `text`, then its NUL.
    pub fn store_text<H: Heap + ?Sized>(&mut self, heap: &mut H, text: &[u8]) -> Result<(), NoMemory> {
        self.append_text(heap, text)?;
        self.append_chars(heap, 1)?;
        self.finish();
        Ok(())
    }

    /// `poolStoreString` + `poolDiscard` (a name looked up in a table and
    /// found there, or not kept).
    pub fn store_text_discard<H: Heap + ?Sized>(&mut self, heap: &mut H, text: &[u8]) -> Result<(), NoMemory> {
        self.append_text(heap, text)?;
        self.append_chars(heap, 1)?;
        self.discard();
        Ok(())
    }

    /// `poolCopyString`: `n` characters and a NUL, finished.
    pub fn copy<H: Heap + ?Sized>(&mut self, heap: &mut H, n: usize) -> Result<(), NoMemory> {
        self.append_chars(heap, n.saturating_add(1))?;
        self.finish();
        Ok(())
    }

    /// `poolCopyStringNoFinish` (or `n` characters and a NUL appended one
    /// by one) + `poolDiscard`.
    pub fn copy_discard<H: Heap + ?Sized>(&mut self, heap: &mut H, n: usize) -> Result<(), NoMemory> {
        self.append_chars(heap, n.saturating_add(1))?;
        self.discard();
        Ok(())
    }

    /// `poolCopyStringN`: `n` characters without a NUL, finished; an empty
    /// pool grows first even for `n == 0`.
    pub fn copy_n<H: Heap + ?Sized>(&mut self, heap: &mut H, n: usize) -> Result<(), NoMemory> {
        if self.blocks.is_empty() {
            self.grow(heap)?;
        }
        self.append_chars(heap, n)?;
        self.finish();
        Ok(())
    }

    /// `poolClear`: every block goes to the free list (the newest first, so
    /// the oldest ends up at the head), nothing is freed.
    pub fn clear(&mut self) {
        if self.free_blocks.is_empty() {
            // C: `pool->freeBlocks = pool->blocks` (list order kept).
            self.free_blocks = core::mem::take(&mut self.blocks);
        } else {
            // C moves the blocks in use onto the free list from the newest:
            // the oldest ends up at the head (vectors keep the head last).
            self.free_blocks.extend(self.blocks.drain(..).rev());
        }
        self.start = 0;
        self.ptr = 0;
    }
}

/// A C `HASH_TABLE`'s allocations without its entries: `power` and
/// `used` (`lookup`, `xmlparse.c` 8248-8335).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TableShadow {
    power: u32,
    used: usize,
}

impl TableShadow {
    /// Charges the insertion of a new entry of `create_size` bytes.
    pub fn insert<H: Heap + ?Sized>(&mut self, heap: &mut H, create_size: usize) -> Result<(), NoMemory> {
        if self.power == 0 {
            if !heap.alloc((1usize << 6) * POINTER_SIZE) {
                return Err(NoMemory);
            }
            self.power = 6;
        } else if self.used >> (self.power - 1) != 0 {
            let new_power = self.power + 1;
            if !heap.alloc((1usize << new_power) * POINTER_SIZE) {
                return Err(NoMemory);
            }
            heap.free((1usize << self.power) * POINTER_SIZE);
            self.power = new_power;
        }
        if !heap.alloc(create_size) {
            return Err(NoMemory);
        }
        self.used += 1;
        Ok(())
    }
}

/// `sizeof(TAG)` ([`c_layout::TAG`]).
pub const TAG_SIZE: usize = c_layout::TAG;
/// `INIT_TAG_BUF_SIZE`
pub const INIT_TAG_BUF_SIZE: usize = 32;
/// `sizeof(BINDING)` ([`c_layout::BINDING`]: five pointers and two
/// `size_t`; 56 bytes on 64-bit targets). Before D-034 the port charged
/// six pointers, 8 bytes less than C per binding (DIVERGENCES.md R1).
pub const BINDING_SIZE: usize = c_layout::BINDING;
/// `EXPAND_SPARE`
pub const EXPAND_SPARE: usize = 24;
/// `sizeof(OPEN_INTERNAL_ENTITY)` ([`c_layout::OPEN_INTERNAL_ENTITY`]).
pub const OPEN_ENTITY_SIZE: usize = c_layout::OPEN_INTERNAL_ENTITY;
/// `sizeof(ATTRIBUTE)` ([`c_layout::ATTRIBUTE`]).
pub const ATTRIBUTE_SIZE: usize = c_layout::ATTRIBUTE;
/// `sizeof(NS_ATT)` ([`c_layout::NS_ATT`]: two `unsigned long`s and a
/// pointer; 24 bytes on 64-bit Unix, 16 on Windows x64). Before stage 6 of
/// D-034 the port charged three 8-byte words everywhere.
pub const NS_ATT_SIZE: usize = c_layout::NS_ATT;
/// `sizeof(DEFAULT_ATTRIBUTE)` ([`c_layout::DEFAULT_ATTRIBUTE`]).
pub const DEFAULT_ATTRIBUTE_SIZE: usize = c_layout::DEFAULT_ATTRIBUTE;
/// `sizeof(NAME_AND_DEFAULT_ATTRIBUTE)`
/// ([`c_layout::NAME_AND_DEFAULT_ATTRIBUTE`]: a pointer, `size_t attIndex`
/// and `bool initialized`; 24 bytes on 64-bit targets). Before D-034 the
/// port charged two pointers, 8 bytes less than C per entry
/// (DIVERGENCES.md R1).
pub const NAME_AND_DEFAULT_ATTRIBUTE_SIZE: usize = c_layout::NAME_AND_DEFAULT_ATTRIBUTE;
/// `sizeof(CONTENT_SCAFFOLD)` ([`c_layout::CONTENT_SCAFFOLD`]).
pub const CONTENT_SCAFFOLD_SIZE: usize = c_layout::CONTENT_SCAFFOLD;
/// `INIT_SCAFFOLD_ELEMENTS`
pub const INIT_SCAFFOLD_ELEMENTS: usize = 32;

/// Every structure size the tracker charges, by C name, for the
/// comparison with the C compiler's `sizeof` / `offsetof`
/// (`tests/tests/tracker_sizes.rs`, on every CI platform).
pub const TRACKER_SIZES: &[(&str, usize)] = &[
    ("TAG", TAG_SIZE),
    ("BINDING", BINDING_SIZE),
    ("PREFIX", super::dtd::PREFIX_SIZE),
    ("ENTITY", super::dtd::ENTITY_SIZE),
    ("CONTENT_SCAFFOLD", CONTENT_SCAFFOLD_SIZE),
    ("BLOCK_HEADER", BLOCK_HEADER_SIZE),
    ("ATTRIBUTE_ID", super::dtd::ATTRIBUTE_ID_SIZE),
    ("DEFAULT_ATTRIBUTE", DEFAULT_ATTRIBUTE_SIZE),
    ("NAME_AND_DEFAULT_ATTRIBUTE", NAME_AND_DEFAULT_ATTRIBUTE_SIZE),
    ("NS_ATT", NS_ATT_SIZE),
    ("ELEMENT_TYPE", super::dtd::ELEMENT_TYPE_SIZE),
    ("OPEN_INTERNAL_ENTITY", OPEN_ENTITY_SIZE),
    ("ATTRIBUTE", ATTRIBUTE_SIZE),
    ("NAMED_PTR", POINTER_SIZE),
    ("MALLOC_HEADER", MALLOC_HEADER as usize),
    ("INT", core::mem::size_of::<i32>()),
];

/// The tracker with the shadows of C's structures whose allocations depend
/// on the input.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tracker {
    /// The byte count and the limits.
    pub t: AllocTracker,
    /// `m_tempPool`
    pub temp_pool: PoolShadow,
    /// `m_temp2Pool`
    pub temp2_pool: PoolShadow,
    /// `dtd->pool`
    pub dtd_pool: PoolShadow,
    /// `dtd->entityValuePool`
    pub entity_value_pool: PoolShadow,
    /// Buffer sizes of the open tags (`m_tagStack`), innermost last.
    pub open_tags: alloc::vec::Vec<usize>,
    /// Buffer sizes of `m_freeTagList`, head last.
    pub free_tags: alloc::vec::Vec<usize>,
    /// `uriAlloc` of each binding, by binding id.
    pub binding_alloc: alloc::vec::Vec<usize>,
    /// `m_nsAttsPower`
    pub ns_atts_power: u32,
    /// Allocated `CONTENT_SCAFFOLD` elements (`dtd->scaffSize`).
    pub scaff_size: usize,
    /// Allocated `dtd->scaffIndex` ints (`dtd->scaffIndexSize`).
    pub scaff_index_size: usize,
    /// `m_groupSize`
    pub group_size: usize,
}

impl Tracker {
    /// A [`Heap`] over the tracker for the direct byte count `direct`.
    pub fn heap(&mut self, direct: u64) -> TrackerHeap<'_> {
        TrackerHeap {
            tracker: &mut self.t,
            direct,
        }
    }

    /// A pool shadow and a heap over the tracker at the same time.
    pub fn pool(&mut self, which: Pool, direct: u64) -> (&mut PoolShadow, TrackerHeap<'_>) {
        let p = match which {
            Pool::Temp => &mut self.temp_pool,
            Pool::Temp2 => &mut self.temp2_pool,
            Pool::Dtd => &mut self.dtd_pool,
            Pool::EntityValue => &mut self.entity_value_pool,
        };
        (
            p,
            TrackerHeap {
                tracker: &mut self.t,
                direct,
            },
        )
    }

    /// `poolStoreString` + `poolFinish` of the converted `text` in `which`.
    pub fn store_text(&mut self, which: Pool, text: &[u8], direct: u64) -> Result<(), NoMemory> {
        let (p, mut h) = self.pool(which, direct);
        p.store_text(&mut h, text)
    }

    /// `poolStoreString` + `poolDiscard` of the converted `text` in `which`.
    pub fn store_text_discard(&mut self, which: Pool, text: &[u8], direct: u64) -> Result<(), NoMemory> {
        let (p, mut h) = self.pool(which, direct);
        p.store_text_discard(&mut h, text)
    }

    /// `poolCopyString` of `n` characters in `which`.
    pub fn copy(&mut self, which: Pool, n: usize, direct: u64) -> Result<(), NoMemory> {
        let (p, mut h) = self.pool(which, direct);
        p.copy(&mut h, n)
    }

    /// `n` characters and a NUL appended to `which`, then discarded.
    pub fn copy_discard(&mut self, which: Pool, n: usize, direct: u64) -> Result<(), NoMemory> {
        let (p, mut h) = self.pool(which, direct);
        p.copy_discard(&mut h, n)
    }

    /// Bytes charged for the structures a parser owns itself and C's
    /// `XML_ParserFree` releases, beyond its creation charge: `m_tempPool`,
    /// `m_temp2Pool`, the tag stack and free tag list, the bindings,
    /// `m_nsAtts` and `m_groupConnector`. Not included: the DTD's pools,
    /// tables and scaffold, which an external parameter entity parser
    /// shares with its parent (D-032).
    pub fn own_bytes(&self) -> u64 {
        let h = MALLOC_HEADER;
        let sz = |n: usize| n as u64;
        let tags = self
            .open_tags
            .iter()
            .chain(self.free_tags.iter())
            .map(|&b| sz(TAG_SIZE) + h + sz(b) + h)
            .sum::<u64>();
        let bindings = self
            .binding_alloc
            .iter()
            .filter(|&&a| a > 0)
            .map(|&a| sz(BINDING_SIZE) + h + sz(a) + h)
            .sum::<u64>();
        let ns_atts = if self.ns_atts_power == 0 {
            0
        } else {
            sz((1usize << self.ns_atts_power) * NS_ATT_SIZE) + h
        };
        let group = if self.group_size == 0 { 0 } else { sz(self.group_size) + h };
        self.temp_pool
            .charged_bytes()
            .saturating_add(self.temp2_pool.charged_bytes())
            .saturating_add(tags)
            .saturating_add(bindings)
            .saturating_add(ns_atts)
            .saturating_add(group)
    }

    /// The start tag of an element with a converted name of `name_len`
    /// bytes and a raw name of `raw_len` bytes: a `TAG` from the free list
    /// or a new one, its buffer doubled until the name fits
    /// (`doContent`, `xmlparse.c` 3750-3793), and enlarged to also hold
    /// the raw name, as `storeRawNames` does when the input buffer moves
    /// (3395-3439; charged at once, which is never later than C).
    pub fn open_tag(&mut self, name_len: usize, raw_len: usize, direct: u64) -> Result<(), NoMemory> {
        let mut size = match self.free_tags.pop() {
            Some(s) => s,
            None => {
                if !self.t.alloc(TAG_SIZE, direct) {
                    return Err(NoMemory);
                }
                if !self.t.alloc(INIT_TAG_BUF_SIZE, direct) {
                    self.t.free(TAG_SIZE);
                    return Err(NoMemory);
                }
                INIT_TAG_BUF_SIZE
            }
        };
        // XmlConvert writes into `buf .. bufEnd - 1` and may stop three
        // bytes early before a multi-byte character.
        while size < name_len.saturating_add(4) {
            let new = size.saturating_mul(2);
            if !self.t.realloc(size, new, direct) {
                self.free_tags.push(size);
                return Err(NoMemory);
            }
            size = new;
        }
        let raw_need = name_len.saturating_add(1).saturating_add(raw_len);
        if raw_need > size {
            if !self.t.realloc(size, raw_need, direct) {
                self.free_tags.push(size);
                return Err(NoMemory);
            }
            size = raw_need;
        }
        self.open_tags.push(size);
        Ok(())
    }

    /// The end tag of the innermost element: its `TAG` goes to the free
    /// list.
    pub fn close_tag(&mut self) {
        if let Some(s) = self.open_tags.pop() {
            self.free_tags.push(s);
        }
    }

    /// `addBinding`'s allocation for a URI of `len` characters (separator
    /// included): a binding from the free list (`reused`, whose buffer is
    /// enlarged when too small) or a new `BINDING` with a new buffer.
    pub fn binding(&mut self, id: usize, reused: bool, len: usize, direct: u64) -> Result<(), NoMemory> {
        let need = len.saturating_add(EXPAND_SPARE);
        if reused {
            let alloc = self.binding_alloc.get(id).copied().unwrap_or(0);
            if len > alloc {
                if !self.t.realloc(alloc, need, direct) {
                    return Err(NoMemory);
                }
                self.set_binding_alloc(id, need);
            }
            return Ok(());
        }
        if !self.t.alloc(BINDING_SIZE, direct) {
            return Err(NoMemory);
        }
        if !self.t.alloc(need, direct) {
            self.t.free(BINDING_SIZE);
            return Err(NoMemory);
        }
        self.set_binding_alloc(id, need);
        Ok(())
    }

    fn set_binding_alloc(&mut self, id: usize, size: usize) {
        if self.binding_alloc.len() <= id {
            self.binding_alloc.resize(id + 1, 0);
        }
        if let Some(a) = self.binding_alloc.get_mut(id) {
            *a = size;
        }
    }

    /// The expanded element name stored in the buffer of binding `id`
    /// (`storeAtts`, `xmlparse.c` 4517-4537): a new buffer when the total
    /// length does not fit.
    pub fn binding_expand(&mut self, id: usize, total_len: usize, direct: u64) -> Result<(), NoMemory> {
        let alloc = self.binding_alloc.get(id).copied().unwrap_or(0);
        if total_len > alloc {
            let need = total_len.saturating_add(EXPAND_SPARE);
            if !self.t.alloc(need, direct) {
                return Err(NoMemory);
            }
            self.t.free(alloc);
            self.set_binding_alloc(id, need);
        }
        Ok(())
    }

    /// `m_nsAtts` for `n_prefixes` prefixed attributes
    /// (`xmlparse.c` 4328-4365).
    pub fn ns_atts(&mut self, n_prefixes: usize, direct: u64) -> Result<(), NoMemory> {
        if self.ns_atts_power == 0 || (n_prefixes >> (self.ns_atts_power - 1)) != 0 {
            let old = if self.ns_atts_power == 0 {
                None
            } else {
                Some((1usize << self.ns_atts_power) * NS_ATT_SIZE)
            };
            let mut power = self.ns_atts_power;
            loop {
                let more = (n_prefixes >> power) != 0;
                power += 1;
                if !more {
                    break;
                }
            }
            power = power.max(3);
            if power >= 32 {
                return Err(NoMemory);
            }
            let new = (1usize << power) * NS_ATT_SIZE;
            let ok = match old {
                Some(o) => self.t.realloc(o, new, direct),
                None => self.t.alloc(new, direct),
            };
            if !ok {
                return Err(NoMemory);
            }
            self.ns_atts_power = power;
        }
        Ok(())
    }

    /// One more content-model scaffold element (`nextScaffoldPart`,
    /// `xmlparse.c` 8683-8730) when `count` elements are in use: the
    /// first call allocates `dtd->scaffIndex` with `m_groupSize` ints.
    pub fn scaffold_part(&mut self, count: usize, direct: u64) -> Result<(), NoMemory> {
        let ints = core::mem::size_of::<i32>();
        if self.scaff_index_size == 0 {
            if !self.t.alloc(self.group_size.saturating_mul(ints), direct) {
                return Err(NoMemory);
            }
            self.scaff_index_size = self.group_size;
        }
        if count >= self.scaff_size {
            let ok = if self.scaff_size == 0 {
                self.t
                    .alloc(INIT_SCAFFOLD_ELEMENTS * CONTENT_SCAFFOLD_SIZE, direct)
            } else {
                let old = self.scaff_size.saturating_mul(CONTENT_SCAFFOLD_SIZE);
                self.t.realloc(old, old.saturating_mul(2), direct)
            };
            if !ok {
                return Err(NoMemory);
            }
            self.scaff_size = if self.scaff_size == 0 {
                INIT_SCAFFOLD_ELEMENTS
            } else {
                self.scaff_size.saturating_mul(2)
            };
        }
        Ok(())
    }

    /// `dtd->scaffIndex` for scaffold nesting level `level`
    /// (`xmlparse.c` 6271-6284).
    pub fn scaffold_level(&mut self, level: usize, direct: u64) -> Result<(), NoMemory> {
        let ints = core::mem::size_of::<i32>();
        while self.scaff_index_size > 0 && level >= self.scaff_index_size {
            let old = self.scaff_index_size.saturating_mul(ints);
            if !self.t.realloc(old, old.saturating_mul(2), direct) {
                return Err(NoMemory);
            }
            self.scaff_index_size = self.scaff_index_size.saturating_mul(2);
        }
        Ok(())
    }

    /// `m_groupConnector` for group nesting level `level`
    /// (`xmlparse.c` 6243-6264).
    pub fn group(&mut self, level: usize, direct: u64) -> Result<(), NoMemory> {
        while level >= self.group_size {
            let ok = if self.group_size == 0 {
                self.t.alloc(32, direct)
            } else {
                self.t
                    .realloc(self.group_size, self.group_size.saturating_mul(2), direct)
            };
            if !ok {
                return Err(NoMemory);
            }
            self.group_size = if self.group_size == 0 {
                32
            } else {
                self.group_size.saturating_mul(2)
            };
        }
        Ok(())
    }
}

/// The C string pools the port simulates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pool {
    /// `m_tempPool`: attribute values of a start tag, processing
    /// instructions, comments, declaration strings; cleared after each.
    Temp,
    /// `m_temp2Pool`: the XML declaration's strings, names looked up while
    /// an attribute value is normalised.
    Temp2,
    /// `dtd->pool`: names and default values (never cleared during a
    /// parse).
    Dtd,
    /// `dtd->entityValuePool`: replacement texts of internal entities.
    EntityValue,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::StringPool;

    /// A heap that records requests, to compare the shadow with the real
    /// pool.
    #[derive(Default)]
    struct Rec(alloc::vec::Vec<(char, usize, usize)>);
    impl Heap for Rec {
        fn alloc(&mut self, size: usize) -> bool {
            self.0.push(('m', 0, size));
            true
        }
        fn realloc(&mut self, old: usize, size: usize) -> bool {
            self.0.push(('r', old, size));
            true
        }
        fn free(&mut self, size: usize) {
            self.0.push(('f', size, 0));
        }
    }

    #[test]
    fn shadow_allocates_exactly_like_the_string_pool() -> Result<(), NoMemory> {
        // Every pool operation the shadow replays, on random texts with
        // one- to four-byte characters (UTF-8 and Latin-1 input) and in a
        // deterministic mix with finish, discard, chop and clear: the
        // shadow's allocation requests must be the real pool's (D-021's
        // port of C's pool), request by request (D-034 stage 2).
        use crate::tok::Normal;
        let mut seed: u32 = 12345;
        let mut next = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            seed >> 8
        };
        let mut pool = StringPool::new();
        let mut shadow = PoolShadow::default();
        let mut hp = Rec::default();
        let mut hs = Rec::default();
        for step in 0..6000 {
            let op = next() % 14;
            let chars = match next() % 4 {
                0 => (next() % 8) as usize,
                1 => (next() % 600) as usize,
                2 => (next() % 3000) as usize,
                _ => (next() % 12_000) as usize,
            };
            // A UTF-8 text and a Latin-1 input with its UTF-8 conversion.
            let mut text = alloc::vec::Vec::new();
            let mut latin1 = alloc::vec::Vec::new();
            let mut latin1_utf8 = alloc::vec::Vec::new();
            for _ in 0..chars {
                let c = match next() % 8 {
                    0 => 0xE9,
                    1 => 0x20AC,
                    2 => 0x1_F600,
                    _ => 0x61 + next() % 26,
                };
                let mut b = [0u8; 4];
                text.extend_from_slice(char::from_u32(c).unwrap_or('x').encode_utf8(&mut b).as_bytes());
                let l = if c < 0x100 { c as u8 } else { 0xFC };
                latin1.push(l);
                latin1_utf8.extend_from_slice(char::from(l).encode_utf8(&mut b).as_bytes());
            }
            match op {
                0..=3 => {
                    pool.store_string(&mut hp, &Normal::Utf8, &text)?;
                    pool.finish();
                    shadow.store_text(&mut hs, &text)?;
                }
                4 => {
                    pool.store_string(&mut hp, &Normal::Latin1, &latin1)?;
                    pool.discard();
                    shadow.store_text_discard(&mut hs, &latin1_utf8)?;
                }
                5 => {
                    pool.append(&mut hp, &Normal::Utf8, &text)?;
                    shadow.append_text(&mut hs, &text)?;
                }
                6 => {
                    let ascii = alloc::vec![b'y'; chars];
                    pool.copy_string(&mut hp, &ascii)?;
                    shadow.copy(&mut hs, chars)?;
                }
                7 => {
                    let ascii = alloc::vec![b'y'; chars];
                    pool.copy_string_no_finish(&mut hp, &ascii)?;
                    pool.discard();
                    shadow.copy_discard(&mut hs, chars)?;
                }
                8 => {
                    let ascii = alloc::vec![b'y'; chars];
                    pool.copy_string_n(&mut hp, &ascii)?;
                    shadow.copy_n(&mut hs, chars)?;
                }
                9 => {
                    let ascii = alloc::vec![b'z'; chars % 5];
                    pool.append_chars(&mut hp, &ascii)?;
                    shadow.append_chars(&mut hs, chars % 5)?;
                }
                10 => {
                    pool.chop();
                    shadow.chop();
                }
                11 => {
                    pool.append_char(&mut hp, 0)?;
                    pool.finish();
                    shadow.append_chars(&mut hs, 1)?;
                    shadow.finish();
                }
                _ => {
                    pool.clear();
                    shadow.clear();
                }
            }
            assert_eq!(hs.0, hp.0, "step {step} op {op}");
        }
        Ok(())
    }

    #[test]
    fn tolerance_matches_c_formula() {
        let mut t = AllocTracker::default();
        t.set_activation_threshold(1000);
        t.set_maximum_amplification(2.0);
        assert!(t.alloc(900, 10)); // 908 < 1000
        assert!(!t.alloc(100, 10)); // 1016 >= 1000 and 101.6 > 2
        assert!(t.alloc(100, 600)); // 1016 / 600 <= 2
        assert!(!t.set_maximum_amplification(f32::NAN));
        assert!(!t.set_maximum_amplification(0.5));
        assert_eq!(t.maximum_amplification_factor, 2.0);
        // Nothing read directly: the amplification is infinite.
        let mut z = AllocTracker::default();
        z.set_activation_threshold(0);
        assert!(!z.alloc(1, 0));
        // Overflow is refused.
        let mut o = AllocTracker {
            bytes_allocated: u64::MAX - 4,
            ..AllocTracker::default()
        };
        assert!(!o.alloc(1, 1));
    }
}
