#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]
#![expect(
    clippy::semicolon_outside_block,
    reason = "a block scope drops its transaction without a commit"
)]

use std::{
    collections::HashMap,
    sync::{
        Arc, LazyLock, Mutex, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use arc_swap::ArcSwap;
use pumpkin_config::world::LevelConfig;
use pumpkin_core::{
    block::{
        OnScheduledTickArgs,
        entities::{
            BlockEntity, chest::ChestBlockEntity, furnace::FurnaceBlockEntity,
            hopper::HopperBlockEntity,
        },
        registry::default_registry,
    },
    world::World,
};
use pumpkin_data::{
    Block, BlockDirection, BlockStateId,
    block_properties::{
        ChestLikeProperties, ChestType, DispenserLikeProperties, Facing, FacingHopper,
        HorizontalFacing,
    },
    dimension::Dimension,
    item::Item,
    item_stack::ItemStack,
};
use pumpkin_inventory::Inventory;
use pumpkin_util::{
    math::{position::BlockPos, vector2::Vector2},
    world_seed::Seed,
};
use pumpkin_world::{chunk::ChunkData, level::Level, world::BlockFlags, world_info::LevelData};

use super::*;
use crate::{
    NativeMod,
    transfer::{ItemStacksResourceHandler, Transaction, TransactionError},
};

/// A registration that stays open for the tests that register providers.
static OPEN: AtomicBool = AtomicBool::new(false);

fn open_event() -> RegisterCapabilitiesEvent {
    RegisterCapabilitiesEvent::new(EventPriority::Normal, &OPEN)
}

/// Runs the host part of the event once and installs the engine hooks, as at startup, without
/// freezing the providers, so the tests can register more.
fn vanilla_providers() {
    static HOST: AtomicBool = AtomicBool::new(false);
    static DONE: LazyLock<()> = LazyLock::new(|| {
        vanilla::install_hooks();
        assert_eq!(post_with(&[], &HOST, true), Ok(()));
    });
    LazyLock::force(&DONE);
}

/// A fresh registration flag for one call of [`post_with`].
fn fresh_flag() -> &'static AtomicBool {
    Box::leak(Box::new(AtomicBool::new(false)))
}

struct TestWorld {
    world: Arc<World>,
    runtime: tokio::runtime::Runtime,
    _folder: tempfile::TempDir,
}

impl TestWorld {
    fn new() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let folder = tempfile::tempdir().unwrap();
        let world = {
            let _guard = runtime.enter();
            let dimension = Dimension::OVERWORLD.clone();
            Arc::new(World::load(
                Level::from_root_folder(
                    &LevelConfig::default(),
                    folder.path().to_path_buf(),
                    0,
                    dimension.clone(),
                ),
                Arc::new(ArcSwap::from_pointee(LevelData::default(Seed(0)))),
                dimension,
                default_registry(),
                Weak::new(),
            ))
        };
        world
            .level
            .loaded_chunks
            .insert(Vector2::new(0, 0), ChunkData::empty_sync(0, 0));
        Self {
            world,
            runtime,
            _folder: folder,
        }
    }

    fn place(&self, pos: BlockPos, state: BlockStateId) {
        self.world.set_block_state(&pos, state, BlockFlags::empty());
    }
}

impl Drop for TestWorld {
    fn drop(&mut self) {
        self.runtime.block_on(self.world.level.shutdown());
    }
}

type ItemHandler = dyn ResourceHandler<ItemResource>;

fn pos(x: i32, y: i32, z: i32) -> BlockPos {
    BlockPos::new(x, y, z)
}

#[test]
fn creating_the_item_handler_by_name_returns_the_built_in_capability() {
    let created = BlockCapability::<ItemHandler, Option<BlockDirection>>::create_sided(
        "neoforge:item_handler",
    )
    .unwrap();
    assert!(std::ptr::eq(created.data, item::BLOCK.data));
    assert_eq!(created.name(), "neoforge:item_handler");
}

#[test]
fn creating_a_name_with_other_types_fails_with_type_mismatch() {
    let error = BlockCapability::<dyn EnergyHandler, Option<BlockDirection>>::create(
        "neoforge:item_handler",
    )
    .unwrap_err();
    assert!(
        matches!(error, CapabilityError::TypeMismatch { ref name, .. } if name == "neoforge:item_handler")
    );
    // The same name with another context type is a mismatch too.
    assert!(BlockCapability::<ItemHandler, ()>::create_void("neoforge:item_handler").is_err());
    // A new name works, and a second create returns it.
    let first = ItemCapability::<dyn EnergyHandler, ()>::create("test:mismatch_free").unwrap();
    let second = ItemCapability::<dyn EnergyHandler, ()>::create("test:mismatch_free").unwrap();
    assert!(std::ptr::eq(first.data, second.data));
}

#[test]
fn a_provider_of_one_mod_answers_the_lookup_of_another() {
    let test = TestWorld::new();
    let capability =
        BlockCapability::<ItemHandler, Option<BlockDirection>>::create_sided("test:mod_items")
            .unwrap();
    let handler = Arc::new(ItemStacksResourceHandler::new(1, 64));
    let provided = handler.clone();
    let mut event = open_event();
    event
        .register_block(
            &capability,
            move |_, _, _, _, _| {
                let handler: Arc<ItemHandler> = provided.clone();
                Some(handler)
            },
            &["minecraft:lodestone"],
        )
        .unwrap();
    assert!(event.is_block_registered(&capability, "lodestone"));
    let at = pos(1, 64, 1);
    test.place(at, Block::LODESTONE.default_state.id);

    let found = capability.get_capability(&test.world, &at, None).unwrap();
    let mut transaction = Transaction::open_root().unwrap();
    let diamond = ItemResource::of_item(&Item::DIAMOND);
    assert_eq!(found.insert(&diamond, 5, &mut transaction), 5);
    transaction.commit();
    assert_eq!(handler.get_amount_as_long(0), 5);
    assert!(
        capability
            .get_capability(&test.world, &pos(2, 64, 1), None)
            .is_none()
    );
}

#[test]
fn a_chest_has_27_slots_and_a_double_chest_54_with_the_right_half_first() {
    vanilla_providers();
    let test = TestWorld::new();
    let single = pos(4, 64, 4);
    test.place(single, Block::CHEST.default_state.id);
    test.world
        .add_block_entity(Arc::new(ChestBlockEntity::new(single)));
    let handler = item::BLOCK
        .get_capability(&test.world, &single, None)
        .unwrap();
    assert_eq!(handler.size(), 27);

    // The left half of a chest facing north joins the right half to its east.
    let left = pos(4, 64, 6);
    let right = pos(5, 64, 6);
    for (at, chest_type) in [(left, ChestType::Left), (right, ChestType::Right)] {
        let mut props = ChestLikeProperties::from_state_id(Block::CHEST.default_state.id);
        props.facing = HorizontalFacing::North;
        props.r#type = chest_type;
        test.place(at, props.to_state_id(&Block::CHEST));
        test.world
            .add_block_entity(Arc::new(ChestBlockEntity::new(at)));
    }
    test.world
        .get_block_entity(&right)
        .unwrap()
        .get_inventory()
        .unwrap()
        .set_stack(0, ItemStack::new(1, &Item::DIAMOND));
    let double = item::BLOCK
        .get_capability(&test.world, &left, None)
        .unwrap();
    assert_eq!(double.size(), 54);
    // The right half is the first: `chestType == RIGHT ? FIRST : SECOND`.
    assert_eq!(double.get_amount_as_long(0), 1);
    let from_right = item::BLOCK
        .get_capability(&test.world, &right, None)
        .unwrap();
    assert_eq!(from_right.get_amount_as_long(0), 1);
}

#[test]
fn an_aborted_extract_leaves_the_chest_untouched() -> Result<(), TransactionError> {
    vanilla_providers();
    let test = TestWorld::new();
    let at = pos(6, 64, 6);
    test.place(at, Block::CHEST.default_state.id);
    let chest = Arc::new(ChestBlockEntity::new(at));
    chest.set_stack(0, ItemStack::new(10, &Item::DIAMOND));
    test.world.add_block_entity(chest.clone());
    let handler = item::BLOCK.get_capability(&test.world, &at, None).unwrap();
    let diamond = ItemResource::of_item(&Item::DIAMOND);
    {
        let mut transaction = Transaction::open_root()?;
        assert_eq!(handler.extract(&diamond, 10, &mut transaction), 10);
        assert!(chest.get_stack(0).is_empty());
    }
    assert_eq!(chest.get_stack(0).item_count, 10);
    let mut transaction = Transaction::open_root()?;
    assert_eq!(handler.extract(&diamond, 4, &mut transaction), 4);
    transaction.commit();
    assert_eq!(chest.get_stack(0).item_count, 6);
    Ok(())
}

#[test]
fn a_rollback_keeps_a_slot_that_another_write_changed() -> Result<(), TransactionError> {
    vanilla_providers();
    let test = TestWorld::new();
    let at = pos(8, 64, 6);
    test.place(at, Block::CHEST.default_state.id);
    let chest = Arc::new(ChestBlockEntity::new(at));
    chest.set_stack(0, ItemStack::new(10, &Item::DIAMOND));
    test.world.add_block_entity(chest.clone());
    let handler = item::BLOCK.get_capability(&test.world, &at, None).unwrap();
    {
        let mut transaction = Transaction::open_root()?;
        handler.extract(&ItemResource::of_item(&Item::DIAMOND), 3, &mut transaction);
        // Another thread writes the slot while the transaction is open.
        chest.set_stack(0, ItemStack::new(1, &Item::EMERALD));
    }
    let stack = chest.get_stack(0);
    assert_eq!((stack.item.id, stack.item_count), (Item::EMERALD.id, 1));
    Ok(())
}

#[test]
fn a_mod_handler_rolls_back_with_its_root_transaction() -> Result<(), TransactionError> {
    let handler = ItemStacksResourceHandler::new(2, 64);
    let stone = ItemResource::of_item(&Item::STONE);
    {
        let mut root = Transaction::open_root()?;
        assert_eq!(handler.insert(&stone, 70, &mut root), 70);
        let nested = Transaction::open(Some(&mut root))?;
        nested.commit();
    }
    assert_eq!(
        handler.get_amount_as_long(0) + handler.get_amount_as_long(1),
        0
    );
    Ok(())
}

#[test]
fn a_furnace_exposes_the_vanilla_slots_of_each_face() -> Result<(), TransactionError> {
    vanilla_providers();
    let test = TestWorld::new();
    let at = pos(10, 64, 4);
    test.place(at, Block::FURNACE.default_state.id);
    let furnace = Arc::new(FurnaceBlockEntity::new(at));
    test.world.add_block_entity(furnace.clone());
    let up = item::BLOCK
        .get_capability(&test.world, &at, Some(BlockDirection::Up))
        .unwrap();
    let down = item::BLOCK
        .get_capability(&test.world, &at, Some(BlockDirection::Down))
        .unwrap();
    let side = item::BLOCK
        .get_capability(&test.world, &at, Some(BlockDirection::North))
        .unwrap();
    assert_eq!((up.size(), down.size(), side.size()), (1, 2, 1));
    let mut transaction = Transaction::open_root()?;
    let iron = ItemResource::of_item(&Item::RAW_IRON);
    let coal = ItemResource::of_item(&Item::COAL);
    // The side reaches only the fuel slot, which takes no ore.
    assert_eq!(side.insert(&iron, 1, &mut transaction), 0);
    assert_eq!(side.insert(&coal, 3, &mut transaction), 3);
    assert_eq!(up.insert(&iron, 2, &mut transaction), 2);
    transaction.commit();
    assert_eq!(furnace.get_stack(0).item_count, 2);
    assert_eq!(furnace.get_stack(1).item_count, 3);
    // The bottom takes fuel out only as a bucket.
    let mut transaction = Transaction::open_root()?;
    assert_eq!(down.extract(&coal, 1, &mut transaction), 0);
    Ok(())
}

#[test]
fn a_cache_listener_runs_when_the_block_entity_goes() {
    vanilla_providers();
    let test = TestWorld::new();
    let at = pos(12, 64, 8);
    test.place(at, Block::CHEST.default_state.id);
    test.world
        .add_block_entity(Arc::new(ChestBlockEntity::new(at)));
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let cache = BlockCapabilityCache::create_with_listener(
        *item::BLOCK,
        &test.world,
        at,
        None,
        || true,
        move || {
            counter.fetch_add(1, Ordering::Relaxed);
        },
    );
    assert!(cache.get_capability().unwrap().is_some());
    test.world.remove_block_entity(&at);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    // A cache that nobody read since the last invalidation is not told again.
    invalidate_capabilities(&test.world, &at);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(cache.get_capability().unwrap().is_none());
    // Placing a block entity invalidates too.
    test.world
        .add_block_entity(Arc::new(ChestBlockEntity::new(at)));
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert!(cache.get_capability().unwrap().is_some());
}

#[test]
fn a_mod_invalidates_the_cache_of_a_plain_block() {
    let test = TestWorld::new();
    let at = pos(13, 64, 8);
    let valid = Arc::new(AtomicBool::new(true));
    let still_valid = valid.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let cache = BlockCapabilityCache::create_with_listener(
        *item::BLOCK,
        &test.world,
        at,
        None,
        move || still_valid.load(Ordering::Relaxed),
        move || {
            counter.fetch_add(1, Ordering::Relaxed);
        },
    );
    assert!(cache.get_capability().unwrap().is_none());
    invalidate_capabilities(&test.world, &at);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    cache.get_capability().unwrap();
    valid.store(false, Ordering::Relaxed);
    invalidate_capabilities(&test.world, &at);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        cache.get_capability().err(),
        Some(CapabilityError::InvalidCache)
    );
}

#[test]
fn a_chunk_unload_invalidates_its_caches() {
    let test = TestWorld::new();
    let at = pos(14, 64, 9);
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let cache = BlockCapabilityCache::create_with_listener(
        *item::BLOCK,
        &test.world,
        at,
        None,
        || true,
        move || {
            counter.fetch_add(1, Ordering::Relaxed);
        },
    );
    cache.get_capability().unwrap();
    invalidate_capabilities_in_chunk(&test.world, Vector2::new(0, 0));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    drop(cache);
    invalidate_capabilities(&test.world, &at);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn registration_closes_after_the_event() {
    let closed = fresh_flag();
    assert_eq!(post_with(&[], closed, false), Ok(()));
    let mut late = RegisterCapabilitiesEvent::new(EventPriority::Normal, closed);
    let result = late.register_block(&item::BLOCK, |_, _, _, _, _| None, &["minecraft:stone"]);
    assert_eq!(result, Err(CapabilityError::RegistrationClosed));
    assert_eq!(
        late.set_proxyable(&fluid::BLOCK),
        Err(CapabilityError::RegistrationClosed)
    );
    assert_eq!(late.error, Some(CapabilityError::RegistrationClosed));
}

#[test]
fn unknown_targets_and_proxyable_conflicts_fail() {
    let capability = BlockCapability::<dyn EnergyHandler, ()>::create_void("test:proxy").unwrap();
    let mut event = open_event();
    assert!(matches!(
        event.register_block(&capability, |_, _, _, _, ()| None, &["test:no_such_block"]),
        Err(CapabilityError::Unknown(_))
    ));
    assert_eq!(
        event.register_block(&capability, |_, _, _, _, ()| None, &[]),
        Err(CapabilityError::NoTargets)
    );
    assert!(matches!(
        event.register_block_entity(&capability, "minecraft:no_such_type", |_, ()| None),
        Err(CapabilityError::UnknownBlockEntityType(_))
    ));
    event.set_proxyable(&capability).unwrap();
    assert!(capability.is_proxyable());
    assert!(get_all_proxyable().contains(&"test:proxy".to_owned()));
    assert!(matches!(
        event.set_non_proxyable(&capability),
        Err(CapabilityError::ProxyableConflict {
            proxyable: true,
            ..
        })
    ));
}

static PHASES_SEEN: Mutex<Vec<(EventPriority, bool)>> = Mutex::new(Vec::new());

struct PhaseMod;

impl NativeMod for PhaseMod {
    fn id(&self) -> &'static str {
        "phasemod"
    }

    fn display_name(&self) -> &'static str {
        "Phase Mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, _cx: &mut crate::ModInit) {}

    fn register_capabilities(&self, event: &mut RegisterCapabilitiesEvent) {
        let chest = event.is_block_registered(&item::BLOCK, "minecraft:chest");
        PHASES_SEEN
            .lock()
            .unwrap()
            .push((event.phase().clone(), chest));
    }
}

#[test]
fn every_mod_gets_each_phase_in_order_after_the_host() {
    static MOD: PhaseMod = PhaseMod;
    vanilla_providers();
    let _ = post_with(
        &[LoadedMod {
            native_mod: &MOD,
            mod_id: "phasemod",
        }],
        fresh_flag(),
        false,
    );
    let seen = PHASES_SEEN.lock().unwrap().clone();
    assert_eq!(
        seen,
        vec![
            (EventPriority::Highest, true),
            (EventPriority::High, true),
            (EventPriority::Normal, true),
            (EventPriority::Low, true),
            (EventPriority::Lowest, true),
        ]
    );
    assert!(item::BLOCK.is_proxyable());
}

/// A mod that registers a provider for `block` at `Normal`, and records at `Low` whether it sees
/// the provider of the other mod.
struct OrderMod {
    id: &'static str,
    block: &'static str,
    other_block: &'static str,
    saw_other: AtomicBool,
}

static ORDER_CAP: LazyLock<BlockCapability<dyn EnergyHandler, ()>> =
    LazyLock::new(|| BlockCapability::create_void("test:load_order").unwrap());

impl NativeMod for OrderMod {
    fn id(&self) -> &'static str {
        self.id
    }

    fn display_name(&self) -> &'static str {
        self.id
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, _cx: &mut crate::ModInit) {}

    fn register_capabilities(&self, event: &mut RegisterCapabilitiesEvent) {
        match event.phase() {
            EventPriority::Normal => {
                event
                    .register_block(&ORDER_CAP, |_, _, _, _, ()| None, &[self.block])
                    .unwrap();
            }
            EventPriority::Low => self.saw_other.store(
                event.is_block_registered(&ORDER_CAP, self.other_block),
                Ordering::Relaxed,
            ),
            _ => {}
        }
    }
}

#[test]
fn a_fallback_at_low_sees_the_normal_providers_of_every_mod_in_either_load_order() {
    let make = |id, block, other_block| -> &'static OrderMod {
        Box::leak(Box::new(OrderMod {
            id,
            block,
            other_block,
            saw_other: AtomicBool::new(false),
        }))
    };
    let first = make("first", "minecraft:end_rod", "minecraft:lightning_rod");
    let second = make("second", "minecraft:lightning_rod", "minecraft:end_rod");
    let loaded = |native_mod: &'static OrderMod| LoadedMod {
        native_mod,
        mod_id: native_mod.id,
    };
    assert_eq!(
        post_with(&[loaded(first), loaded(second)], fresh_flag(), false),
        Ok(())
    );
    assert!(first.saw_other.load(Ordering::Relaxed));
    assert!(second.saw_other.load(Ordering::Relaxed));
    let third = make("third", "minecraft:target", "minecraft:sculk_vein");
    let fourth = make("fourth", "minecraft:sculk_vein", "minecraft:target");
    assert_eq!(
        post_with(&[loaded(fourth), loaded(third)], fresh_flag(), false),
        Ok(())
    );
    assert!(third.saw_other.load(Ordering::Relaxed));
    assert!(fourth.saw_other.load(Ordering::Relaxed));
}

#[test]
fn a_lookup_that_an_invalidation_overtakes_is_not_cached() {
    let test = TestWorld::new();
    let at = pos(3, 64, 14);
    test.place(at, Block::LODESTONE.default_state.id);
    let capability =
        BlockCapability::<dyn EnergyHandler, ()>::create_void("test:overtaken").unwrap();
    let lookups = Arc::new(AtomicUsize::new(0));
    let counter = lookups.clone();
    let world = test.world.clone();
    let mut event = open_event();
    event
        .register_block(
            &capability,
            move |_, pos, _, _, ()| {
                // A parallel tick changes the block while this lookup runs.
                if counter.fetch_add(1, Ordering::Relaxed) == 0 {
                    invalidate_capabilities(&world, pos);
                }
                let handler: Arc<dyn EnergyHandler> =
                    Arc::new(crate::transfer::SimpleEnergyHandler::new(10, 10, 10));
                Some(handler)
            },
            &["minecraft:lodestone"],
        )
        .unwrap();
    let cache = BlockCapabilityCache::create(capability, &test.world, at, ());
    assert!(cache.get_capability().unwrap().is_some());
    assert!(cache.get_capability().unwrap().is_some());
    assert_eq!(lookups.load(Ordering::Relaxed), 2);
    assert!(cache.get_capability().unwrap().is_some());
    assert_eq!(lookups.load(Ordering::Relaxed), 2);
}

#[test]
fn an_invalidation_during_the_listener_calls_reaches_every_listener() {
    let test = TestWorld::new();
    let at = pos(5, 64, 14);
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let refilled = Arc::new(BlockCapabilityCache::create_with_listener(
        *item::BLOCK,
        &test.world,
        at,
        None,
        || true,
        move || {
            counter.fetch_add(1, Ordering::Relaxed);
        },
    ));
    refilled.get_capability().unwrap();
    let other = refilled.clone();
    let world = test.world.clone();
    let second = BlockCapabilityCache::create_with_listener(
        *item::BLOCK,
        &test.world,
        at,
        None,
        || true,
        move || {
            // The first cache was told and read again; another change of the block follows.
            other.get_capability().unwrap();
            invalidate_capabilities(&world, &at);
        },
    );
    second.get_capability().unwrap();
    invalidate_capabilities(&test.world, &at);
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

/// Handlers of the plain blocks that the hopper and dropper tests feed.
static PLAIN: LazyLock<Mutex<HashMap<BlockPos, Arc<ItemStacksResourceHandler>>>> =
    LazyLock::new(Mutex::default);

fn plain_block(at: BlockPos, stack: Option<ItemStack>) -> Arc<ItemStacksResourceHandler> {
    static REGISTERED: LazyLock<()> = LazyLock::new(|| {
        let mut event = open_event();
        event
            .register_block(
                &item::BLOCK,
                |_, pos, _, _, _| {
                    let handler: Arc<ItemHandler> = PLAIN.lock().unwrap().get(pos)?.clone();
                    Some(handler)
                },
                &["minecraft:dried_kelp_block"],
            )
            .unwrap();
    });
    // The hopper and the dropper find a capability through the engine hooks.
    vanilla_providers();
    LazyLock::force(&REGISTERED);
    let handler = Arc::new(ItemStacksResourceHandler::new(1, 64));
    if let Some(stack) = stack {
        let mut transaction = Transaction::open_root().unwrap();
        handler.insert(
            &ItemResource::of(&stack),
            u32::from(stack.item_count),
            &mut transaction,
        );
        transaction.commit();
    }
    PLAIN.lock().unwrap().insert(at, handler.clone());
    handler
}

fn hopper_at(test: &TestWorld, at: BlockPos, stack: Option<ItemStack>) -> Arc<HopperBlockEntity> {
    test.place(at, Block::HOPPER.default_state.id);
    let hopper = Arc::new(HopperBlockEntity::new(at, FacingHopper::Down));
    if let Some(stack) = stack {
        hopper.set_stack(0, stack);
    }
    test.world.add_block_entity(hopper.clone());
    hopper
}

#[test]
fn a_hopper_pushes_into_the_item_capability_of_a_plain_block() {
    let test = TestWorld::new();
    let target = pos(2, 63, 12);
    test.place(target, Block::DRIED_KELP_BLOCK.default_state.id);
    let handler = plain_block(target, None);
    let hopper = hopper_at(&test, target.up(), Some(ItemStack::new(3, &Item::DIAMOND)));
    hopper.tick(&test.world);
    assert_eq!(handler.get_amount_as_long(0), 1);
    assert_eq!(hopper.get_stack(0).item_count, 2);
}

#[test]
fn a_hopper_pulls_from_the_item_capability_of_a_plain_block() {
    let test = TestWorld::new();
    let source = pos(4, 66, 12);
    test.place(source, Block::DRIED_KELP_BLOCK.default_state.id);
    let handler = plain_block(source, Some(ItemStack::new(4, &Item::EMERALD)));
    let hopper = hopper_at(&test, pos(4, 65, 12), None);
    hopper.tick(&test.world);
    assert_eq!(handler.get_amount_as_long(0), 3);
    let pulled = hopper.get_stack(0);
    assert_eq!((pulled.item.id, pulled.item_count), (Item::EMERALD.id, 1));
}

#[test]
fn a_dropper_inserts_into_the_item_capability_of_a_plain_block() {
    use pumpkin_core::block::entities::dropper::DropperBlockEntity;

    let test = TestWorld::new();
    let target = pos(6, 63, 12);
    test.place(target, Block::DRIED_KELP_BLOCK.default_state.id);
    let handler = plain_block(target, None);
    let at = target.up();
    let mut props = DispenserLikeProperties::from_state_id(Block::DROPPER.default_state.id);
    props.facing = Facing::Down;
    test.place(at, props.to_state_id(&Block::DROPPER));
    let dropper = Arc::new(DropperBlockEntity::new(at));
    dropper.set_stack(0, ItemStack::new(2, &Item::GOLD_INGOT));
    test.world.add_block_entity(dropper.clone());
    let Some(behaviour) = test
        .world
        .block_registry
        .get_pumpkin_block(Block::DROPPER.id)
    else {
        panic!("the dropper has no behaviour");
    };
    behaviour.on_scheduled_tick(OnScheduledTickArgs {
        world: &test.world,
        block: &Block::DROPPER,
        position: &at,
    });
    assert_eq!(handler.get_amount_as_long(0), 1);
    assert_eq!(dropper.get_stack(0).item_count, 1);
}
