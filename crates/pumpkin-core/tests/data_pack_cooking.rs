//! Data pack cooking recipes cook in the furnace, the blast furnace, the smoker and the campfire,
//! as vanilla `AbstractFurnaceBlockEntity` and `CampfireBlockEntity` read them from the
//! `RecipeManager`.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first poisoned lock"
)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use pumpkin_core::block::entities::BlockEntity;
use pumpkin_core::block::entities::blasting_furnace::BlastingFurnaceBlockEntity;
use pumpkin_core::block::entities::campfire::CampfireBlockEntity;
use pumpkin_core::block::entities::furnace::FurnaceBlockEntity;
use pumpkin_core::block::entities::furnace_like_block_entity::CookingBlockEntityBase;
use pumpkin_core::block::entities::smoker::SmokerBlockEntity;
use pumpkin_core::world::World;
use pumpkin_data::Block;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::recipes::RecipeCategoryTypes;
use pumpkin_inventory::Inventory;
use pumpkin_protocol::codec::recipe::{
    DynamicRecipe, OwnedCookingRecipe, OwnedCookingRecipeType, OwnedRecipeIngredient,
    OwnedRecipeResult,
};
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::world::BlockFlags;

const COOKING_TIME: i32 = 3;

fn cooking(id: &str, ingredient: &str, result: &str) -> OwnedCookingRecipe {
    OwnedCookingRecipe {
        recipe_id: id.to_string(),
        category: RecipeCategoryTypes::Misc,
        group: None,
        ingredient: OwnedRecipeIngredient::Simple(ingredient.to_string()),
        cooking_time: COOKING_TIME,
        experience: 2.0,
        result: OwnedRecipeResult {
            item_id: result.to_string(),
            count: 1,
        },
    }
}

/// Installs one data pack recipe per block, and a recipe that replaces the vanilla raw iron
/// smelting recipe by its id.
fn install_recipes() {
    support::server().recipe_manager.set_recipes(vec![
        DynamicRecipe::Cooking(OwnedCookingRecipeType::Smelting(cooking(
            "test:diamond_from_dirt",
            "minecraft:dirt",
            "minecraft:diamond",
        ))),
        DynamicRecipe::Cooking(OwnedCookingRecipeType::Smelting(cooking(
            "minecraft:iron_ingot_from_smelting_raw_iron",
            "minecraft:raw_iron",
            "minecraft:emerald",
        ))),
        DynamicRecipe::Cooking(OwnedCookingRecipeType::Blasting(cooking(
            "test:emerald_from_gravel",
            "minecraft:gravel",
            "minecraft:emerald",
        ))),
        DynamicRecipe::Cooking(OwnedCookingRecipeType::Smoking(cooking(
            "test:gold_from_sand",
            "minecraft:sand",
            "minecraft:gold_ingot",
        ))),
        DynamicRecipe::Cooking(OwnedCookingRecipeType::CampfireCooking(cooking(
            "test:diamond_from_stick",
            "minecraft:stick",
            "minecraft:diamond",
        ))),
    ]);
}

/// Loads the chunk of `pos` for the rest of the process and places `block` there.
fn place(world: &Arc<World>, pos: BlockPos, block: &Block) {
    let chunk = pos.chunk_position();
    {
        let mut loading = world.level.chunk_loading.lock().unwrap();
        loading.add_force_ticket(chunk);
        loading.send_change();
    };
    let deadline = Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&chunk) {
        assert!(Instant::now() < deadline, "the chunk of {pos} did not load");
        std::thread::sleep(Duration::from_millis(10));
    }
    world.set_block_state(&pos, block.default_state.id, BlockFlags::empty());
    assert_eq!(world.get_block(&pos).id, block.id);
}

/// Places `block` at `pos`, fills the furnace-like `entity` and ticks it until it cooks once.
fn cook<E: CookingBlockEntityBase>(
    world: &Arc<World>,
    entity: &E,
    block: &Block,
    input: &'static Item,
) -> ItemStack {
    place(world, entity.get_position(), block);
    entity.set_stack(0, ItemStack::new(1, input));
    entity.set_stack(1, ItemStack::new(1, &Item::COAL));
    assert_eq!(i32::from(entity.get_cooking_total_time()), COOKING_TIME);
    for _ in 0..20 {
        entity.tick(world);
        if !entity.get_stack(2).is_empty() {
            break;
        }
    }
    entity.get_stack(2)
}

#[test]
fn a_furnace_cooks_a_data_pack_recipe() {
    let _serial = support::serial();
    install_recipes();
    let world = support::world();
    let furnace = FurnaceBlockEntity::new(BlockPos::new(0, 100, 0));
    let output = cook(&world, &furnace, &Block::FURNACE, &Item::DIRT);
    assert_eq!(output.item.id, Item::DIAMOND.id);
    assert_eq!(furnace.extract_experience_from_recipes(), 2);
}

#[test]
fn a_furnace_uses_the_data_pack_recipe_that_replaces_a_vanilla_recipe() {
    let _serial = support::serial();
    install_recipes();
    let world = support::world();
    let furnace = FurnaceBlockEntity::new(BlockPos::new(2, 100, 0));
    let output = cook(&world, &furnace, &Block::FURNACE, &Item::RAW_IRON);
    assert_eq!(output.item.id, Item::EMERALD.id);
}

#[test]
fn a_blast_furnace_cooks_a_data_pack_recipe() {
    let _serial = support::serial();
    install_recipes();
    let world = support::world();
    let furnace = BlastingFurnaceBlockEntity::new(BlockPos::new(4, 100, 0));
    let output = cook(&world, &furnace, &Block::BLAST_FURNACE, &Item::GRAVEL);
    assert_eq!(output.item.id, Item::EMERALD.id);
}

#[test]
fn a_smoker_cooks_a_data_pack_recipe() {
    let _serial = support::serial();
    install_recipes();
    let world = support::world();
    let smoker = SmokerBlockEntity::new(BlockPos::new(6, 100, 0));
    let output = cook(&world, &smoker, &Block::SMOKER, &Item::SAND);
    assert_eq!(output.item.id, Item::GOLD_INGOT.id);
}

#[test]
fn a_furnace_does_not_cook_a_recipe_of_another_block() {
    let _serial = support::serial();
    install_recipes();
    let world = support::world();
    let furnace = FurnaceBlockEntity::new(BlockPos::new(8, 100, 0));
    place(&world, furnace.get_position(), &Block::FURNACE);
    furnace.set_stack(0, ItemStack::new(1, &Item::SAND));
    furnace.set_stack(1, ItemStack::new(1, &Item::COAL));
    for _ in 0..20 {
        furnace.tick(&world);
    }
    assert!(furnace.get_stack(2).is_empty());
    assert_eq!(furnace.get_stack(0).item.id, Item::SAND.id);
}

#[test]
fn a_campfire_cooks_a_data_pack_recipe() {
    let _serial = support::serial();
    install_recipes();
    let world = support::world();
    let pos = BlockPos::new(10, 100, 0);
    place(&world, pos, &Block::CAMPFIRE);
    let campfire = CampfireBlockEntity::new(pos);
    *campfire.items[0].lock().unwrap() = ItemStack::new(1, &Item::STICK);
    for _ in 0..20 {
        campfire.tick(&world);
        if campfire.items[0].lock().unwrap().is_empty() {
            break;
        }
    }
    assert!(campfire.items[0].lock().unwrap().is_empty());
    let area = BoundingBox {
        min: Vector3::new(9.0, 100.0, -1.0),
        max: Vector3::new(12.0, 103.0, 2.0),
    };
    let cooked = world
        .get_entities_at_box(&area)
        .iter()
        .filter_map(|entity| entity.get_item_entity())
        .any(|item| {
            let mut id = 0;
            item.insert_into(|stack| {
                id = stack.item.id;
                stack
            });
            id == Item::DIAMOND.id
        });
    assert!(cooked, "the campfire drops the data pack result");
}

fn furnace_with(world: &Arc<World>, x: i32) -> FurnaceBlockEntity {
    let furnace = FurnaceBlockEntity::new(BlockPos::new(x, 100, 4));
    place(world, furnace.get_position(), &Block::FURNACE);
    furnace
}

#[test]
fn a_furnace_adds_the_result_count_and_stops_at_the_max_stack_size() {
    let _serial = support::serial();
    let mut recipe = cooking("test:two_stone", "minecraft:dirt", "minecraft:stone");
    recipe.result.count = 2;
    recipe.cooking_time = 1;
    support::server()
        .recipe_manager
        .set_recipes(vec![DynamicRecipe::Cooking(
            OwnedCookingRecipeType::Smelting(recipe),
        )]);
    let world = support::world();
    let furnace = furnace_with(&world, 12);
    furnace.set_stack(0, ItemStack::new(2, &Item::DIRT));
    furnace.set_stack(1, ItemStack::new(1, &Item::COAL));
    for _ in 0..10 {
        furnace.tick(&world);
    }
    assert!(furnace.get_stack(0).is_empty());
    assert_eq!(furnace.get_stack(2).item_count, 4);

    // 63 + 2 is more than a stack, so vanilla `canBurn` keeps the input.
    furnace.set_stack(0, ItemStack::new(1, &Item::DIRT));
    furnace.set_stack(2, ItemStack::new(63, &Item::STONE));
    for _ in 0..10 {
        furnace.tick(&world);
    }
    assert_eq!(furnace.get_stack(0).item_count, 1);
    assert_eq!(furnace.get_stack(2).item_count, 63);
}
