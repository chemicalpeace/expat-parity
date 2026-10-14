//! Module 12, first slice (D-038): the public API on its own.
//!
//! This file uses only `expat::{Parser, Error, ErrorCode}`, never
//! `expat::internal`, so it compiles in every feature combination. CI runs
//! it with the `internal-api` feature off in `cargo test -p expat` (std on)
//! and `cargo test -p expat --no-default-features` (std off); a workspace
//! build turns `internal-api` on through `expat-difftest`, so only those two
//! steps show that the public API does not depend on it. The expected codes
//! and positions are libexpat 2.9.0's for the same input (salt 1..=16);
//! `tests/tests/public_api_diff.rs` compares the same calls with the C
//! oracle on many documents.

use expat::{Error, ErrorCode, Parser};

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
