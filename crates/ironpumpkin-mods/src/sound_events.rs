//! Sound events of mods, and playing a sound to the players near a position.
//!
//! `NeoForge`: `SoundEvent.createVariableRangeEvent` and `SoundEvent.createFixedRangeEvent`,
//! registered in `Registries.SOUND_EVENT` and played with `Level#playSound`. The client plays the
//! sounds that its resource packs list under the name of the event.
//!
//! The server sends the sound by name, so every client can play it, and a client needs no
//! registry entry for it: `minecraft:sound_event` is not in the registry sync.
//!
//! ```
//! use ironpumpkin_mods::{
//!     ModInit,
//!     pumpkin_data::sound::SoundCategory,
//!     sound_events::{self, SoundEvent, SoundEventBuilder},
//!     world::World,
//! };
//!
//! static ZAP: SoundEvent = SoundEvent::new("examplemod:zap");
//!
//! fn register(cx: &mut ModInit) {
//!     let _ = cx.register_sound_event(SoundEventBuilder::new(ZAP.name()));
//! }
//!
//! fn zap(world: &World, position: ironpumpkin_mods::math::vector3::Vector3<f64>) {
//!     let _ = sound_events::play_sound(world, &ZAP, SoundCategory::Players, &position, 1.0, 1.0);
//! }
//! ```

use std::fmt;

use pumpkin_core::{plugin::api::events::registries::RegisterEvent, world::World};
use pumpkin_data::{
    dynamic::{ContentKind, RegistryError},
    dynamic_registries::{self, SoundEventDefinition},
    sound::{Sound, SoundCategory},
};
use pumpkin_util::math::vector3::Vector3;

use crate::ModInit;

/// A sound event of a mod.
#[derive(Debug, Clone)]
pub struct SoundEventBuilder(SoundEventDefinition);

impl SoundEventBuilder {
    /// A sound event named `name`, `namespace:path` outside `minecraft`, whose range grows with
    /// the volume. `NeoForge`: `SoundEvent.createVariableRangeEvent`.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(SoundEventDefinition {
            name: name.into(),
            fixed_range: None,
        })
    }

    /// Sets a fixed hearing range in blocks. `NeoForge`: `SoundEvent.createFixedRangeEvent`. The
    /// registry rejects a range that is not positive and finite.
    #[must_use]
    pub const fn fixed_range(mut self, range: f32) -> Self {
        self.0.fixed_range = Some(range);
        self
    }
}

/// A sound event by name: a vanilla sound such as `minecraft:entity.pig.ambient`, or a sound
/// event of a mod. A mod sound is valid after the freeze.
///
/// `NeoForge`: the `DeferredHolder<SoundEvent, SoundEvent>` of the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundEvent {
    name: &'static str,
}

impl SoundEvent {
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self { name }
    }

    /// The name of the event.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

/// The sound event is neither a vanilla sound nor a registered mod sound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownSoundEvent(pub String);

impl fmt::Display for UnknownSoundEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown sound event \"{}\"", self.0)
    }
}

impl std::error::Error for UnknownSoundEvent {}

/// Plays `sound` at `position` to the players in its range and returns how many players got it.
///
/// `NeoForge`: `Level#playSound(null, x, y, z, sound, source, volume, pitch)`. The range is the
/// fixed range of the event, or 16 blocks times a volume above 1.
///
/// # Errors
///
/// [`UnknownSoundEvent`] when no sound has the name.
pub fn play_sound(
    world: &World,
    sound: &SoundEvent,
    category: SoundCategory,
    position: &Vector3<f64>,
    volume: f32,
    pitch: f32,
) -> Result<usize, UnknownSoundEvent> {
    let fixed_range = if let Some(path) = sound.name.strip_prefix("minecraft:")
        && Sound::from_name(path).is_some()
    {
        None
    } else {
        dynamic_registries::sound_event_by_name(sound.name)
            .ok_or_else(|| UnknownSoundEvent(sound.name.to_owned()))?
            .fixed_range
    };
    Ok(world.play_sound_event_near(sound.name, fixed_range, category, position, volume, pitch))
}

impl ModInit {
    /// Registers a sound event. It applies at once, like [`ModInit::register_block`], and gets
    /// its id at the freeze.
    ///
    /// # Errors
    ///
    /// Returns the [`RegistryError`] of the registry: the name is invalid, uses the `minecraft`
    /// namespace or is already registered, the fixed range is not positive and finite, or the
    /// registry is frozen.
    pub fn register_sound_event(&mut self, sound: SoundEventBuilder) -> Result<(), RegistryError> {
        let name = sound.0.name.clone();
        self.keep_content_error(
            ContentKind::SoundEvent,
            name,
            dynamic_registries::register_sound_event(sound.0),
        )
    }
}

/// Registration of sound events from a handler of [`RegisterEvent`]. See
/// [`RegisterEventExt`](crate::content::RegisterEventExt) for the pattern.
pub trait RegisterSoundEvent {
    /// Registers a sound event on the fire of `minecraft:sound_event`.
    ///
    /// # Errors
    ///
    /// The [`RegistryError`] of the registry.
    fn register_sound_event(&self, sound: SoundEventBuilder) -> Result<(), RegistryError>;
}

impl RegisterSoundEvent for RegisterEvent {
    fn register_sound_event(&self, sound: SoundEventBuilder) -> Result<(), RegistryError> {
        let name = sound.0.name.clone();
        self.register(ContentKind::SoundEvent, &name, || {
            dynamic_registries::register_sound_event(sound.0)
        })
    }
}
