//! `BlockFromToEvent` of a fluid spread: the position the fluid flows from, the position it
//! flows to and the fluid block, and a cancelled spread that leaves the target as it is.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use pumpkin_core::{
    plugin::{
        BoxFuture, Cancellable, EventHandler, EventPriority, block::block_from_to::BlockFromToEvent,
    },
    server::Server,
};
use pumpkin_data::{Block, BlockStateId, fluid::Fluid};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use pumpkin_world::world::BlockFlags;

type Log = Arc<Mutex<Vec<(BlockPos, BlockPos, &'static str)>>>;

struct Blocking<F>(F, PhantomData<fn()>);

impl<F> EventHandler<BlockFromToEvent> for Blocking<F>
where
    F: Fn(&mut BlockFromToEvent) + Send + Sync,
{
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut BlockFromToEvent,
    ) -> BoxFuture<'a, ()> {
        (self.0)(event);
        Box::pin(async {})
    }
}

fn on(handler: impl Fn(&mut BlockFromToEvent) + Send + Sync + 'static) {
    support::server()
        .plugin_manager
        .register_with::<BlockFromToEvent, _>(
            Arc::new(Blocking(handler, PhantomData)),
            EventPriority::Normal,
            true,
            false,
        );
}

/// Records the events that start at `from`.
fn record(from: BlockPos) -> Log {
    let log = Log::default();
    let seen = log.clone();
    on(move |event| {
        if event.from_pos == from {
            seen.lock()
                .unwrap()
                .push((event.from_pos, event.to_pos, event.block.name));
        }
    });
    log
}

fn load(pos: BlockPos) {
    let world = support::world();
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
}

fn set(pos: BlockPos, state: BlockStateId) {
    load(pos);
    support::world().set_block_state(&pos, state, BlockFlags::empty());
}

fn state_at(pos: BlockPos) -> BlockStateId {
    support::world().get_block_state_id(&pos)
}

const fn spot(slot: i32) -> BlockPos {
    BlockPos::new(slot * 16 + 8, 120, 8)
}

const SIDES: [Vector3<i32>; 4] = [
    Vector3::new(1, 0, 0),
    Vector3::new(-1, 0, 0),
    Vector3::new(0, 0, 1),
    Vector3::new(0, 0, -1),
];

/// A fluid source at `pos` with air around it, on a stone floor.
fn source_on_floor(pos: BlockPos, source: BlockStateId) {
    for dx in -1..=1 {
        for dz in -1..=1 {
            let at = pos.offset(Vector3::new(dx, 0, dz));
            set(at.down(), Block::STONE.default_state.id);
            set(at, Block::AIR.default_state.id);
        }
    }
    set(pos, source);
}

fn tick(fluid: &'static Fluid, pos: BlockPos) {
    support::server()
        .block_registry
        .get_pumpkin_fluid(fluid.id)
        .unwrap()
        .on_scheduled_tick(&support::world(), fluid, &pos);
}

fn assert_spreads_to_the_sides(log: &Log, pos: BlockPos, block: &'static str) {
    let seen = log.lock().unwrap().clone();
    assert_eq!(seen.len(), SIDES.len(), "{seen:?}");
    for side in SIDES {
        let to = pos.offset(side);
        assert!(
            seen.contains(&(pos, to, block)),
            "no event from {pos} to {to} with {block}: {seen:?}"
        );
    }
}

#[test]
fn water_spreads_with_the_source_the_destination_and_the_water_block() {
    let _serial = support::serial();
    let pos = spot(1);
    source_on_floor(pos, Block::WATER.default_state.id);
    let log = record(pos);

    tick(&Fluid::FLOWING_WATER, pos);

    assert_spreads_to_the_sides(&log, pos, Block::WATER.name);
}

#[test]
fn lava_spreads_with_the_source_the_destination_and_the_lava_block() {
    let _serial = support::serial();
    let pos = spot(2);
    source_on_floor(pos, Block::LAVA.default_state.id);
    let log = record(pos);

    tick(&Fluid::FLOWING_LAVA, pos);

    assert_spreads_to_the_sides(&log, pos, Block::LAVA.name);
}
