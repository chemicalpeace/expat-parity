//! The prolog: `prologInitProcessor`, `prologProcessor`, `doProlog` (the
//! roles of a prolog without a document type declaration),
//! `processXmlDecl`, and the implicit namespace context (`setContext` with
//! `implicitContext`).

use alloc::vec::Vec;

use super::accounting::Account;
use super::dtd::PrefixId;
use super::error::{Stop, XmlError};
use super::{Enc, Handler, ParamEntityParsing, Parser, Parsing, Processor};
use crate::role::Role;
use crate::tok::{
    Encoding, FoundEncoding, InitEncoding, Scan, State, Tok, init_scan, name_length,
    parse_xml_decl, prolog_tok,
};

/// The URI of the `xml` prefix (`implicitContext`, `xmlNamespace`).
pub(crate) const XML_NAMESPACE: &[u8] = b"http://www.w3.org/XML/1998/namespace";

/// Calls `$body` with `$e` bound to the encoding inside a [`Known`], so a
/// generic function is instantiated per encoding family.
macro_rules! with_known {
    ($k:expr, $e:ident => $body:expr) => {
        match $k {
            $crate::tok::Known::Normal(e) => {
                let $e = &e;
                $body
            }
            $crate::tok::Known::Utf16(e) => {
                let $e = &e;
                $body
            }
        }
    };
}
pub(crate) use with_known;

/// How `doProlog` ended.
pub(crate) enum PrologDone {
    /// It returned (C: `return XML_ERROR_NONE` with `*nextPtr` set).
    Done,
    /// The XML declaration changed the encoding; C's loop continues with
    /// the next token, scanned in the new encoding, at this offset.
    Rescan(usize),
}

/// Converts `from` to UTF-8 completely (`poolStoreString` without the
/// terminating NUL). Stops early only where C does: at an incomplete
/// character, which the tokenizer never lets through.
pub(crate) fn to_utf8<E: Encoding>(enc: &E, from: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(from.len());
    let mut chunk = [0u8; 64];
    let mut rest = from;
    loop {
        let r = enc.to_utf8(rest, &mut chunk);
        out.extend_from_slice(chunk.get(..r.written).unwrap_or_default());
        rest = rest.get(r.read..).unwrap_or_default();
        if r.result != crate::tok::ConvertResult::OutputExhausted || (r.read == 0 && r.written == 0)
        {
            return out;
        }
    }
}

impl Parser {
    /// `initializeEncoding`: the initial encoding for the name the
    /// application gave. An unknown name goes to `handleUnknownEncoding`,
    /// which without an unknown-encoding handler (module 12) fails.
    fn initialize_encoding(&mut self) -> Result<(), Stop> {
        match InitEncoding::new(self.protocol_encoding_name.as_deref(), self.ns) {
            Some(init) => {
                self.enc = Enc::Init(init);
                Ok(())
            }
            None => Err(XmlError::UnknownEncoding.into()),
        }
    }

    /// `prologInitProcessor`.
    pub(crate) fn prolog_init_processor<H: Handler>(
        &mut self,
        h: &mut H,
        buf: &[u8],
        s: usize,
        next_ptr: &mut usize,
    ) -> Result<(), Stop> {
        self.initialize_encoding()?;
        self.processor = Processor::Prolog;
        self.prolog_processor(h, buf, s, next_ptr)
    }

    /// `prologProcessor`: scans one token (detecting the encoding first if
    /// necessary) and hands it to `doProlog`.
    pub(crate) fn prolog_processor<H: Handler>(
        &mut self,
        h: &mut H,
        buf: &[u8],
        s: usize,
        next_ptr: &mut usize,
    ) -> Result<(), Stop> {
        let mut s = s;
        loop {
            let (k, scanned) = match self.enc {
                Enc::Init(init) => {
                    let r = init_scan(&init, State::Prolog, buf, s);
                    match r.encoding {
                        Some(k) => {
                            self.enc = Enc::Known(k);
                            (k, r.scanned)
                        }
                        None => return self.prolog_undecided(r.scanned.scan, s, next_ptr),
                    }
                }
                Enc::Known(k) => (k, k.tok(State::Prolog, buf, s)),
            };
            let next = scanned.next.unwrap_or(s);
            let have_more = !self.final_buffer;
            let done = with_known!(k, e => self.do_prolog(h, e, buf, s, scanned.scan, next, next_ptr, have_more))?;
            match done {
                PrologDone::Done => return Ok(()),
                PrologDone::Rescan(at) => s = at,
            }
        }
    }

    /// `doProlog` for the first scan while the encoding is still
    /// undecided: `initScan` then returns only `XML_TOK_NONE` or
    /// `XML_TOK_PARTIAL`, which `doProlog` handles before it uses the
    /// encoding.
    fn prolog_undecided(&mut self, tok: Scan, s: usize, next_ptr: &mut usize) -> Result<(), Stop> {
        self.event_ptr = Some(s);
        self.event_end_ptr = Some(s);
        if !self.final_buffer && tok != Scan::Invalid {
            *next_ptr = s;
            return Ok(());
        }
        Err(match tok {
            Scan::Invalid => XmlError::InvalidToken,
            Scan::PartialChar => XmlError::PartialChar,
            Scan::None => XmlError::NoElements,
            _ => XmlError::UnclosedToken,
        }
        .into())
    }

    /// `doProlog` (document entity, `XML_ACCOUNT_DIRECT`): handles `tok`
    /// (`buf[s..next]`) and the following tokens.
    #[allow(clippy::too_many_arguments)] // reason: mirrors the C signature of doProlog.
    fn do_prolog<E: Encoding, H: Handler>(
        &mut self,
        h: &mut H,
        enc: &E,
        buf: &[u8],
        s: usize,
        tok: Scan,
        next: usize,
        next_ptr: &mut usize,
        have_more: bool,
    ) -> Result<PrologDone, Stop> {
        let end = buf.len();
        let (mut s, mut tok, mut next) = (s, tok, next);
        loop {
            self.event_ptr = Some(s);
            self.event_end_ptr = Some(next);
            let t = match tok {
                Scan::Token(t) => t,
                _ => {
                    if have_more && tok != Scan::Invalid {
                        *next_ptr = s;
                        return Ok(PrologDone::Done);
                    }
                    match tok {
                        Scan::Invalid => {
                            self.event_ptr = Some(next);
                            return Err(XmlError::InvalidToken.into());
                        }
                        Scan::Partial => return Err(XmlError::UnclosedToken.into()),
                        Scan::PartialChar => return Err(XmlError::PartialChar.into()),
                        Scan::PartialToken(Tok::PrologS) => Tok::PrologS,
                        // A document entity without open internal entities.
                        Scan::None => return Err(XmlError::NoElements.into()),
                        Scan::PartialToken(t) => {
                            next = end;
                            t
                        }
                        // C negates any other negative value; the prolog
                        // scanner returns neither of these, and negating
                        // gives XML_TOK_EMPTY_ELEMENT_WITH_ATTS (3) and
                        // XML_TOK_END_TAG (5).
                        Scan::TrailingCr => {
                            next = end;
                            Tok::EmptyElementWithAtts
                        }
                        Scan::TrailingRsqb => {
                            next = end;
                            Tok::EndTag
                        }
                        Scan::Token(t) => t,
                    }
                }
            };
            let role = self
                .prolog_state
                .token_role(Scan::Token(t), enc, buf, s, next);
            if !matches!(role, Role::InstanceStart | Role::XmlDecl | Role::TextDecl)
                && !self
                    .accounting
                    .diff_tolerated(Scan::Token(t), next - s, Account::Direct)
            {
                return Err(XmlError::AmplificationLimitBreach.into());
            }
            let mut rescan = false;
            match role {
                Role::XmlDecl => rescan = self.process_xml_decl(h, enc, buf, s, next)?,
                Role::InstanceStart => {
                    self.processor = Processor::Content;
                    self.content_processor(h, buf, s, next_ptr)?;
                    return Ok(PrologDone::Done);
                }
                Role::Pi => self.report_processing_instruction(h, enc, buf, s, next),
                Role::Comment => self.report_comment(h, enc, buf, s, next),
                Role::None | Role::DoctypeNone => {}
                Role::Error => {
                    return Err(match t {
                        // PE references in internal subset are not allowed
                        // within declarations.
                        Tok::ParamEntityRef => XmlError::ParamEntityRef,
                        Tok::XmlDecl => XmlError::MisplacedXmlPi,
                        _ => XmlError::Syntax,
                    }
                    .into());
                }
                // Every other role belongs to a document type declaration.
                _ => self.dtd_role(h, enc, buf, role, t, s, next, true)?,
            }
            match self.parsing {
                Parsing::Suspended => {
                    *next_ptr = next;
                    return Ok(PrologDone::Done);
                }
                Parsing::Finished => return Err(XmlError::Aborted.into()),
                Parsing::Parsing if self.reenter => {
                    *next_ptr = next;
                    return Ok(PrologDone::Done);
                }
                Parsing::Parsing | Parsing::Initialized => {}
            }
            if rescan {
                return Ok(PrologDone::Rescan(next));
            }
            s = next;
            let r = prolog_tok(enc, buf, s);
            tok = r.scan;
            if let Some(n) = r.next {
                next = n;
            }
        }
    }

    /// `processXmlDecl(parser, 0, s, next)` for the document entity.
    /// Returns whether the declaration switched the parser to another
    /// encoding.
    fn process_xml_decl<E: Encoding, H: Handler>(
        &mut self,
        h: &mut H,
        enc: &E,
        buf: &[u8],
        s: usize,
        next: usize,
    ) -> Result<bool, Stop> {
        if !self
            .accounting
            .diff_tolerated(Scan::Token(Tok::XmlDecl), next - s, Account::Direct)
        {
            return Err(XmlError::AmplificationLimitBreach.into());
        }
        let d = parse_xml_decl(false, enc, self.ns, buf, s, next);
        if !d.ok {
            if d.bad.is_some() {
                self.event_ptr = d.bad;
            }
            return Err(XmlError::XmlDecl.into());
        }
        let standalone = match d.standalone {
            None => -1,
            Some(false) => 0,
            Some(true) => 1,
        };
        if standalone == 1 {
            self.dtd.standalone = true;
            if self.param_entity_parsing == ParamEntityParsing::UnlessStandalone {
                self.param_entity_parsing = ParamEntityParsing::Never;
            }
        }
        if self.handlers.xml_decl {
            let enc_name = d.encoding_name.map(|p| {
                let n = name_length(enc, buf, p);
                to_utf8(enc, buf.get(p..p + n).unwrap_or_default())
            });
            let version = match (d.version, d.version_end) {
                (Some(v), Some(ve)) => Some(to_utf8(
                    enc,
                    buf.get(v..ve.saturating_sub(E::MINBPC)).unwrap_or_default(),
                )),
                _ => None,
            };
            h.xml_decl(version.as_deref(), enc_name.as_deref(), standalone);
        }
        if self.protocol_encoding_name.is_some() {
            return Ok(false);
        }
        let Enc::Known(cur) = self.enc else {
            return Ok(false);
        };
        match d.encoding {
            Some(FoundEncoding::Known(new)) => {
                let m = new.min_bytes_per_char();
                if m != cur.min_bytes_per_char() || (m == 2 && new != cur) {
                    self.event_ptr = d.encoding_name;
                    return Err(XmlError::IncorrectEncoding.into());
                }
                self.enc = Enc::Known(new);
                Ok(new != cur)
            }
            // `findEncoding` returned the parser's own encoding.
            Some(FoundEncoding::Input) => Ok(false),
            Some(FoundEncoding::Unrecognized) | None => {
                if d.encoding_name.is_some() {
                    // handleUnknownEncoding without a handler (module 12).
                    self.event_ptr = d.encoding_name;
                    return Err(XmlError::UnknownEncoding.into());
                }
                Ok(false)
            }
        }
    }

    /// `setContext(parser, implicitContext)`: binds the `xml` prefix.
    pub(crate) fn set_implicit_context<H: Handler>(&mut self, h: &mut H) -> bool {
        let salt = self.salt;
        let Ok(prefix) = self.dtd.lookup_prefix(&salt, b"xml") else {
            return false;
        };
        let mut inherited = self.inherited_bindings;
        let ok = self
            .add_binding(h, prefix, None, XML_NAMESPACE, &mut inherited)
            .is_ok();
        self.inherited_bindings = inherited;
        debug_assert!(matches!(prefix, PrefixId::Named(_)));
        ok
    }
}
