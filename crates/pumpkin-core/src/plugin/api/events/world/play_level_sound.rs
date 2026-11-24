//! The sound events of `NeoForge`: a world plays a sound at a position or at an entity.

use std::sync::Arc;

use pumpkin_data::sound::SoundCategory;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::vector3::Vector3;

use crate::{
    entity::EntityBase,
    world::{World, sound::SoundHolder},
};

/// The concrete parent of the sound events, which the server never fires.
///
/// `NeoForge`: `net.neoforged.neoforge.event.PlayLevelSoundEvent`. Register a
/// [`ParentEventHandler`](crate::plugin::ParentEventHandler) for it with
/// `ModInit::register_parent_event` to get [`PlayLevelSoundEventAtPosition`] and
/// [`PlayLevelSoundEventAtEntity`].
#[derive(Event, Clone)]
pub struct PlayLevelSoundEvent;

/// A world plays a sound at a position, before it sends the packet.
///
/// `NeoForge`: `PlayLevelSoundEvent.AtPosition`, fired by `EventHooks.onPlaySoundAtPosition` in
/// `ServerLevel.playSeededSound`. [`World::play_seeded_sound`] fires it, and every sound of the
/// world goes through that call. `sound`, `source`, `new_volume` and `new_pitch` are settable. A
/// cancel, or a `sound` of `None`, sends nothing.
#[cancellable]
#[derive(Event, Clone)]
#[event_parents(PlayLevelSoundEvent)]
pub struct PlayLevelSoundEventAtPosition {
    /// The world that plays the sound (`getLevel`).
    pub world: Arc<World>,
    /// The position of the sound (`getPosition`).
    pub position: Vector3<f64>,
    /// The sound event, or `None` for no sound (`getSound`).
    pub sound: Option<SoundHolder>,
    /// The sound category (`getSource`).
    pub source: SoundCategory,
    /// The volume of the call (`getOriginalVolume`).
    pub original_volume: f32,
    /// The pitch of the call (`getOriginalPitch`).
    pub original_pitch: f32,
    /// The volume that the world plays (`getNewVolume`).
    pub new_volume: f32,
    /// The pitch that the world plays (`getNewPitch`).
    pub new_pitch: f32,
}

impl PlayLevelSoundEventAtPosition {
    #[must_use]
    pub const fn new(
        world: Arc<World>,
        position: Vector3<f64>,
        sound: SoundHolder,
        source: SoundCategory,
        volume: f32,
        pitch: f32,
    ) -> Self {
        Self {
            world,
            position,
            sound: Some(sound),
            source,
            original_volume: volume,
            original_pitch: pitch,
            new_volume: volume,
            new_pitch: pitch,
            cancelled: false,
        }
    }
}

/// A world plays a sound that follows an entity, before it sends the packet.
///
/// `NeoForge`: `PlayLevelSoundEvent.AtEntity`, fired by `EventHooks.onPlaySoundAtEntity` in the
/// entity form of `ServerLevel.playSeededSound`. [`World::play_seeded_sound_at_entity`] fires it.
/// The settable fields and the cancel are those of [`PlayLevelSoundEventAtPosition`].
#[cancellable]
#[derive(Event, Clone)]
#[event_parents(PlayLevelSoundEvent)]
pub struct PlayLevelSoundEventAtEntity {
    /// The world of the entity (`getLevel`).
    pub world: Arc<World>,
    /// The entity that the sound follows (`getEntity`).
    pub entity: Arc<dyn EntityBase>,
    /// The sound event, or `None` for no sound (`getSound`).
    pub sound: Option<SoundHolder>,
    /// The sound category (`getSource`).
    pub source: SoundCategory,
    /// The volume of the call (`getOriginalVolume`).
    pub original_volume: f32,
    /// The pitch of the call (`getOriginalPitch`).
    pub original_pitch: f32,
    /// The volume that the world plays (`getNewVolume`).
    pub new_volume: f32,
    /// The pitch that the world plays (`getNewPitch`).
    pub new_pitch: f32,
}

impl PlayLevelSoundEventAtEntity {
    #[must_use]
    pub fn new(
        world: Arc<World>,
        entity: Arc<dyn EntityBase>,
        sound: SoundHolder,
        source: SoundCategory,
        volume: f32,
        pitch: f32,
    ) -> Self {
        Self {
            world,
            entity,
            sound: Some(sound),
            source,
            original_volume: volume,
            original_pitch: pitch,
            new_volume: volume,
            new_pitch: pitch,
            cancelled: false,
        }
    }
}
