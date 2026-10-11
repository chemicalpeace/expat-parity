use super::*;

#[test]
fn ascii_names() {
    for b in b'a'..=b'z' {
        assert!(ucs2_is_name_start(u16::from(b)));
    }
    assert!(ucs2_is_name_start(u16::from(b'_')));
    assert!(ucs2_is_name_start(u16::from(b':')));
    assert!(!ucs2_is_name_start(u16::from(b'-')));
    assert!(ucs2_is_name_char(u16::from(b'-')));
    assert!(ucs2_is_name_char(u16::from(b'.')));
    assert!(!ucs2_is_name_char(u16::from(b' ')));
}

#[test]
fn utf8_predicates_agree_with_ucs2_for_valid_sequences() {
    let mut buf = [0u8; 4];
    // Miri checks for UB, not domain coverage; the full domain runs natively.
    let step = if cfg!(miri) { 97 } else { 1 };
    for cp in (0x80u32..=0xFFFF).step_by(step) {
        let Some(c) = char::from_u32(cp) else { continue };
        let s = c.encode_utf8(&mut buf).as_bytes();
        let unit = u16::try_from(cp).unwrap_or(0);
        match *s {
            [a, b] => {
                assert_eq!(utf8_is_name_start2([a, b]), ucs2_is_name_start(unit));
                assert_eq!(utf8_is_name_char2([a, b]), ucs2_is_name_char(unit));
                assert!(!utf8_is_invalid2([a, b]));
            }
            [a, b, d] => {
                assert_eq!(utf8_is_name_start3([a, b, d]), ucs2_is_name_start(unit));
                assert_eq!(utf8_is_name_char3([a, b, d]), ucs2_is_name_char(unit));
                assert_eq!(utf8_is_invalid3([a, b, d]), cp == 0xFFFE || cp == 0xFFFF);
            }
            _ => {}
        }
    }
}

#[test]
fn invalid4_bounds() {
    assert!(!utf8_is_invalid4([0xF0, 0x90, 0x80, 0x80])); // U+10000
    assert!(utf8_is_invalid4([0xF0, 0x8F, 0xBF, 0xBF])); // overlong
    assert!(!utf8_is_invalid4([0xF4, 0x8F, 0xBF, 0xBF])); // U+10FFFF
    assert!(utf8_is_invalid4([0xF4, 0x90, 0x80, 0x80])); // > U+10FFFF
}

#[test]
fn surrogates_are_invalid3() {
    assert!(utf8_is_invalid3([0xED, 0xA0, 0x80]));
    assert!(!utf8_is_invalid3([0xED, 0x9F, 0xBF]));
}
