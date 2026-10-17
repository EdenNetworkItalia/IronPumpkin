//! Sound events, mob effects, attributes and data component types that are not in the generated
//! tables.
//!
//! They work like the blocks, items and entity types of [`crate::dynamic`]. A mod registers an
//! entry before the first world loads. [`crate::dynamic::freeze`] allocates the ids after the
//! generated counts, in name order, and installs immutable tables. The generated lookups
//! (`StatusEffect::from_minecraft_name`, `Attributes::from_id` and the others) fall through to
//! them. Saves name the entries, so the ids are never persisted.
//!
//! The `NeoForge` attributes `neoforge:swim_speed`, `neoforge:creative_flight` and
//! `neoforge:gliding_flight` are always registered. They take the first ids after the generated
//! attributes, in the order of `NeoForgeMod`, as on a `NeoForge` 26.3 server.
//!
//! A vanilla client knows none of these entries. Packet writers leave mod effects, attributes and
//! components out for [`ContentIds::Display`](crate::dynamic::ContentIds). A sound event travels
//! by name, so every client can play it.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock, PoisonError};

use pumpkin_nbt::tag::NbtTag;

#[cfg(any(feature = "effect", feature = "attributes", feature = "data_component"))]
use pumpkin_util::identifier::Identifier;

#[cfg(feature = "attributes")]
use crate::attributes::Attributes;
#[cfg(feature = "data_component")]
use crate::data_component::DataComponent;
#[cfg(feature = "effect")]
use crate::data_component_impl::{IDSetContent, Operation};
#[cfg(any(feature = "effect", feature = "attributes", feature = "data_component"))]
use crate::dynamic::ContentTables;
use crate::dynamic::{ContentKind, RegistryError, leak, validate_name};
#[cfg(feature = "effect")]
use crate::effect::{MobEffectCategory, Modifiers, StatusEffect};
#[cfg(feature = "sound")]
use crate::sound::Sound;

/// A sound event of a mod.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundEventDefinition {
    /// Namespaced name, for example `mymod:zap`. The client plays the sounds of this name from its
    /// resource packs.
    pub name: String,
    /// The hearing range in blocks, or `None` for the range that grows with the volume, like
    /// `SoundEvent.createFixedRangeEvent` and `SoundEvent.createVariableRangeEvent`.
    pub fixed_range: Option<f32>,
}

/// A frozen sound event of a mod.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundEventEntry {
    pub id: u16,
    pub name: &'static str,
    pub fixed_range: Option<f32>,
}

/// A mob effect of a mod.
#[cfg(feature = "effect")]
#[derive(Debug, Clone)]
pub struct MobEffectDefinition {
    /// Namespaced name, for example `mymod:shock`.
    pub name: String,
    pub category: MobEffectCategory,
    /// The packed RGB colour of the particles.
    pub color: i32,
    /// The modifiers that the effect adds while it lasts, like `MobEffect#addAttributeModifier`.
    pub attribute_modifiers: Vec<MobEffectModifierDefinition>,
}

/// An attribute modifier of a mob effect of a mod.
#[cfg(feature = "effect")]
#[derive(Debug, Clone, PartialEq)]
pub struct MobEffectModifierDefinition {
    /// The attribute: a vanilla, `NeoForge` or mod attribute. A mod attribute resolves at the
    /// freeze.
    pub attribute: String,
    /// The modifier id, unique on the attribute.
    pub id: String,
    /// The amount at amplifier 0. The effect multiplies it by the amplifier plus one.
    pub amount: f64,
    pub operation: Operation,
}

/// What an attribute does with a value, like the attribute classes of vanilla and `NeoForge`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AttributeRange {
    /// Any value, like `Attribute`.
    Unbounded,
    /// A value clamped to `min..=max`, like `RangedAttribute` and the `PercentageAttribute` of
    /// `NeoForge`.
    Ranged { min: f64, max: f64 },
    /// 0 or 1, like the `BooleanAttribute` of `NeoForge`: a value above 0 is 1.
    Boolean,
}

impl AttributeRange {
    /// `Attribute#sanitizeValue` of the attribute class.
    #[must_use]
    pub fn sanitize(self, value: f64) -> f64 {
        match self {
            Self::Unbounded => value,
            Self::Ranged { min, max } => {
                if value.is_nan() {
                    min
                } else {
                    value.clamp(min, max)
                }
            }
            Self::Boolean => {
                if value > 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// An attribute of a mod.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributeDefinition {
    /// Namespaced name, for example `mymod:mana`.
    pub name: String,
    /// The base value of an entity that has no other base value for the attribute.
    pub default_value: f64,
    pub range: AttributeRange,
    /// Whether clients get the value, like `Attribute#setSyncable`. A vanilla client never gets
    /// a mod attribute.
    pub syncable: bool,
}

/// A data component type of a mod. The value codec lives in `ironpumpkin-mods`: item stacks hold
/// the value in its NBT form.
#[derive(Debug, Clone)]
pub struct DataComponentTypeDefinition {
    /// Namespaced name, for example `mymod:charge`.
    pub name: String,
    /// Whether item saves keep the value, like a type built with `persistent(codec)`.
    pub persistent: bool,
    /// Whether a value in NBT form decodes through the codec of the type.
    pub validate: fn(&NbtTag) -> bool,
}

/// A frozen data component type of a mod.
#[derive(Debug, Clone)]
pub struct DataComponentTypeEntry {
    pub id: u16,
    pub name: &'static str,
    pub persistent: bool,
    pub validate: fn(&NbtTag) -> bool,
}

#[cfg(feature = "attributes")]
impl Attributes {
    /// `NeoForgeMod.SWIM_SPEED`: a multiplier of the swim speed of a living entity.
    pub const NEOFORGE_SWIM_SPEED: Self = Self {
        id: Self::COUNT,
        default_value: 1.0,
        name: "neoforge:swim_speed",
    };
    /// `NeoForgeMod.CREATIVE_FLIGHT`: a player with a value above 0 may fly.
    pub const NEOFORGE_CREATIVE_FLIGHT: Self = Self {
        id: Self::COUNT + 1,
        default_value: 0.0,
        name: "neoforge:creative_flight",
    };
    /// `NeoForgeMod.GLIDING_FLIGHT`: a living entity with a value above 0 may glide. An equipped
    /// glider item adds 1, so a `NeoForge` client lets its player glide.
    pub const NEOFORGE_GLIDING_FLIGHT: Self = Self {
        id: Self::COUNT + 2,
        default_value: 0.0,
        name: "neoforge:gliding_flight",
    };
}

/// The attributes of `NeoForgeMod`, in its registration order, with their ranges. All three are
/// syncable.
#[cfg(feature = "attributes")]
pub const NEOFORGE_ATTRIBUTES: [(&Attributes, AttributeRange); 3] = [
    (
        &Attributes::NEOFORGE_SWIM_SPEED,
        AttributeRange::Ranged {
            min: 0.0,
            max: 1024.0,
        },
    ),
    (
        &Attributes::NEOFORGE_CREATIVE_FLIGHT,
        AttributeRange::Boolean,
    ),
    (
        &Attributes::NEOFORGE_GLIDING_FLIGHT,
        AttributeRange::Boolean,
    ),
];

/// The highest id of a u8 registry. Mob effect and attribute ids are u8 in the generated tables.
const MAX_U8_ID: u8 = u8::MAX - 1;
/// The highest id of a u16 registry. `u16::MAX` is the "none" value of u16 ids.
const MAX_U16_ID: u16 = u16::MAX - 1;

struct Pending {
    sound_events: BTreeMap<String, SoundEventDefinition>,
    #[cfg(feature = "effect")]
    mob_effects: BTreeMap<String, MobEffectDefinition>,
    attributes: BTreeMap<String, AttributeDefinition>,
    data_component_types: BTreeMap<String, DataComponentTypeDefinition>,
}

impl Pending {
    const fn new() -> Self {
        Self {
            sound_events: BTreeMap::new(),
            #[cfg(feature = "effect")]
            mob_effects: BTreeMap::new(),
            attributes: BTreeMap::new(),
            data_component_types: BTreeMap::new(),
        }
    }

    fn contains(&self, kind: ContentKind, name: &str) -> bool {
        match kind {
            ContentKind::SoundEvent => self.sound_events.contains_key(name),
            #[cfg(feature = "effect")]
            ContentKind::MobEffect => self.mob_effects.contains_key(name),
            ContentKind::Attribute => self.attributes.contains_key(name),
            ContentKind::DataComponentType => self.data_component_types.contains_key(name),
            _ => false,
        }
    }

    fn build(self) -> Tables {
        let sound_events = Registry::new(self.sound_events.into_values().enumerate().map(
            |(index, definition)| SoundEventEntry {
                id: first_sound_event_id() + index as u16,
                name: leak(definition.name),
                fixed_range: definition.fixed_range,
            },
        ));
        let data_component_types =
            Registry::new(self.data_component_types.into_values().enumerate().map(
                |(index, definition)| DataComponentTypeEntry {
                    id: first_data_component_type_id() + index as u16,
                    name: leak(definition.name),
                    persistent: definition.persistent,
                    validate: definition.validate,
                },
            ));
        #[cfg(feature = "attributes")]
        let attributes = Registry::new(self.attributes.into_values().enumerate().map(
            |(index, definition)| AttributeEntry {
                attribute: Box::leak(Box::new(Attributes {
                    id: first_mod_attribute_id() + index as u8,
                    default_value: definition.default_value,
                    name: leak(definition.name),
                })),
                range: definition.range,
                syncable: definition.syncable,
            },
        ));
        Tables {
            sound_events,
            #[cfg(feature = "effect")]
            mob_effects: Registry::new(
                self.mob_effects
                    .into_values()
                    .enumerate()
                    .map(|(index, definition)| mob_effect_from(definition, index, &attributes)),
            ),
            #[cfg(feature = "attributes")]
            attributes,
            data_component_types,
        }
    }

    /// Checks the references that resolve at the freeze and the id ranges.
    fn check(&self) -> Result<(), RegistryError> {
        check_u16_range(
            "sound_event",
            first_sound_event_id(),
            self.sound_events.len(),
        )?;
        check_u16_range(
            "data_component_type",
            first_data_component_type_id(),
            self.data_component_types.len(),
        )?;
        #[cfg(feature = "attributes")]
        check_u8_range("attribute", first_mod_attribute_id(), self.attributes.len())?;
        #[cfg(feature = "effect")]
        {
            check_u8_range("mob_effect", StatusEffect::COUNT, self.mob_effects.len())?;
            for definition in self.mob_effects.values() {
                for modifier in &definition.attribute_modifiers {
                    let known = Attributes::from_name(&modifier.attribute).is_some()
                        || self.attributes.contains_key(&modifier.attribute);
                    if !known {
                        return Err(RegistryError::UnknownAttribute {
                            effect: definition.name.clone(),
                            attribute: modifier.attribute.clone(),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

/// The entries of one registry in id order, with a name index.
struct Registry<T> {
    entries: Vec<T>,
    names: HashMap<&'static str, usize>,
}

impl<T: Named> Registry<T> {
    fn new(entries: impl Iterator<Item = T>) -> Self {
        let entries: Vec<T> = entries.collect();
        let names = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.name(), index))
            .collect();
        Self { entries, names }
    }

    fn get(&self, index: usize) -> Option<&T> {
        self.entries.get(index)
    }

    fn by_name(&self, name: &str) -> Option<&T> {
        self.names
            .get(name)
            .and_then(|&index| self.entries.get(index))
    }
}

trait Named {
    fn name(&self) -> &'static str;
}

impl Named for SoundEventEntry {
    fn name(&self) -> &'static str {
        self.name
    }
}

impl Named for DataComponentTypeEntry {
    fn name(&self) -> &'static str {
        self.name
    }
}

#[cfg(feature = "effect")]
impl Named for StatusEffect {
    fn name(&self) -> &'static str {
        self.minecraft_name
    }
}

/// A frozen attribute of a mod.
#[cfg(feature = "attributes")]
struct AttributeEntry {
    attribute: &'static Attributes,
    range: AttributeRange,
    syncable: bool,
}

#[cfg(feature = "attributes")]
impl Named for AttributeEntry {
    fn name(&self) -> &'static str {
        self.attribute.name
    }
}

struct Tables {
    sound_events: Registry<SoundEventEntry>,
    #[cfg(feature = "effect")]
    mob_effects: Registry<StatusEffect>,
    #[cfg(feature = "attributes")]
    attributes: Registry<AttributeEntry>,
    data_component_types: Registry<DataComponentTypeEntry>,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(Some(Pending::new()));
static TABLES: OnceLock<Tables> = OnceLock::new();

fn with_pending(
    kind: ContentKind,
    name: &str,
    register: impl FnOnce(&mut Pending) -> Result<(), RegistryError>,
) -> Result<(), RegistryError> {
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    let pending = pending.as_mut().ok_or(RegistryError::RegistryFrozen)?;
    validate_name(kind, name)?;
    if pending.contains(kind, name) || is_builtin(kind, name) {
        return Err(RegistryError::Duplicate {
            kind,
            name: name.to_owned(),
        });
    }
    register(pending)
}

/// Whether `name` is an entry that `IronPumpkin` registers itself.
fn is_builtin(kind: ContentKind, name: &str) -> bool {
    #[cfg(feature = "attributes")]
    if kind == ContentKind::Attribute {
        return NEOFORGE_ATTRIBUTES
            .iter()
            .any(|(attribute, _)| attribute.name == name);
    }
    let _ = (kind, name);
    false
}

fn invalid(kind: ContentKind, name: &str, reason: &'static str) -> RegistryError {
    RegistryError::InvalidDefinition {
        kind,
        name: name.to_owned(),
        reason,
    }
}

/// Registers a sound event. Its id is allocated by [`crate::dynamic::freeze`].
///
/// # Errors
///
/// The name is invalid, uses the `minecraft` namespace or is registered, the fixed range is not
/// positive and finite, or the registry is frozen.
pub fn register_sound_event(definition: SoundEventDefinition) -> Result<(), RegistryError> {
    let kind = ContentKind::SoundEvent;
    if definition
        .fixed_range
        .is_some_and(|range| !range.is_finite() || range <= 0.0)
    {
        return Err(invalid(
            kind,
            &definition.name,
            "the fixed range must be positive and finite",
        ));
    }
    let name = definition.name.clone();
    with_pending(kind, &name, |pending| {
        pending.sound_events.insert(name.clone(), definition);
        Ok(())
    })
}

/// Registers a mob effect. Its id is allocated by [`crate::dynamic::freeze`], which also fails
/// when a modifier names an attribute that nobody registers.
///
/// # Errors
///
/// The name is invalid, uses the `minecraft` namespace or is registered, a modifier amount is not
/// finite, or the registry is frozen.
#[cfg(feature = "effect")]
pub fn register_mob_effect(definition: MobEffectDefinition) -> Result<(), RegistryError> {
    let kind = ContentKind::MobEffect;
    if definition
        .attribute_modifiers
        .iter()
        .any(|modifier| !modifier.amount.is_finite())
    {
        return Err(invalid(
            kind,
            &definition.name,
            "a modifier amount is not finite",
        ));
    }
    let name = definition.name.clone();
    with_pending(kind, &name, |pending| {
        pending.mob_effects.insert(name.clone(), definition);
        Ok(())
    })
}

/// Registers an attribute. Its id is allocated by [`crate::dynamic::freeze`].
///
/// # Errors
///
/// The name is invalid, uses the `minecraft` namespace or is registered (the `NeoForge`
/// attributes are registered), the range or the default value is invalid as `RangedAttribute`
/// and `BooleanAttribute` check them, or the registry is frozen.
pub fn register_attribute(definition: AttributeDefinition) -> Result<(), RegistryError> {
    let kind = ContentKind::Attribute;
    let reason = match definition.range {
        _ if !definition.default_value.is_finite() => Some("the default value is not finite"),
        AttributeRange::Unbounded => None,
        AttributeRange::Ranged { min, max } if !(min.is_finite() && max.is_finite()) => {
            Some("the range is not finite")
        }
        AttributeRange::Ranged { min, max } if min > max => {
            Some("minimum value cannot be bigger than maximum value")
        }
        AttributeRange::Ranged { min, .. } if definition.default_value < min => {
            Some("default value cannot be lower than minimum value")
        }
        AttributeRange::Ranged { max, .. } if definition.default_value > max => {
            Some("default value cannot be bigger than maximum value")
        }
        AttributeRange::Ranged { .. } => None,
        AttributeRange::Boolean
            if definition.default_value != 0.0 && definition.default_value != 1.0 =>
        {
            Some("the default value of a boolean attribute is 0 or 1")
        }
        AttributeRange::Boolean => None,
    };
    if let Some(reason) = reason {
        return Err(invalid(kind, &definition.name, reason));
    }
    let name = definition.name.clone();
    with_pending(kind, &name, |pending| {
        pending.attributes.insert(name.clone(), definition);
        Ok(())
    })
}

/// Registers a data component type. Its id is allocated by [`crate::dynamic::freeze`].
///
/// # Errors
///
/// The name is invalid, uses the `minecraft` namespace or is registered, or the registry is
/// frozen.
pub fn register_data_component_type(
    definition: DataComponentTypeDefinition,
) -> Result<(), RegistryError> {
    let name = definition.name.clone();
    with_pending(ContentKind::DataComponentType, &name, |pending| {
        pending
            .data_component_types
            .insert(name.clone(), definition);
        Ok(())
    })
}

/// Returns whether `name` is registered in this registry of the module and waits for the freeze.
/// Always `false` after the freeze.
#[must_use]
pub fn is_registered(kind: ContentKind, name: &str) -> bool {
    PENDING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .is_some_and(|pending| pending.contains(kind, name))
}

/// Checks the pending entries and installs the tables. [`crate::dynamic::freeze`] calls it once.
pub(crate) fn freeze() -> Result<(), RegistryError> {
    let pending = PENDING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .ok_or(RegistryError::RegistryFrozen)?;
    pending.check()?;
    let _ = TABLES.set(pending.build());
    Ok(())
}

fn check_u8_range(registry: &'static str, first: u8, count: usize) -> Result<(), RegistryError> {
    if count > usize::from(MAX_U8_ID.saturating_sub(first)) + 1 {
        return Err(RegistryError::RangeExceeded { registry, count });
    }
    Ok(())
}

fn check_u16_range(registry: &'static str, first: u16, count: usize) -> Result<(), RegistryError> {
    if count > usize::from(MAX_U16_ID.saturating_sub(first)) + 1 {
        return Err(RegistryError::RangeExceeded { registry, count });
    }
    Ok(())
}

fn first_sound_event_id() -> u16 {
    #[cfg(feature = "sound")]
    {
        Sound::slice().len() as u16
    }
    #[cfg(not(feature = "sound"))]
    {
        MAX_U16_ID
    }
}

fn first_data_component_type_id() -> u16 {
    #[cfg(feature = "data_component")]
    {
        DataComponent::COUNT
    }
    #[cfg(not(feature = "data_component"))]
    {
        MAX_U16_ID
    }
}

#[cfg(feature = "attributes")]
const fn first_mod_attribute_id() -> u8 {
    Attributes::COUNT + NEOFORGE_ATTRIBUTES.len() as u8
}

/// The frozen sound event with this name, or `None`.
#[must_use]
pub fn sound_event_by_name(name: &str) -> Option<&'static SoundEventEntry> {
    TABLES.get()?.sound_events.by_name(name)
}

/// The frozen sound events of mods, in id order.
#[must_use]
pub fn sound_events() -> &'static [SoundEventEntry] {
    TABLES
        .get()
        .map_or(&[], |tables| &tables.sound_events.entries)
}

/// The frozen data component type with this id, or `None`.
#[must_use]
pub fn data_component_type(id: u16) -> Option<&'static DataComponentTypeEntry> {
    let index = id.checked_sub(first_data_component_type_id())?;
    TABLES.get()?.data_component_types.get(usize::from(index))
}

/// The frozen data component type with this name, or `None`.
#[must_use]
pub fn data_component_type_by_name(name: &str) -> Option<&'static DataComponentTypeEntry> {
    TABLES.get()?.data_component_types.by_name(name)
}

/// The frozen data component types of mods, in id order.
#[must_use]
pub fn data_component_types() -> &'static [DataComponentTypeEntry] {
    TABLES
        .get()
        .map_or(&[], |tables| &tables.data_component_types.entries)
}

#[cfg(feature = "effect")]
fn mob_effect_from(
    definition: MobEffectDefinition,
    index: usize,
    attributes: &Registry<AttributeEntry>,
) -> StatusEffect {
    let modifiers: Vec<Modifiers> = definition
        .attribute_modifiers
        .into_iter()
        .filter_map(|modifier| {
            // `Pending::check` resolved every attribute.
            let attribute = Attributes::from_name(&modifier.attribute)
                .or_else(|| attributes.by_name(&modifier.attribute).map(|e| e.attribute))?;
            Some(Modifiers {
                attribute,
                id: leak(modifier.id),
                base_value: modifier.amount,
                operation: modifier.operation,
            })
        })
        .collect();
    // `Util.makeDescriptionId("effect", id)`.
    let translation_key = format!("effect.{}", definition.name.replace([':', '/'], "."));
    StatusEffect {
        minecraft_name: leak(definition.name),
        id: StatusEffect::COUNT + index as u8,
        category: definition.category,
        color: definition.color,
        translation_key: leak(translation_key),
        attribute_modifiers: Box::leak(modifiers.into_boxed_slice()),
    }
}

/// The frozen mob effect with this id, or `None`. `StatusEffect`'s `IDSetContent::from_id`
/// falls through to it.
#[cfg(feature = "effect")]
#[must_use]
pub fn mob_effect(id: u16) -> Option<&'static StatusEffect> {
    let index = id.checked_sub(u16::from(StatusEffect::COUNT))?;
    TABLES.get()?.mob_effects.get(usize::from(index))
}

/// The frozen mob effect with this namespaced name, or `None`.
/// `StatusEffect::from_minecraft_name` falls through to it.
#[cfg(feature = "effect")]
#[must_use]
pub fn mob_effect_by_name(name: &str) -> Option<&'static StatusEffect> {
    TABLES.get()?.mob_effects.by_name(name)
}

/// The frozen mob effects of mods, in id order.
#[cfg(feature = "effect")]
#[must_use]
pub fn mob_effects() -> &'static [StatusEffect] {
    TABLES
        .get()
        .map_or(&[], |tables| &tables.mob_effects.entries)
}

/// Whether the effect is not a generated one: a vanilla client does not know it.
#[cfg(feature = "effect")]
#[must_use]
pub const fn is_custom_mob_effect(effect: &StatusEffect) -> bool {
    effect.id >= StatusEffect::COUNT
}

/// The attribute with this id after the generated ones: a `NeoForge` attribute, or a frozen mod
/// attribute. `Attributes::from_id` falls through to it.
#[cfg(feature = "attributes")]
#[must_use]
pub fn attribute(id: u8) -> Option<&'static Attributes> {
    let index = usize::from(id.checked_sub(Attributes::COUNT)?);
    if let Some((attribute, _)) = NEOFORGE_ATTRIBUTES.get(index) {
        return Some(attribute);
    }
    let index = index - NEOFORGE_ATTRIBUTES.len();
    TABLES
        .get()?
        .attributes
        .get(index)
        .map(|entry| entry.attribute)
}

/// The `NeoForge` or frozen mod attribute with this namespaced name.
/// `Attributes::from_name` falls through to it.
#[cfg(feature = "attributes")]
#[must_use]
pub fn attribute_by_name(name: &str) -> Option<&'static Attributes> {
    NEOFORGE_ATTRIBUTES
        .iter()
        .map(|&(attribute, _)| attribute)
        .find(|attribute| attribute.name == name)
        .or_else(|| {
            TABLES
                .get()?
                .attributes
                .by_name(name)
                .map(|entry| entry.attribute)
        })
}

/// The attributes after the generated ones, in id order: the `NeoForge` attributes, then the
/// frozen mod attributes.
#[cfg(feature = "attributes")]
#[must_use]
pub fn attributes() -> Vec<&'static Attributes> {
    let mods = TABLES
        .get()
        .map_or(&[][..], |tables| &tables.attributes.entries);
    NEOFORGE_ATTRIBUTES
        .iter()
        .map(|&(attribute, _)| attribute)
        .chain(mods.iter().map(|entry| entry.attribute))
        .collect()
}

/// Whether the attribute is not a generated one: a vanilla client does not know it.
#[cfg(feature = "attributes")]
#[must_use]
pub const fn is_custom_attribute(attribute: &Attributes) -> bool {
    attribute.id >= Attributes::COUNT
}

/// The range of an attribute. Generated attributes are [`AttributeRange::Unbounded`]: the
/// extracted data has no ranges.
#[cfg(feature = "attributes")]
#[must_use]
pub fn attribute_range(attribute: &Attributes) -> AttributeRange {
    let Some(index) = attribute.id.checked_sub(Attributes::COUNT) else {
        return AttributeRange::Unbounded;
    };
    let index = usize::from(index);
    if let Some(&(_, range)) = NEOFORGE_ATTRIBUTES.get(index) {
        return range;
    }
    TABLES
        .get()
        .and_then(|tables| tables.attributes.get(index - NEOFORGE_ATTRIBUTES.len()))
        .map_or(AttributeRange::Unbounded, |entry| entry.range)
}

/// Whether clients get the value of the attribute. Generated attributes answer `true`: the server
/// sends the ones it changes.
#[cfg(feature = "attributes")]
#[must_use]
pub fn is_attribute_syncable(attribute: &Attributes) -> bool {
    let Some(index) = attribute.id.checked_sub(Attributes::COUNT) else {
        return true;
    };
    let index = usize::from(index);
    if index < NEOFORGE_ATTRIBUTES.len() {
        return true;
    }
    TABLES
        .get()
        .and_then(|tables| tables.attributes.get(index - NEOFORGE_ATTRIBUTES.len()))
        .is_some_and(|entry| entry.syncable)
}

/// The snapshots of the `NeoForge` registry sync. They read the tables of this module: the
/// content tables exist only after a successful freeze, which installs both.
impl ContentTables {
    /// Every mob effect id with its namespaced name: the generated effects, then the mod effects.
    /// This is the `minecraft:mob_effect` snapshot of the `NeoForge` registry sync.
    #[cfg(feature = "effect")]
    #[must_use]
    pub fn mob_effect_snapshot(&self) -> Vec<(i32, Identifier)> {
        (0..u16::from(StatusEffect::COUNT))
            .filter_map(<StatusEffect as IDSetContent>::from_id)
            .chain(mob_effects())
            .map(|effect| {
                (
                    i32::from(effect.id),
                    Identifier::parse_static(effect.minecraft_name),
                )
            })
            .collect()
    }

    /// Every attribute id with its namespaced name: the generated attributes, the `NeoForge`
    /// attributes, then the mod attributes. This is the `minecraft:attribute` snapshot.
    #[cfg(feature = "attributes")]
    #[must_use]
    pub fn attribute_snapshot(&self) -> Vec<(i32, Identifier)> {
        Attributes::ALL
            .iter()
            .chain(attributes())
            .map(|attribute| {
                (
                    i32::from(attribute.id),
                    Identifier::parse_static(attribute.name),
                )
            })
            .collect()
    }

    /// Every data component type id with its namespaced name: the generated types, then the mod
    /// types. This is the `minecraft:data_component_type` snapshot.
    #[cfg(feature = "data_component")]
    #[must_use]
    pub fn data_component_type_snapshot(&self) -> Vec<(i32, Identifier)> {
        (0..DataComponent::COUNT)
            .filter_map(|id| DataComponent::try_from_id(u8::try_from(id).ok()?))
            .map(|component| {
                (
                    i32::from(component.to_id()),
                    Identifier::parse_static(component.to_name()),
                )
            })
            .chain(
                data_component_types()
                    .iter()
                    .map(|entry| (i32::from(entry.id), Identifier::parse_static(entry.name))),
            )
            .collect()
    }
}
