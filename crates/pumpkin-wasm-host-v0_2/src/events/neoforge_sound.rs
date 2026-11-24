//! The v0.2 conversions of the `NeoForge` 26.3 `PlayLevelSoundEvent.AtPosition` and
//! `PlayLevelSoundEvent.AtEntity`.
//!
//! A blocking handler writes back the sound, the category, the volume, the pitch and the cancel.
//! The other fields keep the native values, as `NeoForge` has no setter for them.

use std::sync::Arc;

use pumpkin_core::{
    plugin::{
        Context, EventPriority,
        world::{PlayLevelSoundEventAtEntity, PlayLevelSoundEventAtPosition},
    },
    world::sound::{SoundHolder, sound_holder_from_name, sound_holder_name},
};
use pumpkin_protocol::IdOr;
use pumpkin_wasm_host_common::state::PluginHostState;

use crate::{
    events::{
        WasmPluginEventHandler,
        neoforge::{add, discard},
        to_wasm_position,
    },
    pumpkin::plugin::{
        event::{Event, EventType},
        neoforge_events::{PlayLevelSoundEventAtEntityData, PlayLevelSoundEventAtPositionData},
    },
    world::{from_wit_sound_category, to_wit_sound_category},
};

macro_rules! sound_event {
    ($($event:tt)*) => {
        neoforge_event!(cleanup_neoforge_sound_event; $($event)*);
    };
}

/// The name of a sound event: a vanilla sound has the `minecraft:` namespace.
fn wit_sound_name(sound: &SoundHolder) -> String {
    match sound {
        IdOr::Id(_) => format!("minecraft:{}", sound_holder_name(sound)),
        IdOr::Value(_) => sound_holder_name(sound),
    }
}

/// Replaces `sound` with the sound of `name` when the name changed, so a sound that the handler
/// leaves alone keeps its fixed range.
fn apply_sound(sound: &mut Option<SoundHolder>, name: Option<&str>) {
    if sound.as_ref().map(wit_sound_name).as_deref() != name {
        *sound = name.map(sound_holder_from_name);
    }
}

sound_event!(
    PlayLevelSoundEventAtPosition,
    PlayLevelSoundEventAtPosition,
    |this, state| PlayLevelSoundEventAtPositionData {
        target_world: add(state, this.world.clone()),
        position: to_wasm_position(this.position),
        sound: this.sound.as_ref().map(wit_sound_name),
        source: to_wit_sound_category(this.source),
        original_volume: this.original_volume,
        original_pitch: this.original_pitch,
        new_volume: this.new_volume,
        new_pitch: this.new_pitch,
        cancelled: this.cancelled,
    },
    |target, data| {
        apply_sound(&mut target.sound, data.sound.as_deref());
        target.source = from_wit_sound_category(data.source);
        target.new_volume = data.new_volume;
        target.new_pitch = data.new_pitch;
        target.cancelled = data.cancelled;
    }
);

sound_event!(
    PlayLevelSoundEventAtEntity,
    PlayLevelSoundEventAtEntity,
    |this, state| PlayLevelSoundEventAtEntityData {
        entity: add(state, this.entity.clone()),
        target_world: add(state, this.world.clone()),
        sound: this.sound.as_ref().map(wit_sound_name),
        source: to_wit_sound_category(this.source),
        original_volume: this.original_volume,
        original_pitch: this.original_pitch,
        new_volume: this.new_volume,
        new_pitch: this.new_pitch,
        cancelled: this.cancelled,
    },
    |target, data| {
        apply_sound(&mut target.sound, data.sound.as_deref());
        target.source = from_wit_sound_category(data.source);
        target.new_volume = data.new_volume;
        target.new_pitch = data.new_pitch;
        target.cancelled = data.cancelled;
    }
);

/// Releases the resources of a record that the host built for a handler.
pub(super) fn cleanup_neoforge_sound_event(event: &Event, state: &mut PluginHostState) {
    match event {
        Event::PlayLevelSoundEventAtPosition(data) => discard(state, &data.target_world),
        Event::PlayLevelSoundEventAtEntity(data) => {
            discard(state, &data.entity);
            discard(state, &data.target_world);
        }
        _ => {}
    }
}

/// Registers the handler of a Wasm plugin for a sound event.
pub fn register_neoforge_sound_event(
    ctx: &Context,
    handler: &Arc<WasmPluginEventHandler>,
    priority: EventPriority,
    blocking: bool,
    event_type: EventType,
) {
    match event_type {
        EventType::PlayLevelSoundEventAtPosition => {
            ctx.register_event::<PlayLevelSoundEventAtPosition, _>(
                Arc::clone(handler),
                priority,
                blocking,
            );
        }
        EventType::PlayLevelSoundEventAtEntity => {
            ctx.register_event::<PlayLevelSoundEventAtEntity, _>(
                Arc::clone(handler),
                priority,
                blocking,
            );
        }
        _ => tracing::error!(
            "non-sound NeoForge event should not be routed to register_neoforge_sound_event"
        ),
    }
}
