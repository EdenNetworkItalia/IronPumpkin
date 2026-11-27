//! Forcing a chunk loads it and holds it, as `/forceload add` and the plugin call do.
#![expect(clippy::unwrap_used, reason = "a test fails on the first error")]

mod support;

use std::{
    thread::sleep,
    time::{Duration, Instant},
};

use std::{
    num::NonZeroI32,
    sync::{Arc, atomic::Ordering},
};

use pumpkin_core::{
    block::entities::{BlockEntity, furnace::FurnaceBlockEntity},
    command::CommandSender,
    entity::EntityBase,
};
use pumpkin_data::{Block, BlockStateId, block_properties::RedstoneOreLikeProperties};
use pumpkin_util::math::{position::BlockPos, vector2::Vector2};
use pumpkin_world::{chunk_system::ChunkLoading, tick::TickPriority, world::BlockFlags};

fn wait_until_loaded(chunk: Vector2<i32>) {
    let world = support::world();
    let deadline = Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&chunk) {
        assert!(Instant::now() < deadline, "the forced chunk did not load");
        sleep(Duration::from_millis(10));
    }
}

#[test]
fn forcing_a_chunk_loads_it() {
    let _serial = support::serial();
    let world = support::world();
    let chunk = Vector2::new(30, 30);

    assert!(world.set_chunk_forced(chunk, true));
    assert!(!world.set_chunk_forced(chunk, true));
    assert!(world.forced_chunks.lock().unwrap().contains(&chunk));
    wait_until_loaded(chunk);

    assert!(world.set_chunk_forced(chunk, false));
    assert!(!world.set_chunk_forced(chunk, false));
    assert!(!world.forced_chunks.lock().unwrap().contains(&chunk));
}

/// A forced chunk that no player is near is entity ticking: everything ticks in it.
#[test]
fn a_forced_chunk_without_a_player_is_active() {
    let _serial = support::serial();
    let world = support::world();
    let chunk = Vector2::new(32, 32);

    world.set_chunk_forced(chunk, true);
    wait_until_loaded(chunk);
    world.update_active_chunks();

    assert!(world.active_chunks.read().unwrap().contains(&chunk));
}

#[test]
fn concurrent_changes_of_one_chunk_keep_the_ticket_and_the_set_in_agreement() {
    let _serial = support::serial();
    let world = support::world();
    let chunk = Vector2::new(33, 33);

    std::thread::scope(|scope| {
        for offset in 0..4 {
            let world = &world;
            scope.spawn(move || {
                for step in 0..500 {
                    world.set_chunk_forced(chunk, (step + offset) % 2 == 0);
                }
            });
        }
    });

    let forced = world.forced_chunks.lock().unwrap().contains(&chunk);
    let tickets = world
        .level
        .chunk_loading
        .lock()
        .unwrap()
        .ticket
        .get(&chunk)
        .map_or(0, |levels| {
            levels
                .iter()
                .filter(|level| **level == ChunkLoading::FORCED_TICKET_LEVEL)
                .count()
        });
    assert_eq!(tickets, usize::from(forced));
}

fn run(command: &str) -> Result<i32, String> {
    let server = support::server();
    let source = CommandSender::Console.into_source(server).with_silent();
    server
        .command_dispatcher
        .load()
        .execute_input(command, &source)
        .map_err(|error| format!("{:?}", error.error_type))
}

/// `/forceload` counts the chunks that changed, fails when none changed and rejects what
/// vanilla rejects.
#[test]
fn forceload_counts_changed_chunks_and_fails_when_none_changed() {
    let _serial = support::serial();

    assert_eq!(run("forceload add 544 544 560 544"), Ok(2));
    assert_eq!(run("forceload add 544 544 560 560"), Ok(2));
    let again = run("forceload add 544 544 560 560").unwrap_err();
    assert!(
        again.contains("commands.forceload.added.failure"),
        "{again}"
    );

    assert_eq!(run("forceload remove 544 544 560 560"), Ok(4));
    let again = run("forceload remove 544 544").unwrap_err();
    assert!(
        again.contains("commands.forceload.removed.failure"),
        "{again}"
    );

    assert_eq!(run("forceload add 544 544"), Ok(1));
    assert_eq!(run("forceload remove all"), Ok(0));
    assert!(support::world().forced_chunks.lock().unwrap().is_empty());
}

#[test]
fn forceload_rejects_a_huge_area_and_coordinates_outside_the_world() {
    let _serial = support::serial();

    let huge = run("forceload add 0 0 1048575 1048575").unwrap_err();
    assert!(huge.contains("commands.forceload.toobig"), "{huge}");
    let outside = run("forceload add 30000000 0").unwrap_err();
    assert!(outside.contains("argument.pos.outofworld"), "{outside}");
    assert!(support::world().forced_chunks.lock().unwrap().is_empty());
}

/// Vanilla `ChunkMap.FORCED_TICKET_LEVEL` is 31, two levels below FULL: the ticket spreads by one
/// per chunk, so the chunks within two of a forced chunk load at FULL.
#[test]
fn the_chunks_within_two_of_a_forced_chunk_load_at_full() {
    let _serial = support::serial();
    let world = support::world();
    let chunk = Vector2::new(70, 70);

    world.set_chunk_forced(chunk, true);
    for dx in -2..=2 {
        for dz in -2..=2 {
            wait_until_loaded(chunk.add_raw(dx, dz));
        }
    }

    world.set_chunk_forced(chunk, false);
    world.update_active_chunks();
}

const fn block_at(chunk: Vector2<i32>, y: i32) -> BlockPos {
    BlockPos::new(chunk.x * 16 + 8, y, chunk.y * 16 + 8)
}

fn lit_lamp() -> BlockStateId {
    RedstoneOreLikeProperties { lit: true }.to_state_id(&Block::REDSTONE_LAMP)
}

/// A furnace that burns for 100 ticks, with nothing to smelt.
fn burning_furnace(pos: BlockPos) -> Arc<FurnaceBlockEntity> {
    let world = support::world();
    world.set_block_state(&pos, Block::FURNACE.default_state.id, BlockFlags::empty());
    let furnace = Arc::new(FurnaceBlockEntity::new(pos));
    furnace.lit_time_remaining.store(100, Ordering::Relaxed);
    furnace.lit_total_time.store(100, Ordering::Relaxed);
    world.add_block_entity(furnace.clone() as Arc<dyn BlockEntity>);
    furnace
}

/// A forced chunk is entity ticking (level 31); its 8 neighbours are block ticking (level 32),
/// and the ring past them is loaded at FULL (level 33) and does not tick. Vanilla runs block
/// entities (`Level.tickBlockEntities` through `shouldTickBlocksAt`) and scheduled ticks
/// (`LevelTicks` through `isPositionTicking`) in block ticking chunks, and random ticks
/// (`ServerChunkCache.tickChunks`, `forEachBlockTickingChunk` through
/// `forEachEntityTickingChunk`) in entity ticking chunks only.
#[test]
fn the_neighbours_of_a_forced_chunk_tick_blocks_and_not_random_ticks() {
    let _serial = support::serial();
    let world = support::world();
    let server = support::server();
    let forced = Vector2::new(60, 60);
    let neighbour = Vector2::new(61, 60);
    let outside = Vector2::new(62, 60);

    world.set_chunk_forced(forced, true);
    for chunk in [forced, neighbour, outside] {
        wait_until_loaded(chunk);
    }
    world.update_active_chunks();
    let entity = world.active_chunks.read().unwrap().clone();
    let block = world.block_ticking_chunks.read().unwrap().clone();
    assert!(entity.contains(&forced) && block.contains(&forced));
    assert!(!entity.contains(&neighbour) && block.contains(&neighbour));
    assert!(!entity.contains(&outside) && !block.contains(&outside));

    let crop = |chunk| {
        let pos = block_at(chunk, 200);
        world.set_block_state(
            &pos.down(),
            Block::FARMLAND.default_state.id,
            BlockFlags::empty(),
        );
        world.set_block_state(&pos, Block::WHEAT.default_state.id, BlockFlags::empty());
        pos
    };
    let forced_crop = crop(forced);
    let neighbour_crop = crop(neighbour);
    let lamp = |chunk| {
        let pos = block_at(chunk, 210);
        world.set_block_state(&pos, lit_lamp(), BlockFlags::empty());
        world.schedule_block_tick(&Block::REDSTONE_LAMP, pos, 1, TickPriority::Normal);
        pos
    };
    let neighbour_lamp = lamp(neighbour);
    let outside_lamp = lamp(outside);
    let neighbour_furnace = burning_furnace(block_at(neighbour, 220));
    let outside_furnace = burning_furnace(block_at(outside, 220));

    let previous = world.level_info.load_full();
    world.level_info.rcu(|info| {
        let mut info = (**info).clone();
        info.game_rules.random_tick_speed = 4096;
        info
    });
    // At 4096 a section takes about one random tick per block and tick, and a crop grows with
    // a chance of 1 in 26 or more: in 400 ticks a crop that random ticks reach grows with a
    // chance above 1 - 1e-6. The loop does not stop early, so the neighbour crop gets the
    // same number of ticks.
    for _ in 0..400 {
        world.tick(server);
    }
    world.level_info.store(previous);

    assert_ne!(
        world.get_block_state_id(&forced_crop),
        Block::WHEAT.default_state.id,
        "random ticks run in the forced chunk"
    );
    assert_eq!(
        world.get_block_state_id(&neighbour_crop),
        Block::WHEAT.default_state.id,
        "random ticks do not run in a block ticking chunk"
    );
    assert_eq!(
        world.get_block_state_id(&neighbour_lamp),
        Block::REDSTONE_LAMP.default_state.id,
        "a scheduled tick runs in a block ticking chunk"
    );
    assert_eq!(world.get_block_state_id(&outside_lamp), lit_lamp());
    assert!(neighbour_furnace.lit_time_remaining.load(Ordering::Relaxed) < 100);
    assert_eq!(
        outside_furnace.lit_time_remaining.load(Ordering::Relaxed),
        100
    );

    world.set_chunk_forced(forced, false);
    world.update_active_chunks();
}

/// A scheduled tick in a loaded chunk that does not block tick keeps its trigger tick: when the
/// chunk block ticks again, the overdue tick runs on the first tick, as vanilla `LevelTicks`
/// keeps a container with a due tick until `tickCheck` passes.
#[test]
fn an_overdue_scheduled_tick_runs_on_the_first_block_tick() {
    let _serial = support::serial();
    let world = support::world();
    let server = support::server();
    let forced = Vector2::new(90, 90);
    let neighbour = Vector2::new(91, 90);
    let outside = Vector2::new(92, 90);

    world.set_chunk_forced(forced, true);
    wait_until_loaded(outside);
    world.update_active_chunks();
    assert!(
        !world
            .block_ticking_chunks
            .read()
            .unwrap()
            .contains(&outside)
    );

    let lamp = block_at(outside, 210);
    world.set_block_state(&lamp, lit_lamp(), BlockFlags::empty());
    world.schedule_block_tick(&Block::REDSTONE_LAMP, lamp, 10, TickPriority::Normal);
    for _ in 0..30 {
        world.tick(server);
    }
    assert_eq!(world.get_block_state_id(&lamp), lit_lamp());

    world.set_chunk_forced(neighbour, true);
    world.update_active_chunks();
    assert!(
        world
            .block_ticking_chunks
            .read()
            .unwrap()
            .contains(&outside)
    );
    world.tick(server);
    assert_eq!(
        world.get_block_state_id(&lamp),
        Block::REDSTONE_LAMP.default_state.id,
        "the overdue tick runs on the first block tick"
    );

    world.set_chunk_forced(neighbour, false);
    world.set_chunk_forced(forced, false);
    world.update_active_chunks();
}

/// The ring at the simulation distance plus one is at level 32 of the player's simulation
/// ticket (`31 - simulation distance`): block ticking, not entity ticking.
#[test]
fn the_ring_past_the_simulation_distance_of_a_player_is_block_ticking_only() {
    let _serial = support::serial();
    let world = support::world();
    let player = support::player("SimulationEdge");
    world.update_active_chunks();
    let center = player.get_entity().chunk_pos.load();
    let distance = NonZeroI32::from(
        support::server()
            .advanced_config
            .networking
            .java
            .simulation_distance,
    )
    .get();
    let entity = world.active_chunks.read().unwrap().clone();
    let block = world.block_ticking_chunks.read().unwrap().clone();
    for (dx, dz) in [(distance, 0), (-distance, distance), (3, -distance)] {
        let chunk = center.add_raw(dx, dz);
        assert!(
            entity.contains(&chunk) && block.contains(&chunk),
            "{dx} {dz}"
        );
    }
    let edge = distance + 1;
    for (dx, dz) in [(edge, 0), (-edge, 3), (edge, edge), (0, -edge)] {
        let chunk = center.add_raw(dx, dz);
        assert!(
            !entity.contains(&chunk) && block.contains(&chunk),
            "{dx} {dz}"
        );
    }
    let chunk = center.add_raw(distance + 2, 0);
    assert!(!entity.contains(&chunk) && !block.contains(&chunk));

    support::RUNTIME.block_on(world.remove_player(&player, false));
    world.update_active_chunks();
    assert!(
        !world
            .block_ticking_chunks
            .read()
            .unwrap()
            .contains(&center.add_raw(distance + 1, 0))
    );
}

/// `/forceload query` without a position lists the forced chunks and returns their count;
/// with a position it returns 1 for a forced chunk and fails for another.
#[test]
fn forceload_query_returns_the_count_or_the_state_of_one_chunk() {
    let _serial = support::serial();
    run("forceload remove all").unwrap();

    assert_eq!(run("forceload query"), Ok(0));
    assert_eq!(run("forceload add 1600 1600 1616 1600"), Ok(2));
    assert_eq!(run("forceload query"), Ok(2));
    assert_eq!(run("forceload query 1616 1615"), Ok(1));
    let missing = run("forceload query 1632 1600").unwrap_err();
    assert!(
        missing.contains("commands.forceload.query.failure"),
        "{missing}"
    );

    assert_eq!(run("forceload remove all"), Ok(0));
    assert_eq!(run("forceload query"), Ok(0));
}
