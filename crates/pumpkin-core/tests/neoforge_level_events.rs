//! The `NeoForge` level, block and tick events (`event.level`, `event.level.block`,
//! `event.tick`, task 2.13 of `openspec/changes/neoforge-shaped-plugin-api`) as Pumpkin events:
//! one test per row that fires from its `NeoForge` fire site, and the cancel and write-back tests
//! of the family, against a server of the support module.
//!
//! Each test works at its own position and filters its handlers by it, because the handlers stay
//! registered on the shared server.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use pumpkin_core::{
    entity::{
        Entity,
        ai::goal::{Goal, place_block::PlaceBlockGoal},
        mob::enderman::EndermanEntity,
        player::Player,
    },
    plugin::{
        BoxFuture, Cancellable, EventHandler, EventPriority, Payload,
        block::{
            block_break::BlockBreakEvent, block_exp::BlockExpEvent,
            block_multi_place::BlockMultiPlaceEvent, block_place::BlockPlaceEvent,
        },
        level::BlockEventEntityPlaceEvent,
    },
    server::Server,
};
use pumpkin_data::{Block, BlockDirection, BlockStateId, entity::EntityType};
use pumpkin_protocol::{codec::var_int::VarInt, java::server::play::SUseItemOn};
use pumpkin_util::{
    math::{position::BlockPos, vector3::Vector3},
    permission::PermissionLvl,
};
use pumpkin_world::world::BlockFlags;

type Log = Arc<Mutex<Vec<String>>>;

fn log() -> Log {
    Arc::new(Mutex::new(Vec::new()))
}

fn push(log: &Log, entry: impl Into<String>) {
    log.lock().unwrap().push(entry.into());
}

fn entries(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
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

/// Registers a blocking handler for `E` with priority `Normal` and without receive-canceled.
fn on<E, F>(handler: F)
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&mut E) + Send + Sync + 'static,
{
    on_with::<E, F>(EventPriority::Normal, false, handler);
}

fn on_with<E, F>(priority: EventPriority, receive_canceled: bool, handler: F)
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&mut E) + Send + Sync + 'static,
{
    support::server().plugin_manager.register_with::<E, _>(
        Arc::new(Blocking(handler, PhantomData)),
        priority,
        true,
        receive_canceled,
    );
}

/// Loads the chunk of `pos` for the rest of the process.
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

/// Loads the chunk of `pos` and sets `pos` to `state`.
fn set(pos: BlockPos, state: BlockStateId) {
    load(pos);
    support::world().set_block_state(&pos, state, BlockFlags::empty());
}

fn state_at(pos: BlockPos) -> BlockStateId {
    support::world().get_block_state_id(&pos)
}

/// A position of the test `slot`, away from the other tests. The blocks around it in x and z
/// stay in one chunk.
const fn spot(slot: i32) -> BlockPos {
    BlockPos::new(slot * 16 + 8, 120, 8)
}

/// A player that the spawn protection does not stop: the world spawn depends on the seed.
fn builder(name: &str) -> Arc<Player> {
    let player = support::player(name);
    player.permission_lvl.store(PermissionLvl::Four);
    player
}

/// Breaks `pos` with `player` as the cause and lets the block drop its loot.
fn break_at(player: &Arc<Player>, pos: BlockPos) -> bool {
    support::world()
        .break_block(&pos, Some(player), BlockFlags::NOTIFY_ALL)
        .is_some()
}

/// `BreakBlockEvent` (`BlockBreakEvent`) fires in `World::break_block` with the player, the
/// position and the state of the block, before the block changes.
#[test]
fn one_event_per_fire_site_break_block_event() {
    let _serial = support::serial();
    let player = builder("Breaker");
    let pos = spot(4);
    let state = Block::STONE.default_state.id;
    set(pos, state);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<BlockBreakEvent, _>(move |event| {
        if event.block_position != pos {
            return;
        }
        push(
            &seen_by_handler,
            format!(
                "{} {} {} {} {} {}",
                event.player.as_ref().unwrap().gameprofile.name,
                event.block.name,
                event.state.id == state,
                event.drop,
                state_at(pos) == state,
                event.exp,
            ),
        );
    });

    assert!(break_at(&player, pos));
    // Stone drops no experience.
    assert_eq!(entries(&seen), ["Breaker stone true true true 0"]);
    assert!(Block::from_state_id(state_at(pos)).is_air());
}

/// Scenario "Cancel a block break": the block stays, and a later handler without
/// receive-canceled does not run.
#[test]
fn cancel_break_block_event() {
    let _serial = support::serial();
    let player = builder("CancelBreaker");
    let pos = spot(5);
    set(pos, Block::STONE.default_state.id);
    let seen = log();
    let (canceller, later, observer) = (seen.clone(), seen.clone(), seen.clone());
    on_with::<BlockBreakEvent, _>(EventPriority::High, false, move |event| {
        if event.block_position == pos {
            push(&canceller, "cancel");
            event.set_cancelled(true);
        }
    });
    on::<BlockBreakEvent, _>(move |event| {
        if event.block_position == pos {
            push(&later, "later");
        }
    });
    on_with::<BlockBreakEvent, _>(EventPriority::Low, true, move |event| {
        if event.block_position == pos {
            push(&observer, format!("observer {}", event.cancelled()));
        }
    });

    assert!(!break_at(&player, pos));
    assert_eq!(entries(&seen), ["cancel", "observer true"]);
    assert_eq!(state_at(pos), Block::STONE.default_state.id);
}

/// The experience of `BlockBreakEvent` is settable: the break drops what the last handler set.
#[test]
fn change_a_settable_field_break_block_event() {
    let _serial = support::serial();
    let player = builder("XpBreaker");
    let pos = spot(6);
    set(pos, Block::COAL_ORE.default_state.id);
    on::<BlockBreakEvent, _>(move |event| {
        if event.block_position == pos {
            event.exp = 7;
        }
    });
    let dropped = log();
    let dropped_by_handler = dropped.clone();
    on::<BlockExpEvent, _>(move |event| {
        if event.block_pos == pos {
            push(&dropped_by_handler, event.exp.to_string());
        }
    });

    assert!(break_at(&player, pos));
    assert_eq!(entries(&dropped), ["7"]);
}

/// The use of a block item on the top of `clicked`.
const fn use_on_top(clicked: BlockPos) -> SUseItemOn {
    SUseItemOn {
        hand: VarInt(0),
        position: clicked,
        face: VarInt(1),
        cursor_pos: Vector3::new(0.5, 1.0, 0.5),
        inside_block: false,
        is_against_world_border: false,
        sequence: VarInt(1),
    }
}

/// Places `block` with `player` on the top of `clicked`, as `BlockItem.place` does.
fn place_on_top(player: &Arc<Player>, block: &'static Block, clicked: BlockPos) -> bool {
    let server = support::server();
    server
        .block_registry
        .place_block(
            player,
            block,
            server,
            &use_on_top(clicked),
            clicked,
            BlockDirection::Up,
        )
        .unwrap()
        .is_some()
}

/// Moves `player` next to `pos`, where it does not block the placement.
fn stand_near(player: &Player, pos: BlockPos) {
    player.living_entity.entity.set_pos(Vector3::new(
        f64::from(pos.0.x) - 2.5,
        f64::from(pos.0.y),
        f64::from(pos.0.z) + 0.5,
    ));
}

/// `BlockEvent.EntityPlaceEvent` (`BlockPlaceEvent`) fires before the block changes, with the
/// placed state, the replaced state and the player.
#[test]
fn one_event_per_fire_site_entity_place_event() {
    let _serial = support::serial();
    let player = builder("Placer");
    let floor = spot(7);
    set(floor, Block::STONE.default_state.id);
    let pos = floor.up();
    set(pos, Block::AIR.default_state.id);
    stand_near(&player, pos);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<BlockPlaceEvent, _>(move |event| {
        if event.block_position != pos {
            return;
        }
        push(
            &seen_by_handler,
            format!(
                "{} {} {} {} {} {}",
                event.player.gameprofile.name,
                event.block_placed.name,
                event.block_placed_against.name,
                event.placed_state.id == Block::GLASS.default_state.id,
                event.replaced_state.is_air(),
                Block::from_state_id(state_at(pos)).is_air(),
            ),
        );
    });

    assert!(place_on_top(&player, &Block::GLASS, floor));
    assert_eq!(entries(&seen), ["Placer glass stone true true true"]);
    assert_eq!(state_at(pos), Block::GLASS.default_state.id);
}

/// A cancelled `BlockPlaceEvent` places nothing.
#[test]
fn cancel_entity_place_event() {
    let _serial = support::serial();
    let player = builder("CancelPlacer");
    let floor = spot(8);
    set(floor, Block::STONE.default_state.id);
    let pos = floor.up();
    set(pos, Block::AIR.default_state.id);
    stand_near(&player, pos);
    on::<BlockPlaceEvent, _>(move |event| {
        if event.block_position == pos {
            event.set_cancelled(true);
        }
    });

    assert!(!place_on_top(&player, &Block::GLASS, floor));
    assert!(Block::from_state_id(state_at(pos)).is_air());
}

/// `BlockEvent.EntityMultiPlaceEvent` (`BlockMultiPlaceEvent`) fires for a door with both
/// halves, after `BlockPlaceEvent`, and a cancel places neither half.
#[test]
fn cancel_entity_multi_place_event() {
    let _serial = support::serial();
    let player = builder("DoorPlacer");
    let floor = spot(9);
    set(floor, Block::STONE.default_state.id);
    let (lower, upper) = (floor.up(), floor.up().up());
    set(lower, Block::AIR.default_state.id);
    set(upper, Block::AIR.default_state.id);
    stand_near(&player, lower);
    let seen = log();
    let single = seen.clone();
    on::<BlockPlaceEvent, _>(move |event| {
        if event.block_position == lower {
            push(&single, "place");
        }
    });
    let multi = seen.clone();
    on::<BlockMultiPlaceEvent, _>(move |event| {
        let positions: Vec<_> = event.placed_blocks.iter().map(|(pos, _)| *pos).collect();
        if positions.first() != Some(&lower) {
            return;
        }
        let replaced_air = event
            .replaced_blocks
            .iter()
            .all(|(_, state)| state.is_air());
        push(
            &multi,
            format!(
                "multi {} {} {}",
                positions == [lower, upper],
                Block::from_state_id(event.placed_blocks[1].1).name,
                replaced_air,
            ),
        );
        event.set_cancelled(true);
    });

    assert!(!place_on_top(&player, &Block::OAK_DOOR, floor));
    assert_eq!(entries(&seen), ["place", "multi true oak_door true"]);
    assert!(Block::from_state_id(state_at(lower)).is_air());
    assert!(Block::from_state_id(state_at(upper)).is_air());
}

/// An enderman that places its carried block on a stone floor around `center`, as
/// `EndermanLeaveBlockGoal` does. Returns the goal and the enderman.
fn enderman_over_floor(center: BlockPos) -> (PlaceBlockGoal, Arc<EndermanEntity>) {
    for dx in -1..=1 {
        for dz in -1..=1 {
            let floor = center.offset(Vector3::new(dx, -1, dz));
            set(floor, Block::STONE.default_state.id);
            set(floor.up(), Block::AIR.default_state.id);
            set(floor.up().up(), Block::AIR.default_state.id);
        }
    }
    let world = support::world();
    let enderman = EndermanEntity::new(Entity::new(
        world,
        Vector3::new(
            f64::from(center.0.x) + 0.5,
            f64::from(center.0.y),
            f64::from(center.0.z) + 0.5,
        ),
        &EntityType::ENDERMAN,
    ));
    enderman.set_carried_block(Some(Block::DIRT.default_state.id));
    (PlaceBlockGoal::new(enderman.clone()), enderman)
}

/// The goal picks a random target: tick it until the enderman placed or tried to place.
fn tick_until(goal: &mut PlaceBlockGoal, enderman: &EndermanEntity, done: impl Fn() -> bool) {
    for _ in 0..1000 {
        goal.tick(enderman);
        if done() {
            return;
        }
    }
    panic!("the enderman never chose a target on the floor");
}

/// `BlockEvent.EntityPlaceEvent` placed by an entity (`BlockEventEntityPlaceEvent`) fires
/// before the enderman sets its carried block, with the entity and the states.
#[test]
fn one_event_per_fire_site_entity_place_event_of_an_entity() {
    let _serial = support::serial();
    let center = spot(10);
    let (mut goal, enderman) = enderman_over_floor(center);
    let seen = log();
    let seen_by_handler = seen.clone();
    let id = enderman.mob_entity.living_entity.entity.entity_id;
    on::<BlockEventEntityPlaceEvent, _>(move |event| {
        if event.entity.get_entity().entity_id != id {
            return;
        }
        push(
            &seen_by_handler,
            format!(
                "{} {} {} {} {}",
                event.entity.get_entity().entity_type.resource_name,
                event.placed_state.id == Block::DIRT.default_state.id,
                event.replaced_state.is_air(),
                event.placed_against.id == Block::STONE.default_state.id,
                Block::from_state_id(state_at(event.pos)).is_air(),
            ),
        );
    });

    tick_until(&mut goal, &enderman, || !entries(&seen).is_empty());
    assert_eq!(entries(&seen), ["enderman true true true true"]);
    assert_eq!(enderman.get_carried_block(), None);
}

/// A cancelled `BlockEventEntityPlaceEvent` keeps the block and the carried block.
#[test]
fn cancel_entity_place_event_of_an_entity() {
    let _serial = support::serial();
    let center = spot(11);
    let (mut goal, enderman) = enderman_over_floor(center);
    let tried = log();
    let tried_by_handler = tried.clone();
    let id = enderman.mob_entity.living_entity.entity.entity_id;
    on::<BlockEventEntityPlaceEvent, _>(move |event| {
        if event.entity.get_entity().entity_id == id {
            push(&tried_by_handler, event.pos.to_string());
            event.set_cancelled(true);
        }
    });

    tick_until(&mut goal, &enderman, || !entries(&tried).is_empty());
    let target = entries(&tried)[0].clone();
    for dx in -1..=1 {
        for dz in -1..=1 {
            let pos = center.offset(Vector3::new(dx, 0, dz));
            assert!(
                Block::from_state_id(state_at(pos)).is_air(),
                "{pos} changed, target {target}"
            );
        }
    }
    assert_eq!(
        enderman.get_carried_block(),
        Some(Block::DIRT.default_state.id)
    );
}
