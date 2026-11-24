//! The sound events and the vanilla game event reach v0.2 plugins (spec `neoforge-event-bus`,
//! requirement "Wasm plugins see the same events", #179): a v0.2 handler gets the record, writes
//! back the settable fields and cancels, and the host keeps the fields that `NeoForge` cannot set.
#![expect(clippy::panic, reason = "a test fails on the first unexpected record")]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

mod guest;

use std::sync::{Arc, Mutex, PoisonError};

use guest::{fire, is_shared_world, plugin, resource};
use pumpkin_core::{
    entity::EntityBase,
    plugin::world::{GenericGameEvent, PlayLevelSoundEventAtEntity, PlayLevelSoundEventAtPosition},
    world::sound::{sound_holder_from_name, sound_holder_name},
};
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_protocol::IdOr;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_wasm_host_v0_2::pumpkin::plugin::{event::Event, sounds};

const fn stone() -> pumpkin_core::world::sound::SoundHolder {
    IdOr::Id(Sound::BlockStoneBreak as u16)
}

/// A handler reads the position sound, replaces the sound, the category, the volume and the
/// pitch, and the host keeps the position, the world and the original values.
#[test]
fn a_plugin_changes_a_sound_at_a_position() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let manager = plugin::<PlayLevelSoundEventAtPosition, _>(move |event, state| {
        let Event::PlayLevelSoundEventAtPosition(data) = event else {
            panic!("expected a sound at a position");
        };
        record.lock().unwrap_or_else(PoisonError::into_inner).push((
            is_shared_world(state, &data.target_world),
            data.position,
            data.sound.clone(),
            data.original_volume,
            data.original_pitch,
        ));
        data.sound = Some("minecraft:entity.pig.ambient".to_owned());
        data.source = sounds::SoundCategory::Hostile;
        data.new_volume = 2.0;
        data.new_pitch = 0.5;
        data.position = (1.0, 2.0, 3.0);
        data.original_volume = 9.0;
    });

    let event = fire(
        &manager,
        PlayLevelSoundEventAtPosition::new(
            support::world(),
            Vector3::new(4.0, 5.0, 6.0),
            stone(),
            SoundCategory::Blocks,
            1.5,
            1.25,
        ),
    );

    assert!(!event.cancelled);
    assert_eq!(
        sound_holder_name(event.sound.as_ref().unwrap_or_else(|| panic!("a sound"))),
        "entity.pig.ambient"
    );
    assert_eq!(event.source, SoundCategory::Hostile);
    assert_eq!((event.new_volume, event.new_pitch), (2.0, 0.5));
    assert_eq!((event.original_volume, event.original_pitch), (1.5, 1.25));
    assert_eq!(event.position, Vector3::new(4.0, 5.0, 6.0));
    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        [(
            true,
            (4.0, 5.0, 6.0),
            Some("minecraft:block.stone.break".to_owned()),
            1.5,
            1.25
        )]
    );
}

#[test]
fn a_plugin_removes_the_sound_and_cancels_at_a_position() {
    let manager = plugin::<PlayLevelSoundEventAtPosition, _>(|event, _state| {
        let Event::PlayLevelSoundEventAtPosition(data) = event else {
            panic!("expected a sound at a position");
        };
        data.sound = None;
        data.cancelled = true;
    });

    let event = fire(
        &manager,
        PlayLevelSoundEventAtPosition::new(
            support::world(),
            Vector3::default(),
            sound_holder_from_name("pack:custom.ping"),
            SoundCategory::Master,
            1.0,
            1.0,
        ),
    );

    assert!(event.cancelled);
    assert!(event.sound.is_none());
}

/// A sound that the handler leaves alone keeps its fixed range.
#[test]
fn an_unchanged_sound_keeps_its_holder() {
    let manager = plugin::<PlayLevelSoundEventAtPosition, _>(|_event, _state| {});
    let ranged = IdOr::Value(pumpkin_protocol::SoundEvent {
        sound_name: "pack:ranged".into(),
        range: Some(40.0),
    });

    let event = fire(
        &manager,
        PlayLevelSoundEventAtPosition::new(
            support::world(),
            Vector3::default(),
            ranged.clone(),
            SoundCategory::Master,
            1.0,
            1.0,
        ),
    );

    assert!(event.sound == Some(ranged));
}

#[test]
fn a_plugin_changes_a_sound_at_an_entity() {
    let player = support::player("WasmSoundEntity");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let manager = plugin::<PlayLevelSoundEventAtEntity, _>(move |event, state| {
        let Event::PlayLevelSoundEventAtEntity(data) = event else {
            panic!("expected a sound at an entity");
        };
        record.lock().unwrap_or_else(PoisonError::into_inner).push((
            resource(state, &data.entity).get_entity().entity_id,
            is_shared_world(state, &data.target_world),
            data.sound.clone(),
        ));
        data.sound = Some("pack:custom.ping".to_owned());
        data.source = sounds::SoundCategory::Neutral;
        data.new_volume = 3.0;
        data.new_pitch = 0.75;
        data.cancelled = true;
    });
    let entity: Arc<dyn EntityBase> = player.clone();

    let event = fire(
        &manager,
        PlayLevelSoundEventAtEntity::new(
            support::world(),
            entity,
            stone(),
            SoundCategory::Players,
            1.0,
            1.0,
        ),
    );

    assert!(event.cancelled);
    assert_eq!(
        sound_holder_name(event.sound.as_ref().unwrap_or_else(|| panic!("a sound"))),
        "pack:custom.ping"
    );
    assert_eq!(event.source, SoundCategory::Neutral);
    assert_eq!((event.new_volume, event.new_pitch), (3.0, 0.75));
    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        [(
            player.living_entity.entity.entity_id,
            true,
            Some("minecraft:block.stone.break".to_owned())
        )]
    );
}

/// The game event record has the key and the position, and a cancel reaches the host.
#[test]
fn a_plugin_cancels_a_vanilla_game_event() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let manager = plugin::<GenericGameEvent, _>(move |event, _state| {
        let Event::GenericGameEvent(data) = event else {
            panic!("expected a game event");
        };
        record
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((data.event_id.clone(), data.pos));
        data.cancelled = true;
    });

    let event = fire(
        &manager,
        GenericGameEvent::new(
            support::world(),
            "block_activate".to_owned(),
            Vector3::new(1.0, 2.0, 3.0),
            None,
        ),
    );

    assert!(event.cancelled);
    assert_eq!(event.event_key(), "block_activate");
    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        [("block_activate".to_owned(), (1.0, 2.0, 3.0))]
    );
}
