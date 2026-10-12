use crate::block::Block;
use crate::db::{Db, DbKind};
use crate::everything::Everything;
use crate::item::{Item, ItemLoader};
use crate::token::Token;
use crate::validator::Validator;

#[derive(Clone, Debug)]
pub struct ConsoleGroup {}

inventory::submit! {
    ItemLoader::Normal(Item::ConsoleGroup, ConsoleGroup::add)
}

impl ConsoleGroup {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::ConsoleGroup, key, block, Box::new(Self {}));
    }
}

impl DbKind for ConsoleGroup {
    fn validate(&self, _key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);
        // Entries are bare console commands or quoted labels followed by a command.
        // Their contents are console text, rather than database references or script fields.
        vd.values();
    }
}
