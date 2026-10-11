//! The scanner template: a function-by-function port of `xmltok_impl.c`
//! (`R_2_9_0`), generic over [`Encoding`].
//!
//! Each function names its C counterpart. The C file's macros map to:
//! `HAS_CHARS`/`REQUIRE_CHARS` → [`S::has`]/`req!`, `INVALID_CASES` →
//! [`S::invalid_cases`], `CHECK_NAME_CASES` → [`S::check_name`],
//! `CHECK_NMSTRT_CASES` → [`S::check_nmstrt`]. A C `switch` that lists one of
//! those macros next to explicit cases becomes a call to the helper (which
//! handles exactly the macro's byte types) followed by a `match` on the
//! remaining ones; the sets are disjoint, as they are in C.
//!
//! Positions are `usize` offsets. Advancing never overflows: every position
//! is at most `buf.len() + 4`, and a slice is never longer than
//! `isize::MAX`. A position may exceed `end` only after skipping a lead byte
//! in the functions that trust already-validated input (exactly where the C
//! pointer may pass `end`); every comparison against `end` is written so that
//! such a position counts as "no characters left", like the signed C
//! pointer difference.

// reason: position arithmetic is bounded as explained in the module docs
// (offsets never exceed `buf.len() + 4 <= isize::MAX`), and the counters
// `level` and the attribute count are bounded by the input length.
#![allow(clippy::arithmetic_side_effects)]

use super::{Encoding, Scan, Scanned, Tok};
use crate::chartab::{ByteType as Bt, Table};

const PARTIAL: Scanned = bare(Scan::Partial);

/// A result that leaves the C `*nextTokPtr` untouched.
const fn bare(scan: Scan) -> Scanned {
    Scanned { scan, next: None }
}

/// A result that sets the C `*nextTokPtr` to `next`.
const fn done(scan: Scan, next: usize) -> Scanned {
    Scanned {
        scan,
        next: Some(next),
    }
}

const fn tok(t: Tok, next: usize) -> Scanned {
    done(Scan::Token(t), next)
}

const fn invalid(at: usize) -> Scanned {
    done(Scan::Invalid, at)
}

/// Outcome of one of the C case macros.
enum Step {
    /// Continue scanning at this position (`ptr += n; break;`).
    Advance(usize),
    /// Return from the scanner.
    Return(Scanned),
}

/// Applies a [`Step`]: on `Advance` updates `$p` and yields `true`, on
/// `Return` returns from the enclosing function, and yields `false` when the
/// byte type is not one the helper handles.
macro_rules! step {
    ($e:expr, $p:ident) => {
        match $e {
            Some(Step::Advance(q)) => {
                $p = q;
                true
            }
            Some(Step::Return(r)) => return r,
            None => false,
        }
    };
}

/// `REQUIRE_CHARS(enc, ptr, end, count)`.
macro_rules! req {
    ($s:ident, $p:expr, $end:expr, $n:expr) => {
        if !$s.has($p, $end, $n) {
            return PARTIAL;
        }
    };
}

/// Length of the multi-byte sequence a lead byte type starts.
fn lead_len(bt: Bt) -> Option<usize> {
    match bt {
        Bt::Lead2 => Some(2),
        Bt::Lead3 => Some(3),
        Bt::Lead4 => Some(4),
        _ => None,
    }
}

/// The scanner state shared by all functions: encoding and buffer.
struct S<'a, E> {
    e: &'a E,
    b: &'a [u8],
}

impl<E: Encoding> S<'_, E> {
    const M: usize = E::MINBPC;

    /// `HAS_CHARS(enc, ptr, end, count)`.
    fn has(&self, p: usize, end: usize, count: usize) -> bool {
        end.saturating_sub(p) >= count * Self::M && p <= end
    }

    /// `end - ptr < n`, with a position past `end` counting as negative.
    fn short(&self, p: usize, end: usize, n: usize) -> bool {
        p > end || end - p < n
    }

    fn bt(&self, p: usize) -> Bt {
        self.e.byte_type(self.b, p)
    }

    fn cm(&self, p: usize, c: u8) -> bool {
        self.e.char_matches(self.b, p, c)
    }

    fn ascii(&self, p: usize) -> i32 {
        self.e.byte_to_ascii(self.b, p)
    }

    /// The `MINBPC(enc) > 1` prologue of the top-level scanners: drop a
    /// trailing incomplete code unit. `None` means "return
    /// `XML_TOK_PARTIAL`". Requires `p < end`.
    fn trunc(&self, p: usize, end: usize) -> Option<usize> {
        if Self::M > 1 {
            let n = end - p;
            if n & (Self::M - 1) != 0 {
                let n = n & !(Self::M - 1);
                if n == 0 {
                    return None;
                }
                return Some(p + n);
            }
        }
        Some(end)
    }

    /// `INVALID_CASES(ptr, nextTokPtr)`.
    fn invalid_cases(&self, bt: Bt, p: usize, end: usize) -> Option<Step> {
        if let Some(n) = lead_len(bt) {
            return Some(if self.short(p, end, n) {
                Step::Return(bare(Scan::PartialChar))
            } else if self.e.is_invalid_char(self.b, p, n) {
                Step::Return(invalid(p))
            } else {
                Step::Advance(p + n)
            });
        }
        match bt {
            Bt::NonXml | Bt::Malform | Bt::Trail => Some(Step::Return(invalid(p))),
            _ => None,
        }
    }

    /// `CHECK_NAME_CASES(enc, ptr, end, nextTokPtr)`.
    fn check_name(&self, bt: Bt, p: usize, end: usize) -> Option<Step> {
        if let Some(n) = lead_len(bt) {
            return Some(if self.short(p, end, n) {
                Step::Return(bare(Scan::PartialChar))
            } else if self.e.is_invalid_char(self.b, p, n) || !self.e.is_name_char(self.b, p, n) {
                Step::Return(invalid(p))
            } else {
                Step::Advance(p + n)
            });
        }
        match bt {
            Bt::NonAscii if !self.e.is_name_char_minbpc(self.b, p) => {
                Some(Step::Return(invalid(p)))
            }
            Bt::NonAscii | Bt::Nmstrt | Bt::Hex | Bt::Digit | Bt::Name | Bt::Minus => {
                Some(Step::Advance(p + Self::M))
            }
            _ => None,
        }
    }

    /// `CHECK_NMSTRT_CASES(enc, ptr, end, nextTokPtr)`.
    fn check_nmstrt(&self, bt: Bt, p: usize, end: usize) -> Option<Step> {
        if let Some(n) = lead_len(bt) {
            return Some(if self.short(p, end, n) {
                Step::Return(bare(Scan::PartialChar))
            } else if self.e.is_invalid_char(self.b, p, n) || !self.e.is_nmstrt_char(self.b, p, n)
            {
                Step::Return(invalid(p))
            } else {
                Step::Advance(p + n)
            });
        }
        match bt {
            Bt::NonAscii if !self.e.is_nmstrt_char_minbpc(self.b, p) => {
                Some(Step::Return(invalid(p)))
            }
            Bt::NonAscii | Bt::Nmstrt | Bt::Hex => Some(Step::Advance(p + Self::M)),
            _ => None,
        }
    }

    /// `scanComment`: `p` follows `<!-`.
    fn scan_comment(&self, mut p: usize, end: usize) -> Scanned {
        if self.has(p, end, 1) {
            if !self.cm(p, b'-') {
                return invalid(p);
            }
            p += Self::M;
            while self.has(p, end, 1) {
                let bt = self.bt(p);
                if step!(self.invalid_cases(bt, p, end), p) {
                    continue;
                }
                if bt == Bt::Minus {
                    p += Self::M;
                    req!(self, p, end, 1);
                    if self.cm(p, b'-') {
                        p += Self::M;
                        req!(self, p, end, 1);
                        if !self.cm(p, b'>') {
                            return invalid(p);
                        }
                        return tok(Tok::Comment, p + Self::M);
                    }
                } else {
                    p += Self::M;
                }
            }
        }
        PARTIAL
    }

    /// `scanDecl`: `p` follows `<!`.
    fn scan_decl(&self, mut p: usize, end: usize) -> Scanned {
        req!(self, p, end, 1);
        match self.bt(p) {
            Bt::Minus => return self.scan_comment(p + Self::M, end),
            Bt::Lsqb => return tok(Tok::CondSectOpen, p + Self::M),
            Bt::Nmstrt | Bt::Hex => p += Self::M,
            _ => return invalid(p),
        }
        while self.has(p, end, 1) {
            match self.bt(p) {
                Bt::Percnt => {
                    req!(self, p, end, 2);
                    // don't allow <!ENTITY% foo "whatever">
                    if let Bt::S | Bt::Cr | Bt::Lf | Bt::Percnt = self.bt(p + Self::M) {
                        return invalid(p);
                    }
                    return tok(Tok::DeclOpen, p);
                }
                Bt::S | Bt::Cr | Bt::Lf => return tok(Tok::DeclOpen, p),
                Bt::Nmstrt | Bt::Hex => p += Self::M,
                _ => return invalid(p),
            }
        }
        PARTIAL
    }

    /// `checkPiTarget`: `None` for a reserved target (`xml` in another case),
    /// otherwise the token (`XML_TOK_XML_DECL` for `xml`, else `XML_TOK_PI`).
    fn check_pi_target(&self, mut p: usize, end: usize) -> Option<Tok> {
        if end - p != Self::M * 3 {
            return Some(Tok::Pi);
        }
        let mut upper = false;
        for (lower, capital) in [(b'x', b'X'), (b'm', b'M'), (b'l', b'L')] {
            let c = self.ascii(p);
            if c == i32::from(capital) {
                upper = true;
            } else if c != i32::from(lower) {
                return Some(Tok::Pi);
            }
            p += Self::M;
        }
        if upper { None } else { Some(Tok::XmlDecl) }
    }

    /// `scanPi`: `p` follows `<?`.
    fn scan_pi(&self, mut p: usize, end: usize) -> Scanned {
        let target = p;
        req!(self, p, end, 1);
        if !step!(self.check_nmstrt(self.bt(p), p, end), p) {
            return invalid(p);
        }
        while self.has(p, end, 1) {
            let bt = self.bt(p);
            if step!(self.check_name(bt, p, end), p) {
                continue;
            }
            match bt {
                Bt::S | Bt::Cr | Bt::Lf => {
                    let Some(t) = self.check_pi_target(target, p) else {
                        return invalid(p);
                    };
                    p += Self::M;
                    while self.has(p, end, 1) {
                        let bt = self.bt(p);
                        if step!(self.invalid_cases(bt, p, end), p) {
                            continue;
                        }
                        p += Self::M;
                        if bt == Bt::Quest {
                            req!(self, p, end, 1);
                            if self.cm(p, b'>') {
                                return tok(t, p + Self::M);
                            }
                        }
                    }
                    return PARTIAL;
                }
                Bt::Quest => {
                    let Some(t) = self.check_pi_target(target, p) else {
                        return invalid(p);
                    };
                    p += Self::M;
                    req!(self, p, end, 1);
                    if self.cm(p, b'>') {
                        return tok(t, p + Self::M);
                    }
                    return invalid(p);
                }
                _ => return invalid(p),
            }
        }
        PARTIAL
    }

    /// `scanCdataSection`: `p` follows `<![`.
    fn scan_cdata_section(&self, mut p: usize, end: usize) -> Scanned {
        req!(self, p, end, 6);
        for c in *b"CDATA[" {
            if !self.cm(p, c) {
                return invalid(p);
            }
            p += Self::M;
        }
        tok(Tok::CdataSectOpen, p)
    }

    /// `cdataSectionTok`.
    fn cdata_section_tok(&self, mut p: usize, end: usize) -> Scanned {
        if p >= end {
            return bare(Scan::None);
        }
        let Some(end) = self.trunc(p, end) else {
            return PARTIAL;
        };
        let bt = self.bt(p);
        if !step!(self.invalid_cases(bt, p, end), p) {
            match bt {
                Bt::Rsqb => {
                    p += Self::M;
                    req!(self, p, end, 1);
                    if self.cm(p, b']') {
                        p += Self::M;
                        req!(self, p, end, 1);
                        if self.cm(p, b'>') {
                            return tok(Tok::CdataSectClose, p + Self::M);
                        }
                        p -= Self::M;
                    }
                }
                Bt::Cr => {
                    p += Self::M;
                    req!(self, p, end, 1);
                    if self.bt(p) == Bt::Lf {
                        p += Self::M;
                    }
                    return tok(Tok::DataNewline, p);
                }
                Bt::Lf => return tok(Tok::DataNewline, p + Self::M),
                _ => p += Self::M,
            }
        }
        while self.has(p, end, 1) {
            let bt = self.bt(p);
            if let Some(n) = lead_len(bt) {
                if self.short(p, end, n) || self.e.is_invalid_char(self.b, p, n) {
                    return tok(Tok::DataChars, p);
                }
                p += n;
                continue;
            }
            match bt {
                Bt::NonXml | Bt::Malform | Bt::Trail | Bt::Cr | Bt::Lf | Bt::Rsqb => {
                    return tok(Tok::DataChars, p);
                }
                _ => p += Self::M,
            }
        }
        tok(Tok::DataChars, p)
    }

    /// `scanEndTag`: `p` follows `</`.
    fn scan_end_tag(&self, mut p: usize, end: usize) -> Scanned {
        req!(self, p, end, 1);
        if !step!(self.check_nmstrt(self.bt(p), p, end), p) {
            return invalid(p);
        }
        while self.has(p, end, 1) {
            let bt = self.bt(p);
            if step!(self.check_name(bt, p, end), p) {
                continue;
            }
            match bt {
                Bt::S | Bt::Cr | Bt::Lf => {
                    p += Self::M;
                    while self.has(p, end, 1) {
                        match self.bt(p) {
                            Bt::S | Bt::Cr | Bt::Lf => {}
                            Bt::Gt => return tok(Tok::EndTag, p + Self::M),
                            _ => return invalid(p),
                        }
                        p += Self::M;
                    }
                    return PARTIAL;
                }
                // No need to check qname syntax here, since the end tag must
                // match the start tag exactly.
                Bt::Colon => p += Self::M,
                Bt::Gt => return tok(Tok::EndTag, p + Self::M),
                _ => return invalid(p),
            }
        }
        PARTIAL
    }

    /// `scanHexCharRef`: `p` follows `&#x`.
    fn scan_hex_char_ref(&self, mut p: usize, end: usize) -> Scanned {
        if self.has(p, end, 1) {
            if !matches!(self.bt(p), Bt::Digit | Bt::Hex) {
                return invalid(p);
            }
            p += Self::M;
            while self.has(p, end, 1) {
                match self.bt(p) {
                    Bt::Digit | Bt::Hex => {}
                    Bt::Semi => return tok(Tok::CharRef, p + Self::M),
                    _ => return invalid(p),
                }
                p += Self::M;
            }
        }
        PARTIAL
    }

    /// `scanCharRef`: `p` follows `&#`.
    fn scan_char_ref(&self, mut p: usize, end: usize) -> Scanned {
        if self.has(p, end, 1) {
            if self.cm(p, b'x') {
                return self.scan_hex_char_ref(p + Self::M, end);
            }
            if self.bt(p) != Bt::Digit {
                return invalid(p);
            }
            p += Self::M;
            while self.has(p, end, 1) {
                match self.bt(p) {
                    Bt::Digit => {}
                    Bt::Semi => return tok(Tok::CharRef, p + Self::M),
                    _ => return invalid(p),
                }
                p += Self::M;
            }
        }
        PARTIAL
    }

    /// `scanRef`: `p` follows `&`.
    fn scan_ref(&self, mut p: usize, end: usize) -> Scanned {
        req!(self, p, end, 1);
        let bt = self.bt(p);
        if !step!(self.check_nmstrt(bt, p, end), p) {
            if bt == Bt::Num {
                return self.scan_char_ref(p + Self::M, end);
            }
            return invalid(p);
        }
        while self.has(p, end, 1) {
            let bt = self.bt(p);
            if step!(self.check_name(bt, p, end), p) {
                continue;
            }
            if bt == Bt::Semi {
                return tok(Tok::EntityRef, p + Self::M);
            }
            return invalid(p);
        }
        PARTIAL
    }

    /// The `sol:` label of `scanAtts`/`scanLt`: `p` is at `/`.
    fn empty_element_close(&self, mut p: usize, end: usize, t: Tok) -> Scanned {
        p += Self::M;
        req!(self, p, end, 1);
        if !self.cm(p, b'>') {
            return invalid(p);
        }
        tok(t, p + Self::M)
    }

    /// `scanAtts`: `p` follows the first character of an attribute name.
    fn scan_atts(&self, mut p: usize, end: usize) -> Scanned {
        let mut had_colon = false;
        while self.has(p, end, 1) {
            let bt = self.bt(p);
            if step!(self.check_name(bt, p, end), p) {
                continue;
            }
            match bt {
                Bt::Colon => {
                    if had_colon {
                        return invalid(p);
                    }
                    had_colon = true;
                    p += Self::M;
                    req!(self, p, end, 1);
                    if !step!(self.check_nmstrt(self.bt(p), p, end), p) {
                        return invalid(p);
                    }
                }
                Bt::S | Bt::Cr | Bt::Lf | Bt::Equals => {
                    if bt != Bt::Equals {
                        loop {
                            p += Self::M;
                            req!(self, p, end, 1);
                            match self.bt(p) {
                                Bt::Equals => break,
                                Bt::S | Bt::Lf | Bt::Cr => {}
                                _ => return invalid(p),
                            }
                        }
                    }
                    // case BT_EQUALS:
                    had_colon = false;
                    let open = loop {
                        p += Self::M;
                        req!(self, p, end, 1);
                        match self.bt(p) {
                            o @ (Bt::Quot | Bt::Apos) => break o,
                            Bt::S | Bt::Lf | Bt::Cr => {}
                            _ => return invalid(p),
                        }
                    };
                    p += Self::M;
                    // in attribute value
                    loop {
                        req!(self, p, end, 1);
                        let t = self.bt(p);
                        if t == open {
                            break;
                        }
                        if step!(self.invalid_cases(t, p, end), p) {
                            continue;
                        }
                        match t {
                            Bt::Amp => {
                                let r = self.scan_ref(p + Self::M, end);
                                match r.scan {
                                    // scanRef always sets the position of a
                                    // token, so the fallback is never used.
                                    Scan::Token(_) => p = r.next.unwrap_or(p + Self::M),
                                    Scan::Invalid => return invalid(r.next.unwrap_or(p)),
                                    other => return bare(other),
                                }
                            }
                            Bt::Lt => return invalid(p),
                            _ => p += Self::M,
                        }
                    }
                    p += Self::M;
                    req!(self, p, end, 1);
                    match self.bt(p) {
                        Bt::S | Bt::Cr | Bt::Lf => {}
                        Bt::Sol => return self.empty_element_close(p, end, Tok::EmptyElementWithAtts),
                        Bt::Gt => return tok(Tok::StartTagWithAtts, p + Self::M),
                        _ => return invalid(p),
                    }
                    // p points to the white space after the closing quote
                    loop {
                        p += Self::M;
                        req!(self, p, end, 1);
                        let bt = self.bt(p);
                        if step!(self.check_nmstrt(bt, p, end), p) {
                            break;
                        }
                        match bt {
                            Bt::S | Bt::Cr | Bt::Lf => {}
                            Bt::Gt => return tok(Tok::StartTagWithAtts, p + Self::M),
                            Bt::Sol => {
                                return self.empty_element_close(p, end, Tok::EmptyElementWithAtts);
                            }
                            _ => return invalid(p),
                        }
                    }
                }
                _ => return invalid(p),
            }
        }
        PARTIAL
    }

    /// `scanLt`: `p` follows `<`.
    fn scan_lt(&self, mut p: usize, end: usize) -> Scanned {
        req!(self, p, end, 1);
        let bt = self.bt(p);
        if !step!(self.check_nmstrt(bt, p, end), p) {
            return match bt {
                Bt::Excl => {
                    p += Self::M;
                    req!(self, p, end, 1);
                    match self.bt(p) {
                        Bt::Minus => self.scan_comment(p + Self::M, end),
                        Bt::Lsqb => self.scan_cdata_section(p + Self::M, end),
                        _ => invalid(p),
                    }
                }
                Bt::Quest => self.scan_pi(p + Self::M, end),
                Bt::Sol => self.scan_end_tag(p + Self::M, end),
                _ => invalid(p),
            };
        }
        let mut had_colon = false;
        // we have a start-tag
        while self.has(p, end, 1) {
            let bt = self.bt(p);
            if step!(self.check_name(bt, p, end), p) {
                continue;
            }
            match bt {
                Bt::Colon => {
                    if had_colon {
                        return invalid(p);
                    }
                    had_colon = true;
                    p += Self::M;
                    req!(self, p, end, 1);
                    if !step!(self.check_nmstrt(self.bt(p), p, end), p) {
                        return invalid(p);
                    }
                }
                Bt::S | Bt::Cr | Bt::Lf => {
                    p += Self::M;
                    while self.has(p, end, 1) {
                        let bt = self.bt(p);
                        if step!(self.check_nmstrt(bt, p, end), p) {
                            return self.scan_atts(p, end);
                        }
                        match bt {
                            Bt::Gt => return tok(Tok::StartTagNoAtts, p + Self::M),
                            Bt::Sol => {
                                return self.empty_element_close(p, end, Tok::EmptyElementNoAtts);
                            }
                            Bt::S | Bt::Cr | Bt::Lf => p += Self::M,
                            _ => return invalid(p),
                        }
                    }
                    return PARTIAL;
                }
                Bt::Gt => return tok(Tok::StartTagNoAtts, p + Self::M),
                Bt::Sol => return self.empty_element_close(p, end, Tok::EmptyElementNoAtts),
                _ => return invalid(p),
            }
        }
        PARTIAL
    }

    /// `contentTok`.
    fn content_tok(&self, mut p: usize, end: usize) -> Scanned {
        if p >= end {
            return bare(Scan::None);
        }
        let Some(end) = self.trunc(p, end) else {
            return PARTIAL;
        };
        let bt = self.bt(p);
        if !step!(self.invalid_cases(bt, p, end), p) {
            match bt {
                Bt::Lt => return self.scan_lt(p + Self::M, end),
                Bt::Amp => return self.scan_ref(p + Self::M, end),
                Bt::Cr => {
                    p += Self::M;
                    if !self.has(p, end, 1) {
                        return bare(Scan::TrailingCr);
                    }
                    if self.bt(p) == Bt::Lf {
                        p += Self::M;
                    }
                    return tok(Tok::DataNewline, p);
                }
                Bt::Lf => return tok(Tok::DataNewline, p + Self::M),
                Bt::Rsqb => {
                    p += Self::M;
                    if !self.has(p, end, 1) {
                        return bare(Scan::TrailingRsqb);
                    }
                    if self.cm(p, b']') {
                        p += Self::M;
                        if !self.has(p, end, 1) {
                            return bare(Scan::TrailingRsqb);
                        }
                        if self.cm(p, b'>') {
                            return invalid(p);
                        }
                        p -= Self::M;
                    }
                }
                _ => p += Self::M,
            }
        }
        while self.has(p, end, 1) {
            let bt = self.bt(p);
            if let Some(n) = lead_len(bt) {
                if self.short(p, end, n) || self.e.is_invalid_char(self.b, p, n) {
                    return tok(Tok::DataChars, p);
                }
                p += n;
                continue;
            }
            match bt {
                Bt::Rsqb => {
                    if self.has(p, end, 2) {
                        if !self.cm(p + Self::M, b']') {
                            p += Self::M;
                            continue;
                        }
                        if self.has(p, end, 3) {
                            if !self.cm(p + 2 * Self::M, b'>') {
                                p += Self::M;
                                continue;
                            }
                            return invalid(p + 2 * Self::M);
                        }
                    }
                    return tok(Tok::DataChars, p);
                }
                Bt::Amp
                | Bt::Lt
                | Bt::NonXml
                | Bt::Malform
                | Bt::Trail
                | Bt::Cr
                | Bt::Lf => return tok(Tok::DataChars, p),
                _ => p += Self::M,
            }
        }
        tok(Tok::DataChars, p)
    }

    /// `scanPercent`: `p` follows `%`.
    fn scan_percent(&self, mut p: usize, end: usize) -> Scanned {
        req!(self, p, end, 1);
        let bt = self.bt(p);
        if !step!(self.check_nmstrt(bt, p, end), p) {
            return match bt {
                Bt::S | Bt::Lf | Bt::Cr | Bt::Percnt => done(Scan::Token(Tok::Percent), p),
                _ => invalid(p),
            };
        }
        while self.has(p, end, 1) {
            let bt = self.bt(p);
            if step!(self.check_name(bt, p, end), p) {
                continue;
            }
            if bt == Bt::Semi {
                return tok(Tok::ParamEntityRef, p + Self::M);
            }
            return invalid(p);
        }
        PARTIAL
    }

    /// `scanPoundName`: `p` follows `#`.
    fn scan_pound_name(&self, mut p: usize, end: usize) -> Scanned {
        req!(self, p, end, 1);
        if !step!(self.check_nmstrt(self.bt(p), p, end), p) {
            return invalid(p);
        }
        while self.has(p, end, 1) {
            let bt = self.bt(p);
            if step!(self.check_name(bt, p, end), p) {
                continue;
            }
            return match bt {
                Bt::Cr | Bt::Lf | Bt::S | Bt::Rpar | Bt::Gt | Bt::Percnt | Bt::Verbar => {
                    tok(Tok::PoundName, p)
                }
                _ => invalid(p),
            };
        }
        bare(Scan::PartialToken(Tok::PoundName))
    }

    /// `scanLit`: `p` follows the opening quote, whose type is `open`.
    fn scan_lit(&self, open: Bt, mut p: usize, end: usize) -> Scanned {
        while self.has(p, end, 1) {
            let t = self.bt(p);
            if step!(self.invalid_cases(t, p, end), p) {
                continue;
            }
            p += Self::M;
            if matches!(t, Bt::Quot | Bt::Apos) && t == open {
                if !self.has(p, end, 1) {
                    return bare(Scan::PartialToken(Tok::Literal));
                }
                return match self.bt(p) {
                    Bt::S | Bt::Cr | Bt::Lf | Bt::Gt | Bt::Percnt | Bt::Lsqb => {
                        tok(Tok::Literal, p)
                    }
                    _ => invalid(p),
                };
            }
        }
        PARTIAL
    }

    /// `prologTok`.
    fn prolog_tok(&self, mut p: usize, end: usize) -> Scanned {
        if p >= end {
            return bare(Scan::None);
        }
        let Some(end) = self.trunc(p, end) else {
            return PARTIAL;
        };
        let bt = self.bt(p);
        let mut t = match bt {
            Bt::Quot | Bt::Apos => return self.scan_lit(bt, p + Self::M, end),
            Bt::Lt => {
                p += Self::M;
                req!(self, p, end, 1);
                return match self.bt(p) {
                    Bt::Excl => self.scan_decl(p + Self::M, end),
                    Bt::Quest => self.scan_pi(p + Self::M, end),
                    Bt::Nmstrt
                    | Bt::Hex
                    | Bt::NonAscii
                    | Bt::Lead2
                    | Bt::Lead3
                    | Bt::Lead4 => tok(Tok::InstanceStart, p - Self::M),
                    _ => invalid(p),
                };
            }
            Bt::Cr | Bt::S | Bt::Lf => {
                if bt == Bt::Cr && p + Self::M == end {
                    // indicate that this might be part of a CR/LF pair
                    return done(Scan::PartialToken(Tok::PrologS), end);
                }
                loop {
                    p += Self::M;
                    if !self.has(p, end, 1) {
                        break;
                    }
                    match self.bt(p) {
                        Bt::S | Bt::Lf => {}
                        // don't split CR/LF pair
                        Bt::Cr if p + Self::M != end => {}
                        _ => return tok(Tok::PrologS, p),
                    }
                }
                return tok(Tok::PrologS, p);
            }
            Bt::Percnt => return self.scan_percent(p + Self::M, end),
            Bt::Comma => return tok(Tok::Comma, p + Self::M),
            Bt::Lsqb => return tok(Tok::OpenBracket, p + Self::M),
            Bt::Rsqb => {
                p += Self::M;
                if !self.has(p, end, 1) {
                    return bare(Scan::PartialToken(Tok::CloseBracket));
                }
                if self.cm(p, b']') {
                    req!(self, p, end, 2);
                    if self.cm(p + Self::M, b'>') {
                        return tok(Tok::CondSectClose, p + 2 * Self::M);
                    }
                }
                return tok(Tok::CloseBracket, p);
            }
            Bt::Lpar => return tok(Tok::OpenParen, p + Self::M),
            Bt::Rpar => {
                p += Self::M;
                if !self.has(p, end, 1) {
                    return bare(Scan::PartialToken(Tok::CloseParen));
                }
                return match self.bt(p) {
                    Bt::Ast => tok(Tok::CloseParenAsterisk, p + Self::M),
                    Bt::Quest => tok(Tok::CloseParenQuestion, p + Self::M),
                    Bt::Plus => tok(Tok::CloseParenPlus, p + Self::M),
                    Bt::Cr
                    | Bt::Lf
                    | Bt::S
                    | Bt::Gt
                    | Bt::Comma
                    | Bt::Verbar
                    | Bt::Rpar => tok(Tok::CloseParen, p),
                    _ => invalid(p),
                };
            }
            Bt::Verbar => return tok(Tok::Or, p + Self::M),
            Bt::Gt => return tok(Tok::DeclClose, p + Self::M),
            Bt::Num => return self.scan_pound_name(p + Self::M, end),
            Bt::Lead2 | Bt::Lead3 | Bt::Lead4 => {
                let n = lead_len(bt).unwrap_or(Self::M);
                if self.short(p, end, n) {
                    return bare(Scan::PartialChar);
                }
                if self.e.is_invalid_char(self.b, p, n) {
                    return invalid(p);
                }
                let t = if self.e.is_nmstrt_char(self.b, p, n) {
                    Tok::Name
                } else if self.e.is_name_char(self.b, p, n) {
                    Tok::Nmtoken
                } else {
                    return invalid(p);
                };
                p += n;
                t
            }
            Bt::Nmstrt | Bt::Hex => {
                p += Self::M;
                Tok::Name
            }
            Bt::Digit | Bt::Name | Bt::Minus | Bt::Colon => {
                p += Self::M;
                Tok::Nmtoken
            }
            Bt::NonAscii if self.e.is_nmstrt_char_minbpc(self.b, p) => {
                p += Self::M;
                Tok::Name
            }
            Bt::NonAscii if self.e.is_name_char_minbpc(self.b, p) => {
                p += Self::M;
                Tok::Nmtoken
            }
            _ => return invalid(p),
        };
        while self.has(p, end, 1) {
            let bt = self.bt(p);
            if step!(self.check_name(bt, p, end), p) {
                continue;
            }
            match bt {
                Bt::Gt
                | Bt::Rpar
                | Bt::Comma
                | Bt::Verbar
                | Bt::Lsqb
                | Bt::Percnt
                | Bt::S
                | Bt::Cr
                | Bt::Lf => return tok(t, p),
                Bt::Colon => {
                    p += Self::M;
                    match t {
                        Tok::Name => {
                            req!(self, p, end, 1);
                            t = Tok::PrefixedName;
                            if !step!(self.check_name(self.bt(p), p, end), p) {
                                t = Tok::Nmtoken;
                            }
                        }
                        Tok::PrefixedName => t = Tok::Nmtoken,
                        _ => {}
                    }
                }
                Bt::Plus | Bt::Ast | Bt::Quest => {
                    if t == Tok::Nmtoken {
                        return invalid(p);
                    }
                    let suffixed = match bt {
                        Bt::Plus => Tok::NamePlus,
                        Bt::Ast => Tok::NameAsterisk,
                        _ => Tok::NameQuestion,
                    };
                    return tok(suffixed, p + Self::M);
                }
                _ => return invalid(p),
            }
        }
        bare(Scan::PartialToken(t))
    }

    /// The common body of `attributeValueTok` (`entity == false`) and
    /// `entityValueTok` (`entity == true`).
    fn literal_value_tok(&self, mut p: usize, end: usize, entity: bool) -> Scanned {
        if p >= end {
            return bare(Scan::None);
        }
        if !self.has(p, end, 1) {
            // Unreachable from the parser (the data has already been
            // tokenized once); kept like the C paranoia check.
            return PARTIAL;
        }
        let start = p;
        while self.has(p, end, 1) {
            let bt = self.bt(p);
            if let Some(n) = lead_len(bt) {
                // The encoding has already been validated.
                p += n;
                continue;
            }
            match bt {
                Bt::Amp => {
                    if p == start {
                        return self.scan_ref(p + Self::M, end);
                    }
                    return tok(Tok::DataChars, p);
                }
                Bt::Percnt if entity => {
                    if p == start {
                        let r = self.scan_percent(p + Self::M, end);
                        return if r.scan == Scan::Token(Tok::Percent) {
                            Scanned {
                                scan: Scan::Invalid,
                                ..r
                            }
                        } else {
                            r
                        };
                    }
                    return tok(Tok::DataChars, p);
                }
                // this is for inside entity references
                Bt::Lt if !entity => return invalid(p),
                Bt::Lf => {
                    if p == start {
                        return tok(Tok::DataNewline, p + Self::M);
                    }
                    return tok(Tok::DataChars, p);
                }
                Bt::Cr => {
                    if p == start {
                        p += Self::M;
                        if !self.has(p, end, 1) {
                            return bare(Scan::TrailingCr);
                        }
                        if self.bt(p) == Bt::Lf {
                            p += Self::M;
                        }
                        return tok(Tok::DataNewline, p);
                    }
                    return tok(Tok::DataChars, p);
                }
                Bt::S if !entity => {
                    if p == start {
                        return tok(Tok::AttributeValueS, p + Self::M);
                    }
                    return tok(Tok::DataChars, p);
                }
                _ => p += Self::M,
            }
        }
        tok(Tok::DataChars, p)
    }

    /// `ignoreSectionTok`.
    fn ignore_section_tok(&self, mut p: usize, mut end: usize) -> Scanned {
        let mut level: usize = 0;
        if Self::M > 1 {
            let n = end.saturating_sub(p);
            if n & (Self::M - 1) != 0 {
                end = p + (n & !(Self::M - 1));
            }
        }
        while self.has(p, end, 1) {
            let bt = self.bt(p);
            if step!(self.invalid_cases(bt, p, end), p) {
                continue;
            }
            match bt {
                Bt::Lt => {
                    p += Self::M;
                    req!(self, p, end, 1);
                    if self.cm(p, b'!') {
                        p += Self::M;
                        req!(self, p, end, 1);
                        if self.cm(p, b'[') {
                            level += 1;
                            p += Self::M;
                        }
                    }
                }
                Bt::Rsqb => {
                    p += Self::M;
                    req!(self, p, end, 1);
                    if self.cm(p, b']') {
                        p += Self::M;
                        req!(self, p, end, 1);
                        if self.cm(p, b'>') {
                            p += Self::M;
                            if level == 0 {
                                return tok(Tok::IgnoreSect, p);
                            }
                            level -= 1;
                        }
                    }
                }
                _ => p += Self::M,
            }
        }
        PARTIAL
    }

    /// `isPublicId`: `Err(bad)` is the C `0` return with `*badPtr = bad`.
    fn is_public_id(&self, mut p: usize, end: usize) -> Result<(), usize> {
        p += Self::M;
        let end = end.saturating_sub(Self::M);
        while self.has(p, end, 1) {
            match self.bt(p) {
                Bt::Digit
                | Bt::Hex
                | Bt::Minus
                | Bt::Apos
                | Bt::Lpar
                | Bt::Rpar
                | Bt::Plus
                | Bt::Comma
                | Bt::Sol
                | Bt::Equals
                | Bt::Quest
                | Bt::Cr
                | Bt::Lf
                | Bt::Semi
                | Bt::Excl
                | Bt::Ast
                | Bt::Percnt
                | Bt::Num
                | Bt::Colon => {}
                Bt::S => {
                    if self.cm(p, b'\t') {
                        return Err(p);
                    }
                }
                Bt::Name | Bt::Nmstrt if self.ascii(p) & !0x7f == 0 => {}
                _ => {
                    if !matches!(self.ascii(p), 0x24 | 0x40) {
                        return Err(p);
                    }
                }
            }
            p += Self::M;
        }
        Ok(())
    }

    /// `getAtts`, additionally stopping at the end of the buffer.
    fn get_atts(&self, ptr: usize, atts: &mut [Attribute]) -> usize {
        #[derive(PartialEq, Eq)]
        enum State {
            Other,
            InName,
            InValue,
        }
        let end = self.b.len();
        let mut state = State::InName;
        let mut n_atts: usize = 0;
        // Only read in state `InValue`, where it has been set; C uses 0,
        // which is BT_NONXML.
        let mut open = Bt::NonXml;
        let mut p = ptr + Self::M;
        while self.has(p, end, 1) {
            let bt = self.bt(p);
            let start_name = matches!(bt, Bt::NonAscii | Bt::Nmstrt | Bt::Hex) || lead_len(bt).is_some();
            if start_name {
                if state == State::Other {
                    if let Some(a) = atts.get_mut(n_atts) {
                        a.name = p;
                        a.normalized = true;
                    }
                    state = State::InName;
                }
                if let Some(n) = lead_len(bt) {
                    // The encoding has already been validated.
                    p += n - Self::M;
                }
            } else {
                match bt {
                    Bt::Quot | Bt::Apos => {
                        if state != State::InValue {
                            if let Some(a) = atts.get_mut(n_atts) {
                                a.value_ptr = p + Self::M;
                            }
                            state = State::InValue;
                            open = bt;
                        } else if open == bt {
                            state = State::Other;
                            if let Some(a) = atts.get_mut(n_atts) {
                                a.value_end = p;
                            }
                            n_atts += 1;
                        }
                    }
                    Bt::Amp => {
                        if let Some(a) = atts.get_mut(n_atts) {
                            a.normalized = false;
                        }
                    }
                    Bt::S => {
                        if state == State::InName {
                            state = State::Other;
                        } else if state == State::InValue {
                            let denormalizes = |a: &Attribute| {
                                a.normalized
                                    && (p == a.value_ptr
                                        || self.ascii(p) != 0x20
                                        || self.ascii(p + Self::M) == 0x20
                                        || self.bt(p + Self::M) == open)
                            };
                            if let Some(a) = atts.get_mut(n_atts)
                                && denormalizes(a)
                            {
                                a.normalized = false;
                            }
                        }
                    }
                    Bt::Cr | Bt::Lf => {
                        // This case ensures that the first attribute name is
                        // counted. Apart from that we could just change state
                        // on the quote.
                        if state == State::InName {
                            state = State::Other;
                        } else if state == State::InValue
                            && let Some(a) = atts.get_mut(n_atts)
                        {
                            a.normalized = false;
                        }
                    }
                    Bt::Gt | Bt::Sol => {
                        if state != State::InValue {
                            return n_atts;
                        }
                    }
                    _ => {}
                }
            }
            p += Self::M;
        }
        n_atts
    }

    /// `charRefNumber`, additionally stopping (with `-1`) at the end of the
    /// buffer.
    fn char_ref_number(&self, ptr: usize) -> i32 {
        let end = self.b.len();
        let mut result: i32 = 0;
        // skip &#
        let mut p = ptr + 2 * Self::M;
        if self.cm(p, b'x') {
            p += Self::M;
            while !self.cm(p, b';') {
                if !self.has(p, end, 1) {
                    return -1;
                }
                let c = self.ascii(p);
                match u8::try_from(c) {
                    Ok(d @ b'0'..=b'9') => result = (result << 4) | i32::from(d - b'0'),
                    Ok(d @ b'A'..=b'F') => result = (result << 4) + 10 + i32::from(d - b'A'),
                    Ok(d @ b'a'..=b'f') => result = (result << 4) + 10 + i32::from(d - b'a'),
                    _ => {}
                }
                if result >= 0x11_0000 {
                    return -1;
                }
                p += Self::M;
            }
        } else {
            while !self.cm(p, b';') {
                if !self.has(p, end, 1) {
                    return -1;
                }
                let c = self.ascii(p);
                // Only digits reach here from the parser; for other input C
                // has signed overflow (undefined), the port wraps.
                result = result.wrapping_mul(10).wrapping_add(c.wrapping_sub(0x30));
                if result >= 0x11_0000 {
                    return -1;
                }
                p += Self::M;
            }
        }
        check_char_ref_number(result)
    }

    /// `predefinedEntityName`.
    fn predefined_entity_name(&self, mut p: usize, end: usize) -> Option<u8> {
        let is = |p: usize, c: u8| self.cm(p, c);
        match end.saturating_sub(p) / Self::M {
            2 => {
                if is(p + Self::M, b't') {
                    if is(p, b'l') {
                        return Some(b'<');
                    }
                    if is(p, b'g') {
                        return Some(b'>');
                    }
                }
            }
            3 => {
                if is(p, b'a') && is(p + Self::M, b'm') && is(p + 2 * Self::M, b'p') {
                    return Some(b'&');
                }
            }
            4 => {
                let (word, value) = if is(p, b'q') {
                    (b"uot", b'"')
                } else if is(p, b'a') {
                    (b"pos", b'\'')
                } else {
                    return None;
                };
                for c in *word {
                    p += Self::M;
                    if !is(p, c) {
                        return None;
                    }
                }
                return Some(value);
            }
            _ => {}
        }
        None
    }

    /// `nameMatchesAscii`.
    fn name_matches_ascii(&self, mut p: usize, end: usize, ascii: &[u8]) -> bool {
        for &c in ascii {
            if self.short(p, end, Self::M) {
                // Unreachable from the parser; kept like the C check.
                return false;
            }
            if !self.cm(p, c) {
                return false;
            }
            p += Self::M;
        }
        p == end
    }

    /// `nameLength`, additionally stopping at the end of the buffer.
    fn name_length(&self, start: usize) -> usize {
        let mut p = start;
        loop {
            let bt = self.bt(p);
            if let Some(n) = lead_len(bt) {
                // The encoding has already been validated.
                p += n;
                continue;
            }
            match bt {
                Bt::NonAscii
                | Bt::Nmstrt
                | Bt::Colon
                | Bt::Hex
                | Bt::Digit
                | Bt::Name
                | Bt::Minus
                    if p < self.b.len() =>
                {
                    p += Self::M;
                }
                _ => return p - start,
            }
        }
    }

    /// `skipS`, additionally stopping at the end of the buffer.
    fn skip_s(&self, mut p: usize) -> usize {
        while p < self.b.len() && matches!(self.bt(p), Bt::Lf | Bt::Cr | Bt::S) {
            p += Self::M;
        }
        p
    }

    /// `updatePosition`.
    fn update_position(&self, mut p: usize, end: usize, pos: &mut Position) {
        while self.has(p, end, 1) {
            let bt = self.bt(p);
            if let Some(n) = lead_len(bt) {
                // The encoding has already been validated.
                p += n;
                pos.column_number = pos.column_number.wrapping_add(1);
                continue;
            }
            match bt {
                Bt::Lf => {
                    pos.column_number = 0;
                    pos.line_number = pos.line_number.wrapping_add(1);
                    p += Self::M;
                }
                Bt::Cr => {
                    pos.line_number = pos.line_number.wrapping_add(1);
                    p += Self::M;
                    if self.has(p, end, 1) && self.bt(p) == Bt::Lf {
                        p += Self::M;
                    }
                    pos.column_number = 0;
                }
                _ => {
                    p += Self::M;
                    pos.column_number = pos.column_number.wrapping_add(1);
                }
            }
        }
    }
}

/// `checkCharRefNumber` (`xmltok.c`): `-1` for a code point that is not an
/// XML character, otherwise `result`.
pub fn check_char_ref_number(result: i32) -> i32 {
    match result >> 8 {
        0xD8..=0xDF => -1,
        0 => match u8::try_from(result) {
            Ok(b) if Table::Latin1.byte_type(b) == Bt::NonXml => -1,
            _ => result,
        },
        0xFF if result == 0xFFFE || result == 0xFFFF => -1,
        _ => result,
    }
}

/// One attribute found by [`get_atts`] (`ATTRIBUTE` in `xmltok.h`), as
/// offsets into the buffer. Fields that the C code does not write keep their
/// previous value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attribute {
    /// Start of the name.
    pub name: usize,
    /// First byte of the value (after the opening quote).
    pub value_ptr: usize,
    /// End of the value (the closing quote).
    pub value_end: usize,
    /// Whether the value needs no attribute-value normalisation.
    pub normalized: bool,
}

/// A line/column position (`POSITION`). Both count from 0 and wrap like the
/// C `XML_Size` (an `unsigned long`, 64-bit on the platforms in CI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    /// `lineNumber`
    pub line_number: u64,
    /// `columnNumber`
    pub column_number: u64,
}

fn s<'a, E: Encoding>(e: &'a E, b: &'a [u8]) -> S<'a, E> {
    S { e, b }
}

/// `XmlPrologTok`: scans `buf[ptr..]` in the prolog state.
pub fn prolog_tok<E: Encoding>(enc: &E, buf: &[u8], ptr: usize) -> Scanned {
    s(enc, buf).prolog_tok(ptr, buf.len())
}

/// `XmlContentTok`: scans `buf[ptr..]` in the content state.
pub fn content_tok<E: Encoding>(enc: &E, buf: &[u8], ptr: usize) -> Scanned {
    s(enc, buf).content_tok(ptr, buf.len())
}

/// `XmlCdataSectionTok`.
pub fn cdata_section_tok<E: Encoding>(enc: &E, buf: &[u8], ptr: usize) -> Scanned {
    s(enc, buf).cdata_section_tok(ptr, buf.len())
}

/// `XmlIgnoreSectionTok`.
pub fn ignore_section_tok<E: Encoding>(enc: &E, buf: &[u8], ptr: usize) -> Scanned {
    s(enc, buf).ignore_section_tok(ptr, buf.len())
}

/// `XmlAttributeValueTok`.
pub fn attribute_value_tok<E: Encoding>(enc: &E, buf: &[u8], ptr: usize) -> Scanned {
    s(enc, buf).literal_value_tok(ptr, buf.len(), false)
}

/// `XmlEntityValueTok`.
pub fn entity_value_tok<E: Encoding>(enc: &E, buf: &[u8], ptr: usize) -> Scanned {
    s(enc, buf).literal_value_tok(ptr, buf.len(), true)
}

/// `XmlIsPublicId` on the literal `buf[ptr..]` (quotes included).
pub fn is_public_id<E: Encoding>(enc: &E, buf: &[u8], ptr: usize) -> Result<(), usize> {
    s(enc, buf).is_public_id(ptr, buf.len())
}

/// `XmlGetAttributes` on the start tag at `buf[ptr]` (`<`). Fills at most
/// `atts.len()` entries and returns the number of attributes.
pub fn get_atts<E: Encoding>(enc: &E, buf: &[u8], ptr: usize, atts: &mut [Attribute]) -> usize {
    s(enc, buf).get_atts(ptr, atts)
}

/// `XmlCharRefNumber` on the reference at `buf[ptr]` (`&#`).
pub fn char_ref_number<E: Encoding>(enc: &E, buf: &[u8], ptr: usize) -> i32 {
    s(enc, buf).char_ref_number(ptr)
}

/// `XmlPredefinedEntityName` on the name `buf[ptr..]`: the replacement
/// character of `lt`, `gt`, `amp`, `quot` or `apos` (C returns `0` for
/// `None`).
pub fn predefined_entity_name<E: Encoding>(enc: &E, buf: &[u8], ptr: usize) -> Option<u8> {
    s(enc, buf).predefined_entity_name(ptr, buf.len())
}

/// `XmlNameMatchesAscii`: does the name `buf[ptr..]` equal `ascii`?
pub fn name_matches_ascii<E: Encoding>(enc: &E, buf: &[u8], ptr: usize, ascii: &[u8]) -> bool {
    s(enc, buf).name_matches_ascii(ptr, buf.len(), ascii)
}

/// `XmlNameLength` of the name starting at `buf[ptr]`, in bytes.
pub fn name_length<E: Encoding>(enc: &E, buf: &[u8], ptr: usize) -> usize {
    s(enc, buf).name_length(ptr)
}

/// `XmlSkipS`: the first non-white-space position at or after `ptr`.
pub fn skip_s<E: Encoding>(enc: &E, buf: &[u8], ptr: usize) -> usize {
    s(enc, buf).skip_s(ptr)
}

/// `XmlUpdatePosition` over `buf[ptr..]`.
pub fn update_position<E: Encoding>(enc: &E, buf: &[u8], ptr: usize, pos: &mut Position) {
    s(enc, buf).update_position(ptr, buf.len(), pos);
}
