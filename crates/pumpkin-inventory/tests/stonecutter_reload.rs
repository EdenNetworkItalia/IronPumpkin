//! A reload while a stonecutter menu is open keeps the list of the menu, as vanilla
//! `StonecutterMenu` does: the list changes only when the input item changes. The recipe index
//! is process-wide, so this file is its own test binary.
use std::sync::{Arc, Mutex};

use pumpkin_data::dynamic::ContentIds;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_inventory::entity_equipment::EntityEquipment;
use pumpkin_inventory::inventory::Inventory;
use pumpkin_inventory::player::player_inventory::PlayerInventory;
use pumpkin_inventory::recipe_index;
use pumpkin_inventory::stonecutter_screen_handler::StonecutterScreenHandler;
use pumpkin_protocol::codec::recipe::{
    DynamicRecipe, OwnedRecipeIngredient, OwnedRecipeResult, OwnedStonecutterRecipe,
};

fn dirt_to_diamond() -> DynamicRecipe {
    DynamicRecipe::Stonecutting(OwnedStonecutterRecipe {
        recipe_id: "test:dirt_to_diamond".to_string(),
        group: None,
        ingredient: OwnedRecipeIngredient::Simple("minecraft:dirt".to_string()),
        result: OwnedRecipeResult {
            item_id: "minecraft:diamond".to_string(),
            count: 1,
            components: None,
        },
    })
}

#[test]
fn a_reload_keeps_the_list_until_the_input_item_changes() {
    let player_inventory = Arc::new(PlayerInventory::new(
        Arc::new(Mutex::new(EntityEquipment::new())),
        Arc::new(rustc_hash::FxHashMap::default()),
    ));
    let handler = StonecutterScreenHandler::new(1, &player_inventory, ContentIds::Real);
    recipe_index::install(&[dirt_to_diamond()]);
    handler
        .input_inventory
        .set_stack(0, ItemStack::new(1, &Item::DIRT));
    handler.update_output();

    recipe_index::install(&[]);
    handler.set_selected_recipe(0);
    assert_eq!(
        handler.output_inventory.get_stack(0).item.id,
        Item::DIAMOND.id
    );

    handler
        .input_inventory
        .set_stack(0, ItemStack::new(1, &Item::STONE));
    handler.set_selected_recipe(0);
    let result = handler.output_inventory.get_stack(0);
    assert!(!result.is_empty());
    assert_ne!(result.item.id, Item::DIAMOND.id);
}
