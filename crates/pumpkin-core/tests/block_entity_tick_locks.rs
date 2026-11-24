//! Block entities tick in parallel batches, so a hopper on another thread can hold the items lock
//! of a block entity while that block entity ticks. Vanilla ticks block entities one after
//! another (`Level.tickBlockEntities`): a tick never sees a held lock and never loses progress.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first poisoned lock"
)]

mod support;

use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use pumpkin_core::block::entities::BlockEntity;
use pumpkin_core::block::entities::furnace::FurnaceBlockEntity;
use pumpkin_core::block::entities::furnace_like_block_entity::CookingBlockEntityBase;
use pumpkin_core::world::World;
use pumpkin_data::Block;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::recipes::{CookingRecipeKind, RecipeCategoryTypes};
use pumpkin_inventory::Inventory;
use pumpkin_protocol::codec::recipe::{
    DynamicRecipe, OwnedCookingRecipe, OwnedCookingRecipeType, OwnedRecipeIngredient,
    OwnedRecipeResult,
};
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;

const COOKING_TIME: i32 = 4;

/// How long the push holds the items lock. A tick that does not wait for the lock starts and
/// ends inside this time.
const HOLD: Duration = Duration::from_millis(100);

fn cooking(ingredient: &str) -> OwnedCookingRecipe {
    OwnedCookingRecipe {
        recipe_id: format!(
            "test:diamond_from_{}",
            ingredient.trim_start_matches("minecraft:")
        ),
        category: RecipeCategoryTypes::Misc,
        group: None,
        ingredient: OwnedRecipeIngredient::Simple(ingredient.to_string()),
        cooking_time: COOKING_TIME,
        experience: 0.0,
        result: OwnedRecipeResult {
            item_id: "minecraft:diamond".to_string(),
            count: 1,
            components: None,
        },
    }
}

fn install_recipes() {
    support::server()
        .recipe_manager
        .set_recipes(vec![DynamicRecipe::Cooking(
            OwnedCookingRecipeType::Smelting(cooking("minecraft:dirt")),
        )]);
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

/// Runs `action` while another thread pushes one `item` into `slot` of `inventory`. The push
/// holds the items lock for [`HOLD`] through `update_stack`, as the hopper insert
/// `HopperBlockEntity::try_move_in_item` does.
fn during_push<I: Inventory + Sync, R>(
    inventory: &I,
    slot: usize,
    item: &'static Item,
    action: impl FnOnce() -> R,
) -> R {
    let held = Barrier::new(2);
    std::thread::scope(|scope| {
        let pusher = scope.spawn(|| {
            inventory.update_stack(slot, &mut |stack| {
                held.wait();
                std::thread::sleep(HOLD);
                if stack.is_empty() {
                    *stack = ItemStack::new(1, item);
                } else {
                    stack.increment(1);
                }
                true
            })
        });
        held.wait();
        let result = action();
        assert!(pusher.join().unwrap(), "the push lands");
        result
    })
}

/// A hopper above a cooking furnace pushes on every tick. The cook ends on the vanilla tick, and
/// every push lands.
#[test]
fn a_hopper_push_does_not_cost_the_furnace_its_cook() {
    let _serial = support::serial();
    install_recipes();
    let world = support::world();
    let furnace = FurnaceBlockEntity::new(BlockPos::new(0, 100, 0));
    place(&world, furnace.get_position(), &Block::FURNACE);
    furnace.set_stack(0, ItemStack::new(1, &Item::DIRT));
    furnace.set_stack(1, ItemStack::new(1, &Item::COAL));
    for tick in 1..=COOKING_TIME {
        during_push(&furnace, 0, &Item::DIRT, || furnace.tick(&world));
        if tick < COOKING_TIME {
            assert_eq!(i32::from(furnace.get_cooking_time_spent()), tick);
            assert!(furnace.get_stack(2).is_empty());
        }
    }
    let output = furnace.get_stack(2);
    assert_eq!(output.item.id, Item::DIAMOND.id);
    assert_eq!(output.item_count, 1);
    // One dirt at the start and one push per tick, less the cooked one.
    assert_eq!(i32::from(furnace.get_stack(0).item_count), COOKING_TIME);
}

/// `canBurn` and `burn` of vanilla `AbstractFurnaceBlockEntity` wait for a push that holds the
/// items lock, so the cook that the tick ended is not lost.
#[test]
fn the_furnace_burn_waits_for_a_held_items_lock() {
    let _serial = support::serial();
    install_recipes();
    let furnace = FurnaceBlockEntity::new(BlockPos::new(2, 100, 0));
    furnace.set_stack(0, ItemStack::new(1, &Item::DIRT));
    let index = pumpkin_inventory::recipe_index::current();
    let recipe = index
        .cooking_recipe(&Item::DIRT, &CookingRecipeKind::Smelting)
        .unwrap();
    assert!(during_push(&furnace, 1, &Item::COAL, || {
        furnace.can_accept_recipe_output(Some(recipe), 64)
    }));
    assert!(during_push(&furnace, 1, &Item::COAL, || {
        furnace.craft_recipe(Some(recipe))
    }));
    assert_eq!(furnace.get_stack(2).item.id, Item::DIAMOND.id);
    assert!(furnace.get_stack(0).is_empty());
    assert_eq!(furnace.get_stack(1).item_count, 2);
}
