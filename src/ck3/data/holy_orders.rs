use crate::block::Block;
use crate::context::ScopeContext;
use crate::db::{Db, DbKind};
use crate::everything::Everything;
use crate::item::{Item, ItemLoader};
use crate::scopes::Scopes;
use crate::token::Token;
use crate::tooltipped::Tooltipped;
use crate::validator::Validator;

/// A weighted creation template, distinct from the runtime holy-order scope.
#[derive(Clone, Debug)]
pub struct HolyOrderType {}

inventory::submit! {
    ItemLoader::Normal(Item::HolyOrderType, HolyOrderType::add)
}

impl HolyOrderType {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::HolyOrderType, key, block, Box::new(Self {}));
    }
}

/// Selection happens before the title and holy order have been created.
fn selection_context(key: &Token) -> ScopeContext {
    let mut sc = ScopeContext::new(Scopes::None, key);
    sc.define_name("founder", Scopes::Character, key);
    sc.define_name("leader", Scopes::Character, key);
    sc.define_name("province", Scopes::Province, key);
    sc
}

/// The post-creation effect runs on the assigned title, not the holy order.
fn effect_context(key: &Token) -> ScopeContext {
    let mut sc = ScopeContext::new(Scopes::LandedTitle, key);
    sc.define_name("founder", Scopes::Character, key);
    sc.define_name("leader", Scopes::Character, key);
    sc.define_name("province", Scopes::Province, key);
    sc.define_name("title", Scopes::LandedTitle, key);
    // Actual creation exposes new_holy_order; the .info name holy_order is inaccurate.
    sc.define_name("new_holy_order", Scopes::HolyOrder, key);
    sc
}

fn name_context(key: &Token) -> ScopeContext {
    let mut sc = ScopeContext::new(Scopes::None, key);
    sc.define_name("FOUNDER", Scopes::Character, key);
    sc.define_name("LEADER", Scopes::Character, key);
    sc.define_name("PROVINCE", Scopes::Province, key);
    sc.define_name("TITLE", Scopes::LandedTitle, key);
    sc.define_name("FAITH", Scopes::Faith, key);
    sc
}

impl DbKind for HolyOrderType {
    fn validate(&self, key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);
        // CK3 1.20 accepts only military and monastic, despite the broader .info list.
        vd.req_field("type");
        vd.req_field("government");
        vd.field_choice("type", &["military", "monastic"]);
        vd.field_item("government", Item::GovernmentType);
        vd.field_item("title", Item::Title);
        vd.field_item("coat_of_arms", Item::Coa);
        vd.field_item("tenet", Item::Tenet);
        vd.field_list_items("men_at_arms", Item::MenAtArms);

        let mut sc = name_context(key);
        if let Some(name) = vd.field_value("name").filter(|name| !name.as_str().is_empty()) {
            data.verify_exists(Item::Localization, name);
            data.validate_localization_sc(name.as_str(), &mut sc);
        } else if !block.has_key("title") {
            // A scripted title supplies its own name; otherwise the definition key is used.
            data.verify_exists(Item::Localization, key);
            data.validate_localization_sc(key.as_str(), &mut sc);
        }

        vd.field_trigger_builder("trigger", Tooltipped::No, selection_context);
        vd.field_script_value_builder("weight", selection_context);
        vd.field_effect_builder("effect", Tooltipped::No, effect_context);
    }
}
