//! Block entities tick in parallel batches, so a hopper on another thread can hold the items lock
//! of a block entity while that block entity ticks. Vanilla ticks block entities one after
//! another (`Level.tickBlockEntities`): a tick never sees a held lock and never loses progress.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails on the first poisoned lock or missing value"
)]

mod support;

use std::marker::PhantomData;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, Instant};

use pumpkin_core::block::entities::BlockEntity;
use pumpkin_core::block::entities::brewing_stand::BrewingStandBlockEntity;
use pumpkin_core::block::entities::campfire::CampfireBlockEntity;
use pumpkin_core::block::entities::furnace::FurnaceBlockEntity;
use pumpkin_core::block::entities::furnace_like_block_entity::CookingBlockEntityBase;
use pumpkin_core::plugin::block::brewing_start::BrewingStartEvent;
use pumpkin_core::plugin::block::campfire_start::CampfireStartEvent;
use pumpkin_core::plugin::inventory::brewing_stand_fuel::BrewingStandFuelEvent;
use pumpkin_core::plugin::inventory::furnace_burn::FurnaceBurnEvent;
use pumpkin_core::plugin::{BoxFuture, EventHandler, EventPriority, Payload};
use pumpkin_core::server::Server;
use pumpkin_core::world::World;
use pumpkin_data::Block;
use pumpkin_data::data_component_impl::PotionContentsImpl;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::potion::Potion;
use pumpkin_data::recipes::{CookingRecipeKind, RecipeCategoryTypes};
use pumpkin_inventory::Inventory;
use pumpkin_nbt::compound::NbtCompound;
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
    support::server().recipe_manager.set_recipes(vec![
        DynamicRecipe::Cooking(OwnedCookingRecipeType::Smelting(cooking("minecraft:dirt"))),
        DynamicRecipe::Cooking(OwnedCookingRecipeType::CampfireCooking(cooking(
            "minecraft:stick",
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

/// A blocking handler that runs a closure on the event.
struct Blocking<E, F>(F, PhantomData<fn() -> E>);

impl<E, F> EventHandler<E> for Blocking<E, F>
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&mut E) + Send + Sync,
{
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut E,
    ) -> BoxFuture<'a, ()> {
        (self.0)(event);
        Box::pin(async {})
    }
}

/// Registers a blocking handler for `E` for the rest of the process.
fn on<E, F>(handler: F)
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&mut E) + Send + Sync + 'static,
{
    support::server().plugin_manager.register_with::<E, _>(
        Arc::new(Blocking(handler, PhantomData)),
        EventPriority::Normal,
        true,
        false,
    );
}

/// The pusher thread that [`start_push`] starts.
type Push = Mutex<Option<std::thread::JoinHandle<bool>>>;

/// Starts a push of one `item` into `slot` of `inventory` on another thread, as
/// [`during_push`] does, and returns once the push holds the items lock. An event handler calls
/// it, so the push holds the lock at the next lock of the tick after the event.
fn start_push<I: Inventory + 'static>(
    inventory: &Arc<I>,
    slot: usize,
    item: &'static Item,
    push: &Push,
) {
    let held = Arc::new(Barrier::new(2));
    let inventory = inventory.clone();
    let pusher_held = held.clone();
    let pusher = std::thread::spawn(move || {
        inventory.update_stack(slot, &mut |stack| {
            pusher_held.wait();
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
    *push.lock().unwrap() = Some(pusher);
}

/// Waits for the push that [`start_push`] started and checks that it landed.
fn join_push(push: &Push) {
    let pusher = push
        .lock()
        .unwrap()
        .take()
        .expect("the event starts the push");
    assert!(pusher.join().unwrap(), "the push lands");
}

/// A `CampfireStartEvent` handler reads the campfire. Its locks are free, and the cooking time
/// that the handler sets is the total time of the slot.
#[test]
fn a_campfire_start_handler_can_read_the_campfire() {
    let _serial = support::serial();
    install_recipes();
    let world = support::world();
    let pos = BlockPos::new(4, 100, 0);
    place(&world, pos, &Block::CAMPFIRE);
    let campfire = Arc::new(CampfireBlockEntity::new(pos));
    *campfire.items[0].lock().unwrap() = ItemStack::new(1, &Item::STICK);

    let read: Arc<Mutex<Option<Option<NbtCompound>>>> = Arc::default();
    let handler = {
        let campfire = campfire.clone();
        let read = read.clone();
        move |event: &mut CampfireStartEvent| {
            if event.block_pos != pos {
                return;
            }
            // `try_lock`: a handler that waits on a held lock hangs the test.
            let free = campfire.items.iter().all(|slot| slot.try_lock().is_ok())
                && campfire.cooking_times.iter().all(|t| t.try_lock().is_ok())
                && campfire
                    .cooking_total_times
                    .iter()
                    .all(|t| t.try_lock().is_ok());
            let nbt = free.then(|| {
                let mut nbt = NbtCompound::new();
                campfire.write_nbt(&mut nbt);
                nbt
            });
            *read.lock().unwrap() = Some(nbt);
            event.cooking_time = COOKING_TIME + 3;
        }
    };
    on(handler);

    campfire.tick(&world);
    let nbt = read
        .lock()
        .unwrap()
        .take()
        .expect("the start event fires")
        .expect("the campfire locks are free while the handler runs");
    assert_eq!(
        nbt.get_int_array("CookingTotalTimes").map(<[i32]>::len),
        Some(CampfireBlockEntity::SLOT_COUNT)
    );
    assert_eq!(
        *campfire.cooking_total_times[0].lock().unwrap(),
        COOKING_TIME + 3
    );
    assert_eq!(*campfire.cooking_times[0].lock().unwrap(), 1);
}

/// Vanilla `BrewingStandBlockEntity.serverTick` refuels from blaze powder and stops a brew that
/// has no ingredient, also while a hopper pushes a bottle into the stand.
#[test]
fn a_hopper_push_does_not_cost_the_brewing_stand_a_tick() {
    let _serial = support::serial();
    let world = support::world();
    let stand = BrewingStandBlockEntity::new(BlockPos::new(6, 100, 0));
    place(&world, stand.get_position(), &Block::BREWING_STAND);
    stand.set_stack(4, ItemStack::new(1, &Item::BLAZE_POWDER));
    during_push(&stand, 0, &Item::GLASS_BOTTLE, || stand.tick(&world));
    assert_eq!(stand.fuel.load(Ordering::Relaxed), 20);
    assert!(stand.get_stack(4).is_empty());

    stand.brew_time.store(5, Ordering::Relaxed);
    during_push(&stand, 1, &Item::GLASS_BOTTLE, || stand.tick(&world));
    assert_eq!(stand.brew_time.load(Ordering::Relaxed), 0);
}

/// The furnace uses its fuel while a push holds the items lock. A `FurnaceBurnEvent` handler
/// starts the push, so the lock is held when the tick takes the fuel.
#[test]
fn the_fuel_use_waits_for_a_push_that_the_burn_event_starts() {
    let _serial = support::serial();
    install_recipes();
    let world = support::world();
    let pos = BlockPos::new(8, 100, 0);
    place(&world, pos, &Block::FURNACE);
    let furnace = Arc::new(FurnaceBlockEntity::new(pos));
    furnace.set_stack(0, ItemStack::new(1, &Item::DIRT));
    furnace.set_stack(1, ItemStack::new(1, &Item::COAL));
    let push: Arc<Push> = Arc::default();
    on({
        let furnace = furnace.clone();
        let push = push.clone();
        move |event: &mut FurnaceBurnEvent| {
            if event.block_pos == pos {
                start_push(&furnace, 1, &Item::COAL, &push);
            }
        }
    });
    furnace.tick(&world);
    join_push(&push);
    assert!(furnace.is_burning());
    // One coal, one pushed coal, one coal used.
    assert_eq!(furnace.get_stack(1).item_count, 1);
}

/// The brewing stand takes its blaze powder while a push holds the items lock. A
/// `BrewingStandFuelEvent` handler starts the push, so the lock is held at the refuel.
#[test]
fn the_refuel_waits_for_a_push_that_the_fuel_event_starts() {
    let _serial = support::serial();
    let world = support::world();
    let pos = BlockPos::new(10, 100, 0);
    place(&world, pos, &Block::BREWING_STAND);
    let stand = Arc::new(BrewingStandBlockEntity::new(pos));
    stand.set_stack(4, ItemStack::new(1, &Item::BLAZE_POWDER));
    let push: Arc<Push> = Arc::default();
    on({
        let stand = stand.clone();
        let push = push.clone();
        move |event: &mut BrewingStandFuelEvent| {
            if event.block_pos == pos {
                start_push(&stand, 0, &Item::GLASS_BOTTLE, &push);
            }
        }
    });
    stand.tick(&world);
    join_push(&push);
    assert_eq!(stand.fuel.load(Ordering::Relaxed), 20);
    assert!(stand.get_stack(4).is_empty());
}

fn water_bottle() -> ItemStack {
    let mut bottle = ItemStack::new(1, &Item::POTION);
    bottle.set_data_component(PotionContentsImpl {
        potion_id: Some(i32::from(Potion::WATER.id)),
        custom_color: None,
        custom_effects: Vec::new(),
        custom_name: None,
    });
    bottle
}

/// The brewing stand reads its bottles for the block state while a push holds the items lock.
/// A `BrewingStartEvent` handler starts the push, so the lock is held at the read.
#[test]
fn the_bottle_state_waits_for_a_push_that_the_brewing_start_event_starts() {
    let _serial = support::serial();
    let world = support::world();
    let pos = BlockPos::new(12, 100, 0);
    place(&world, pos, &Block::BREWING_STAND);
    let stand = Arc::new(BrewingStandBlockEntity::new(pos));
    stand.set_stack(0, water_bottle());
    stand.set_stack(3, ItemStack::new(1, &Item::NETHER_WART));
    stand.fuel.store(5, Ordering::Relaxed);
    let push: Arc<Push> = Arc::default();
    on({
        let stand = stand.clone();
        let push = push.clone();
        move |event: &mut BrewingStartEvent| {
            if event.block_pos == pos {
                start_push(&stand, 4, &Item::BLAZE_POWDER, &push);
            }
        }
    });
    stand.tick(&world);
    join_push(&push);
    assert_eq!(stand.brew_time.load(Ordering::Relaxed), 400);
    assert_eq!(
        *stand.last_potion_count.lock().unwrap(),
        Some([true, false, false])
    );
}

/// Vanilla `burn` shrinks the input that it took the recipe from. A `FurnaceSmeltEvent` handler
/// that swaps the input gets no result for the new input.
#[test]
fn the_burn_skips_an_input_that_the_smelt_event_swaps() {
    let _serial = support::serial();
    install_recipes();
    let world = support::world();
    let pos = BlockPos::new(0, 100, 2);
    place(&world, pos, &Block::FURNACE);
    let furnace = Arc::new(FurnaceBlockEntity::new(pos));
    furnace.set_stack(0, ItemStack::new(1, &Item::DIRT));
    furnace.set_stack(1, ItemStack::new(1, &Item::COAL));
    on({
        let furnace = furnace.clone();
        move |event: &mut pumpkin_core::plugin::inventory::furnace_smelt::FurnaceSmeltEvent| {
            if event.block_pos == pos {
                furnace.set_stack(0, ItemStack::new(1, &Item::COBBLESTONE));
            }
        }
    });
    for _ in 0..COOKING_TIME {
        furnace.tick(&world);
    }
    assert!(furnace.get_stack(2).is_empty());
    assert_eq!(furnace.get_stack(0).item.id, Item::COBBLESTONE.id);
    assert_eq!(furnace.get_stack(0).item_count, 1);
}

/// Vanilla `consumeFuel` uses the fuel that lit the furnace. A `FurnaceBurnEvent` handler that
/// takes the fuel leaves the furnace unlit.
#[test]
fn the_furnace_does_not_light_when_the_burn_event_takes_the_fuel() {
    let _serial = support::serial();
    install_recipes();
    let world = support::world();
    let pos = BlockPos::new(2, 100, 2);
    place(&world, pos, &Block::FURNACE);
    let furnace = Arc::new(FurnaceBlockEntity::new(pos));
    furnace.set_stack(0, ItemStack::new(1, &Item::DIRT));
    furnace.set_stack(1, ItemStack::new(1, &Item::COAL));
    on({
        let furnace = furnace.clone();
        move |event: &mut FurnaceBurnEvent| {
            if event.block_pos == pos {
                furnace.remove_stack(1);
            }
        }
    });
    furnace.tick(&world);
    assert!(!furnace.is_burning());
    assert_eq!(furnace.get_cooking_time_spent(), 0);
    assert!(furnace.get_stack(1).is_empty());
}

/// The refuel takes the blaze powder that the stand read, also when a hopper adds one more
/// during `BrewingStandFuelEvent`.
#[test]
fn the_refuel_takes_a_fuel_stack_that_grew_during_the_fuel_event() {
    let _serial = support::serial();
    let world = support::world();
    let pos = BlockPos::new(4, 100, 2);
    place(&world, pos, &Block::BREWING_STAND);
    let stand = Arc::new(BrewingStandBlockEntity::new(pos));
    stand.set_stack(4, ItemStack::new(1, &Item::BLAZE_POWDER));
    on({
        let stand = stand.clone();
        move |event: &mut BrewingStandFuelEvent| {
            if event.block_pos == pos {
                stand.update_stack(4, &mut |stack| {
                    stack.increment(1);
                    true
                });
            }
        }
    });
    stand.tick(&world);
    assert_eq!(stand.fuel.load(Ordering::Relaxed), 20);
    assert_eq!(stand.get_stack(4).item_count, 1);
}

/// A `CampfireStartEvent` handler that sets the total time of the slot itself keeps its value.
#[test]
fn a_campfire_start_handler_can_set_the_slot_time() {
    let _serial = support::serial();
    install_recipes();
    let world = support::world();
    let pos = BlockPos::new(6, 100, 2);
    place(&world, pos, &Block::CAMPFIRE);
    let campfire = Arc::new(CampfireBlockEntity::new(pos));
    *campfire.items[0].lock().unwrap() = ItemStack::new(1, &Item::STICK);
    on({
        let campfire = campfire.clone();
        move |event: &mut CampfireStartEvent| {
            if event.block_pos == pos {
                *campfire.cooking_total_times[0].lock().unwrap() = COOKING_TIME + 5;
            }
        }
    });
    campfire.tick(&world);
    assert_eq!(
        *campfire.cooking_total_times[0].lock().unwrap(),
        COOKING_TIME + 5
    );
    assert_eq!(*campfire.cooking_times[0].lock().unwrap(), 1);
}

/// Vanilla `serverTick` (`AbstractFurnaceBlockEntity.java:173-176`) leaves the cook progress
/// alone for an input without a recipe.
#[test]
fn a_lit_furnace_keeps_its_progress_for_an_input_without_a_recipe() {
    let _serial = support::serial();
    install_recipes();
    let world = support::world();
    let pos = BlockPos::new(8, 100, 2);
    place(&world, pos, &Block::FURNACE);
    let furnace = FurnaceBlockEntity::new(pos);
    furnace.set_stack(0, ItemStack::new(1, &Item::GRAVEL));
    furnace.set_lit_time_remaining(100);
    furnace.set_lit_total_time(100);
    furnace.set_cooking_time_spent(5);
    furnace.tick(&world);
    assert!(furnace.is_burning());
    assert_eq!(furnace.get_cooking_time_spent(), 5);
}

/// Vanilla `serverTick` (`BrewingStandBlockEntity.java:187`) sets the bottle state with flag 2:
/// the clients get it, the neighbors get no update.
#[test]
fn the_bottle_state_does_not_update_the_neighbors() {
    let _serial = support::serial();
    let world = support::world();
    let pos = BlockPos::new(10, 100, 2);
    place(&world, pos, &Block::BREWING_STAND);
    let stand = BrewingStandBlockEntity::new(pos);
    stand.set_stack(0, water_bottle());
    let updates: Arc<std::sync::atomic::AtomicUsize> = Arc::default();
    on({
        let updates = updates.clone();
        move |event: &mut pumpkin_core::plugin::block::block_physics::BlockPhysicsEvent| {
            if event.changed_pos == pos {
                updates.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
    stand.tick(&world);
    assert_eq!(
        *stand.last_potion_count.lock().unwrap(),
        Some([true, false, false])
    );
    assert_eq!(updates.load(Ordering::Relaxed), 0);
}

/// A cancelled `BrewingStartEvent` stops only the brew: the stand still sets its bottle state,
/// as the rest of vanilla `serverTick` does.
#[test]
fn a_cancelled_brewing_start_still_updates_the_bottle_state() {
    let _serial = support::serial();
    let world = support::world();
    let pos = BlockPos::new(12, 100, 2);
    place(&world, pos, &Block::BREWING_STAND);
    let stand = BrewingStandBlockEntity::new(pos);
    stand.set_stack(0, water_bottle());
    stand.set_stack(3, ItemStack::new(1, &Item::NETHER_WART));
    stand.fuel.store(5, Ordering::Relaxed);
    on(move |event: &mut BrewingStartEvent| {
        if event.block_pos == pos {
            event.cancelled = true;
        }
    });
    stand.tick(&world);
    assert_eq!(stand.brew_time.load(Ordering::Relaxed), 0);
    assert_eq!(stand.fuel.load(Ordering::Relaxed), 5);
    assert_eq!(
        *stand.last_potion_count.lock().unwrap(),
        Some([true, false, false])
    );
}
