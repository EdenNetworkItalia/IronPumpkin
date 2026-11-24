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
use pumpkin_data::data_component_impl::{CookingFuelImpl, FloatProvider, IntProvider};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::recipes::{CookingRecipeKind, RecipeCategoryTypes};
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
        experience: 0.7,
        result: OwnedRecipeResult {
            item_id: result.to_string(),
            count: 1,
            components: None,
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
    // 0.7 experience: no whole point, and one more point when the roll is under 0.7.
    assert_eq!(furnace.extract_experience_with(&mut || 0.5), 1);
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

/// The input gets the 200-tick default without a recipe. Lighting takes the cook time of the
/// recipe that a reload added, as vanilla `serverTick` does.
#[test]
fn lighting_takes_the_cook_time_of_a_recipe_that_a_reload_added() {
    let _serial = support::serial();
    support::server().recipe_manager.set_recipes(Vec::new());
    let world = support::world();
    let furnace = furnace_with(&world, 14);
    furnace.set_stack(0, ItemStack::new(1, &Item::DIRT));
    assert_eq!(furnace.get_cooking_total_time(), 200);
    install_recipes();
    furnace.set_stack(1, ItemStack::new(1, &Item::COAL));
    furnace.tick(&world);
    assert_eq!(i32::from(furnace.get_cooking_total_time()), COOKING_TIME);
    for _ in 1..COOKING_TIME - 1 {
        furnace.tick(&world);
    }
    assert!(furnace.get_stack(2).is_empty());
    furnace.tick(&world);
    assert_eq!(furnace.get_stack(2).item.id, Item::DIAMOND.id);
}

/// A furnace that goes out keeps its progress. Lighting again scales the progress to the cook
/// time of the recipe: 50 of 200 ticks is a quarter of the recipe time, rounded up.
#[test]
fn lighting_keeps_the_progress_ratio() {
    let _serial = support::serial();
    install_recipes();
    let world = support::world();
    let furnace = furnace_with(&world, 14);
    furnace.set_stack(0, ItemStack::new(1, &Item::DIRT));
    furnace.set_cooking_total_time(200);
    furnace.set_cooking_time_spent(50);
    furnace.set_stack(1, ItemStack::new(1, &Item::COAL));
    furnace.tick(&world);
    assert_eq!(i32::from(furnace.get_cooking_total_time()), COOKING_TIME);
    // The lit tick adds one tick of progress.
    let quarter = (COOKING_TIME as f32 / 4.0).ceil() as i32;
    assert_eq!(i32::from(furnace.get_cooking_time_spent()), quarter + 1);
    assert!(furnace.get_stack(2).is_empty());
}

#[test]
fn a_cooking_time_of_zero_cooks_every_tick() {
    let _serial = support::serial();
    let mut recipe = cooking("test:fast_dirt", "minecraft:dirt", "minecraft:diamond");
    recipe.cooking_time = 0;
    support::server()
        .recipe_manager
        .set_recipes(vec![DynamicRecipe::Cooking(
            OwnedCookingRecipeType::Smelting(recipe),
        )]);
    let world = support::world();
    let furnace = furnace_with(&world, 16);
    furnace.set_stack(0, ItemStack::new(3, &Item::DIRT));
    furnace.set_stack(1, ItemStack::new(1, &Item::COAL));
    for _ in 0..3 {
        furnace.tick(&world);
    }
    assert!(furnace.get_stack(0).is_empty());
    assert_eq!(furnace.get_stack(2).item_count, 3);
}

#[test]
fn a_campfire_drops_the_raw_item_when_a_reload_removes_its_recipe() {
    let _serial = support::serial();
    install_recipes();
    let world = support::world();
    let pos = BlockPos::new(18, 100, 4);
    place(&world, pos, &Block::CAMPFIRE);
    let campfire = CampfireBlockEntity::new(pos);
    *campfire.items[0].lock().unwrap() = ItemStack::new(1, &Item::STICK);
    campfire.tick(&world);
    assert_eq!(
        *campfire.cooking_total_times[0].lock().unwrap(),
        COOKING_TIME
    );
    support::server().recipe_manager.set_recipes(Vec::new());
    for _ in 0..COOKING_TIME {
        campfire.tick(&world);
    }
    assert!(campfire.items[0].lock().unwrap().is_empty());
    let area = BoundingBox {
        min: Vector3::new(17.0, 100.0, 3.0),
        max: Vector3::new(20.0, 103.0, 6.0),
    };
    let dropped_stick = world
        .get_entities_at_box(&area)
        .iter()
        .filter_map(|entity| entity.get_item_entity())
        .any(|item| {
            let mut id = 0;
            item.insert_into(|stack| {
                id = stack.item.id;
                stack
            });
            id == Item::STICK.id
        });
    assert!(dropped_stick, "the campfire drops the raw item");
}

/// Vanilla `AbstractFurnaceBlockEntity.createExperience` rounds the experience of each used
/// recipe on its own (`AbstractFurnaceBlockEntity.java:432-450`).
#[test]
fn a_furnace_rounds_the_experience_of_each_recipe_with_the_roll() {
    let _serial = support::serial();
    install_recipes();
    let furnace = FurnaceBlockEntity::new(BlockPos::new(40, 100, 0));
    let index = pumpkin_inventory::recipe_index::current();
    let dirt = index
        .cooking_recipe(&Item::DIRT, &CookingRecipeKind::Smelting)
        .unwrap();
    let iron = index
        .cooking_recipe(&Item::RAW_IRON, &CookingRecipeKind::Smelting)
        .unwrap();
    let use_recipes = || {
        for _ in 0..3 {
            furnace.add_recipe_used(dirt);
        }
        furnace.add_recipe_used(iron);
    };
    // 3 x 0.7 = 2.1 and 1 x 0.7: the floor of the total gives 2, each recipe can add 1.
    use_recipes();
    assert_eq!(furnace.extract_experience_with(&mut || 0.0), 4);
    assert_eq!(furnace.extract_experience_with(&mut || 0.0), 0);
    use_recipes();
    assert_eq!(furnace.extract_experience_with(&mut || 0.99), 2);
}

fn smelt_dirt(id: &str, result: &str) -> DynamicRecipe {
    DynamicRecipe::Cooking(OwnedCookingRecipeType::Smelting(cooking(
        id,
        "minecraft:dirt",
        result,
    )))
}

fn campfire_dirt(id: &str, result: &str) -> DynamicRecipe {
    DynamicRecipe::Cooking(OwnedCookingRecipeType::CampfireCooking(cooking(
        id,
        "minecraft:dirt",
        result,
    )))
}

/// Vanilla `quickCheck` keeps the last recipe while it takes the input. A reload that adds a
/// recipe that comes first in vanilla order does not change the result.
#[test]
fn a_furnace_keeps_its_last_recipe_when_a_reload_adds_an_earlier_one() {
    let _serial = support::serial();
    let manager = &support::server().recipe_manager;
    // `z:a` sorts before `a:zzz`: vanilla orders by the path first.
    manager.set_recipes(vec![smelt_dirt("a:zzz", "minecraft:emerald")]);
    let world = support::world();
    let furnace = furnace_with(&world, 42);
    furnace.set_stack(0, ItemStack::new(2, &Item::DIRT));
    furnace.set_stack(1, ItemStack::new(1, &Item::COAL));
    let cook_until = |count: u8| {
        for _ in 0..20 {
            if furnace.get_stack(2).item_count >= count {
                break;
            }
            furnace.tick(&world);
        }
    };
    cook_until(1);
    assert_eq!(furnace.get_stack(2).item.id, Item::EMERALD.id);
    manager.set_recipes(vec![
        smelt_dirt("a:zzz", "minecraft:emerald"),
        smelt_dirt("z:a", "minecraft:diamond"),
    ]);
    cook_until(2);
    let output = furnace.get_stack(2);
    assert_eq!(output.item.id, Item::EMERALD.id);
    assert_eq!(output.item_count, 2);
}

/// The ids of the items that item entities hold around `pos`.
fn dropped_items(world: &Arc<World>, pos: BlockPos) -> Vec<u16> {
    let center = pos.to_f64();
    let area = BoundingBox {
        min: Vector3::new(center.x - 1.0, center.y, center.z - 1.0),
        max: Vector3::new(center.x + 2.0, center.y + 3.0, center.z + 2.0),
    };
    world
        .get_entities_at_box(&area)
        .iter()
        .filter_map(|entity| entity.get_item_entity())
        .map(|item| {
            let mut id = 0;
            item.insert_into(|stack| {
                id = stack.item.id;
                stack
            });
            id
        })
        .collect()
}

/// The campfire keeps its last recipe as the furnace does (`CampfireBlockEntity.java:69`).
#[test]
fn a_campfire_keeps_its_last_recipe_when_a_reload_adds_an_earlier_one() {
    let _serial = support::serial();
    let manager = &support::server().recipe_manager;
    manager.set_recipes(vec![campfire_dirt("a:zzz", "minecraft:emerald")]);
    let world = support::world();
    let pos = BlockPos::new(50, 100, 0);
    place(&world, pos, &Block::CAMPFIRE);
    let campfire = CampfireBlockEntity::new(pos);
    let cook = || {
        *campfire.items[0].lock().unwrap() = ItemStack::new(1, &Item::DIRT);
        for _ in 0..20 {
            if campfire.items[0].lock().unwrap().is_empty() {
                break;
            }
            campfire.tick(&world);
        }
        assert!(campfire.items[0].lock().unwrap().is_empty());
    };
    cook();
    manager.set_recipes(vec![
        campfire_dirt("a:zzz", "minecraft:emerald"),
        campfire_dirt("z:a", "minecraft:diamond"),
    ]);
    cook();
    let dropped = dropped_items(&world, pos);
    assert_eq!(
        dropped.iter().filter(|id| **id == Item::EMERALD.id).count(),
        2,
        "{dropped:?}"
    );
    assert!(!dropped.contains(&Item::DIAMOND.id));
}

/// A coal item with the `cooking_fuel` component of a fuel that burns 100 ticks at `speed`.
fn fuel_with_speed(speed: f32) -> ItemStack {
    let mut fuel = ItemStack::new(1, &Item::COAL);
    fuel.set_data_component(CookingFuelImpl {
        burn_time: IntProvider::Inline(100),
        speed_multiplier: FloatProvider::Inline(speed),
    });
    fuel
}

/// Vanilla `getTotalCookTime` divides the cook time by the speed multiplier of the fuel that lit
/// the furnace. The furnace saves the multiplier as `speed_multiplier`.
#[test]
fn a_fuel_with_a_speed_multiplier_of_2_halves_the_cook_time() {
    let _serial = support::serial();
    let mut recipe = cooking("test:slow_dirt", "minecraft:dirt", "minecraft:diamond");
    recipe.cooking_time = 10;
    support::server()
        .recipe_manager
        .set_recipes(vec![DynamicRecipe::Cooking(
            OwnedCookingRecipeType::Smelting(recipe),
        )]);
    let world = support::world();
    let furnace = furnace_with(&world, 20);
    furnace.set_stack(0, ItemStack::new(2, &Item::DIRT));
    assert_eq!(furnace.get_cooking_total_time(), 10);
    furnace.set_stack(1, fuel_with_speed(2.0));
    furnace.tick(&world);
    assert_eq!(furnace.get_speed_multiplier(), 2.0);
    assert_eq!(furnace.get_cooking_total_time(), 5);
    for _ in 1..4 {
        furnace.tick(&world);
    }
    assert!(furnace.get_stack(2).is_empty());
    furnace.tick(&world);
    assert_eq!(furnace.get_stack(2).item_count, 1);
    // The next cook time uses the multiplier of the fuel that still burns.
    assert_eq!(furnace.get_cooking_total_time(), 5);

    // A furnace that loads from its save keeps the multiplier. An old save has none.
    let mut nbt = pumpkin_nbt::compound::NbtCompound::new();
    furnace.write_nbt(&mut nbt);
    assert_eq!(nbt.get_float("speed_multiplier"), Some(2.0));
    let loaded = FurnaceBlockEntity::from_nbt(&nbt, furnace.get_position());
    assert_eq!(loaded.get_speed_multiplier(), 2.0);
    loaded.set_stack(0, ItemStack::new(1, &Item::GRAVEL));
    loaded.set_stack(0, ItemStack::new(1, &Item::DIRT));
    assert_eq!(loaded.get_cooking_total_time(), 5);
    nbt.child_tags.remove("speed_multiplier");
    let old_save = FurnaceBlockEntity::from_nbt(&nbt, furnace.get_position());
    assert_eq!(old_save.get_speed_multiplier(), 1.0);
}

/// The default speed of a fuel is 2 in a blast furnace and a smoker, and the blasting recipe of
/// vanilla 26.3 takes 200 ticks.
#[test]
fn a_blast_furnace_halves_the_cook_time_of_a_vanilla_recipe() {
    let _serial = support::serial();
    support::server().recipe_manager.set_recipes(Vec::new());
    let world = support::world();
    let furnace = BlastingFurnaceBlockEntity::new(BlockPos::new(22, 100, 4));
    place(&world, furnace.get_position(), &Block::BLAST_FURNACE);
    furnace.set_stack(0, ItemStack::new(1, &Item::RAW_IRON));
    furnace.set_stack(1, ItemStack::new(1, &Item::COAL));
    assert_eq!(furnace.get_cooking_total_time(), 200);
    furnace.tick(&world);
    assert_eq!(furnace.get_cooking_total_time(), 100);
    assert_eq!(furnace.get_speed_multiplier(), 2.0);
}
