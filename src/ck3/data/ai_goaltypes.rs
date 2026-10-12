use crate::block::Block;
use crate::db::{Db, DbKind};
use crate::everything::Everything;
use crate::item::{Item, ItemLoader};
use crate::report::Severity;
use crate::token::Token;
use crate::validator::Validator;

#[derive(Clone, Debug)]
pub struct AiGoalType {}

inventory::submit! {
    ItemLoader::Normal(Item::AiGoalType, AiGoalType::add)
}

impl AiGoalType {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::AiGoalType, key, block, Box::new(Self {}));
    }
}

impl DbKind for AiGoalType {
    fn validate(&self, _key: &Token, block: &Block, data: &Everything) {
        // The shipped definition is key-only; undocumented contents remain advisory.
        let mut vd = Validator::new(block, data);
        vd.set_max_severity(Severity::Warning);
    }
}
