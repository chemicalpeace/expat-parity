//! Module 9: the string pool and hash table of `xmlparse.c`, white-box
//! against C (see `expat_difftest::core`).

use expat::internal::hash::hash;
use expat::internal::heap::BLOCK_HEADER_SIZE;
use expat::internal::pool::bytes_to_allocate_for;
use expat::internal::siphash::SipKey;
use expat_difftest::core::{
    HashOp, PoolOp, check_hash, check_pool, hash_ops_from_bytes, pool_ops_from_bytes,
};
use expat_difftest::tok::Rng;

const SALT: [u8; 16] = *b"expat-rs-oracle!";

fn random_bytes(r: &mut Rng, n: usize) -> Vec<u8> {
    (0..n).map(|_| r.next_u64() as u8).collect()
}

#[test]
fn block_arithmetic_matches_c() {
    assert_eq!(BLOCK_HEADER_SIZE, expat_oracle::block_header_size());
    let h = BLOCK_HEADER_SIZE as i32;
    let mut values = vec![i32::MIN, -1, 0, 1, 2, 1023, 1024, 1025, 4096, 1 << 20];
    for d in -3..=3 {
        values.push(i32::MAX - h + d);
        values.push(i32::MAX / 2 + d);
    }
    values.push(i32::MAX);
    for v in values {
        assert_eq!(
            bytes_to_allocate_for(v),
            expat_oracle::pool_bytes_to_allocate_for(v),
            "poolBytesToAllocateFor({v})"
        );
    }
}

#[test]
fn hash_values_match_c() {
    let mut r = Rng::new(9);
    for i in 0..2000 {
        let mut salt = [0u8; 16];
        if i % 2 == 1 {
            salt.copy_from_slice(&random_bytes(&mut r, 16));
        }
        let n = (r.below(40)) as usize;
        let key: Vec<u8> = random_bytes(&mut r, n)
            .into_iter()
            .map(|b| b.max(1))
            .collect();
        let (c, bits) = expat_oracle::parser_hash(&salt, &key);
        assert_eq!(bits, std::ffi::c_ulong::BITS, "width of unsigned long");
        assert_eq!(
            hash(&SipKey::from_bytes(salt), &key) as u64,
            c,
            "key {key:02x?}"
        );
    }
}

/// Every growth strategy, every pool function, both clear orders.
fn pool_scenario() -> Vec<PoolOp> {
    use PoolOp::*;
    vec![
        LastChar,
        Chop,
        Finish,
        AppendChars(vec![]),
        CopyStringN(vec![]),
        AppendChar(b'a'),
        LastChar,
        AppendChars(vec![b'b'; 1500]),
        Finish,
        CopyString(b"name".to_vec()),
        AppendString(b"pre:".to_vec()),
        Append(0, "ünï".as_bytes().to_vec()),
        Append(4, b"caf\xE9".to_vec()),
        Append(8, b"a\0\xE9\0\x3D\xD8\x00\xDC".to_vec()),
        Append(10, vec![0x00, 0x41, 0x00]),
        StoreString(2, b"!".to_vec()),
        Finish,
        CopyStringNoFinish(vec![b'n'; 700]),
        Chop,
        LastChar,
        Discard,
        Append(5, vec![0xE9; 3000]),
        Finish,
        Clear,
        Grow,
        GrowUntil(5000),
        CopyStringN(vec![b'x'; 2000]),
        Clear,
        AppendChar(b'z'),
        AppendChars(vec![b'y'; 1100]),
        CopyString(vec![b'w'; 9000]),
        Clear,
        StoreString(6, b"ascii".to_vec()),
        Grow,
        Grow,
        Finish,
        Grow,
        CopyStringNoFinish(vec![b'q'; 5000]),
        Finish,
    ]
}

#[test]
fn pool_scenario_matches_c() {
    check_pool(&pool_scenario());
}

#[test]
fn pool_every_allocation_failure_matches_c() {
    // Fail each allocation attempt of the scenario in turn (attempt `k`
    // counted from the start), and also two in a row.
    let scenario = pool_scenario();
    for k in 0..40 {
        let mut ops = vec![PoolOp::FailNext(k)];
        ops.extend(scenario.iter().cloned());
        check_pool(&ops);
    }
    for (i, _) in scenario.iter().enumerate() {
        let mut ops = scenario.clone();
        ops.insert(i, PoolOp::FailNext(0));
        ops.insert(i + 1, PoolOp::FailNext(1));
        check_pool(&ops);
    }
}

#[test]
fn pool_huge_requests_match_c() {
    // Growth until the harness heap cap refuses, from both strategies.
    check_pool(&[PoolOp::GrowUntil(u32::MAX)]);
    check_pool(&[
        PoolOp::AppendChar(1),
        PoolOp::Finish,
        PoolOp::GrowUntil(40 << 20),
        PoolOp::LastChar,
    ]);
}

#[test]
fn pool_random_programs_match_c() {
    let mut r = Rng::new(0x9001);
    for _ in 0..1500 {
        let n = r.below(120) as usize;
        let ops = pool_ops_from_bytes(&random_bytes(&mut r, n));
        check_pool(&ops);
    }
}

#[test]
fn hash_large_tables_match_c() {
    // Up to 2^14 slots: the probe step is truncated to 8 bits from 2^10 on.
    for salt in [SALT, [0; 16], [0xFF; 16]] {
        let mut ops: Vec<HashOp> = (0..6000)
            .map(|i| HashOp::Insert(format!("n{i}").into_bytes(), 0))
            .collect();
        ops.push(HashOp::Iterate);
        ops.extend(
            (0..6100)
                .step_by(7)
                .map(|i| HashOp::Find(format!("n{i}").into_bytes())),
        );
        ops.push(HashOp::Clear);
        ops.extend((0..100).map(|i| HashOp::Insert(format!("m{i}").into_bytes(), 40)));
        ops.push(HashOp::Iterate);
        check_hash(salt, &ops);
    }
}

#[test]
fn hash_every_allocation_failure_matches_c() {
    // 70 keys: first allocation, one doubling, entries; fail each attempt.
    let base: Vec<HashOp> = (0..70)
        .map(|i| HashOp::Insert(format!("k{}", i % 60).into_bytes(), 24))
        .collect();
    for k in 0..80 {
        let mut ops = vec![HashOp::FailNext(k)];
        ops.extend(base.iter().cloned());
        ops.push(HashOp::Iterate);
        check_hash(SALT, &ops);
    }
    // Failure exactly at the doubling (the 33rd insertion).
    for k in 0..3 {
        let mut ops: Vec<HashOp> = base.iter().take(32).cloned().collect();
        ops.push(HashOp::FailNext(k));
        ops.extend(base.iter().skip(32).cloned());
        check_hash(SALT, &ops);
    }
}

#[test]
fn hash_random_programs_match_c() {
    let mut r = Rng::new(0x4a54);
    for i in 0..1500 {
        let n = r.below(200) as usize;
        let mut salt = SALT;
        if i % 3 == 0 {
            salt.copy_from_slice(&random_bytes(&mut r, 16));
        }
        let ops = hash_ops_from_bytes(&random_bytes(&mut r, n));
        check_hash(salt, &ops);
    }
}
