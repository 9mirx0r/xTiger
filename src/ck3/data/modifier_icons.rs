use crate::block::Block;
use crate::db::{Db, DbKind};
use crate::everything::Everything;
use crate::item::{Item, ItemLoader};
use crate::token::Token;
use crate::validator::Validator;

#[derive(Clone, Debug)]
pub struct ModifierIcon {}

inventory::submit! {
    ItemLoader::Normal(Item::ModifierIcon, ModifierIcon::add)
}

impl ModifierIcon {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::ModifierIcon, key, block, Box::new(Self {}));
    }
}

impl DbKind for ModifierIcon {
    fn validate(&self, _key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);
        vd.field_item("positive", Item::File);
        vd.field_item("negative", Item::File);
        vd.field_bool("default");
    }
}
