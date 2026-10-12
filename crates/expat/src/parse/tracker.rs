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

use crate::heap::{BLOCK_HEADER_SIZE, Heap, NoMemory, POINTER_SIZE};

/// `sizeof(size_t) + EXPAT_MALLOC_PADDING`: the tracker's header per
/// allocation. `EXPAT_MALLOC_ALIGNMENT` is `sizeof(union { long long l;
/// void *p; })`, 8 on every supported target, and the padding fills the
/// header up to it.
pub const MALLOC_HEADER: u64 = 8;

/// `EXPAT_ALLOC_TRACKER_MAXIMUM_AMPLIFICATION_DEFAULT`.
pub const ALLOC_MAXIMUM_AMPLIFICATION_DEFAULT: f32 = 100.0;

/// `EXPAT_ALLOC_TRACKER_ACTIVATION_THRESHOLD_DEFAULT` (64 MiB).
pub const ALLOC_ACTIVATION_THRESHOLD_DEFAULT: u64 = 67_108_864;

/// Charged when the parser is created: an upper bound of what
/// `parserCreate` allocates (the parser structure, `m_atts`, `m_dataBuf`,
/// the DTD, the implicit namespace context; 3,040 bytes on 64-bit Linux,
/// measured through the oracle) plus the small fixed allocations C makes
/// later that the port does not replay one by one (`m_groupConnector`'s
/// first 32 bytes, the XML declaration's version and encoding in
/// `m_temp2Pool`, `m_unknownEncodingMem`). `tracker_diff.rs` checks it
/// against C's count after creation on every CI platform.
pub const CREATION_CHARGE: u64 = 16_384;

/// `INIT_BLOCK_SIZE` of the string pools.
const INIT_BLOCK_SIZE: i64 = 1024;

/// Characters a pool string is assumed to need beyond its final length:
/// C converts into the pool with `XmlConvert`, which stops up to three
/// bytes before the end of a block when the next UTF-8 character does not
/// fit, and `storeAttributeValue` may append a trailing space that it
/// chops afterwards. Growing a little earlier than C can only make the
/// simulated pool larger.
const POOL_SLACK: i64 = 4;

/// `MALLOC_TRACKER` of the root parser.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AllocTracker {
    /// `bytesAllocated`: what the port has charged and not released.
    pub bytes_allocated: u64,
    /// The highest `bytes_allocated` so far.
    pub peak_bytes_allocated: u64,
    /// `maximumAmplificationFactor` (>= 1.0)
    pub maximum_amplification_factor: f32,
    /// `activationThresholdBytes`
    pub activation_threshold_bytes: u64,
}

impl Default for AllocTracker {
    fn default() -> Self {
        AllocTracker {
            bytes_allocated: 0,
            peak_bytes_allocated: 0,
            maximum_amplification_factor: ALLOC_MAXIMUM_AMPLIFICATION_DEFAULT,
            activation_threshold_bytes: ALLOC_ACTIVATION_THRESHOLD_DEFAULT,
        }
    }
}

impl AllocTracker {
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
        if !self.increase_tolerable(increase, direct) {
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
            if !self.increase_tolerable(size - old, direct) {
                return false;
            }
            self.add(size - old);
        } else {
            self.bytes_allocated = self.bytes_allocated.saturating_sub(old - size);
        }
        true
    }

    /// Releases a charged allocation of `size` bytes (`FREE`).
    pub fn free(&mut self, size: usize) {
        self.bytes_allocated = self
            .bytes_allocated
            .saturating_sub((size as u64).saturating_add(MALLOC_HEADER));
    }

    /// Charges the creation of an external entity parser (the same bound
    /// as a root parser's creation, `CREATION_CHARGE`, plus the encoding
    /// name), checked against the root parser's limits like C's
    /// `parserCreate` with a parent; `false` = refused, nothing charged.
    pub fn charge_child_creation(&mut self, encoding_name_len: Option<usize>, direct: u64) -> bool {
        let increase = CREATION_CHARGE
            .saturating_add(encoding_name_len.map_or(0, |n| (n as u64).saturating_add(1 + MALLOC_HEADER)));
        if !self.increase_tolerable(increase, direct) {
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
        self.bytes_allocated = self.bytes_allocated.saturating_sub(charge);
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

    /// Appends `len` characters to the current string, growing like C
    /// (with `POOL_SLACK`).
    pub fn append<H: Heap + ?Sized>(&mut self, heap: &mut H, len: usize) -> Result<(), NoMemory> {
        let len = i64::try_from(len).map_err(|_| NoMemory)?;
        if self.blocks.is_empty() {
            self.grow(heap)?;
        }
        while self.end() - self.ptr < len + POOL_SLACK {
            self.grow(heap)?;
        }
        self.ptr += len;
        Ok(())
    }

    /// `poolFinish`.
    pub fn finish(&mut self) {
        self.start = self.ptr;
    }

    /// `poolDiscard`.
    pub fn discard(&mut self) {
        self.ptr = self.start;
    }

    /// Appends a string of `len` characters plus its NUL and finishes it
    /// (`poolStoreString` + `poolFinish`, `poolCopyString`).
    pub fn store<H: Heap + ?Sized>(&mut self, heap: &mut H, len: usize) -> Result<(), NoMemory> {
        self.append(heap, len.saturating_add(1))?;
        self.finish();
        Ok(())
    }

    /// Appends a string of `len` characters plus its NUL and discards it
    /// (`poolStoreString` of a name that is then found in a table).
    pub fn store_discard<H: Heap + ?Sized>(&mut self, heap: &mut H, len: usize) -> Result<(), NoMemory> {
        self.append(heap, len.saturating_add(1))?;
        self.discard();
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

/// `sizeof(TAG)` bound: a parent pointer, the raw name pointer and length,
/// `TAG_NAME` (three pointers, three ints), the buffer pointers and the
/// bindings pointer: at most twelve pointer-sized words.
pub const TAG_SIZE: usize = 12 * POINTER_SIZE;
/// `INIT_TAG_BUF_SIZE`
pub const INIT_TAG_BUF_SIZE: usize = 32;
/// `sizeof(BINDING)` bound: five pointers and two ints.
pub const BINDING_SIZE: usize = 6 * POINTER_SIZE;
/// `EXPAND_SPARE`
pub const EXPAND_SPARE: usize = 24;
/// `sizeof(OPEN_INTERNAL_ENTITY)` bound: four pointers, an int, a bool and
/// an enum.
pub const OPEN_ENTITY_SIZE: usize = 6 * POINTER_SIZE;
/// `sizeof(ATTRIBUTE)`: three pointers and a char, padded.
pub const ATTRIBUTE_SIZE: usize = 4 * POINTER_SIZE;
/// `sizeof(NS_ATT)` bound: two `unsigned long`s and a pointer.
pub const NS_ATT_SIZE: usize = 3 * 8;
/// `sizeof(DEFAULT_ATTRIBUTE)`: two pointers and a bool, padded.
pub const DEFAULT_ATTRIBUTE_SIZE: usize = 3 * POINTER_SIZE;
/// `sizeof(NAME_AND_DEFAULT_ATTRIBUTE)` bound: a name pointer and an index.
pub const NAME_AND_DEFAULT_ATTRIBUTE_SIZE: usize = 2 * POINTER_SIZE;
/// `sizeof(CONTENT_SCAFFOLD)` bound: two enums, a name pointer, four ints.
pub const CONTENT_SCAFFOLD_SIZE: usize = 4 * POINTER_SIZE;
/// `INIT_SCAFFOLD_ELEMENTS`
pub const INIT_SCAFFOLD_ELEMENTS: usize = 32;

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

    /// Stores and finishes a string of `len` characters in `which`.
    pub fn store(&mut self, which: Pool, len: usize, direct: u64) -> Result<(), NoMemory> {
        let (p, mut h) = self.pool(which, direct);
        p.store(&mut h, len)
    }

    /// Stores and discards a string of `len` characters in `which`.
    pub fn store_discard(&mut self, which: Pool, len: usize, direct: u64) -> Result<(), NoMemory> {
        let (p, mut h) = self.pool(which, direct);
        p.store_discard(&mut h, len)
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
    fn shadow_grows_like_the_string_pool() -> Result<(), NoMemory> {
        // Strings appended in one piece (the shadow's only operation) with
        // finish, discard and clear in a deterministic mix: the shadow must
        // request at least the pool's bytes at every step.
        let mut seed: u32 = 12345;
        let mut next = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            seed >> 8
        };
        let mut pool = StringPool::new();
        let mut shadow = PoolShadow::default();
        let mut hp = Rec::default();
        let mut hs = Rec::default();
        let total = |r: &Rec| -> i64 {
            r.0.iter()
                .map(|&(k, a, b)| match k {
                    'm' => b as i64,
                    'r' => b as i64 - a as i64,
                    _ => -(a as i64),
                })
                .sum()
        };
        for _ in 0..3000 {
            let op = next() % 10;
            let len = match next() % 4 {
                0 => (next() % 8) as usize,
                1 => (next() % 600) as usize,
                2 => (next() % 3000) as usize,
                _ => (next() % 20_000) as usize,
            };
            let s = alloc::vec![b'x'; len];
            match op {
                0..=5 => {
                    pool.append_chars(&mut hp, &s)?;
                    pool.append_char(&mut hp, 0)?;
                    pool.finish();
                    shadow.store(&mut hs, len)?;
                }
                6 | 7 => {
                    pool.append_chars(&mut hp, &s)?;
                    pool.append_char(&mut hp, 0)?;
                    pool.discard();
                    shadow.store_discard(&mut hs, len)?;
                }
                _ => {
                    pool.clear();
                    shadow.clear();
                }
            }
            assert!(
                total(&hs) >= total(&hp),
                "shadow {} < pool {}",
                total(&hs),
                total(&hp)
            );
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
