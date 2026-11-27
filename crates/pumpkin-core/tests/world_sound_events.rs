//! The sound entry points of a world and the vanilla game event path (issue #179): the
//! `NeoForge` `PlayLevelSoundEvent.AtPosition` and `.AtEntity` fire in `World::play_seeded_sound`
//! and `World::play_seeded_sound_at_entity`, the sound ranges follow vanilla, and
//! `VanillaGameEvent` fires from `World::emit_game_event_with_context` with a cancel that stops
//! the vanilla listeners.
//!
//! Each test works at its own x position and filters its handlers by it, because the handlers
//! stay registered on the shared server. A test reads the packets of its player up to a marker
//! sound, so it can tell "sent nothing" from "not yet sent".
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::{
    marker::PhantomData,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use pumpkin_core::{
    entity::{EntityBase, player::Player},
    net::java::neoforge::NegotiatedState,
    plugin::{
        BoxFuture, EventHandler, EventPriority, Payload,
        world::{
            GenericGameEvent, LevelTickEventPost, PlayLevelSoundEventAtEntity,
            PlayLevelSoundEventAtPosition,
        },
    },
    server::Server,
    world::sound::{SoundHolder, sound_holder_from_name, sound_holder_name},
};
use pumpkin_data::{
    entity::EntityType,
    packet::CURRENT_MC_VERSION,
    sound::{Sound, SoundCategory},
};
use pumpkin_protocol::{
    IdOr,
    java::{
        client::play::{CEntitySoundEffect, CSoundEffect},
        packet_decoder::TCPNetworkDecoder,
    },
    packet::MultiVersionJavaPacket,
    ser::NetworkReadExt,
};
use pumpkin_util::math::vector3::Vector3;

type Peer = TCPNetworkDecoder<tokio::io::BufReader<tokio::net::TcpStream>>;

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

/// A sound packet as a client reads it.
#[derive(Debug, PartialEq)]
struct Heard {
    /// The sound: a registry name, or the name of a direct event.
    sound: String,
    category: i32,
    /// The x of the position of a position sound, or the entity id of an entity sound.
    at: i32,
    volume: f32,
    pitch: f32,
}

fn read_sound(body: &mut &[u8]) -> String {
    let id = body.get_var_int().unwrap().0;
    if id == 0 {
        let name = body.get_str().unwrap().to_string();
        if body.get_bool().unwrap() {
            body.get_f32_be().unwrap();
        }
        name
    } else {
        Sound::NAMES[usize::try_from(id - 1).unwrap()].to_owned()
    }
}

/// Reads the packets of `peer` up to the sound `marker` and returns the sound packets before it.
/// A sound at a position reports the x of the position in blocks; an entity sound the entity id.
fn heard_until(peer: &mut Peer, marker: Sound) -> Vec<Heard> {
    support::RUNTIME.block_on(async {
        let mut heard = Vec::new();
        loop {
            let packet = tokio::time::timeout(Duration::from_secs(10), peer.get_raw_packet())
                .await
                .unwrap()
                .unwrap();
            let mut body = &packet.payload[..];
            let entry = if packet.id == CSoundEffect::to_id(CURRENT_MC_VERSION) {
                let sound = read_sound(&mut body);
                let category = body.get_var_int().unwrap().0;
                let x = body.get_i32_be().unwrap() / 8;
                body.get_i32_be().unwrap();
                body.get_i32_be().unwrap();
                Heard {
                    sound,
                    category,
                    at: x,
                    volume: body.get_f32_be().unwrap(),
                    pitch: body.get_f32_be().unwrap(),
                }
            } else if packet.id == CEntitySoundEffect::to_id(CURRENT_MC_VERSION) {
                let sound = read_sound(&mut body);
                let category = body.get_var_int().unwrap().0;
                let entity = body.get_var_int().unwrap().0;
                Heard {
                    sound,
                    category,
                    at: entity,
                    volume: body.get_f32_be().unwrap(),
                    pitch: body.get_f32_be().unwrap(),
                }
            } else {
                continue;
            };
            if entry.sound == marker.to_name_string() {
                return heard;
            }
            heard.push(entry);
        }
    })
}

trait SoundName {
    fn to_name_string(&self) -> String;
}

impl SoundName for Sound {
    fn to_name_string(&self) -> String {
        Self::NAMES[*self as usize].to_owned()
    }
}

/// A player of its own at `x`, with the client end of its socket.
fn listener(name: &str, x: f64) -> (Arc<Player>, Peer) {
    let (player, peer) = support::player_with_peer(name, NegotiatedState::default());
    player
        .living_entity
        .entity
        .set_pos(Vector3::new(x, 100.0, 0.5));
    (
        player,
        TCPNetworkDecoder::new(tokio::io::BufReader::new(peer)),
    )
}

const fn stone(x: f64) -> (u16, Vector3<f64>) {
    (Sound::BlockStoneBreak as u16, Vector3::new(x, 100.0, 0.5))
}

/// The marker that every test sends last, with a sound that no handler touches.
fn marker(position: &Vector3<f64>) {
    support::world().play_sound_raw(
        Sound::BlockAnvilLand as u16,
        SoundCategory::Master,
        position,
        1.0,
        1.0,
    );
}

#[test]
fn a_handler_changes_the_sound_the_source_the_volume_and_the_pitch_at_a_position() {
    let _serial = support::serial();
    let x = 10_000.0;
    let (_player, mut peer) = listener("SoundChange", x);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    on::<PlayLevelSoundEventAtPosition, _>(move |event| {
        if event.position.x == x && event.original_volume == 1.5 {
            record.lock().unwrap().push((
                sound_holder_name(event.sound.as_ref().unwrap()),
                event.source,
                event.original_pitch,
                event.new_volume,
                event.position.z,
            ));
            event.sound = Some(sound_holder_from_name("minecraft:entity.pig.ambient"));
            event.source = SoundCategory::Hostile;
            event.new_volume = 2.0;
            event.new_pitch = 0.5;
        }
    });
    let (id, position) = stone(x);

    support::world().play_sound_raw(id, SoundCategory::Blocks, &position, 1.5, 1.0);
    marker(&position);

    assert_eq!(
        *seen.lock().unwrap(),
        [(
            "block.stone.break".to_owned(),
            SoundCategory::Blocks,
            1.0,
            1.5,
            0.5
        )]
    );
    assert_eq!(
        heard_until(&mut peer, Sound::BlockAnvilLand),
        [Heard {
            sound: "entity.pig.ambient".to_owned(),
            category: SoundCategory::Hostile as i32,
            at: x as i32,
            volume: 2.0,
            pitch: 0.5,
        }]
    );
}

#[test]
fn a_cancel_or_a_null_sound_sends_nothing_at_a_position() {
    let _serial = support::serial();
    let x = 11_000.0;
    let (_player, mut peer) = listener("SoundCancel", x);
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    on::<PlayLevelSoundEventAtPosition, _>(move |event| {
        if event.position.x == x
            && event.original_pitch == 1.0
            && event.original_volume == 1.0
            && sound_holder_name(event.sound.as_ref().unwrap()) == "block.stone.break"
        {
            // The first sound is cancelled, the second loses its sound, the third passes.
            match count.fetch_add(1, Ordering::Relaxed) {
                0 => event.cancelled = true,
                1 => event.sound = None,
                _ => {}
            }
        }
    });
    let (id, position) = stone(x);

    for _ in 0..3 {
        support::world().play_sound_raw(id, SoundCategory::Blocks, &position, 1.0, 1.0);
    }
    marker(&position);

    assert_eq!(calls.load(Ordering::Relaxed), 3);
    let heard = heard_until(&mut peer, Sound::BlockAnvilLand);
    assert_eq!(heard.len(), 1);
    assert_eq!(heard[0].sound, "block.stone.break");
}

#[test]
fn a_handler_changes_the_sound_and_cancels_at_an_entity() {
    let _serial = support::serial();
    let x = 12_000.0;
    let (player, mut peer) = listener("SoundEntity", x);
    let entity_id = player.living_entity.entity.entity_id;
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    on::<PlayLevelSoundEventAtEntity, _>(move |event| {
        if event.entity.get_entity().entity_id == entity_id {
            assert_eq!(event.original_volume, 1.5);
            assert_eq!(event.original_pitch, 1.0);
            // The first sound changes, the second is cancelled, the third loses its sound.
            match count.fetch_add(1, Ordering::Relaxed) {
                0 => {
                    event.sound = Some(sound_holder_from_name("entity.pig.ambient"));
                    event.source = SoundCategory::Neutral;
                    event.new_volume = 3.0;
                    event.new_pitch = 0.75;
                }
                1 => event.cancelled = true,
                2 => event.sound = None,
                _ => {}
            }
        }
    });
    let world = support::world();
    let stone: SoundHolder = IdOr::Id(Sound::BlockStoneBreak as u16);

    for _ in 0..4 {
        world.play_seeded_sound_at_entity(
            None,
            player.get_entity(),
            stone.clone(),
            SoundCategory::Players,
            1.5,
            1.0,
            0,
        );
    }
    marker(&Vector3::new(x, 100.0, 0.5));

    assert_eq!(calls.load(Ordering::Relaxed), 4);
    assert_eq!(
        heard_until(&mut peer, Sound::BlockAnvilLand),
        [
            Heard {
                sound: "entity.pig.ambient".to_owned(),
                category: SoundCategory::Neutral as i32,
                at: entity_id,
                volume: 3.0,
                pitch: 0.75,
            },
            Heard {
                sound: "block.stone.break".to_owned(),
                category: SoundCategory::Players as i32,
                at: entity_id,
                volume: 1.5,
                pitch: 1.0,
            },
        ]
    );
}

#[test]
fn the_except_player_does_not_hear_a_sound() {
    let _serial = support::serial();
    let x = 13_000.0;
    let (player, mut peer) = listener("SoundExcept", x);
    let (_other, mut other_peer) = listener("SoundOther", x);
    let position = Vector3::new(x, 100.0, 0.5);

    let count = support::world().play_seeded_sound(
        Some(&player),
        &position,
        IdOr::Id(Sound::BlockStoneBreak as u16),
        SoundCategory::Blocks,
        1.0,
        1.0,
        0,
    );
    marker(&position);

    assert_eq!(count, 1);
    assert!(heard_until(&mut peer, Sound::BlockAnvilLand).is_empty());
    assert_eq!(heard_until(&mut other_peer, Sound::BlockAnvilLand).len(), 1);
}

/// A sound has the range of its event: 16 blocks, or 16 blocks times a volume above 1, and the
/// range is a distance, not a count of chunks.
#[test]
fn a_sound_reaches_the_players_closer_than_its_range() {
    let _serial = support::serial();
    let x = 14_000.0;
    let (_near, _near_peer) = listener("RangeNear", x + 15.0);
    let (_edge, _edge_peer) = listener("RangeEdge", x + 17.0);
    let (_far, _far_peer) = listener("RangeFar", x + 100.0);
    let (_goat, _goat_peer) = listener("RangeGoat", x + 200.0);
    let position = Vector3::new(x, 100.0, 0.5);
    let world = support::world();
    let play = |volume: f32| {
        world.play_seeded_sound(
            None,
            &position,
            IdOr::Id(Sound::BlockStoneBreak as u16),
            SoundCategory::Blocks,
            volume,
            1.0,
            0,
        )
    };

    assert_eq!(play(1.0), 1);
    assert_eq!(play(2.0), 2);
    // A goat horn plays at volume 16, which reaches 256 blocks.
    assert_eq!(play(16.0), 4);
}

/// `play_custom_sound` has a range, and a sound that no registry knows has the default one.
#[test]
fn a_custom_sound_has_a_range() {
    let _serial = support::serial();
    let x = 15_000.0;
    let (_player, mut peer) = listener("CustomRange", x + 10.0);
    let (_far, mut far_peer) = listener("CustomFar", x + 100.0);
    let position = Vector3::new(x, 100.0, 0.5);
    let world = support::world();

    world.play_custom_sound(
        "pack:custom.ping",
        SoundCategory::Blocks,
        &position,
        1.0,
        1.0,
    );
    world.play_custom_sound(
        "pack:custom.loud",
        SoundCategory::Blocks,
        &position,
        8.0,
        1.0,
    );
    marker(&Vector3::new(x + 10.0, 100.0, 0.5));
    marker(&Vector3::new(x + 100.0, 100.0, 0.5));

    let near: Vec<_> = heard_until(&mut peer, Sound::BlockAnvilLand)
        .into_iter()
        .map(|heard| heard.sound)
        .collect();
    let far: Vec<_> = heard_until(&mut far_peer, Sound::BlockAnvilLand)
        .into_iter()
        .filter(|heard| heard.at == x as i32)
        .map(|heard| heard.sound)
        .collect();
    assert_eq!(near, ["pack:custom.ping", "pack:custom.loud"]);
    assert_eq!(far, ["pack:custom.loud"]);
}

/// With no handler a fire site builds no event.
#[test]
fn a_fire_site_without_a_handler_builds_no_event() {
    let _serial = support::serial();
    let built = AtomicUsize::new(0);

    let event = support::world().plugin_event_in_world(|world| {
        built.fetch_add(1, Ordering::Relaxed);
        LevelTickEventPost::new(world)
    });

    assert!(event.is_none());
    assert_eq!(built.load(Ordering::Relaxed), 0);
}

/// Every game event that Pumpkin can emit reaches `VanillaGameEvent` once, from the one emit
/// path, with its key, its position and its world.
#[test]
fn one_event_per_fire_site_vanilla_game_event() {
    let _serial = support::serial();
    let x = 16_000.0;
    let names: Vec<String> = serde_json::from_str::<Vec<serde_json::Value>>(include_str!(
        "../../../assets/game_event.json"
    ))
    .unwrap()
    .iter()
    .map(|event| event["name"].as_str().unwrap().to_owned())
    .collect();
    assert_eq!(names.len(), 61);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    on::<GenericGameEvent, _>(move |event| {
        if event.position().x == x {
            assert!(event.cause.is_none());
            record.lock().unwrap().push(event.event_key().to_owned());
        }
    });
    let world = support::world();

    for name in &names {
        world.emit_game_event(name, Vector3::new(x, 64.5, 0.5));
    }

    assert_eq!(*seen.lock().unwrap(), names);
}

/// The cause of a game event is the entity of the context.
#[test]
fn a_vanilla_game_event_carries_its_cause() {
    let _serial = support::serial();
    let x = 17_000.0;
    let player = support::player("GameEventCause");
    player
        .living_entity
        .entity
        .set_pos(Vector3::new(x, 100.0, 0.5));
    let cause = player.living_entity.entity.entity_id;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    on::<GenericGameEvent, _>(move |event| {
        if event.position().x == x {
            record.lock().unwrap().push(
                event
                    .cause
                    .as_ref()
                    .map(|cause| cause.get_entity().entity_id),
            );
        }
    });

    support::world().emit_game_event_with_context(
        "entity_action",
        Vector3::new(x, 100.0, 0.5),
        &pumpkin_core::world::game_event::GameEventContext::of_entity(Some(player.as_ref())),
    );

    assert_eq!(*seen.lock().unwrap(), [Some(cause)]);
}

/// A cancel stops the vanilla listeners: an allay hears a jukebox start through the game event
/// and dances, and a cancelled event leaves it still.
#[test]
fn a_cancel_stops_the_vanilla_game_event_listeners() {
    let _serial = support::serial();
    let x = 2.5;
    let world = support::world();
    // The listeners of an unloaded chunk hear nothing.
    let chunk = pumpkin_util::math::vector2::Vector2::new(0, 0);
    world.set_chunk_forced(chunk, true);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while !world.level.is_chunk_loaded(&chunk) {
        assert!(
            std::time::Instant::now() < deadline,
            "the chunk did not load"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let spawn = |dz: f64| {
        let entity = pumpkin_core::entity::r#type::from_type(
            &EntityType::ALLAY,
            Vector3::new(x, 120.0, 0.5 + dz),
            &world,
            uuid::Uuid::new_v4(),
        );
        world.spawn_entity(entity.clone());
        entity
    };
    let allay_dances = |entity: &Arc<dyn EntityBase>| {
        entity
            .cast_any()
            .downcast_ref::<pumpkin_core::entity::passive::allay::AllayEntity>()
            .unwrap()
            .is_dancing()
    };
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let cancelled = cancel.clone();
    on::<GenericGameEvent, _>(move |event| {
        if event.position().x == x {
            event.cancelled = cancelled.load(Ordering::Relaxed);
        }
    });
    let allay = spawn(0.0);

    world.emit_game_event("jukebox_play", Vector3::new(x, 121.0, 0.5));
    assert!(!allay_dances(&allay));

    cancel.store(false, Ordering::Relaxed);
    world.emit_game_event("jukebox_play", Vector3::new(x, 121.0, 0.5));
    assert!(allay_dances(&allay));
}

/// The death sound of the ender dragon is a global level event, as `EnderDragon.tickDeath`
/// sends it: a player far from the dragon gets it, with the global flag.
#[test]
fn the_dragon_death_sound_is_a_global_level_event() {
    use pumpkin_core::entity::boss::ender_dragon::{
        EnderDragonEntity,
        phase::{DyingPhase, Phase},
    };
    use pumpkin_protocol::java::client::play::CWorldEvent;

    let _serial = support::serial();
    let (_player, mut peer) = listener("DragonFar", 21_000.0);
    let world = support::world();
    let dragon = pumpkin_core::entity::r#type::from_type(
        &EntityType::ENDER_DRAGON,
        Vector3::new(0.0, 100.0, 0.5),
        &world,
        uuid::Uuid::new_v4(),
    );
    let dragon = dragon
        .cast_any()
        .downcast_ref::<EnderDragonEntity>()
        .unwrap();

    DyingPhase.begin(dragon);
    DyingPhase.tick(dragon);

    let found = support::RUNTIME.block_on(async {
        loop {
            let packet = tokio::time::timeout(Duration::from_secs(10), peer.get_raw_packet())
                .await
                .unwrap()
                .unwrap();
            if packet.id == CWorldEvent::to_id(CURRENT_MC_VERSION) {
                let mut body = &packet.payload[..];
                let event = body.get_i32_be().unwrap();
                body.get_i64_be().unwrap();
                body.get_i32_be().unwrap();
                return (event, body.get_bool().unwrap());
            }
        }
    });
    assert_eq!(found, (1028, true));
}

/// The use of a goat horn plays an entity sound with the player as `except`, and emits a game event
/// with the player as its cause. A handler of both events reads the inventory of the player and
/// plays a sound and emits a game event from inside the use. The call does not deadlock, and the
/// horn sound reaches the other player but not the player of the horn.
#[test]
fn a_handler_can_play_a_sound_and_emit_a_game_event_from_inside_an_item_use() {
    use pumpkin_core::{item::ItemBehaviour, item::items::goat_horn::GoatHornItem};
    use pumpkin_data::{data_component_impl::InstrumentImpl, item::Item, item_stack::ItemStack};

    let _serial = support::serial();
    let x = 22_000.0;
    let (player, mut own_peer) = listener("HornPlayer", x);
    let (_other, mut other_peer) = listener("HornListener", x + 10.0);
    let mut horn = ItemStack::new(1, &Item::GOAT_HORN);
    horn.set_data_component(InstrumentImpl {
        instrument: "minecraft:ponder_goat_horn".into(),
    });
    player
        .inventory
        .set_stack_in_hand(pumpkin_util::Hand::Right, horn);
    let entity_id = player.living_entity.entity.entity_id;
    let reads = Arc::new(AtomicUsize::new(0));
    let (sound_reads, game_reads) = (reads.clone(), reads.clone());
    let world = support::world();
    let nested = world.clone();
    on::<PlayLevelSoundEventAtEntity, _>(move |event| {
        if event.entity.get_entity().entity_id == entity_id {
            // The handler runs inside the use: it reads the inventory and plays a sound.
            assert!(
                !event
                    .entity
                    .get_player()
                    .unwrap()
                    .inventory()
                    .held_item()
                    .is_empty()
            );
            nested.play_sound_raw(
                Sound::BlockStoneBreak as u16,
                SoundCategory::Blocks,
                &Vector3::new(x + 5.0, 100.0, 0.5),
                1.0,
                1.0,
            );
            sound_reads.fetch_add(1, Ordering::Relaxed);
        }
    });
    let nested = world;
    on::<GenericGameEvent, _>(move |event| {
        if event.event_key() == "instrument_play" && event.position().x == x {
            assert_eq!(
                event
                    .cause
                    .as_ref()
                    .map(|cause| cause.get_entity().entity_id),
                Some(entity_id)
            );
            nested.emit_game_event("block_change", Vector3::new(x, 100.0, 0.5));
            game_reads.fetch_add(1, Ordering::Relaxed);
        }
    });

    let (tx, rx) = std::sync::mpsc::channel();
    let using = player;
    std::thread::spawn(move || {
        GoatHornItem.normal_use_with_hand(
            &Item::GOAT_HORN,
            &using,
            0.0,
            0.0,
            pumpkin_util::Hand::Right,
        );
        tx.send(()).unwrap();
    });
    rx.recv_timeout(Duration::from_secs(60))
        .expect("the use of the horn hangs");
    marker(&Vector3::new(x + 5.0, 100.0, 0.5));

    assert_eq!(reads.load(Ordering::Relaxed), 2);
    let own: Vec<_> = heard_until(&mut own_peer, Sound::BlockAnvilLand)
        .into_iter()
        .map(|heard| heard.sound)
        .collect();
    let other: Vec<_> = heard_until(&mut other_peer, Sound::BlockAnvilLand)
        .into_iter()
        .map(|heard| heard.sound)
        .collect();
    assert_eq!(own, ["block.stone.break"]);
    assert_eq!(other.len(), 2);
    assert!(
        other.contains(
            &Sound::NAMES[pumpkin_data::instrument::Instrument::PonderGoatHorn.sound() as usize]
                .to_owned()
        )
    );
}

/// A level event as a client reads it: the event, the block position, the data and the global flag.
type LevelEventPacket = (i32, (i32, i32, i32), i32, bool);

/// Reads the level events of `peer` up to the marker sound.
fn level_events_until(peer: &mut Peer, marker: Sound) -> Vec<LevelEventPacket> {
    use pumpkin_protocol::java::client::play::CWorldEvent;
    support::RUNTIME.block_on(async {
        let mut events = Vec::new();
        loop {
            let packet = tokio::time::timeout(Duration::from_secs(10), peer.get_raw_packet())
                .await
                .unwrap()
                .unwrap();
            let mut body = &packet.payload[..];
            if packet.id == CWorldEvent::to_id(CURRENT_MC_VERSION) {
                let event = body.get_i32_be().unwrap();
                let position =
                    pumpkin_util::math::position::BlockPos::from_i64(body.get_i64_be().unwrap());
                let data = body.get_i32_be().unwrap();
                events.push((
                    event,
                    (position.0.x, position.0.y, position.0.z),
                    data,
                    body.get_bool().unwrap(),
                ));
            } else if packet.id == CSoundEffect::to_id(CURRENT_MC_VERSION)
                && read_sound(&mut body) == marker.to_name_string()
            {
                return events;
            }
        }
    })
}

/// With `global_sound_events` on, each player gets its own packet: the block center within 32
/// blocks, else a point 32 blocks from the player toward the event.
#[test]
fn a_global_level_event_gives_each_player_a_position_within_32_blocks() {
    use pumpkin_data::world::WorldEvent;
    use pumpkin_util::math::position::BlockPos;

    let _serial = support::serial();
    let x = 23_000.0;
    let (_near, mut near_peer) = listener("GlobalNear", x);
    let (_far, mut far_peer) = listener("GlobalFar", x + 1000.0);
    let world = support::world();

    world.sync_global_world_event(
        WorldEvent::SoundDragonDeath,
        BlockPos::new(x as i32 + 10, 100, 0),
        0,
    );
    marker(&Vector3::new(x, 100.0, 0.5));
    marker(&Vector3::new(x + 1000.0, 100.0, 0.5));

    let near = level_events_until(&mut near_peer, Sound::BlockAnvilLand);
    let far = level_events_until(&mut far_peer, Sound::BlockAnvilLand);
    assert_eq!(near, [(1028, (x as i32 + 10, 100, 0), 0, true)]);
    // 32 blocks from the player at x + 1000, toward the event 990 blocks to the west.
    assert_eq!(far, [(1028, (x as i32 + 1000 - 32, 100, 0), 0, true)]);
}

/// Puts the rule `global_sound_events` back on when the test ends, so another test of the process
/// sees the default.
struct RestoreRule(Arc<pumpkin_core::world::World>);

impl Drop for RestoreRule {
    fn drop(&mut self) {
        self.0.set_game_rule(
            &pumpkin_data::game_rules::GameRule::GlobalSoundEvents,
            pumpkin_data::game_rules::GameRuleValue::Bool(true),
        );
    }
}

/// With the rule off the event is local: the players closer than 64 blocks get it, without the
/// global flag.
#[test]
fn a_global_level_event_is_local_when_global_sound_events_is_off() {
    use pumpkin_data::{
        game_rules::{GameRule, GameRuleValue},
        world::WorldEvent,
    };
    use pumpkin_util::math::position::BlockPos;

    let _serial = support::serial();
    let x = 25_000.0;
    let (_near, mut near_peer) = listener("LocalNear", x + 50.0);
    let (_far, mut far_peer) = listener("LocalFar", x + 70.0);
    let world = support::world();
    let _restore = RestoreRule(world.clone());
    world.set_game_rule(&GameRule::GlobalSoundEvents, GameRuleValue::Bool(false));

    world.sync_global_world_event(
        WorldEvent::SoundDragonDeath,
        BlockPos::new(x as i32, 100, 0),
        7,
    );
    marker(&Vector3::new(x + 50.0, 100.0, 0.5));
    marker(&Vector3::new(x + 70.0, 100.0, 0.5));

    let near = level_events_until(&mut near_peer, Sound::BlockAnvilLand);
    let far = level_events_until(&mut far_peer, Sound::BlockAnvilLand);
    assert_eq!(near, [(1028, (x as i32, 100, 0), 7, false)]);
    assert!(far.is_empty());
}

/// A player of another world gets its own position.
#[test]
fn a_global_level_event_position_follows_vanilla() {
    use pumpkin_core::world::sound::global_event_position;

    let center = Vector3::new(10.5, 64.5, 0.5);
    let near = Vector3::new(0.0, 64.0, 0.5);
    let far = Vector3::new(-100.0, 64.5, 0.5);

    assert_eq!(global_event_position(center, near, true), center);
    assert_eq!(
        global_event_position(center, far, true),
        Vector3::new(-68.0, 64.5, 0.5)
    );
    assert_eq!(global_event_position(center, far, false), far);
}

/// The raid horn goes to the players within 64 blocks horizontally and to the players in the
/// raid, whatever their distance, 13 blocks from the player toward the raid.
#[test]
fn the_raid_horn_reaches_the_players_in_range_and_in_the_raid() {
    use pumpkin_core::world::raid::Raid;
    use pumpkin_util::{difficulty::Difficulty, math::position::BlockPos};

    let _serial = support::serial();
    let x = 26_000.0;
    let (_near, mut near_peer) = listener("RaidNear", x + 30.0);
    let (_far, mut far_peer) = listener("RaidFar", x + 100.0);
    let (member, mut member_peer) = listener("RaidMember", x - 100.0);
    let mut raid = Raid::new(1, BlockPos::new(x as i32, 100, 0), Difficulty::Normal);
    raid.players_in_raid.insert(member.gameprofile.id);

    raid.play_sound(&support::world(), BlockPos::new(x as i32, 100, 0));
    marker(&Vector3::new(x + 30.0, 100.0, 0.5));
    marker(&Vector3::new(x + 100.0, 100.0, 0.5));
    marker(&Vector3::new(x - 100.0, 100.0, 0.5));

    let horn = |peer: &mut Peer| -> Vec<(i32, f32)> {
        heard_until(peer, Sound::BlockAnvilLand)
            .into_iter()
            .filter(|heard| heard.sound == "event.raid.horn")
            .map(|heard| (heard.at, heard.volume))
            .collect()
    };
    // The packet position is 13 blocks from the player toward the raid center at x + 0.5.
    assert_eq!(horn(&mut near_peer), [(x as i32 + 17, 64.0)]);
    assert!(horn(&mut far_peer).is_empty());
    assert_eq!(horn(&mut member_peer), [(x as i32 - 87, 64.0)]);
}
