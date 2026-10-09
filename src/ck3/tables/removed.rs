//! Script keys that older game versions accepted and 1.20 no longer knows, with what replaced them.
//!
//! These are only used to add a hint to an "unknown field", "unknown token", "unknown
//! datafunction" or missing-item report, so a mod written for an older version gets pointed at the
//! replacement instead of just being told that the name does not exist. Add an entry only after
//! checking the name against the game's own files.

use crate::item::Item;

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
}
