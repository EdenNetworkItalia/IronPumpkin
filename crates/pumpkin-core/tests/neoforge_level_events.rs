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
    block::{BlockHitResult, RandomTickArgs, registry::BlockActionResult},
    command::CommandSender,
    entity::{
        Entity,
        ai::goal::{Goal, place_block::PlaceBlockGoal},
        item::ItemEntity,
        mob::enderman::EndermanEntity,
        player::Player,
    },
    plugin::{
        BoxFuture, Cancellable, EventHandler, EventPriority, Payload,
        block::{
            block_break::BlockBreakEvent, block_exp::BlockExpEvent,
            block_multi_place::BlockMultiPlaceEvent, block_place::BlockPlaceEvent,
        },
        entity::entity_explode::EntityExplodeEvent,
        level::{
            BlockEventEntityPlaceEvent, ChunkWatchEventUnWatch, CreateFluidSourceEvent,
            CropGrowEventPost, EntityTickEventPost, EntityTickEventPre, ExplosionEventDetonate,
            ExplosionKnockbackEvent, GameRuleChangedEvent, NoteBlockEventChange, PistonEventPost,
            PistonMoveType, PlayerTickEventPost, PlayerTickEventPre,
        },
        world::{chunk_load::ChunkLoad, chunk_unload::ChunkUnloadEvent},
    },
    server::Server,
    world::{ExplosionInteraction, chunker},
};
use pumpkin_data::{
    Block, BlockDirection, BlockStateId,
    block_properties::{NoteBlockLikeProperties, StickyPistonLikeProperties, WheatLikeProperties},
    entity::EntityType,
    fluid::Fluid,
    game_rules::{GameRule, GameRuleValue},
    item::Item,
    item_stack::ItemStack,
};
use pumpkin_protocol::{codec::var_int::VarInt, java::server::play::SUseItemOn};
use pumpkin_util::{
    math::{position::BlockPos, vector2::Vector2, vector3::Vector3},
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

/// `PlayerTickEvent.Pre` and `.Post` (`PlayerTickEventPre`, `PlayerTickEventPost`) fire at the
/// start and at the end of the tick of the player, once each.
#[test]
fn one_event_per_fire_site_player_tick_events() {
    let _serial = support::serial();
    let player = support::player("Ticker");
    let seen = log();
    let (pre, post) = (seen.clone(), seen.clone());
    on::<PlayerTickEventPre, _>(move |event| {
        if event.player.gameprofile.name == "Ticker" {
            push(&pre, "pre");
        }
    });
    on::<PlayerTickEventPost, _>(move |event| {
        if event.player.gameprofile.name == "Ticker" {
            push(&post, "post");
        }
    });

    player.tick(support::server());

    assert_eq!(entries(&seen), ["pre", "post"]);
}

/// An item entity at `pos`, in a chunk that the world ticks.
fn item_at(pos: BlockPos) -> Arc<ItemEntity> {
    load(pos);
    let world = support::world();
    world
        .forced_chunks
        .lock()
        .unwrap()
        .insert(pos.chunk_position());
    let entity = Entity::new(
        world.clone(),
        Vector3::new(
            f64::from(pos.0.x) + 0.5,
            f64::from(pos.0.y),
            f64::from(pos.0.z) + 0.5,
        ),
        &EntityType::ITEM,
    );
    let item = Arc::new(ItemEntity::new(entity, ItemStack::new(1, &Item::STICK)));
    world.spawn_entity(item.clone());
    item
}

fn age(item: &ItemEntity) -> i32 {
    item.get_entity()
        .age
        .load(std::sync::atomic::Ordering::Relaxed)
}

/// `EntityTickEvent.Pre` and `.Post` (`EntityTickEventPre`, `EntityTickEventPost`) fire around
/// the tick of an entity in the world tick.
#[test]
fn one_event_per_fire_site_entity_tick_events() {
    let _serial = support::serial();
    let item = item_at(spot(12));
    let id = item.get_entity().entity_id;
    let seen = log();
    let (pre, post) = (seen.clone(), seen.clone());
    on::<EntityTickEventPre, _>(move |event| {
        if event.entity.get_entity().entity_id == id {
            push(
                &pre,
                format!(
                    "pre {}",
                    event
                        .entity
                        .get_entity()
                        .age
                        .load(std::sync::atomic::Ordering::Relaxed)
                ),
            );
        }
    });
    on::<EntityTickEventPost, _>(move |event| {
        if event.entity.get_entity().entity_id == id {
            push(
                &post,
                format!(
                    "post {}",
                    event
                        .entity
                        .get_entity()
                        .age
                        .load(std::sync::atomic::Ordering::Relaxed)
                ),
            );
        }
    });

    support::world().tick(support::server());

    assert_eq!(entries(&seen), ["pre 0", "post 1"]);
}

/// A cancelled `EntityTickEventPre` skips the tick and `EntityTickEventPost`.
#[test]
fn cancel_entity_tick_event_pre() {
    let _serial = support::serial();
    let item = item_at(spot(13));
    let id = item.get_entity().entity_id;
    let seen = log();
    let post = seen.clone();
    on::<EntityTickEventPre, _>(move |event| {
        if event.entity.get_entity().entity_id == id {
            event.set_cancelled(true);
        }
    });
    on::<EntityTickEventPost, _>(move |event| {
        if event.entity.get_entity().entity_id == id {
            push(&post, "post");
        }
    });

    support::world().tick(support::server());

    assert!(entries(&seen).is_empty());
    assert_eq!(age(&item), 0);
}

/// Uses the note block at `pos` with `player`, as a right click without an item.
fn tune(player: &Arc<Player>, pos: BlockPos) -> BlockActionResult {
    let server = support::server();
    server.block_registry.on_use(
        &Block::NOTE_BLOCK,
        player,
        &pos,
        &BlockHitResult {
            face: &BlockDirection::Up,
            cursor_pos: &Vector3::new(0.5, 1.0, 0.5),
        },
        server,
        &support::world(),
    )
}

fn note_at(pos: BlockPos) -> u8 {
    NoteBlockLikeProperties::from_state_id(state_at(pos)).note
}

/// A note block at `pos` with the note 0 and air above.
fn note_block_at(pos: BlockPos) {
    set(pos, Block::NOTE_BLOCK.default_state.id);
    set(pos.up(), Block::AIR.default_state.id);
    assert_eq!(note_at(pos), 0);
}

/// `NoteBlockEvent.Change` (`NoteBlockEventChange`) fires before a tune, with the old and the
/// new note.
#[test]
fn one_event_per_fire_site_note_block_event_change() {
    let _serial = support::serial();
    let player = builder("Tuner");
    let pos = spot(14);
    note_block_at(pos);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<NoteBlockEventChange, _>(move |event| {
        if event.pos == pos {
            push(
                &seen_by_handler,
                format!("{} {} {}", event.old_note, event.note, note_at(pos)),
            );
        }
    });

    assert!(matches!(tune(&player, pos), BlockActionResult::Success));
    assert_eq!(entries(&seen), ["0 1 0"]);
    assert_eq!(note_at(pos), 1);
}

/// A cancelled `NoteBlockEventChange` keeps the note and fails the use.
#[test]
fn cancel_note_block_event_change() {
    let _serial = support::serial();
    let player = builder("CancelTuner");
    let pos = spot(15);
    note_block_at(pos);
    on::<NoteBlockEventChange, _>(move |event| {
        if event.pos == pos {
            event.set_cancelled(true);
        }
    });

    assert!(matches!(tune(&player, pos), BlockActionResult::Fail));
    assert_eq!(note_at(pos), 0);
}

/// The note of `NoteBlockEventChange` is settable: the block takes the note of the last handler.
#[test]
fn change_a_settable_field_note_block_event_change() {
    let _serial = support::serial();
    let player = builder("SetTuner");
    let pos = spot(16);
    note_block_at(pos);
    on::<NoteBlockEventChange, _>(move |event| {
        if event.pos == pos {
            event.note = 12;
        }
    });

    assert!(matches!(tune(&player, pos), BlockActionResult::Success));
    assert_eq!(note_at(pos), 12);
}

/// `CropGrowEvent.Post` (`CropGrowEventPost`) fires after a crop grew in its random tick, with
/// the state before and after.
#[test]
fn one_event_per_fire_site_crop_grow_event_post() {
    let _serial = support::serial();
    let pos = spot(17);
    set(pos.down(), Block::FARMLAND.default_state.id);
    set(pos, Block::WHEAT.default_state.id);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<CropGrowEventPost, _>(move |event| {
        if event.pos == pos {
            push(
                &seen_by_handler,
                format!(
                    "{} {} {}",
                    WheatLikeProperties::from_state_id(event.original_state.id).age,
                    WheatLikeProperties::from_state_id(event.state.id).age,
                    event.state.id == state_at(pos),
                ),
            );
        }
    });

    let world = support::world();
    let wheat = support::server()
        .block_registry
        .get_pumpkin_block(Block::WHEAT.id)
        .unwrap();
    // A random tick grows the crop with a chance of 1 in 26 or more.
    for _ in 0..5000 {
        wheat.random_tick(RandomTickArgs {
            world: &world,
            block: &Block::WHEAT,
            position: &pos,
        });
        if !entries(&seen).is_empty() {
            break;
        }
    }
    assert_eq!(entries(&seen), ["0 1 true"]);
}

/// `PistonEvent.Post` (`PistonEventPost`) fires after a piston extended.
#[test]
fn one_event_per_fire_site_piston_event_post() {
    let _serial = support::serial();
    let pos = spot(18);
    let mut piston = StickyPistonLikeProperties::default(&Block::PISTON);
    piston.facing = BlockDirection::Up.to_facing();
    set(pos, piston.to_state_id(&Block::PISTON));
    set(pos.up(), Block::AIR.default_state.id);
    set(pos.down(), Block::REDSTONE_BLOCK.default_state.id);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<PistonEventPost, _>(move |event| {
        if event.pos == pos {
            push(
                &seen_by_handler,
                format!(
                    "{:?} {:?} {}",
                    event.move_type,
                    event.direction,
                    StickyPistonLikeProperties::from_state_id(event.state.id).extended,
                ),
            );
        }
    });

    let world = support::world();
    assert!(support::server().block_registry.on_synced_block_event(
        &Block::PISTON,
        &world,
        &pos,
        0,
        BlockDirection::Up as u8,
    ));
    assert_eq!(
        entries(&seen),
        [format!(
            "{:?} {:?} true",
            PistonMoveType::Extend,
            BlockDirection::Up
        )]
    );
}

fn rule_value(value: &GameRuleValue<i64, bool>) -> String {
    value.to_string()
}

/// `GameRuleChangedEvent` fires after `World::set_game_rule` and after the `/gamerule` command,
/// with the new value.
#[test]
fn one_event_per_fire_site_game_rule_changed_event() {
    let _serial = support::serial();
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<GameRuleChangedEvent, _>(move |event| {
        if event.rule == GameRule::KeepInventory {
            push(&seen_by_handler, rule_value(&event.value));
        }
    });

    support::world().set_game_rule(&GameRule::KeepInventory, GameRuleValue::Bool(true));
    let server = support::server();
    let source = CommandSender::Console.into_source(server).with_silent();
    server.command_dispatcher.load().handle_command(
        &source,
        &format!("gamerule {} false", GameRule::KeepInventory),
    );

    assert_eq!(entries(&seen), ["true", "false"]);
}

fn center(pos: BlockPos) -> Vector3<f64> {
    Vector3::new(
        f64::from(pos.0.x) + 0.5,
        f64::from(pos.0.y) + 0.5,
        f64::from(pos.0.z) + 0.5,
    )
}

/// Dirt at `pos` and air around it, for an explosion at its center.
fn dirt_at(pos: BlockPos) {
    for dx in -2..=2 {
        for dy in -2..=2 {
            for dz in -2..=2 {
                set(
                    pos.offset(Vector3::new(dx, dy, dz)),
                    Block::AIR.default_state.id,
                );
            }
        }
    }
    set(pos, Block::DIRT.default_state.id);
}

fn explode(pos: BlockPos) {
    support::world().explode(center(pos), 4.0, ExplosionInteraction::Tnt);
}

/// `ExplosionEvent.Start` (`EntityExplodeEvent`) fires before the explosion with its world,
/// center and power, and a cancel keeps the blocks and fires no `ExplosionEventDetonate`.
#[test]
fn cancel_explosion_event_start() {
    let _serial = support::serial();
    let pos = spot(19);
    dirt_at(pos);
    let seen = log();
    let start = seen.clone();
    let world_name = support::world().dimension.minecraft_name;
    on::<EntityExplodeEvent, _>(move |event| {
        if event.position == center(pos) {
            push(
                &start,
                format!(
                    "start {} {}",
                    event.world.dimension.minecraft_name == world_name,
                    event.yield_rate
                ),
            );
            event.set_cancelled(true);
        }
    });
    let detonate = seen.clone();
    on::<ExplosionEventDetonate, _>(move |event| {
        if event.center == center(pos) {
            push(&detonate, "detonate");
        }
    });

    explode(pos);

    assert_eq!(entries(&seen), ["start true 4"]);
    assert_eq!(state_at(pos), Block::DIRT.default_state.id);
}

/// `ExplosionEvent.Detonate` (`ExplosionEventDetonate`) fires with the blocks and the entities
/// of the explosion, and the explosion changes only the blocks that the handler left.
#[test]
fn change_a_settable_field_explosion_event_detonate() {
    let _serial = support::serial();
    let pos = spot(20);
    dirt_at(pos);
    let item = item_at(pos.up());
    let id = item.get_entity().entity_id;
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<ExplosionEventDetonate, _>(move |event| {
        if event.center != center(pos) {
            return;
        }
        push(
            &seen_by_handler,
            format!(
                "{} {}",
                event.affected_blocks.contains(&pos),
                event
                    .affected_entities
                    .iter()
                    .any(|entity| entity.get_entity().entity_id == id),
            ),
        );
        event.affected_blocks.clear();
    });

    explode(pos);

    assert_eq!(entries(&seen), ["true true"]);
    assert_eq!(state_at(pos), Block::DIRT.default_state.id);
}

/// `ExplosionKnockbackEvent` fires for an entity that the explosion pushes, and the entity takes
/// the knockback of the last handler.
#[test]
fn change_a_settable_field_explosion_knockback_event() {
    let _serial = support::serial();
    let pos = spot(21);
    dirt_at(pos);
    // The explosion is in the air above the dirt, so the dirt does not hide the item.
    let item = item_at(pos.up().offset(Vector3::new(1, 0, 0)));
    let id = item.get_entity().entity_id;
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<ExplosionKnockbackEvent, _>(move |event| {
        if event.entity.get_entity().entity_id != id {
            return;
        }
        push(
            &seen_by_handler,
            format!(
                "{} {}",
                event.knockback.x > 0.0,
                event.affected_blocks.contains(&pos)
            ),
        );
        event.knockback = Vector3::new(0.0, 7.0, 0.0);
    });

    explode(pos.up());

    assert_eq!(entries(&seen), ["true true"]);
    // The damage of the explosion adds a small push of its own; the vanilla push of a power 4
    // explosion stays far below 7.
    let velocity = item.get_entity().velocity.load();
    assert!(velocity.y >= 7.0 && velocity.x < 1.0, "{velocity:?}");
}

/// `ChunkWatchEvent.UnWatch` (`ChunkWatchEventUnWatch`) fires for each chunk that leaves the
/// view of a player that moves, and for each chunk of the view when the player leaves.
#[test]
fn one_event_per_fire_site_chunk_watch_event_unwatch() {
    let _serial = support::serial();
    let player = support::player("Watcher");
    // The view change spawns the chunk watcher updates on the runtime of the server.
    let _runtime = support::RUNTIME.enter();
    let dropped = log();
    let dropped_by_handler = dropped.clone();
    on::<ChunkWatchEventUnWatch, _>(move |event| {
        if event.player.gameprofile.name == "Watcher" {
            push(
                &dropped_by_handler,
                format!("{} {}", event.pos.x, event.pos.y),
            );
        }
    });

    player
        .living_entity
        .entity
        .set_pos(Vector3::new(8.0, 100.0, 8.0));
    chunker::update_position(&player);
    let first = player.watched_section.load();
    let first_chunks: Vec<Vector2<i32>> = first.all_chunks_within().collect();
    assert!(entries(&dropped).is_empty());
    // Far enough that no chunk of the old view stays in the new one.
    player
        .living_entity
        .entity
        .set_pos(Vector3::new(8.0 + 16.0 * 200.0, 100.0, 8.0));
    chunker::update_position(&player);

    let mut unwatched = entries(&dropped);
    let mut expected: Vec<String> = first_chunks
        .iter()
        .map(|pos| format!("{} {}", pos.x, pos.y))
        .collect();
    unwatched.sort();
    expected.sort();
    assert_eq!(unwatched, expected);

    dropped.lock().unwrap().clear();
    let second = player.watched_section.load().all_chunks_within().len();
    support::RUNTIME.block_on(player.remove(true));
    assert_eq!(entries(&dropped).len(), second);
}

/// `ChunkEvent.Load` (`ChunkLoad`) and `ChunkEvent.Unload` (`ChunkUnloadEvent`) fire in the world
/// tick after a chunk joined or left the loaded chunks of the world.
#[test]
fn one_event_per_fire_site_chunk_load_and_unload_events() {
    let _serial = support::serial();
    let world = support::world();
    let chunk = spot(22).chunk_position();
    let seen = log();
    let loaded = seen.clone();
    on::<ChunkLoad, _>(move |event| {
        if event.chunk.x == chunk.x && event.chunk.z == chunk.y {
            push(&loaded, "load");
        }
    });
    let unloaded = seen.clone();
    on::<ChunkUnloadEvent, _>(move |event| {
        if event.chunk_pos == chunk {
            push(&unloaded, "unload");
        }
    });

    load(spot(22));
    world.update_active_chunks();
    assert_eq!(entries(&seen), ["load"]);

    {
        let mut loading = world.level.chunk_loading.lock().unwrap();
        loading.remove_force_ticket(chunk);
        loading.send_change();
    };
    let deadline = Instant::now() + support::SLOW_RUNNER;
    while world.level.is_chunk_loaded(&chunk) {
        assert!(Instant::now() < deadline, "the chunk did not unload");
        std::thread::sleep(Duration::from_millis(10));
    }
    world.update_active_chunks();
    assert_eq!(entries(&seen), ["load", "unload"]);
}

/// Two water sources around a flowing water at `pos`, on a stone floor.
fn water_between_sources(pos: BlockPos) {
    let east = BlockPos::new(1, 0, 0).0;
    let flowing = Block::WATER
        .from_properties(&[("level", "1")])
        .to_state_id(&Block::WATER);
    for offset in [-1, 0, 1] {
        let at = pos.offset(east * offset);
        set(at.down(), Block::STONE.default_state.id);
        set(
            at.offset(Vector3::new(0, 0, 1)),
            Block::STONE.default_state.id,
        );
        set(
            at.offset(Vector3::new(0, 0, -1)),
            Block::STONE.default_state.id,
        );
        let water = if offset == 0 {
            flowing
        } else {
            Block::WATER.default_state.id
        };
        set(at, water);
    }
}

/// Runs the scheduled fluid tick of the water at `pos`.
fn tick_water(pos: BlockPos) {
    let world = support::world();
    support::server()
        .block_registry
        .get_pumpkin_fluid(Fluid::FLOWING_WATER.id)
        .unwrap()
        .on_scheduled_tick(&world, &Fluid::FLOWING_WATER, &pos);
}

/// `CreateFluidSourceEvent` fires for each source next to a flowing water, with the vanilla
/// result, and the water becomes a source.
#[test]
fn one_event_per_fire_site_create_fluid_source_event() {
    let _serial = support::serial();
    let pos = spot(23);
    water_between_sources(pos);
    let seen = log();
    let seen_by_handler = seen.clone();
    let sources = [
        pos.offset(Vector3::new(-1, 0, 0)),
        pos.offset(Vector3::new(1, 0, 0)),
    ];
    on::<CreateFluidSourceEvent, _>(move |event| {
        if sources.contains(&event.pos) {
            push(
                &seen_by_handler,
                format!(
                    "{} {} {}",
                    event.pos.0.x - pos.0.x,
                    event.vanilla_result,
                    event.state.id == Block::WATER.default_state.id
                ),
            );
        }
    });

    tick_water(pos);

    let seen = entries(&seen);
    assert!(seen.contains(&"-1 true true".to_owned()), "{seen:?}");
    assert!(seen.contains(&"1 true true".to_owned()), "{seen:?}");
    assert_eq!(state_at(pos), Block::WATER.default_state.id);
}

/// `can_convert` of `CreateFluidSourceEvent` is settable: a source that cannot convert does not
/// count, and the water stays flowing.
#[test]
fn change_a_settable_field_create_fluid_source_event() {
    let _serial = support::serial();
    let pos = spot(24);
    water_between_sources(pos);
    let sources = [
        pos.offset(Vector3::new(-1, 0, 0)),
        pos.offset(Vector3::new(1, 0, 0)),
    ];
    on::<CreateFluidSourceEvent, _>(move |event| {
        if sources.contains(&event.pos) {
            event.can_convert = false;
        }
    });

    tick_water(pos);

    assert_ne!(state_at(pos), Block::WATER.default_state.id);
}
