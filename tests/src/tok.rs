//! Differential checks of the tokenizer port (`expat::internal::tok`) against
//! the C tokenizer (`expat_oracle::tok`), shared by the integration tests and
//! the `tok_diff` fuzz target.
//!
//! [`check_buffer`] runs every scanner from a set of start offsets and
//! compares token and `next` exactly. For every token that a scanner
//! accepts, it also compares the helpers whose C precondition that token
//! establishes (`getAtts` on start tags, `charRefNumber` on character
//! references, `predefinedEntityName` and `nameMatchesAscii` on entity
//! names, `isPublicId` on literals), and it compares the helpers that are
//! total in C (`nameLength`, `skipS`, `updatePosition`) at every offset.
//! Any difference panics with the encoding, scanner, offset and input.

use expat::internal::tok::{self as rs, Normal, Scan, Tok};
use expat_oracle::TypeTable;
use expat_oracle::tok::{self as c, Scanner};

/// The C table of a Rust normal encoding.
pub fn table_of(enc: Normal) -> TypeTable {
    match enc {
        Normal::Utf8 => TypeTable::Utf8,
        Normal::Utf8Ns => TypeTable::Utf8Ns,
        Normal::InternalUtf8 => TypeTable::InternalUtf8,
        Normal::InternalUtf8Ns => TypeTable::InternalUtf8Ns,
        Normal::Latin1 => TypeTable::Latin1,
        Normal::Latin1Ns => TypeTable::Latin1Ns,
        Normal::Ascii => TypeTable::Ascii,
        Normal::AsciiNs => TypeTable::AsciiNs,
    }
}

fn rust_scan(enc: Normal, which: Scanner, buf: &[u8], ptr: usize) -> rs::Scanned {
    match which {
        Scanner::Prolog => rs::prolog_tok(&enc, buf, ptr),
        Scanner::Content => rs::content_tok(&enc, buf, ptr),
        Scanner::CdataSection => rs::cdata_section_tok(&enc, buf, ptr),
        Scanner::IgnoreSection => rs::ignore_section_tok(&enc, buf, ptr),
        Scanner::AttributeValue => rs::attribute_value_tok(&enc, buf, ptr),
        Scanner::EntityValue => rs::entity_value_tok(&enc, buf, ptr),
    }
}

/// Names compared with `nameMatchesAscii` (the ones `xmlparse.c` uses).
const ASCII_NAMES: [&[u8]; 8] = [
    b"ID",
    b"IDREF",
    b"CDATA",
    b"NOTATION",
    b"xml",
    b"xmlns",
    b"lt",
    b"x",
];

/// Compares one scanner call and, for accepted tokens, the helpers that the
/// token's C precondition allows. Returns the (equal) Rust result.
pub fn check_scan(enc: Normal, which: Scanner, buf: &[u8], ptr: usize) -> rs::Scanned {
    let t = table_of(enc);
    let r = rust_scan(enc, which, buf, ptr);
    let (ct, cn) = c::scan(t, which, buf, ptr);
    assert_eq!(
        (r.scan.to_c(), r.next),
        (ct, cn),
        "{enc:?} {which:?} at {ptr} of {buf:?}"
    );
    if let (Scan::Token(tok), Some(next)) = (r.scan, r.next) {
        check_token_helpers(enc, buf, ptr, tok, next);
    }
    r
}

/// Helpers whose precondition is "this is a token of type `tok` spanning
/// `buf[ptr..next]`".
fn check_token_helpers(enc: Normal, buf: &[u8], ptr: usize, tok: Tok, next: usize) {
    let t = table_of(enc);
    match tok {
        Tok::StartTagWithAtts
        | Tok::EmptyElementWithAtts
        | Tok::StartTagNoAtts
        | Tok::EmptyElementNoAtts => {
            // `getAtts` on the tag, with too little and with enough room.
            for max in [0, 1, 2, 5] {
                let mut atts = vec![rs::Attribute::default(); max];
                let n = rs::get_atts(&enc, buf, ptr, &mut atts);
                let (cn, catts) = c::get_atts(t, buf, ptr, max);
                let ratts: Vec<c::Attribute> = atts
                    .iter()
                    .map(|a| c::Attribute {
                        name: a.name,
                        value_ptr: a.value_ptr,
                        value_end: a.value_end,
                        normalized: a.normalized,
                    })
                    .collect();
                assert_eq!(
                    (n, ratts),
                    (cn, catts),
                    "{enc:?} getAtts({max}) at {ptr} of {buf:?}"
                );
            }
            // The element name follows `<`.
            let name = ptr + 1;
            assert_eq!(
                rs::name_length(&enc, buf, name),
                c::name_length(t, buf, name),
                "{enc:?} nameLength at {name} of {buf:?}"
            );
        }
        Tok::CharRef => {
            assert_eq!(
                rs::char_ref_number(&enc, buf, ptr),
                c::char_ref_number(t, buf, ptr),
                "{enc:?} charRefNumber at {ptr} of {buf:?}"
            );
        }
        Tok::EntityRef | Tok::ParamEntityRef => {
            // The name is between `&`/`%` and `;`.
            let name = buf.get(ptr + 1..next - 1).expect("token inside buffer");
            assert_eq!(
                rs::predefined_entity_name(&enc, name, 0).map_or(0, i32::from),
                c::predefined_entity_name(t, name, 0),
                "{enc:?} predefinedEntityName {name:?}"
            );
            for a in ASCII_NAMES {
                assert_eq!(
                    rs::name_matches_ascii(&enc, name, 0, a),
                    c::name_matches_ascii(t, name, 0, a),
                    "{enc:?} nameMatchesAscii {name:?} {a:?}"
                );
            }
        }
        Tok::Literal => {
            let lit = buf.get(ptr..next).expect("token inside buffer");
            let r = rs::is_public_id(&enc, lit, 0).map_err(Some);
            assert_eq!(r, c::is_public_id(t, lit, 0), "{enc:?} isPublicId {lit:?}");
        }
        Tok::Name | Tok::PrefixedName | Tok::Nmtoken => {
            let name = buf.get(ptr..next).expect("token inside buffer");
            for a in ASCII_NAMES {
                assert_eq!(
                    rs::name_matches_ascii(&enc, name, 0, a),
                    c::name_matches_ascii(t, name, 0, a),
                    "{enc:?} nameMatchesAscii {name:?} {a:?}"
                );
            }
        }
        _ => {}
    }
}

/// Compares the helpers that are total in C at offset `ptr`.
pub fn check_total_helpers(enc: Normal, buf: &[u8], ptr: usize) {
    let t = table_of(enc);
    assert_eq!(
        rs::name_length(&enc, buf, ptr),
        c::name_length(t, buf, ptr),
        "{enc:?} nameLength at {ptr} of {buf:?}"
    );
    assert_eq!(
        rs::skip_s(&enc, buf, ptr),
        c::skip_s(t, buf, ptr),
        "{enc:?} skipS at {ptr} of {buf:?}"
    );
    // Start near the top of the range to compare the wrap-around, except
    // where C's `XML_Size` is 32 bits (Windows); see DECISIONS D-016.
    let column = if cfg!(windows) { 3 } else { u64::MAX - 2 };
    let mut pos = rs::Position {
        line_number: 7,
        column_number: column,
    };
    rs::update_position(&enc, buf, ptr, &mut pos);
    assert_eq!(
        (pos.line_number, pos.column_number),
        c::update_position(t, buf, ptr, (7, column)),
        "{enc:?} updatePosition at {ptr} of {buf:?}"
    );
    let r = rs::is_public_id(&enc, buf, ptr).map_err(Some);
    assert_eq!(
        r,
        c::is_public_id(t, buf, ptr),
        "{enc:?} isPublicId at {ptr} of {buf:?}"
    );
    let r = rs::predefined_entity_name(&enc, buf, ptr).map_or(0, i32::from);
    assert_eq!(
        r,
        c::predefined_entity_name(t, buf, ptr),
        "{enc:?} predefinedEntityName at {ptr}"
    );
}

/// Runs [`check_scan`] for every scanner and [`check_total_helpers`] at each
/// offset in `offsets` (clamped to the buffer), for one encoding.
pub fn check_buffer(enc: Normal, buf: &[u8], offsets: impl IntoIterator<Item = usize>) {
    for ptr in offsets {
        if ptr > buf.len() {
            break;
        }
        for which in Scanner::ALL {
            check_scan(enc, which, buf, ptr);
        }
        check_total_helpers(enc, buf, ptr);
    }
}

/// Tokenizes `buf` the way the parser walks it (from token to token, in the
/// content and the prolog state) and checks every step. Exercises long
/// realistic token sequences rather than single offsets.
pub fn check_token_walk(enc: Normal, buf: &[u8]) {
    for which in [Scanner::Prolog, Scanner::Content] {
        let mut ptr = 0;
        loop {
            let r = check_scan(enc, which, buf, ptr);
            match (r.scan, r.next) {
                (Scan::Token(_), Some(next)) if next > ptr && next <= buf.len() => ptr = next,
                // Resynchronise after an error or a partial token.
                _ if ptr < buf.len() => ptr += 1,
                _ => break,
            }
        }
    }
}
