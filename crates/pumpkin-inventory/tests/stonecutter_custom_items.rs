//! The stonecutter list of a client is the list that the server selects from. A vanilla client
//! sees a custom item as its display item, so the server selects by the display item. The content registry is process-wide and freezes once, so this file is its own test
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

fn stonecutting(id: &str, ingredient: OwnedRecipeIngredient) -> DynamicRecipe {
    DynamicRecipe::Stonecutting(OwnedStonecutterRecipe {
        recipe_id: id.to_string(),
        group: None,
        ingredient,
        result: OwnedRecipeResult {
            item_id: "minecraft:diamond".to_string(),
            count: 1,
            components: None,
        },
    })
}

/// A recipe of the custom item only, and a recipe of the custom item and dirt.
fn index() -> RecipeIndex {
    RecipeIndex::build(&[
        stonecutting(
            "test:ruby_slab",
            OwnedRecipeIngredient::Simple("test:ruby_stone".to_string()),
        ),
        stonecutting(
            "test:ruby_or_dirt",
            OwnedRecipeIngredient::OneOf(vec![
                "test:ruby_stone".to_string(),
                "minecraft:dirt".to_string(),
            ]),
        ),
    ])
}

fn ids_for(index: &RecipeIndex, item: &Item, ids: ContentIds) -> Vec<String> {
    index
        .stonecutter_recipes_for(item, ids)
        .map(|r| r.recipe_id.clone())
        .collect()
}

#[test]
fn a_display_client_gets_no_recipe_of_a_custom_item() {
    ruby();
    let index = index();
    let stone = ids_for(&index, &Item::STONE, ContentIds::Display);
    assert!(!stone.is_empty());
    assert!(!stone.iter().any(|id| id == "test:ruby_slab"));
    assert!(
        index
            .stonecutter_recipes(ContentIds::Display)
            .all(|r| r.recipe_id != "test:ruby_slab")
    );
}

/// The client sees the custom item as stone and shows the buttons of stone. The server selects
/// the same list, so each button index selects the recipe that the client shows.
#[test]
fn a_display_client_selects_by_the_display_item() {
    let ruby = ruby();
    let index = index();
    assert_eq!(
        ids_for(&index, ruby, ContentIds::Display),
        ids_for(&index, &Item::STONE, ContentIds::Display)
    );
}

#[test]
fn a_real_client_gets_the_recipe_of_a_custom_item() {
    let ruby = ruby();
    let index = index();
    assert_eq!(
        ids_for(&index, ruby, ContentIds::Real),
        [
            "test:ruby_or_dirt".to_string(),
            "test:ruby_slab".to_string()
        ]
    );
    assert_eq!(
        ids_for(&index, &Item::STONE, ContentIds::Real),
        ids_for(&index, &Item::STONE, ContentIds::Display)
    );
}
