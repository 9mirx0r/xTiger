use crate::block::Block;
use crate::context::ScopeContext;
use crate::db::{Db, DbKind};
use crate::everything::Everything;
use crate::item::{Item, ItemLoader};
use crate::scopes::Scopes;
use crate::token::Token;
use crate::tooltipped::Tooltipped;
use crate::validator::Validator;

#[derive(Clone, Debug)]
pub struct LeaseContract {}

inventory::submit! {
    ItemLoader::Normal(Item::LeaseContract, LeaseContract::add)
}

impl LeaseContract {
    pub fn add(db: &mut Db, key: Token, block: Block) {
        db.add(Item::LeaseContract, key, block, Box::new(Self {}));
    }
}

impl DbKind for LeaseContract {
    fn validate(&self, key: &Token, block: &Block, data: &Everything) {
        let mut vd = Validator::new(block, data);
        let has_hierarchy = block.has_key("hierarchy");
        if key.is("theocracy_lease") {
            vd.req_field("hierarchy");
        }
        vd.field_validated_block("hierarchy", |block, data| {
            let mut vd = Validator::new(block, data);
            vd.field_choice("type", &["vassal", "clerical_region"]);
            vd.field_trigger_rooted("ruler_valid", Tooltipped::No, Scopes::Character);
            vd.field_trigger_builder("liege_or_vassal_valid", Tooltipped::No, |key| {
                let mut sc = ScopeContext::new(Scopes::Character, key);
                sc.define_name("target", Scopes::Character, key);
                sc
            });
            vd.field_trigger_rooted("barony_valid", Tooltipped::No, Scopes::LandedTitle);
            let mut sc = ScopeContext::new(Scopes::Character, key);
            vd.field_target("lessee", &mut sc, Scopes::Character);
        });

        vd.field_item("government", Item::GovernmentType); // undocumented
        vd.field_list_items("valid_holdings", Item::HoldingType);
        vd.field_integer("ruler_share_min_opinion_from_lessee");
        vd.field_choice("hook_strength_max_opinion", &["none", "any", "strong"]);

        for field in &["tax_split", "levy_split"] {
            vd.field_validated_block(field, |block, data| {
                let mut vd = Validator::new(block, data);
                for share in &["lease_liege", "top_lease_liege_direct", "ruler"] {
                    if !has_hierarchy && *share != "ruler" {
                        vd.ban_field(share, || "lease contracts with a `hierarchy`");
                        continue;
                    }
                    // The docs list `lessee` plus one other scope per share, but vanilla also uses
                    // `lease_liege` inside `top_lease_liege_direct`, so all of them are allowed.
                    let mut sc = ScopeContext::new(Scopes::Character, key);
                    for name in &["lessee", "lease_liege", "top_lease_liege", "ruler"] {
                        sc.define_name(name, Scopes::Character, key);
                    }
                    vd.field_script_value(share, &mut sc);
                    vd.field_numeric_range(&format!("{share}_max"), 0.0..=1.0);
                }
            });
        }
        for field in &["tax", "levy"] {
            vd.replaced_field(field, &format!("{field}_split"));
        }
    }
}
