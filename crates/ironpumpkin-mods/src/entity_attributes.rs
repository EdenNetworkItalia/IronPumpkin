//! Attributes of mods, the `NeoForge` attributes, and the attributes of living entities.
//!
//! `NeoForge`: an `Attribute` (`RangedAttribute`, `PercentageAttribute`, `BooleanAttribute`)
//! registered in `Registries.ATTRIBUTE`, and the `AttributeInstance` that
//! `LivingEntity#getAttribute` returns: base value, modifiers and value.
//!
//! The functions answer for vanilla, `NeoForge` and mod attributes alike. An entity whose type
//! has no base value for an attribute starts from the default value of the attribute. A change of
//! a syncable attribute goes to the clients that know it: a vanilla client gets the vanilla
//! attributes only. `IronPumpkin` does not save attributes, so a base value or modifier set here
//! lasts until the entity unloads.
//!
//! `IronPumpkin` registers the `NeoForge` attributes [`SWIM_SPEED`], [`CREATIVE_FLIGHT`] and
//! [`GLIDING_FLIGHT`] with the defaults of `NeoForgeMod`. The swim speed scales the swim speed of
//! a mob in water and its upward push when it jumps in a fluid; the player's own movement is
//! client authoritative, so a `NeoForge` client applies it for the player. Creative flight lets a
//! survival player fly, also on a vanilla client: the server sends it as the may-fly ability.
//! Gliding flight is registered for the registry sync; nothing reads it yet.
//!
//! ```
//! use ironpumpkin_mods::{
//!     ModInit,
//!     entity::living::LivingEntity,
//!     entity_attributes::{self, AttributeBuilder, AttributeModifier, ModifierOperation},
//! };
//!
//! fn register(cx: &mut ModInit) {
//!     let _ = cx.register_attribute(AttributeBuilder::ranged("examplemod:mana", 20.0, 0.0, 1000.0));
//! }
//!
//! fn grant_flight(player: &LivingEntity) {
//!     entity_attributes::add_modifier(
//!         player,
//!         entity_attributes::CREATIVE_FLIGHT,
//!         AttributeModifier {
//!             id: "examplemod:wings".to_owned(),
//!             amount: 1.0,
//!             operation: ModifierOperation::Add,
//!         },
//!     );
//! }
//! ```

use pumpkin_core::{
    entity::{attributes::send_attribute_updates_for_living, living::LivingEntity},
    plugin::api::events::registries::RegisterEvent,
};
use pumpkin_data::{
    dynamic::{ContentKind, RegistryError},
    dynamic_registries::{self, AttributeDefinition},
};

pub use pumpkin_core::entity::attributes::{Modifier as AttributeModifier, ModifierOperation};
pub use pumpkin_data::attributes::Attributes;
pub use pumpkin_data::dynamic_registries::AttributeRange;

use crate::ModInit;

/// `NeoForgeMod.SWIM_SPEED`: a percentage attribute from 0 to 1024, default 1.
pub const SWIM_SPEED: &Attributes = &Attributes::NEOFORGE_SWIM_SPEED;
/// `NeoForgeMod.CREATIVE_FLIGHT`: a boolean attribute, default 0.
pub const CREATIVE_FLIGHT: &Attributes = &Attributes::NEOFORGE_CREATIVE_FLIGHT;
/// `NeoForgeMod.GLIDING_FLIGHT`: a boolean attribute, default 0.
pub const GLIDING_FLIGHT: &Attributes = &Attributes::NEOFORGE_GLIDING_FLIGHT;

/// An attribute of a mod. It is not syncable unless [`AttributeBuilder::syncable`] says so, like
/// a vanilla `Attribute`.
#[derive(Debug, Clone)]
pub struct AttributeBuilder(AttributeDefinition);

impl AttributeBuilder {
    /// An attribute without a range. `NeoForge`: a plain `Attribute`.
    #[must_use]
    pub fn new(name: impl Into<String>, default_value: f64) -> Self {
        Self::with_range(name, default_value, AttributeRange::Unbounded)
    }

    /// An attribute clamped to `min..=max`. `NeoForge`: `RangedAttribute` and
    /// `PercentageAttribute`. The registry rejects a default outside the range.
    #[must_use]
    pub fn ranged(name: impl Into<String>, default_value: f64, min: f64, max: f64) -> Self {
        Self::with_range(name, default_value, AttributeRange::Ranged { min, max })
    }

    /// An attribute that is on (1) or off (0). `NeoForge`: `BooleanAttribute`.
    #[must_use]
    pub fn boolean(name: impl Into<String>, default_value: bool) -> Self {
        Self::with_range(
            name,
            f64::from(u8::from(default_value)),
            AttributeRange::Boolean,
        )
    }

    fn with_range(name: impl Into<String>, default_value: f64, range: AttributeRange) -> Self {
        Self(AttributeDefinition {
            name: name.into(),
            default_value,
            range,
            syncable: false,
        })
    }

    /// Whether clients get the value. `NeoForge`: `Attribute#setSyncable`.
    #[must_use]
    pub const fn syncable(mut self, syncable: bool) -> Self {
        self.0.syncable = syncable;
        self
    }
}

/// The attribute named `name`: a vanilla, `NeoForge` or mod attribute. A name without a namespace
/// is in `minecraft`.
#[must_use]
pub fn attribute(name: &str) -> Option<&'static Attributes> {
    Attributes::from_name(name)
}

/// The value of the attribute on the entity: the base value with the modifiers, clamped to the
/// range of the attribute. `NeoForge`: `LivingEntity#getAttributeValue`.
#[must_use]
pub fn value(entity: &LivingEntity, attribute: &Attributes) -> f64 {
    entity.get_attribute_value(attribute)
}

/// The base value of the attribute on the entity. `NeoForge`: `AttributeInstance#getBaseValue`.
#[must_use]
pub fn base_value(entity: &LivingEntity, attribute: &Attributes) -> f64 {
    entity.get_attribute_base(attribute)
}

/// Sets the base value of the attribute on the entity. `NeoForge`:
/// `AttributeInstance#setBaseValue`.
pub fn set_base_value(entity: &LivingEntity, attribute: &Attributes, base_value: f64) {
    entity.set_attribute_base(attribute, base_value);
    send_attribute_updates_for_living(entity, vec![attribute.clone()]);
}

/// The modifiers of the attribute on the entity. `NeoForge`: `AttributeInstance#getModifiers`.
#[must_use]
pub fn modifiers(entity: &LivingEntity, attribute: &Attributes) -> Vec<AttributeModifier> {
    entity
        .attributes
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&attribute.id)
        .map_or_else(Vec::new, |instance| instance.modifiers.clone())
}

/// Adds the modifier to the attribute on the entity, replacing a modifier with the same id.
/// `NeoForge`: `AttributeInstance#addOrReplacePermanentModifier`.
pub fn add_modifier(entity: &LivingEntity, attribute: &Attributes, modifier: AttributeModifier) {
    entity.update_attribute(attribute, |instance| {
        instance.add_or_replace_modifier(modifier);
    });
    send_attribute_updates_for_living(entity, vec![attribute.clone()]);
}

/// Removes the modifier with this id from the attribute on the entity and returns whether it
/// had one. `NeoForge`: `AttributeInstance#removeModifier`.
pub fn remove_modifier(entity: &LivingEntity, attribute: &Attributes, id: &str) -> bool {
    if !modifiers(entity, attribute)
        .iter()
        .any(|modifier| modifier.id == id)
    {
        return false;
    }
    entity.update_attribute(attribute, |instance| instance.remove_modifier(id));
    send_attribute_updates_for_living(entity, vec![attribute.clone()]);
    true
}

impl ModInit {
    /// Registers an attribute. It applies at once, like [`ModInit::register_block`], and gets
    /// its id at the freeze.
    ///
    /// # Errors
    ///
    /// Returns the [`RegistryError`] of the registry: the name is invalid, uses the `minecraft`
    /// namespace or is already registered (the `NeoForge` attributes are), the range or the
    /// default value is invalid, or the registry is frozen.
    pub fn register_attribute(&mut self, attribute: AttributeBuilder) -> Result<(), RegistryError> {
        let name = attribute.0.name.clone();
        self.keep_content_error(
            ContentKind::Attribute,
            name,
            dynamic_registries::register_attribute(attribute.0),
        )
    }
}

/// Registration of attributes from a handler of [`RegisterEvent`]. See
/// [`RegisterEventExt`](crate::content::RegisterEventExt) for the pattern.
pub trait RegisterAttribute {
    /// Registers an attribute on the fire of `minecraft:attribute`.
    ///
    /// # Errors
    ///
    /// The [`RegistryError`] of the registry.
    fn register_attribute(&self, attribute: AttributeBuilder) -> Result<(), RegistryError>;
}

impl RegisterAttribute for RegisterEvent {
    fn register_attribute(&self, attribute: AttributeBuilder) -> Result<(), RegistryError> {
        let name = attribute.0.name.clone();
        self.register(ContentKind::Attribute, &name, || {
            dynamic_registries::register_attribute(attribute.0)
        })
    }
}
