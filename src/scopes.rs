//! The core [`Scopes`] type which tracks our knowledge about the types of in-game values.

use std::fmt::{Display, Formatter};

use bitflags::bitflags;

use crate::context::ScopeContext;
use crate::everything::Everything;
use crate::helpers::{camel_case_to_separated_words, display_choices, snake_case_to_camel_case};
use crate::item::Item;
use crate::lowercase::Lowercase;
use crate::report::{ErrorKey, err};
use crate::token::Token;

/// vic3 and ck3 and eu5 need more than 64 bits, but the others don't.
type ScopesBits = u128;

bitflags! {
    /// This type represents our knowledge about the set of scope types that a script value can
    /// have. In most cases it's narrowed down to a single scope type, but not always.
    ///
    /// The available scope types depend on the game.
    /// They are listed in `event_scopes.log` from the game data dumps.
    // LAST UPDATED CK3 VERSION 1.16.0
    // LAST UPDATED VIC3 VERSION 1.8.1
    // LAST UPDATED IR VERSION 2.0.4
    //
    // Each scope type gets one bitflag. In order to keep the bit count down, scope types from
    // the different games have overlapping bitflags. Therefore, scope types from different games
    // should be kept carefully separated.
    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    #[rustfmt::skip] // having the cfg and the flag on one line is much more readable
    pub struct Scopes: ScopesBits {
        // Generic scope types
        const None = 1<<0;
        const Value = 1<<1;
        const Bool = 1<<2;
        const Flag = 1<<3;

        // Scope types shared by multiple games

        const Character = 1<<6;
        const Culture = 1<<7;
        const Province = 1<<8;
        const Religion = 1<<11;
        const War = 1<<13;
        const Decision = 1<<15;

        // Scope types for CK3
        const Accolade = 1<<16;
        const AccoladeType = 1<<17;
        const Activity = 1<<18;
        const ActivityType = 1<<19;
        const Army = 1<<20;
        const Artifact = 1<<21;
        const CasusBelli = 1<<22;
        const CharacterMemory = 1<<23;
        const Combat = 1<<24;
        const CombatSide = 1<<25;
        const CouncilTask = 1<<26;
        const CulturePillar = 1<<27;
        const CultureTradition = 1<<28;
        const Doctrine = 1<<29;
        const Dynasty = 1<<30;
        const DynastyHouse = 1<<31;
        const Faction = 1<<32;
        const Faith = 1<<33;
        const GovernmentType = 1<<34;
        const GreatHolyWar = 1<<35;
        const HolyOrder = 1<<36;
        const Inspiration = 1<<37;
        const LandedTitle = 1<<38;
        const MercenaryCompany = 1<<39;
        const Scheme = 1<<40;
        const Secret = 1<<41;
        const StoryCycle = 1<<42;
        const Struggle = 1<<43;
        const TitleAndVassalChange = 1<<44;
        const Trait = 1<<45;
        const TravelPlan = 1<<46;
        const VassalContract = 1<<47;
        const VassalObligationLevel = 1<<48;
        // CK3 1.11
        const HoldingType = 1<<49;
        const TaxSlot = 1<<50;
        // CK3 1.12
        const EpidemicType = 1<<51;
        const Epidemic = 1<<52;
        const LegendType = 1<<53;
        const Legend = 1<<54;
        const GeographicalRegion = 1<<55;
        // CK3 1.13
        const Domicile = 1<<56;
        const AgentSlot = 1<<57;
        const TaskContract = 1<<58;
        const TaskContractType = 1<<59;
        const Regiment = 1<<60;
        const CasusBelliType = 1<<61;
        // CK3 1.15
        const CourtPosition = 1<<62;
        const CourtPositionType = 1<<63;
        // CK3 1.16
        const Situation = 1<<64;
        const SituationParticipantGroup = 1<<65;
        const SituationSubRegion = 1<<66;
        const Confederation = 1<<67;
        // CK3 1.18
        const HouseAspiration = 1<<68;
        const HouseRelation = 1<<69;
        const HouseRelationType = 1<<70;
        const HouseRelationLevel = 1<<71;
        const ConfederationType = 1<<72;
        const GreatProject = 1<<73;
        const ProjectContribution = 1<<74;
        const CultureInnovation = 1<<75;
        const GreatProjectType = 1<<76;
        // CK3 1.20
        const HolySite = 1<<77;
        const Organization = 1<<78;
        const Rite = 1<<79;
        const Tenet = 1<<80;
        const HolySiteType = 1<<81;
        const RiteType = 1<<82;




        // These two "combined" ones represent the odd scopes created for events.
    }
}

// These have to be expressed a bit awkwardly because the binary operators are not `const`.
// TODO: Scopes::all() returns a too-large set if multiple features are enabled.
impl Scopes {
    pub const fn non_primitive() -> Scopes {
        Scopes::all()
            .difference(Scopes::None.union(Scopes::Value).union(Scopes::Bool).union(Scopes::Flag))
    }

    pub const fn primitive() -> Scopes {
        Scopes::Value.union(Scopes::Bool).union(Scopes::Flag)
    }

    pub const fn all_but_none() -> Scopes {
        Scopes::all().difference(Scopes::None)
    }

    /// Read a scope type in string form and return it as a [`Scopes`] value.
    pub fn from_snake_case(s: &str) -> Option<Scopes> {
        // Deal with some exceptions to the general pattern
        match s {
            "ghw" => return Some(Scopes::GreatHolyWar),
            "story" => return Some(Scopes::StoryCycle),
            "great_holy_war" | "story_cycle" => return None,
            _ => (),
        }

        Scopes::from_name(&snake_case_to_camel_case(s))
    }

    /// Similar to `from_snake_case`, but allows multiple scopes separated by `|`
    /// Returns None if any of the conversions fail.
    pub fn from_snake_case_multi(s: &str) -> Option<Scopes> {
        let mut scopes = Scopes::empty();
        for part in s.split('|') {
            scopes |= Scopes::from_snake_case(part)?;
        }
        // If `scopes` is still empty then probably `s` was empty.
        // Remember that `Scopes::empty()` is different from a bitfield containing `Scopes::None`.
        if scopes == Scopes::empty() {
            return None;
        }
        Some(scopes)
    }
}

impl Display for Scopes {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        if *self == Scopes::all() {
            write!(f, "any scope")
        } else if *self == Scopes::primitive() {
            write!(f, "any primitive scope")
        } else if *self == Scopes::non_primitive() {
            write!(f, "non-primitive scope")
        } else if *self == Scopes::all_but_none() {
            write!(f, "any except none scope")
        } else {
            let mut vec = Vec::new();
            for (name, _) in self.iter_names() {
                vec.push(camel_case_to_separated_words(name));
            }
            let vec: Vec<&str> = vec.iter().map(String::as_ref).collect();
            display_choices(f, &vec, "or")
        }
    }
}

/// A description of the constraints on a value with a prefix such as `var:` or `list_size:`
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ArgumentValue {
    /// The value must be an expression that resolves to a scope object of the given type.
    Scope(Scopes),
    /// The value must be the name of an item of the given item type.
    Item(Item),
    /// The value can be either a Scope or an Item
    ScopeOrItem(Scopes, Item),
    /// The value can be a trait name or `trait|track`.
    TraitTrack,
    /// The value must be a single word
    Identifier(&'static str),
    /// The value can be anything
    UncheckedValue,
    /// This trigger no longer exists. Arguments are version and explanation
    Removed(&'static str, &'static str),
}

/// Look up an "event link", which is a script token that looks up something related
/// to a scope value and returns another scope value.
///
/// `name` is the token.
///
/// Returns a pair of `Scopes`. The first is the scope types this token can accept as input,
/// and the second is the scope types it may return.
pub fn scope_to_scope(name: &Token) -> Option<(Scopes, Scopes)> {
    let scope_to_scope = crate::ck3::tables::targets::scope_to_scope;
    let scope_to_scope_removed = crate::ck3::tables::targets::scope_to_scope_removed;

    let name_lc = name.as_str().to_ascii_lowercase();
    if let scopes @ Some(_) = scope_to_scope(&name_lc) {
        scopes
    } else if let Some((version, explanation)) = scope_to_scope_removed(&name_lc) {
        let msg = format!("`{name}` was removed in {version}");
        err(ErrorKey::Removed).strong().msg(msg).info(explanation).loc(name).push();
        Some((Scopes::all(), Scopes::all_but_none()))
    } else {
        None
    }
}

/// Look up a prefixed token that is used to look up items in the game database.
///
/// For example, `character:alexander_the_great` to fetch that character as a scope value.
///
/// Some prefixes have an input scope, and they look up something related to the input scope value.
///
/// Returns a pair of `Scopes` and the type of argument it accepts.
/// The first `Scopes` is the scope types this token can accept as input, and the second one is
/// the scope types it may return. The first will be `Scopes::None` if it needs no input.
pub fn scope_prefix(prefix: &Token) -> Option<(Scopes, Scopes, ArgumentValue)> {
    let scope_prefix = crate::ck3::tables::targets::scope_prefix;
    let prefix_lc = prefix.as_str().to_ascii_lowercase();
    scope_prefix(&prefix_lc)
}

/// Look up a token that's an invalid target, and see if it might be missing a prefix.
/// Return the prefix if one was found.
///
/// `scopes` should be a singular `Scopes` flag.
///
/// Example: if the token is "irish" and `scopes` is `Scopes::Culture` then return
/// `Some("culture")` to indicate that the token should have been "culture:irish".
pub fn needs_prefix(arg: &str, data: &Everything, scopes: Scopes) -> Option<&'static str> {
    crate::ck3::scopes::needs_prefix(arg, data, scopes)
}

/// Look up an iterator, which is a script element that executes its block multiple times, once for
/// each applicable scope value. Iterators may be builtin (the usual case) or may be scripted lists.
///
/// `name` is the name of the iterator, without its `any_`, `every_`, `random_` or `ordered_` prefix.
/// `sc` is a [`ScopeContext`], only used for validating scripted lists.
///
/// Returns a pair of `Scopes`. The first is the scope types this token can accept as input,
/// and the second is the scope types it may return.
/// The first will be `Scopes::None` if it needs no input.
pub fn scope_iterator(
    name: &Token,
    data: &Everything,
    sc: &mut ScopeContext,
) -> Option<(Scopes, Scopes)> {
    let scope_iterator = crate::ck3::tables::iterators::iterator;
    let scope_iterator_removed = crate::ck3::tables::iterators::iterator_removed;

    let name_lc = Lowercase::new(name.as_str());
    if let scopes @ Some(_) = scope_iterator(&name_lc, name, data) {
        return scopes;
    }
    if let Some((version, explanation)) = scope_iterator_removed(name_lc.as_str()) {
        let msg = format!("`{name}` iterators were removed in {version}");
        err(ErrorKey::Removed).strong().msg(msg).info(explanation).loc(name).push();
        return Some((Scopes::all(), Scopes::all()));
    }
    if data.scripted_lists.exists(name.as_str()) {
        data.scripted_lists.validate_call(name, data, sc);
        return data
            .scripted_lists
            .base(name)
            .and_then(|base| scope_iterator(&Lowercase::new(base.as_str()), base, data));
    }
    None
}
