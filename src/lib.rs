//! This library holds the bulk of `ck3-tiger`, the xTiger validator for Crusader Kings III mods.
//! The executables are small wrappers around the functions in this library that start and
//! perform validation.

pub use crate::ck3::tables::removed::{Rename, table_renames};
pub use crate::config_load::validate_config_file;
pub use crate::everything::Everything;
pub use crate::fileset::FileKind;
pub use crate::helpers::{TigerHashMap, TigerHashSet};
pub use crate::item::Item;
pub use crate::launcher_settings::get_version_from_launcher;
pub use crate::modfile::ModFile;
pub use crate::report::{
    Confidence, LogReportMetadata, LogReportPointers, PointedMessage, Severity,
    add_loaded_mod_root, disable_ansi_colors, emit_reports, log, set_output_style,
    set_show_loaded_mods, set_show_vanilla, suppress_from_json, take_reports,
};
pub use crate::token::{Loc, Token};

#[cfg(feature = "internal_benches")]
mod benches;

mod ck3;

mod block;
mod config_load;
mod context;
mod data;
mod datacontext;
mod datatype;
mod date;
mod db;
mod dds;
mod deferred;
mod defines;
mod desc;
mod effect;
mod effect_validation;
mod everything;
mod fileset;
mod game;
mod gui;
mod helpers;
mod item;
mod launcher_settings;
mod lowercase;
mod macros;
mod modfile;
mod modif;
mod on_action;
mod parse;
mod pathtable;
mod pdxfile;
mod report;
mod rivers;
mod scopes;
mod script_value;
mod special_tokens;
mod token;
mod tooltipped;
mod trigger;
mod util;
mod validate;
mod validator;
mod variable_scopes;
