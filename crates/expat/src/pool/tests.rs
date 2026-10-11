use super::*;
use crate::heap::Unlimited;
use crate::tok::{Normal, Utf16};
use alloc::vec::Vec;

/// Records every request and refuses the one with index `fail_at`.
#[derive(Default)]
struct Recorder {
    log: Vec<(char, usize, usize)>,
    fail_at: Option<usize>,
    calls: usize,
}

impl Recorder {
    fn decide(&mut self) -> bool {
        let ok = self.fail_at != Some(self.calls);
        self.calls += 1;
        ok
    }
}

impl Heap for Recorder {
    fn alloc(&mut self, size: usize) -> bool {
        let ok = self.decide();
        self.log.push(('a', 0, if ok { size } else { 0 }));
        ok
    }
    fn realloc(&mut self, old: usize, size: usize) -> bool {
        let ok = self.decide();
        self.log.push(('r', old, if ok { size } else { 0 }));
        ok
    }
    fn free(&mut self, size: usize) {
        self.log.push(('f', size, 0));
    }
}

#[test]
fn bytes_to_allocate_for_bounds() {
    assert_eq!(bytes_to_allocate_for(0), 0);
    assert_eq!(bytes_to_allocate_for(-1), 0);
    assert_eq!(bytes_to_allocate_for(1024), BLOCK_HEADER_SIZE + 1024);
    let last = i32::MAX - i32::try_from(BLOCK_HEADER_SIZE).unwrap_or(0);
    assert_eq!(bytes_to_allocate_for(last), i32::MAX as usize);
    assert_eq!(bytes_to_allocate_for(last + 1), 0);
}

#[test]
fn growth_strategies() -> Result<(), NoMemory> {
    let mut heap = Recorder::default();
    let mut pool = StringPool::new();
    // First block: 1024.
    pool.append_char(&mut heap, b'a')?;
    assert_eq!(pool.layout().blocks, [1024]);
    // The current string starts the block: it is enlarged in place.
    pool.append_chars(&mut heap, &[b'b'; 1500])?;
    assert_eq!(pool.layout().blocks, [2048]);
    pool.finish();
    // A new string that does not fit gets a new block twice the size of
    // the space left after `start`... at least INIT_BLOCK_SIZE.
    pool.append_chars(&mut heap, &[b'c'; 600])?;
    assert_eq!(pool.layout().blocks, [2048, 1024]);
    assert_eq!(pool.current(), &[b'c'; 600][..]);
    let first = PoolStr { block: 0, start: 0, len: 1501 };
    assert_eq!(pool.get(first).map(|s| s.len()), Some(1501));
    assert_eq!(
        heap.log,
        [
            ('a', 0, BLOCK_HEADER_SIZE + 1024),
            ('r', BLOCK_HEADER_SIZE + 1024, BLOCK_HEADER_SIZE + 2048),
            ('a', 0, BLOCK_HEADER_SIZE + 1024),
        ]
    );
    // Clear moves the blocks to the free list, newest on top; growing reuses
    // it without allocating.
    pool.clear();
    assert_eq!(pool.layout().free_blocks, [2048, 1024]);
    pool.append_char(&mut heap, b'x')?;
    assert_eq!(pool.layout().blocks, [1024]);
    assert_eq!(pool.layout().free_blocks, [2048]);
    // A larger free block takes over the current string.
    pool.append_chars(&mut heap, &[b'y'; 1100])?;
    assert_eq!(pool.layout().blocks, [1024, 2048]);
    assert!(pool.layout().free_blocks.is_empty());
    assert_eq!(pool.current().len(), 1101);
    assert_eq!(heap.log.len(), 3);
    // Clearing into an empty free list keeps the order (newest on top),
    // clearing into a non-empty one pushes the blocks newest first.
    pool.clear();
    assert_eq!(pool.layout().free_blocks, [1024, 2048]);
    pool.append_char(&mut heap, b'z')?;
    assert_eq!(pool.layout().blocks, [2048]);
    pool.clear();
    assert_eq!(pool.layout().free_blocks, [1024, 2048]);
    pool.destroy(&mut heap);
    assert_eq!(
        heap.log.get(3..).unwrap_or(&[]),
        [('f', BLOCK_HEADER_SIZE + 2048, 0), ('f', BLOCK_HEADER_SIZE + 1024, 0)]
    );
    Ok(())
}

#[test]
fn failures_leave_c_state() {
    let mut pool = StringPool::new();
    let mut heap = Recorder {
        fail_at: Some(0),
        ..Recorder::default()
    };
    assert_eq!(pool.copy_string(&mut heap, b"abc"), Err(NoMemory));
    assert_eq!(pool.layout().cursor, None);
    // A failed in-place enlargement keeps the old block and string.
    let mut heap = Recorder {
        fail_at: Some(1),
        ..Recorder::default()
    };
    let mut pool = StringPool::new();
    assert!(pool.append_chars(&mut heap, &[1; 1000]).is_ok());
    assert_eq!(
        pool.copy_string_no_finish(&mut heap, &[2; 30]),
        Err(NoMemory)
    );
    assert_eq!(pool.current(), &[1; 1000][..]);
    assert_eq!(pool.layout().blocks, [1024]);
}

#[test]
fn append_converts_and_drops_incomplete_tail() -> Result<(), NoMemory> {
    let mut pool = StringPool::new();
    let mut heap = Unlimited;
    pool.append(&mut heap, &Utf16::Little, b"a\0\xE9\0\x3D")?;
    assert_eq!(pool.current(), "aé".as_bytes());
    let s = pool.store_string(&mut heap, &Normal::Utf8, b"!")?;
    assert_eq!(pool.get(s), Some("aé!\0".as_bytes()));
    // A long conversion crosses block boundaries without splitting a
    // character.
    pool.discard();
    let input: Vec<u8> = core::iter::repeat_n(0xE9u8, 3000).collect();
    pool.append(&mut heap, &Normal::Latin1, &input)?;
    assert_eq!(pool.len(), 6000);
    assert!(pool.current().chunks(2).all(|c| c == [0xC3, 0xA9]));
    Ok(())
}

#[test]
fn small_helpers() -> Result<(), NoMemory> {
    let mut pool = StringPool::new();
    let mut heap = Unlimited;
    assert_eq!(pool.last_char(), None);
    pool.chop();
    assert_eq!(pool.len(), 0);
    pool.append_string(&mut heap, b"ab ")?;
    assert_eq!(pool.last_char(), Some(b' '));
    pool.chop();
    assert_eq!(pool.current(), b"ab");
    let s = pool.copy_string_n(&mut heap, b"cd")?;
    assert_eq!(pool.get(s), Some(&b"abcd"[..]));
    assert!(pool.is_empty());
    assert_eq!(pool.last_char(), None);
    pool.append_chars(&mut heap, b"")?;
    pool.discard();
    assert_eq!(
        pool.get(PoolStr { block: 5, start: 0, len: 1 }),
        None
    );
    Ok(())
}
