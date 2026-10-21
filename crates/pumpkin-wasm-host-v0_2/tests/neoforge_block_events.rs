//! The block and use events of `NeoForge` 26.3 reach v0.2 plugins (spec `neoforge-event-bus`,
//! requirement "Wasm plugins see the same events", #131): a v0.2 handler receives each event with
//! its `NeoForge` fields, writes back the settable fields and cancels it, and the host keeps the
//! fields that `NeoForge` cannot set.
#![expect(clippy::panic, reason = "a test fails on the first unexpected record")]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

mod guest;

use std::{
    marker::PhantomData,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use guest::{fire, plugin, resource};
use pumpkin_core::{
    entity::r#type::from_type,
    plugin::{
        BoxFuture, Cancellable, EventHandler, EventPriority, Payload, PluginMetadata,
        api::events::results::{InteractionResult, TriState},
        block::{
            block_break::BlockBreakEvent, block_multi_place::BlockMultiPlaceEvent,
            block_place::BlockPlaceEvent,
        },
        level::{
            BlockEventBlockToolModificationEvent, BlockEventEntityPlaceEvent,
            CreateFluidSourceEvent, CropGrowEventPost, FIRESTARTER_LIGHT, NoteBlockEventChange,
            PistonEventPost, PistonMoveType,
        },
        player::{
            player_interact_event::{LeftClickBlockAction, PlayerInteractEvent},
            use_item_on_block::{UseItemOnBlockEvent, UsePhase},
        },
        startup::create_context,
    },
    server::Server,
};
use pumpkin_data::{
    Block, BlockDirection, BlockState, BlockStateId, entity::EntityType, item::Item,
    item_stack::ItemStack,
};
use pumpkin_util::{
    Hand,
    math::{position::BlockPos, vector3::Vector3},
};
use pumpkin_wasm_host_common::state::PluginHostState;
use pumpkin_wasm_host_v0_2::pumpkin::plugin::{
    common::{BlockPos as WitBlockPos, Hand as WitHand},
    event::Event,
    neoforge_events::{
        InteractionResult as WitInteractionResult, LeftClickBlockAction as WitLeftClickAction,
        PistonMoveType as WitPistonMoveType, TriState as WitTriState, UsePhase as WitUsePhase,
    },
    world::BlockDirection as WitBlockDirection,
};
use pumpkin_world::world::BlockFlags;

const fn native(pos: &WitBlockPos) -> BlockPos {
    BlockPos::new(pos.x, pos.y, pos.z)
}

/// A state of `block` other than its default state.
fn other_state(block: &Block) -> &'static BlockState {
    BlockState::from_id(BlockStateId::new_or_air(
        block.default_state.id.as_u16() + 1,
    ))
}

#[test]
fn a_plugin_cancels_use_item_on_block_and_sets_its_result() {
    let player = support::player("UseOnBlock");
    let pos = BlockPos::new(2, 64, 2);
    let hit = Vector3::new(2.5, 65.0, 2.5);
    let manager = plugin::<UseItemOnBlockEvent, _>(move |event, state| {
        let Event::UseItemOnBlockEvent(data) = event else {
            panic!("expected use item on block");
        };
        assert_eq!(data.hand, WitHand::Left);
        assert_eq!(native(&data.block_pos), pos);
        assert_eq!(data.face, WitBlockDirection::Up);
        assert_eq!(data.hit_vector, (hit.x, hit.y, hit.z));
        assert_eq!(data.phase, WitUsePhase::ItemAfterBlock);
        assert_eq!(
            data.cancellation_result,
            WitInteractionResult::TryWithEmptyHand
        );
        let item = resource(state, &data.item);
        assert_eq!(
            item.try_lock().map(|item| item.item.id).ok(),
            Some(Item::STONE.id)
        );
        data.cancellation_result = WitInteractionResult::Success;
        data.phase = WitUsePhase::Block;
        data.cancelled = true;
    });

    let event = fire(
        &manager,
        UseItemOnBlockEvent::new(
            player,
            Hand::Left,
            ItemStack::new(1, &Item::STONE),
            pos,
            BlockDirection::Up,
            hit,
            UsePhase::ItemAfterBlock,
        ),
    );

    assert!(event.cancelled);
    assert_eq!(event.cancellation_result, InteractionResult::Success);
    assert_eq!(
        event.phase,
        UsePhase::ItemAfterBlock,
        "the phase has no setter"
    );
}

#[test]
fn a_plugin_sets_the_decisions_of_a_right_click_block() {
    let player = support::player("RightClick");
    let pos = BlockPos::new(3, 64, 3);
    let hit = Vector3::new(3.5, 64.5, 3.0);
    let manager = plugin::<PlayerInteractEvent, _>(move |event, _state| {
        let Event::PlayerInteractEvent(data) = event else {
            panic!("expected player interact");
        };
        assert_eq!(data.hand, WitHand::Left);
        assert_eq!(data.face, Some(WitBlockDirection::North));
        assert_eq!(data.hit_vector, Some((hit.x, hit.y, hit.z)));
        assert_eq!(data.left_click_action, None);
        assert_eq!(data.use_block, WitTriState::Default);
        assert_eq!(data.use_item, WitTriState::Default);
        assert_eq!(data.cancellation_result, WitInteractionResult::Pass);
        data.use_block = WitTriState::False;
        data.use_item = WitTriState::True;
        data.cancellation_result = WitInteractionResult::SuccessServer;
        data.hand = WitHand::Right;
        data.cancelled = true;
    });

    let event = fire(
        &manager,
        PlayerInteractEvent::right_click_block(
            &player,
            Hand::Left,
            &Block::STONE,
            pos,
            BlockDirection::North,
            hit,
        ),
    );

    assert!(event.cancelled);
    assert_eq!(event.use_block, TriState::False);
    assert_eq!(event.use_item, TriState::True);
    assert_eq!(event.cancellation_result, InteractionResult::SuccessServer);
    assert_eq!(event.hand, Hand::Left, "the hand has no setter");
}

#[test]
fn a_plugin_receives_the_dig_action_of_a_left_click_block() {
    let player = support::player("LeftClick");
    let pos = BlockPos::new(4, 64, 4);
    let manager = plugin::<PlayerInteractEvent, _>(move |event, _state| {
        let Event::PlayerInteractEvent(data) = event else {
            panic!("expected player interact");
        };
        assert_eq!(data.left_click_action, Some(WitLeftClickAction::Abort));
        assert_eq!(data.face, Some(WitBlockDirection::East));
        assert_eq!(data.hit_vector, None);
        data.left_click_action = Some(WitLeftClickAction::Start);
        data.use_block = WitTriState::False;
    });

    let event = fire(
        &manager,
        PlayerInteractEvent::left_click_block(
            &player,
            &Block::STONE,
            pos,
            Some(BlockDirection::East),
            LeftClickBlockAction::Abort,
        ),
    );

    assert_eq!(event.use_block, TriState::False);
    assert_eq!(event.left_click_action, Some(LeftClickBlockAction::Abort));
}

#[test]
fn a_plugin_receives_the_state_of_a_block_break_and_sets_its_drops() {
    let state = other_state(&Block::OAK_LOG);
    let pos = BlockPos::new(5, 64, 5);
    let manager = plugin::<BlockBreakEvent, _>(move |event, _state| {
        let Event::BlockBreakEvent(data) = event else {
            panic!("expected block break");
        };
        assert_eq!(data.state_id, state.id.as_u16());
        assert_eq!(data.exp, 3);
        data.state_id = Block::STONE.default_state.id.as_u16();
        data.exp = 7;
        data.should_drop = false;
        data.cancelled = true;
    });

    let event = fire(
        &manager,
        BlockBreakEvent::new(None, &Block::OAK_LOG, state, pos, 3, true),
    );

    assert!(event.cancelled);
    assert_eq!(event.exp, 7);
    assert!(!event.drop);
    assert_eq!(event.state.id, state.id, "the state has no setter");
}

#[test]
fn a_plugin_receives_the_states_of_a_block_place() {
    let player = support::player("Placer");
    let placed = other_state(&Block::OAK_LOG);
    let replaced = Block::SHORT_GRASS.default_state;
    let manager = plugin::<BlockPlaceEvent, _>(move |event, _state| {
        let Event::BlockPlaceEvent(data) = event else {
            panic!("expected block place");
        };
        assert_eq!(data.placed_state_id, placed.id.as_u16());
        assert_eq!(data.replaced_state_id, replaced.id.as_u16());
        data.placed_state_id = Block::STONE.default_state.id.as_u16();
        data.replaced_state_id = Block::STONE.default_state.id.as_u16();
        data.cancelled = true;
    });

    let event = fire(
        &manager,
        BlockPlaceEvent::new(
            player,
            &Block::OAK_LOG,
            &Block::DIRT,
            BlockPos::new(6, 64, 6),
            true,
            placed,
            replaced,
        ),
    );

    assert!(event.cancelled);
    assert_eq!(
        event.placed_state.id, placed.id,
        "the placed state has no setter"
    );
    assert_eq!(
        event.replaced_state.id, replaced.id,
        "the replaced state has no setter"
    );
}

#[test]
fn a_plugin_receives_the_replaced_blocks_of_a_multi_place() {
    let player = support::player("MultiPlacer");
    let world = support::world();
    let lower = BlockPos::new(7, 64, 7);
    let upper = BlockPos::new(7, 65, 7);
    let door = Block::OAK_DOOR.default_state.id;
    let manager = plugin::<BlockMultiPlaceEvent, _>(move |event, _state| {
        let Event::BlockMultiPlaceEvent(data) = event else {
            panic!("expected block multi place");
        };
        let replaced: Vec<_> = data
            .replaced_blocks
            .iter()
            .map(|(pos, state)| (native(pos), *state))
            .collect();
        assert_eq!(
            replaced,
            [
                (lower, Block::AIR.default_state.id.as_u16()),
                (upper, Block::WATER.default_state.id.as_u16()),
            ]
        );
        data.cancelled = true;
    });

    let event = fire(
        &manager,
        BlockMultiPlaceEvent::new(
            player,
            world,
            vec![(lower, door), (upper, door)],
            vec![
                (lower, Block::AIR.default_state),
                (upper, Block::WATER.default_state),
            ],
        ),
    );

    assert!(event.cancelled);
}

#[test]
fn a_plugin_sets_the_final_state_of_a_tool_modification() {
    let world = support::world();
    let pos = BlockPos::new(8, 64, 8);
    let state = Block::CAMPFIRE.default_state;
    let lit = other_state(&Block::CAMPFIRE);
    let manager = plugin::<BlockEventBlockToolModificationEvent, _>(move |event, state_table| {
        let Event::BlockEventBlockToolModificationEvent(data) = event else {
            panic!("expected tool modification");
        };
        assert!(data.player.is_none());
        assert_eq!(data.item_ability, FIRESTARTER_LIGHT);
        assert!(!data.simulated);
        assert_eq!(data.state_id, state.id.as_u16());
        assert_eq!(data.final_state_id, Some(state.id.as_u16()));
        let item = resource(state_table, &data.held_item);
        assert_eq!(
            item.try_lock().map(|item| item.item.id).ok(),
            Some(Item::FLINT_AND_STEEL.id)
        );
        data.final_state_id = Some(lit.id.as_u16());
        data.state_id = Block::STONE.default_state.id.as_u16();
    });

    let event = fire(
        &manager,
        BlockEventBlockToolModificationEvent::new(
            world,
            pos,
            None,
            ItemStack::new(1, &Item::FLINT_AND_STEEL),
            FIRESTARTER_LIGHT,
            state,
        ),
    );

    assert_eq!(event.final_state.map(|state| state.id), Some(lit.id));
    assert_eq!(event.state.id, state.id, "the state has no setter");
    assert!(!event.cancelled);
}

#[test]
fn a_plugin_cancels_a_tool_modification() {
    let manager = plugin::<BlockEventBlockToolModificationEvent, _>(|event, _state| {
        let Event::BlockEventBlockToolModificationEvent(data) = event else {
            panic!("expected tool modification");
        };
        data.final_state_id = None;
        data.cancelled = true;
    });

    let event = fire(
        &manager,
        BlockEventBlockToolModificationEvent::new(
            support::world(),
            BlockPos::new(9, 64, 9),
            None,
            ItemStack::new(1, &Item::FLINT_AND_STEEL),
            FIRESTARTER_LIGHT,
            Block::CAMPFIRE.default_state,
        ),
    );

    assert!(event.cancelled);
    assert!(event.final_state.is_none());
}

#[test]
fn a_plugin_receives_a_crop_grow_post() {
    let world = support::world();
    let pos = BlockPos::new(10, 64, 10);
    let original = Block::WHEAT.default_state;
    let grown = other_state(&Block::WHEAT);
    let seen = Arc::new(Mutex::new(None));
    let record = seen.clone();
    let manager = plugin::<CropGrowEventPost, _>(move |event, state| {
        let Event::CropGrowEventPost(data) = event else {
            panic!("expected crop grow post");
        };
        let target = resource(state, &data.target_world);
        *record.lock().unwrap_or_else(PoisonError::into_inner) = Some((
            Arc::ptr_eq(&target, &support::world()),
            native(&data.block_pos),
            data.original_state_id,
            data.state_id,
        ));
    });

    fire(
        &manager,
        CropGrowEventPost::new(world, pos, original, grown),
    );

    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        Some((true, pos, original.id.as_u16(), grown.id.as_u16()))
    );
}

#[test]
fn a_plugin_sets_can_convert_of_create_fluid_source() {
    let state = Block::WATER.default_state;
    let manager = plugin::<CreateFluidSourceEvent, _>(move |event, _state| {
        let Event::CreateFluidSourceEvent(data) = event else {
            panic!("expected create fluid source");
        };
        assert_eq!(data.state_id, state.id.as_u16());
        assert!(data.vanilla_result);
        assert!(data.can_convert);
        data.can_convert = false;
        data.vanilla_result = false;
    });

    let event = fire(
        &manager,
        CreateFluidSourceEvent::new(support::world(), BlockPos::new(11, 64, 11), state, true),
    );

    assert!(!event.can_convert);
    assert!(event.vanilla_result, "the vanilla result has no setter");
}

#[test]
fn a_plugin_tunes_and_cancels_a_note_block_change() {
    let state = Block::NOTE_BLOCK.default_state;
    let manager = plugin::<NoteBlockEventChange, _>(move |event, _state| {
        let Event::NoteBlockEventChange(data) = event else {
            panic!("expected note block change");
        };
        assert_eq!((data.old_note, data.note), (3, 4));
        assert_eq!(data.state_id, state.id.as_u16());
        data.note = 12;
        data.old_note = 0;
    });

    let event = fire(
        &manager,
        NoteBlockEventChange::new(support::world(), BlockPos::new(12, 64, 12), state, 3, 4),
    );
    assert_eq!(event.note, 12);
    assert_eq!(event.old_note, 3, "the old note has no setter");

    let cancel = plugin::<NoteBlockEventChange, _>(|event, _state| {
        if let Event::NoteBlockEventChange(data) = event {
            data.cancelled = true;
        }
    });
    let event = fire(
        &cancel,
        NoteBlockEventChange::new(support::world(), BlockPos::new(12, 64, 12), state, 3, 4),
    );
    assert!(event.cancelled);
}

#[test]
fn a_plugin_receives_a_piston_post() {
    let state = Block::PISTON.default_state;
    let pos = BlockPos::new(13, 64, 13);
    let seen = Arc::new(Mutex::new(None));
    let record = seen.clone();
    let manager = plugin::<PistonEventPost, _>(move |event, _state| {
        let Event::PistonEventPost(data) = event else {
            panic!("expected piston post");
        };
        *record.lock().unwrap_or_else(PoisonError::into_inner) = Some((
            native(&data.block_pos),
            data.state_id,
            data.direction,
            data.move_type,
        ));
    });

    fire(
        &manager,
        PistonEventPost::new(
            support::world(),
            pos,
            state,
            BlockDirection::South,
            PistonMoveType::Retract,
        ),
    );

    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        Some((
            pos,
            state.id.as_u16(),
            WitBlockDirection::South,
            WitPistonMoveType::Retract
        ))
    );
}

/// A blocking handler of a native mod.
struct Native<E, F>(F, PhantomData<fn() -> E>);

impl<E, F> EventHandler<E> for Native<E, F>
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

/// Registers a blocking handler of a native mod for `BlockEventEntityPlaceEvent`.
fn native_on(
    priority: EventPriority,
    receive_canceled: bool,
    handler: impl Fn(&mut BlockEventEntityPlaceEvent) + Send + Sync + 'static,
) {
    support::server()
        .plugin_manager
        .register_with::<BlockEventEntityPlaceEvent, _>(
            Arc::new(Native(handler, PhantomData)),
            priority,
            true,
            receive_canceled,
        );
}

fn metadata(name: &str) -> PluginMetadata {
    PluginMetadata {
        name: name.to_owned(),
        version: "1.0.0".to_owned(),
        authors: Vec::new(),
        description: "v0.2 event test plugin".to_owned(),
        dependencies: Vec::new(),
        permissions: Vec::new(),
    }
}

/// Loads the chunk of `pos` for the rest of the process and sets `pos` to air.
fn clear_loaded(pos: BlockPos) {
    let world = support::world();
    let chunk = pos.chunk_position();
    {
        let mut loading = world
            .level
            .chunk_loading
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        loading.add_force_ticket(chunk);
        loading.send_change();
    };
    let deadline = Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&chunk) {
        assert!(Instant::now() < deadline, "the chunk of {pos} did not load");
        std::thread::sleep(Duration::from_millis(10));
    }
    world.set_block_state(&pos, Block::AIR.default_state.id, BlockFlags::empty());
}

/// Scenario "One event pipeline" for a placement by an entity that is not a player, in both
/// directions: `World::place_block` fires `BlockEventEntityPlaceEvent`, which a v0.2 plugin
/// cancels and a native mod observes, then which a native mod cancels and a v0.2 plugin observes.
#[test]
fn world_place_block_by_an_entity_reaches_plugins_and_mods() {
    let _serial = support::serial();
    let server = support::server();
    let world = support::world();
    let plugin_cancels = BlockPos::new(14, 120, 14);
    let mod_cancels = BlockPos::new(14, 120, 15);
    clear_loaded(plugin_cancels);
    clear_loaded(mod_cancels);
    let pig = from_type(
        &EntityType::PIG,
        Vector3::new(14.5, 121.0, 14.5),
        &world,
        uuid::Uuid::new_v4(),
    );
    assert!(world.spawn_entity(pig.clone()));
    let pig_id = pig.get_entity().entity_id;
    let stone = Block::STONE.default_state.id;
    let seen_by_plugin = Arc::new(Mutex::new(Vec::new()));
    let record = seen_by_plugin.clone();
    create_context(server, metadata("v0.2-place-plugin"))
        .register_event::<BlockEventEntityPlaceEvent, _>(
            Arc::new(guest::Guest::new(
                move |event: &mut Event, state: &mut PluginHostState| {
                    let Event::BlockEventEntityPlaceEvent(data) = event else {
                        panic!("expected entity place");
                    };
                    let entity = resource(state, &data.entity);
                    if entity.get_entity().entity_id != pig_id {
                        return;
                    }
                    record.lock().unwrap_or_else(PoisonError::into_inner).push((
                        native(&data.block_pos),
                        data.placed_state_id,
                        data.replaced_state_id,
                        data.cancelled,
                    ));
                    if native(&data.block_pos) == plugin_cancels {
                        data.cancelled = true;
                    }
                },
            )),
            EventPriority::Normal,
            true,
        );
    let seen_by_mod = Arc::new(Mutex::new(Vec::new()));
    let record = seen_by_mod.clone();
    native_on(EventPriority::Low, true, move |event| {
        if event.entity.get_entity().entity_id == pig_id && event.pos == plugin_cancels {
            record
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(event.cancelled());
        }
    });
    native_on(EventPriority::High, false, move |event| {
        if event.entity.get_entity().entity_id == pig_id && event.pos == mod_cancels {
            event.set_cancelled(true);
        }
    });

    assert!(!world.place_block(&plugin_cancels, stone, pig.as_ref()));
    assert!(!world.place_block(&mod_cancels, stone, pig.as_ref()));

    assert_eq!(
        world.get_block_state_id(&plugin_cancels),
        Block::AIR.default_state.id
    );
    assert_eq!(
        world.get_block_state_id(&mod_cancels),
        Block::AIR.default_state.id
    );
    let air = Block::AIR.default_state.id.as_u16();
    assert_eq!(
        *seen_by_plugin
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
        [
            (plugin_cancels, stone.as_u16(), air, false),
            (mod_cancels, stone.as_u16(), air, true),
        ]
    );
    assert_eq!(
        *seen_by_mod.lock().unwrap_or_else(PoisonError::into_inner),
        [true]
    );
}
