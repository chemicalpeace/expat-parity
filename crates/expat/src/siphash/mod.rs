//! SipHash-2-4, the keyed hash of libexpat's internal hash tables.
//!
//! Port of `siphash.h` (`R_2_9_0`; originally by William Ahern, MIT). libexpat
//! seeds it with a per-parser salt to resist hash-flooding; the hash value
//! itself is never observable through the parser API.
//!
//! Differences from the C interface, all for safety:
//! - [`SipHasher24::finish`] consumes the hasher (C's `sip24_final` mutates
//!   the state, so calling it twice silently gives a different value).
//! - The 8-byte buffer is tracked by a length, not by an interior pointer.
//!
//! ```
//! # #[cfg(feature = "internal-api")] {
//! use expat::internal::siphash::{SipHasher24, SipKey, siphash24};
//! let key = SipKey::from_bytes([0; 16]);
//! let mut h = SipHasher24::new(&key);
//! h.update(b"hello ");
//! h.update(b"world");
//! assert_eq!(h.finish(), siphash24(b"hello world", &key));
//! # }
//! ```

/// A 128-bit SipHash key (`struct sipkey`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SipKey {
    k0: u64,
    k1: u64,
}

impl SipKey {
    /// Key from 16 bytes, little-endian halves (`sip_tokey`).
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        let [a0, a1, a2, a3, a4, a5, a6, a7, b0, b1, b2, b3, b4, b5, b6, b7] = bytes;
        Self {
            k0: u64::from_le_bytes([a0, a1, a2, a3, a4, a5, a6, a7]),
            k1: u64::from_le_bytes([b0, b1, b2, b3, b4, b5, b6, b7]),
        }
    }

    /// Key from its two 64-bit halves.
    pub fn from_halves(k0: u64, k1: u64) -> Self {
        Self { k0, k1 }
    }
}

/// Incremental SipHash-2-4 state (`struct siphash`).
#[derive(Debug, Clone)]
pub struct SipHasher24 {
    v0: u64,
    v1: u64,
    v2: u64,
    v3: u64,
    /// Pending bytes of an incomplete 8-byte block.
    buf: [u8; 8],
    /// Number of valid bytes in `buf` (0..=7 between calls).
    buf_len: usize,
    /// Bytes compressed so far (`c`; wraps like the C `uint64_t`).
    count: u64,
}

impl SipHasher24 {
    /// `sip24_init`.
    pub fn new(key: &SipKey) -> Self {
        Self {
            v0: 0x736f_6d65_7073_6575 ^ key.k0,
            v1: 0x646f_7261_6e64_6f6d ^ key.k1,
            v2: 0x6c79_6765_6e65_7261 ^ key.k0,
            v3: 0x7465_6462_7974_6573 ^ key.k1,
            buf: [0; 8],
            buf_len: 0,
            count: 0,
        }
    }

    /// `sip_round` applied `rounds` times. SipHash is defined over wrapping
    /// 64-bit arithmetic, hence the `wrapping_add`s.
    fn rounds(&mut self, rounds: usize) {
        for _ in 0..rounds {
            self.v0 = self.v0.wrapping_add(self.v1);
            self.v1 = self.v1.rotate_left(13) ^ self.v0;
            self.v0 = self.v0.rotate_left(32);
            self.v2 = self.v2.wrapping_add(self.v3);
            self.v3 = self.v3.rotate_left(16) ^ self.v2;
            self.v0 = self.v0.wrapping_add(self.v3);
            self.v3 = self.v3.rotate_left(21) ^ self.v0;
            self.v2 = self.v2.wrapping_add(self.v1);
            self.v1 = self.v1.rotate_left(17) ^ self.v2;
            self.v2 = self.v2.rotate_left(32);
        }
    }

    fn compress(&mut self, m: u64) {
        self.v3 ^= m;
        self.rounds(2);
        self.v0 ^= m;
        self.count = self.count.wrapping_add(8);
    }

    /// `sip24_update`: absorbs `data`.
    pub fn update(&mut self, mut data: &[u8]) {
        // Complete a pending partial block first.
        if self.buf_len > 0 {
            let room = self.buf.get_mut(self.buf_len..).unwrap_or_default();
            let take = room.len().min(data.len());
            let (head, rest) = data.split_at(take);
            if let Some(dst) = room.get_mut(..take) {
                dst.copy_from_slice(head);
            }
            self.buf_len = self.buf_len.saturating_add(take);
            data = rest;
            if self.buf_len < 8 {
                return;
            }
            self.compress(u64::from_le_bytes(self.buf));
            self.buf_len = 0;
        }
        let (blocks, tail) = data.as_chunks::<8>();
        for block in blocks {
            self.compress(u64::from_le_bytes(*block));
        }
        if let Some(dst) = self.buf.get_mut(..tail.len()) {
            dst.copy_from_slice(tail);
        }
        self.buf_len = tail.len();
    }

    /// `sip24_final`: the 64-bit hash.
    pub fn finish(mut self) -> u64 {
        // `buf_len <= 7`: the cast is exact and the pending bytes occupy the
        // low 7 bytes, disjoint from the length byte at the top.
        let left = self.buf_len as u64;
        let mut tail = [0u8; 8];
        if let (Some(dst), Some(src)) = (tail.get_mut(..self.buf_len), self.buf.get(..self.buf_len)) {
            dst.copy_from_slice(src);
        }
        let b = (self.count.wrapping_add(left) << 56) | u64::from_le_bytes(tail);
        self.v3 ^= b;
        self.rounds(2);
        self.v0 ^= b;
        self.v2 ^= 0xff;
        self.rounds(4);
        self.v0 ^ self.v1 ^ self.v2 ^ self.v3
    }
}

/// One-shot SipHash-2-4 (`siphash24`).
pub fn siphash24(data: &[u8], key: &SipKey) -> u64 {
    let mut h = SipHasher24::new(key);
    h.update(data);
    h.finish()
}

#[cfg(test)]
// reason: test code; an out-of-bounds index or overflow there is a test failure.
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod tests;
