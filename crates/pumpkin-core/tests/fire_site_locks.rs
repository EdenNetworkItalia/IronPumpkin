//! A sound or a game event runs mod handlers. A handler that reads or changes the world must find
//! the state of the fire site unlocked: a fire site that holds a lock while it fires can hang the
//! server. Each test fires from a site that held a lock, with a handler that takes the same lock.
//! A handler that waits for a lock that the fire site holds hangs, so the tests run the fire
//! site on a thread with a timeout.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use pumpkin_core::{
    block::{
        BlockBehaviour, BlockHitResult, NormalUseArgs,
        blocks::{
            brushable_block::BrushableBlock, redstone::daylight_detector::DaylightDetectorBlock,
        },
        entities::{brushable_block::BrushableBlockBlockEntity, jukebox::JukeboxBlockEntity},
    },
    plugin::{
        BoxFuture, Deferral, EventHandler, EventPriority, Payload,
        world::{GenericGameEvent, PlayLevelSoundEventAtPosition},
    },
    server::Server,
    world::World,
};
use pumpkin_data::{
    Block, BlockDirection,
    block_properties::{DoubleBlockHalf, OakDoorLikeProperties as DoorProperties},
    item::Item,
    item_stack::ItemStack,
    sound::Sound,
};
use pumpkin_inventory::{Inventory, screen_handler::InventoryPlayer};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use pumpkin_world::world::BlockFlags;

/// How long a fire site may take. A fire site that waits for a lock that it holds never returns.
const TIMEOUT: Duration = Duration::from_secs(60);

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

/// Runs `action` on a thread and returns its result, or fails when it does not return.
fn within_timeout<R: Send + 'static>(action: impl FnOnce() -> R + Send + 'static) -> R {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(action());
    });
    rx.recv_timeout(TIMEOUT)
        .expect("the fire site hangs: it holds a lock that the handler takes")
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
    world.set_block_state(&pos, block.default_state.id, BlockFlags::NOTIFY_ALL);
    assert_eq!(world.get_block(&pos).id, block.id);
}

/// A brush stroke plays its sound with the hit counter of the block unlocked.
#[test]
fn the_brush_sound_handler_can_read_the_hit_counter() {
    let _serial = support::serial();
    let world = support::world();
    let x = 32_000;
    let pos = BlockPos::new(x, 100, 0);
    place(&world, pos, &Block::SUSPICIOUS_SAND);
    let brushable = Arc::new(BrushableBlockBlockEntity::new(pos));
    world.add_block_entity(brushable.clone());
    let read: Arc<Mutex<Vec<Option<i32>>>> = Arc::default();
    let (record, entity) = (read.clone(), brushable);
    on::<PlayLevelSoundEventAtPosition, _>(move |event| {
        if event.position.x == f64::from(x) + 0.5 {
            record
                .lock()
                .unwrap()
                .push(entity.hits.try_lock().ok().map(|hits| *hits));
        }
    });

    let brushing = world;
    within_timeout(move || BrushableBlock::brush(&brushing, &pos, &Block::SUSPICIOUS_SAND));

    assert_eq!(*read.lock().unwrap(), [Some(1)]);
}

/// A toggle of the daylight detector emits its game event with the abilities of the player
/// unlocked.
#[test]
fn the_daylight_detector_game_event_handler_can_read_the_player_abilities() {
    let _serial = support::serial();
    let world = support::world();
    let x = 32_100;
    let pos = BlockPos::new(x, 100, 0);
    place(&world, pos, &Block::DAYLIGHT_DETECTOR);
    let player = support::player("DaylightAbilities");
    let read: Arc<Mutex<Vec<bool>>> = Arc::default();
    let (record, reader) = (read.clone(), player.clone());
    on::<GenericGameEvent, _>(move |event| {
        if event.position().x == f64::from(x) + 0.5 {
            record
                .lock()
                .unwrap()
                .push(reader.abilities.try_lock().is_ok());
        }
    });

    let (using, using_world) = (player, world);
    within_timeout(move || {
        let face = BlockDirection::Up;
        let cursor = Vector3::new(0.5, 0.5, 0.5);
        DaylightDetectorBlock.normal_use(NormalUseArgs {
            server: support::server(),
            world: &using_world,
            block: &Block::DAYLIGHT_DETECTOR,
            position: &pos,
            player: &using,
            hit: &BlockHitResult {
                face: &face,
                cursor_pos: &cursor,
            },
        })
    });

    assert_eq!(*read.lock().unwrap(), [true]);
}

/// A record change of a jukebox emits its game events after the record transition lock. A
/// handler of the stop event takes the lock again with `clear_record`. Vanilla `itemChanged`
/// emits `block_change` (`has_record`) before `jukebox_stop_play`.
#[test]
fn a_jukebox_game_event_handler_can_change_the_record() {
    let _serial = support::serial();
    let world = support::world();
    let x = 32_200;
    let pos = BlockPos::new(x, 100, 0);
    place(&world, pos, &Block::JUKEBOX);
    let jukebox = jukebox_at(&world, pos);
    jukebox.set_stack(0, ItemStack::new(1, &Item::MUSIC_DISC_13));
    assert!(jukebox.is_playing(), "the disc starts a song");
    let handled = Arc::new(Mutex::new(Vec::new()));
    let (record, reader) = (handled.clone(), jukebox.clone());
    on::<GenericGameEvent, _>(move |event| {
        if event.position().x == f64::from(x) + 0.5 {
            record.lock().unwrap().push(event.event_key().to_owned());
            if event.event_key() == "jukebox_stop_play" {
                // `clear_record` takes the record transition lock.
                assert!(reader.clear_record().is_empty());
            }
        }
    });

    let taking = jukebox.clone();
    within_timeout(move || taking.remove_stack(0));

    let handled = handled.lock().unwrap();
    assert_eq!(handled[..2], ["block_change", "jukebox_stop_play"]);
    assert!(jukebox.get_stack(0).is_empty());
}

/// A jukebox block entity that replaces the one of the placed block, so the test keeps a handle.
fn jukebox_at(world: &Arc<World>, pos: BlockPos) -> Arc<JukeboxBlockEntity> {
    let jukebox = Arc::new(JukeboxBlockEntity::new(pos));
    world.add_block_entity(jukebox.clone());
    jukebox
}

/// A playing jukebox powers a door. The stop of the song updates the neighbors, and the door
/// closes with a sound and `block_close`. A handler of `block_close` takes the record
/// transition lock with `clear_record`: the stop updates the neighbors after the lock.
#[test]
fn a_door_that_closes_when_the_jukebox_stops_can_change_the_record() {
    let _serial = support::serial();
    let world = support::world();
    let x = 32_400;
    let pos = BlockPos::new(x, 100, 0);
    place(&world, pos, &Block::JUKEBOX);
    let jukebox = jukebox_at(&world, pos);
    let door_pos = BlockPos::new(x + 1, 100, 0);
    // The chunk is loaded. The door halves go in with no update, on a block that holds them.
    world.set_block_state(
        &door_pos.down(),
        Block::STONE.default_state.id,
        BlockFlags::empty(),
    );
    let door = |half| {
        let mut props = DoorProperties::from_state_id(Block::OAK_DOOR.default_state.id);
        props.half = half;
        props.open = true;
        props.powered = true;
        props.to_state_id(&Block::OAK_DOOR)
    };
    world.set_block_state(&door_pos, door(DoubleBlockHalf::Lower), BlockFlags::empty());
    world.set_block_state(
        &door_pos.up(),
        door(DoubleBlockHalf::Upper),
        BlockFlags::empty(),
    );
    jukebox.set_stack(0, ItemStack::new(1, &Item::MUSIC_DISC_13));
    assert!(jukebox.is_playing());
    // The door opened and closed again with the setup: start from the powered open state.
    world.set_block_state(&door_pos, door(DoubleBlockHalf::Lower), BlockFlags::empty());
    world.set_block_state(
        &door_pos.up(),
        door(DoubleBlockHalf::Upper),
        BlockFlags::empty(),
    );
    let handled = Arc::new(Mutex::new(0));
    let (count, reader) = (handled.clone(), jukebox.clone());
    on::<GenericGameEvent, _>(move |event| {
        if event.event_key() == "block_close" && event.position().x == f64::from(x) + 1.5 {
            // `clear_record` takes the record transition lock.
            reader.clear_record();
            *count.lock().unwrap() += 1;
        }
    });

    let taking = jukebox;
    within_timeout(move || taking.clear_record());

    assert_eq!(*handled.lock().unwrap(), 1);
}

/// The take-result sound of a screen handler plays after the screen handler lock, with the
/// deferral of the slot click. A handler of the sound reads the screen handler of the player.
#[test]
fn a_screen_handler_sound_plays_after_the_screen_handler_lock() {
    let _serial = support::serial();
    let x = 32_300;
    let pos = BlockPos::new(x, 100, 0);
    let player = support::player("ScreenSound");
    player.open_container_pos.store(Some(pos));
    let handler = player.current_screen_handler.lock().unwrap().clone();
    let read: Arc<Mutex<Vec<bool>>> = Arc::default();
    let (record, reader) = (read.clone(), handler.clone());
    on::<PlayLevelSoundEventAtPosition, _>(move |event| {
        if event.position.x == f64::from(x) + 0.5
            && event.sound.as_ref().is_some_and(|sound| {
                pumpkin_core::world::sound::sound_holder_name(sound)
                    == Sound::NAMES[Sound::UiStonecutterTakeResult as usize]
            })
        {
            record.lock().unwrap().push(reader.try_lock().is_ok());
        }
    });

    let deferral = Deferral::start();
    let guard = handler.lock().unwrap();
    player.play_block_sound(Sound::UiStonecutterTakeResult, 1.0);
    assert!(
        read.lock().unwrap().is_empty(),
        "the sound waits for the screen handler lock"
    );
    drop(guard);
    drop(deferral);

    assert_eq!(*read.lock().unwrap(), [true]);
}

/// A creative slot set of a helmet plays its equip sound after the screen handler lock. A handler
/// of the sound takes the lock of the player screen handler.
#[test]
fn the_equip_sound_of_a_creative_slot_set_plays_after_the_screen_handler_lock() {
    use pumpkin_core::net::ClientPlatform;
    use pumpkin_protocol::{
        codec::item_stack_seralizer::ItemStackSerializer, java::server::play::SSetCreativeSlot,
    };

    let _serial = support::serial();
    let x = 32_600.0;
    let player = support::player("CreativeEquip");
    player.gamemode.store(pumpkin_util::GameMode::Creative);
    player
        .living_entity
        .entity
        .set_pos(Vector3::new(x, 100.0, 0.5));
    let read: Arc<Mutex<Vec<bool>>> = Arc::default();
    let (record, reader) = (read.clone(), player.clone());
    on::<PlayLevelSoundEventAtPosition, _>(move |event| {
        if event.position.x == x {
            record
                .lock()
                .unwrap()
                .push(reader.player_screen_handler.try_lock().is_ok());
        }
    });

    let setting = player;
    within_timeout(move || {
        let ClientPlatform::Java(client) = setting.client.as_ref() else {
            return;
        };
        // Slot 5 of the player screen is the head slot.
        client
            .handle_set_creative_slot(
                &setting,
                SSetCreativeSlot::new(
                    5,
                    ItemStackSerializer::from(ItemStack::new(1, &Item::IRON_HELMET)),
                ),
            )
            .unwrap();
    });

    assert_eq!(*read.lock().unwrap(), [true]);
}

/// A frog lays its spawn in a behavior of its brain tick, which holds the brain lock. A handler
/// of the `block_place` game event writes the NBT of the frog, which takes the brain lock: the
/// fire waits for the end of the tick.
#[test]
fn a_game_event_of_a_behavior_can_write_the_nbt_of_the_mob() {
    use pumpkin_core::entity::{
        ai::brain::memory::{BlockPosTracker, WalkTarget, types},
        r#type::from_type,
    };
    use pumpkin_data::entity::EntityType;
    use pumpkin_nbt::compound::NbtCompound;

    let _serial = support::serial();
    let world = support::world();
    let x = 32_700;
    let ground = BlockPos::new(x, 99, 0);
    place(&world, ground, &Block::STONE);
    place(&world, ground.east(), &Block::WATER);
    world.set_block_state(
        &ground.east().up(),
        Block::AIR.default_state.id,
        BlockFlags::empty(),
    );
    let frog = from_type(
        &EntityType::FROG,
        Vector3::new(f64::from(x) + 0.5, 100.0, 0.5),
        &world,
        uuid::Uuid::new_v4(),
    );
    frog.get_entity()
        .on_ground
        .store(true, std::sync::atomic::Ordering::Relaxed);
    world.spawn_entity(frog.clone());
    let mob = frog.get_mob().expect("a mob");
    mob.get_mob_entity().with_brain(mob, |brain| {
        brain.set(types::IS_PREGNANT, ());
        brain.set(
            types::WALK_TARGET,
            WalkTarget::new(
                Arc::new(BlockPosTracker::new(ground.offset(Vector3::new(30, 0, 0)))),
                1.0,
                1,
            ),
        );
    });
    let written = Arc::new(Mutex::new(0));
    let (count, cause) = (written.clone(), frog.clone());
    on::<GenericGameEvent, _>(move |event| {
        if event.event_key() == "block_place" && event.position().x == f64::from(x) + 1.5 {
            // `write_nbt` takes the brain lock.
            cause.write_nbt(&mut NbtCompound::new());
            *count.lock().unwrap() += 1;
        }
    });

    let ticking = frog.clone();
    within_timeout(move || {
        let mob = ticking.get_mob().expect("a mob");
        for _ in 0..40 {
            mob.custom_server_ai_step(ticking.as_ref());
        }
    });

    assert!(*written.lock().unwrap() >= 1, "the frog laid no spawn");
}

/// Two threads change the record of one jukebox, as a hopper above that inserts and a hopper
/// below that takes. The effects of the changes (`has_record`, the song) apply in storage order,
/// so the block ends with the state of the slot.
#[test]
fn threads_that_change_the_record_leave_the_block_with_the_state_of_the_slot() {
    let _serial = support::serial();
    let world = support::world();
    let x = 32_800;
    let pos = BlockPos::new(x, 100, 0);
    place(&world, pos, &Block::JUKEBOX);
    let jukebox = jukebox_at(&world, pos);
    let has_record = |world: &Arc<World>| {
        pumpkin_data::block_properties::JukeboxLikeProperties::from_state_id(
            world.get_block_state_id(&pos),
        )
        .has_record
    };

    for round in 0..200 {
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let (inserting, taking) = (jukebox.clone(), jukebox.clone());
        let (insert_barrier, take_barrier) = (barrier.clone(), barrier);
        let insert = std::thread::spawn(move || {
            insert_barrier.wait();
            for _ in 0..20 {
                inserting.set_stack(0, ItemStack::new(1, &Item::MUSIC_DISC_13));
            }
        });
        let take = std::thread::spawn(move || {
            take_barrier.wait();
            for _ in 0..20 {
                taking.remove_stack(0);
            }
        });
        insert.join().unwrap();
        take.join().unwrap();
        assert_eq!(
            has_record(&world),
            !jukebox.get_stack(0).is_empty(),
            "round {round}: the block and the slot disagree"
        );
        assert_eq!(
            jukebox.is_playing(),
            !jukebox.get_stack(0).is_empty(),
            "round {round}: the song and the slot disagree"
        );
    }
}

/// A creative slot set of the selected hotbar slot sends the equipment change after the screen
/// handler lock: the change reads the attribute modifiers, which fires
/// `ItemAttributeModifierEvent`. A handler of the event takes the screen handler lock.
#[test]
fn the_hotbar_equipment_change_of_a_creative_slot_set_waits_for_the_screen_handler_lock() {
    use pumpkin_core::{net::ClientPlatform, plugin::inventory::ItemAttributeModifierEvent};
    use pumpkin_protocol::{
        codec::item_stack_seralizer::ItemStackSerializer, java::server::play::SSetCreativeSlot,
    };

    let _serial = support::serial();
    let player = support::player("CreativeHotbar");
    player.gamemode.store(pumpkin_util::GameMode::Creative);
    let read: Arc<Mutex<Vec<bool>>> = Arc::default();
    let (record, reader) = (read.clone(), player.clone());
    on::<ItemAttributeModifierEvent, _>(move |event| {
        if event.item_stack.item.id == Item::NETHERITE_SWORD.id {
            record
                .lock()
                .unwrap()
                .push(reader.player_screen_handler.try_lock().is_ok());
        }
    });

    let setting = player;
    within_timeout(move || {
        let ClientPlatform::Java(client) = setting.client.as_ref() else {
            return;
        };
        // Slot 36 of the player screen is the first hotbar slot, the selected one.
        client
            .handle_set_creative_slot(
                &setting,
                SSetCreativeSlot::new(
                    36,
                    ItemStackSerializer::from(ItemStack::new(1, &Item::NETHERITE_SWORD)),
                ),
            )
            .unwrap();
    });

    let read = read.lock().unwrap();
    assert!(!read.is_empty(), "the event did not fire");
    assert!(read.iter().all(|free| *free), "{read:?}");
}
