//! Character DNA for portraits.

use crate::block::Block;
use crate::db::{Db, DbKind};
use crate::everything::Everything;
use crate::item::{Item, ItemLoader};
use crate::token::Token;
use crate::validator::Validator;

#[derive(Clone, Debug)]
pub struct Dna {}

inventory::submit! {
    ItemLoader::Normal(Item::Dna, Dna::add)
}

impl Dna {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::Dna, key, block, Box::new(Self {}));
    }
}

impl DbKind for Dna {
    fn validate(&self, _key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);

        vd.field_validated_block("portrait_info", validate_portrait_info);
        vd.field_bool("enabled");
        reject_pre_1_20_fields(&mut vd);
    }
}

/// `override` and `entity` were valid in older versions. In 1.20 the parser fails on them
/// ("Unexpected token: override").
fn reject_pre_1_20_fields(vd: &mut Validator) {
    for name in ["override", "entity"] {
        vd.fatal_in_game_field(name, "CK3 1.20 logs a parse error on it");
    }
}

fn validate_portrait_info(block: &Block, data: &Everything) {
    let mut vd = Validator::new(block, data);
    reject_pre_1_20_fields(&mut vd);
    vd.field_validated_block("genes", validate_genes);
    vd.field_choice("type", &["male", "female", "boy", "girl"]);
    vd.field_value("id");
}

pub fn validate_genes(block: &Block, data: &Everything) {
    let mut vd = Validator::new(block, data);
    vd.unknown_block_fields(|key, block| {
        data.verify_exists(Item::GeneCategory, key);
        data.validate_use(Item::GeneCategory, key, block);
    });
}
