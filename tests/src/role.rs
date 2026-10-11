//! Differential checks of the prolog state machine (`expat::internal::role`,
//! module 8) against `xmlrole.c`, shared by the integration tests and the
//! `role_diff` fuzz target.
//!
//! A token sequence is fed to a fresh state on both sides; after every token
//! the role and the whole state (handler, `level`, `role_none`,
//! `includeLevel`, `documentEntity`) must be equal. Sequences come from
//! tokenizing documents with the prolog scanner, the way the parser feeds
//! the machine, and from synthetic token lists that reach every handler with
//! every token kind, keyword or not.

use expat::internal::role::{Handler, PrologState, Role};
use expat::internal::tok::{self as rs, Encoding, Scan, Tok};
use expat_oracle::tok::{self as c, RoleToken};

use crate::tok::Subject;

/// The C value of a Rust handler state (its index in declaration order).
fn handler_index(h: Handler) -> usize {
    Handler::ALL
        .iter()
        .position(|&x| x == h)
        .expect("listed handler")
}

/// The C view of a Rust state.
pub fn c_state(st: &PrologState) -> c::RoleState {
    c::RoleState {
        handler: handler_index(st.handler),
        level: st.level,
        role_none: st.role_none as i32,
        include_level: st.include_level,
        document_entity: st.document_entity,
    }
}

/// Feeds `toks` (tokens of `buf`) to both machines and compares role and
/// state after each token.
pub fn check_roles<S: Subject>(s: &S, external: bool, buf: &[u8], toks: &[(Scan, usize, usize)]) {
    let mut st = PrologState::new();
    if external {
        st.init_external_entity();
    }
    check_roles_from(s, external, None, st, buf, toks);
}

/// [`check_roles`] from the state `start` (given to C as `init`, or `None`
/// for C's own initialisation).
pub fn check_roles_from<S: Subject>(
    s: &S,
    external: bool,
    init: Option<c::RoleState>,
    mut st: PrologState,
    buf: &[u8],
    toks: &[(Scan, usize, usize)],
) {
    let ctoks: Vec<RoleToken> = toks
        .iter()
        .map(|&(t, ptr, end)| RoleToken {
            tok: t.to_c(),
            ptr,
            end,
        })
        .collect();
    let expected = c::role_run(external, init, s.c(), buf, &ctoks);
    for (i, (&(t, ptr, end), (crole, cstate))) in toks.iter().zip(expected).enumerate() {
        let role = st.token_role(t, s.rs(), buf, ptr, end);
        let state = c_state(&st);
        assert_eq!(
            (role as i32, state),
            (crole, cstate),
            "{s:?} external {external}: token {i} {t:?} {:?} of {toks:?} in {buf:?}",
            buf.get(ptr..end)
        );
    }
}

/// The tokens of `buf` in the prolog state, from offset 0 until the first
/// result that is not a complete token, followed by that result (as the
/// parser would pass `XML_TOK_NONE` and errors are passed through as-is to
/// exercise the default cases).
pub fn prolog_tokens<E: Encoding>(enc: &E, buf: &[u8]) -> Vec<(Scan, usize, usize)> {
    let mut out = Vec::new();
    let mut ptr = 0;
    loop {
        let r = rs::prolog_tok(enc, buf, ptr);
        match (r.scan, r.next) {
            (Scan::Token(_), Some(next)) if next > ptr => {
                out.push((r.scan, ptr, next));
                ptr = next;
            }
            _ => {
                out.push((r.scan, ptr, buf.len()));
                return out;
            }
        }
    }
}

/// Compares the role sequence of `buf` tokenized in the prolog state, as a
/// document entity and as an external entity.
pub fn check_document<S: Subject>(s: &S, buf: &[u8]) {
    let toks = prolog_tokens(s.rs(), buf);
    check_roles(s, false, buf, &toks);
    check_roles(s, true, buf, &toks);
}

/// Every `Scan` value the handlers can be given.
pub fn all_scans() -> Vec<Scan> {
    let mut v: Vec<Scan> = (1..=42)
        .filter_map(tok_from_c)
        .map(Scan::Token)
        .collect();
    v.extend([
        Scan::Invalid,
        Scan::Partial,
        Scan::PartialChar,
        Scan::TrailingCr,
        Scan::None,
        Scan::TrailingRsqb,
        Scan::PartialToken(Tok::PrologS),
    ]);
    v
}

fn tok_from_c(v: i32) -> Option<Tok> {
    use Tok as T;
    const ALL: [Tok; 42] = [
        T::StartTagWithAtts,
        T::StartTagNoAtts,
        T::EmptyElementWithAtts,
        T::EmptyElementNoAtts,
        T::EndTag,
        T::DataChars,
        T::DataNewline,
        T::CdataSectOpen,
        T::EntityRef,
        T::CharRef,
        T::Pi,
        T::XmlDecl,
        T::Comment,
        T::Bom,
        T::PrologS,
        T::DeclOpen,
        T::DeclClose,
        T::Name,
        T::Nmtoken,
        T::PoundName,
        T::Or,
        T::Percent,
        T::OpenParen,
        T::CloseParen,
        T::OpenBracket,
        T::CloseBracket,
        T::Literal,
        T::ParamEntityRef,
        T::InstanceStart,
        T::NameQuestion,
        T::NameAsterisk,
        T::NamePlus,
        T::CondSectOpen,
        T::CondSectClose,
        T::CloseParenQuestion,
        T::CloseParenAsterisk,
        T::CloseParenPlus,
        T::Comma,
        T::AttributeValueS,
        T::CdataSectClose,
        T::PrefixedName,
        T::IgnoreSect,
    ];
    ALL.into_iter().find(|&t| t as i32 == v)
}

/// Token texts that the handlers compare against keywords, with the prefix
/// characters of `<!` declarations and `#` names, plus near misses.
pub const TEXTS: [&str; 30] = [
    "<!DOCTYPE",
    "<!ENTITY",
    "<!ATTLIST",
    "<!ELEMENT",
    "<!NOTATION",
    "<!doctype",
    "SYSTEM",
    "PUBLIC",
    "NDATA",
    "CDATA",
    "ID",
    "IDREF",
    "IDREFS",
    "ENTITY",
    "ENTITIES",
    "NMTOKEN",
    "NMTOKENS",
    "NOTATION",
    "EMPTY",
    "ANY",
    "INCLUDE",
    "IGNORE",
    "#IMPLIED",
    "#REQUIRED",
    "#FIXED",
    "#PCDATA",
    "SYSTEMX",
    "I",
    "",
    "x",
];

/// Encodes ASCII `text` in the encoding of `s` (one unit per character).
pub fn encode_ascii<S: Subject>(s: &S, text: &str) -> Vec<u8> {
    let probe = s.rs().byte_to_ascii(&[b'a', 0], 0) == i32::from(b'a');
    let little = S::E::MINBPC == 2 && probe;
    text.bytes()
        .flat_map(|b| match (S::E::MINBPC, little) {
            (1, _) => vec![b],
            (_, true) => vec![b, 0],
            _ => vec![0, b],
        })
        .collect()
}

/// Builds a synthetic token sequence: `(scan index, text index)` pairs into
/// [`all_scans`] and [`TEXTS`], laid out one after another in a buffer.
pub fn synthetic<S: Subject>(s: &S, picks: &[(usize, usize)]) -> (Vec<u8>, Vec<(Scan, usize, usize)>) {
    let scans = all_scans();
    let mut buf = Vec::new();
    let mut toks = Vec::new();
    for &(si, ti) in picks {
        let scan = scans[si % scans.len()];
        let text = encode_ascii(s, TEXTS[ti % TEXTS.len()]);
        let ptr = buf.len();
        buf.extend(&text);
        toks.push((scan, ptr, buf.len()));
    }
    (buf, toks)
}

/// Every handler x every token kind x every keyword text, from states with
/// each combination of `level` (1, 2), `includeLevel` (0, 1),
/// `documentEntity` and `role_none`: one step each.
pub fn check_all_single_steps<S: Subject>(s: &S) {
    let scans = all_scans();
    for h in Handler::ALL {
        for level in [1, 2] {
            for include_level in [0, 1] {
                for document_entity in [false, true] {
                    for role_none in [Role::EntityNone, Role::ElementNone] {
                        let st = PrologState {
                            handler: h,
                            level,
                            role_none,
                            include_level,
                            document_entity,
                            in_entity_value: false,
                        };
                        for &scan in &scans {
                            for text in TEXTS {
                                let buf = encode_ascii(s, text);
                                let toks = [(scan, 0, buf.len())];
                                check_roles_from(s, false, Some(c_state(&st)), st, &buf, &toks);
                            }
                        }
                    }
                }
            }
        }
    }
}
