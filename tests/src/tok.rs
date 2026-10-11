//! Differential checks of the tokenizer port (`expat::internal::tok`) against
//! the C tokenizer (`expat_oracle::tok`), shared by the integration tests and
//! the `tok_diff` fuzz target.
//!
//! A [`Subject`] pairs a Rust encoding with its C counterpart: the built-in
//! single-byte ([`Normal`]) and UTF-16 ([`Utf16`]) encodings, and unknown
//! encodings ([`UnknownPair`]) built from the same map and converter on both
//! sides.
//!
//! [`check_buffer`] runs every scanner from a set of start offsets and
//! compares token and `next` exactly. For every token that a scanner
//! accepts, it also compares the helpers whose C precondition that token
//! establishes (`getAtts` on start tags, `charRefNumber` on character
//! references, `predefinedEntityName` and `nameMatchesAscii` on entity
//! names, `isPublicId` on literals), and it compares the helpers that are
//! total in C (`nameLength`, `skipS`, `updatePosition`) at every offset.
//! [`check_convert`] compares the converters to UTF-8 and UTF-16 with a
//! range of output sizes. Any difference panics with the encoding, function,
//! offset and input.

use std::fmt;
use std::rc::Rc;

use expat::internal::chartab::{ByteType, Table};
use expat::internal::tok::{self as rs, Converter, Encoding, Normal, Scan, Tok, Unknown, Utf16};
use expat_oracle::tok::{self as c, Builtin, CONVERTER_TABLE_LEN, Enc, Scanner, UnknownEncoding};

/// A Rust encoding and its C counterpart.
pub trait Subject: fmt::Debug {
    /// The Rust encoding type.
    type E: Encoding;
    /// The Rust encoding.
    fn rs(&self) -> &Self::E;
    /// The C encoding.
    fn c(&self) -> Enc<'_>;
}

/// The C encoding of a Rust normal encoding.
pub fn builtin_of_normal(enc: Normal) -> Builtin {
    match enc {
        Normal::Utf8 => Builtin::Utf8,
        Normal::Utf8Ns => Builtin::Utf8Ns,
        Normal::InternalUtf8 => Builtin::InternalUtf8,
        Normal::InternalUtf8Ns => Builtin::InternalUtf8Ns,
        Normal::Latin1 => Builtin::Latin1,
        Normal::Latin1Ns => Builtin::Latin1Ns,
        Normal::Ascii => Builtin::Ascii,
        Normal::AsciiNs => Builtin::AsciiNs,
    }
}

/// The C encoding of a Rust UTF-16 encoding.
pub fn builtin_of_utf16(enc: Utf16) -> Builtin {
    match enc {
        Utf16::Little => Builtin::Little2,
        Utf16::LittleNs => Builtin::Little2Ns,
        Utf16::Big => Builtin::Big2,
        Utf16::BigNs => Builtin::Big2Ns,
    }
}

impl Subject for Normal {
    type E = Normal;
    fn rs(&self) -> &Normal {
        self
    }
    fn c(&self) -> Enc<'_> {
        Enc::Builtin(builtin_of_normal(*self))
    }
}

impl Subject for Utf16 {
    type E = Utf16;
    fn rs(&self) -> &Utf16 {
        self
    }
    fn c(&self) -> Enc<'_> {
        Enc::Builtin(builtin_of_utf16(*self))
    }
}

/// The Rust mirror of the shim's table converter (see
/// [`UnknownEncoding`]): `tab[(s[0] << 8) | (s[1] ^ ... ^ s[n-1])]`.
#[derive(Clone)]
pub struct TableConverter {
    tab: Rc<[i32]>,
}

impl Converter for TableConverter {
    fn convert(&self, seq: &[u8]) -> i32 {
        let (&b0, rest) = seq.split_first().expect("non-empty sequence");
        let x = rest.iter().fold(0u8, |a, &b| a ^ b);
        self.tab[(usize::from(b0) << 8) | usize::from(x)]
    }
}

/// An unknown encoding, built from the same map and converter table by the
/// port and by C.
pub struct UnknownPair {
    label: String,
    /// The port's encoding.
    pub rs: Unknown<TableConverter>,
    /// The C encoding.
    pub c: UnknownEncoding,
}

impl fmt::Debug for UnknownPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unknown({})", self.label)
    }
}

impl Subject for UnknownPair {
    type E = Unknown<TableConverter>;
    fn rs(&self) -> &Self::E {
        &self.rs
    }
    fn c(&self) -> Enc<'_> {
        Enc::Unknown(&self.c)
    }
}

impl UnknownPair {
    /// Builds the pair, checking that the port accepts the map exactly when
    /// C does. `None` when both reject it.
    pub fn new(label: &str, map: &[i32; 256], tab: Option<Vec<i32>>, ns: bool) -> Option<Self> {
        let tab: Option<Rc<[i32]>> = tab.map(Into::into);
        let conv = tab.clone().map(|tab| TableConverter { tab });
        let r = Unknown::new(map, conv, ns);
        let cenc = UnknownEncoding::new(map, tab.as_deref(), ns);
        assert_eq!(
            r.is_ok(),
            cenc.is_some(),
            "XmlInitUnknownEncoding acceptance of {label}: map {map:?}"
        );
        Some(UnknownPair {
            label: label.to_owned(),
            rs: r.ok()?,
            c: cenc?,
        })
    }

    /// Length of the longest prefix of `buf` that ends on a sequence
    /// boundary (where the C converters may be called).
    pub fn complete_prefix(&self, buf: &[u8]) -> usize {
        self.c.complete_prefix(buf)
    }
}

/// splitmix64, for reproducible maps and converter tables from a seed.
pub struct Rng(u64);

impl Rng {
    /// A generator seeded with `seed`.
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..n`.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

/// A code point that is interesting for the converter or the map: name
/// characters, other BMP characters, surrogates, noncharacters, values past
/// the BMP or past Unicode, and negative values.
fn interesting_code_point(r: &mut Rng) -> i32 {
    match r.below(12) {
        0 => -1,
        1 => 0xD800 + r.below(0x800) as i32,
        2 => 0xFFFE + r.below(2) as i32,
        3 => 0x1_0000 + r.below(0x10_0000) as i32,
        4 => 0x11_0000 + r.below(16) as i32,
        5 => r.below(0x80) as i32,
        6 => 0x4E00 + r.below(0x5000) as i32, // CJK: name characters
        7 => 0xC0 + r.below(0x140) as i32,    // Latin letters
        8 => -2 - r.below(8) as i32,
        _ => r.below(0x1_0000) as i32,
    }
}

/// A map for `XmlInitUnknownEncoding` from a seed: mostly valid (identity
/// for the ASCII characters XML syntax uses, lead bytes, Latin-1 and other
/// BMP characters, malformed and non-XML bytes), and sometimes invalid in
/// one of the ways C rejects.
pub fn random_map(r: &mut Rng) -> [i32; 256] {
    let mut map = [0i32; 256];
    // C requires these ASCII characters to map to themselves.
    let syntax = |i: usize| {
        let t = Table::Latin1.byte_type(u8::try_from(i).expect("byte"));
        t != ByteType::Other && t != ByteType::NonXml
    };
    let leads = r.below(4) != 0;
    for (i, m) in map.iter_mut().enumerate() {
        *m = i as i32;
        if i < 0x80 && syntax(i) {
            continue;
        }
        *m = match r.below(10) {
            0 | 1 if leads => -2 - r.below(3) as i32,
            2 => -1,
            3 => 0xD800 + r.below(0x800) as i32,
            4 | 5 => 0x80 + r.below(0xFF80) as i32,
            6 => {
                // Another ASCII character that XML syntax does not use.
                let c = r.below(0x80) as usize;
                if syntax(c) { i as i32 } else { c as i32 }
            }
            _ => i as i32,
        };
    }
    // Occasionally break one rule.
    match r.below(16) {
        0 => map[usize::from(b"<>&;=\"' \nA0:"[r.below(12) as usize])] = 0x20AC,
        1 => map[0x80 + r.below(0x80) as usize] = 0x1_0000 + r.below(0x1000) as i32,
        2 => map[0x80 + r.below(0x80) as usize] = -5 - r.below(4) as i32,
        _ => {}
    }
    map
}

/// A converter table from a seed: name characters, other BMP characters,
/// surrogates, noncharacters, values past the BMP or Unicode, negatives.
pub fn random_tab(r: &mut Rng) -> Vec<i32> {
    (0..CONVERTER_TABLE_LEN)
        .map(|_| interesting_code_point(r))
        .collect()
}

/// An unknown-encoding pair from a seed. `None` when the map is invalid
/// (which both sides then agree on).
pub fn unknown_from_seed(seed: u64) -> Option<UnknownPair> {
    let mut r = Rng::new(seed);
    let map = random_map(&mut r);
    let tab = (r.below(8) != 0).then(|| random_tab(&mut r));
    let ns = r.below(2) == 0;
    UnknownPair::new(&format!("seed {seed:#x}"), &map, tab, ns)
}

fn rust_scan<S: Subject>(s: &S, which: Scanner, buf: &[u8], ptr: usize) -> rs::Scanned {
    let enc = s.rs();
    match which {
        Scanner::Prolog => rs::prolog_tok(enc, buf, ptr),
        Scanner::Content => rs::content_tok(enc, buf, ptr),
        Scanner::CdataSection => rs::cdata_section_tok(enc, buf, ptr),
        Scanner::IgnoreSection => rs::ignore_section_tok(enc, buf, ptr),
        Scanner::AttributeValue => rs::attribute_value_tok(enc, buf, ptr),
        Scanner::EntityValue => rs::entity_value_tok(enc, buf, ptr),
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
pub fn check_scan<S: Subject>(s: &S, which: Scanner, buf: &[u8], ptr: usize) -> rs::Scanned {
    let r = rust_scan(s, which, buf, ptr);
    let (ct, cn) = c::scan(s.c(), which, buf, ptr);
    assert_eq!(
        (r.scan.to_c(), r.next),
        (ct, cn),
        "{s:?} {which:?} at {ptr} of {buf:?}"
    );
    if let (Scan::Token(tok), Some(next)) = (r.scan, r.next) {
        check_token_helpers(s, buf, ptr, tok, next);
    }
    r
}

/// Helpers whose precondition is "this is a token of type `tok` spanning
/// `buf[ptr..next]`".
fn check_token_helpers<S: Subject>(s: &S, buf: &[u8], ptr: usize, tok: Tok, next: usize) {
    let (enc, t) = (s.rs(), s.c());
    let m = S::E::MINBPC;
    match tok {
        Tok::StartTagWithAtts
        | Tok::EmptyElementWithAtts
        | Tok::StartTagNoAtts
        | Tok::EmptyElementNoAtts => {
            // `getAtts` on the tag, with too little and with enough room.
            for max in [0, 1, 2, 5] {
                let mut atts = vec![rs::Attribute::default(); max];
                let n = rs::get_atts(enc, buf, ptr, &mut atts);
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
                    "{s:?} getAtts({max}) at {ptr} of {buf:?}"
                );
            }
            // The element name follows `<`.
            let name = ptr + m;
            assert_eq!(
                rs::name_length(enc, buf, name),
                c::name_length(t, buf, name),
                "{s:?} nameLength at {name} of {buf:?}"
            );
        }
        Tok::CharRef => {
            assert_eq!(
                rs::char_ref_number(enc, buf, ptr),
                c::char_ref_number(t, buf, ptr),
                "{s:?} charRefNumber at {ptr} of {buf:?}"
            );
        }
        Tok::EntityRef | Tok::ParamEntityRef => {
            // The name is between `&`/`%` and `;`.
            let name = buf.get(ptr + m..next - m).expect("token inside buffer");
            assert_eq!(
                rs::predefined_entity_name(enc, name, 0).map_or(0, i32::from),
                c::predefined_entity_name(t, name, 0),
                "{s:?} predefinedEntityName {name:?}"
            );
            for a in ASCII_NAMES {
                assert_eq!(
                    rs::name_matches_ascii(enc, name, 0, a),
                    c::name_matches_ascii(t, name, 0, a),
                    "{s:?} nameMatchesAscii {name:?} {a:?}"
                );
            }
        }
        Tok::Literal => {
            let lit = buf.get(ptr..next).expect("token inside buffer");
            let r = rs::is_public_id(enc, lit, 0).map_err(Some);
            assert_eq!(r, c::is_public_id(t, lit, 0), "{s:?} isPublicId {lit:?}");
        }
        Tok::Name | Tok::PrefixedName | Tok::Nmtoken => {
            let name = buf.get(ptr..next).expect("token inside buffer");
            for a in ASCII_NAMES {
                assert_eq!(
                    rs::name_matches_ascii(enc, name, 0, a),
                    c::name_matches_ascii(t, name, 0, a),
                    "{s:?} nameMatchesAscii {name:?} {a:?}"
                );
            }
        }
        _ => {}
    }
}

/// Compares the helpers that are total in C at offset `ptr`.
///
/// For UTF-16 the buffer is cut to whole code units from `ptr`: the C
/// helpers without an end pointer would combine a dangling final byte with
/// whatever follows it, while the port treats it as the end (D-014).
pub fn check_total_helpers<S: Subject>(s: &S, buf: &[u8], ptr: usize) {
    let (enc, t) = (s.rs(), s.c());
    let m = S::E::MINBPC;
    let buf = &buf[..ptr + (buf.len() - ptr) / m * m];
    assert_eq!(
        rs::name_length(enc, buf, ptr),
        c::name_length(t, buf, ptr),
        "{s:?} nameLength at {ptr} of {buf:?}"
    );
    assert_eq!(
        rs::skip_s(enc, buf, ptr),
        c::skip_s(t, buf, ptr),
        "{s:?} skipS at {ptr} of {buf:?}"
    );
    // Start near the top of the range to compare the wrap-around, except
    // where C's `XML_Size` is 32 bits (Windows); see DECISIONS D-016.
    let column = if cfg!(windows) { 3 } else { u64::MAX - 2 };
    let mut pos = rs::Position {
        line_number: 7,
        column_number: column,
    };
    rs::update_position(enc, buf, ptr, &mut pos);
    assert_eq!(
        (pos.line_number, pos.column_number),
        c::update_position(t, buf, ptr, (7, column)),
        "{s:?} updatePosition at {ptr} of {buf:?}"
    );
    let r = rs::is_public_id(enc, buf, ptr).map_err(Some);
    assert_eq!(
        r,
        c::is_public_id(t, buf, ptr),
        "{s:?} isPublicId at {ptr} of {buf:?}"
    );
    let r = rs::predefined_entity_name(enc, buf, ptr).map_or(0, i32::from);
    assert_eq!(
        r,
        c::predefined_entity_name(t, buf, ptr),
        "{s:?} predefinedEntityName at {ptr}"
    );
}

/// Whether C's `char` is signed on this target. `utf8_toUtf16` copies a
/// byte that does not start a sequence through `char`, so for stray bytes
/// of 0x80 and above C's output depends on it; the port always
/// sign-extends, like C on every CI target (D-017).
const C_CHAR_SIGNED: bool = std::ffi::c_char::MIN != 0;

fn utf16_units_equal<S: Subject>(s: &S, r: &[u16], c: &[u16]) -> bool {
    let utf8 = matches!(
        s.c(),
        Enc::Builtin(
            Builtin::Utf8 | Builtin::Utf8Ns | Builtin::InternalUtf8 | Builtin::InternalUtf8Ns
        )
    );
    if C_CHAR_SIGNED || !utf8 {
        return r == c;
    }
    r.len() == c.len()
        && r.iter()
            .zip(c)
            .all(|(&r, &c)| r == c || (r >= 0xFF80 && c == r & 0xFF))
}

/// Output sizes to try for an input of `len` bytes: none, tiny, around
/// every split point of the first few characters, partial and ample.
/// `thorough == false` keeps a smaller set.
fn output_sizes(len: usize, thorough: bool) -> Vec<usize> {
    if thorough {
        (0..=9)
            .chain([len / 3, len / 2, len, 2 * len, 4 * len + 4])
            .collect()
    } else {
        vec![0, 1, 3, len / 2, 4 * len + 4]
    }
}

/// Compares `XmlUtf8Convert` and `XmlUtf16Convert` on `from` with several
/// output sizes, and (if `thorough`) on `from` with each of its last 4
/// bytes cut off. For an unknown encoding, the C side only sees inputs that
/// end on a sequence boundary (C reads past the end otherwise); on the rest
/// the port must stop at the incomplete sequence.
pub fn check_convert<S: Subject>(s: &S, from: &[u8], thorough: bool) {
    let enc = s.rs();
    let cuts = if thorough { from.len().min(4) } else { 0 };
    for cut in 0..=cuts {
        let from = &from[..from.len() - cut];
        let limit = match s.c() {
            Enc::Unknown(u) => u.complete_prefix(from),
            Enc::Builtin(_) => from.len(),
        };
        let complete = &from[..limit];
        for size in output_sizes(from.len(), thorough) {
            let mut out8 = vec![0u8; size];
            let r = enc.to_utf8(complete, &mut out8);
            let (cr, cout) = c::to_utf8(s.c(), complete, size);
            assert_eq!(
                (r.result as i32, r.read, r.written, &out8[..r.written]),
                (cr.result, cr.read, cr.written, &cout[..]),
                "{s:?} toUtf8 into {size} of {complete:?}"
            );
            let mut out16 = vec![0u16; size];
            let r = enc.to_utf16(complete, &mut out16);
            let (cr, cout) = c::to_utf16(s.c(), complete, size);
            assert!(
                (r.result as i32, r.read, r.written) == (cr.result, cr.read, cr.written)
                    && utf16_units_equal(s, &out16[..r.written], &cout),
                "{s:?} toUtf16 into {size} of {complete:?}: port {r:?} {:?}, C {cr:?} {cout:?}",
                &out16[..r.written]
            );
            if limit < from.len() {
                check_incomplete_unknown(s, from, limit, size);
            }
        }
    }
}

/// The port's own contract for an unknown encoding on input that ends inside
/// a sequence: the same as on the complete prefix, except that a call that
/// converts the whole prefix reports `InputIncomplete` (for UTF-16 output:
/// unless the output is full, which C's loop checks first).
fn check_incomplete_unknown<S: Subject>(s: &S, from: &[u8], limit: usize, size: usize) {
    let enc = s.rs();
    let mut a = vec![0u8; size];
    let mut b = vec![0u8; size];
    let full = enc.to_utf8(from, &mut a);
    let prefix = enc.to_utf8(&from[..limit], &mut b);
    let expected = if prefix.read == limit {
        rs::ConvertResult::InputIncomplete
    } else {
        prefix.result
    };
    assert_eq!(
        (full.result, full.read, full.written, &a[..full.written]),
        (expected, prefix.read, prefix.written, &b[..prefix.written]),
        "{s:?} toUtf8 incomplete into {size} of {from:?}"
    );
    let mut a = vec![0u16; size];
    let mut b = vec![0u16; size];
    let full = enc.to_utf16(from, &mut a);
    let prefix = enc.to_utf16(&from[..limit], &mut b);
    // As in C's loop, a full output is checked before the next character.
    let expected = if prefix.read < limit {
        prefix.result
    } else if prefix.written == size {
        rs::ConvertResult::OutputExhausted
    } else {
        rs::ConvertResult::InputIncomplete
    };
    assert_eq!(
        (full.result, full.read, full.written, &a[..full.written]),
        (expected, prefix.read, prefix.written, &b[..prefix.written]),
        "{s:?} toUtf16 incomplete into {size} of {from:?}"
    );
}

/// Compares `_INTERNAL_trim_to_complete_utf8_characters`.
pub fn check_trim(buf: &[u8]) {
    for end in 0..=buf.len() {
        let b = &buf[..end];
        assert_eq!(
            rs::trim_to_complete_utf8_characters(b),
            c::trim_utf8(b),
            "trim of {b:?}"
        );
    }
}

/// Runs [`check_scan`] for every scanner and [`check_total_helpers`] at each
/// offset in `offsets` (clamped to the buffer), for one encoding.
pub fn check_buffer<S: Subject>(s: &S, buf: &[u8], offsets: impl IntoIterator<Item = usize>) {
    for ptr in offsets {
        if ptr > buf.len() {
            break;
        }
        for which in Scanner::ALL {
            check_scan(s, which, buf, ptr);
        }
        check_total_helpers(s, buf, ptr);
    }
}

/// Tokenizes `buf` the way the parser walks it (from token to token, in the
/// content and the prolog state) and checks every step, including the
/// conversion of each accepted token. Exercises long realistic token
/// sequences rather than single offsets.
pub fn check_token_walk<S: Subject>(s: &S, buf: &[u8]) {
    for which in [Scanner::Prolog, Scanner::Content] {
        let mut ptr = 0;
        loop {
            let r = check_scan(s, which, buf, ptr);
            match (r.scan, r.next) {
                (Scan::Token(_), Some(next)) if next > ptr && next <= buf.len() => {
                    if which == Scanner::Content {
                        check_convert(s, &buf[ptr..next], false);
                    }
                    ptr = next;
                }
                // Resynchronise after an error or a partial token.
                _ if ptr < buf.len() => ptr += 1,
                _ => break,
            }
        }
    }
}
