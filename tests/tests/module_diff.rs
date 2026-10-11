//! White-box differential tests of ported leaf modules against the C statics
//! (reached through crates/expat-oracle/shim/oracle_internals.c).
//! Domains are covered exhaustively where they are small enough.

use expat::internal::{chartab, encode, naming, siphash};
use expat_oracle::{Naming, TypeTable, Utf8Predicate};
use proptest::prelude::*;

fn rust_table(t: TypeTable) -> chartab::Table {
    match t {
        TypeTable::Utf8 => chartab::Table::Utf8,
        TypeTable::Utf8Ns => chartab::Table::Utf8Ns,
        TypeTable::InternalUtf8 => chartab::Table::InternalUtf8,
        TypeTable::InternalUtf8Ns => chartab::Table::InternalUtf8Ns,
        TypeTable::Latin1 => chartab::Table::Latin1,
        TypeTable::Latin1Ns => chartab::Table::Latin1Ns,
        TypeTable::Ascii => chartab::Table::Ascii,
        TypeTable::AsciiNs => chartab::Table::AsciiNs,
        TypeTable::Little2 => chartab::Table::Little2,
        TypeTable::Little2Ns => chartab::Table::Little2Ns,
    }
}

#[test]
fn chartab_all_tables_all_bytes() {
    for t in TypeTable::ALL {
        let c = expat_oracle::type_table(t);
        let r = rust_table(t);
        for b in 0..=255u8 {
            assert_eq!(
                r.byte_type(b) as u8,
                c[usize::from(b)],
                "{t:?} byte {b:#04x}"
            );
        }
    }
}

#[test]
fn naming_all_bmp_code_units() {
    for unit in 0..=u16::MAX {
        assert_eq!(
            naming::ucs2_is_name_start(unit),
            expat_oracle::ucs2_naming(Naming::NameStart, unit),
            "name start {unit:#06x}"
        );
        assert_eq!(
            naming::ucs2_is_name_char(unit),
            expat_oracle::ucs2_naming(Naming::NameChar, unit),
            "name char {unit:#06x}"
        );
    }
}

fn c_pred(p: Utf8Predicate, b: &[u8]) -> bool {
    expat_oracle::utf8_predicate(p, b).expect("buffer long enough")
}

#[test]
fn utf8_two_byte_predicates_exhaustive() {
    for b0 in 0..=255u8 {
        for b1 in 0..=255u8 {
            let b = [b0, b1];
            assert_eq!(
                naming::utf8_is_name_char2(b),
                c_pred(Utf8Predicate::IsName2, &b)
            );
            assert_eq!(
                naming::utf8_is_name_start2(b),
                c_pred(Utf8Predicate::IsNmstrt2, &b)
            );
            assert_eq!(
                naming::utf8_is_invalid2(b),
                c_pred(Utf8Predicate::IsInvalid2, &b),
                "{b:02x?}"
            );
        }
    }
}

#[test]
fn utf8_three_byte_predicates_exhaustive_over_lead3() {
    for b0 in 0xE0..=0xEFu8 {
        for b1 in 0..=255u8 {
            for b2 in 0..=255u8 {
                let b = [b0, b1, b2];
                assert_eq!(
                    naming::utf8_is_name_char3(b),
                    c_pred(Utf8Predicate::IsName3, &b)
                );
                assert_eq!(
                    naming::utf8_is_name_start3(b),
                    c_pred(Utf8Predicate::IsNmstrt3, &b)
                );
                assert_eq!(
                    naming::utf8_is_invalid3(b),
                    c_pred(Utf8Predicate::IsInvalid3, &b),
                    "{b:02x?}"
                );
            }
        }
    }
}

#[test]
fn utf8_four_byte_invalid_over_lead4_and_boundary_trails() {
    const EDGES: [u8; 8] = [0x00, 0x7F, 0x80, 0x8F, 0x90, 0xBF, 0xC0, 0xFF];
    for b0 in 0xF0..=0xF7u8 {
        for b1 in 0..=255u8 {
            for b2 in EDGES {
                for b3 in EDGES {
                    let b = [b0, b1, b2, b3];
                    assert_eq!(
                        naming::utf8_is_invalid4(b),
                        c_pred(Utf8Predicate::IsInvalid4, &b),
                        "{b:02x?}"
                    );
                }
            }
        }
    }
}

#[test]
fn encoders_all_code_points_and_beyond() {
    let mut b8 = [0u8; 4];
    let mut b16 = [0u16; 2];
    for c in (0..=0x11_0010u32).chain([0x7FFF_FFFF]) {
        let n = encode::utf8_encode(c, &mut b8);
        let ci = i32::try_from(c).expect("fits");
        assert_eq!(
            &b8[..n],
            expat_oracle::utf8_encode(ci).as_slice(),
            "utf8 {c:#x}"
        );
        let n = encode::utf16_encode(c, &mut b16);
        assert_eq!(
            &b16[..n],
            expat_oracle::utf16_encode(ci).as_slice(),
            "utf16 {c:#x}"
        );
    }
}

/// Rust SipHash-2-4 with `update` fed in pieces of `split` bytes (`0` = one
/// call), mirroring `expat_oracle::siphash24`.
fn rust_siphash(key: &[u8; 16], data: &[u8], split: usize) -> u64 {
    let mut h = siphash::SipHasher24::new(&siphash::SipKey::from_bytes(*key));
    if split == 0 {
        h.update(data);
    } else {
        for piece in data.chunks(split) {
            h.update(piece);
        }
    }
    h.finish()
}

#[test]
fn siphash_c_self_test_passes() {
    assert!(expat_oracle::sip24_valid());
}

#[test]
fn siphash_all_lengths_and_splits() {
    let key: [u8; 16] = core::array::from_fn(|i| u8::try_from(i).expect("< 16").wrapping_mul(37));
    let data: Vec<u8> = (0..=255u8)
        .map(|b| b.wrapping_mul(101).wrapping_add(7))
        .collect();
    for len in 0..=80 {
        for split in 0..=17 {
            assert_eq!(
                rust_siphash(&key, &data[..len], split),
                expat_oracle::siphash24(&key, &data[..len], split),
                "len {len} split {split}"
            );
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20_000))]

    #[test]
    fn siphash_arbitrary(key in any::<[u8; 16]>(), data in proptest::collection::vec(any::<u8>(), 0..300), split in 0usize..40) {
        prop_assert_eq!(rust_siphash(&key, &data, split), expat_oracle::siphash24(&key, &data, split));
    }

    #[test]
    fn utf8_predicates_on_arbitrary_bytes(b in any::<[u8; 4]>()) {
        let [b0, b1, b2, b3] = b;
        prop_assert_eq!(naming::utf8_is_name_char3([b0, b1, b2]), c_pred(Utf8Predicate::IsName3, &b[..3]));
        prop_assert_eq!(naming::utf8_is_name_start3([b0, b1, b2]), c_pred(Utf8Predicate::IsNmstrt3, &b[..3]));
        prop_assert_eq!(naming::utf8_is_invalid3([b0, b1, b2]), c_pred(Utf8Predicate::IsInvalid3, &b[..3]));
        prop_assert_eq!(naming::utf8_is_invalid4([b0, b1, b2, b3]), c_pred(Utf8Predicate::IsInvalid4, &b));
    }
}
