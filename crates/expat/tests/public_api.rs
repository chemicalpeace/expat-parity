//! Module 12, first slice (D-038): the public API on its own.
//!
//! This file uses only `expat::{Parser, Options, Error, ErrorCode}`, never
//! `expat::internal`, so it compiles in every feature combination. CI runs
//! it with the `internal-api` feature off in `cargo test -p expat` (std on)
//! and `cargo test -p expat --no-default-features` (std off); a workspace
//! build turns `internal-api` on through `expat-difftest`, so only those two
//! steps show that the public API does not depend on it. The expected codes
//! and positions are libexpat 2.9.0's for the same input (salt 1..=16);
//! `tests/tests/public_api_diff.rs` compares the same calls with the C
//! oracle on many documents.

use expat::{Error, ErrorCode, Options, Parser};

const SALT: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

/// The code and position of a failed call (`None` if it succeeded).
fn failure(r: Result<(), Error>) -> Option<(ErrorCode, (u64, u64, i64))> {
    r.err()
        .map(|e| (e.code(), (e.line(), e.column(), e.byte_index())))
}

#[test]
fn well_formed_document_in_one_call() {
    let mut p = Parser::new(SALT);
    assert_eq!(p.parse(b"<doc>hello</doc>", true), Ok(()));
}

#[test]
fn well_formed_document_in_pieces() {
    let mut p = Parser::new(SALT);
    assert_eq!(p.parse(b"<do", false), Ok(()));
    assert_eq!(p.parse(b"c>hel", false), Ok(()));
    assert_eq!(p.parse(b"lo</doc>", false), Ok(()));
    assert_eq!(p.parse(b"", true), Ok(()));
}

#[test]
fn default_encoding_reads_the_declaration_and_utf8() {
    let mut p = Parser::new(SALT);
    let doc = "<?xml version='1.0' encoding='UTF-8'?>\n<d>\u{e9}</d>";
    assert_eq!(p.parse(doc.as_bytes(), true), Ok(()));
}

#[test]
fn default_encoding_detects_utf16() {
    let mut doc = vec![0xFF, 0xFE];
    for unit in "<d/>".encode_utf16() {
        doc.extend_from_slice(&unit.to_le_bytes());
    }
    let mut p = Parser::new(SALT);
    assert_eq!(p.parse(&doc, true), Ok(()));
}

#[test]
fn tag_mismatch_with_position() {
    let mut p = Parser::new(SALT);
    assert_eq!(
        failure(p.parse(b"<a></b>", true)),
        Some((ErrorCode::TagMismatch, (1, 5, 5)))
    );
    assert_eq!(ErrorCode::TagMismatch.to_c(), 7);
}

#[test]
fn unclosed_token_on_a_later_line() {
    let mut p = Parser::new(SALT);
    assert_eq!(
        failure(p.parse(b"<a>\n  <b>\n</a", true)),
        Some((ErrorCode::UnclosedToken, (3, 0, 10)))
    );
}

#[test]
fn junk_after_the_document_element() {
    let mut p = Parser::new(SALT);
    assert_eq!(
        failure(p.parse(b"<doc>x</doc>\n<more/>", true)),
        Some((ErrorCode::JunkAfterDocElement, (2, 0, 13)))
    );
}

#[test]
fn duplicate_attribute() {
    let mut p = Parser::new(SALT);
    assert_eq!(
        failure(p.parse(b"<d a='1' a='2'/>", true)),
        Some((ErrorCode::DuplicateAttribute, (1, 9, 9)))
    );
}

#[test]
fn empty_document() {
    let mut p = Parser::new(SALT);
    assert_eq!(
        failure(p.parse(b"", true)),
        Some((ErrorCode::NoElements, (1, 0, 0)))
    );
}

#[test]
fn call_after_the_final_call_is_finished() {
    let mut p = Parser::new(SALT);
    assert_eq!(p.parse(b"<d/>", true), Ok(()));
    let f = failure(p.parse(b"", true));
    assert_eq!(f.map(|(code, _)| code), Some(ErrorCode::Finished));
    assert_eq!(ErrorCode::Finished.to_c(), 36);
}

#[test]
fn error_code_is_non_exhaustive() {
    // Outside the crate a `match` needs a wildcard arm (`#[non_exhaustive]`).
    let describe = |c: ErrorCode| match c {
        ErrorCode::TagMismatch => "mismatch",
        ErrorCode::NoElements => "empty",
        _ => "other",
    };
    assert_eq!(describe(ErrorCode::TagMismatch), "mismatch");
    assert_eq!(describe(ErrorCode::Syntax), "other");
}

#[test]
fn errors_are_plain_values() {
    let mut p = Parser::new(SALT);
    let r = p.parse(b"<a></b>", true);
    let copy = r;
    assert_eq!(copy, r);
    assert!(format!("{r:?}").contains("TagMismatch"));
}

#[test]
fn error_texts_and_display() {
    assert_eq!(ErrorCode::TagMismatch.description(), "mismatched tag");
    assert_eq!(ErrorCode::TagMismatch.to_string(), "mismatched tag");
    let mut p = Parser::new(SALT);
    let e = p.parse(b"<a></b>", true).err();
    assert_eq!(
        e.map(|e| e.to_string()).as_deref(),
        Some("mismatched tag at line 1, column 5")
    );
    // Line 2: the column counts from the start of that line.
    let mut p = Parser::new(SALT);
    let e = p.parse(b"<a>\n</b>", true).err();
    assert_eq!(e.map(|e| (e.line(), e.column())), Some((2, 2)));
    assert_eq!(
        e.map(|e| e.to_string()).as_deref(),
        Some("mismatched tag at line 2, column 2")
    );
}

/// `std::error::Error` exists with the `std` feature (`cargo test -p
/// expat`); without it (`--no-default-features`) this test is not built.
#[cfg(feature = "std")]
#[test]
fn error_is_a_std_error() {
    let mut p = Parser::new(SALT);
    let boxed: Option<Box<dyn std::error::Error>> = p
        .parse(b"<a></b>", true)
        .err()
        .map(|e| Box::new(e) as Box<dyn std::error::Error>);
    assert_eq!(
        boxed.as_ref().map(|e| e.to_string()).as_deref(),
        Some("mismatched tag at line 1, column 5")
    );
    assert!(boxed.is_some_and(|e| e.source().is_none()));
}

/// The code and position of a failed call with `options`, from
/// `Parser::with_options` (`None` if it succeeded). The expected values
/// below are libexpat 2.9.0's (`tests/tests/public_api_diff.rs`, S3).
fn with_options(options: Options, doc: &[u8]) -> Option<(ErrorCode, (u64, u64, i64))> {
    failure(Parser::with_options(SALT, options).parse(doc, true))
}

#[test]
fn default_options_are_new() {
    assert_eq!(Options::new(), Options::default());
    for doc in [
        &b"<doc>hello</doc>"[..],
        b"<a></b>",
        b"<p:a/>",
        b"<a>caf\xe9</a>",
    ] {
        assert_eq!(
            failure(Parser::new(SALT).parse(doc, true)),
            with_options(Options::default(), doc)
        );
    }
}

#[test]
fn encoding_name() {
    let latin1 = b"<a>caf\xe9</a>";
    assert_eq!(
        failure(Parser::new(SALT).parse(latin1, true)),
        Some((ErrorCode::InvalidToken, (1, 6, 6)))
    );
    assert_eq!(
        with_options(Options::new().encoding(b"ISO-8859-1"), latin1),
        None
    );
    assert_eq!(
        with_options(Options::new().encoding(b"iso-8859-1"), latin1),
        None
    );
    // The name ends at its first NUL byte, as a C string does.
    assert_eq!(
        with_options(Options::new().encoding(b"ISO-8859-1\0X"), latin1),
        None
    );
    let unknown = Some((ErrorCode::UnknownEncoding, (1, 0, -1)));
    assert_eq!(
        with_options(Options::new().encoding(b"\0UTF-8"), b"<a/>"),
        unknown
    );
    assert_eq!(with_options(Options::new().encoding(b""), b"<a/>"), unknown);
    assert_eq!(
        with_options(Options::new().encoding(b"KOI8-R"), b"<a/>"),
        unknown
    );
    // The override wins over the encoding of the XML declaration.
    let decl = b"<?xml version='1.0' encoding='KOI8-R'?><a/>";
    assert_eq!(with_options(Options::new().encoding(b"UTF-8"), decl), None);
    let utf16: Vec<u8> = "<a>x</a>"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_eq!(
        with_options(Options::new().encoding(b"UTF-16LE"), &utf16),
        None
    );
}

#[test]
fn namespace_separator() {
    let ns = |sep: u8| Options::new().namespace_separator(sep);
    let unbound = Some((ErrorCode::UnboundPrefix, (1, 0, 0)));
    assert_eq!(with_options(Options::new(), b"<p:a/>"), None);
    assert_eq!(with_options(ns(b'|'), b"<p:a/>"), unbound);
    // 0 turns namespace processing on, as in C.
    assert_eq!(with_options(ns(0), b"<p:a/>"), unbound);
    assert_eq!(with_options(ns(0), b"<p:a xmlns:p='u'/>"), None);
    // A namespace name containing a non-URI separator is a syntax error.
    let syntax = Some((ErrorCode::Syntax, (1, 0, 0)));
    assert_eq!(with_options(ns(b'|'), b"<a xmlns='u|v'/>"), syntax);
    assert_eq!(with_options(ns(b':'), b"<a xmlns='u:v'/>"), None);
    assert_eq!(with_options(ns(0xc3), b"<a xmlns='u\xc3\xa9v'/>"), syntax);
    assert_eq!(with_options(ns(0xe9), b"<a xmlns='u\xc3\xa9v'/>"), None);
}
