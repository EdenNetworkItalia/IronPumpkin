//! The cost of the tick of an idle hopper: an empty hopper under air, and a full hopper over
//! air, so every tick tries to move items and finds no inventory. With the capability hooks
//! installed, each try also looks up `Capabilities.Item.BLOCK` of the air block.
#![expect(clippy::unwrap_used, reason = "a bench stops on a setup failure")]

use std::{
    hint::black_box,
    sync::{Arc, Weak},
};

use arc_swap::ArcSwap;
use criterion::{Criterion, criterion_group, criterion_main};
use pumpkin_config::world::LevelConfig;
use pumpkin_core::{
    block::{
        entities::{BlockEntity, hopper::HopperBlockEntity},
        registry::default_registry,
    },
    world::World,
};
use pumpkin_data::{
    Block, block_properties::FacingHopper, dimension::Dimension, item::Item, item_stack::ItemStack,
};
use pumpkin_inventory::Inventory;
use pumpkin_util::{
    math::{position::BlockPos, vector2::Vector2},
    world_seed::Seed,
};
use pumpkin_world::{chunk::ChunkData, level::Level, world::BlockFlags, world_info::LevelData};

fn bench(c: &mut Criterion) {
    // BENCH-HOOKS: installs the capability hooks and the vanilla providers.
    ironpumpkin_mods::capabilities::install_for_bench();
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
    let hopper = |pos: BlockPos, stack: Option<ItemStack>| {
        world.set_block_state(&pos, Block::HOPPER.default_state.id, BlockFlags::empty());
        let hopper = Arc::new(HopperBlockEntity::new(pos, FacingHopper::Down));
        if let Some(stack) = stack {
            hopper.set_stack(0, stack);
        }
        world.add_block_entity(hopper.clone());
        hopper
    };
    let empty = hopper(BlockPos::new(2, 64, 2), None);
    let full = hopper(
        BlockPos::new(6, 64, 6),
        Some(ItemStack::new(64, &Item::STONE)),
    );
    for slot in 1..5 {
        full.set_stack(slot, ItemStack::new(64, &Item::STONE));
    }
    let mut group = c.benchmark_group("hopper_tick");
    group.bench_function("empty_under_air", |b| {
        b.iter(|| empty.tick(black_box(&world)));
    });
    group.bench_function("full_over_air", |b| {
        b.iter(|| full.tick(black_box(&world)));
    });
    group.finish();
    runtime.block_on(world.level.shutdown());
}

criterion_group!(benches, bench);
criterion_main!(benches);
