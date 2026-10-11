//! Expansion of internal entities (`processEntity`,
//! `internalEntityProcessor`, the open-entity lists). Module 11a ports the
//! declarations; the expansion itself is the next step (11b), so opening
//! an entity still stops the parser with [`Stop::DtdNotPorted`].

use super::Parser;
use super::dtd::EntityRef;
use super::error::Stop;

impl Parser {
    /// `parser->m_openInternalEntities != NULL` (or a parent parser,
    /// which only external entity parsers have).
    pub(crate) fn in_internal_entity(&self) -> bool {
        false
    }

    /// `processEntity(parser, entity, betweenDecl, ENTITY_INTERNAL)` for a
    /// parameter entity referenced in the prolog.
    pub(crate) fn open_param_entity(&mut self, _r: EntityRef, _between_decl: bool) -> Result<(), Stop> {
        Err(Stop::DtdNotPorted)
    }

    /// `processEntity(parser, entity, XML_FALSE, ENTITY_INTERNAL)` for a
    /// general entity referenced in content.
    pub(crate) fn open_content_entity(&mut self, _r: EntityRef) -> Result<(), Stop> {
        Err(Stop::DtdNotPorted)
    }

    /// `processEntity(parser, entity, XML_FALSE, ENTITY_ATTRIBUTE)` for a
    /// general entity referenced in an attribute value.
    pub(crate) fn expand_attribute_entity(&mut self, _r: EntityRef) -> Result<(), Stop> {
        Err(Stop::DtdNotPorted)
    }
}
