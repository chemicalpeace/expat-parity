use super::*;

#[test]
fn discriminants_match_c_order() {
    assert_eq!(ByteType::NonXml as u8, 0);
    assert_eq!(ByteType::S as u8, 21);
    assert_eq!(ByteType::NonAscii as u8, 29);
    assert_eq!(ByteType::Verbar as u8, 36);
}

#[test]
fn internal_tables_treat_cr_as_space() {
    assert_eq!(Table::Utf8.byte_type(b'\r'), ByteType::Cr);
    assert_eq!(Table::InternalUtf8.byte_type(b'\r'), ByteType::S);
    for t in Table::ALL {
        for b in 0..=255u8 {
            if b != b'\r' && b != b':' {
                let base = match t {
                    Table::InternalUtf8 => Table::Utf8,
                    Table::InternalUtf8Ns => Table::Utf8Ns,
                    other => other,
                };
                assert_eq!(t.byte_type(b), base.byte_type(b));
            }
        }
    }
}

#[test]
fn ascii_tables_reject_high_bytes() {
    for b in 0x80..=0xFFu8 {
        assert_eq!(Table::Ascii.byte_type(b), ByteType::NonXml);
        assert_eq!(Table::AsciiNs.byte_type(b), ByteType::NonXml);
    }
}

#[test]
fn utf8_lead_bytes() {
    assert_eq!(Table::Utf8.byte_type(0xC2), ByteType::Lead2);
    assert_eq!(Table::Utf8.byte_type(0xE0), ByteType::Lead3);
    assert_eq!(Table::Utf8.byte_type(0xF0), ByteType::Lead4);
    assert_eq!(Table::Utf8.byte_type(0x80), ByteType::Trail);
}

#[test]
fn entries_follow_the_variant_order() {
    use core::ptr;
    let expected: [(Table, &[ByteType; 256]); 10] = [
        (Table::Utf8, &UTF8),
        (Table::Utf8Ns, &UTF8_NS),
        (Table::InternalUtf8, &INTERNAL_UTF8),
        (Table::InternalUtf8Ns, &INTERNAL_UTF8_NS),
        (Table::Latin1, &LATIN1),
        (Table::Latin1Ns, &LATIN1_NS),
        (Table::Ascii, &ASCII),
        (Table::AsciiNs, &ASCII_NS),
        (Table::Little2, &LATIN1),
        (Table::Little2Ns, &LATIN1_NS),
    ];
    for (i, (t, e)) in expected.into_iter().enumerate() {
        assert_eq!(Table::ALL[i], t);
        assert!(ptr::eq(t.entries(), e), "{t:?}");
    }
}
