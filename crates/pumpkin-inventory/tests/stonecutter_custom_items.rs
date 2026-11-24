//! The stonecutter list of a client is the list that the server selects from. A vanilla client
//! sees a custom item as its display item, so it gets no recipe whose ingredient holds a custom
//! item. The content registry is process-wide and freezes once, so this file is its own test
//! binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::sync::Once;

use pumpkin_data::dynamic::{self, ContentIds, ItemDefinition};
use pumpkin_data::item::Item;
use pumpkin_inventory::recipe_index::RecipeIndex;
use pumpkin_protocol::codec::recipe::{
    DynamicRecipe, OwnedRecipeIngredient, OwnedRecipeResult, OwnedStonecutterRecipe,
};

fn ruby() -> &'static Item {
    static FREEZE: Once = Once::new();
    FREEZE.call_once(|| {
        dynamic::register_item(ItemDefinition {
            name: "test:ruby_stone".to_string(),
            display: &Item::STONE,
            block: None,
            tags: Vec::new(),
        })
        .unwrap();
        dynamic::freeze().unwrap();
    });
    Item::from_registry_key("test:ruby_stone").unwrap()
}

fn index() -> RecipeIndex {
    RecipeIndex::build(&[DynamicRecipe::Stonecutting(OwnedStonecutterRecipe {
        recipe_id: "test:ruby_slab".to_string(),
        group: None,
        ingredient: OwnedRecipeIngredient::Simple("test:ruby_stone".to_string()),
        result: OwnedRecipeResult {
            item_id: "minecraft:diamond".to_string(),
            count: 1,
        },
    })])
}

fn ids_for(index: &RecipeIndex, item: &Item, ids: ContentIds) -> Vec<String> {
    index
        .stonecutter_recipes_for(item, ids)
        .map(|r| r.recipe_id.clone())
        .collect()
}

#[test]
fn a_display_client_gets_no_recipe_of_a_custom_item() {
    let ruby = ruby();
    let index = index();
    let stone = ids_for(&index, &Item::STONE, ContentIds::Display);
    assert!(!stone.is_empty());
    assert!(!stone.iter().any(|id| id == "test:ruby_slab"));
    assert!(ids_for(&index, ruby, ContentIds::Display).is_empty());
    assert!(
        index
            .stonecutter_recipes(ContentIds::Display)
            .all(|r| r.recipe_id != "test:ruby_slab")
    );
}

#[test]
fn a_real_client_gets_the_recipe_of_a_custom_item() {
    let ruby = ruby();
    let index = index();
    assert_eq!(
        ids_for(&index, ruby, ContentIds::Real),
        ["test:ruby_slab".to_string()]
    );
    assert_eq!(
        ids_for(&index, &Item::STONE, ContentIds::Real),
        ids_for(&index, &Item::STONE, ContentIds::Display)
    );
}
