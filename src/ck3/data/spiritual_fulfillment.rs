use crate::block::Block;
use crate::ck3::modif::ModifKinds;
use crate::db::{Db, DbKind};
use crate::everything::Everything;
use crate::item::{Item, ItemLoader};
use crate::modif::validate_modifs;
use crate::token::Token;
use crate::validator::Validator;

/// Loaded from `common/spiritual_fulfillment` (1.20+)
#[derive(Clone, Debug)]
pub struct SpiritualFulfillmentType {}

inventory::submit! {
    ItemLoader::Normal(Item::SpiritualFulfillmentType, SpiritualFulfillmentType::add)
}

impl SpiritualFulfillmentType {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::SpiritualFulfillmentType, key, block, Box::new(Self {}));
    }
}

impl DbKind for SpiritualFulfillmentType {
    fn add_subitems(&self, _key: &Token, block: &Block, db: &mut Db) {
        for level in &block.get_field_blocks("level") {
            for flags in &level.get_field_blocks("flags") {
                for flag in flags.iter_values() {
                    db.add_flag(Item::SpiritualFulfillmentFlag, flag.clone());
                }
            }
        }
    }

    fn validate(&self, _key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);
        vd.field_list_items("religions", Item::Religion);
        vd.multi_field_validated_block("level", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.field_numeric("threshold");
            vd.field_validated_block("modifier", |block, data| {
                let vd = Validator::new(block, data);
                validate_modifs(block, data, ModifKinds::Character, vd);
            });
            vd.field_item("icon", Item::File);
            vd.field_list("flags");
        });
    }
}
