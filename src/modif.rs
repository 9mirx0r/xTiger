//! Validator for `modifs` which is our name for the basic things that modifiers modify.
//!
//! The main entry points are the [`validate_modifs`] function and the [`ModifKinds`] type.

use std::fmt::Display;

use bitflags::Flags;

use crate::block::Block;
use crate::everything::Everything;
use crate::item::Item;
use crate::report::{ErrorKey, Severity, err};
use crate::script_value::validate_non_dynamic_script_value;
use crate::token::Token;
use crate::validator::Validator;

/// All the things a modif can apply to.
/// Many modifs are for multiple things, so this is a bitflags type.
///
/// This trait is used to warn when a modif is used inappropriately.
pub trait ModifKinds: Display + Flags + Copy {
    fn require(self, other: Self, token: &Token) {
        if !self.intersects(other) {
            let msg = format!("`{token}` is a modifier for {other} but expected {self}");
            err(ErrorKey::Modifiers).msg(msg).loc(token).push();
        }
    }

    /// Returns Some(kinds) if the token is a valid modif or *could* be a valid modif if the appropriate item existed.
    /// Returns None otherwise.
    fn lookup_modif(name: &Token, data: &Everything, warn: Option<Severity>) -> Option<Self>;
}

pub fn validate_modifs<'a, MK: ModifKinds>(
    _block: &Block,
    data: &'a Everything,
    kinds: MK,
    mut vd: Validator<'a>,
) {
    vd.unknown_fields(|key, bv| {
        if let Some(mk) = MK::lookup_modif(key, data, Some(Severity::Error)) {
            kinds.require(mk, key);
            validate_non_dynamic_script_value(bv, data);
            if !key.is("health")
                && !key.is("elderly_health")
                && !key.is("child_health")
                && !key.is("negate_health_penalty_add")
            {
                data.verify_exists(Item::ModifierFormat, key);
            }
        } else {
            let msg = format!("unknown modifier `{key}`");
            err(ErrorKey::UnknownField).msg(msg).loc(key).push();
        }
    });
}

pub fn verify_modif_exists<MK: ModifKinds>(
    key: &Token,
    data: &Everything,
    kinds: MK,
    sev: Severity,
) {
    if let Some(mk) = MK::lookup_modif(key, data, Some(sev)) {
        kinds.require(mk, key);
    } else {
        let msg = format!("unknown modifier `{key}`");
        err(ErrorKey::UnknownField).msg(msg).loc(key).push();
    }
}
