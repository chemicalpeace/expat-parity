//! Differential target for the string pool and hash table (module 9): the
//! port's `expat::pool` / `expat::hash` against `xmlparse.c`, white-box,
//! through `expat_difftest::core`.
//!
//! The first byte selects the structure: bit 0 clear runs the rest as a
//! pool program, set as a hash table program, salted by bits 1-2 (the
//! oracle's salt, all zeros, all ones, or the next 16 input bytes).
#![no_main]

use expat_difftest::core::{check_hash, check_pool, hash_ops_from_bytes, pool_ops_from_bytes};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&sel, rest)) = data.split_first() else {
        return;
    };
    if sel & 1 == 0 {
        check_pool(&pool_ops_from_bytes(rest));
        return;
    }
    let (salt, rest) = match (sel >> 1) & 3 {
        0 => (*b"expat-rs-oracle!", rest),
        1 => ([0; 16], rest),
        2 => ([0xFF; 16], rest),
        _ => {
            let mut salt = [0u8; 16];
            let n = rest.len().min(16);
            salt[..n].copy_from_slice(&rest[..n]);
            (salt, &rest[n..])
        }
    };
    check_hash(salt, &hash_ops_from_bytes(rest));
});
