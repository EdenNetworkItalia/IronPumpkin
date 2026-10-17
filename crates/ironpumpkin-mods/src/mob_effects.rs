//! Mob effects of mods, and the effects of living entities.
//!
//! `NeoForge`: a `MobEffect` subclass registered in `Registries.MOB_EFFECT`, applied with
//! `LivingEntity#addEffect` and read with `hasEffect`, `getEffect` and `removeEffect`.
//!
//! A mod registers an effect with a [`MobEffectBuilder`]. Its behaviour is a [`MobEffect`]
//! implementation, the trait of the vanilla effects: the server asks
//! [`MobEffect::should_apply_effect_tick`] every tick while the effect lasts and calls
//! [`MobEffect::apply_effect_tick`] when it answers `true`, as vanilla calls
//! `MobEffect#applyEffectTick`. The attribute modifiers of the builder apply while the effect
//! lasts, scaled by the amplifier plus one, like `MobEffect#addAttributeModifier`.
//!
//! Saves name the effect. A client that synced the registries (a `NeoForge` client) gets the
//! effect packets with the id of the mod effect; a vanilla client gets none for a mod effect, and
//! sees only its particles.
//!
//! ```
//! use std::sync::Arc;
//!
//! use ironpumpkin_mods::{
//!     ModInit,
//!     entity::living::LivingEntity,
//!     mob_effects::{MobEffect, MobEffectBuilder, MobEffectCategory},
//! };
//!
//! struct Shock;
//!
//! impl MobEffect for Shock {
//!     fn should_apply_effect_tick(&self, duration: i32, _amplifier: u8) -> bool {
//!         duration % 20 == 0
//!     }
//!
//!     fn apply_effect_tick(&self, living: &LivingEntity, _amplifier: u8) {
//!         let _ = living;
//!     }
//! }
//!
//! fn register(cx: &mut ModInit) {
//!     let _ = cx.register_mob_effect(
//!         MobEffectBuilder::new("examplemod:shock", MobEffectCategory::Harmful, 0x00_FF_FF)
//!             .behaviour(Arc::new(Shock)),
//!     );
//! }
//! ```

use std::{fmt, sync::Arc};

use pumpkin_core::{
    entity::{effect::custom, living::LivingEntity},
    plugin::api::events::registries::RegisterEvent,
};
use pumpkin_data::{
    dynamic::{ContentKind, RegistryError},
    dynamic_registries::{MobEffectDefinition, MobEffectModifierDefinition},
};

pub use pumpkin_core::entity::effect::MobEffect;
pub use pumpkin_data::data_component_impl::Operation;
pub use pumpkin_data::effect::{MobEffectCategory, StatusEffect};
pub use pumpkin_data::potion::Effect;

use crate::ModInit;

/// A mob effect of a mod.
#[derive(Clone)]
pub struct MobEffectBuilder {
    definition: MobEffectDefinition,
    behaviour: Option<Arc<dyn MobEffect>>,
}

impl MobEffectBuilder {
    /// An effect named `name`, `namespace:path` outside `minecraft`, with the particle colour
    /// `color` (packed RGB). `NeoForge`: `new MobEffect(category, color)`.
    #[must_use]
    pub fn new(name: impl Into<String>, category: MobEffectCategory, color: i32) -> Self {
        Self {
            definition: MobEffectDefinition {
                name: name.into(),
                category,
                color,
                attribute_modifiers: Vec::new(),
            },
            behaviour: None,
        }
    }

    /// Adds a modifier of `attribute` while the effect lasts. The amount applies at amplifier 0
    /// and scales by the amplifier plus one. `NeoForge`: `MobEffect#addAttributeModifier`. The
    /// freeze fails when no attribute has the name.
    #[must_use]
    pub fn attribute_modifier(
        mut self,
        attribute: impl Into<String>,
        id: impl Into<String>,
        amount: f64,
        operation: Operation,
    ) -> Self {
        self.definition
            .attribute_modifiers
            .push(MobEffectModifierDefinition {
                attribute: attribute.into(),
                id: id.into(),
                amount,
                operation,
            });
        self
    }

    /// Sets the behaviour that ticks the effect. Without it the effect only shows and applies
    /// its modifiers.
    #[must_use]
    pub fn behaviour(mut self, behaviour: Arc<dyn MobEffect>) -> Self {
        self.behaviour = Some(behaviour);
        self
    }

    fn register(self) -> Result<(), RegistryError> {
        custom::register_mob_effect(self.definition, self.behaviour)
    }
}

impl fmt::Debug for MobEffectBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MobEffectBuilder")
            .field("definition", &self.definition)
            .field("behaviour", &self.behaviour.is_some())
            .finish()
    }
}

/// The effect named `name`: a vanilla effect such as `minecraft:speed`, or a mod effect after the
/// freeze. A name without a namespace is in `minecraft`.
#[must_use]
pub fn effect(name: &str) -> Option<&'static StatusEffect> {
    StatusEffect::from_minecraft_name(&pumpkin_data::dynamic::namespaced_name(name))
}

/// An instance of `effect` for `duration` ticks (`-1` lasts forever) at `amplifier`, with
/// particles and an icon. `NeoForge`: `new MobEffectInstance(effect, duration, amplifier)`.
#[must_use]
pub const fn instance(effect: &'static StatusEffect, duration: i32, amplifier: u8) -> Effect {
    Effect {
        effect_type: effect,
        duration,
        amplifier,
        ambient: false,
        show_particles: true,
        show_icon: true,
        blend: false,
    }
}

/// Applies the effect to the entity, replacing an instance of the same effect.
/// `NeoForge`: `LivingEntity#addEffect`.
pub fn add_effect(entity: &LivingEntity, effect: Effect) {
    entity.add_effect(effect);
}

/// Whether the entity has the effect. `NeoForge`: `LivingEntity#hasEffect`.
#[must_use]
pub fn has_effect(entity: &LivingEntity, effect: &'static StatusEffect) -> bool {
    entity.has_effect(effect)
}

/// The instance of the effect on the entity. `NeoForge`: `LivingEntity#getEffect`.
#[must_use]
pub fn get_effect(entity: &LivingEntity, effect: &'static StatusEffect) -> Option<Effect> {
    entity.get_effect(effect)
}

/// Removes the effect from the entity and returns whether it had it.
/// `NeoForge`: `LivingEntity#removeEffect`.
pub fn remove_effect(entity: &LivingEntity, effect: &'static StatusEffect) -> bool {
    entity.remove_effect(effect)
}

impl ModInit {
    /// Registers a mob effect and its behaviour. It applies at once, like
    /// [`ModInit::register_block`], and gets its id at the freeze.
    ///
    /// # Errors
    ///
    /// Returns the [`RegistryError`] of the registry: the name is invalid, uses the `minecraft`
    /// namespace or is already registered, a modifier amount is not finite, or the registry is
    /// frozen. An unknown modifier attribute fails the freeze.
    pub fn register_mob_effect(&mut self, effect: MobEffectBuilder) -> Result<(), RegistryError> {
        let name = effect.definition.name.clone();
        self.keep_content_error(ContentKind::MobEffect, name, effect.register())
    }
}

/// Registration of mob effects from a handler of [`RegisterEvent`]. See
/// [`RegisterEventExt`](crate::content::RegisterEventExt) for the pattern.
pub trait RegisterMobEffect {
    /// Registers a mob effect on the fire of `minecraft:mob_effect`.
    ///
    /// # Errors
    ///
    /// The [`RegistryError`] of the registry.
    fn register_mob_effect(&self, effect: MobEffectBuilder) -> Result<(), RegistryError>;
}

impl RegisterMobEffect for RegisterEvent {
    fn register_mob_effect(&self, effect: MobEffectBuilder) -> Result<(), RegistryError> {
        let name = effect.definition.name.clone();
        self.register(ContentKind::MobEffect, &name, || effect.register())
    }
}
