//! Differential target for the ported leaf modules (byte tables, naming
//! predicates, UTF-8 validity, encoders, SipHash) against the C statics. Exhaustive
//! tests already cover most of these domains; this target keeps them under
//! continuous random exploration with sanitizers on the C side.
#![no_main]

use expat::internal::{chartab, encode, naming, siphash};
use expat_oracle::{Naming, TypeTable, Utf8Predicate};
use libfuzzer_sys::fuzz_target;

fn c(p: Utf8Predicate, b: &[u8]) -> bool {
    expat_oracle::utf8_predicate(p, b).expect("long enough")
}

fuzz_target!(|data: &[u8]| {
    for w in data.windows(4) {
        let [b0, b1, b2, b3] = [w[0], w[1], w[2], w[3]];
        assert_eq!(
            naming::utf8_is_name_char2([b0, b1]),
            c(Utf8Predicate::IsName2, w)
        );
        assert_eq!(
            naming::utf8_is_name_start2([b0, b1]),
            c(Utf8Predicate::IsNmstrt2, w)
        );
        assert_eq!(
            naming::utf8_is_invalid2([b0, b1]),
            c(Utf8Predicate::IsInvalid2, w)
        );
        assert_eq!(
            naming::utf8_is_name_char3([b0, b1, b2]),
            c(Utf8Predicate::IsName3, w)
        );
        assert_eq!(
            naming::utf8_is_name_start3([b0, b1, b2]),
            c(Utf8Predicate::IsNmstrt3, w)
        );
        assert_eq!(
            naming::utf8_is_invalid3([b0, b1, b2]),
            c(Utf8Predicate::IsInvalid3, w)
        );
        assert_eq!(
            naming::utf8_is_invalid4([b0, b1, b2, b3]),
            c(Utf8Predicate::IsInvalid4, w)
        );
        let unit = u16::from_be_bytes([b0, b1]);
        assert_eq!(
            naming::ucs2_is_name_start(unit),
            expat_oracle::ucs2_naming(Naming::NameStart, unit)
        );
        assert_eq!(
            naming::ucs2_is_name_char(unit),
            expat_oracle::ucs2_naming(Naming::NameChar, unit)
        );
        let cp = u32::from_le_bytes([b0, b1, b2, b3 & 0x7F]);
        let ci = i32::try_from(cp).expect("masked to 31 bits");
        let mut b8 = [0u8; 4];
        let n = encode::utf8_encode(cp, &mut b8);
        assert_eq!(&b8[..n], expat_oracle::utf8_encode(ci).as_slice());
        let mut b16 = [0u16; 2];
        let n = encode::utf16_encode(cp, &mut b16);
        assert_eq!(&b16[..n], expat_oracle::utf16_encode(ci).as_slice());
    }
    // SipHash: key = first 16 bytes, split = byte 16, message = the rest.
    if let Some((head, msg)) = data.split_first_chunk::<17>() {
        let (key, split) = head.split_first_chunk::<16>().expect("17 = 16 + 1");
        let split = usize::from(split[0] % 24);
        let mut h = siphash::SipHasher24::new(&siphash::SipKey::from_bytes(*key));
        if split == 0 {
            h.update(msg);
        } else {
            for piece in msg.chunks(split) {
                h.update(piece);
            }
        }
        assert_eq!(h.finish(), expat_oracle::siphash24(key, msg, split));
    }
    if let Some(&b) = data.first() {
        for t in [TypeTable::Utf8, TypeTable::Latin1Ns, TypeTable::AsciiNs] {
            let rt = match t {
                TypeTable::Utf8 => chartab::Table::Utf8,
                TypeTable::Latin1Ns => chartab::Table::Latin1Ns,
                _ => chartab::Table::AsciiNs,
            };
            assert_eq!(
                rt.byte_type(b) as u8,
                expat_oracle::type_table(t)[usize::from(b)]
            );
        }
    }
});
