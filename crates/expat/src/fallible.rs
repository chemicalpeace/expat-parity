//! Allocations that report failure instead of aborting (D-061).
//!
//! The parser core grows its collections through these functions. When
//! the global allocator cannot provide the memory, they return
//! [`NoMemory`] and leave the collection as it was, and the parser fails
//! with `XML_ERROR_NO_MEMORY` as libexpat does after a failed `malloc`.

use alloc::vec::Vec;

use crate::heap::NoMemory;

/// Collections that grow fallibly.
pub trait TryGrow<T> {
    /// Room for `additional` more elements, or [`NoMemory`].
    fn try_room(&mut self, additional: usize) -> Result<(), NoMemory>;
    /// `push` that fails instead of aborting.
    fn try_push(&mut self, value: T) -> Result<(), NoMemory>;
    /// `extend_from_slice` that fails instead of aborting.
    fn try_extend(&mut self, values: &[T]) -> Result<(), NoMemory>
    where
        T: Clone;
    /// `resize` that fails instead of aborting.
    fn try_resize(&mut self, len: usize, value: T) -> Result<(), NoMemory>
    where
        T: Clone;
}

impl<T> TryGrow<T> for Vec<T> {
    fn try_room(&mut self, additional: usize) -> Result<(), NoMemory> {
        self.try_reserve(additional).map_err(|_| NoMemory)
    }

    fn try_push(&mut self, value: T) -> Result<(), NoMemory> {
        self.try_room(1)?;
        self.push(value);
        Ok(())
    }

    fn try_extend(&mut self, values: &[T]) -> Result<(), NoMemory>
    where
        T: Clone,
    {
        self.try_room(values.len())?;
        self.extend_from_slice(values);
        Ok(())
    }

    fn try_resize(&mut self, len: usize, value: T) -> Result<(), NoMemory>
    where
        T: Clone,
    {
        self.try_room(len.saturating_sub(self.len()))?;
        self.resize(len, value);
        Ok(())
    }
}

/// `values.to_vec()` that fails instead of aborting.
pub fn try_copy<T: Clone>(values: &[T]) -> Result<Vec<T>, NoMemory> {
    let mut v = Vec::new();
    v.try_extend(values)?;
    Ok(v)
}

/// `vec![value; len]` that fails instead of aborting.
pub fn try_filled<T: Clone>(value: T, len: usize) -> Result<Vec<T>, NoMemory> {
    let mut v = Vec::new();
    v.try_resize(len, value)?;
    Ok(v)
}

/// `Vec::with_capacity` that fails instead of aborting.
pub fn try_with_capacity<T>(capacity: usize) -> Result<Vec<T>, NoMemory> {
    let mut v = Vec::new();
    v.try_room(capacity)?;
    Ok(v)
}

/// A map kept as a sorted vector, growing fallibly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortedMap<K, V>(Vec<(K, V)>);

impl<K, V> Default for SortedMap<K, V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<K: Ord + Copy, V> SortedMap<K, V> {
    /// The value for `key`.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.0
            .binary_search_by_key(key, |e| e.0)
            .ok()
            .and_then(|i| self.0.get(i))
            .map(|e| &e.1)
    }

    /// Whether `key` has a value.
    pub fn contains_key(&self, key: &K) -> bool {
        self.0.binary_search_by_key(key, |e| e.0).is_ok()
    }

    /// Room for one more entry.
    pub fn reserve_one(&mut self) -> Result<(), NoMemory> {
        self.0.try_room(1)
    }

    /// Inserts `value` for `key` unless `key` has one; whether it did.
    pub fn insert_if_absent(&mut self, key: K, value: V) -> Result<bool, NoMemory> {
        match self.0.binary_search_by_key(&key, |e| e.0) {
            Ok(_) => Ok(false),
            Err(at) => {
                self.0.try_room(1)?;
                self.0.insert(at, (key, value));
                Ok(true)
            }
        }
    }
}

/// `value.clone()` of an optional byte string that fails instead of
/// aborting.
pub fn try_copy_opt(value: &Option<Vec<u8>>) -> Result<Option<Vec<u8>>, NoMemory> {
    value.as_deref().map(try_copy).transpose()
}

/// `Arc::new(value)` that reports a failure of the system allocator
/// instead of aborting, as far as stable Rust allows (D-067).
///
/// Stable Rust has no fallible `Arc` constructor. This function first
/// reserves a block of the same size as the `Arc`'s and returns
/// [`NoMemory`] when that fails; otherwise it frees the block and calls
/// `Arc::new`, which then asks for a block of the size just freed. That
/// call can still abort when another thread takes the memory in between.
pub fn try_arc<T>(value: T) -> Result<alloc::sync::Arc<T>, NoMemory> {
    probe_arc::<T>()?;
    Ok(alloc::sync::Arc::new(value))
}

/// The reservation [`try_arc`] makes before `Arc::new`, for a caller that
/// creates the `Arc<T>` itself right after it.
pub fn probe_arc<T>() -> Result<(), NoMemory> {
    let mut probe: Vec<u8> = Vec::new();
    probe.try_reserve_exact(arc_block_size::<T>()).map_err(|_| NoMemory)
}

/// The size of the block behind an `Arc<T>`: two reference counts and
/// the value.
pub fn arc_block_size<T>() -> usize {
    let counts = core::alloc::Layout::new::<[core::sync::atomic::AtomicUsize; 2]>();
    counts
        .extend(core::alloc::Layout::new::<T>())
        .map_or(usize::MAX, |(l, _)| l.pad_to_align().size())
}
