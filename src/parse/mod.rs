//! Parsers for the various kinds of game script.

pub mod cob;
pub mod csv;
pub mod ignore;
pub mod json;
pub mod localization;
pub mod pdxfile;

/// Global state for parser that need it. Can be passed down to the parser.
#[derive(Clone, Default, Debug)]
pub struct ParserMemory {
    pub pdxfile: pdxfile::memory::PdxfileMemory,
}
