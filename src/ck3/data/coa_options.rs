use crate::block::Block;
use crate::db::{Db, DbKind};
use crate::everything::Everything;
use crate::item::{Item, ItemLoader, LoadAsFile, Recursive};
use crate::pdxfile::PdxEncoding;
use crate::report::{ErrorKey, err};
use crate::token::Token;
use crate::validator::Validator;

#[derive(Clone, Debug)]
pub struct CoaOptions {}

inventory::submit! {
    ItemLoader::Full(Item::CoaOptions, PdxEncoding::Utf8OptionalBom, ".txt", LoadAsFile::Yes, Recursive::No, CoaOptions::add)
}

impl CoaOptions {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::CoaOptions, key, block, Box::new(Self {}));
    }
}

impl DbKind for CoaOptions {
    fn validate(&self, _key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);
        // CK3 loads only the canonical atlases.txt file. Its atlas blocks repeat rather than
        // defining unique database keys.
        vd.multi_field_validated_block("atlas", validate_atlas);
    }
}

fn validate_atlas(block: &Block, data: &Everything) {
    let mut vd = Validator::new(block, data);
    vd.field_list_integers_exactly("tile_size", 2);
    vd.field_integer("nr_of_tiles");
    vd.field_validated_block("actual_size", validate_sizes);
    vd.field_validated_block("fallback", validate_sizes);
}

fn validate_sizes(block: &Block, data: &Everything) {
    let mut vd = Validator::new(block, data);
    let sizes = vd.blocks();
    // The game requires one size pair for each of its four texture quality options.
    if sizes.len() != 4 {
        err(ErrorKey::Validation).msg("expected 4 texture quality size pairs").loc(block).push();
    }
    for size in sizes {
        let mut vd = Validator::new(size, data);
        vd.req_tokens_integers_exactly(2);
    }
}
