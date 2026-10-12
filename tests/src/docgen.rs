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

// ---------------------------------------------------------------------
// Documents with a document type declaration (module 11).
// ---------------------------------------------------------------------

const ENTITY_NAMES: &[&str] = &["e1", "e2", "e3", "ext", "unp", "lt", "amp", "p:x", "é"];
const PE_NAMES: &[&str] = &["pe1", "pe2", "pe3", "extpe"];
const ENTITY_VALUES: &[&str] = &[
    "",
    "text",
    "a&amp;b",
    "&#60;b/&#62;",
    "&#x3C;b>x&#x3C;/b>",
    "<b>in</b>",
    "&e1;",
    "&e2;&e2;",
    "&e3;",
    "&ext;",
    "&undeclared;",
    "&#37;pe1;",
    "line\nbreak\r\nand\rcr",
    "&#38;#38;",
    "]]>",
    "<a/>",
    "<!ELEMENT q ANY>",
    "<!ENTITY e9 'nine'>",
    "&#x10000;☃",
];
/// Entity values that are errors (used rarely).
const BAD_ENTITY_VALUES: &[&str] = &["%pe1;", "&#0;", "&#xD800;", "&", "&#x110000;", "%"];
const PUBLIC_IDS: &[&str] = &[
    "-//X//DTD x//EN",
    "  spaced \r\n  id  ",
    "",
    "bad{char}",
    "a'b",
];

/// A random content model (`depth` levels of nested groups).
fn content_model(r: &mut Rng, out: &mut String, depth: usize) {
    out.push('(');
    let n = 1 + r.below(4);
    let sep = *r.pick(&[",", "|", " , ", "|"]);
    for i in 0..n {
        if i > 0 {
            out.push_str(sep);
        }
        if depth > 0 && r.one_in(3) {
            content_model(r, out, depth - 1);
        } else {
            out.push_str(r.pick(NAMES));
            out.push_str(r.pick(&["", "", "?", "*", "+"]));
        }
    }
    if r.one_in(40) {
        // Mixed connectors: a syntax error.
        out.push_str(",x|y");
    }
    out.push(')');
    out.push_str(r.pick(&["", "", "?", "*", "+"]));
}

fn quoted(r: &mut Rng, out: &mut String, value: &str) {
    let q = if value.contains('"') {
        '\''
    } else if value.contains('\'') {
        '"'
    } else {
        *r.pick(&['"', '\''])
    };
    out.push(q);
    out.push_str(value);
    out.push(q);
}

/// One markup declaration (or a PE reference, comment, PI or white space
/// between declarations).
fn declaration(r: &mut Rng, out: &mut String) {
    match r.below(14) {
        0 | 1 => {
            out.push_str("<!ELEMENT ");
            out.push_str(r.pick(NAMES));
            out.push(' ');
            match r.below(6) {
                0 => out.push_str("EMPTY"),
                1 => out.push_str("ANY"),
                2 => out.push_str("(#PCDATA)"),
                3 => {
                    out.push_str("(#PCDATA");
                    for _ in 0..r.below(4) {
                        out.push('|');
                        out.push_str(r.pick(NAMES));
                    }
                    out.push_str(")*");
                }
                _ => {
                    let depth = r.below(4);
                    content_model(r, out, depth);
                }
            }
            out.push('>');
        }
        2..=4 => {
            out.push_str("<!ATTLIST ");
            out.push_str(r.pick(NAMES));
            for _ in 0..1 + r.below(3) {
                out.push(' ');
                out.push_str(r.pick(ATT_NAMES));
                out.push(' ');
                out.push_str(r.pick(&[
                    "CDATA",
                    "ID",
                    "IDREF",
                    "IDREFS",
                    "ENTITY",
                    "ENTITIES",
                    "NMTOKEN",
                    "NMTOKENS",
                    "(x|y|z)",
                    "( x | y )",
                    "NOTATION (n1|n2)",
                    "CDATA",
                ]));
                out.push(' ');
                match r.below(5) {
                    0 => out.push_str("#IMPLIED"),
                    1 => out.push_str("#REQUIRED"),
                    2 => {
                        out.push_str("#FIXED ");
                        let v = *r.pick(ATT_VALUES);
                        quoted(r, out, v);
                    }
                    _ => {
                        let v = if r.one_in(3) {
                            *r.pick(&["&e1;", "x&e2;y", "&ext;", "&unp;", "&undeclared;", "&lt;"])
                        } else {
                            *r.pick(ATT_VALUES)
                        };
                        quoted(r, out, v);
                    }
                }
            }
            out.push('>');
        }
        5..=7 => {
            out.push_str("<!ENTITY ");
            let is_pe = r.one_in(3);
            if is_pe {
                out.push_str("% ");
                out.push_str(r.pick(PE_NAMES));
            } else {
                out.push_str(r.pick(ENTITY_NAMES));
            }
            out.push(' ');
            match r.below(6) {
                0 => {
                    out.push_str("SYSTEM ");
                    quoted(r, out, "sys.ent");
                }
                1 => {
                    out.push_str("PUBLIC ");
                    let p = *r.pick(PUBLIC_IDS);
                    quoted(r, out, p);
                    out.push(' ');
                    quoted(r, out, "sys2.ent");
                }
                2 if !is_pe => {
                    out.push_str("SYSTEM 'u.gif' NDATA ");
                    out.push_str(r.pick(&["n1", "n2", "undeclared"]));
                }
                _ => {
                    let v = if r.one_in(15) {
                        *r.pick(BAD_ENTITY_VALUES)
                    } else if is_pe && r.one_in(2) {
                        *r.pick(&[
                            "<!ENTITY e3 'from pe'>",
                            "<!ELEMENT z ANY>",
                            "<!ATTLIST doc pa CDATA 'pe default'>",
                            "%pe2;",
                            "&#37;pe1;",
                            "<!ENTITY e1 '&#37;pe1;'>",
                            "",
                            "  ",
                        ])
                    } else {
                        *r.pick(ENTITY_VALUES)
                    };
                    quoted(r, out, v);
                }
            }
            out.push_str(r.pick(&[">", " >"]));
        }
        8 => {
            out.push_str("<!NOTATION ");
            out.push_str(r.pick(&["n1", "n2"]));
            match r.below(3) {
                0 => out.push_str(" SYSTEM 'viewer'"),
                1 => {
                    out.push_str(" PUBLIC ");
                    let p = *r.pick(PUBLIC_IDS);
                    quoted(r, out, p);
                }
                _ => out.push_str(" PUBLIC 'p' 'viewer'"),
            }
            out.push('>');
        }
        9 => {
            out.push_str(r.pick(&["%pe1;", "%pe2;", "%extpe;", "%undeclared;", " %pe1; "]));
        }
        10 => out.push_str(r.pick(&["<!--dtd comment-->", "<?dtdpi x?>", "<!---->"])),
        11 => out.push_str(r.pick(&["\n", " ", "\r\n", "\t"])),
        12 => {
            // Rare malformed declarations.
            if r.one_in(3) {
                out.push_str(r.pick(&[
                    "<!ELEMENT>",
                    "<!ATTLIST a b>",
                    "<!ENTITY a>",
                    "<!ENTITY % a 'x' NDATA n>",
                    "<!NOTATION n>",
                    "<!ELEMENT a (b,c|d)>",
                    "<!ELEMENT a (#PCDATA|b)>",
                    "<!ELEMENT a (#PCDATA)+>",
                    "<!ATTLIST a b CDATA '<'>",
                    "<!ATTLIST a b ID #FIXED 'x'>",
                    "<!ENTITY a 'x' 'y'>",
                    "<![INCLUDE[ ]]>",
                    "<!DOCTYPE x>",
                    "<?xml version='1.0'?>",
                    "<!ELEMENT a %pe1;>",
                    "<!ENTITY a PUBLIC 'x'>",
                    "]",
                ]));
            }
        }
        _ => {
            // Many attributes with defaults for one element type.
            out.push_str("<!ATTLIST doc");
            for i in 0..r.below(20) {
                out.push_str(&format!(" d{i} CDATA 'v{i}'"));
            }
            out.push('>');
        }
    }
}

/// Content that uses the declarations: entity references in text and in
/// attribute values, and elements whose types have attribute defaults.
fn dtd_content(r: &mut Rng, out: &mut String, depth: usize) {
    let name = *r.pick(&["doc", "a", "b", "doc"]);
    out.push('<');
    out.push_str(name);
    for _ in 0..r.below(3) {
        out.push(' ');
        out.push_str(r.pick(&["a", "id", "c", "p:a", "xmlns:p"]));
        out.push('=');
        let v = *r.pick(&[
            "'1'",
            "'&e1;'",
            "' x &e2; '",
            "'&ext;'",
            "'&lt;'",
            "'&undeclared;'",
            "'urn:p'",
        ]);
        out.push_str(v);
    }
    out.push('>');
    for _ in 0..r.below(5) {
        match r.below(6) {
            0 | 1 => out.push_str(r.pick(&[
                "&e1;",
                "&e2;",
                "&e3;",
                "&ext;",
                "&unp;",
                "&undeclared;",
                "&e9;",
                "&lt;",
            ])),
            2 => out.push_str(r.pick(TEXTS)),
            3 if depth > 0 => dtd_content(r, out, depth - 1),
            4 => out.push_str(r.pick(&["<a/>", "<b x='y'/>", "<doc/>", "<![CDATA[&e1;]]>"])),
            _ => out.push_str(r.pick(&["<!--c-->", "<?pi?>"])),
        }
    }
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

/// A random document with a document type declaration, as text.
pub fn dtd_document_text(r: &mut Rng) -> String {
    let mut out = String::new();
    if r.one_in(2) {
        out.push_str(r.pick(&[
            "<?xml version='1.0'?>",
            "<?xml version='1.0' standalone='yes'?>",
            "<?xml version='1.0' standalone='no'?>",
            "<?xml version='1.0' encoding='UTF-8'?>",
        ]));
    }
    out.push_str(r.pick(&["", "\n", "<!--before-->"]));
    out.push_str("<!DOCTYPE ");
    out.push_str(r.pick(&["doc", "doc", "a", "p:e"]));
    match r.below(4) {
        0 => out.push_str(" SYSTEM 'doc.dtd'"),
        1 => {
            out.push_str(" PUBLIC ");
            let p = *r.pick(PUBLIC_IDS);
            quoted(r, &mut out, p);
            out.push_str(" 'doc.dtd'");
        }
        _ => {}
    }
    if !r.one_in(5) {
        out.push_str(" [");
        for _ in 0..r.below(14) {
            declaration(r, &mut out);
            if r.one_in(2) {
                out.push('\n');
            }
        }
        out.push(']');
    }
    out.push_str(r.pick(&[">", " >", ">\n"]));
    let depth = r.below(3);
    dtd_content(r, &mut out, depth);
    out
}

/// A random document with a document type declaration: generated,
/// encoded, and damaged with probability 1/4.
pub fn dtd_document(r: &mut Rng) -> Vec<u8> {
    let text = dtd_document_text(r);
    let mut doc = encode(r, &text);
    if r.one_in(4) {
        mutate(r, &mut doc);
    }
    doc
}

/// A piece of replacement text for a general entity: text, character and
/// predefined references, references to other entities (by index, so the
/// caller controls cycles), balanced or unbalanced elements, comments,
/// processing instructions and CDATA sections.
fn entity_text(r: &mut Rng, out: &mut String, n_entities: usize, below: usize) {
    for _ in 0..r.below(5) {
        match r.below(12) {
            0..=2 => out.push_str(r.pick(&["x", "text ", " sp  ace ", "\u{e9}", "a\nb", "\t"])),
            3 => out.push_str(r.pick(&[
                "&#65;",
                "&#x20;",
                "&amp;",
                "&lt;",
                "&#38;#60;",
                "&#38;#38;",
            ])),
            4 | 5 => {
                // Mostly references to later entities (no cycle); sometimes
                // to any entity, which can close a cycle.
                let j = if below < n_entities && !r.one_in(6) {
                    below + r.below(n_entities - below)
                } else {
                    r.below(n_entities.max(1))
                };
                out.push_str(&format!("&e{j};"));
            }
            6 => out.push_str(r.pick(&["&undeclared;", "&un;", "&ext;", "&unp;"])),
            7 => {
                out.push_str(r.pick(&["<i/>", "<i a=\"v\"/>", "<i>in</i>", "<i a=\"&amp;\">t</i>"]))
            }
            8 => out.push_str(r.pick(&[
                "<b>", "</b>", "<b></b>", "<b>x</b>", "<b/>", "<i>", "<b>y</b>", "]]>",
            ])),
            9 => out.push_str(r.pick(&["<!--c-->", "<?pi d?>", "<![CDATA[x<y]]>"])),
            // Rare: a character reference C rejects at declaration time.
            _ => out.push_str(r.pick(&["\r\n", "&#13;", "&#x10FFFF;", "\r", "&#10;", "&#xD800;"])),
        }
    }
}

/// A random document built around internal entities (general and
/// parameter), as text. References form a random graph with occasional
/// direct and indirect cycles; entities are referenced in content, in
/// attribute values, in default attribute values, and through parameter
/// entities between and inside declarations.
pub fn entity_document_text(r: &mut Rng) -> String {
    let n = 1 + r.below(7);
    let mut out = String::new();
    out.push_str(r.pick(&[
        "",
        "",
        "<?xml version='1.0'?>",
        "<?xml version='1.0' standalone='yes'?>",
        "<?xml version='1.0' standalone='no'?>",
    ]));
    out.push_str("<!DOCTYPE r");
    if r.one_in(5) {
        out.push_str(" SYSTEM 'r.dtd'");
    }
    out.push_str(" [");
    if r.one_in(3) {
        out.push_str("<!ENTITY ext SYSTEM 'ext.xml'><!NOTATION n SYSTEM 'n'><!ENTITY unp SYSTEM 'u' NDATA n>");
    }
    for i in 0..n {
        let mut v = String::new();
        entity_text(r, &mut v, n, i + 1);
        out.push_str(&format!("<!ENTITY e{i} '{v}'>"));
        if r.one_in(4) {
            // A repeated declaration: the first one wins.
            out.push_str(&format!("<!ENTITY e{i} 'second'>"));
        }
    }
    let pe = r.below(4);
    for k in 0..pe {
        let body = match r.below(6) {
            0 => format!("<!ENTITY e{} \"from pe\">", r.below(n + 2)),
            1 => "<!ATTLIST r d CDATA \"&e0;\">".to_string(),
            // `&#37;` is stored as `%`, so the replacement text holds the
            // parameter entity reference (a literal `%` in an entity value
            // of the internal subset would be an error).
            2 => format!("&#37;p{};", r.below(pe)),
            3 => format!("<!ENTITY &#37; p{} \"&#37;p{};\">", pe + k, r.below(pe + 1)),
            4 => "<!ELEMENT r ANY>".to_string(),
            _ => r
                .pick(&["<!ENTITY", "\"x\">", "<!-- c -->", "<?pi?>"])
                .to_string(),
        };
        out.push_str(&format!("<!ENTITY % p{k} '{body}'>"));
    }
    for _ in 0..r.below(3) {
        if pe > 0 {
            out.push_str(&format!("%p{};", r.below(pe + 1)));
        }
    }
    if r.one_in(2) {
        out.push_str(&format!(
            "<!ATTLIST r a CDATA \"&e{};\" t NMTOKENS \"  &e{}; x \" f CDATA #FIXED \"&amp;&e{};\">",
            r.below(n),
            r.below(n),
            r.below(n)
        ));
    }
    out.push_str("]>");
    out.push_str("<r");
    if r.one_in(2) {
        out.push_str(&format!(" s=\"v&e{};w\"", r.below(n)));
    }
    if r.one_in(4) {
        out.push_str(" t=' &e0;  y '");
    }
    out.push('>');
    for _ in 0..1 + r.below(4) {
        match r.below(4) {
            0 => out.push_str("txt"),
            1 => out.push_str("<c>&e0;</c>"),
            _ => out.push_str(&format!("&e{};", r.below(n))),
        }
    }
    out.push_str("</r>");
    out
}

/// A random entity document ([`entity_document_text`]), encoded, and
/// damaged with probability 1/5.
pub fn entity_document(r: &mut Rng) -> Vec<u8> {
    let text = entity_document_text(r);
    let mut doc = encode(r, &text);
    if r.one_in(5) {
        mutate(r, &mut doc);
    }
    doc
}

/// Separates a `parse_trace` fuzz input's document from the bodies of its
/// external entities (module 11d, step D2; [`split_external_input`]).
pub const EXTERNAL_MARKER: &[u8] = b"\x00\xEE\xEE\x00";

/// The system identifiers a document declares (`SYSTEM 'x'` or
/// `SYSTEM "x"`, ASCII spelling), distinct, in order of appearance.
pub fn system_ids(doc: &[u8]) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut i = 0;
    while let Some(p) = doc
        .get(i..)
        .and_then(|rest| rest.windows(6).position(|w| w == b"SYSTEM"))
    {
        let mut j = i + p + 6;
        while doc
            .get(j)
            .is_some_and(|c| matches!(c, b' ' | b'\t' | b'\r' | b'\n'))
        {
            j += 1;
        }
        if let Some(&q) = doc.get(j).filter(|&&q| q == b'\'' || q == b'"') {
            let start = j + 1;
            if let Some(len) = doc
                .get(start..)
                .and_then(|r| r.iter().position(|&c| c == q))
            {
                let id = doc.get(start..start + len).unwrap_or_default().to_vec();
                if !out.contains(&id) {
                    out.push(id);
                }
                j = start + len;
            }
        }
        i = j.max(i + p + 6);
    }
    out
}

/// Splits a fuzz document at [`EXTERNAL_MARKER`]: the document, and the
/// entity bodies (further marker-separated segments) assigned in order to
/// the document's system identifiers ([`system_ids`]). `None` without a
/// marker.
pub fn split_external_input(data: &[u8]) -> Option<(&[u8], Vec<(Vec<u8>, Vec<u8>)>)> {
    let m = EXTERNAL_MARKER;
    let at = data.windows(m.len()).position(|w| w == m)?;
    let doc = data.get(..at)?;
    let mut bodies = Vec::new();
    let mut rest = data.get(at + m.len()..).unwrap_or_default();
    loop {
        match rest.windows(m.len()).position(|w| w == m) {
            Some(p) => {
                bodies.push(rest.get(..p).unwrap_or_default().to_vec());
                rest = rest.get(p + m.len()..).unwrap_or_default();
            }
            None => {
                bodies.push(rest.to_vec());
                break;
            }
        }
    }
    let entities = system_ids(doc).into_iter().zip(bodies).collect();
    Some((doc, entities))
}

/// Joins a document and entity bodies into a fuzz document
/// ([`split_external_input`] is the inverse for the bodies of the
/// document's system identifiers, in order).
pub fn join_external_input(doc: &[u8], bodies: &[Vec<u8>]) -> Vec<u8> {
    let mut out = doc.to_vec();
    for b in bodies {
        out.extend_from_slice(EXTERNAL_MARKER);
        out.extend_from_slice(b);
    }
    if bodies.is_empty() {
        out.extend_from_slice(EXTERNAL_MARKER);
    }
    out
}

/// A random fragment of an external general entity (content).
fn external_content(r: &mut Rng, n: usize, out: &mut String) {
    for _ in 0..1 + r.below(4) {
        let s = match r.below(16) {
            0 => "text ".to_string(),
            1 => "<a x='1'>t</a>".to_string(),
            2 => format!("&g{};", r.below(n)),
            3 => "&e0;".to_string(),
            4 => "<p:q xmlns:p='urn:p'/>".to_string(),
            5 => "<q:z/>".to_string(),
            6 => "<b>".to_string(),
            7 => "</b>".to_string(),
            8 => "<![CDATA[c]]>".to_string(),
            9 => "<?pi d?><!--c-->".to_string(),
            10 => "&#x41;&#66;".to_string(),
            11 => "&undeclared;".to_string(),
            12 => "<c a='&e0;'/>".to_string(),
            13 => "\r\n".to_string(),
            14 => "</r>".to_string(),
            _ => "<d xmlns='urn:x'>&g0;</d>".to_string(),
        };
        out.push_str(&s);
    }
}

/// A random fragment of an external parameter entity or external subset
/// (declarations, conditional sections, references).
fn external_decls(r: &mut Rng, n: usize, out: &mut String) {
    for _ in 0..1 + r.below(4) {
        let s = match r.below(20) {
            0 => format!("<!ENTITY e{} 'from ext'>", r.below(3)),
            1 => "<!ENTITY % q 'x'>".to_string(),
            2 => format!("%p{};", r.below(n)),
            3 => "<![INCLUDE[ <!ENTITY i 'inc'> ]]>".to_string(),
            4 => "<![IGNORE[ <!ENTITY i 'ign'> <![ nested ]]> ]]>".to_string(),
            5 => "<!ATTLIST r a CDATA 'd' b CDATA #IMPLIED>".to_string(),
            6 => "<!ELEMENT r ANY>".to_string(),
            7 => format!("<!ENTITY v '%p{};'>", r.below(n)),
            8 => "<!ENTITY w 'a%q;b'>".to_string(),
            9 => "<!ENTITY".to_string(),
            10 => "<![INCLUDE[".to_string(),
            11 => "]]>".to_string(),
            12 => "<![%q;[ <!ENTITY j 'cond'> ]]>".to_string(),
            13 => format!("<!ENTITY g{} SYSTEM 'g{}.xml'>", r.below(n), r.below(n)),
            14 => "<!ENTITY u SYSTEM 'u' NDATA n><!NOTATION n SYSTEM 'n'>".to_string(),
            15 => "<!-- c --><?pi?>".to_string(),
            16 => "<![IGNORE[ unclosed".to_string(),
            17 => format!("<!ENTITY % p{} SYSTEM 'p{}.ent'>", r.below(n), r.below(n)),
            18 => "<r/>".to_string(),
            _ => "<!ELEMENT a (#PCDATA|b)*>".to_string(),
        };
        out.push_str(&s);
    }
}

/// A random document with external general and parameter entities and an
/// optional external subset, with the bodies of its entities (by system
/// identifier; some identifiers have no body, some bodies are damaged or
/// encoded differently). Module 11d, step D2.
pub fn external_case(r: &mut Rng) -> (Vec<u8>, Vec<(Vec<u8>, Vec<u8>)>) {
    let n = 1 + r.below(3);
    let mut doc = String::new();
    doc.push_str(r.pick(&[
        "",
        "",
        "<?xml version='1.0'?>",
        "<?xml version='1.0' standalone='yes'?>",
        "<?xml version='1.0' standalone='no'?>",
    ]));
    doc.push_str("<!DOCTYPE r");
    if r.one_in(2) {
        doc.push_str(" SYSTEM 'sub.dtd'");
    }
    doc.push_str(" [<!ENTITY e0 'int'>");
    for k in 0..n {
        doc.push_str(&format!("<!ENTITY g{k} SYSTEM 'g{k}.xml'>"));
        if r.one_in(2) {
            doc.push_str(&format!("<!ENTITY % p{k} SYSTEM 'p{k}.ent'>"));
        }
    }
    for _ in 0..r.below(3) {
        doc.push_str(&format!("%p{};", r.below(n)));
    }
    if r.one_in(4) {
        doc.push_str("<!ATTLIST r xmlns:q CDATA 'urn:q'>");
    }
    doc.push_str("]>");
    doc.push_str(r.pick(&["<r>", "<r xmlns='urn:d' xmlns:p='urn:p'>", "<r a='1'>"]));
    for _ in 0..1 + r.below(3) {
        match r.below(4) {
            0 => doc.push_str("x"),
            1 => doc.push_str("<k/>"),
            _ => doc.push_str(&format!("&g{};", r.below(n))),
        }
    }
    doc.push_str("</r>");
    let doc = encode(r, &doc);
    let mut entities = Vec::new();
    for id in system_ids(&doc) {
        if r.one_in(8) {
            continue;
        }
        let general = id.ends_with(b".xml");
        let mut text = String::new();
        text.push_str(r.pick(&[
            "",
            "",
            "",
            "<?xml encoding='UTF-8'?>",
            "<?xml version='1.0' encoding='ISO-8859-1'?>",
            "<?xml version='1.0'?>",
            "<?xml standalone='yes' encoding='UTF-8'?>",
        ]));
        if general && !r.one_in(6) {
            external_content(r, n, &mut text);
        } else {
            external_decls(r, n, &mut text);
        }
        let mut body = if text.starts_with("<?xml") || r.one_in(2) {
            let mut v = Vec::new();
            if r.one_in(5) {
                v.extend_from_slice(b"\xEF\xBB\xBF");
            }
            v.extend_from_slice(text.as_bytes());
            v
        } else {
            encode(r, &text)
        };
        if r.one_in(6) {
            mutate(r, &mut body);
        }
        entities.push((id, body));
    }
    (doc, entities)
}
