use super::*;

fn content(doc: &[u8]) -> Scanned {
    content_tok(&Normal::Utf8, doc, 0)
}

fn prolog(doc: &[u8]) -> Scanned {
    prolog_tok(&Normal::Utf8Ns, doc, 0)
}

#[test]
fn c_values_of_scan_results() {
    assert_eq!(Scan::Token(Tok::IgnoreSect).to_c(), 42);
    assert_eq!(Scan::PartialToken(Tok::PrologS).to_c(), -15);
    assert_eq!(Scan::Invalid.to_c(), 0);
    assert_eq!(Scan::TrailingRsqb.to_c(), -5);
}

#[test]
fn content_tokens() {
    let t = |s, n| Scanned { scan: Scan::Token(s), next: Some(n) };
    assert_eq!(content(b"<a>"), t(Tok::StartTagNoAtts, 3));
    assert_eq!(content(b"<a x='1' y=\"2\">z"), t(Tok::StartTagWithAtts, 15));
    assert_eq!(content(b"</a >"), t(Tok::EndTag, 5));
    assert_eq!(content(b"&amp;"), t(Tok::EntityRef, 5));
    assert_eq!(content(b"&#x1F;"), t(Tok::CharRef, 6));
    assert_eq!(content(b"<!--c-->"), t(Tok::Comment, 8));
    assert_eq!(content(b"<![CDATA[x"), t(Tok::CdataSectOpen, 9));
    assert_eq!(content(b"<?pi x?>"), t(Tok::Pi, 8));
    assert_eq!(content(b"<?xml v?>"), t(Tok::XmlDecl, 9));
    assert_eq!(content(b"ab\xc3\xa9c<"), t(Tok::DataChars, 5));
    assert_eq!(content(b"\r\n"), t(Tok::DataNewline, 2));
}

#[test]
fn content_partial_and_invalid() {
    assert_eq!(content(b"").scan, Scan::None);
    assert_eq!(content(b"\r").scan, Scan::TrailingCr);
    assert_eq!(content(b"]]").scan, Scan::TrailingRsqb);
    assert_eq!(content(b"<a"), Scanned { scan: Scan::Partial, next: None });
    assert_eq!(content(b"<\xc3"), Scanned { scan: Scan::PartialChar, next: None });
    assert_eq!(content(b"]]>"), Scanned { scan: Scan::Invalid, next: Some(2) });
    assert_eq!(content(b"x]]>"), Scanned { scan: Scan::Invalid, next: Some(3) });
    assert_eq!(content(b"<?XmL ?>"), Scanned { scan: Scan::Invalid, next: Some(5) });
    // Without namespaces a second colon is fine; with them it is not.
    assert_eq!(content(b"<a:b:c/>").scan, Scan::Token(Tok::EmptyElementNoAtts));
    let ns = content_tok(&Normal::Utf8Ns, b"<a:b:c/>", 0);
    assert_eq!(ns, Scanned { scan: Scan::Invalid, next: Some(4) });
}

#[test]
fn prolog_tokens() {
    assert_eq!(prolog(b"  \n<").scan, Scan::Token(Tok::PrologS));
    assert_eq!(prolog(b"\r"), Scanned { scan: Scan::PartialToken(Tok::PrologS), next: Some(1) });
    assert_eq!(prolog(b"<!DOCTYPE d").next, Some(9));
    assert_eq!(prolog(b"<d").scan, Scan::Token(Tok::InstanceStart));
    assert_eq!(prolog(b"p:q ").scan, Scan::Token(Tok::PrefixedName));
    assert_eq!(prolog(b"p:q:r ").scan, Scan::Token(Tok::Nmtoken));
    assert_eq!(prolog(b"name*").scan, Scan::Token(Tok::NameAsterisk));
    assert_eq!(prolog(b"-n*").scan, Scan::Invalid);
    assert_eq!(prolog(b"name").scan, Scan::PartialToken(Tok::Name));
    assert_eq!(prolog(b"'lit' ").scan, Scan::Token(Tok::Literal));
    assert_eq!(prolog(b"'lit'").scan, Scan::PartialToken(Tok::Literal));
    assert_eq!(prolog(b")*").scan, Scan::Token(Tok::CloseParenAsterisk));
    assert_eq!(prolog(b"]]>").scan, Scan::Token(Tok::CondSectClose));
    assert_eq!(prolog(b"#PCDATA)").scan, Scan::Token(Tok::PoundName));
    assert_eq!(prolog(b"%pe;").scan, Scan::Token(Tok::ParamEntityRef));
    assert_eq!(prolog(b"% ").scan, Scan::Token(Tok::Percent));
}

#[test]
fn literal_and_section_scanners() {
    let e = Normal::Utf8;
    assert_eq!(attribute_value_tok(&e, b" x", 0).scan, Scan::Token(Tok::AttributeValueS));
    assert_eq!(attribute_value_tok(&e, b"a<", 0), Scanned { scan: Scan::Invalid, next: Some(1) });
    assert_eq!(entity_value_tok(&e, b"% x", 0), Scanned { scan: Scan::Invalid, next: Some(1) });
    assert_eq!(entity_value_tok(&e, b"a<b", 0).scan, Scan::Token(Tok::DataChars));
    assert_eq!(cdata_section_tok(&e, b"]]>", 0).scan, Scan::Token(Tok::CdataSectClose));
    assert_eq!(
        ignore_section_tok(&e, b"<![ x ]]> ]]>tail", 0),
        Scanned { scan: Scan::Token(Tok::IgnoreSect), next: Some(13) }
    );
}

#[test]
fn helpers() {
    let e = Normal::Utf8;
    let tag = b"<e a='1' b=\"x&amp;y\" c=' z'>";
    let mut atts = [Attribute::default(); 4];
    assert_eq!(get_atts(&e, tag, 0, &mut atts), 3);
    assert_eq!((atts[0].name, atts[0].value_ptr, atts[0].value_end), (3, 6, 7));
    assert!(atts[0].normalized);
    assert!(!atts[1].normalized);
    assert!(!atts[2].normalized);
    assert_eq!(char_ref_number(&e, b"&#x41;", 0), 0x41);
    assert_eq!(char_ref_number(&e, b"&#65;", 0), 65);
    assert_eq!(char_ref_number(&e, b"&#xD800;", 0), -1);
    assert_eq!(char_ref_number(&e, b"&#1114112;", 0), -1);
    assert_eq!(predefined_entity_name(&e, b"quot", 0), Some(b'"'));
    assert_eq!(predefined_entity_name(&e, b"qu0t", 0), None);
    assert!(name_matches_ascii(&e, b"ID", 0, b"ID"));
    assert!(!name_matches_ascii(&e, b"IDS", 0, b"ID"));
    assert_eq!(name_length(&e, b"ab\xc3\xa9:c d", 0), 6);
    assert_eq!(skip_s(&e, b" \r\n\tx", 0), 4);
    assert_eq!(is_public_id(&e, b"'a b'", 0), Ok(()));
    assert_eq!(is_public_id(&e, b"'a\tb'", 0), Err(2));
    let mut pos = Position::default();
    update_position(&e, b"a\r\nb\xc3\xa9\rc", 0, &mut pos);
    assert_eq!(pos, Position { line_number: 2, column_number: 1 });
}

/// Q-5 (docs/DIVERGENCES.md): `toAscii` with an unknown encoding whose map
/// makes a trailing byte a lead byte. C's `unknown_toUtf8` hands the
/// converter the whole sequence even past `fromLim` and, when it converts
/// to nothing, goes on until the output is full, reading past the data
/// (heap-buffer-overflow under ASan, docs/evidence/q5). The port converts
/// like C inside the buffer and stops where C would leave it. Port-only and
/// free of FFI, so it also runs under Miri (CI job `miri`).
mod q5 {
    use super::*;

    /// ASCII identity; `\xC3` lead of 2, `\xA9` and `\xBE` leads of 4,
    /// every other byte malformed. `C3 A9` converts to `I`, anything else
    /// to `-1` (no output).
    type Q5Enc = Unknown<fn(&[u8]) -> i32>;

    fn enc() -> Result<Q5Enc, InvalidMap> {
        let mut map = [-1i32; 256];
        for (i, m) in map.iter_mut().enumerate().take(128) {
            *m = i32::try_from(i).unwrap_or_default();
        }
        map[0xC3] = -2;
        map[0xA9] = -4;
        map[0xBE] = -4;
        let convert: fn(&[u8]) -> i32 = |seq| if seq == b"\xC3\xA9" { 0x49 } else { -1 };
        Unknown::new(&map, Some(convert), false)
    }

    const DECL: &[u8] = b"<?xml \xC3\xA9?>";

    #[test]
    fn to_ascii_stops_at_the_end_of_the_buffer() -> Result<(), InvalidMap> {
        let e = enc()?;
        for (tail, ascii, bad) in [
            // The sequence `A9 ? > ..` is not complete in the buffer.
            (&b""[..], -1, 7),
            (b"<", -1, 7),
            // C: the next character after the empty conversion (`a`).
            (b"<a", i32::from(b'a'), 8),
            (b"<a/>", i32::from(b'a'), 8),
            // A run of sequences that convert to nothing, up to the end of
            // the buffer (C continues past it, docs/evidence/q5/asan.txt).
            (&[0xBE; 4096][..], -1, 7),
            (&[0xBE; 4095][..], -1, 7),
        ] {
            let buf = [DECL, tail].concat();
            assert_eq!(to_ascii(&e, &buf, 7, 8), ascii, "tail len {}", tail.len());
            assert_eq!(to_ascii(&e, &buf, 6, 8), i32::from(b'I'));
            let d = parse_xml_decl(false, &e, false, &buf, 0, DECL.len());
            assert_eq!((d.ok, d.bad), (false, Some(bad)), "tail len {}", tail.len());
        }
        Ok(())
    }

    /// Every start and `fromLim` in and around a buffer, every output size:
    /// no panic, nothing consumed past the buffer, and conversion bounded by
    /// the buffer even when `lim` is before `from` (never reached, as in C).
    #[test]
    fn to_utf8_until_never_leaves_the_buffer() -> Result<(), InvalidMap> {
        let e = enc()?;
        let bufs: [&[u8]; 4] =
            [b"<?xml \xC3\xA9?>", b"<?xml no\xC3\xA9 ?>", b"\xA9\xBE\xBE\xBEab\xC3", b"\xC3"];
        for buf in bufs {
            let n = buf.len();
            for from in 0..=n + 2 {
                for lim in 0..=n + 2 {
                    for size in 0..=4 {
                        let mut out = [0u8; 4];
                        let to = out.get_mut(..size).unwrap_or_default();
                        let r = e.to_utf8_until(buf, from, lim, to);
                        assert!(r.written <= size);
                        assert!(from.saturating_add(r.read) <= n.max(from), "{buf:?} {from} {lim} {size}: {r:?}");
                    }
                }
            }
        }
        Ok(())
    }
}
