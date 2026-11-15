//! Precomputed entity expansion sizes (libexpat 2.10.0, CVE-2026-115506:
//! upstream PRs #1433, #1436, #1439 and #1442; `entities.h` and
//! `xmlparse.c` 6776-7182, 7213-7242).
//!
//! Before 2.10.0 the billion-laughs accounting only noticed an
//! amplification attack while the expansion ran, after up to 100 times
//! the input had been produced. 2.10.0 computes, on the first expansion
//! of an entity, the indirect bytes its complete expansion will charge
//! (`ENTITY.expandedSize`) and refuses the reference up front when the
//! root parser's limits would not tolerate them
//! ([`Parser::check_entity_expansion`], called by `processEntity`).
//!
//! The size of an entity is its text length (one per `XML_Char`) plus, for
//! every reference `&name;` (`%name;` in a parameter entity), the
//! reference's own length and the size of the referenced entity:
//!
//! - a predefined entity (`amp`, `apos`, `gt`, `lt`, `quot`, general text
//!   only) adds one character;
//! - an undefined, external or empty entity, or one on the search stack
//!   (a cycle, which the runtime check reports if it is ever expanded),
//!   adds nothing;
//! - `<![CDATA[ ... ]]>`, `<!-- ... -->` and `<? ... ?>` are counted
//!   verbatim without looking for references in them (#1439; without
//!   this, valid documents with `&` in such sections were rejected), up to
//!   the end of the text when they are not closed;
//! - `&` / `%` without a following `;` counts the rest of the text
//!   verbatim.
//!
//! The sizes of the referenced entities are computed first, by an
//! iterative depth-first search (`resolveEntityClosureExpandedSize`): a
//! scan stops at a reference to an entity whose size is not known yet,
//! that entity is pushed, and the scan resumes at the reference
//! (`scanPtr`) once it is done. The search stack is a tracked allocation
//! of one pointer per entry of the entity table, like C's.
//!
//! The entity statistics that C's `EXPAT_ENTITY_DEBUG` output reports
//! (`m_entity_stats`) are not ported (they are only printed), so the
//! depth bookkeeping of the search is left out.

use super::dtd::EntityRef;
use super::error::{Stop, XmlError};
use super::{Parser, c_layout};

/// `enum EntityScanResult`: how a [`scan_entity_text`] call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityScanResult {
    /// `ENTITY_SCAN_COMPLETE`: the end of the text was reached; the size
    /// is final.
    Complete,
    /// `ENTITY_SCAN_BLOCKED`: stopped at a reference to a non-empty entity
    /// whose size is not computed yet (the blocker); the cursor is parked
    /// at the reference.
    Blocked(EntityRef),
    /// `ENTITY_SCAN_OVERFLOW`: the size would exceed `XmlBigCount`.
    Overflow,
}

/// The kind of section [`is_cdata_comment_or_pi_start`] recognises
/// (`XML_TOK_CDATA_SECT_OPEN`, `XML_TOK_COMMENT`, `XML_TOK_PI`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionStart {
    /// `<![CDATA[`
    Cdata,
    /// `<!--`
    Comment,
    /// `<?`
    Pi,
}

/// `"<![CDATA["`
const CDATA_OPEN: &[u8] = b"<![CDATA[";
/// `"]]>"`
const CDATA_CLOSE: &[u8] = b"]]>";
/// `"<!--"`
const COMMENT_OPEN: &[u8] = b"<!--";
/// `"-->"`
const COMMENT_CLOSE: &[u8] = b"-->";
/// `"<?"`
const PI_OPEN_LEN: usize = 2;
/// `"?>"`
const PI_CLOSE: &[u8] = b"?>";

/// `sizeof(ENTITY_ANALYSIS_TASK)`: one `ENTITY *`.
const ANALYSIS_TASK_SIZE: usize = c_layout::ENTITY_ANALYSIS_TASK;

/// `isPredefinedEntityName`: `Some(character)` for `amp`, `apos`, `gt`,
/// `lt` and `quot`.
pub fn is_predefined_entity_name(name: &[u8]) -> Option<u8> {
    match name {
        b"lt" => Some(b'<'),
        b"gt" => Some(b'>'),
        b"amp" => Some(b'&'),
        b"apos" => Some(b'\''),
        b"quot" => Some(b'"'),
        _ => None,
    }
}

/// `isCDataCommentOrPIStart(pos, maxLen, &openTextLen)` for
/// `text[..max_len]`: the section that starts there and the length of its
/// opening, or `None`.
pub fn is_cdata_comment_or_pi_start(text: &[u8], max_len: usize) -> Option<(SectionStart, usize)> {
    let text = text.get(..max_len.min(text.len()))?;
    if text.len() < 2 || text.first() != Some(&b'<') {
        return None;
    }
    match text.get(1) {
        Some(b'?') => Some((SectionStart::Pi, PI_OPEN_LEN)),
        Some(b'!') if text.starts_with(CDATA_OPEN) => Some((SectionStart::Cdata, CDATA_OPEN.len())),
        Some(b'!') if text.starts_with(COMMENT_OPEN) => Some((SectionStart::Comment, COMMENT_OPEN.len())),
        _ => None,
    }
}

/// `xcsnstr(haystack, needle, maxLen)` (`xcs.c`, a port of BSD
/// `strnstr`): the offset of the first `needle` that starts within the
/// first `max_len - needle.len() + 1` positions of `haystack`; like C, the
/// search stops at a NUL in the haystack.
pub fn xcsnstr(haystack: &[u8], needle: &[u8], max_len: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > max_len {
        return None;
    }
    let max_tries = max_len - needle.len() + 1;
    for i in 0..max_tries {
        match haystack.get(i) {
            None | Some(0) => return None,
            Some(_) => {}
        }
        if haystack.get(i..).is_some_and(|rest| rest.starts_with(needle)) {
            return Some(i);
        }
    }
    None
}

/// `scanEntityText`: scans `text` (the text of an entity, a parameter
/// entity's when `is_param`) from `*pos`, adding to `*expanded_size`.
/// `lookup` resolves a referenced name in the entity's table to the
/// entity and its state. On `Complete` and `Blocked` `*pos` is updated
/// (to the end, or to the blocking reference); after `Overflow` neither
/// `*pos` nor `*expanded_size` is meaningful.
pub fn scan_entity_text<F>(
    text: &[u8],
    is_param: bool,
    pos: &mut usize,
    expanded_size: &mut u64,
    mut lookup: F,
) -> EntityScanResult
where
    F: FnMut(&[u8]) -> Option<(EntityRef, ScanView)>,
{
    let ref_char = if is_param { b'%' } else { b'&' };
    let end = text.len();
    let mut p = *pos;
    // Adds `n` to the size, `false` on overflow.
    let add = |size: &mut u64, n: u64| -> bool {
        match size.checked_add(n) {
            Some(v) => {
                *size = v;
                true
            }
            None => false,
        }
    };
    while p < end {
        let rest = text.get(p..).unwrap_or_default();
        // Detect and skip CDATA/comment/PI sections because entities have
        // no effect in these places.
        if rest.first() == Some(&b'<')
            && let Some((kind, open_len)) = is_cdata_comment_or_pi_start(rest, end - p)
        {
            if !add(expanded_size, open_len as u64) {
                return EntityScanResult::Overflow;
            }
            p += open_len;
            let close: &[u8] = match kind {
                SectionStart::Cdata => CDATA_CLOSE,
                SectionStart::Comment => COMMENT_CLOSE,
                SectionStart::Pi => PI_CLOSE,
            };
            let remainder_len = end - p;
            let Some(close_start) = xcsnstr(text.get(p..).unwrap_or_default(), close, remainder_len) else {
                // The section is not terminated.
                if !add(expanded_size, remainder_len as u64) {
                    return EntityScanResult::Overflow;
                }
                *pos = end;
                return EntityScanResult::Complete;
            };
            let skipped_len = close_start + close.len();
            if !add(expanded_size, skipped_len as u64) {
                return EntityScanResult::Overflow;
            }
            p += skipped_len;
            continue;
        }
        // Detect an entity reference.
        if rest.first() != Some(&ref_char) {
            if !add(expanded_size, 1) {
                return EntityScanResult::Overflow;
            }
            p += 1;
            continue;
        }
        let name_start = p + 1;
        let Some(name_len) = text
            .get(name_start..)
            .and_then(|t| t.iter().position(|&c| c == b';'))
        else {
            // Unterminated reference at the end of the text: the trailing
            // bytes count as literal characters.
            if !add(expanded_size, (end - p) as u64) {
                return EntityScanResult::Overflow;
            }
            *pos = end;
            return EntityScanResult::Complete;
        };
        let name = text.get(name_start..name_start + name_len).unwrap_or_default();
        // The whole "&name;" / "%name;" token is charged at runtime, then
        // its expansion.
        let ref_token_len = (1 + name_len + 1) as u64;
        let addend = match lookup(name) {
            None => {
                // Predefined entities are general-only and expand to one
                // character; an undefined entity expands to nothing.
                if !is_param && is_predefined_entity_name(name).is_some() {
                    ref_token_len + 1
                } else {
                    ref_token_len
                }
            }
            // A cycle: only the token (the runtime recursion check reports
            // it if the cycle is ever expanded).
            Some((_, v)) if v.being_analyzed => ref_token_len,
            Some((r, v)) if v.text_len > 0 && v.expanded_size == 0 => {
                *pos = p;
                return EntityScanResult::Blocked(r);
            }
            Some((_, v)) if v.expanded_size > 0 => {
                let Some(a) = v.expanded_size.checked_add(ref_token_len) else {
                    return EntityScanResult::Overflow;
                };
                a
            }
            // An empty or external entity: only the token.
            Some(_) => ref_token_len,
        };
        if !add(expanded_size, addend) {
            return EntityScanResult::Overflow;
        }
        p = name_start + name_len + 1;
    }
    *pos = end;
    EntityScanResult::Complete
}

/// What [`scan_entity_text`] needs to know about a referenced entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanView {
    /// `textLen` (0 for an external entity).
    pub text_len: usize,
    /// `expandedSize`
    pub expanded_size: u64,
    /// `beingAnalyzed`
    pub being_analyzed: bool,
}

impl Parser {
    /// The table entry of `name` in the general (or parameter) entity
    /// table (`lookupWithLength(..., 0)`), with its scan state.
    fn scan_lookup(&self, is_param: bool, name: &[u8]) -> Option<(EntityRef, ScanView)> {
        let table = if is_param {
            &self.dtd.param_entities
        } else {
            &self.dtd.general_entities
        };
        let id = table.find(&self.salt, name)?;
        let e = table.value(id)?;
        let r = if is_param {
            EntityRef::Param(id)
        } else {
            EntityRef::General(id)
        };
        Some((
            r,
            ScanView {
                text_len: e.text.as_ref().map_or(0, |t| t.len()),
                expanded_size: e.expanded_size,
                being_analyzed: e.being_analyzed,
            },
        ))
    }

    /// One `scanEntityText` call on the entity `r` from its saved cursor;
    /// on `Complete` and `Blocked` the cursor and the size so far are
    /// stored back into the entity, like C's `e->scanPtr` and
    /// `e->expandedSize = sizeSoFar`.
    fn scan_entity(&mut self, r: EntityRef) -> EntityScanResult {
        let Some(e) = self.dtd.entity(r) else {
            return EntityScanResult::Complete;
        };
        let text = e.text.clone().unwrap_or_else(|| alloc::sync::Arc::from(&[][..]));
        let is_param = e.is_param;
        let mut pos = e.scan_pos;
        let mut size = e.expanded_size;
        let result = scan_entity_text(&text, is_param, &mut pos, &mut size, |name| {
            self.scan_lookup(is_param, name)
        });
        if !matches!(result, EntityScanResult::Overflow)
            && let Some(e) = self.dtd.entity_mut(r)
        {
            e.scan_pos = pos;
            e.expanded_size = size;
        }
        result
    }

    /// `resolveEntityClosureExpandedSize`: computes the expanded size of
    /// `root` and of every entity it references whose size is not known
    /// yet. `NoMemory` when the tracker refuses the search stack,
    /// `AmplificationLimitBreach` when a size overflows `XmlBigCount`.
    fn resolve_entity_closure_expanded_size(&mut self, root: EntityRef) -> Result<(), Stop> {
        let Some(e) = self.dtd.entity(root) else {
            return Ok(());
        };
        if e.expanded_size != 0 || e.text.as_ref().is_none_or(|t| t.is_empty()) {
            return Ok(());
        }
        // `table->used` of the table the entity is in.
        let used = match root {
            EntityRef::General(_) => self.dtd.general_entities.len(),
            EntityRef::Param(_) => self.dtd.param_entities.len(),
        };
        if used == 0 {
            return Ok(());
        }
        // `MALLOC(parser, sizeof(ENTITY_ANALYSIS_TASK) * table->used)`.
        let Some(stack_bytes) = used.checked_mul(ANALYSIS_TASK_SIZE) else {
            return Err(XmlError::NoMemory.into());
        };
        let direct = self.direct();
        if !self.tracker.t.alloc(stack_bytes, direct) {
            return Err(XmlError::NoMemory.into());
        }
        let mut stack: alloc::vec::Vec<EntityRef> = alloc::vec::Vec::new();
        stack.push(root);
        self.set_being_analyzed(root, true);
        while let Some(&e) = stack.last() {
            match self.scan_entity(e) {
                EntityScanResult::Overflow => {
                    // The parse is dead; the marks of the stacked entities
                    // stay set, like C's.
                    self.tracker.t.free(stack_bytes);
                    return Err(XmlError::AmplificationLimitBreach.into());
                }
                EntityScanResult::Complete => {
                    self.set_being_analyzed(e, false);
                    stack.pop();
                }
                EntityScanResult::Blocked(blocker) => {
                    // A blocker's size is still uncomputed and it is never
                    // on the stack already, so each entity is pushed at
                    // most once.
                    stack.push(blocker);
                    self.set_being_analyzed(blocker, true);
                }
            }
        }
        self.tracker.t.free(stack_bytes);
        Ok(())
    }

    fn set_being_analyzed(&mut self, r: EntityRef, on: bool) {
        if let Some(e) = self.dtd.entity_mut(r) {
            e.being_analyzed = on;
        }
    }

    /// The 2.10.0 part of `processEntity` (`xmlparse.c` 7213-7242), before
    /// the open-entity structure is taken: computes the entity's expanded
    /// size on its first expansion, then refuses the expansion with
    /// `XML_ERROR_AMPLIFICATION_LIMIT_BREACH` when the root parser's
    /// accounting would not tolerate that many more indirect bytes.
    pub(crate) fn check_entity_expansion(&mut self, r: EntityRef) -> Result<(), Stop> {
        let Some(e) = self.dtd.entity(r) else {
            return Ok(());
        };
        if e.text.as_ref().is_some_and(|t| !t.is_empty()) && e.expanded_size == 0 {
            self.resolve_entity_closure_expanded_size(r)?;
        }
        let expanded = self.dtd.entity(r).map_or(0, |e| e.expanded_size);
        if expanded > 0 && !self.accounting.amplification_tolerated(expanded) {
            return Err(XmlError::AmplificationLimitBreach.into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
