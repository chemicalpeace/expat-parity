//! External entities in the public API (D-055): what
//! [`Handler::external_entity_ref`] receives to create the parser of an
//! external entity (`XML_ExternalEntityParserCreate`).
//!
//! The library never fetches an entity. The handler obtains the entity's
//! bytes itself, creates an [`ExternalParser`] for it, feeds it with the
//! methods of [`Parser`] (to which it dereferences) and drops it before
//! returning. Dropping it is `XML_ParserFree` of the external entity
//! parser: the state it shares with the document's parser (the DTD of a
//! parameter entity, the allocation and amplification counts) goes back
//! to that parser.
//!
//! ```
//! use expat::{Events, ExternalEntities, Handler, Parser};
//!
//! /// Supplies `<b/>` for every external general entity.
//! #[derive(Default)]
//! struct Resolver(Vec<Vec<u8>>);
//!
//! impl Handler for Resolver {
//!     const EVENTS: Events = Events::NONE.start_element(true).external_entity_ref(true);
//!     fn start_element(&mut self, name: &[u8], _atts: &[(&[u8], &[u8])], _specified: usize) {
//!         self.0.push(name.to_vec());
//!     }
//!     fn external_entity_ref(
//!         &mut self,
//!         entities: &mut ExternalEntities<'_>,
//!         context: Option<&[u8]>,
//!         _base: Option<&[u8]>,
//!         _system_id: Option<&[u8]>,
//!         _public_id: Option<&[u8]>,
//!     ) -> bool {
//!         let Some(mut child) = entities.create(context, None) else { return false };
//!         child.parse_with(self, b"<b/>", true).is_ok()
//!     }
//! }
//!
//! let mut r = Resolver::default();
//! let mut p = Parser::new([7; 16]);
//! p.parse_with(&mut r, b"<!DOCTYPE a [<!ENTITY e SYSTEM 'e.xml'>]><a>&e;</a>", true)?;
//! assert_eq!(r.0, [b"a".to_vec(), b"b".to_vec()]);
//! # Ok::<(), expat::Error>(())
//! ```
//!
//! [`Handler::external_entity_ref`]: crate::Handler::external_entity_ref

use core::fmt;
use core::ops::{Deref, DerefMut};

use crate::api::Parser;
use crate::parse;

/// Creates the parser of the external entity whose reference
/// [`Handler::external_entity_ref`] is called for.
///
/// [`Handler::external_entity_ref`]: crate::Handler::external_entity_ref
pub struct ExternalEntities<'a> {
    parent: &'a mut parse::Parser,
}

impl fmt::Debug for ExternalEntities<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExternalEntities").finish_non_exhaustive()
    }
}

impl<'a> ExternalEntities<'a> {
    pub(crate) fn new(parent: &'a mut parse::Parser) -> Self {
        ExternalEntities { parent }
    }

    /// `XML_ExternalEntityParserCreate(parser, context, encoding)`:
    /// `context` as the handler received it (`None` for a parameter
    /// entity or the external subset), `encoding` the entity's encoding
    /// when the caller knows it better than the entity says (as for
    /// [`Options::encoding`]).
    ///
    /// The new parser reports the events the document's parser reports
    /// at the time of the call, and keeps its namespace processing,
    /// [`Parser::set_return_ns_triplet`], [`Parser::set_param_entity_parsing`]
    /// and reparse deferral. `None` where C returns `NULL`: the allocation
    /// tracker refuses the new parser, or `context` names a prefix or
    /// entity that cannot be set up.
    ///
    /// [`Options::encoding`]: crate::Options::encoding
    pub fn create(
        &mut self,
        context: Option<&[u8]>,
        encoding: Option<&[u8]>,
    ) -> Option<ExternalParser<'_>> {
        let (child, bytes_before) = self
            .parent
            .external_entity_parser_create(context, encoding)?;
        Some(ExternalParser {
            parent: &mut *self.parent,
            child: Parser::from_inner(child),
            bytes_before,
        })
    }
}

/// The parser of an external entity, created by
/// [`ExternalEntities::create`]. It dereferences to a [`Parser`]; dropping
/// it is `XML_ParserFree`.
///
/// [`Parser::reset`] refuses it, as `XML_ParserReset` refuses an external
/// entity parser.
pub struct ExternalParser<'p> {
    parent: &'p mut parse::Parser,
    child: Parser,
    /// The root parser's allocation count before the child was created.
    bytes_before: u64,
}

impl fmt::Debug for ExternalParser<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExternalParser").finish_non_exhaustive()
    }
}

impl Deref for ExternalParser<'_> {
    type Target = Parser;
    fn deref(&self) -> &Parser {
        &self.child
    }
}

impl DerefMut for ExternalParser<'_> {
    fn deref_mut(&mut self) -> &mut Parser {
        &mut self.child
    }
}

impl Drop for ExternalParser<'_> {
    fn drop(&mut self) {
        self.parent
            .external_entity_parser_free(self.child.inner_mut(), self.bytes_before);
    }
}
