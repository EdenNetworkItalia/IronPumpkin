//! Spawn factories of custom entity types.
//!
//! The data of a custom entity type lives in `pumpkin_data::dynamic`, which cannot name
//! [`Entity`]. The factory that builds its entity lives here, keyed by the same name, and becomes
//! a lock-free table indexed by the custom id on the first spawn after the freeze.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use pumpkin_data::dynamic::{self, EntityTypeDefinition, RegistryError};
use pumpkin_data::entity::EntityType;

use crate::entity::{Entity, EntityBase};

/// Builds the entity of a custom type from its base [`Entity`].
///
/// It works like the vanilla constructors in [`from_type`](crate::entity::r#type::from_type).
/// Saved fields are read afterwards through [`EntityBase::read_nbt_non_mut`].
pub type EntityFactory = fn(Entity) -> Arc<dyn EntityBase>;

static PENDING: Mutex<BTreeMap<String, EntityFactory>> = Mutex::new(BTreeMap::new());
static FACTORIES: OnceLock<Vec<Option<EntityFactory>>> = OnceLock::new();

/// Registers a custom entity type and the factory that spawns it. See [`EntityTypeDefinition`]
/// for what vanilla clients see.
pub fn register_entity_type(
    definition: EntityTypeDefinition,
    factory: EntityFactory,
) -> Result<(), RegistryError> {
    // Held across both steps, so the table built after the freeze sees every factory whose
    // type was accepted.
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    let name = definition.name.clone();
    dynamic::register_entity_type(definition)?;
    pending.insert(name, factory);
    Ok(())
}

/// The factory of a custom entity type, or `None` for vanilla types and for custom types
/// registered without one.
pub fn factory(entity_type: &EntityType) -> Option<EntityFactory> {
    let index = usize::from(entity_type.id.checked_sub(EntityType::COUNT)?);
    // A custom id resolves only after the freeze, and registration fails after it, so the
    // pending map is complete when the table is built.
    EntityType::from_raw(entity_type.id)?;
    FACTORIES.get_or_init(build).get(index).copied().flatten()
}

fn build() -> Vec<Option<EntityFactory>> {
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    (EntityType::COUNT..=u16::MAX)
        .map_while(EntityType::from_raw)
        .map(|entity_type| pending.remove(entity_type.resource_name))
        .collect()
}
