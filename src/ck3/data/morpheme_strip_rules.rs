use crate::block::Block;
use crate::db::{Db, DbKind};
use crate::everything::Everything;
use crate::item::{Item, ItemLoader};
use crate::report::{ErrorKey, err};
use crate::token::Token;
use crate::validator::Validator;

#[derive(Clone, Debug)]
pub struct MorphemeStripRule {}

inventory::submit! {
    ItemLoader::Normal(Item::MorphemeStripRule, MorphemeStripRule::add)
}

impl MorphemeStripRule {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::MorphemeStripRule, key, block, Box::new(Self {}));
    }
}

impl DbKind for MorphemeStripRule {
    fn validate(&self, _key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);
        vd.field_choice("seek_from", &["head", "tail"]);
        vd.multi_field_validated_block("rule", |block, data| {
            let mut vd = Validator::new(block, data);
            // Match lists are ordered from the selected end towards the string's interior.
            vd.multi_field_validated_list("match", |token, _| {
                if token.as_str().chars().count() != 1 {
                    err(ErrorKey::Validation)
                        .msg("expected a single UTF-8 character in morpheme match list")
                        .loc(token)
                        .push();
                }
            });
            vd.field_integer_range("strip", 0..);
        });
    }
}
