use crate::block::Block;
use crate::ck3::modif::ModifKinds;
use crate::ck3::validate::validate_traits;
use crate::context::ScopeContext;
use crate::db::{Db, DbKind};
use crate::desc::validate_desc;
use crate::everything::Everything;
use crate::item::{Item, ItemLoader};
use crate::modif::validate_modifs;
use crate::scopes::Scopes;
use crate::token::Token;
use crate::tooltipped::Tooltipped;
use crate::validator::Validator;

#[derive(Clone, Debug)]
pub struct DoctrineGroup {}
#[derive(Clone, Debug)]
pub struct Doctrine {}
/// Loaded from `common/religion/tenet_types` (1.20+)
#[derive(Clone, Debug)]
pub struct Tenet {}

inventory::submit! {
    ItemLoader::Normal(Item::DoctrineGroup, DoctrineGroup::add)
}
inventory::submit! {
    ItemLoader::Normal(Item::Doctrine, Doctrine::add)
}

inventory::submit! {
    ItemLoader::Normal(Item::Tenet, Tenet::add)
}

impl Tenet {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::Tenet, key, block, Box::new(Self {}));
    }
}

impl DoctrineGroup {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::DoctrineGroup, key, block, Box::new(Self {}));
    }
}
impl Doctrine {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::Doctrine, key, block, Box::new(Self {}));
    }
}

impl DbKind for DoctrineGroup {
    fn validate(&self, key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);

        let loca = format!("{key}_name");
        data.verify_exists_implied(Item::Localization, &loca, key);

        data.verify_icon("NGameIcons|DOCTRINE_GROUP_TYPE_ICON_PATH", key, ".dds");

        vd.field_value("category");
        vd.field_bool("main_rite");
        vd.field_choice("divergence", &["category", "sequence"]);
        vd.field_choice("doctrine_lock", &["none", "rite", "faith", "religion"]);

        vd.field_integer("number_of_picks");
        vd.field_trigger_rooted("is_available_on_create", Tooltipped::No, Scopes::Faith);
    }
}

impl DbKind for Doctrine {
    fn has_property(
        &self,
        _key: &Token,
        block: &Block,
        property: &str,
        _data: &Everything,
    ) -> bool {
        if property == "unreformed"
            && let Some(parameters) = block.get_field_block("parameters")
        {
            for (key, _) in parameters.iter_assignments() {
                if key.is("unreformed") {
                    return true;
                }
            }
            for value in parameters.iter_values() {
                if value.is("unreformed") {
                    return true;
                }
            }
        }
        false
    }

    fn add_subitems(&self, _key: &Token, block: &Block, db: &mut Db) {
        if let Some(block) = block.get_field_block("parameters") {
            for (key, value) in block.iter_assignments() {
                if value.is("yes") || value.is("no") {
                    db.add_flag(Item::DoctrineBooleanParameter, key.clone());
                }
                db.add_flag(Item::DoctrineParameter, key.clone());
            }
            for value in block.iter_values() {
                db.add_flag(Item::DoctrineBooleanParameter, value.clone());
                db.add_flag(Item::DoctrineParameter, value.clone());
            }
        }
    }

    fn validate(&self, key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);
        let mut sc = ScopeContext::new(Scopes::Faith.union(Scopes::Rite), key);
        sc.define_list("selected_doctrines", Scopes::Doctrine, key);

        vd.req_field("doctrine_group_type");
        vd.field_item("doctrine_group_type", Item::DoctrineGroup);
        vd.field_integer("index");
        vd.field_integer("divergence");

        let icon = vd.field_value("icon").unwrap_or(key);
        data.verify_icon("NGameIcons|DOCTRINE_TYPE_ICON_PATH", icon, ".dds");

        if !vd.field_validated_sc("name", &mut sc, validate_desc) {
            let loca = format!("{key}_name");
            data.verify_exists_implied(Item::Localization, &loca, key);
        }

        if !vd.field_validated_sc("desc", &mut sc, validate_desc) {
            let loca = format!("{key}_desc");
            data.verify_exists_implied(Item::Localization, &loca, key);
        }

        vd.field_bool("visible");
        vd.field_validated_block("parameters", validate_parameters);
        vd.field_script_value("piety_cost", &mut sc);
        vd.field_trigger("is_shown", Tooltipped::No, &mut sc);
        vd.field_trigger("can_pick", Tooltipped::Yes, &mut sc);

        vd.field_validated_block("character_modifier", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.field_item("name", Item::Localization);
            validate_modifs(block, data, ModifKinds::Character, vd);
        });

        // Not documented, but used in vanilla
        vd.field_validated_block("clergy_modifier", |block, data| {
            let vd = Validator::new(block, data);
            validate_modifs(block, data, ModifKinds::Character, vd);
        });

        // In the docs but not used in vanilla
        vd.field_validated_block("faith_character_modifier", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.field_item("doctrine", Item::Doctrine);
            validate_modifs(block, data, ModifKinds::Character, vd);
        });

        vd.field_validated_block("special_parameters", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.field_validated_block("hostility_levels", |block, data| {
                let mut vd = Validator::new(block, data);
                for f in &["same_religion", "same_family", "unrelated"] {
                    vd.field_choice(f, &["righteous", "astray", "hostile", "evil"]);
                }
            });
            vd.unknown_fields(|_, _| {});
        });
        vd.field_validated_block("traits", validate_traits);
    }
}

fn validate_parameters(block: &Block, data: &Everything) {
    let mut vd = Validator::new(block, data);

    vd.values(); // bare boolean parameters (1.20)
    vd.unknown_value_fields(|_, value| {
        if value.is("yes") || value.is("no") {
            return;
        }
        value.expect_number();
    });
}

impl DbKind for Tenet {
    fn add_subitems(&self, _key: &Token, block: &Block, db: &mut Db) {
        if let Some(block) = block.get_field_block("personal_tenet_parameters") {
            for value in block.iter_values() {
                db.add_flag(Item::PersonalTenetFlag, value.clone());
            }
        }
        if let Some(block) = block.get_field_block("parameters") {
            for (key, value) in block.iter_assignments() {
                if value.is("yes") || value.is("no") {
                    db.add_flag(Item::DoctrineBooleanParameter, key.clone());
                }
                db.add_flag(Item::DoctrineParameter, key.clone());
            }
            for value in block.iter_values() {
                db.add_flag(Item::DoctrineBooleanParameter, value.clone());
                db.add_flag(Item::DoctrineParameter, value.clone());
            }
        }
    }

    fn validate(&self, key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);
        let mut sc = ScopeContext::new(Scopes::Faith.union(Scopes::Rite), key);
        sc.define_list("selected_tenets", Scopes::Tenet, key);

        if let Some(icon) = vd.field_value("icon") {
            data.verify_icon("NGameIcons|TENET_TYPE_ICON_PATH", icon, ".dds");
        } else {
            vd.field_block("icon"); // triggered icons
            data.verify_icon("NGameIcons|TENET_TYPE_ICON_PATH", key, ".dds");
        }

        if !vd.field_validated_sc("name", &mut sc, validate_desc) {
            let loca = format!("{key}_name");
            data.verify_exists_implied(Item::Localization, &loca, key);
        }
        if !vd.field_validated_sc("desc", &mut sc, validate_desc) {
            let loca = format!("{key}_desc");
            data.verify_exists_implied(Item::Localization, &loca, key);
        }

        vd.field_bool("visible");
        vd.field_item("requires_dlc_flag", Item::DlcFeature);
        vd.field_validated_block("parameters", validate_parameters);
        vd.field_script_value("piety_cost", &mut sc);
        vd.field_trigger("is_shown", Tooltipped::No, &mut sc);
        vd.field_trigger("can_pick", Tooltipped::Yes, &mut sc);
        vd.field_trigger_rooted("can_pick_as_personal_tenet", Tooltipped::Yes, Scopes::Character);
        vd.field_numeric("divergence_multiplier");

        vd.field_validated_block("character_modifier", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.field_item("name", Item::Localization);
            validate_modifs(block, data, ModifKinds::Character, vd);
        });
        vd.field_validated_block("county_modifier", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.field_item("name", Item::Localization);
            validate_modifs(block, data, ModifKinds::County, vd);
        });
        vd.field_validated_block("personal_tenet_modifier", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.field_item("name", Item::Localization);
            validate_modifs(block, data, ModifKinds::Character, vd);
        });
        vd.field_validated_block("personal_tenet_parameters", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.values();
        });
        vd.field_validated_block("special_parameters", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.unknown_fields(|_, _| {});
        });
        vd.field_validated_block("traits", validate_traits);
    }
}
