use crate::block::Block;
use crate::db::{Db, DbKind};
use crate::everything::Everything;
use crate::item::{Item, ItemLoader};
use crate::token::Token;
use crate::validator::Validator;

#[derive(Clone, Debug)]
pub struct Dlc {}

inventory::submit! {
    ItemLoader::Normal(Item::Dlc, Dlc::add)
}

impl Dlc {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::Dlc, key, block, Box::new(Self {}));
    }
}

impl DbKind for Dlc {
    fn add_subitems(&self, _key: &Token, block: &Block, db: &mut Db) {
        let field = "key";
        if let Some(name) = block.get_field_value(field) {
            db.add_flag(Item::DlcName, name.clone());
        }
    }

    fn validate(&self, key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);

        {
            data.verify_exists(Item::Localization, key);
            let loca = format!("{key}_desc");
            data.verify_exists_implied(Item::Localization, &loca, key);
        }

        {
            let path = format!("gfx/interface/illustrations/dlc_event_decorations/{key}.dds");
            data.verify_exists_implied(Item::File, &path, key);
            let path = format!("gfx/interface/icons/dlc/{key}.dds");
            data.verify_exists_implied(Item::File, &path, key);
        }

        vd.req_field("key");
        vd.field_value("key");
        vd.field_choice("type", &["minor", "medium", "major"]);
        vd.field_integer("priority");

        vd.field_value("steam_id");
        vd.field_value("msgr_id");

        // Documented but not used
        vd.field_list_items("features", Item::Localization);
    }
}
