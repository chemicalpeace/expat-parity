//! Encoding detection: `XmlInitEncoding(NS)`, `initScan` and the encoding
//! tables of `xmltok_ns.c`.
//!
//! Before the first token the parser does not know the encoding. It holds an
//! [`InitEncoding`] (C: `INIT_ENCODING`), which remembers the encoding named
//! by the application, if any. Its first scan ([`init_scan`]) looks at the
//! first bytes (byte order mark, `<` or `NUL` patterns), selects the real
//! encoding and scans the first token with it.
//!
//! ```
//! # #[cfg(feature = "internal-api")] {
//! use expat::internal::tok::{init_scan, InitEncoding, Known, Scan, State, Tok, Utf16};
//! let Some(init) = InitEncoding::new(None, false) else { return };
//! let r = init_scan(&init, State::Prolog, b"\xff\xfe<\0a\0/\0>\0", 0);
//! assert_eq!(r.scanned.scan, Scan::Token(Tok::Bom));
//! assert_eq!(r.encoding, Some(Known::Utf16(Utf16::Little)));
//! # }
//! ```

use super::scan::{Position, content_tok, prolog_tok, update_position};
use super::{Encoding, Normal, Scan, Scanned, Utf16};

/// `XML_PROLOG_STATE` / `XML_CONTENT_STATE`: which scanner the first token
/// is scanned with (content for an external parsed entity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    /// `XML_PROLOG_STATE`
    Prolog,
    /// `XML_CONTENT_STATE`
    Content,
}

/// The encoding indices of `xmltok.c` (`ISO_8859_1_ENC` ... `NO_ENC`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EncodingIndex {
    /// `ISO_8859_1_ENC`
    Iso8859_1,
    /// `US_ASCII_ENC`
    UsAscii,
    /// `UTF_8_ENC`
    Utf8,
    /// `UTF_16_ENC`
    Utf16,
    /// `UTF_16BE_ENC`
    Utf16Be,
    /// `UTF_16LE_ENC`
    Utf16Le,
    /// `NO_ENC`: no encoding was named.
    NoEnc,
}

impl EncodingIndex {
    /// The C value (`UNKNOWN_ENC` is -1 and has no variant).
    pub fn to_c(self) -> i32 {
        match self {
            EncodingIndex::Iso8859_1 => 0,
            EncodingIndex::UsAscii => 1,
            EncodingIndex::Utf8 => 2,
            EncodingIndex::Utf16 => 3,
            EncodingIndex::Utf16Be => 4,
            EncodingIndex::Utf16Le => 5,
            EncodingIndex::NoEnc => 6,
        }
    }
}

/// `streqci(s1, s2)` for an upper-case ASCII `s2`: equal ignoring the case
/// of ASCII letters in `s1`. Like the C string, `s1` ends at its first NUL.
fn streqci(s1: &[u8], s2: &[u8]) -> bool {
    let s1 = s1.split(|&b| b == 0).next().unwrap_or_default();
    s1.len() == s2.len() && s1.iter().zip(s2).all(|(&a, &b)| a.to_ascii_uppercase() == b)
}

/// `getEncodingIndex(name)`: `None` is `UNKNOWN_ENC`; no name is `NO_ENC`.
pub fn get_encoding_index(name: Option<&[u8]>) -> Option<EncodingIndex> {
    let Some(name) = name else {
        return Some(EncodingIndex::NoEnc);
    };
    [
        (&b"ISO-8859-1"[..], EncodingIndex::Iso8859_1),
        (b"US-ASCII", EncodingIndex::UsAscii),
        (b"UTF-8", EncodingIndex::Utf8),
        (b"UTF-16", EncodingIndex::Utf16),
        (b"UTF-16BE", EncodingIndex::Utf16Be),
        (b"UTF-16LE", EncodingIndex::Utf16Le),
    ]
    .into_iter()
    .find(|(kw, _)| streqci(name, kw))
    .map(|(_, i)| i)
}

/// A built-in encoding, as selected by detection or by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Known {
    /// A single-byte encoding.
    Normal(Normal),
    /// A UTF-16 encoding.
    Utf16(Utf16),
}

impl Known {
    /// `NS(encodings)[index]`: the encoding table of `xmltok_ns.c`. Plain
    /// `UTF-16` without a byte order mark is big-endian, and no name at all
    /// is UTF-8.
    pub fn from_index(index: EncodingIndex, ns: bool) -> Known {
        fn pick<T>(ns: bool, plain: T, with_ns: T) -> T {
            if ns { with_ns } else { plain }
        }
        match index {
            EncodingIndex::Iso8859_1 => Known::Normal(pick(ns, Normal::Latin1, Normal::Latin1Ns)),
            EncodingIndex::UsAscii => Known::Normal(pick(ns, Normal::Ascii, Normal::AsciiNs)),
            EncodingIndex::Utf8 | EncodingIndex::NoEnc => {
                Known::Normal(pick(ns, Normal::Utf8, Normal::Utf8Ns))
            }
            EncodingIndex::Utf16 | EncodingIndex::Utf16Be => {
                Known::Utf16(pick(ns, Utf16::Big, Utf16::BigNs))
            }
            EncodingIndex::Utf16Le => Known::Utf16(pick(ns, Utf16::Little, Utf16::LittleNs)),
        }
    }

    /// `XmlTok(enc, state, ...)`.
    pub fn tok(self, state: State, buf: &[u8], ptr: usize) -> Scanned {
        match self {
            Known::Normal(e) => tok(&e, state, buf, ptr),
            Known::Utf16(e) => tok(&e, state, buf, ptr),
        }
    }

    /// `minBytesPerChar`.
    pub fn min_bytes_per_char(self) -> usize {
        match self {
            Known::Normal(_) => Normal::MINBPC,
            Known::Utf16(_) => Utf16::MINBPC,
        }
    }
}

/// `XmlTok(enc, state, ptr, end, nextTokPtr)` for any encoding.
pub fn tok<E: Encoding>(enc: &E, state: State, buf: &[u8], ptr: usize) -> Scanned {
    match state {
        State::Prolog => prolog_tok(enc, buf, ptr),
        State::Content => content_tok(enc, buf, ptr),
    }
}

/// `XmlGetUtf8InternalEncoding(NS)`: the encoding of text the parser
/// generates itself.
pub fn internal_encoding(ns: bool) -> Normal {
    if ns {
        Normal::InternalUtf8Ns
    } else {
        Normal::InternalUtf8
    }
}

/// `INIT_ENCODING` after `XmlInitEncoding(NS)`: the encoding the application
/// named (its index) and whether namespace processing is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InitEncoding {
    index: EncodingIndex,
    ns: bool,
}

impl InitEncoding {
    /// `XmlInitEncoding` (`ns == false`) or `XmlInitEncodingNS`. `None` when
    /// the name is not one of the built-in encodings (C returns 0).
    pub fn new(name: Option<&[u8]>, ns: bool) -> Option<Self> {
        get_encoding_index(name).map(|index| InitEncoding { index, ns })
    }

    /// The index of the named encoding (`INIT_ENC_INDEX`).
    pub fn index(self) -> EncodingIndex {
        self.index
    }

    /// Whether this is the namespace-aware variant.
    pub fn ns(self) -> bool {
        self.ns
    }

    /// `initUpdatePosition`: positions before detection are counted as
    /// UTF-8.
    pub fn update_position(self, buf: &[u8], ptr: usize, pos: &mut Position) {
        update_position(&Normal::Utf8, buf, ptr, pos);
    }
}

/// Result of [`init_scan`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InitScan {
    /// The token, as the selected encoding's scanner returns it, or a BOM,
    /// or `None`/`Partial` when there are not enough bytes to decide.
    pub scanned: Scanned,
    /// The selected encoding (C: `*encPtr`). `None` when C leaves `*encPtr`
    /// at the initial encoding, i.e. detection needs more bytes.
    pub encoding: Option<Known>,
}

/// `initScan` (as `NS(initScanProlog)` / `NS(initScanContent)`): detects the
/// encoding from `buf[ptr..]` and scans the first token in `state`.
pub fn init_scan(init: &InitEncoding, state: State, buf: &[u8], ptr: usize) -> InitScan {
    use EncodingIndex as I;
    let undecided = |scan| InitScan {
        scanned: Scanned { scan, next: None },
        encoding: None,
    };
    let select = |index: EncodingIndex| {
        let e = Known::from_index(index, init.ns);
        InitScan {
            scanned: e.tok(state, buf, ptr),
            encoding: Some(e),
        }
    };
    let bom = |index: EncodingIndex, len: usize| InitScan {
        scanned: Scanned {
            scan: Scan::Token(super::Tok::Bom),
            next: Some(ptr.saturating_add(len)),
        },
        encoding: Some(Known::from_index(index, init.ns)),
    };
    let content = state == State::Content;
    let idx = init.index;
    let rest = buf.get(ptr..).unwrap_or_default();
    match *rest {
        [] => return undecided(Scan::None),
        [b0] => {
            // Only a single byte available for auto-detection (`XML_DTD` is
            // on, so a document entity is not rejected here). If UTF-16 was
            // externally specified, then we need at least 2 bytes.
            if matches!(idx, I::Utf16 | I::Utf16Le | I::Utf16Be) {
                return undecided(Scan::Partial);
            }
            match b0 {
                // Possibly the first byte of a byte order mark.
                0xFE | 0xFF | 0xEF if idx == I::Iso8859_1 && content => {}
                0xFE | 0xFF | 0xEF | 0x00 | 0x3C => return undecided(Scan::Partial),
                _ => {}
            }
        }
        // A failed guard is C's `break`: the arms after it cannot match the
        // same two bytes with a true guard, so control reaches the final
        // `select(idx)` exactly as in C.
        [b0, b1, ..] => match (b0, b1) {
            (0xFE, 0xFF) if !(idx == I::Iso8859_1 && content) => return bom(I::Utf16Be, 2),
            // 00 3C is handled with the other NUL patterns.
            (0x3C, 0x00) if !(matches!(idx, I::Utf16Be | I::Utf16) && content) => {
                return select(I::Utf16Le);
            }
            (0xFF, 0xFE) if !(idx == I::Iso8859_1 && content) => return bom(I::Utf16Le, 2),
            // Maybe a UTF-8 BOM (EF BB BF). In an external entity with an
            // explicit ISO-8859-1 or UTF-16 encoding it might be data.
            (0xEF, 0xBB)
                if !(content
                    && matches!(idx, I::Iso8859_1 | I::Utf16Be | I::Utf16Le | I::Utf16)) =>
            {
                match rest.get(2) {
                    None => return undecided(Scan::Partial),
                    Some(0xBF) => return bom(I::Utf8, 3),
                    Some(_) => {}
                }
            }
            // 0 isn't a legal data character, and a document entity can only
            // start with ASCII characters, so this is big-endian UTF-16
            // unless it is an external entity labelled UTF-16LE.
            (0x00, _) if !(content && idx == I::Utf16Le) => return select(I::Utf16Be),
            // Not recovered as UTF-16LE in an external entity: with a single
            // byte the parser could not tell whether it needs more.
            (_, 0x00) if !content => return select(I::Utf16Le),
            _ => {}
        },
    }
    select(idx)
}
