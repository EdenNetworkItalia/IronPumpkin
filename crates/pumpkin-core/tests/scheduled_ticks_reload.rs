//! A scheduled tick that a chunk saved runs after the chunk loads again, with the delay that was
//! left. Vanilla adds the tick containers of a chunk to the level when the chunk loads
//! (`LevelChunk.registerTickContainerInLevel`).
#![expect(clippy::unwrap_used, reason = "a test fails on the first error")]

use std::{
    path::PathBuf,
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

use arc_swap::ArcSwap;
use pumpkin_config::world::LevelConfig;
use pumpkin_core::{
    block::registry::default_registry,
    world::{World, WorldPortal},
};
use pumpkin_data::{
    Block, BlockStateId, block_properties::RedstoneOreLikeProperties, dimension::Dimension,
    fluid::Fluid,
};
use pumpkin_util::{
    math::{position::BlockPos, vector2::Vector2},
    world_seed::Seed,
};
use pumpkin_world::{
    level::Level, tick::TickPriority, world::BlockFlags, world::WorldPortalExt,
    world_info::LevelData,
};

const CHUNK: Vector2<i32> = Vector2::new(2, 2);
/// The delay that the water has left when the chunk saves.
const WATER_DELAY: u8 = 5;
/// The delay that the lamp has left when the chunk saves.
const LAMP_DELAY: u8 = 10;

fn world_in(folder: PathBuf, level_info: Arc<ArcSwap<LevelData>>) -> Arc<World> {
    let dimension = Dimension::OVERWORLD.clone();
    let world = Arc::new(World::load(
        Level::from_root_folder(&LevelConfig::default(), folder, 0, dimension.clone()),
        level_info,
        dimension,
        default_registry(),
        Weak::new(),
    ));
    // Chunk generation needs the portal, as `Server::new` stores it.
    let portal: Arc<dyn WorldPortalExt> = Arc::new(WorldPortal(world.clone()));
    world.level.world_portal.store(Arc::new(Some(portal)));
    world
}

async fn force_and_wait(world: &Arc<World>) {
    world.set_chunk_forced(CHUNK, true);
    let deadline = Instant::now() + Duration::from_secs(180);
    while !world.level.is_chunk_loaded(&CHUNK) {
        assert!(Instant::now() < deadline, "the forced chunk did not load");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    world.update_active_chunks();
}

fn lit_lamp() -> BlockStateId {
    RedstoneOreLikeProperties { lit: true }.to_state_id(&Block::REDSTONE_LAMP)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_ticks_of_a_saved_chunk_run_after_it_loads_again() {
    let folder = tempfile::tempdir().unwrap();
    let level_info = Arc::new(ArcSwap::from_pointee(LevelData::default(Seed(0))));
    let lamp = BlockPos::new(CHUNK.x * 16 + 8, 210, CHUNK.y * 16 + 8);
    let water = BlockPos::new(CHUNK.x * 16 + 3, 200, CHUNK.y * 16 + 3);

    let saved = world_in(folder.path().to_path_buf(), level_info.clone());
    force_and_wait(&saved).await;
    saved.set_block_state(&lamp, lit_lamp(), BlockFlags::empty());
    saved.schedule_block_tick(
        &Block::REDSTONE_LAMP,
        lamp,
        LAMP_DELAY,
        TickPriority::Normal,
    );
    saved.set_block_state(&water, Block::WATER.default_state.id, BlockFlags::empty());
    saved.schedule_fluid_tick(&Fluid::WATER, water, WATER_DELAY, TickPriority::Normal);
    saved.level.shutdown().await;
    drop(saved);

    let loaded = world_in(folder.path().to_path_buf(), level_info);
    force_and_wait(&loaded).await;
    assert_eq!(loaded.get_block_state_id(&lamp), lit_lamp());
    assert!(
        loaded.is_block_tick_scheduled(&lamp, &Block::REDSTONE_LAMP),
        "the lamp tick is not in the loaded chunk"
    );
    assert!(
        loaded.is_fluid_tick_scheduled(&water, &Fluid::WATER),
        "the water tick is not in the loaded chunk"
    );
    let below_water = |world: &Arc<World>| world.get_block(&water.down()).id == Block::WATER.id;

    // The delay of the water counts the steps from 0: the tick runs on step `WATER_DELAY`.
    for _ in 0..WATER_DELAY {
        loaded.tick_chunks();
    }
    assert!(!below_water(&loaded), "the water flowed before its delay");
    loaded.tick_chunks();
    assert!(below_water(&loaded), "the water did not flow after a load");

    for _ in WATER_DELAY + 1..LAMP_DELAY {
        loaded.tick_chunks();
    }
    assert_eq!(
        loaded.get_block_state_id(&lamp),
        lit_lamp(),
        "the lamp went out before its delay"
    );
    loaded.tick_chunks();
    assert_eq!(
        loaded.get_block_state_id(&lamp),
        Block::REDSTONE_LAMP.default_state.id,
        "the lamp did not go out after a load"
    );
    loaded.level.shutdown().await;
}
