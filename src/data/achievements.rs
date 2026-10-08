use crate::block::Block;
use crate::db::{Db, DbKind};
use crate::everything::Everything;
use crate::item::{Item, ItemLoader};
use crate::report::{ErrorKey, err};
use crate::scopes::Scopes;
use crate::token::Token;
use crate::tooltipped::Tooltipped;
use crate::validator::Validator;

#[derive(Clone, Debug)]
pub struct Achievement {}

inventory::submit! {
    ItemLoader::Normal(Item::Achievement, Achievement::add)
}

impl Achievement {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::Achievement, key, block, Box::new(Self {}));
    }
}

impl DbKind for Achievement {
    fn validate(&self, key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);

        {
            let loca = format!("ACHIEVEMENT_{key}");
            data.verify_exists_implied(Item::Localization, &loca, key);
            let loca = format!("ACHIEVEMENT_DESC_{key}");
            data.verify_exists_implied(Item::Localization, &loca, key);
        }

        vd.field_trigger_rooted("possible", Tooltipped::No, achievement_scope());
        vd.field_trigger_rooted("happened", Tooltipped::No, achievement_scope());
    }
}

fn achievement_scope() -> Scopes {
    Scopes::Character
}

#[derive(Clone, Debug)]
pub struct AchievementGroup {}

inventory::submit! {
    ItemLoader::Normal(Item::AchievementGroup, AchievementGroup::add)
}

impl AchievementGroup {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        if key.is("group") {
            if let Some(name) = block.get_field_value("name").cloned() {
                db.add(Item::AchievementGroup, name, block, Box::new(Self {}));
            } else {
                let msg = "group missing `name` field";
                err(ErrorKey::FieldMissing).msg(msg).loc(key).push();
            }
        } else {
            let msg = "unknown key in achievement groups";
            let info = "expected only `group`";
            err(ErrorKey::UnknownField).msg(msg).info(info).loc(key).push();
        }
    }
}

impl DbKind for AchievementGroup {
    fn validate(&self, key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);

        let loca = format!("ACHIEVEMENT_GROUP_{key}");
        data.verify_exists_implied(Item::Localization, &loca, key);

        vd.field_value("name");
        vd.field_list_items("order", Item::Achievement);
    }
}
