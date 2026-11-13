//! Starts an embedded server with the test mod and a second mod, `cap-reader`, defined here. The
//! test mod exposes the item storage of its block through `Capabilities.Item.BLOCK`. The second
//! mod sees the provider in a later phase of `RegisterCapabilitiesEvent`, and reaches the
//! storage through the capability with transactions that commit and roll back. A hopper pushes
//! into the block and pulls from it, a dropper drops into it, and a capability cache on the block
//! hears when the test mod drops the storage.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

use std::{
    net::{Ipv4Addr, SocketAddr},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    capabilities::{BlockCapabilityCache, RegisterCapabilitiesEvent, item},
    event::EventPriority,
    math::{position::BlockPos, vector2::Vector2},
    pumpkin_core::{
        PumpkinServer,
        block::{
            OnScheduledTickArgs,
            entities::{BlockEntity, dropper::DropperBlockEntity, hopper::HopperBlockEntity},
        },
        data::VanillaData,
        plugin::startup,
        world::World,
    },
    pumpkin_data::{
        Block, BlockDirection, BlockStateId,
        block_properties::{DispenserLikeProperties, Facing, FacingHopper},
        item::Item,
        item_stack::ItemStack,
    },
    pumpkin_world::{chunk::ChunkData, world::BlockFlags},
    register_mod,
    transfer::{ItemResource, ResourceHandler, Transaction},
};
use ironpumpkin_test_mod::{BLOCK, block_storage, remove_block_storage};
use pumpkin_config::{AdvancedConfiguration, BasicConfiguration, TelemetryConfig};
use pumpkin_inventory::Inventory;

/// Whether `cap-reader` saw the provider of the test mod in the `Low` phase.
static SAW_PROVIDER: AtomicBool = AtomicBool::new(false);

struct CapReader;

impl NativeMod for CapReader {
    fn id(&self) -> &'static str {
        "cap-reader"
    }

    fn display_name(&self) -> &'static str {
        "Capability Reader"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, _cx: &mut ModInit) {}

    fn register_capabilities(&self, event: &mut RegisterCapabilitiesEvent) {
        // A fallback provider registers at `Low` only where no other mod registered one, which
        // works whatever the load order of the two mods is.
        if *event.phase() == EventPriority::Low {
            SAW_PROVIDER.store(
                event.is_block_registered(&item::BLOCK, BLOCK),
                Ordering::Relaxed,
            );
        }
    }
}

register_mod!(CapReader);

fn configs(dir: &Path) -> (BasicConfiguration, AdvancedConfiguration) {
    let basic = BasicConfiguration {
        default_level_name: dir.join("world").to_string_lossy().into_owned(),
        allow_nether: false,
        allow_end: false,
        allow_chat_reports: false,
        ..BasicConfiguration::default()
    };
    let mut advanced = AdvancedConfiguration::default();
    advanced.networking.java.address = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    advanced.networking.java.online_mode = false;
    advanced.networking.bedrock.enabled = false;
    advanced.networking.query.enabled = false;
    advanced.networking.lan_broadcast.enabled = false;
    advanced.update_checker.enabled = false;
    (basic, advanced)
}

fn place(world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
    world.set_block_state(&pos, state, BlockFlags::empty());
}

fn test_block(world: &Arc<World>, pos: BlockPos) {
    let Some(block) = Block::from_name(BLOCK) else {
        panic!("{BLOCK} is not registered");
    };
    place(world, pos, block.default_state.id);
}

fn diamonds(count: u8) -> ItemStack {
    ItemStack::new(count, &Item::DIAMOND)
}

fn check(world: &Arc<World>) {
    assert!(SAW_PROVIDER.load(Ordering::Relaxed));
    world
        .level
        .loaded_chunks
        .insert(Vector2::new(100, 100), ChunkData::empty_sync(100, 100));
    let diamond = ItemResource::of_item(&Item::DIAMOND);

    // The second mod inserts through the capability, and an aborted extract changes nothing.
    let block = BlockPos::new(1601, 64, 1601);
    test_block(world, block);
    let handler = item::BLOCK
        .get_capability(world, &block, Some(BlockDirection::Up))
        .unwrap();
    let mut transaction = Transaction::open_root().unwrap();
    assert_eq!(handler.insert(&diamond, 5, &mut transaction), 5);
    transaction.commit();
    let mut transaction = Transaction::open_root().unwrap();
    assert_eq!(handler.extract(&diamond, 2, &mut transaction), 2);
    drop(transaction);
    assert_eq!(block_storage(block).get_amount_as_long(0), 5);

    // A hopper above pushes one item into the block.
    let hopper_pos = block.up();
    place(world, hopper_pos, Block::HOPPER.default_state.id);
    let hopper = Arc::new(HopperBlockEntity::new(hopper_pos, FacingHopper::Down));
    hopper.set_stack(0, diamonds(3));
    world.add_block_entity(hopper.clone());
    hopper.tick(world);
    assert_eq!(block_storage(block).get_amount_as_long(0), 6);
    assert_eq!(hopper.get_stack(0).item_count, 2);

    // A hopper below pulls one item out of the block.
    let source = BlockPos::new(1605, 66, 1601);
    test_block(world, source);
    let mut transaction = Transaction::open_root().unwrap();
    block_storage(source).insert(&diamond, 4, &mut transaction);
    transaction.commit();
    let below = source.down();
    place(world, below, Block::HOPPER.default_state.id);
    let puller = Arc::new(HopperBlockEntity::new(below, FacingHopper::Down));
    world.add_block_entity(puller.clone());
    puller.tick(world);
    assert_eq!(block_storage(source).get_amount_as_long(0), 3);
    assert_eq!(puller.get_stack(0).item_count, 1);

    // A dropper drops one item into the block in front of it.
    let target = BlockPos::new(1609, 66, 1601);
    test_block(world, target);
    let dropper_pos = target.down();
    let mut props = DispenserLikeProperties::from_state_id(Block::DROPPER.default_state.id);
    props.facing = Facing::Up;
    place(world, dropper_pos, props.to_state_id(&Block::DROPPER));
    let dropper = Arc::new(DropperBlockEntity::new(dropper_pos));
    dropper.set_stack(0, diamonds(2));
    world.add_block_entity(dropper.clone());
    let Some(behaviour) = world.block_registry.get_pumpkin_block(Block::DROPPER.id) else {
        panic!("the dropper has no behaviour");
    };
    behaviour.on_scheduled_tick(OnScheduledTickArgs {
        world,
        block: &Block::DROPPER,
        position: &dropper_pos,
    });
    assert_eq!(block_storage(target).get_amount_as_long(0), 1);
    assert_eq!(dropper.get_stack(0).item_count, 1);

    // A cache on the block hears when the test mod drops the storage.
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let cache = BlockCapabilityCache::create_with_listener(
        *item::BLOCK,
        world,
        block,
        None,
        || true,
        move || {
            counter.fetch_add(1, Ordering::Relaxed);
        },
    );
    assert_eq!(
        cache
            .get_capability()
            .unwrap()
            .unwrap()
            .get_amount_as_long(0),
        6
    );
    remove_block_storage(world, block);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        cache
            .get_capability()
            .unwrap()
            .unwrap()
            .get_amount_as_long(0),
        0
    );
}

#[test]
fn a_second_mod_and_vanilla_blocks_use_the_item_handler_of_the_test_block() {
    let dir = tempfile::tempdir().unwrap();
    // The mod configs and the plugin data folders are in the working directory.
    std::env::set_current_dir(dir.path()).unwrap();
    startup::STARTUP_HOOK
        .set(ironpumpkin_mods::init_mods)
        .unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let (basic, advanced) = configs(dir.path());
        let telemetry = TelemetryConfig {
            enabled: false,
            ..TelemetryConfig::default()
        };
        let pumpkin = PumpkinServer::new(
            basic,
            advanced,
            telemetry,
            VanillaData::default(),
            Vec::new(),
            false,
        )
        .await
        .unwrap();
        let world = pumpkin.server.worlds.load()[0].clone();
        check(&world);
    });
    runtime.shutdown_background();
}
