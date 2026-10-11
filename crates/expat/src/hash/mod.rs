//! The hash table of `xmlparse.c` (`HASH_TABLE`, `lookup`).
//!
//! An open-addressing table keyed by C strings: SipHash-2-4 of the key,
//! seeded with the root parser's salt, picks the first slot; collisions
//! probe with a second hash taken from the bits the mask discarded
//! (double hashing, `PROBE_STEP`). The table starts with 64 slots on the
//! first insertion and doubles when an insertion finds it half full. There
//! is no removal; `hashTableClear` empties the table but keeps its size.
//!
//! The order of the slots is observable: the parser iterates tables (for
//! example over the namespace prefixes when it builds the context string of
//! an external entity reference, `getContext`), so the port keeps the slot
//! layout of C exactly, including the width of the hash value (`unsigned
//! long`, 32 bits on Windows and 64 bits on LP64 targets; D-021). Entries
//! live in an arena and are named by [`EntryId`], which stays valid until the
//! table is cleared; the slots hold ids. Each key is owned by its entry (C
//! points at a string in a pool, which outlives the table entry).
//!
//! ```
//! # #[cfg(feature = "internal-api")]
//! # fn main() -> Result<(), expat::internal::heap::NoMemory> {
//! use expat::internal::hash::{HashTable, Lookup};
//! use expat::internal::heap::Unlimited;
//! use expat::internal::siphash::SipKey;
//!
//! let salt = SipKey::from_bytes([7; 16]);
//! let mut table: HashTable<u32> = HashTable::new();
//! let (id, l) = table.lookup_or_insert(&salt, &mut Unlimited, b"xml", 16, || 1)?;
//! assert_eq!(l, Lookup::Inserted);
//! let (again, l) = table.lookup_or_insert(&salt, &mut Unlimited, b"xml", 16, || 2)?;
//! assert_eq!((again, l), (id, Lookup::Found));
//! assert_eq!(table.find(&salt, b"xml"), Some(id));
//! assert_eq!(table.value(id), Some(&1));
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "internal-api"))]
//! # fn main() {}
//! ```

#[cfg(test)]
mod tests;

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::ffi::c_ulong;

use crate::heap::{Heap, NoMemory, POINTER_SIZE};
use crate::siphash::{SipKey, siphash24};

/// `INIT_POWER`: the first allocation has `1 << INIT_POWER` slots.
pub const INIT_POWER: u8 = 6;

/// Names an entry of a [`HashTable`] (C: the `NAMED *` that `lookup`
/// returns). Valid until the table is cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryId(pub usize);

/// Whether [`HashTable::lookup_or_insert`] found the key or created it (C
/// tells them apart by the zeroed fields of a new entry).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lookup {
    /// The key was already present.
    Found,
    /// A new entry was created.
    Inserted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry<V> {
    key: Box<[u8]>,
    /// `createSize` of the C allocation, released by `clear`/`destroy`.
    alloc_size: usize,
    value: V,
}

/// `HASH_TABLE` with entries of type `V`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashTable<V> {
    /// `v`: `size` slots (`size` is `slots.len()`, 0 before the first
    /// insertion).
    slots: Vec<Option<EntryId>>,
    /// `power`: `size == 1 << power` once allocated.
    power: u8,
    /// The entries, in insertion order; `used` is their number.
    entries: Vec<Entry<V>>,
}

impl<V> Default for HashTable<V> {
    fn default() -> Self {
        Self::new()
    }
}

/// `hash(parser, s, keyLen)`: SipHash-2-4 of the key, as C's `unsigned long`.
pub fn hash(salt: &SipKey, key: &[u8]) -> c_ulong {
    // C converts the 64-bit hash to `unsigned long` (truncating where it is
    // 32 bits wide).
    siphash24(key, salt) as c_ulong
}

/// `PROBE_STEP(hash, mask, power)`: odd, at most `mask >> 2`, truncated to
/// `unsigned char`.
fn probe_step(h: c_ulong, mask: c_ulong, power: u8) -> u8 {
    let second = (h & !mask)
        .checked_shr(u32::from(power).wrapping_sub(1))
        .unwrap_or(0)
        & (mask >> 2);
    (second as u8) | 1
}

/// One probing step backwards: `i < step ? (i += size - step) : (i -= step)`.
fn probe_next(i: usize, step: u8, size: usize) -> usize {
    let step = usize::from(step);
    if i < step {
        i.wrapping_add(size).wrapping_sub(step)
    } else {
        i.wrapping_sub(step)
    }
}

/// Index of a slot: `hash & mask`.
fn slot_index(h: c_ulong, mask: c_ulong) -> usize {
    // `mask` is `size - 1` and `size` is a `usize`.
    usize::try_from(h & mask).unwrap_or(0)
}

impl<V> HashTable<V> {
    /// `hashTableInit`: an empty table without slots.
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            power: 0,
            entries: Vec::new(),
        }
    }

    /// `size`: number of slots.
    pub fn size(&self) -> usize {
        self.slots.len()
    }

    /// `used`: number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `power`.
    pub fn power(&self) -> u8 {
        self.power
    }

    /// The slots in order, each holding the id of its entry or `None`.
    pub fn slots(&self) -> &[Option<EntryId>] {
        &self.slots
    }

    /// The key of an entry.
    pub fn key(&self, id: EntryId) -> Option<&[u8]> {
        self.entries.get(id.0).map(|e| &*e.key)
    }

    /// The value of an entry.
    pub fn value(&self, id: EntryId) -> Option<&V> {
        self.entries.get(id.0).map(|e| &e.value)
    }

    /// The value of an entry, mutably.
    pub fn value_mut(&mut self, id: EntryId) -> Option<&mut V> {
        self.entries.get_mut(id.0).map(|e| &mut e.value)
    }

    fn mask(&self) -> c_ulong {
        (self.slots.len() as c_ulong).wrapping_sub(1)
    }

    /// Probes for `key` from its first slot; returns the slot index and the
    /// entry if the key is present, otherwise the first empty slot. Needs
    /// slots.
    fn probe(&self, h: c_ulong, key: &[u8]) -> (usize, Option<EntryId>) {
        let mask = self.mask();
        let mut i = slot_index(h, mask);
        let mut step = 0u8;
        while let Some(Some(id)) = self.slots.get(i) {
            if self.entries.get(id.0).is_some_and(|e| *e.key == *key) {
                return (i, Some(*id));
            }
            if step == 0 {
                step = probe_step(h, mask, self.power);
            }
            i = probe_next(i, step, self.slots.len());
        }
        (i, None)
    }

    /// `lookup(parser, table, name, 0)`: the entry with key `key`. Unlike
    /// insertion, C does not need a NUL-terminated key here
    /// (`lookupWithLength`), so any byte string can be looked up.
    pub fn find(&self, salt: &SipKey, key: &[u8]) -> Option<EntryId> {
        if self.slots.is_empty() {
            return None;
        }
        self.probe(hash(salt, key), key).1
    }

    /// `lookup(parser, table, name, createSize)`: the entry with key `key`,
    /// created with `make()` if it is missing. `alloc_size` is the C
    /// `createSize` (the size of the entry structure), announced to `heap`.
    /// `key` is a C string: it must not contain NUL.
    ///
    /// On `Err` the table is as C leaves it: unchanged, except that an
    /// insertion that failed after doubling the table keeps the doubled
    /// table, and a failed first allocation sets `power` but no slots.
    pub fn lookup_or_insert<H: Heap + ?Sized>(
        &mut self,
        salt: &SipKey,
        heap: &mut H,
        key: &[u8],
        alloc_size: usize,
        make: impl FnOnce() -> V,
    ) -> Result<(EntryId, Lookup), NoMemory> {
        debug_assert!(!key.contains(&0), "C string key without interior NUL");
        let h = hash(salt, key);
        let i = if self.slots.is_empty() {
            self.power = INIT_POWER;
            let size = 1usize << INIT_POWER;
            if !heap.alloc(size.wrapping_mul(POINTER_SIZE)) {
                return Err(NoMemory);
            }
            self.slots = vec![None; size];
            slot_index(h, self.mask())
        } else {
            let (i, found) = self.probe(h, key);
            if let Some(id) = found {
                return Ok((id, Lookup::Found));
            }
            // Half full: double before inserting.
            let half = u32::from(self.power).wrapping_sub(1);
            if self.entries.len().checked_shr(half).unwrap_or(0) != 0 {
                self.double(salt, heap)?;
                self.probe(h, key).0
            } else {
                i
            }
        };
        if !heap.alloc(alloc_size) {
            return Err(NoMemory);
        }
        let id = EntryId(self.entries.len());
        self.entries.push(Entry {
            key: key.into(),
            alloc_size,
            value: make(),
        });
        if let Some(slot) = self.slots.get_mut(i) {
            *slot = Some(id);
        }
        Ok((id, Lookup::Inserted))
    }

    /// The doubling step of `lookupWithLength`: rehashes every entry into a
    /// table of twice the size, in slot order.
    fn double<H: Heap + ?Sized>(&mut self, salt: &SipKey, heap: &mut H) -> Result<(), NoMemory> {
        let new_power = self.power.wrapping_add(1);
        if u32::from(new_power) >= c_ulong::BITS {
            return Err(NoMemory);
        }
        let new_size = 1usize.checked_shl(u32::from(new_power)).ok_or(NoMemory)?;
        let bytes = new_size.checked_mul(POINTER_SIZE).ok_or(NoMemory)?;
        if !heap.alloc(bytes) {
            return Err(NoMemory);
        }
        let new_mask = (new_size as c_ulong).wrapping_sub(1);
        let mut new_slots = vec![None; new_size];
        for id in self.slots.iter().flatten() {
            let Some(e) = self.entries.get(id.0) else {
                continue;
            };
            let nh = hash(salt, &e.key);
            let mut j = slot_index(nh, new_mask);
            let mut step = 0u8;
            while let Some(Some(_)) = new_slots.get(j) {
                if step == 0 {
                    step = probe_step(nh, new_mask, new_power);
                }
                j = probe_next(j, step, new_size);
            }
            if let Some(slot) = new_slots.get_mut(j) {
                *slot = Some(*id);
            }
        }
        heap.free(self.slots.len().wrapping_mul(POINTER_SIZE));
        self.slots = new_slots;
        self.power = new_power;
        Ok(())
    }

    /// The slot that holds the entry with key `key` (white-box view for the
    /// differential tests).
    pub fn find_slot(&self, salt: &SipKey, key: &[u8]) -> Option<usize> {
        if self.slots.is_empty() {
            return None;
        }
        match self.probe(hash(salt, key), key) {
            (i, Some(_)) => Some(i),
            (_, None) => None,
        }
    }

    /// `hashTableIterInit` + `hashTableIterNext`: the entries in slot order.
    pub fn iter(&self) -> impl Iterator<Item = EntryId> + '_ {
        self.slots.iter().flatten().copied()
    }

    /// `hashTableClear`: releases every entry (in slot order) but keeps the
    /// slots. Every [`EntryId`] becomes invalid.
    pub fn clear<H: Heap + ?Sized>(&mut self, heap: &mut H) {
        for slot in &mut self.slots {
            if let Some(id) = slot.take()
                && let Some(e) = self.entries.get(id.0)
            {
                heap.free(e.alloc_size);
            }
        }
        self.entries.clear();
    }

    /// `hashTableDestroy`: releases every entry (in slot order), then the
    /// slots.
    pub fn destroy<H: Heap + ?Sized>(mut self, heap: &mut H) {
        let size = self.slots.len();
        self.clear(heap);
        if size != 0 {
            heap.free(size.wrapping_mul(POINTER_SIZE));
        }
    }
}
