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

#[test]
fn fifth_edition_accepts_every_fourth_edition_name_character() {
    for unit in 0..=u16::MAX {
        let c = u32::from(unit);
        if ucs2_is_name_start(unit) {
            assert!(fifth_is_name_start(c), "U+{c:04X} start");
        }
        if ucs2_is_name_char(unit) {
            assert!(fifth_is_name_char(c), "U+{c:04X} char");
        }
    }
}

#[test]
fn fifth_edition_classes() {
    // U+0221 (Unicode 4.0) and U+10000 (outside the BMP) are new.
    assert!(!ucs2_is_name_start(0x221));
    assert!(fifth_is_name_start(0x221));
    assert!(fifth_is_name_start(0x1_0000));
    assert!(!fifth_is_name_start(0xF_0000));
    // Still not names: U+00D7, U+3000 (space), U+FFFE.
    for c in [0xD7, 0xF7, 0x3000, 0xFFFE, 0xFFFF, 0x37E] {
        assert!(!fifth_is_name_char(c), "U+{c:04X}");
    }
    // Name characters that cannot start a name.
    for c in [0x2D, 0x2E, 0x30, 0xB7, 0x300, 0x36F, 0x203F, 0x2040] {
        assert!(fifth_is_name_char(c) && !fifth_is_name_start(c), "U+{c:04X}");
    }
}

#[test]
fn utf8_decode_matches_char() {
    for c in ['\u{80}', '\u{7FF}', '\u{800}', '\u{FFFD}', '\u{10000}', '\u{10FFFF}'] {
        let mut b = [0u8; 4];
        let s = c.encode_utf8(&mut b);
        assert_eq!(utf8_decode(s.as_bytes()), Some(u32::from(c)));
    }
    assert_eq!(utf8_decode(&[0xC0, 0xAF]), None);
    assert_eq!(utf8_decode(&[0xED, 0xA0, 0x80]), None);
    assert_eq!(utf8_decode(&[0xF4, 0x90, 0x80, 0x80]), None);
}
