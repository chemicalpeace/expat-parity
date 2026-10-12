//! External parsed entities and their encoding, against the C oracle
//! (docs/EXTERNAL-ENTITY-ENCODING.md has the matrix, the C source
//! references and how external entity processing is enabled).
//!
//! Every case is parsed by libexpat 2.9.0 and by the port with identical
//! input bytes, identical parser configuration and identical test handlers
//! (`ExtHandler::None` / `Fail` / `Accept` / `Read`). The `encodingName`
//! argument of `XML_ExternalEntityParserCreate` is set through
//! `ParseOptions::ext_encoding` on both sides (the shim's `ext_encoding`,
//! the port adapter's `ExternalEntities::create(context, encoding)`).
//!
//! For each case the C result and the port result are recorded separately
//! ([`Outcome`]: final status, error code, line, column, byte index, every
//! `extend` line of a child parser, the handler's return value for every
//! `extref`, and the full canonical trace, i.e. the event and callback
//! sequence) and then compared. A difference fails the test with both
//! records.
//!
//! Groups: `encoding_matrix` (entity context x body encoding / BOM / text
//! declaration x `encodingName` argument x the parent's protocol encoding,
//! fed whole, in 1-byte chunks and in 7-byte chunks through the buffer
//! API), `configuration_matrix` (parameter entity parsing, namespaces,
//! namespace triplets, foreign DTD, standalone, not-standalone handler and
//! every handler mode), `c_behaviour_pins` (C's results the document
//! states, pinned so the document stays true) and `matrix_dump` (writes
//! the per-case table when `EXT_ENC_MATRIX` names a file).

use expat_difftest::port::port_trace;
use expat_oracle::{ExtHandler, ParamEntityParsing, ParseOptions, parse_trace};
use std::fmt::Write as _;

/// In-memory external entities: (system identifier, bytes).
type Entities = Vec<(Vec<u8>, Vec<u8>)>;

/// A pinned C result: context, body, `encodingName`, parent protocol
/// encoding and scheme, the parent's error code, the first child's
/// `extend` line.
type Pin = (
    Ctx,
    &'static str,
    Option<&'static str>,
    (Option<&'static str>, Scheme),
    i64,
    &'static str,
);

/// How the bytes of an entity body or a document are produced from text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scheme {
    Utf8,
    Utf8Bom,
    Le,
    LeBom,
    Be,
    BeBom,
    /// Every character as one byte (ISO-8859-1; text must be below U+0100).
    Latin1,
}

fn encode(scheme: Scheme, text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    match scheme {
        Scheme::Utf8Bom => out.extend_from_slice(&[0xEF, 0xBB, 0xBF]),
        Scheme::LeBom => out.extend_from_slice(&[0xFF, 0xFE]),
        Scheme::BeBom => out.extend_from_slice(&[0xFE, 0xFF]),
        _ => {}
    }
    match scheme {
        Scheme::Utf8 | Scheme::Utf8Bom => out.extend_from_slice(text.as_bytes()),
        Scheme::Le | Scheme::LeBom => out.extend(text.encode_utf16().flat_map(u16::to_le_bytes)),
        Scheme::Be | Scheme::BeBom => out.extend(text.encode_utf16().flat_map(u16::to_be_bytes)),
        Scheme::Latin1 => out.extend(
            text.chars()
                .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')),
        ),
    }
    out
}

/// Where the external entity is referenced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ctx {
    /// External parsed general entity referenced in content (non-NULL
    /// context, `externalEntityInitProcessor`, content state).
    General,
    /// External DTD subset (NULL context, `externalParEntInitProcessor`,
    /// prolog state).
    Subset,
    /// External parameter entity referenced in the internal subset (NULL
    /// context, prolog state).
    Param,
    /// External parameter entity referenced inside an entity value (NULL
    /// context, `entityValueInitProcessor`: its text declaration goes
    /// through `processXmlDecl(parser, 0, ...)`).
    Value,
}

impl Ctx {
    const ALL: [Ctx; 4] = [Ctx::General, Ctx::Subset, Ctx::Param, Ctx::Value];

    fn name(self) -> &'static str {
        match self {
            Ctx::General => "G",
            Ctx::Subset => "S",
            Ctx::Param => "P",
            Ctx::Value => "V",
        }
    }

    /// The parent document (text; encoded with the parent's scheme).
    fn document(self, standalone: bool) -> String {
        let decl = if standalone {
            "<?xml version=\"1.0\" standalone=\"yes\"?>"
        } else {
            ""
        };
        match self {
            Ctx::General => format!("{decl}<!DOCTYPE d [<!ENTITY e SYSTEM \"ent\">]><d>&e;</d>"),
            Ctx::Subset => format!("{decl}<!DOCTYPE d SYSTEM \"ent\"><d>&x;</d>"),
            Ctx::Param => {
                format!("{decl}<!DOCTYPE d [<!ENTITY % p SYSTEM \"ent\">%p;]><d>&x;</d>")
            }
            // A parameter entity reference in an entity value is only
            // allowed outside the internal subset: the external subset
            // `sub` declares both.
            Ctx::Value => format!("{decl}<!DOCTYPE d SYSTEM \"sub\"><d>&x;</d>"),
        }
    }

    /// The in-memory entities: `ent` (the entity under test) and, for
    /// [`Ctx::Value`], the external subset `sub`, written in the scheme
    /// the `encodingName` argument (which applies to every child) reads.
    fn entities(self, ext: Option<&str>, body: Vec<u8>) -> Entities {
        let mut v = vec![(b"ent".to_vec(), body)];
        if self == Ctx::Value {
            let scheme = match ext {
                Some("UTF-16" | "UTF-16BE") => Scheme::Be,
                Some("UTF-16LE") => Scheme::Le,
                _ => Scheme::Utf8,
            };
            let sub = encode(scheme, "<!ENTITY % p SYSTEM \"ent\"><!ENTITY x \"[%p;]\">");
            v.push((b"sub".to_vec(), sub));
        }
        v
    }

    /// The entity's content after the text declaration: a non-ASCII
    /// character shows which encoding decoded it.
    fn payload(self) -> &'static str {
        match self {
            Ctx::General => "<t>\u{e9}x</t>",
            Ctx::Subset | Ctx::Param => "<!ENTITY x \"\u{e9}x\">",
            Ctx::Value => "\u{e9}x",
        }
    }
}

/// One entity body: a text declaration (or none), an encoding scheme, and
/// optionally a raw override for bodies that are not "declaration +
/// payload".
struct Body {
    name: &'static str,
    decl: &'static str,
    scheme: Scheme,
    raw: Option<fn(Ctx) -> Vec<u8>>,
}

const fn b(name: &'static str, decl: &'static str, scheme: Scheme) -> Body {
    Body {
        name,
        decl,
        scheme,
        raw: None,
    }
}

const fn raw(name: &'static str, f: fn(Ctx) -> Vec<u8>) -> Body {
    Body {
        name,
        decl: "",
        scheme: Scheme::Utf8,
        raw: Some(f),
    }
}

const D8: &str = "<?xml encoding=\"UTF-8\"?>";
const D8L: &str = "<?xml encoding=\"utf-8\"?>";
const D16: &str = "<?xml encoding=\"UTF-16\"?>";
const D16LE: &str = "<?xml encoding=\"UTF-16LE\"?>";
const D16BE: &str = "<?xml encoding=\"UTF-16BE\"?>";
const DLAT: &str = "<?xml encoding=\"ISO-8859-1\"?>";
const DASCII: &str = "<?xml encoding=\"US-ASCII\"?>";
const DUNK: &str = "<?xml encoding=\"x-unknown-enc\"?>";
const DEMPTY: &str = "<?xml encoding=\"\"?>";
const DVER: &str = "<?xml version=\"1.0\"?>";
const DVER8: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>";
const DSTAND: &str = "<?xml encoding=\"UTF-8\" standalone=\"yes\"?>";

fn bodies() -> Vec<Body> {
    use Scheme::{Be, BeBom, Latin1, Le, LeBom, Utf8, Utf8Bom};
    vec![
        b("u8", "", Utf8),
        b("u8-bom", "", Utf8Bom),
        b("u8-decl8", D8, Utf8),
        b("u8-decl8-lower", D8L, Utf8),
        b("u8-bom-decl8", D8, Utf8Bom),
        b("u8-ver8", DVER8, Utf8),
        b("u8-veronly", DVER, Utf8),
        b("u8-standalone", DSTAND, Utf8),
        b("u8-decl16", D16, Utf8),
        b("u8-decl16le", D16LE, Utf8),
        b("u8-bom-decl16le", D16LE, Utf8Bom),
        b("u8-bom-decllat", DLAT, Utf8Bom),
        b("u8-declascii", DASCII, Utf8),
        b("u8-declunknown", DUNK, Utf8),
        b("u8-declempty", DEMPTY, Utf8),
        b("lat-decllat", DLAT, Latin1),
        b("lat-nodecl", "", Latin1),
        b("le-bom", "", LeBom),
        b("be-bom", "", BeBom),
        b("le-nobom", "", Le),
        b("be-nobom", "", Be),
        b("le-bom-decl16", D16, LeBom),
        b("le-bom-decl16le", D16LE, LeBom),
        b("be-bom-decl16be", D16BE, BeBom),
        b("le-nobom-decl16le", D16LE, Le),
        b("be-nobom-decl16", D16, Be),
        b("le-bom-decl16be", D16BE, LeBom),
        b("be-bom-decl16le", D16LE, BeBom),
        b("le-bom-decl8", D8, LeBom),
        b("le-bom-decllat", DLAT, LeBom),
        b("le-bom-veronly", DVER, LeBom),
        b("le-bom-declunknown", DUNK, LeBom),
        raw("empty", |_| Vec::new()),
        raw("u8-bom-only", |_| vec![0xEF, 0xBB, 0xBF]),
        raw("le-bom-only", |_| vec![0xFF, 0xFE]),
        raw("le-bom-odd", |c| {
            let mut v = encode(Scheme::LeBom, c.payload());
            v.push(b'<');
            v
        }),
        raw("u8-misplaced-decl", |c| {
            let mut v = b"\n".to_vec();
            v.extend_from_slice(D8.as_bytes());
            v.extend_from_slice(c.payload().as_bytes());
            v
        }),
        // Character data first (G) / a comment first (S, P), UTF-16LE
        // without a BOM: the first two bytes are not `<` 00.
        raw("le-nobom-textfirst", |c| {
            let t = match c {
                Ctx::General => "\u{e9}<t/>".to_string(),
                _ => format!("\u{e9}{}", c.payload()),
            };
            encode(Scheme::Le, &t)
        }),
        raw("be-nobom-textfirst", |c| {
            let t = match c {
                Ctx::General => "\u{e9}<t/>".to_string(),
                _ => format!("\u{e9}{}", c.payload()),
            };
            encode(Scheme::Be, &t)
        }),
        // A UTF-8 BOM followed by UTF-16LE (BOM conflicts with content).
        raw("u8-bom-then-le", |c| {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend(encode(Scheme::Le, c.payload()));
            v
        }),
        // A child parse error after valid content (error position inside
        // the child).
        raw("u8-error-late", |c| {
            let t = match c {
                Ctx::General => "<t>ok</t>\n<u>&#0;</u>".to_string(),
                Ctx::Value => format!("{}\n&#0;", c.payload()),
                _ => format!("{}\n<!ENTITY y \"&#0;\">", c.payload()),
            };
            t.into_bytes()
        }),
        raw("le-bom-error-late", |c| {
            let t = match c {
                Ctx::General => "<t>ok</t>\n<u></v>".to_string(),
                Ctx::Value => format!("{}\n%undeclared;", c.payload()),
                _ => format!("{}\n<!ENTITY y >", c.payload()),
            };
            encode(Scheme::LeBom, &t)
        }),
    ]
}

impl Body {
    fn bytes(&self, ctx: Ctx) -> Vec<u8> {
        match self.raw {
            Some(f) => f(ctx),
            None => encode(self.scheme, &format!("{}{}", self.decl, ctx.payload())),
        }
    }
}

/// `encodingName` arguments of `XML_ExternalEntityParserCreate`.
const EXT_ENCODINGS: [Option<&str>; 10] = [
    None,
    Some("UTF-8"),
    Some("utf-8"),
    Some("ISO-8859-1"),
    Some("US-ASCII"),
    Some("UTF-16"),
    Some("UTF-16LE"),
    Some("UTF-16BE"),
    Some("x-unknown-enc"),
    Some(""),
];

/// The parent's protocol encoding (`XML_ParserCreate(encoding)`) with the
/// scheme its document is written in.
const PARENTS: [(Option<&str>, Scheme); 4] = [
    (None, Scheme::Utf8),
    (Some("ISO-8859-1"), Scheme::Latin1),
    (Some("UTF-16LE"), Scheme::Le),
    (None, Scheme::BeBom),
];

/// Feeding: chunk size and buffer API (the child is fed the same way).
const FEEDS: [(usize, bool); 3] = [(0, false), (1, false), (7, true)];

/// One side's result, recorded separately.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Outcome {
    status: String,
    code: i64,
    line: u64,
    column: u64,
    byte_index: i64,
    /// `extend <ok> <code> <line> <column> <index>` of every child parser.
    children: Vec<String>,
    /// The handler's return value for each `extref`, in order (1 OK, 0
    /// error): fixed by the mode for `Fail` / `Accept`, the child's status
    /// (or a failed creation / nesting cap) for `Read`.
    handler_returns: Vec<u8>,
    /// The full canonical trace (events and callbacks).
    trace: String,
}

impl Outcome {
    fn from_trace(trace: &[u8], o: &ParseOptions) -> Outcome {
        let mode = o.ext_handler;
        let keys: Vec<&[u8]> = o.ext_entities.iter().map(|(k, _)| k.as_slice()).collect();
        let text = String::from_utf8_lossy(trace).into_owned();
        let mut out = Outcome {
            status: String::new(),
            code: -1,
            line: 0,
            column: 0,
            byte_index: -1,
            children: Vec::new(),
            handler_returns: Vec::new(),
            trace: text.clone(),
        };
        // Handler calls that have not returned yet (nesting).
        let mut open: Vec<usize> = Vec::new();
        for l in text.lines() {
            let mut f = l.split(' ');
            match f.next() {
                Some("extref") => {
                    // Fields: context, base, system id, public id (plain
                    // ASCII identifiers in this file, written literally).
                    let read = mode == ExtHandler::Read
                        && f.nth(2).is_some_and(|id| keys.contains(&id.as_bytes()));
                    out.handler_returns.push(u8::from(matches!(
                        mode,
                        ExtHandler::Accept | ExtHandler::Read
                    )));
                    if read {
                        open.push(out.handler_returns.len() - 1);
                    }
                }
                Some("extend") => {
                    out.children.push(l.to_string());
                    let ok = f.next() == Some("1");
                    if let Some(i) = open.pop() {
                        out.handler_returns[i] = u8::from(ok);
                    }
                }
                Some("extdepth" | "extcreate") => {
                    if let Some(i) = open.pop() {
                        out.handler_returns[i] = 0;
                    }
                }
                Some("result") => {
                    let v: Vec<&str> = f.collect();
                    out.status = v.first().copied().unwrap_or_default().to_string();
                    out.code = v.get(1).and_then(|x| x.parse().ok()).unwrap_or(-1);
                    out.line = v.get(2).and_then(|x| x.parse().ok()).unwrap_or(0);
                    out.column = v.get(3).and_then(|x| x.parse().ok()).unwrap_or(0);
                    out.byte_index = v.get(4).and_then(|x| x.parse().ok()).unwrap_or(-1);
                }
                _ => {}
            }
        }
        out
    }

    fn short(&self) -> String {
        let kids: Vec<String> = self
            .children
            .iter()
            .map(|c| c.trim_start_matches("extend ").to_string())
            .collect();
        format!(
            "{} {} @{}:{}/{} child[{}] ret{:?}",
            self.status,
            self.code,
            self.line,
            self.column,
            self.byte_index,
            kids.join("; "),
            self.handler_returns
        )
    }
}

struct Run {
    pairs: usize,
    failures: Vec<String>,
}

impl Run {
    fn new() -> Self {
        Run {
            pairs: 0,
            failures: Vec::new(),
        }
    }

    /// Parses `doc` on both sides under `o`; records and compares.
    fn check(&mut self, id: &str, doc: &[u8], o: &ParseOptions) -> (Outcome, Outcome) {
        let c = Outcome::from_trace(&parse_trace(doc, o).expect("oracle"), o);
        let r = Outcome::from_trace(&port_trace(doc, o), o);
        self.pairs += 1;
        if c != r {
            let mut msg = format!("{id}\n  C:    {}\n  Rust: {}\n", c.short(), r.short());
            let cl: Vec<&str> = c.trace.lines().collect();
            let rl: Vec<&str> = r.trace.lines().collect();
            if let Some(i) = (0..cl.len().max(rl.len())).find(|&i| cl.get(i) != rl.get(i)) {
                let _ = writeln!(
                    msg,
                    "  first trace difference at line {}: C `{}` Rust `{}`",
                    i + 1,
                    cl.get(i).unwrap_or(&"<none>"),
                    rl.get(i).unwrap_or(&"<none>")
                );
            }
            let _ = write!(msg, "  options: {o:?}");
            self.failures.push(msg);
        }
        (c, r)
    }

    fn finish(&self, what: &str) {
        eprintln!(
            "{what}: {} pairs compared, {} differences",
            self.pairs,
            self.failures.len()
        );
        for f in self.failures.iter().take(20) {
            eprintln!("{f}");
        }
        assert!(
            self.failures.is_empty(),
            "{what}: {} differences between C and the port",
            self.failures.len()
        );
    }
}

fn options(
    ctx: Ctx,
    parent: Option<&str>,
    ext_encoding: Option<&str>,
    body: Vec<u8>,
    (chunk, buffer_api): (usize, bool),
) -> ParseOptions {
    ParseOptions {
        encoding: parent.map(str::to_string),
        ext_encoding: ext_encoding.map(str::to_string),
        ext_handler: ExtHandler::Read,
        ext_entities: ctx.entities(ext_encoding, body),
        param_entity_parsing: ParamEntityParsing::Always,
        chunk_size: chunk,
        buffer_api,
        ..ParseOptions::default()
    }
}

fn enc_name(e: Option<&str>) -> String {
    match e {
        None => "NULL".to_string(),
        Some(s) => format!("{s:?}"),
    }
}

fn parent_name(p: (Option<&str>, Scheme)) -> String {
    format!("{}/{:?}", enc_name(p.0), p.1)
}

/// Entity context x body x `encodingName` x parent protocol encoding x
/// feeding.
#[test]
fn encoding_matrix() {
    let mut run = Run::new();
    for ctx in Ctx::ALL {
        for parent in PARENTS {
            let doc = encode(parent.1, &ctx.document(false));
            for body in bodies() {
                let bytes = body.bytes(ctx);
                for ext in EXT_ENCODINGS {
                    for feed in FEEDS {
                        let id = format!(
                            "{} body={} ext={} parent={} feed={feed:?}",
                            ctx.name(),
                            body.name,
                            enc_name(ext),
                            parent_name(parent)
                        );
                        let o = options(ctx, parent.0, ext, bytes.clone(), feed);
                        run.check(&id, &doc, &o);
                    }
                }
            }
        }
    }
    run.finish("encoding_matrix");
}

/// Parser configuration: parameter entity parsing, namespaces (with and
/// without triplets), foreign DTD, standalone, the not-standalone handler
/// and every handler mode, for a representative set of bodies and
/// `encodingName` arguments.
#[test]
fn configuration_matrix() {
    let picked = [
        "u8",
        "u8-bom-decl8",
        "le-bom",
        "be-nobom",
        "le-bom-decl16be",
        "u8-decl16",
        "u8-declunknown",
        "u8-veronly",
        "lat-decllat",
        "u8-error-late",
    ];
    let all = bodies();
    let mut run = Run::new();
    for ctx in Ctx::ALL {
        for standalone in [false, true] {
            let doc = ctx.document(standalone).into_bytes();
            for body in all.iter().filter(|b| picked.contains(&b.name)) {
                let bytes = body.bytes(ctx);
                for ext in [
                    None,
                    Some("UTF-16LE"),
                    Some("ISO-8859-1"),
                    Some("x-unknown-enc"),
                ] {
                    for mode in [
                        ExtHandler::None,
                        ExtHandler::Fail,
                        ExtHandler::Accept,
                        ExtHandler::Read,
                    ] {
                        for pep in [
                            ParamEntityParsing::Never,
                            ParamEntityParsing::UnlessStandalone,
                            ParamEntityParsing::Always,
                        ] {
                            for (ns, triplets) in
                                [(None, false), (Some(b'|'), false), (Some(b'|'), true)]
                            {
                                for not_standalone in [None, Some(true), Some(false)] {
                                    for foreign in [false, true] {
                                        for feed in [(0, false), (1, false)] {
                                            let o = ParseOptions {
                                                ext_encoding: ext.map(str::to_string),
                                                ext_handler: mode,
                                                ext_entities: ctx.entities(ext, bytes.clone()),
                                                param_entity_parsing: pep,
                                                ns_separator: ns,
                                                ns_triplets: triplets,
                                                not_standalone,
                                                use_foreign_dtd: foreign,
                                                chunk_size: feed.0,
                                                buffer_api: feed.1,
                                                ..ParseOptions::default()
                                            };
                                            let id = format!(
                                                "{} standalone={standalone} body={} ext={} mode={mode:?} pep={pep:?} ns={ns:?}/{triplets} notstandalone={not_standalone:?} foreign={foreign} feed={feed:?}",
                                                ctx.name(),
                                                body.name,
                                                enc_name(ext)
                                            );
                                            run.check(&id, &doc, &o);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    // A document without a DOCTYPE under XML_UseForeignDTD: the handler is
    // called with a NULL system identifier (nothing to read).
    for ext in [None, Some("UTF-16LE")] {
        for mode in [ExtHandler::Fail, ExtHandler::Accept, ExtHandler::Read] {
            for pep in [ParamEntityParsing::Never, ParamEntityParsing::Always] {
                let o = ParseOptions {
                    ext_encoding: ext.map(str::to_string),
                    ext_handler: mode,
                    param_entity_parsing: pep,
                    use_foreign_dtd: true,
                    ..ParseOptions::default()
                };
                let id = format!(
                    "foreign-dtd-only ext={} mode={mode:?} pep={pep:?}",
                    enc_name(ext)
                );
                run.check(&id, b"<d>&x;</d>", &o);
            }
        }
    }
    run.finish("configuration_matrix");
}

/// Nested external entities: the `encodingName` argument applies to every
/// child, also a child of a child; a general entity inside a UTF-16 entity
/// and a parameter entity inside the external subset.
#[test]
fn nested_children() {
    let mut run = Run::new();
    let cases: Vec<(&str, &str, Entities)> = vec![
        (
            "G-in-G",
            "<!DOCTYPE d [<!ENTITY a SYSTEM \"a\"><!ENTITY b SYSTEM \"b\">]><d>&a;</d>",
            vec![
                (b"a".to_vec(), encode(Scheme::LeBom, "<x>&b;</x>")),
                (
                    b"b".to_vec(),
                    encode(Scheme::Utf8, "<?xml encoding=\"UTF-8\"?><y>\u{e9}</y>"),
                ),
            ],
        ),
        (
            "G-in-G-nobom",
            "<!DOCTYPE d [<!ENTITY a SYSTEM \"a\"><!ENTITY b SYSTEM \"b\">]><d>&a;</d>",
            vec![
                (b"a".to_vec(), encode(Scheme::Le, "<x>&b;</x>")),
                (b"b".to_vec(), encode(Scheme::Le, "<y>\u{e9}</y>")),
            ],
        ),
        (
            "P-in-S",
            "<!DOCTYPE d SYSTEM \"s\"><d>&x;</d>",
            vec![
                (
                    b"s".to_vec(),
                    encode(Scheme::BeBom, "<!ENTITY % q SYSTEM \"q\">%q;"),
                ),
                (
                    b"q".to_vec(),
                    encode(Scheme::Utf8, "<!ENTITY x \"\u{e9}\">"),
                ),
            ],
        ),
        (
            "error-in-grandchild",
            "<!DOCTYPE d [<!ENTITY a SYSTEM \"a\"><!ENTITY b SYSTEM \"b\">]><d>&a;</d>",
            vec![
                (b"a".to_vec(), encode(Scheme::Utf8, "<x>\n&b;</x>")),
                (
                    b"b".to_vec(),
                    encode(Scheme::LeBom, "<?xml encoding=\"UTF-8\"?><y/>"),
                ),
            ],
        ),
    ];
    for (id, doc, ents) in &cases {
        for ext in EXT_ENCODINGS {
            for feed in FEEDS {
                let o = ParseOptions {
                    ext_encoding: ext.map(str::to_string),
                    ext_handler: ExtHandler::Read,
                    ext_entities: ents.clone(),
                    param_entity_parsing: ParamEntityParsing::Always,
                    chunk_size: feed.0,
                    buffer_api: feed.1,
                    ..ParseOptions::default()
                };
                run.check(
                    &format!("{id} ext={} feed={feed:?}", enc_name(ext)),
                    doc.as_bytes(),
                    &o,
                );
            }
        }
    }
    run.finish("nested_children");
}

/// C's behaviour as docs/EXTERNAL-ENTITY-ENCODING.md states it, pinned
/// (C result first; the port is compared as everywhere else).
#[test]
fn c_behaviour_pins() {
    let all = bodies();
    let body = |n: &str| all.iter().find(|b| b.name == n).expect("body");
    let mut run = Run::new();
    // (context, body, encodingName, parent, expected C: result code, child
    // `extend` line or "" when no child is created)
    const P8: (Option<&str>, Scheme) = (None, Scheme::Utf8);
    const PLAT: (Option<&str>, Scheme) = (Some("ISO-8859-1"), Scheme::Latin1);
    let pins: [Pin; 24] = [
        // An external parameter entity read inside an entity value: its
        // text declaration goes through processXmlDecl(parser, 0, ...)
        // (entityValueInitProcessor), i.e. the XML declaration rules:
        // without version it is XML_ERROR_XML_DECL (30), version-only is
        // accepted. The first `extend` is the inner child (`ent`).
        (Ctx::Value, "u8-decl8", None, P8, 21, "extend 0 30 1 6 6"),
        (Ctx::Value, "u8-veronly", None, P8, 0, "extend 1 0 1 0 0"),
        // Unknown or empty encodingName: the child fails on its first
        // parse call with XML_ERROR_UNKNOWN_ENCODING, before any token
        // (byte index -1); the handler's error stops the parent with
        // XML_ERROR_EXTERNAL_ENTITY_HANDLING at the reference.
        (
            Ctx::General,
            "u8",
            Some("x-unknown-enc"),
            P8,
            21,
            "extend 0 18 1 0 -1",
        ),
        (Ctx::General, "u8", Some(""), P8, 21, "extend 0 18 1 0 -1"),
        (
            Ctx::Subset,
            "u8",
            Some("x-unknown-enc"),
            P8,
            21,
            "extend 0 18 1 0 -1",
        ),
        // Unknown name in the text declaration (no encodingName): error 18
        // at the name.
        (
            Ctx::General,
            "u8-declunknown",
            None,
            P8,
            21,
            "extend 0 18 1 16 16",
        ),
        // encodingName given: the text declaration's encoding is ignored.
        (
            Ctx::General,
            "u8-declunknown",
            Some("UTF-8"),
            P8,
            0,
            "extend 1 0 1 41 42",
        ),
        (
            Ctx::General,
            "u8-decl16",
            Some("UTF-8"),
            P8,
            0,
            "extend 1 0 1 34 35",
        ),
        // No encodingName: declaration inconsistent with the detected
        // encoding is XML_ERROR_INCORRECT_ENCODING.
        (
            Ctx::General,
            "u8-decl16",
            None,
            P8,
            21,
            "extend 0 19 1 16 16",
        ),
        (
            Ctx::General,
            "le-bom-decl16be",
            None,
            P8,
            21,
            "extend 0 19 1 17 34",
        ),
        (
            Ctx::General,
            "le-bom-decl8",
            None,
            P8,
            21,
            "extend 0 19 1 17 34",
        ),
        // Text declaration without encoding / with standalone / empty name.
        (
            Ctx::General,
            "u8-veronly",
            None,
            P8,
            21,
            "extend 0 31 1 19 19",
        ),
        (
            Ctx::General,
            "u8-standalone",
            None,
            P8,
            21,
            "extend 0 31 1 23 23",
        ),
        (
            Ctx::Subset,
            "u8-declempty",
            None,
            P8,
            21,
            "extend 0 31 1 16 16",
        ),
        // A UTF-16 BOM wins over an explicit UTF-8 / US-ASCII / other UTF-16
        // name; with ISO-8859-1 in content it is data.
        (
            Ctx::General,
            "le-bom",
            Some("UTF-8"),
            P8,
            0,
            "extend 1 0 1 10 20",
        ),
        (
            Ctx::General,
            "be-bom",
            Some("UTF-16LE"),
            P8,
            0,
            "extend 1 0 1 10 20",
        ),
        (
            Ctx::General,
            "le-bom",
            Some("ISO-8859-1"),
            P8,
            21,
            "extend 0 4 1 3 3",
        ),
        (
            Ctx::Subset,
            "le-bom",
            Some("ISO-8859-1"),
            P8,
            0,
            "extend 1 0 1 17 34",
        ),
        // The UTF-8 BOM is not looked for in content with an ISO-8859-1 or
        // UTF-16 name, but is in the prolog state (subset, PE).
        (
            Ctx::General,
            "u8-bom",
            Some("UTF-16LE"),
            P8,
            21,
            "extend 0 5 1 6 12",
        ),
        (
            Ctx::Subset,
            "u8-bom",
            Some("UTF-16LE"),
            P8,
            0,
            "extend 1 0 1 17 20",
        ),
        // BOM-less UTF-16LE starting with `<` is detected even with an
        // explicit UTF-8 name; starting with character data it is not.
        (
            Ctx::General,
            "le-nobom",
            Some("UTF-8"),
            P8,
            0,
            "extend 1 0 1 9 18",
        ),
        (
            Ctx::General,
            "le-nobom-textfirst",
            None,
            P8,
            21,
            "extend 0 4 1 0 0",
        ),
        // The parent's protocol encoding is not inherited: ISO-8859-1 text
        // in the child is still read as UTF-8.
        (
            Ctx::General,
            "lat-nodecl",
            None,
            PLAT,
            21,
            "extend 0 4 1 3 3",
        ),
        // A child error reports the child's own position.
        (
            Ctx::General,
            "u8-error-late",
            None,
            P8,
            21,
            "extend 0 14 2 3 13",
        ),
    ];
    for (ctx, b, ext, parent, code, child) in pins {
        let doc = encode(parent.1, &ctx.document(false));
        let o = options(ctx, parent.0, ext, body(b).bytes(ctx), (0, false));
        let id = format!("pin {} {b} ext={}", ctx.name(), enc_name(ext));
        let (c, _) = run.check(&id, &doc, &o);
        assert_eq!(c.code, code, "{id}: C result code");
        assert_eq!(
            c.children.first().map_or("", String::as_str),
            child,
            "{id}: C child"
        );
    }
    run.finish("c_behaviour_pins");
}

/// Writes the per-case table (whole-document feeding, parameter entity
/// parsing ALWAYS, no namespaces) to the file `EXT_ENC_MATRIX` names; a
/// no-op otherwise.
#[test]
fn matrix_dump() {
    let Some(path) = std::env::var_os("EXT_ENC_MATRIX") else {
        return;
    };
    let mut out = String::new();
    for ctx in Ctx::ALL {
        for parent in PARENTS {
            let doc = encode(parent.1, &ctx.document(false));
            for body in bodies() {
                for ext in EXT_ENCODINGS {
                    let o = options(ctx, parent.0, ext, body.bytes(ctx), (0, false));
                    let c = Outcome::from_trace(&parse_trace(&doc, &o).expect("oracle"), &o);
                    let r = Outcome::from_trace(&port_trace(&doc, &o), &o);
                    let _ = writeln!(
                        out,
                        "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                        ctx.name(),
                        parent_name(parent),
                        body.name,
                        enc_name(ext),
                        r.short(),
                        c.short(),
                        if r == c { "equal" } else { "DIFF" }
                    );
                }
            }
        }
    }
    // Configuration rows: body "u8", no encodingName, whole document.
    let u8body = bodies().into_iter().find(|b| b.name == "u8").expect("body");
    for ctx in Ctx::ALL {
        for standalone in [false, true] {
            let doc = ctx.document(standalone).into_bytes();
            for mode in [
                ExtHandler::None,
                ExtHandler::Fail,
                ExtHandler::Accept,
                ExtHandler::Read,
            ] {
                for pep in [
                    ParamEntityParsing::Never,
                    ParamEntityParsing::UnlessStandalone,
                    ParamEntityParsing::Always,
                ] {
                    for not_standalone in [None, Some(true), Some(false)] {
                        let o = ParseOptions {
                            ext_handler: mode,
                            ext_entities: ctx.entities(None, u8body.bytes(ctx)),
                            param_entity_parsing: pep,
                            not_standalone,
                            ..ParseOptions::default()
                        };
                        let c = Outcome::from_trace(&parse_trace(&doc, &o).expect("oracle"), &o);
                        let r = Outcome::from_trace(&port_trace(&doc, &o), &o);
                        let calls = c.trace.lines().filter(|l| l.starts_with("extref")).count();
                        let nsa = c
                            .trace
                            .lines()
                            .filter(|l| l.starts_with("notstandalone"))
                            .count();
                        let skipped = c.trace.lines().filter(|l| l.starts_with("skipped")).count();
                        let _ = writeln!(
                            out,
                            "CONFIG\t{}\tstandalone={standalone}\t{mode:?}\t{pep:?}\tnotstandalone={not_standalone:?}\textref={calls} notstandalone-calls={nsa} skipped={skipped}\t{}\t{}\t{}",
                            ctx.name(),
                            r.short(),
                            c.short(),
                            if r == c { "equal" } else { "DIFF" }
                        );
                    }
                }
            }
        }
    }
    std::fs::write(path, out).expect("write matrix");
}
