use crate::block::Block;
use crate::context::ScopeContext;
use crate::db::{Db, DbKind};
use crate::everything::Everything;
use crate::item::{Item, ItemLoader};
use crate::scopes::Scopes;
use crate::token::Token;
use crate::tooltipped::Tooltipped;
use crate::validate::validate_possibly_named_color;
use crate::validator::Validator;

/// Loaded from `common/puppets/types` (1.20+)
#[derive(Clone, Debug)]
pub struct PuppetType {}
/// Loaded from `common/puppets/actions` (1.20+)
#[derive(Clone, Debug)]
pub struct PuppetAction {}

inventory::submit! {
    ItemLoader::Normal(Item::PuppetType, PuppetType::add)
}
inventory::submit! {
    ItemLoader::Normal(Item::PuppetAction, PuppetAction::add)
}

impl PuppetType {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::PuppetType, key, block, Box::new(Self {}));
    }
}

impl PuppetAction {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::PuppetAction, key, block, Box::new(Self {}));
    }
}

/// Root is the puppeteer, and `scope:puppet` is the puppet.
fn puppet_sc(key: &Token) -> ScopeContext {
    let mut sc = ScopeContext::new(Scopes::Character, key);
    sc.define_name("puppet", Scopes::Character, key);
    sc
}

impl DbKind for PuppetType {
    fn validate(&self, _key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);
        vd.field_validated("color", validate_possibly_named_color);
        vd.field_item("interaction", Item::CharacterInteraction);
        vd.field_trigger_rooted("can_have", Tooltipped::No, Scopes::Character);
        vd.field_trigger_builder("is_valid", Tooltipped::No, puppet_sc);
        vd.field_script_value_rooted("priority", Scopes::Character);
        vd.field_list_items("actions", Item::PuppetAction);
        vd.multi_field_validated_block("asset", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.field_trigger_builder("trigger", Tooltipped::No, puppet_sc);
            vd.field_item("icon", Item::File);
            vd.field_item("background", Item::File);
        });
        vd.multi_field_validated_block("animation", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.field_trigger_builder("trigger", Tooltipped::No, puppet_sc);
            vd.field_item("reference", Item::PortraitAnimation);
        });
    }
}

impl DbKind for PuppetAction {
    fn validate(&self, key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);

        // Only the important actions are shown in the UI, so only they need a description.
        if block.get_field_bool("is_important").unwrap_or(false) {
            let loca = format!("puppet_action_description_{key}");
            data.verify_exists_implied(Item::Localization, &loca, key);
        }

        let kind = block.get_field_value("type").map_or("character_interaction", Token::as_str);
        vd.field_choice("type", &["character_interaction", "decision", "great_project", "trigger"]);
        match kind {
            "character_interaction" => {
                vd.req_field("interaction");
            }
            "decision" => {
                vd.req_field("decision");
            }
            "great_project" => {
                vd.req_field("great_project");
            }
            _ => (),
        }
        vd.field_item("interaction", Item::CharacterInteraction);
        vd.field_item("decision", Item::Decision);
        vd.field_item("great_project", Item::GreatProjectType);
        vd.field_trigger_builder("is_enabled", Tooltipped::Yes, puppet_sc);
        vd.field_bool("is_important");
    }
}
