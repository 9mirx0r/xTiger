//! Script keys that older game versions accepted and 1.20 no longer knows, with what replaced them.
//!
//! These are only used to add a hint to an "unknown field", "unknown token", "unknown
//! datafunction" or missing-item report, so a mod written for an older version gets pointed at the
//! replacement instead of just being told that the name does not exist. Add an entry only after
//! checking the name against the game's own files.

use crate::ck3::tables::effects::SCOPE_EFFECT;
use crate::ck3::tables::triggers::TRIGGER;
use crate::effect::Effect;
use crate::item::Item;
use crate::trigger::Trigger;

const SCHOLAR: &str = "the `scholar` trait is gone; the lifestyle trait is `lifestyle_scholar`, and the game's own trait conversion table maps old `scholar` to `erudite`";

/// `(key, hint)`, sorted by key.
const REMOVED: &[(&str, &str)] = &[
    (
        "create_holy_order_effect",
        "in 1.20 this is `create_holy_order_accompanying_effect`, and `create_holy_order_neutral_effect` needs an `ORDER_TYPE` argument",
    ),
    ("every_character", "in 1.20 use `every_living_character`"),
    ("is_created", "in 1.20 use `is_title_created`"),
    ("scholar", SCHOLAR),
    (
        "set_title_flag",
        "title flags do not exist in 1.20; use a variable on the title or a character flag instead",
    ),
    (
        "trait_xp",
        "history characters have no `trait_xp` in 1.20; use a dated `effect = { add_trait_xp = { trait = <trait> track = <track> value = <n> } }` instead",
    ),
];

/// `(datafunction, hint)`, sorted by name, for data functions that no longer exist at all.
const REMOVED_DATAFUNCTIONS: &[(&str, &str)] = &[(
    "GetFaithDoctrine",
    "in 1.20 a doctrine comes from `GetDoctrine('<key>')`, for example `GetDoctrine('<key>').GetName( GetPlayer.GetFaith )`",
)];

/// `(datafunction, type it was taken from, hint)`, for data functions that moved on one type.
const MOVED_DATAFUNCTIONS: &[(&str, &str, &str)] =
    &[("GetOwner", "Activity", "an Activity has no owner in 1.20; use `GetHost`")];

/// The hint for a key that older versions accepted, if there is one.
pub fn removed_key_hint(key: &str) -> Option<&'static str> {
    REMOVED.binary_search_by(|(k, _)| (*k).cmp(key)).ok().map(|idx| REMOVED[idx].1)
}

/// The hint for a datafunction that no longer exists, if there is one.
pub fn removed_datafunction_hint(name: &str) -> Option<&'static str> {
    REMOVED_DATAFUNCTIONS
        .binary_search_by(|(k, _)| (*k).cmp(name))
        .ok()
        .map(|idx| REMOVED_DATAFUNCTIONS[idx].1)
}

/// The hint for a datafunction that is not allowed after the type `after` any more.
pub fn moved_datafunction_hint(name: &str, after: &str) -> Option<&'static str> {
    MOVED_DATAFUNCTIONS.iter().find(|(n, t, _)| *n == name && *t == after).map(|(.., hint)| *hint)
}

/// The hint for an item that older versions defined and 1.20 does not.
pub fn removed_item_hint(itype: Item, key: &str) -> Option<&'static str> {
    match (itype, key) {
        (Item::Trait, "scholar") => Some(SCHOLAR),
        // `add_trait_track_xp` reads as the lifestyle `trait_track`.
        (Item::Lifestyle, "trait_track") => Some(
            "`add_trait_track_xp` does not exist in 1.20; use `add_trait_xp = { trait = <trait> track = <track> value = <n> }`",
        ),
        _ => None,
    }
}

/// An effect or trigger that an older version had under another name, read from the `Removed`
/// entries of the effect and trigger tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rename {
    pub old: &'static str,
    pub new: &'static str,
    /// The version that removed `old`.
    pub version: &'static str,
    /// `"effect"` or `"trigger"`.
    pub kind: &'static str,
}

/// The single key a `Removed` explanation names as the replacement, if that is all it says:
/// "renamed to x", "replaced by `x`" or "replaced with x".
fn named_replacement(explanation: &'static str) -> Option<&'static str> {
    let rest = ["renamed to ", "replaced by ", "replaced with "]
        .iter()
        .find_map(|prefix| explanation.strip_prefix(prefix))?;
    let key = rest.strip_prefix('`').and_then(|r| r.strip_suffix('`')).unwrap_or(rest);
    (!key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then_some(key)
}

/// The renames in the effect and trigger tables that are one old key to one new key, sorted by old
/// key. The new key must be a current entry of the same table, and no other removed key may name
/// it, so a replacement that merges several keys ("replaced with `num_accolades`" for both
/// `num_active_accolades` and `num_inactive_accolades`) is left out.
pub fn table_renames() -> Vec<Rename> {
    let effects: Vec<_> = SCOPE_EFFECT
        .iter()
        .map(|(_, name, effect)| match effect {
            Effect::Removed(version, explanation) => (*name, Some((*version, *explanation))),
            _ => (*name, None),
        })
        .collect();
    let triggers: Vec<_> = TRIGGER
        .iter()
        .map(|(_, name, trigger)| match trigger {
            Trigger::Removed(version, explanation) => (*name, Some((*version, *explanation))),
            _ => (*name, None),
        })
        .collect();
    let mut renames: Vec<Rename> = Vec::new();
    for (kind, table) in [("effect", effects), ("trigger", triggers)] {
        let current = |key: &str| table.iter().any(|(name, gone)| *name == key && gone.is_none());
        let candidates: Vec<Rename> = table
            .iter()
            .filter_map(|(old, gone)| {
                let (version, explanation) = (*gone)?;
                let new = named_replacement(explanation)?;
                current(new).then_some(Rename { old, new, version, kind })
            })
            .collect();
        let unique =
            |rename: &&Rename| candidates.iter().filter(|c| c.new == rename.new).count() == 1;
        renames.extend(candidates.iter().filter(unique).copied());
    }
    renames.sort_by_key(|rename| (rename.old, rename.kind));
    renames.dedup_by_key(|rename| rename.old);
    renames
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_sorted_and_unique() {
        assert!(REMOVED.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(REMOVED_DATAFUNCTIONS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn finds_known_keys() {
        assert!(removed_key_hint("trait_xp").is_some());
        assert!(removed_key_hint("every_character").unwrap().contains("every_living_character"));
        assert!(removed_key_hint("is_created").unwrap().contains("is_title_created"));
        assert!(removed_key_hint("no_such_key").is_none());
    }

    #[test]
    fn finds_datafunctions_and_items() {
        assert!(removed_datafunction_hint("GetFaithDoctrine").unwrap().contains("GetDoctrine"));
        assert!(removed_datafunction_hint("GetDoctrine").is_none());
        assert!(moved_datafunction_hint("GetOwner", "Activity").unwrap().contains("GetHost"));
        assert!(moved_datafunction_hint("GetOwner", "Character").is_none());
        assert!(removed_item_hint(Item::Trait, "scholar").is_some());
        assert!(removed_item_hint(Item::Trait, "brave").is_none());
        assert!(
            removed_item_hint(Item::Lifestyle, "trait_track").unwrap().contains("add_trait_xp")
        );
        assert!(removed_item_hint(Item::Faith, "scholar").is_none());
    }

    #[test]
    fn reads_one_to_one_renames_from_the_tables() {
        let renames = table_renames();
        assert!(renames.windows(2).all(|w| w[0].old < w[1].old));
        let find = |old: &str| renames.iter().find(|r| r.old == old).copied();
        let diarchy = find("start_diarchy").unwrap();
        assert_eq!(
            (diarchy.new, diarchy.version, diarchy.kind),
            ("try_start_diarchy", "1.16", "effect")
        );
        assert_eq!(find("is_widget_open").unwrap().new, "is_widgetid_open");
        assert_eq!(find("has_holy_site_flag").unwrap().new, "has_holy_site_parameter");
        // Several old keys onto one new key is not a rename.
        assert!(find("num_active_accolades").is_none());
        // The replacement was removed itself later.
        assert!(find("remove_title_to_sub_region").is_none());
        // More than a name.
        assert!(find("invite_character_to_activity").is_none());
        assert!(find("accept_invitation_for_character").is_none());
    }

    #[test]
    fn reads_only_a_bare_key_as_the_replacement() {
        assert_eq!(
            named_replacement("renamed to remove_title_from_sub_region"),
            Some("remove_title_from_sub_region")
        );
        assert_eq!(named_replacement("replaced with `max_accolades`"), Some("max_accolades"));
        assert_eq!(named_replacement("replaced by `scheme_freeze`"), Some("scheme_freeze"));
        assert_eq!(named_replacement("replaced with return_home character effect"), None);
        assert_eq!(named_replacement("replaced by the `extra_building_slot` modifier"), None);
        assert_eq!(named_replacement(""), None);
    }
}
