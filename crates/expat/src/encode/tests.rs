use super::*;
use proptest::prelude::*;

#[test]
fn matches_core_for_all_scalar_values() {
    let mut a = [0u8; 4];
    let mut w = [0u16; 2];
    // Miri checks for UB, not domain coverage; the full domain runs natively.
    let last = if cfg!(miri) { 0x1_0100 } else { MAX_CODE_POINT };
    // Under Miri also visit the top of the range; natively it is already in.
    let tail = (MAX_CODE_POINT - 4..=MAX_CODE_POINT).filter(|_| cfg!(miri));
    for cp in (0..=last).chain(tail) {
        let Some(c) = char::from_u32(cp) else { continue };
        let n = utf8_encode(cp, &mut a);
        let mut want = [0u8; 4];
        assert_eq!(&a[..n], c.encode_utf8(&mut want).as_bytes());
        let n = utf16_encode(cp, &mut w);
        let mut want16 = [0u16; 2];
        assert_eq!(&w[..n], c.encode_utf16(&mut want16));
    }
}

#[test]
fn surrogates_are_encoded_like_c() {
    let mut a = [0u8; 4];
    assert_eq!(utf8_encode(0xD800, &mut a), 3);
    assert_eq!(a[..3], [0xED, 0xA0, 0x80]);
    let mut w = [0u16; 2];
    assert_eq!(utf16_encode(0xDFFF, &mut w), 1);
    assert_eq!(w[0], 0xDFFF);
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: if cfg!(miri) { 8 } else { 256 },
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn out_of_range_writes_nothing(c in (MAX_CODE_POINT + 1)..=u32::MAX) {
        let mut a = [0xAAu8; 4];
        prop_assert_eq!(utf8_encode(c, &mut a), 0);
        prop_assert_eq!(a, [0xAA; 4]);
        let mut w = [0x5555u16; 2];
        prop_assert_eq!(utf16_encode(c, &mut w), 0);
        prop_assert_eq!(w, [0x5555; 2]);
    }
}
