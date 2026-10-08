use crate::block::{BV, Block};
use crate::ck3::tables::misc::CUSTOM_RELIGION_LOCAS;
use crate::ck3::validate::validate_traits;
use crate::context::ScopeContext;
use crate::db::{Db, DbKind};
use crate::everything::Everything;
use crate::fileset::FileKind;
use crate::helpers::TigerHashMap;
use crate::item::{Item, ItemLoader};
use crate::report::{ErrorKey, err, warn};
use crate::scopes::Scopes;
use crate::token::Token;
use crate::tooltipped::Tooltipped;
use crate::validate::validate_possibly_named_color;
use crate::validator::Validator;

#[derive(Clone, Debug)]
pub struct Religion {}

inventory::submit! {
    ItemLoader::Normal(Item::Religion, Religion::add)
}

impl Religion {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::Religion, key, block, Box::new(Self {}));
    }
}

impl DbKind for Religion {
    fn add_subitems(&self, _key: &Token, block: &Block, db: &mut Db) {
        // 1.19+: faiths live in common/religion/faith_types, see [`Faith`]
        if let Some(block) = block.get_field_block("custom_faith_icons") {
            for token in block.iter_values() {
                db.add_flag(Item::FaithIcon, token.clone());
            }
        }
        if let Some(details) = block.get_field_block("religion_details")
            && let Some(token) = details.get_field_value("graphical_faith")
        {
            db.add_flag(Item::GraphicalFaith, token.clone());
        }
    }

    fn validate(&self, key: &Token, block: &Block, data: &Everything) {
        data.verify_exists(Item::Localization, key);
        let loca = format!("{key}_adj");
        data.verify_exists_implied(Item::Localization, &loca, key);
        let loca = format!("{key}_adherent");
        data.verify_exists_implied(Item::Localization, &loca, key);
        let loca = format!("{key}_adherent_plural");
        data.verify_exists_implied(Item::Localization, &loca, key);
        let loca = format!("{key}_desc");
        data.verify_exists_implied(Item::Localization, &loca, key);

        // let modif = format!("{key}_opinion");
        // data.verify_exists_implied(Item::ModifierFormat, &modif, key);

        let mut vd = Validator::new(block, data);

        vd.req_field("religion_details");
        vd.field_validated_block("religion_details", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.req_field("family");
            vd.field_item("family", Item::ReligionFamily);
            vd.field_icon("tenet_background_icon", "NGameIcons|FAITH_DOCTRINE_BACKGROUND_PATH", "");
            vd.field_icon(
                "doctrine_background_icon",
                "NGameIcons|FAITH_DOCTRINE_BACKGROUND_PATH",
                "",
            );
            validate_piety_icon_group(vd.field_value("piety_icon_group"), data);
            vd.field_value("graphical_faith");
            vd.field_item("theocracy_government_type", Item::GovernmentType);
            vd.field_item("theocracy_lease_contract_type", Item::LeaseContract);
        });

        validate_doctrines("religion", data, &mut vd);

        vd.field_item("main_holy_site", Item::HolySite);
        vd.field_bool("pagan_roots");
        vd.field_validated_block("traits", validate_traits);

        vd.field_validated_list("custom_faith_icons", |icon, data| {
            data.verify_icon("NGameIcons|FAITH_ICON_PATH", icon, ".dds");
        });

        vd.field_list("reserved_male_names"); // TODO
        vd.field_list("reserved_female_names"); // TODO

        vd.field_validated_block("holy_order_names", validate_holy_order_names);
        vd.field_list_items("holy_order_maa", Item::MenAtArms);
        vd.field_validated_block("localization", validate_localization);
    }

    fn has_property(
        &self,
        _key: &Token,
        block: &Block,
        property: &str,
        _data: &Everything,
    ) -> bool {
        if let Some(block) = block.get_field_block("localization") {
            block.has_key(property)
        } else {
            false
        }
    }
}

fn validate_doctrines(iname: &str, data: &Everything, vd: &mut Validator) {
    // TODO: maybe cache doctrine_types and number_of_picks,
    // though the cache might only make sense if done globally instead of per religion/faith.
    let mut groups: TigerHashMap<&str, Vec<Token>> = TigerHashMap::default();
    vd.multi_field_validated_value("doctrine", |_, mut vd| {
        vd.item(Item::Doctrine);
        let doctrine = vd.value();
        for (group, block) in data.database.iter_key_block(Item::DoctrineGroup) {
            if let Some(doctrines) = block.get_field_block("doctrine_types") {
                if !doctrines.iter_values().any(|t| t == doctrine) {
                    continue;
                }
            } else {
                continue;
            }

            if let Some(seen) = groups.get_mut(group.as_str()) {
                let picks_token = block.get_field_value("number_of_picks");
                let picks = picks_token.and_then(Token::get_integer).unwrap_or(1);
                #[allow(clippy::cast_possible_wrap)]
                if let Some(other_doctrine) = seen.iter().find(|&d| d == doctrine) {
                    let msg = format!("duplicate doctrine {doctrine}");
                    warn(ErrorKey::DuplicateField)
                        .msg(msg)
                        .loc(doctrine)
                        .loc_msg(other_doctrine, "earlier doctrine")
                        .push();
                } else if picks == 1 {
                    // SAFETY: we never push empty vecs into this hash
                    let other_doctrine = &seen[0];
                    let msg = format!("{doctrine} and {other_doctrine} are both from {group}");
                    let info = format!("{group} only allows 1 pick");
                    err(ErrorKey::Conflict)
                        .msg(msg)
                        .info(info)
                        .loc(doctrine)
                        .loc_msg(other_doctrine, "earlier doctrine")
                        .push();
                } else if picks == (seen.len() as i64) {
                    let msg = format!("{iname} has more than {picks} doctrines from {group}");
                    // SAFETY: picks_token can be unwrapped because Some(Token) is the only
                    // way to get picks > 1
                    err(ErrorKey::Conflict)
                        .msg(msg)
                        .loc(doctrine)
                        .loc_msg(picks_token.unwrap(), "picks")
                        .push();
                }
                seen.push(doctrine.clone());
            } else {
                groups.insert(group.as_str(), vec![doctrine.clone()]);
            }
        }
    });

    vd.multi_field_validated_block("doctrine_selection_pair", |block, data| {
        let mut vd = Validator::new(block, data);
        vd.field_item("requires_dlc_flag", Item::DlcFeature);
        vd.field_item("doctrine", Item::Doctrine);
        vd.field_item("fallback_doctrine", Item::Doctrine);
    });
}

fn validate_localization(block: &Block, data: &Everything) {
    let mut vd = Validator::new(block, data);
    for field in CUSTOM_RELIGION_LOCAS {
        vd.field_validated(field, |bv, data| match bv {
            BV::Value(token) => data.verify_exists(Item::Localization, token),
            BV::Block(block) => {
                let mut vd = Validator::new(block, data);
                for token in vd.values() {
                    data.verify_exists(Item::Localization, token);
                }
            }
        });
    }
}

fn validate_holy_order_names(block: &Block, data: &Everything) {
    let mut vd = Validator::new(block, data);
    for block in vd.blocks() {
        let mut vd = Validator::new(block, data);
        vd.req_field("name");
        vd.field_item("name", Item::Localization);
        vd.field_item("coat_of_arms", Item::Coa);
    }
}

/// Loaded from `common/religion/faith_types` (1.20+; earlier versions nested faiths in religions)
#[derive(Clone, Debug)]
pub struct Faith {
    religion: Token,
}

inventory::submit! {
    ItemLoader::Normal(Item::Faith, Faith::add)
}

impl Faith {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        let details = block.get_field_block("faith_details");
        if let Some(details) = details {
            if let Some(token) = details.get_field_value("graphical_faith") {
                db.add_flag(Item::GraphicalFaith, token.clone());
            }
            if let Some(token) = details.get_field_value("icon") {
                db.add_flag(Item::FaithIcon, token.clone());
            } else {
                db.add_flag(Item::FaithIcon, key.clone());
            }
            if let Some(token) = details.get_field_value("reformed_icon") {
                db.add_flag(Item::FaithIcon, token.clone());
            }
        } else {
            db.add_flag(Item::FaithIcon, key.clone());
        }
        let religion = details
            .and_then(|d| d.get_field_value("religion"))
            .cloned()
            .unwrap_or_else(|| key.clone());
        db.add(Item::Faith, key, block, Box::new(Self { religion }));
    }

    fn check_have_customs(&self, key: &Token, block: &Block, data: &Everything) {
        let details = block.get_field_block("faith_details");
        let locas = block.get_field_block("localization");
        for loca in CUSTOM_RELIGION_LOCAS {
            if let Some(block) = locas
                && block.has_key(loca)
            {
                continue;
            }
            if let Some(details) = details
                && details.has_key(loca)
            {
                continue;
            }
            if data.item_has_property(Item::Religion, self.religion.as_str(), loca) {
                continue;
            }
            let msg = format!("faith or religion missing localization for {loca}");
            warn(ErrorKey::MissingLocalization).msg(msg).loc(key).push();
        }
    }
}

impl DbKind for Faith {
    fn validate(&self, key: &Token, block: &Block, data: &Everything) {
        data.verify_exists(Item::Localization, key);
        let loca = format!("{key}_adj");
        data.verify_exists_implied(Item::Localization, &loca, key);
        let loca = format!("{key}_adherent");
        data.verify_exists_implied(Item::Localization, &loca, key);
        let loca = format!("{key}_adherent_plural");
        data.verify_exists_implied(Item::Localization, &loca, key);
        let loca = format!("{key}_desc");
        data.verify_exists_implied(Item::Localization, &loca, key);

        let pagan = block.get_field_block("doctrines").is_some_and(|b| {
            b.iter_values()
                .any(|value| data.item_has_property(Item::Doctrine, value.as_str(), "unreformed"))
        });
        if pagan {
            let loca = format!("{key}_old");
            data.verify_exists_implied(Item::Localization, &loca, key);
            let loca = format!("{key}_old_adj");
            data.verify_exists_implied(Item::Localization, &loca, key);
            let loca = format!("{key}_old_adherent");
            data.verify_exists_implied(Item::Localization, &loca, key);
            let loca = format!("{key}_old_adherent_plural");
            data.verify_exists_implied(Item::Localization, &loca, key);
        }

        let mut vd = Validator::new(block, data);

        vd.req_field("faith_details");
        vd.field_validated_block("faith_details", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.req_field("religion");
            vd.field_item("religion", Item::Religion);
            vd.req_field("color");
            vd.field_validated("color", validate_possibly_named_color);
            let icon = vd.field_value("icon").unwrap_or(key);
            data.verify_icon("NGameIcons|FAITH_ICON_PATH", icon, ".dds");
            if pagan {
                vd.req_field_fatal("reformed_icon");
            } else {
                vd.ban_field("reformed_icon", || "unreformed faiths");
            }
            vd.field_icon("reformed_icon", "NGameIcons|FAITH_ICON_PATH", ".dds");
            vd.field_value("graphical_faith");
            validate_piety_icon_group(vd.field_value("piety_icon_group"), data);
            vd.field_icon(
                "doctrine_background_icon",
                "NGameIcons|FAITH_DOCTRINE_BACKGROUND_PATH",
                "",
            );
            vd.field_item("religious_head", Item::Title);
            vd.field_item("head_of_rite", Item::Title);
            vd.field_item("theocracy_government_type", Item::GovernmentType);
            vd.field_item("theocracy_lease_contract_type", Item::LeaseContract);
            // The rest are localization-like custom values (HighGodName etc.)
            vd.unknown_value_fields(|_, _| ());
        });

        vd.field_item("origin", Item::Faith);
        vd.field_list_items("holy_sites", Item::HolySite);
        vd.field_list_items("eminent_holy_sites", Item::HolySite);
        vd.field_list_items("tenets", Item::Tenet);
        vd.field_list_items("doctrines", Item::Doctrine);
        vd.field_item("main_rite", Item::Rite);
        vd.field_list_items("cultures", Item::Culture);
        vd.field_bool("historical");
        vd.field_list("clerical_elector_titles"); // TODO
        vd.multi_field_validated_block("tenet_selection_pair", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.field_item("requires_dlc_flag", Item::DlcFeature);
            vd.field_item("tenet", Item::Tenet);
            vd.field_item("fallback_tenet", Item::Tenet);
        });

        vd.field_list("reserved_male_names");
        vd.field_list("reserved_female_names");
        vd.field_validated_block("localization", validate_localization);
        vd.field_validated_block("holy_order_names", validate_holy_order_names);
        vd.field_list_items("holy_order_maa", Item::MenAtArms);

        self.check_have_customs(key, block, data);
    }

    fn has_property(
        &self,
        key: &Token,
        _block: &Block,
        property: &str,
        _data: &Everything,
    ) -> bool {
        if property == "is_modded" {
            return key.loc.kind == FileKind::Mod;
        }
        false
    }
}

/// Loaded from `common/religion/rite_types` (1.20+)
#[derive(Clone, Debug)]
pub struct Rite {}

inventory::submit! {
    ItemLoader::Normal(Item::Rite, Rite::add)
}

impl Rite {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::Rite, key, block, Box::new(Self {}));
    }
}

impl DbKind for Rite {
    fn validate(&self, key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);
        if !block.has_key("name") {
            data.verify_exists(Item::Localization, key);
        }
        vd.field("name"); // TODO: dynamic desc
        vd.field("desc"); // TODO: dynamic desc
        vd.field_value("icon");
        vd.field_validated("color", validate_possibly_named_color);
        vd.field_item("founder", Item::Title);
        vd.field_item("faith", Item::Faith);
        vd.field_list_items("cultures", Item::Culture);
        vd.field_bool("convert");
        vd.field_bool("create");
        vd.field_list_items("tenets", Item::Tenet);
        vd.field_list_items("doctrines", Item::Doctrine);
        vd.multi_field_validated_block("tenet_selection_pair", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.field_item("requires_dlc_flag", Item::DlcFeature);
            vd.field_item("tenet", Item::Tenet);
            vd.field_item("fallback_tenet", Item::Tenet);
        });
        vd.field_validated_block("localization", validate_localization);
    }
}

#[derive(Clone, Debug)]
pub struct ReligionFamily {}

inventory::submit! {
    ItemLoader::Normal(Item::ReligionFamily, ReligionFamily::add)
}

impl ReligionFamily {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::ReligionFamily, key, block, Box::new(Self {}));
    }
}

impl DbKind for ReligionFamily {
    fn add_subitems(&self, _key: &Token, block: &Block, db: &mut Db) {
        if let Some(token) = block.get_field_value("graphical_faith") {
            db.add_flag(Item::GraphicalFaith, token.clone());
        }
    }

    fn validate(&self, key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);

        // let modif = format!("{key}_opinion");
        // data.verify_exists_implied(Item::ModifierFormat, &modif, key);

        let name = vd.field_value("name").unwrap_or(key);
        data.verify_exists(Item::Localization, name);
        let loca = format!("{name}_desc");
        data.verify_exists_implied(Item::Localization, &loca, name);

        vd.field_bool("is_pagan");
        vd.field_value("graphical_faith");
        validate_piety_icon_group(vd.field_value("piety_icon_group"), data);
        vd.field_icon("doctrine_background_icon", "NGameIcons|FAITH_DOCTRINE_BACKGROUND_PATH", "");
        for f in &[
            "tenet_background_icon",
            "tenet_heretical_background_icon",
            "tenet_neutral_background_icon",
            "tenet_unknown_background_icon",
        ] {
            vd.field_icon(f, "NGameIcons|FAITH_DOCTRINE_BACKGROUND_PATH", "");
        }
        vd.field_item("hostility_doctrine", Item::Doctrine);
    }
}

#[derive(Clone, Debug)]
pub struct FervorModifier {}

inventory::submit! {
    ItemLoader::Normal(Item::FervorModifier, FervorModifier::add)
}

impl FervorModifier {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::FervorModifier, key, block, Box::new(Self {}));
    }
}

impl DbKind for FervorModifier {
    fn validate(&self, key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);
        let mut sc = ScopeContext::new(Scopes::Faith, key);
        vd.field_script_value("value", &mut sc);
        vd.field_trigger("trigger", Tooltipped::No, &mut sc);
    }
}

fn validate_piety_icon_group(group: Option<&Token>, data: &Everything) {
    if let Some(group) = group {
        if let Some(valid) = data.get_defined_array_warn(group, "NGameIcons|PIETY_GROUPS") {
            for valid_group in valid.iter_values() {
                if group == valid_group {
                    return;
                }
            }
        }
        let msg = "piety icon group not listed in PIETY_GROUPS define";
        warn(ErrorKey::Choice).msg(msg).loc(group).push();
    }
}
