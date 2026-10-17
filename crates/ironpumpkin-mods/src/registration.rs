//! The `NeoForge` registration events that follow `RegisterEvent`.
//!
//! Each event is a method of [`NativeMod`](crate::NativeMod) that gets a handle, as each
//! `NeoForge` mod gets the event on its own mod bus. The server calls the method of every mod in
//! load order, at the position of `NeoForge` 26.3 (see [`lifecycle`](crate::lifecycle)):
//!
//! 1. After the last `RegisterEvent`, in `GameData.postRegisterEvents`:
//!    [`EntityAttributeCreationEvent`], [`EntityAttributeModificationEvent`],
//!    [`RegisterSpawnPlacementsEvent`] and [`BlockEntityTypeAddBlocksEvent`].
//! 2. After `dedicated_server_setup`, in `RegistrationEvents.init`: [`ExtendPoiTypesEvent`].
//!
//! Every event fires before the freeze of the content registry, so a mod names entries by
//! `namespace:path`, as `registry` does: a mod entry has no id yet. A name without a namespace is
//! in `minecraft`. The server applies a registration when the method returns, and it takes effect
//! from the freeze on.
//!
//! A call that `NeoForge` rejects with an exception, or that names an unknown entry, returns a
//! [`RegistrationError`]. The handle keeps the first one, so the `Result` can be ignored: the
//! server logs it with the mod id and stops after the step, before the first world loads. A valid
//! `NeoForge` call that the server cannot back logs a warning and changes nothing.
//!
//! ```
//! use ironpumpkin_mods::{
//!     ModInit, NativeMod,
//!     pumpkin_data::{attributes::Attributes, entity::EntityType},
//!     registration::{AttributeSupplier, EntityAttributeCreationEvent},
//! };
//!
//! struct GolemMod;
//!
//! impl NativeMod for GolemMod {
//!     fn id(&self) -> &'static str {
//!         "golems"
//!     }
//!     fn display_name(&self) -> &'static str {
//!         "Golems"
//!     }
//!     fn version(&self) -> &'static str {
//!         "1.0.0"
//!     }
//!     fn init(&self, _cx: &mut ModInit) {}
//!     fn entity_attribute_creation(&self, event: &mut EntityAttributeCreationEvent) {
//!         // `Zombie.createAttributes()` with more health. The handle keeps an error.
//!         let _ = event.put(
//!             "golems:tin_golem",
//!             AttributeSupplier::of(&EntityType::ZOMBIE).add(Attributes::MAX_HEALTH, 40.0),
//!         );
//!     }
//! }
//! ```

use std::{fmt, sync::Arc};

use pumpkin_core::{
    content::{attributes, poi, spawn_placements},
    world::World,
};
use pumpkin_data::{
    Block, BlockId,
    block_properties::BLOCK_ENTITY_TYPES,
    dynamic::{self, ContentKind},
    dynamic_types,
    entity::EntityType,
};
use pumpkin_util::{identifier::Identifier, math::position::BlockPos};
use tracing::warn;

pub use pumpkin_core::content::spawn_placements::SpawnPlacementOperation as Operation;
pub use pumpkin_data::{attributes::Attributes, entity::SpawnLocation};
pub use pumpkin_util::HeightMap;

pub use crate::content::RegistryError;

/// `SpawnPlacements.SpawnPredicate`: whether an entity of the type may spawn at the position.
pub type SpawnPredicate = spawn_placements::SpawnPredicate;

/// A call of a registration event that `NeoForge` rejects or that names an unknown entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrationError {
    /// No vanilla or mod entity type has this name.
    UnknownEntityType(String),
    /// No vanilla or mod block has this name.
    UnknownBlock(String),
    /// No vanilla block entity type has this name.
    UnknownBlockEntityType(String),
    /// No vanilla POI type has this name.
    UnknownPoiType(String),
    /// The entity type already has default attributes: every vanilla type with attributes, or a
    /// mod type that got them before. `NeoForge`: "Duplicate `DefaultAttributes` entry".
    DuplicateAttributes(String),
    /// The entity type has no spawn placement yet, so the registration must name the placement
    /// type and the heightmap.
    MissingSpawnPlacement(String),
    /// A placement type or heightmap with an operation other than [`Operation::Replace`].
    SpawnPlacementWithoutReplace(String),
    /// The block is already in a POI type. `NeoForge`: "is defined in more than one `PoI` type".
    DuplicatePoiBlock { block: String, poi_type: String },
    /// The content registry refused the registration, for example after the freeze.
    Registry(RegistryError),
}

impl fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEntityType(name) => write!(f, "unknown entity type \"{name}\""),
            Self::UnknownBlock(name) => write!(f, "unknown block \"{name}\""),
            Self::UnknownBlockEntityType(name) => {
                write!(f, "unknown block entity type \"{name}\"")
            }
            Self::UnknownPoiType(name) => write!(f, "unknown POI type \"{name}\""),
            Self::DuplicateAttributes(name) => {
                write!(f, "entity type \"{name}\" already has default attributes")
            }
            Self::MissingSpawnPlacement(name) => write!(
                f,
                "entity type \"{name}\" has no spawn placement yet: name its placement type and heightmap"
            ),
            Self::SpawnPlacementWithoutReplace(name) => write!(
                f,
                "the spawn placement of entity type \"{name}\" changes its placement type or heightmap only with Operation::Replace"
            ),
            Self::DuplicatePoiBlock { block, poi_type } => write!(
                f,
                "block \"{block}\" is already in the POI type \"{poi_type}\""
            ),
            Self::Registry(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for RegistrationError {}

/// The first error of a handle, which the server reads when the method returns.
#[derive(Debug, Default)]
struct FirstError(Option<RegistrationError>);

impl FirstError {
    fn keep<T>(&mut self, result: Result<T, RegistrationError>) -> Result<T, RegistrationError> {
        if let Err(error) = &result {
            self.0.get_or_insert_with(|| error.clone());
        }
        result
    }
}

/// The handles of the registration events, for [`lifecycle`](crate::lifecycle).
///
/// Only [`lifecycle`](crate::lifecycle) builds a handle, as the constructors of the `NeoForge`
/// events are internal: a handle outside the lifecycle position would drop its error.
///
/// ```compile_fail
/// let _ = ironpumpkin_mods::registration::EntityAttributeCreationEvent::default();
/// ```
/// ```compile_fail
/// let _ = ironpumpkin_mods::registration::EntityAttributeModificationEvent::default();
/// ```
/// ```compile_fail
/// let _ = ironpumpkin_mods::registration::RegisterSpawnPlacementsEvent::default();
/// ```
/// ```compile_fail
/// let _ = ironpumpkin_mods::registration::BlockEntityTypeAddBlocksEvent::default();
/// ```
/// ```compile_fail
/// let _ = ironpumpkin_mods::registration::ExtendPoiTypesEvent::default();
/// ```
pub(crate) trait RegistrationEvent {
    /// The `NeoForge` class name, for the error message.
    const NAME: &'static str;

    fn new() -> Self;

    fn take_error(&mut self) -> Option<RegistrationError>;
}

macro_rules! registration_event {
    ($event:ident) => {
        impl RegistrationEvent for $event {
            const NAME: &'static str = stringify!($event);

            fn new() -> Self {
                Self {
                    error: FirstError::default(),
                }
            }

            fn take_error(&mut self) -> Option<RegistrationError> {
                self.error.0.take()
            }
        }
    };
}

/// A vanilla or mod entity type name, with the `minecraft` namespace when it has none, and
/// whether the type is vanilla.
fn entity_type(name: &str) -> Result<(String, bool), RegistrationError> {
    let name = namespaced(name);
    if let Some(entity_type) = EntityType::from_name(&name)
        && entity_type.id < EntityType::COUNT
    {
        return Ok((name, true));
    }
    if dynamic::is_registered(ContentKind::EntityType, &name) {
        return Ok((name, false));
    }
    Err(RegistrationError::UnknownEntityType(name))
}

/// A vanilla or mod block name, with the `minecraft` namespace when it has none, and the vanilla
/// block when it is one.
fn block(name: &str) -> Result<(String, Option<&'static Block>), RegistrationError> {
    let name = namespaced(name);
    if let Some(block) = Block::from_name(&name).filter(|block| block.id.as_u16() < BlockId::COUNT)
    {
        return Ok((name, Some(block)));
    }
    if dynamic::is_registered(ContentKind::Block, &name) {
        return Ok((name, None));
    }
    Err(RegistrationError::UnknownBlock(name))
}

fn namespaced(name: &str) -> String {
    let name = name.strip_prefix('#').unwrap_or(name);
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

/// `AttributeSupplier`: the default attributes of an entity type and their base values.
#[derive(Debug, Clone, Default)]
pub struct AttributeSupplier(Vec<(Attributes, f64)>);

impl AttributeSupplier {
    /// An empty set, as `AttributeSupplier.builder()`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The default attributes of a vanilla entity type, the set its `createAttributes()` builds,
    /// to extend with [`add`](Self::add).
    #[must_use]
    pub fn of(entity_type: &EntityType) -> Self {
        Self(entity_type.attributes.to_vec())
    }

    /// Adds the attribute with this base value, or replaces its value.
    #[must_use]
    pub fn add(mut self, attribute: Attributes, value: f64) -> Self {
        match self.0.iter_mut().find(|(a, _)| *a == attribute) {
            Some(entry) => entry.1 = value,
            None => self.0.push((attribute, value)),
        }
        self
    }

    /// Adds the attribute with its default value.
    #[must_use]
    pub fn add_default(self, attribute: Attributes) -> Self {
        let value = attribute.default_value;
        self.add(attribute, value)
    }
}

/// `EntityAttributeCreationEvent`: sets the default attributes of a mod entity type.
#[derive(Debug)]
pub struct EntityAttributeCreationEvent {
    error: FirstError,
}

registration_event!(EntityAttributeCreationEvent);

impl EntityAttributeCreationEvent {
    /// Sets the default attributes of the mod entity type `entity_type`. An entity of the type
    /// spawns with them, if its factory builds a living entity. Without this call a mod type
    /// keeps the attributes of its display type.
    ///
    /// # Errors
    ///
    /// [`RegistrationError::DuplicateAttributes`] for a vanilla type with attributes or a type
    /// that already got them, [`RegistrationError::UnknownEntityType`] for an unknown name.
    pub fn put(
        &mut self,
        entity_type: &str,
        attributes: AttributeSupplier,
    ) -> Result<(), RegistrationError> {
        let result = put_attributes(entity_type, attributes);
        self.error.keep(result)
    }
}

fn put_attributes(
    entity_type: &str,
    attributes: AttributeSupplier,
) -> Result<(), RegistrationError> {
    let (name, vanilla) = self::entity_type(entity_type)?;
    let duplicate = || RegistrationError::DuplicateAttributes(name.clone());
    if vanilla && EntityType::from_name(&name).is_some_and(|ty| !ty.attributes.is_empty()) {
        return Err(duplicate());
    }
    if attributes::create(&name, attributes.0).map_err(RegistrationError::Registry)? {
        Ok(())
    } else {
        Err(duplicate())
    }
}

/// `EntityAttributeModificationEvent`: adds attributes to entity types, vanilla ones included.
#[derive(Debug)]
pub struct EntityAttributeModificationEvent {
    error: FirstError,
}

registration_event!(EntityAttributeModificationEvent);

impl EntityAttributeModificationEvent {
    /// Adds the attribute with this base value to the default attributes of `entity_type`, or
    /// replaces its value.
    ///
    /// # Errors
    ///
    /// [`RegistrationError::UnknownEntityType`] for an unknown name.
    pub fn add(
        &mut self,
        entity_type: &str,
        attribute: Attributes,
        value: f64,
    ) -> Result<(), RegistrationError> {
        let result = self::entity_type(entity_type).and_then(|(name, _)| {
            attributes::add(&name, attribute, value).map_err(RegistrationError::Registry)
        });
        self.error.keep(result)
    }

    /// Adds the attribute with its default value. See [`add`](Self::add).
    ///
    /// # Errors
    ///
    /// [`RegistrationError::UnknownEntityType`] for an unknown name.
    pub fn add_default(
        &mut self,
        entity_type: &str,
        attribute: Attributes,
    ) -> Result<(), RegistrationError> {
        let value = attribute.default_value;
        self.add(entity_type, attribute, value)
    }

    /// Whether `entity_type` has the attribute by default: from its vanilla data, from
    /// [`EntityAttributeCreationEvent::put`], or from this event. A mod type without `put` answers
    /// only for this event, as it has no supplier in `NeoForge`.
    #[must_use]
    pub fn has(&self, entity_type: &str, attribute: &Attributes) -> bool {
        attributes::has(&namespaced(entity_type), attribute)
    }

    /// The entity types that have default attributes: the vanilla types with attributes and the
    /// mod types of [`EntityAttributeCreationEvent::put`].
    #[must_use]
    pub fn types(&self) -> Vec<Identifier> {
        let mut types: Vec<Identifier> = (0..EntityType::COUNT)
            .filter_map(EntityType::from_raw)
            .filter(|ty| !ty.attributes.is_empty())
            .map(|ty| Identifier::parse_static(ty.resource_name))
            .collect();
        types.extend(
            attributes::created()
                .into_iter()
                .filter_map(|name| Identifier::parse(&name).ok()),
        );
        types
    }
}

/// `RegisterSpawnPlacementsEvent`: the placement type, heightmap and spawn predicate of entity
/// types. The natural spawner reads them for every spawn check of the type.
#[derive(Debug)]
pub struct RegisterSpawnPlacementsEvent {
    error: FirstError,
}

registration_event!(RegisterSpawnPlacementsEvent);

impl RegisterSpawnPlacementsEvent {
    /// Adds `predicate` to the placement of `entity_type` with `operation`, keeping its placement
    /// type and heightmap. A mod type needs [`register_placement`](Self::register_placement)
    /// first.
    ///
    /// # Errors
    ///
    /// [`RegistrationError::MissingSpawnPlacement`] for a type without a placement,
    /// [`RegistrationError::UnknownEntityType`] for an unknown name.
    pub fn register(
        &mut self,
        entity_type: &str,
        predicate: SpawnPredicate,
        operation: Operation,
    ) -> Result<(), RegistrationError> {
        self.register_placement(entity_type, None, None, predicate, operation)
    }

    /// `register(EntityType, SpawnPlacementType, Heightmap.Types, SpawnPredicate, Operation)`.
    /// The first registration of a mod type sets its placement: it needs `placement` and
    /// `heightmap`, and `predicate` becomes the original predicate. Later registrations merge
    /// `predicate` with `operation`; only [`Operation::Replace`] may change the placement type or
    /// the heightmap, and `None` keeps them.
    ///
    /// # Errors
    ///
    /// [`RegistrationError::MissingSpawnPlacement`],
    /// [`RegistrationError::SpawnPlacementWithoutReplace`] and
    /// [`RegistrationError::UnknownEntityType`].
    pub fn register_placement(
        &mut self,
        entity_type: &str,
        placement: Option<SpawnLocation>,
        heightmap: Option<HeightMap>,
        predicate: SpawnPredicate,
        operation: Operation,
    ) -> Result<(), RegistrationError> {
        let result = self::entity_type(entity_type).and_then(|(name, _)| {
            spawn_placements::register(&name, placement, heightmap, predicate, operation).map_err(
                |error| match error {
                    spawn_placements::SpawnPlacementError::MissingPlacement => {
                        RegistrationError::MissingSpawnPlacement(name.clone())
                    }
                    spawn_placements::SpawnPlacementError::PlacementWithoutReplace => {
                        RegistrationError::SpawnPlacementWithoutReplace(name.clone())
                    }
                    spawn_placements::SpawnPlacementError::RegistryFrozen => {
                        RegistrationError::Registry(RegistryError::RegistryFrozen)
                    }
                },
            )
        });
        self.error.keep(result)
    }
}

/// Builds a [`SpawnPredicate`] from a closure.
pub fn spawn_predicate(
    predicate: impl Fn(&'static EntityType, &World, &BlockPos) -> bool + Send + Sync + 'static,
) -> SpawnPredicate {
    Arc::new(predicate)
}

/// `BlockEntityTypeAddBlocksEvent`: adds mod blocks to the valid blocks of a vanilla block entity
/// type.
///
/// Every state of the block then carries the block entity type, so placing the block creates the
/// block entity, which runs the server's behaviour of that type.
#[derive(Debug)]
pub struct BlockEntityTypeAddBlocksEvent {
    error: FirstError,
}

registration_event!(BlockEntityTypeAddBlocksEvent);

impl BlockEntityTypeAddBlocksEvent {
    /// Adds `blocks` to the valid blocks of `block_entity_type`, such as `minecraft:barrel`.
    ///
    /// A block state has one block entity type, so a mod block that already has another one keeps
    /// it, with a warning. A vanilla block keeps its generated block entity type, with a warning
    /// unless it already has this one.
    ///
    /// # Errors
    ///
    /// [`RegistrationError::UnknownBlockEntityType`] and [`RegistrationError::UnknownBlock`].
    pub fn modify(
        &mut self,
        block_entity_type: &str,
        blocks: &[&str],
    ) -> Result<(), RegistrationError> {
        let result = add_blocks(block_entity_type, blocks);
        self.error.keep(result)
    }
}

fn add_blocks(block_entity_type: &str, blocks: &[&str]) -> Result<(), RegistrationError> {
    let name = namespaced(block_entity_type);
    let Some(index) = name
        .strip_prefix("minecraft:")
        .and_then(|path| BLOCK_ENTITY_TYPES.iter().position(|ty| *ty == path))
        .and_then(|index| u16::try_from(index).ok())
    else {
        return Err(RegistrationError::UnknownBlockEntityType(name));
    };
    for block_name in blocks {
        let (block_name, vanilla) = block(block_name)?;
        if let Some(vanilla) = vanilla {
            if vanilla.default_state.block_entity_type != index {
                warn!(
                    "BlockEntityTypeAddBlocksEvent: vanilla block \"{block_name}\" keeps its block entity type, not \"{name}\""
                );
            }
            continue;
        }
        if !dynamic_types::add_block_entity_block(&block_name, index)
            .map_err(RegistrationError::Registry)?
        {
            warn!(
                "BlockEntityTypeAddBlocksEvent: block \"{block_name}\" keeps its first block entity type, not \"{name}\""
            );
        }
    }
    Ok(())
}

/// `ExtendPoiTypesEvent`: adds blocks to vanilla POI types. A villager then takes the block as a
/// job site of the profession of the POI type.
#[derive(Debug)]
pub struct ExtendPoiTypesEvent {
    error: FirstError,
}

registration_event!(ExtendPoiTypesEvent);

/// The vanilla POI types without a villager profession, from `PoiTypes`. The server tracks only
/// job sites as block POIs, so a block added to one of these is accepted and ignored.
const OTHER_POI_TYPES: [&str; 8] = [
    "minecraft:home",
    "minecraft:meeting",
    "minecraft:beehive",
    "minecraft:bee_nest",
    "minecraft:nether_portal",
    "minecraft:lodestone",
    "minecraft:lightning_rod",
    "minecraft:test_instance",
];

impl ExtendPoiTypesEvent {
    /// Adds every state of `block` to the POI type `poi_type`, such as `minecraft:librarian`.
    ///
    /// `addStatesToPoi` with a part of the states has no counterpart: a mod block has no state
    /// ids before the freeze, and the server tracks job sites by block.
    ///
    /// # Errors
    ///
    /// [`RegistrationError::DuplicatePoiBlock`] for a block that is already in a POI type,
    /// [`RegistrationError::UnknownPoiType`] and [`RegistrationError::UnknownBlock`].
    pub fn add_block_to_poi(
        &mut self,
        poi_type: &str,
        block: &str,
    ) -> Result<(), RegistrationError> {
        let result = add_block_to_poi(poi_type, block);
        self.error.keep(result)
    }
}

fn add_block_to_poi(poi_type: &str, block_name: &str) -> Result<(), RegistrationError> {
    let poi_type = namespaced(poi_type);
    let (block_name, vanilla) = block(block_name)?;
    let Some(profession) = poi::job_site_profession(&poi_type) else {
        if OTHER_POI_TYPES.contains(&poi_type.as_str()) {
            warn!(
                "ExtendPoiTypesEvent: the server tracks only job site POI types, so \"{block_name}\" does not join \"{poi_type}\""
            );
            return Ok(());
        }
        return Err(RegistrationError::UnknownPoiType(poi_type));
    };
    let duplicate = |previous: &str| RegistrationError::DuplicatePoiBlock {
        block: block_name.clone(),
        poi_type: previous.to_owned(),
    };
    if let Some(previous) = vanilla.and_then(pumpkin_core::world::villager_poi::poi_type_for_block)
    {
        return Err(duplicate(previous));
    }
    poi::add_block(&block_name, profession)
        .map_err(RegistrationError::Registry)?
        .map_or(Ok(()), |previous| {
            Err(duplicate(&format!("minecraft:{}", previous.to_name())))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vanilla_entity_type_already_has_default_attributes() {
        let mut event = EntityAttributeCreationEvent::new();
        assert_eq!(
            event.put("zombie", AttributeSupplier::new()),
            Err(RegistrationError::DuplicateAttributes(
                "minecraft:zombie".to_owned()
            ))
        );
        assert_eq!(
            event.put("examplemod:unknown", AttributeSupplier::new()),
            Err(RegistrationError::UnknownEntityType(
                "examplemod:unknown".to_owned()
            ))
        );
        // The handle keeps the first error.
        assert_eq!(
            event.take_error(),
            Some(RegistrationError::DuplicateAttributes(
                "minecraft:zombie".to_owned()
            ))
        );
    }

    #[test]
    fn attribute_supplier_replaces_a_value_and_adds_defaults() {
        let supplier = AttributeSupplier::of(&EntityType::ZOMBIE)
            .add(Attributes::MAX_HEALTH, 40.0)
            .add_default(Attributes::LUCK);
        let values = |attribute: &Attributes| {
            supplier
                .0
                .iter()
                .filter(|(a, _)| a == attribute)
                .map(|(_, v)| *v)
                .collect::<Vec<_>>()
        };
        assert_eq!(values(&Attributes::MAX_HEALTH), [40.0]);
        assert_eq!(values(&Attributes::LUCK), [Attributes::LUCK.default_value]);
        assert_eq!(supplier.0.len(), EntityType::ZOMBIE.attributes.len() + 1);
    }

    #[test]
    fn a_vanilla_spawn_placement_changes_its_placement_only_on_replace() {
        let mut event = RegisterSpawnPlacementsEvent::new();
        assert_eq!(
            event.register_placement(
                "minecraft:zombie",
                Some(SpawnLocation::InWater),
                None,
                spawn_predicate(|_, _, _| true),
                Operation::And,
            ),
            Err(RegistrationError::SpawnPlacementWithoutReplace(
                "minecraft:zombie".to_owned()
            ))
        );
        assert_eq!(
            event.register(
                "examplemod:unknown",
                spawn_predicate(|_, _, _| true),
                Operation::Or
            ),
            Err(RegistrationError::UnknownEntityType(
                "examplemod:unknown".to_owned()
            ))
        );
    }

    #[test]
    fn a_mod_spawn_placement_needs_its_placement_type_and_heightmap_first() {
        dynamic::register_entity_type(dynamic::EntityTypeDefinition {
            name: "test:placement_golem".to_owned(),
            display: &EntityType::ZOMBIE,
            dimensions: None,
            eye_height: None,
            tags: Vec::new(),
        })
        .unwrap();
        let mut event = RegisterSpawnPlacementsEvent::new();
        assert_eq!(
            event.register(
                "test:placement_golem",
                spawn_predicate(|_, _, _| true),
                Operation::Or,
            ),
            Err(RegistrationError::MissingSpawnPlacement(
                "test:placement_golem".to_owned()
            ))
        );
    }

    #[test]
    fn block_entity_types_and_blocks_must_exist() {
        let mut event = BlockEntityTypeAddBlocksEvent::new();
        assert_eq!(
            event.modify("minecraft:crate", &["minecraft:barrel"]),
            Err(RegistrationError::UnknownBlockEntityType(
                "minecraft:crate".to_owned()
            ))
        );
        assert_eq!(
            event.modify("minecraft:barrel", &["examplemod:unknown"]),
            Err(RegistrationError::UnknownBlock(
                "examplemod:unknown".to_owned()
            ))
        );
        // A vanilla block keeps its block entity type.
        assert_eq!(event.modify("barrel", &["minecraft:barrel"]), Ok(()));
    }

    #[test]
    fn a_block_joins_one_poi_type_only() {
        let mut event = ExtendPoiTypesEvent::new();
        assert_eq!(
            event.add_block_to_poi("minecraft:librarian", "minecraft:lectern"),
            Err(RegistrationError::DuplicatePoiBlock {
                block: "minecraft:lectern".to_owned(),
                poi_type: "minecraft:librarian".to_owned(),
            })
        );
        assert_eq!(
            event.add_block_to_poi("examplemod:alchemist", "minecraft:stone"),
            Err(RegistrationError::UnknownPoiType(
                "examplemod:alchemist".to_owned()
            ))
        );
        // A vanilla POI type without a profession is accepted and ignored.
        assert_eq!(
            event.add_block_to_poi("minecraft:home", "minecraft:stone"),
            Ok(())
        );
    }
}
