//! Content: `contentProcessor`, `doContent`, `storeAtts`, `addBinding`,
//! `freeBindings`, attribute values (`storeAttributeValue`,
//! `appendAttributeValue`), CDATA sections, the epilog, and the reports of
//! processing instructions and comments.

use alloc::vec::Vec;
use core::ffi::c_ulong;

use super::accounting::Account;
use super::dtd::{Binding, BindingId, EntityRef, PrefixId};
use super::error::{Stop, XmlError};
use super::entity::EntityType;
use super::prolog::{to_utf8, with_known};
use super::tracker::{ATTRIBUTE_SIZE, Pool};
use super::{Enc, Handler, INIT_ATTS_SIZE, INIT_DATA_BUF_SIZE, Parser, Parsing, Processor, Tag};
use crate::encode::utf8_encode;
use crate::hash::EntryId;
use crate::siphash::siphash24;
use crate::tok::{
    Attribute, ConvertResult, Encoding, Scan, Tok, attribute_value_tok, cdata_section_tok,
    char_ref_number, content_tok, get_atts, name_length, predefined_entity_name, prolog_tok,
    skip_s,
};

/// `"http://www.w3.org/2000/xmlns/"` (`xmlnsNamespace`).
const XMLNS_NAMESPACE: &[u8] = b"http://www.w3.org/2000/xmlns/";

/// `is_rfc3986_uri_char`: may `c` appear in a URI (RFC 3986 appendix A)?
fn is_rfc3986_uri_char(c: u8) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            b'%' | b'-'
                | b'.'
                | b'_'
                | b'~'
                | b':'
                | b'/'
                | b'?'
                | b'#'
                | b'['
                | b']'
                | b'@'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
        )
}

/// `normalizeLines`: CR LF and lone CR become LF.
fn normalize_lines(s: &mut Vec<u8>) {
    if !s.contains(&0xD) {
        return;
    }
    let mut out = Vec::with_capacity(s.len());
    let mut it = s.iter().copied().peekable();
    while let Some(c) = it.next() {
        if c == 0xD {
            out.push(0xA);
            if it.peek() == Some(&0xA) {
                it.next();
            }
        } else {
            out.push(c);
        }
    }
    *s = out;
}

/// The bytes `buf[from..to]` (empty if out of range).
fn bytes(buf: &[u8], from: usize, to: usize) -> &[u8] {
    buf.get(from..to).unwrap_or_default()
}

/// One attribute of a start tag while `storeAtts` builds the list.
struct AppAtt {
    name: Vec<u8>,
    value: Vec<u8>,
    id: EntryId,
}

/// Whether character data comes from content or from a CDATA section
/// (they differ in where `m_eventEndPtr` points while it is converted).
#[derive(Clone, Copy, PartialEq, Eq)]
enum DataIn {
    Content,
    Cdata,
}

impl Parser {
    /// `contentProcessor` (`storeRawNames` is not needed: open tags keep a
    /// copy of their raw name from the start).
    pub(crate) fn content_processor<H: Handler>(
        &mut self,
        h: &mut H,
        buf: &[u8],
        s: usize,
        next_ptr: &mut usize,
    ) -> Result<(), Stop> {
        // The document entity reaches content only through a token scanned
        // in a detected encoding; an initial encoding here would need
        // `initScanContent`, which only external entities use (module 12).
        let Enc::Known(k) = self.enc else {
            return Err(XmlError::UnexpectedState.into());
        };
        let have_more = !self.final_buffer;
        with_known!(k, e => self.do_content(h, e, 0, buf, s, next_ptr, have_more, Account::Direct))
    }

    /// `cdataSectionProcessor`.
    pub(crate) fn cdata_section_processor<H: Handler>(
        &mut self,
        h: &mut H,
        buf: &[u8],
        s: usize,
        next_ptr: &mut usize,
    ) -> Result<(), Stop> {
        let Enc::Known(k) = self.enc else {
            return Err(XmlError::UnexpectedState.into());
        };
        let have_more = !self.final_buffer;
        let mut start = Some(s);
        with_known!(k, e => self.do_cdata_section(h, e, buf, &mut start, next_ptr, have_more, Account::Direct))?;
        if let Some(s) = start {
            self.processor = Processor::Content;
            return self.content_processor(h, buf, s, next_ptr);
        }
        Ok(())
    }

    /// `epilogProcessor`.
    pub(crate) fn epilog_processor<H: Handler>(
        &mut self,
        h: &mut H,
        buf: &[u8],
        s: usize,
        next_ptr: &mut usize,
    ) -> Result<(), Stop> {
        let Enc::Known(k) = self.enc else {
            return Err(XmlError::UnexpectedState.into());
        };
        with_known!(k, e => self.do_epilog(h, e, buf, s, next_ptr))
    }

    fn do_epilog<E: Encoding, H: Handler>(
        &mut self,
        h: &mut H,
        enc: &E,
        buf: &[u8],
        s: usize,
        next_ptr: &mut usize,
    ) -> Result<(), Stop> {
        self.processor = Processor::Epilog;
        self.event_ptr = Some(s);
        let mut s = s;
        loop {
            let r = prolog_tok(enc, buf, s);
            // C starts from `next = NULL`; a scanner that does not write it
            // only returns tokens that do not use it.
            let next = r.next.unwrap_or(s);
            if !self
                .accounting
                .diff_tolerated(r.scan, next.saturating_sub(s), Account::Direct)
            {
                return Err(XmlError::AmplificationLimitBreach.into());
            }
            self.event_end_ptr = r.next;
            match r.scan {
                // Report partial linebreak - it might be the last token.
                Scan::PartialToken(Tok::PrologS) => {
                    *next_ptr = next;
                    return Ok(());
                }
                Scan::None => {
                    *next_ptr = s;
                    return Ok(());
                }
                Scan::Token(Tok::PrologS) => {}
                Scan::Token(Tok::Pi) => self.report_processing_instruction(h, enc, buf, s, next)?,
                Scan::Token(Tok::Comment) => self.report_comment(h, enc, buf, s, next)?,
                Scan::Invalid => {
                    self.event_ptr = Some(next);
                    return Err(XmlError::InvalidToken.into());
                }
                Scan::Partial => {
                    if !self.final_buffer {
                        *next_ptr = s;
                        return Ok(());
                    }
                    return Err(XmlError::UnclosedToken.into());
                }
                Scan::PartialChar => {
                    if !self.final_buffer {
                        *next_ptr = s;
                        return Ok(());
                    }
                    return Err(XmlError::PartialChar.into());
                }
                _ => return Err(XmlError::JunkAfterDocElement.into()),
            }
            match self.parsing {
                Parsing::Suspended => {
                    self.event_ptr = Some(next);
                    *next_ptr = next;
                    return Ok(());
                }
                Parsing::Finished => {
                    self.event_ptr = Some(next);
                    return Err(XmlError::Aborted.into());
                }
                Parsing::Parsing if self.reenter => return Err(XmlError::UnexpectedState.into()),
                Parsing::Parsing | Parsing::Initialized => {
                    s = next;
                    self.event_ptr = Some(s);
                }
            }
        }
    }

    /// Reports `buf[s..next]` as character data, converted to UTF-8 through
    /// the 1024-byte data buffer when the encoding is not UTF-8.
    fn report_chars<E: Encoding, H: Handler>(
        &mut self,
        h: &mut H,
        enc: &E,
        buf: &[u8],
        s: usize,
        next: usize,
        from: DataIn,
    ) {
        if !self.handlers.character_data {
            return;
        }
        if enc.is_utf8() {
            h.character_data(bytes(buf, s, next));
            return;
        }
        let mut s = s;
        let mut data = [0u8; INIT_DATA_BUF_SIZE];
        loop {
            let r = enc.to_utf8(bytes(buf, s, next), &mut data);
            s += r.read;
            let end_at = match from {
                DataIn::Content => s,
                DataIn::Cdata => next,
            };
            self.ev_end(enc, Some(end_at));
            h.character_data(data.get(..r.written).unwrap_or_default());
            if matches!(
                r.result,
                ConvertResult::Completed | ConvertResult::InputIncomplete
            ) {
                return;
            }
            self.ev(enc, Some(s));
        }
    }

    /// `doContent` (document entity: `startTagLevel` 0).
    #[allow(clippy::too_many_arguments)] // reason: mirrors the C signature of doContent.
    pub(crate) fn do_content<E: Encoding, H: Handler>(
        &mut self,
        h: &mut H,
        enc: &E,
        start_tag_level: i32,
        buf: &[u8],
        s: usize,
        next_ptr: &mut usize,
        have_more: bool,
        account: Account,
    ) -> Result<(), Stop> {
        let end = buf.len();
        let m = E::MINBPC;
        let mut s = s;
        self.ev(enc, Some(s));
        loop {
            let r = content_tok(enc, buf, s);
            let tok = r.scan;
            // XmlContentTok doesn't always set the last arg.
            let mut next = r.next.unwrap_or(s);
            let account_after = match tok {
                Scan::TrailingRsqb | Scan::TrailingCr => {
                    if have_more {
                        s
                    } else {
                        end
                    }
                }
                _ => next,
            };
            if !self
                .accounting
                .diff_tolerated(tok, account_after.saturating_sub(s), account)
            {
                return Err(XmlError::AmplificationLimitBreach.into());
            }
            self.ev_end(enc, Some(next));
            match tok {
                Scan::TrailingCr => {
                    if have_more {
                        *next_ptr = s;
                        return Ok(());
                    }
                    self.ev_end(enc, Some(end));
                    if self.handlers.character_data {
                        h.character_data(b"\n");
                    }
                    if start_tag_level == 0 {
                        return Err(XmlError::NoElements.into());
                    }
                    if self.tag_level != start_tag_level {
                        return Err(XmlError::AsyncEntity.into());
                    }
                    *next_ptr = end;
                    return Ok(());
                }
                Scan::None => {
                    if have_more {
                        *next_ptr = s;
                        return Ok(());
                    }
                    if start_tag_level > 0 {
                        if self.tag_level != start_tag_level {
                            return Err(XmlError::AsyncEntity.into());
                        }
                        *next_ptr = s;
                        return Ok(());
                    }
                    return Err(XmlError::NoElements.into());
                }
                Scan::Invalid => {
                    self.ev(enc, Some(next));
                    return Err(XmlError::InvalidToken.into());
                }
                Scan::Partial => {
                    if have_more {
                        *next_ptr = s;
                        return Ok(());
                    }
                    return Err(XmlError::UnclosedToken.into());
                }
                Scan::PartialChar => {
                    if have_more {
                        *next_ptr = s;
                        return Ok(());
                    }
                    return Err(XmlError::PartialChar.into());
                }
                Scan::Token(Tok::EntityRef) => {
                    if let Some(ch) = predefined_entity_name(enc, bytes(buf, 0, next - m), s + m) {
                        // Replacing 4-6 characters of input by 1 is no
                        // amplification, so C ignores the verdict.
                        let _ = self.accounting.diff_tolerated(
                            tok,
                            1,
                            Account::EntityExpansion,
                        );
                        if self.handlers.character_data {
                            h.character_data(&[ch]);
                        }
                    } else {
                        let name = to_utf8(enc, bytes(buf, s + m, next - m));
                        // The name is stored in `dtd->pool` for the lookup.
                        let direct = self.direct();
                        self.tracker
                            .store_discard(Pool::Dtd, name.len(), direct)
                            .map_err(|_| Stop::Error(XmlError::NoMemory))?;
                        let entity = self.dtd.general_entities.find(&self.salt, &name);
                        // First, determine if a check for an existing
                        // declaration is needed; if yes, check that the
                        // entity exists, and that it is internal,
                        // otherwise call the skipped entity handler.
                        let check = !self.dtd.has_param_entity_refs || self.dtd.standalone;
                        match entity {
                            None if check => return Err(XmlError::UndefinedEntity.into()),
                            None => {
                                if self.handlers.skipped_entity {
                                    h.skipped_entity(&name, false);
                                }
                            }
                            Some(id) => {
                                let r = EntityRef::General(id);
                                let (is_internal, open, notation, text) = match self.dtd.entity(r) {
                                    Some(e) => (e.is_internal, e.open, e.notation.is_some(), e.text.is_some()),
                                    None => (true, false, false, false),
                                };
                                if check && !is_internal {
                                    return Err(XmlError::EntityDeclaredInPe.into());
                                }
                                if open {
                                    return Err(XmlError::RecursiveEntityRef.into());
                                }
                                if notation {
                                    return Err(XmlError::BinaryEntityRef.into());
                                }
                                if text {
                                    // m_defaultExpandInternalEntities is
                                    // true without a default handler
                                    // (public API, module 12).
                                    self.open_content_entity(r)?;
                                } else if self.handlers.external_entity_ref {
                                    self.external_general_entity(h, r)?;
                                }
                                // An external entity without a handler: C
                                // only reports it to the default handler
                                // (public API, module 12).
                            }
                        }
                    }
                }
                Scan::Token(Tok::StartTagNoAtts | Tok::StartTagWithAtts) => {
                    let raw_start = s + m;
                    let raw_len = name_length(enc, buf, raw_start);
                    let raw_name = bytes(buf, raw_start, raw_start + raw_len).to_vec();
                    let mut name = to_utf8(enc, &raw_name);
                    let direct = self.direct();
                    self.tracker
                        .open_tag(name.len(), raw_len, direct)
                        .map_err(|_| Stop::Error(XmlError::NoMemory))?;
                    self.tag_stack.push(Tag {
                        raw_name,
                        name: Vec::new(),
                        bindings: None,
                    });
                    self.tag_level += 1;
                    let mut bindings = None;
                    let res = self.store_atts(h, enc, buf, s, &mut name, &mut bindings, account);
                    if let Some(tag) = self.tag_stack.last_mut() {
                        tag.bindings = bindings;
                    }
                    res?;
                    if self.handlers.start_element {
                        self.call_start_element(h, &name);
                    }
                    if let Some(tag) = self.tag_stack.last_mut() {
                        tag.name = name;
                    }
                    self.tracker.temp_pool.clear();
                }
                Scan::Token(Tok::EmptyElementNoAtts | Tok::EmptyElementWithAtts) => {
                    let raw_start = s + m;
                    let raw_len = name_length(enc, buf, raw_start);
                    let mut name = to_utf8(enc, bytes(buf, raw_start, raw_start + raw_len));
                    let mut bindings = None;
                    // C keeps the name in `m_tempPool`.
                    let direct = self.direct();
                    self.tracker
                        .store(Pool::Temp, name.len(), direct)
                        .map_err(|_| Stop::Error(XmlError::NoMemory))?;
                    // The token spans the whole start tag, which the
                    // accounting above has counted already.
                    if let Err(e) =
                        self.store_atts(h, enc, buf, s, &mut name, &mut bindings, Account::None)
                    {
                        self.free_bindings(h, bindings);
                        return Err(e);
                    }
                    if self.handlers.start_element {
                        self.call_start_element(h, &name);
                    }
                    if self.handlers.end_element {
                        if self.handlers.start_element {
                            self.ev(enc, self.ev_end_get(enc));
                        }
                        h.end_element(&name);
                    }
                    self.tracker.temp_pool.clear();
                    self.free_bindings(h, bindings);
                    if let Some(r) = self.after_root_closed(h, buf, next, next_ptr) {
                        return r;
                    }
                }
                Scan::Token(Tok::EndTag) => {
                    if self.tag_level == start_tag_level {
                        return Err(XmlError::AsyncEntity.into());
                    }
                    let raw_start = s + 2 * m;
                    let len = name_length(enc, buf, raw_start);
                    let raw = bytes(buf, raw_start, raw_start + len);
                    let matches = self.tag_stack.last().is_some_and(|t| t.raw_name == raw);
                    if !matches {
                        self.ev(enc, Some(raw_start));
                        return Err(XmlError::TagMismatch.into());
                    }
                    let Some(tag) = self.tag_stack.pop() else {
                        return Err(XmlError::TagMismatch.into());
                    };
                    self.tracker.close_tag();
                    self.tag_level -= 1;
                    if self.handlers.end_element {
                        h.end_element(&tag.name);
                    }
                    self.release_bindings(h, tag.bindings);
                    if let Some(r) = self.after_root_closed(h, buf, next, next_ptr) {
                        return r;
                    }
                }
                Scan::Token(Tok::CharRef) => {
                    let n = char_ref_number(enc, buf, s);
                    let Ok(c) = u32::try_from(n) else {
                        return Err(XmlError::BadCharRef.into());
                    };
                    if self.handlers.character_data {
                        let mut out = [0u8; 4];
                        let k = utf8_encode(c, &mut out);
                        h.character_data(out.get(..k).unwrap_or_default());
                    }
                }
                Scan::Token(Tok::XmlDecl) => return Err(XmlError::MisplacedXmlPi.into()),
                Scan::Token(Tok::DataNewline) => {
                    if self.handlers.character_data {
                        h.character_data(b"\n");
                    }
                }
                Scan::Token(Tok::CdataSectOpen) => {
                    if self.handlers.start_cdata_section {
                        h.start_cdata_section();
                    }
                    let mut after = Some(next);
                    self.do_cdata_section(h, enc, buf, &mut after, next_ptr, have_more, account)?;
                    match after {
                        None => {
                            self.processor = Processor::CdataSection;
                            return Ok(());
                        }
                        // The loop goes on after the end of the section.
                        Some(a) => next = a,
                    }
                }
                Scan::TrailingRsqb => {
                    if have_more {
                        *next_ptr = s;
                        return Ok(());
                    }
                    if self.handlers.character_data {
                        if enc.is_utf8() {
                            h.character_data(bytes(buf, s, end));
                        } else {
                            let mut data = [0u8; INIT_DATA_BUF_SIZE];
                            let r = enc.to_utf8(bytes(buf, s, end), &mut data);
                            h.character_data(data.get(..r.written).unwrap_or_default());
                        }
                    }
                    if start_tag_level == 0 {
                        self.ev(enc, Some(end));
                        return Err(XmlError::NoElements.into());
                    }
                    if self.tag_level != start_tag_level {
                        self.ev(enc, Some(end));
                        return Err(XmlError::AsyncEntity.into());
                    }
                    *next_ptr = end;
                    return Ok(());
                }
                Scan::Token(Tok::DataChars) => {
                    self.report_chars(h, enc, buf, s, next, DataIn::Content);
                }
                Scan::Token(Tok::Pi) => self.report_processing_instruction(h, enc, buf, s, next)?,
                Scan::Token(Tok::Comment) => self.report_comment(h, enc, buf, s, next)?,
                // Every token the content scanner produces has its own
                // case; C keeps a default that only reports to the default
                // handler (module 12).
                _ => {}
            }
            match self.parsing {
                Parsing::Suspended => {
                    self.ev(enc, Some(next));
                    *next_ptr = next;
                    return Ok(());
                }
                Parsing::Finished => {
                    self.ev(enc, Some(next));
                    return Err(XmlError::Aborted.into());
                }
                Parsing::Parsing if self.reenter => {
                    *next_ptr = next;
                    return Ok(());
                }
                Parsing::Parsing | Parsing::Initialized => {
                    s = next;
                    self.ev(enc, Some(s));
                }
            }
        }
    }

    /// The code after an end tag or empty element in `doContent`: when the
    /// root element closed, continue in the epilog. `None` means the loop
    /// goes on.
    fn after_root_closed<H: Handler>(
        &mut self,
        h: &mut H,
        buf: &[u8],
        next: usize,
        next_ptr: &mut usize,
    ) -> Option<Result<(), Stop>> {
        if self.tag_level == 0 && self.parsing != Parsing::Finished {
            if self.parsing == Parsing::Suspended
                || (self.parsing == Parsing::Parsing && self.reenter)
            {
                self.processor = Processor::Epilog;
            } else {
                return Some(self.epilog_processor(h, buf, next, next_ptr));
            }
        }
        None
    }

    /// Calls the start element handler with the attribute list `storeAtts`
    /// built.
    fn call_start_element<H: Handler>(&self, h: &mut H, name: &[u8]) {
        let atts: Vec<(&[u8], &[u8])> = self
            .app_atts
            .iter()
            .map(|a| (a.0.as_slice(), a.1.as_slice()))
            .collect();
        h.start_element(name, &atts, self.n_specified_atts);
    }

    /// `doCdataSection`. `start` is the C `*startPtr`: set to the end of
    /// the section when it closes, `None` while it is still open.
    #[allow(clippy::too_many_arguments)] // reason: mirrors the C signature of doCdataSection.
    fn do_cdata_section<E: Encoding, H: Handler>(
        &mut self,
        h: &mut H,
        enc: &E,
        buf: &[u8],
        start: &mut Option<usize>,
        next_ptr: &mut usize,
        have_more: bool,
        account: Account,
    ) -> Result<(), Stop> {
        let Some(mut s) = start.take() else {
            return Err(XmlError::UnexpectedState.into());
        };
        self.ev(enc, Some(s));
        loop {
            let r = cdata_section_tok(enc, buf, s);
            // In case of XML_TOK_NONE or XML_TOK_PARTIAL.
            let next = r.next.unwrap_or(s);
            if !self
                .accounting
                .diff_tolerated(r.scan, next.saturating_sub(s), account)
            {
                return Err(XmlError::AmplificationLimitBreach.into());
            }
            self.ev_end(enc, Some(next));
            match r.scan {
                Scan::Token(Tok::CdataSectClose) => {
                    if self.handlers.end_cdata_section {
                        h.end_cdata_section();
                    }
                    *start = Some(next);
                    *next_ptr = next;
                    if self.parsing == Parsing::Finished {
                        return Err(XmlError::Aborted.into());
                    }
                    return Ok(());
                }
                Scan::Token(Tok::DataNewline) => {
                    if self.handlers.character_data {
                        h.character_data(b"\n");
                    }
                }
                Scan::Token(Tok::DataChars) => {
                    self.report_chars(h, enc, buf, s, next, DataIn::Cdata);
                }
                Scan::Invalid => {
                    self.ev(enc, Some(next));
                    return Err(XmlError::InvalidToken.into());
                }
                Scan::PartialChar => {
                    if have_more {
                        *next_ptr = s;
                        return Ok(());
                    }
                    return Err(XmlError::PartialChar.into());
                }
                Scan::Partial | Scan::None => {
                    if have_more {
                        *next_ptr = s;
                        return Ok(());
                    }
                    return Err(XmlError::UnclosedCdataSection.into());
                }
                _ => {
                    self.ev(enc, Some(next));
                    return Err(XmlError::UnexpectedState.into());
                }
            }
            match self.parsing {
                Parsing::Suspended => {
                    self.ev(enc, Some(next));
                    *next_ptr = next;
                    return Ok(());
                }
                Parsing::Finished => {
                    self.ev(enc, Some(next));
                    return Err(XmlError::Aborted.into());
                }
                Parsing::Parsing if self.reenter => return Err(XmlError::UnexpectedState.into()),
                Parsing::Parsing | Parsing::Initialized => {
                    s = next;
                    self.ev(enc, Some(s));
                }
            }
        }
    }

    /// `reportProcessingInstruction` (without a default handler).
    pub(crate) fn report_processing_instruction<E: Encoding, H: Handler>(
        &mut self,
        h: &mut H,
        enc: &E,
        buf: &[u8],
        start: usize,
        end: usize,
    ) -> Result<(), Stop> {
        if !self.handlers.processing_instruction {
            return Ok(());
        }
        let m = E::MINBPC;
        let start = start + 2 * m;
        let tem = start + name_length(enc, buf, start);
        let target = to_utf8(enc, bytes(buf, start, tem));
        let data_start = skip_s(enc, bytes(buf, 0, end), tem);
        let mut data = to_utf8(enc, bytes(buf, data_start, end - 2 * m));
        // Target and data in `m_tempPool`, cleared after the handler.
        let direct = self.direct();
        let no_mem = |_| Stop::Error(XmlError::NoMemory);
        self.tracker.store(Pool::Temp, target.len(), direct).map_err(no_mem)?;
        self.tracker.store(Pool::Temp, data.len(), direct).map_err(no_mem)?;
        normalize_lines(&mut data);
        h.processing_instruction(&target, &data);
        self.tracker.temp_pool.clear();
        Ok(())
    }

    /// `reportComment` (without a default handler).
    pub(crate) fn report_comment<E: Encoding, H: Handler>(
        &mut self,
        h: &mut H,
        enc: &E,
        buf: &[u8],
        start: usize,
        end: usize,
    ) -> Result<(), Stop> {
        if !self.handlers.comment {
            return Ok(());
        }
        let m = E::MINBPC;
        let mut data = to_utf8(enc, bytes(buf, start + 4 * m, end - 3 * m));
        // The text in `m_tempPool`, cleared after the handler.
        let direct = self.direct();
        self.tracker
            .store(Pool::Temp, data.len(), direct)
            .map_err(|_| Stop::Error(XmlError::NoMemory))?;
        normalize_lines(&mut data);
        h.comment(&data);
        self.tracker.temp_pool.clear();
        Ok(())
    }

    /// `storeAtts`: normalises and checks the attributes of the start tag
    /// at `att_str`, processes namespace declarations, applies attribute
    /// defaults and expands names; the list for the start handler is left
    /// in `self.app_atts` and the element name in `tag_name`.
    #[allow(clippy::too_many_arguments)] // reason: mirrors the C signature of storeAtts.
    fn store_atts<E: Encoding, H: Handler>(
        &mut self,
        h: &mut H,
        enc: &E,
        buf: &[u8],
        att_str: usize,
        tag_name: &mut Vec<u8>,
        bindings: &mut Option<BindingId>,
        account: Account,
    ) -> Result<(), Stop> {
        let salt = self.salt;
        let no_mem = |_| Stop::Error(XmlError::NoMemory);
        // Look up the element type name.
        let direct = self.direct();
        let et_id = self
            .dtd
            .element_type_for_content(&salt, &mut self.tracker, direct, self.ns, tag_name)
            .map_err(no_mem)?;
        let (et_prefix, et_id_att, defaults, att_index) = match self.dtd.element_types.value(et_id) {
            Some(et) => (
                et.prefix,
                et.id_att,
                et.default_atts.clone(),
                et.default_att_for_name.clone(),
            ),
            None => (None, None, Vec::new(), alloc::collections::BTreeMap::new()),
        };
        let n_default = defaults.len();

        // Get the attributes from the tokenizer.
        if self.atts.len() > i32::MAX as usize {
            return Err(XmlError::NoMemory.into());
        }
        let mut n = get_atts(enc, buf, att_str, &mut self.atts);
        if n + n_default > self.atts.len() {
            let old = self.atts.len();
            if !self.tracker.t.realloc(
                old * ATTRIBUTE_SIZE,
                (n + n_default + INIT_ATTS_SIZE) * ATTRIBUTE_SIZE,
                direct,
            ) {
                return Err(XmlError::NoMemory.into());
            }
            self.atts
                .resize(n + n_default + INIT_ATTS_SIZE, Attribute::default());
            if n > old {
                n = get_atts(enc, buf, att_str, &mut self.atts);
            }
        }

        let mut app: Vec<AppAtt> = Vec::with_capacity(n + n_default);
        let mut n_prefixes = 0usize;
        for i in 0..n {
            let Some(att) = self.atts.get(i).copied() else {
                break;
            };
            let name_end = att.name + name_length(enc, buf, att.name);
            let name = to_utf8(enc, bytes(buf, att.name, name_end));
            let direct = self.direct();
            let id = self
                .dtd
                .attribute_id(&salt, &mut self.tracker, direct, self.ns, &name)
                .map_err(no_mem)?;
            // Detect duplicate attributes by their QNames. This does not
            // work when namespace processing is turned on and different
            // prefixes for the same namespace are used. For this case we
            // have a check further down.
            if self.dtd.att_flag(id) != 0 {
                self.ev_doc(enc, Some(att.name));
                return Err(XmlError::DuplicateAttribute.into());
            }
            self.dtd.set_att_flag(id, 1);
            let a = self.dtd.attribute_ids.value(id).copied().unwrap_or_default();
            let value = if att.normalized {
                // The value did not need normalizing.
                to_utf8(enc, bytes(buf, att.value_ptr, att.value_end))
            } else {
                // Figure out whether declared as other than CDATA.
                let is_cdata = if a.maybe_tokenized {
                    // `defaultAttForName` names the first definition.
                    att_index
                        .get(&id)
                        .and_then(|&i| defaults.get(i))
                        .is_none_or(|d| d.is_cdata)
                } else {
                    true
                };
                self.store_attribute_value(
                    enc,
                    buf,
                    is_cdata,
                    att.value_ptr,
                    att.value_end,
                    account,
                    false,
                )?
            };
            if att.normalized {
                // `poolStoreString(&parser->m_tempPool, ...)`; a value that
                // needed normalising was charged while it was built.
                let direct = self.direct();
                self.tracker
                    .store(Pool::Temp, value.len(), direct)
                    .map_err(no_mem)?;
            }
            match a.prefix {
                Some(prefix) if a.xmlns => {
                    // Deal with namespace declarations here.
                    self.add_binding(h, prefix, Some(id), &value, bindings)?;
                }
                Some(_) => {
                    // Deal with other prefixed names later.
                    n_prefixes += 1;
                    self.dtd.set_att_flag(id, 2);
                    app.push(AppAtt { name, value, id });
                }
                None => app.push(AppAtt { name, value, id }),
            }
        }

        // Set up for XML_GetSpecifiedAttributeCount and
        // XML_GetIdAttributeIndex.
        self.n_specified_atts = (2 * app.len()) as i32;
        match et_id_att {
            Some(ida) if self.dtd.att_flag(ida) != 0 => {
                if let Some(i) = app.iter().position(|a| a.id == ida) {
                    self.id_att_index = (2 * i) as i32;
                }
            }
            _ => self.id_att_index = -1,
        }

        // Do attribute defaulting.
        for da in &defaults {
            let Some(value) = &da.value else { continue };
            if self.dtd.att_flag(da.id) != 0 {
                continue;
            }
            let a = self
                .dtd
                .attribute_ids
                .value(da.id)
                .copied()
                .unwrap_or_default();
            let name = self
                .dtd
                .attribute_ids
                .key(da.id)
                .unwrap_or_default()
                .to_vec();
            match a.prefix {
                Some(prefix) if a.xmlns => {
                    self.add_binding(h, prefix, Some(da.id), value, bindings)?;
                }
                Some(_) => {
                    self.dtd.set_att_flag(da.id, 2);
                    n_prefixes += 1;
                    app.push(AppAtt {
                        name,
                        value: value.clone(),
                        id: da.id,
                    });
                }
                None => {
                    self.dtd.set_att_flag(da.id, 1);
                    app.push(AppAtt {
                        name,
                        value: value.clone(),
                        id: da.id,
                    });
                }
            }
        }

        // Expand prefixed attribute names, check for duplicates, and clear
        // flags that say whether attributes were specified.
        let mut i = 0;
        if n_prefixes > 0 {
            let direct = self.direct();
            self.tracker.ns_atts(n_prefixes, direct).map_err(no_mem)?;
            // C keeps the expanded names in a hash table keyed by SipHash
            // of URI and local part and reports a duplicate when a stored
            // name with the same hash starts with the new one; entries with
            // equal hashes share a probe sequence, so this list finds the
            // same ones.
            let mut seen: Vec<(c_ulong, Vec<u8>)> = Vec::new();
            while i < app.len() {
                let Some(att) = app.get_mut(i) else { break };
                let id = att.id;
                if self.dtd.att_flag(id) == 2 {
                    self.dtd.set_att_flag(id, 0);
                    let Some(prefix) = self.dtd.attribute_ids.value(id).and_then(|a| a.prefix)
                    else {
                        return Err(XmlError::NoMemory.into());
                    };
                    let Some(b) = self.dtd.prefix_binding(prefix) else {
                        return Err(XmlError::UnboundPrefix.into());
                    };
                    let Some(binding) = self.dtd.binding(b) else {
                        return Err(XmlError::UnboundPrefix.into());
                    };
                    let local_start = att
                        .name
                        .iter()
                        .position(|&c| c == b':')
                        .map_or(0, |p| p + 1);
                    let mut expanded = binding.uri.clone();
                    expanded.extend_from_slice(att.name.get(local_start..).unwrap_or_default());
                    let hash = siphash24(&expanded, &salt) as c_ulong;
                    if seen
                        .iter()
                        .any(|(h2, s2)| *h2 == hash && s2.starts_with(&expanded))
                    {
                        return Err(XmlError::DuplicateAttribute.into());
                    }
                    if self.ns_triplets {
                        // Append namespace separator and prefix.
                        expanded.push(self.namespace_separator);
                        expanded.extend_from_slice(
                            self.dtd.prefix_name(binding.prefix).unwrap_or_default(),
                        );
                    }
                    // The expanded name is built in `m_tempPool`.
                    let direct = self.direct();
                    self.tracker
                        .store(Pool::Temp, expanded.len(), direct)
                        .map_err(no_mem)?;
                    seen.push((hash, expanded.clone()));
                    att.name = expanded;
                    n_prefixes -= 1;
                    if n_prefixes == 0 {
                        i += 1;
                        break;
                    }
                } else {
                    self.dtd.set_att_flag(id, 0);
                }
                i += 1;
            }
        }
        // Clear flags for the remaining attributes.
        for att in app.get(i..).unwrap_or_default() {
            self.dtd.set_att_flag(att.id, 0);
        }
        let mut b = *bindings;
        while let Some(id) = b {
            let Some(binding) = self.dtd.binding(id) else { break };
            let (att, next) = (binding.att_id, binding.next_tag_binding);
            if let Some(att) = att {
                self.dtd.set_att_flag(att, 0);
            }
            b = next;
        }
        self.app_atts = app.into_iter().map(|a| (a.name, a.value)).collect();

        if !self.ns {
            return Ok(());
        }
        // Expand the element type name.
        let (b, local_start) = if let Some(p) = et_prefix {
            let Some(b) = self.dtd.prefix_binding(p) else {
                return Err(XmlError::UnboundPrefix.into());
            };
            let colon = tag_name.iter().position(|&c| c == b':').map_or(0, |p| p + 1);
            (b, colon)
        } else if let Some(b) = self.dtd.default_prefix.binding {
            (b, 0)
        } else {
            return Ok(());
        };
        let Some(binding) = self.dtd.binding(b) else {
            return Ok(());
        };
        // The expanded name goes into the binding's URI buffer
        // (`uriLen + localPartLen (+ prefixLen)`, NULs included).
        let prefix_len = if self.ns_triplets {
            self.dtd.prefix_name(binding.prefix).map_or(0, |p| p.len() + 1)
        } else {
            0
        };
        let total = binding.uri.len() + tag_name.len().saturating_sub(local_start) + 1 + prefix_len;
        let direct = self.direct();
        self.tracker.binding_expand(b.0, total, direct).map_err(no_mem)?;
        let Some(binding) = self.dtd.binding(b) else {
            return Ok(());
        };
        let mut expanded = binding.uri.clone();
        expanded.extend_from_slice(tag_name.get(local_start..).unwrap_or_default());
        if self.ns_triplets
            && let Some(p) = self.dtd.prefix_name(binding.prefix)
        {
            // We always have a namespace separator between localPart and
            // prefix.
            expanded.push(self.namespace_separator);
            expanded.extend_from_slice(p);
        }
        *tag_name = expanded;
        Ok(())
    }

    /// `addBinding`: binds `prefix` to `uri` for the element whose binding
    /// list is `bindings`. `att_id` is `None` for the implicit context
    /// (no namespace scope starts, no handler is called).
    pub(crate) fn add_binding<H: Handler>(
        &mut self,
        h: &mut H,
        prefix: PrefixId,
        att_id: Option<EntryId>,
        uri: &[u8],
        bindings: &mut Option<BindingId>,
    ) -> Result<(), Stop> {
        let pname = self.dtd.prefix_name(prefix).map(<[u8]>::to_vec);
        // Empty URI is only valid for default namespace per XML NS 1.0 (not
        // 1.1).
        if uri.is_empty() && pname.is_some() {
            return Err(XmlError::UndeclaringPrefix.into());
        }
        let mut must_be_xml = false;
        if let Some(n) = &pname {
            if n == b"xmlns" {
                // Not allowed to bind xmlns.
                return Err(XmlError::ReservedPrefixXmlns.into());
            }
            if n == b"xml" {
                must_be_xml = true;
            }
        }
        // An application that splits expanded names at the separator must
        // not be confused by a separator inside a URI, unless the separator
        // is itself a URI character.
        if self.ns
            && uri
                .iter()
                .any(|&c| c == self.namespace_separator && !is_rfc3986_uri_char(c))
        {
            return Err(XmlError::Syntax.into());
        }
        let is_xml = uri == super::prolog::XML_NAMESPACE;
        let is_xmlns = !must_be_xml && uri == XMLNS_NAMESPACE;
        if must_be_xml != is_xml {
            return Err(if must_be_xml {
                XmlError::ReservedPrefixXml
            } else {
                XmlError::ReservedNamespaceUri
            }
            .into());
        }
        if is_xmlns {
            return Err(XmlError::ReservedNamespaceUri.into());
        }
        let mut stored = uri.to_vec();
        if self.namespace_separator != 0 {
            stored.push(self.namespace_separator);
        }
        let prev = self.dtd.prefix_binding(prefix);
        let (next_id, reused) = self.dtd.next_binding();
        let direct = self.direct();
        self.tracker
            .binding(next_id.0, reused, stored.len(), direct)
            .map_err(|_| Stop::Error(XmlError::NoMemory))?;
        let (b, _) = self.dtd.new_binding(Binding {
            prefix,
            att_id,
            uri: stored,
            next_tag_binding: *bindings,
            prev_prefix_binding: prev,
        });
        // NULL binding when default namespace undeclared.
        let bound = !(uri.is_empty() && prefix == PrefixId::Default);
        self.dtd.set_prefix_binding(prefix, bound.then_some(b));
        *bindings = Some(b);
        if att_id.is_some() && self.handlers.start_namespace_decl {
            h.start_namespace_decl(pname.as_deref(), bound.then_some(uri));
        }
        Ok(())
    }

    /// `freeBindings`: ends the namespace scopes of a list of bindings.
    fn free_bindings<H: Handler>(&mut self, h: &mut H, bindings: Option<BindingId>) {
        self.release_bindings(h, bindings);
    }

    /// Ends the scope of each binding in the list (end handler, back to the
    /// free list, restore the prefix's previous binding), in list order.
    fn release_bindings<H: Handler>(&mut self, h: &mut H, bindings: Option<BindingId>) {
        let mut cur = bindings;
        while let Some(id) = cur {
            let Some(b) = self.dtd.binding(id) else { break };
            let (prefix, next, prev) = (b.prefix, b.next_tag_binding, b.prev_prefix_binding);
            if self.handlers.end_namespace_decl {
                let name = self.dtd.prefix_name(prefix).map(<[u8]>::to_vec);
                h.end_namespace_decl(name.as_deref());
            }
            self.dtd.free_bindings.push(id);
            self.dtd.set_prefix_binding(prefix, prev);
            cur = next;
        }
    }

    /// `appendAttributeValue` on `buf[ptr..]` (the slice ends at the end of
    /// the value), appending to `out` (the current string of the pool).
    /// Returns at the end of the value or after opening an entity
    /// (`*next_ptr` is then where to continue).
    #[allow(clippy::too_many_arguments)] // reason: mirrors the C signature of appendAttributeValue.
    pub(crate) fn append_attribute_value<E: Encoding>(
        &mut self,
        enc: &E,
        buf: &[u8],
        is_cdata: bool,
        ptr: usize,
        out: &mut Vec<u8>,
        account: Account,
        in_prolog: bool,
        next_ptr: &mut usize,
    ) -> Result<(), Stop> {
        let m = E::MINBPC;
        let mut ptr = ptr;
        let pool = if in_prolog { Pool::Dtd } else { Pool::Temp };
        loop {
            // The pool grows with each token's characters (C: before the
            // next token is checked).
            self.charge_current(pool, out.len())?;
            let r = attribute_value_tok(enc, buf, ptr);
            let mut next = r.next.unwrap_or(ptr);
            if !self
                .accounting
                .diff_tolerated(r.scan, next.saturating_sub(ptr), account)
            {
                return Err(XmlError::AmplificationLimitBreach.into());
            }
            match r.scan {
                Scan::None => {
                    *next_ptr = next;
                    return Ok(());
                }
                Scan::Invalid => {
                    self.ev_doc(enc, Some(next));
                    return Err(XmlError::InvalidToken.into());
                }
                Scan::Partial => {
                    self.ev_doc(enc, Some(ptr));
                    return Err(XmlError::InvalidToken.into());
                }
                Scan::Token(Tok::CharRef) => {
                    let n = char_ref_number(enc, buf, ptr);
                    let Ok(c) = u32::try_from(n) else {
                        self.ev_doc(enc, Some(ptr));
                        return Err(XmlError::BadCharRef.into());
                    };
                    if !(!is_cdata && c == 0x20 && (out.is_empty() || out.last() == Some(&0x20))) {
                        let mut tmp = [0u8; 4];
                        let k = utf8_encode(c, &mut tmp);
                        out.extend_from_slice(tmp.get(..k).unwrap_or_default());
                    }
                }
                Scan::Token(Tok::DataChars) => {
                    out.extend_from_slice(&to_utf8(enc, bytes(buf, ptr, next)));
                }
                Scan::TrailingCr | Scan::Token(Tok::AttributeValueS | Tok::DataNewline) => {
                    if r.scan == Scan::TrailingCr {
                        next = ptr + m;
                    }
                    if is_cdata || !(out.is_empty() || out.last() == Some(&0x20)) {
                        out.push(0x20);
                    }
                }
                Scan::Token(Tok::EntityRef) => {
                    if let Some(ch) = predefined_entity_name(enc, bytes(buf, 0, next - m), ptr + m)
                    {
                        // No amplification: C records without checking.
                        let _ = self.accounting.diff_tolerated(
                            r.scan,
                            1,
                            Account::EntityExpansion,
                        );
                        out.push(ch);
                    } else {
                        let name = to_utf8(enc, bytes(buf, ptr + m, next - m));
                        // The name is stored in `m_temp2Pool` for the lookup.
                        let direct = self.direct();
                        self.tracker
                            .store_discard(Pool::Temp2, name.len(), direct)
                            .map_err(|_| Stop::Error(XmlError::NoMemory))?;
                        let entity = self.dtd.general_entities.find(&self.salt, &name);
                        // First, determine if a check for an existing
                        // declaration is needed; if yes, check that the
                        // entity exists, and that it is internal.
                        let check = if in_prolog {
                            self.prolog_state.document_entity
                                && if self.dtd.standalone {
                                    !self.in_internal_entity()
                                } else {
                                    !self.dtd.has_param_entity_refs
                                }
                        } else {
                            !self.dtd.has_param_entity_refs || self.dtd.standalone
                        };
                        let Some(id) = entity else {
                            if check {
                                return Err(XmlError::UndefinedEntity.into());
                            }
                            // An undeclared entity is skipped (C cannot
                            // report it here).
                            ptr = next;
                            continue;
                        };
                        let r = EntityRef::General(id);
                        let (is_internal, open, notation, text) = match self.dtd.entity(r) {
                            Some(e) => (e.is_internal, e.open, e.notation.is_some(), e.text.is_some()),
                            None => (true, false, false, false),
                        };
                        if check && !is_internal {
                            return Err(XmlError::EntityDeclaredInPe.into());
                        }
                        if open {
                            self.ev_doc(enc, Some(ptr));
                            return Err(XmlError::RecursiveEntityRef.into());
                        }
                        if notation {
                            self.ev_doc(enc, Some(ptr));
                            return Err(XmlError::BinaryEntityRef.into());
                        }
                        if !text {
                            self.ev_doc(enc, Some(ptr));
                            return Err(XmlError::AttributeExternalEntityRef.into());
                        }
                        self.process_entity(r, false, EntityType::Attribute)?;
                        *next_ptr = next;
                        return Ok(());
                    }
                }
                _ => {
                    self.ev_doc(enc, Some(ptr));
                    return Err(XmlError::UnexpectedState.into());
                }
            }
            ptr = next;
        }
    }
}
