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
