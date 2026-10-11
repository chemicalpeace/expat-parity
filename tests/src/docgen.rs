//! Random XML-ish documents for the whole-parser differential tests.
//!
//! The generator produces mostly well-formed documents that use every
//! construct the module 10 parser handles (XML declaration, comments and
//! processing instructions in prolog, content and epilog, nested elements,
//! attributes with character and predefined entity references and white
//! space, namespace declarations including undeclaring and the reserved
//! prefixes and URIs, CDATA sections, CR / CR LF line ends, long text that
//! crosses the 1024-byte buffer boundaries) and then optionally damages
//! them (truncation, byte flips, inserted bytes), so the error paths are
//! compared as well.

/// A small deterministic PRNG (xorshift64*), so runs are reproducible from
/// a seed.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// A generator seeded with `seed` (0 is replaced by a constant).
    pub fn new(seed: u64) -> Self {
        Rng(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `0..n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    /// True with probability `1 / n`.
    pub fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }

    /// A random element of `items`.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const NAMES: &[&str] = &["a", "b", "doc", "p:e", "q:e", "é", "n-1", "_u", "a", "b"];
/// Element names that fail with namespaces (unbound or reserved prefixes).
const RARE_NAMES: &[&str] = &["x:y", "xml:a", "xmlns:b"];
const ATT_NAMES: &[&str] = &[
    "a", "b", "id", "p:a", "q:a", "xml:lang", "xmlns", "xmlns:p", "xmlns:q", "c", "xmlns:p",
    "xmlns:q", "xmlns",
];
/// Attribute names that fail with namespaces.
const RARE_ATT_NAMES: &[&str] = &["xmlns:xml", "xmlns:xmlns", "r:a"];
const ATT_VALUES: &[&str] = &[
    "1",
    "",
    " x  y ",
    "a&amp;b",
    "&lt;&gt;&quot;&apos;",
    "&#65;&#x42;&#32;",
    "&#x10000;",
    "line\nbreak\r\nand\rcr\ttab",
    "urn:p",
    "urn:q",
    "http://www.w3.org/XML/1998/namespace",
    "http://www.w3.org/2000/xmlns/",
    "a|b",
    "a\u{a0}b",
];
/// Values that make the document fail (used rarely).
const BAD_ATT_VALUES: &[&str] = &["&undefined;", "&#0;", "<", "&#xD800;", "&#xFFFE;"];
const TEXTS: &[&str] = &[
    "text",
    " ",
    "\n",
    "\r\n",
    "\r",
    "a&amp;b",
    "&lt;&#60;&#x3C;",
    "]]",
    "café ☃ 𝄞",
    "\t",
    "&#13;",
];
/// Content that makes the document fail (used rarely).
const BAD_TEXTS: &[&str] = &[
    "]]>",
    "&ent;",
    "<?xml version='1.0'?>",
    "&#1;",
    "<![CDATA[open",
];

/// Appends a random element (with content up to `depth` levels deep).
fn element(r: &mut Rng, out: &mut String, depth: usize) {
    let name = if r.one_in(25) {
        *r.pick(RARE_NAMES)
    } else {
        *r.pick(NAMES)
    };
    out.push('<');
    out.push_str(name);
    let natt = if r.one_in(3) { 0 } else { r.below(4) };
    for _ in 0..natt {
        let sep = *r.pick(&[' ', '\n', '\t', ' ']);
        out.push(sep);
        let att = if r.one_in(30) {
            *r.pick(RARE_ATT_NAMES)
        } else {
            *r.pick(ATT_NAMES)
        };
        out.push_str(att);
        let eq = *r.pick(&["=", " = "]);
        out.push_str(eq);
        let q = *r.pick(&['"', '\'']);
        out.push(q);
        let value = if r.one_in(40) {
            *r.pick(BAD_ATT_VALUES)
        } else {
            *r.pick(ATT_VALUES)
        };
        out.push_str(value);
        out.push(q);
    }
    if depth == 0 || r.one_in(4) {
        out.push_str(r.pick(&["/>", " />"]));
        return;
    }
    out.push('>');
    for _ in 0..r.below(5) {
        match r.below(9) {
            0..=2 => {
                let t = if r.one_in(30) {
                    *r.pick(BAD_TEXTS)
                } else {
                    *r.pick(TEXTS)
                };
                out.push_str(t);
            }
            3 | 4 => element(r, out, depth - 1),
            5 => {
                out.push_str("<![CDATA[");
                out.push_str(r.pick(&["", "x<y&z", "a]b", "\r\n", "]]"]));
                out.push_str("]]>");
            }
            6 => out.push_str(r.pick(&["<!--c-->", "<!---->", "<!--a\r\nb-->"])),
            7 => out.push_str(r.pick(&["<?pi data?>", "<?pi?>", "<?p\r\n x\ry ?>"])),
            _ => {
                // Long text crosses buffer boundaries.
                let n = r.below(3000);
                for i in 0..n {
                    out.push(if i % 61 == 60 { '\n' } else { 'z' });
                }
            }
        }
    }
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

/// A random document as text (before encoding).
pub fn document_text(r: &mut Rng) -> String {
    let mut out = String::new();
    if r.one_in(2) {
        out.push_str(r.pick(&[
            "<?xml version='1.0'?>",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<?xml version='1.0' encoding='ISO-8859-1'?>",
            "<?xml version='1.0' encoding='US-ASCII'?>",
            "<?xml version='1.0' encoding='UTF-16'?>",
            "<?xml version='1.0' encoding='x-unknown'?>",
            "<?xml version='1.0' standalone='yes'?>",
            "<?xml version='1.0' standalone='no' ?>",
            "<?xml version='1.1'?>",
            "<?xml encoding='UTF-8'?>",
            "<?xml version='1.0' encoding='UTF-8' standalone='maybe'?>",
        ]));
    }
    for _ in 0..r.below(3) {
        let misc = *r.pick(&["\n", "<!--p-->", "<?pi x?>", "\r\n", " "]);
        out.push_str(misc);
    }
    let depth = 1 + r.below(4);
    element(r, &mut out, depth);
    for _ in 0..r.below(3) {
        let misc = *r.pick(&[
            "\n", "<!--e-->", "<?pi y?>", "\r\n", "\r", " ", "junk", "<a/>",
        ]);
        out.push_str(misc);
    }
    out
}

/// Encodes `text` as UTF-8, UTF-16LE/BE (with or without BOM) or Latin-1
/// (characters outside Latin-1 become `?`).
pub fn encode(r: &mut Rng, text: &str) -> Vec<u8> {
    match r.below(6) {
        0 | 1 => {
            let mut v = Vec::new();
            if r.one_in(4) {
                v.extend_from_slice(b"\xEF\xBB\xBF");
            }
            v.extend_from_slice(text.as_bytes());
            v
        }
        2 | 3 => {
            let le = r.one_in(2);
            let mut v = Vec::new();
            if r.one_in(2) {
                v.extend_from_slice(if le { b"\xFF\xFE" } else { b"\xFE\xFF" });
            }
            for u in text.encode_utf16() {
                let b = if le { u.to_le_bytes() } else { u.to_be_bytes() };
                v.extend_from_slice(&b);
            }
            v
        }
        _ => {
            let mut v = Vec::new();
            if !text.starts_with("<?xml") && r.one_in(2) {
                v.extend_from_slice(b"<?xml version='1.0' encoding='ISO-8859-1'?>");
            }
            v.extend(
                text.chars()
                    .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')),
            );
            v
        }
    }
}

/// Damages a document: truncation, byte flips or inserted bytes.
pub fn mutate(r: &mut Rng, doc: &mut Vec<u8>) {
    for _ in 0..1 + r.below(3) {
        if doc.is_empty() {
            return;
        }
        let i = r.below(doc.len());
        match r.below(4) {
            0 => doc.truncate(i),
            1 => doc[i] = r.next_u64() as u8,
            2 => {
                let b = *r.pick(&[b'<', b'&', b'\r', b'>', b'"', 0, 0x80, b']']);
                doc.insert(i, b);
            }
            _ => {
                doc.remove(i);
            }
        }
    }
}

/// A random document: generated, encoded, and damaged with probability
/// 1/3.
pub fn document(r: &mut Rng) -> Vec<u8> {
    let text = document_text(r);
    let mut doc = encode(r, &text);
    if r.one_in(40) {
        // Ends inside a multi-byte character (in UTF-8 documents).
        doc.extend_from_slice(b"\xE2\x82");
    }
    if r.one_in(3) {
        mutate(r, &mut doc);
    }
    doc
}
