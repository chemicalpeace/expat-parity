use super::*;
use crate::heap::Unlimited;
use alloc::format;
use alloc::vec::Vec;

fn salt() -> SipKey {
    SipKey::from_bytes(*b"expat-rs-oracle!")
}

#[test]
fn grows_at_half_full_and_keeps_entries() -> Result<(), NoMemory> {
    let mut t: HashTable<usize> = HashTable::new();
    let s = salt();
    assert_eq!(t.find(&s, b"a"), None);
    let mut ids = Vec::new();
    for n in 0..200usize {
        let key = format!("k{n}");
        let (id, l) = t.lookup_or_insert(&s, &mut Unlimited, key.as_bytes(), 16, || n)?;
        assert_eq!(l, Lookup::Inserted);
        ids.push(id);
        let expected_power = match n + 1 {
            0..=32 => 6,
            33..=64 => 7,
            65..=128 => 8,
            _ => 9,
        };
        assert_eq!(t.power(), expected_power, "after {} entries", n + 1);
    }
    assert_eq!(t.size(), 512);
    assert_eq!(t.len(), 200);
    for (n, id) in ids.iter().enumerate() {
        let key = format!("k{n}");
        assert_eq!(t.find(&s, key.as_bytes()), Some(*id));
        assert_eq!(t.key(*id), Some(key.as_bytes()));
        assert_eq!(t.value(*id), Some(&n));
    }
    let mut seen: Vec<EntryId> = t.iter().collect();
    seen.sort();
    assert_eq!(seen, ids);
    let third = EntryId(3);
    if let Some(v) = t.value_mut(third) {
        *v = 99;
    }
    assert_eq!(t.value(third), Some(&99));
    t.clear(&mut Unlimited);
    assert!(t.is_empty());
    assert_eq!(t.size(), 512);
    assert_eq!(t.find(&s, b"k1"), None);
    t.destroy(&mut Unlimited);
    Ok(())
}

#[test]
fn probe_step_is_odd_and_bounded() {
    for power in 6..20u8 {
        let mask = (1 as c_ulong).wrapping_shl(u32::from(power)) - 1;
        for h in [0, 1, 0xdead_beef, c_ulong::MAX] {
            let step = probe_step(h, mask, power);
            assert_eq!(step % 2, 1);
            assert!(c_ulong::from(step) <= (mask >> 2).max(1));
        }
    }
    assert_eq!(probe_next(0, 3, 64), 61);
    assert_eq!(probe_next(10, 3, 64), 7);
}

#[test]
fn cached_lookups_match_probing() -> Result<(), NoMemory> {
    // Keys that share the length and the first and last bytes compete for
    // one cache entry; every answer must still be the probed one.
    let s = salt();
    let mut t: HashTable<usize> = HashTable::new();
    let keys: Vec<_> = (0..300usize).map(|n| format!("a{n}z")).collect();
    for round in 0..3 {
        for (n, key) in keys.iter().enumerate() {
            let (id, l) = t.lookup_or_insert(&s, &mut Unlimited, key.as_bytes(), 16, || n)?;
            let expected = if round == 0 { Lookup::Inserted } else { Lookup::Found };
            assert_eq!(l, expected);
            assert_eq!(t.value(id), Some(&n));
            assert_eq!(t.probe(hash(&s, key.as_bytes()), key.as_bytes()).1, Some(id));
            assert_eq!(t.find(&s, key.as_bytes()), Some(id));
        }
    }
    assert_eq!(t.find(&s, b"a0y"), None);
    t.clear(&mut Unlimited);
    assert_eq!(t.find(&s, b"a0z"), None);
    let (id, l) = t.lookup_or_insert(&s, &mut Unlimited, b"a1z", 16, || 7)?;
    assert_eq!((id, l), (EntryId(0), Lookup::Inserted));
    assert_eq!(t.find(&s, b"a0z"), None);
    Ok(())
}
