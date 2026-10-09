//! Custom entity type factories resolve after the freeze. The content registry is process-wide
//! and freezes once, so this file is its own test binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::sync::Arc;

use pumpkin_core::entity::custom::{factory, register_entity_type};
use pumpkin_core::entity::{Entity, EntityBase};
use pumpkin_data::dynamic::{self, ContentKind, EntityTypeDefinition, RegistryError};
use pumpkin_data::entity::EntityType;

fn spawn(entity: Entity) -> Arc<dyn EntityBase> {
    Arc::new(entity)
}

fn definition(name: &str) -> EntityTypeDefinition {
    EntityTypeDefinition {
        name: name.to_string(),
        display: &EntityType::ZOMBIE,
        dimensions: Some([1.0, 2.0]),
        eye_height: Some(1.5),
    }
}

#[test]
fn factories_resolve_after_the_freeze() {
    register_entity_type(definition("test:golem"), spawn).unwrap();
    dynamic::register_entity_type(definition("test:plain")).unwrap();
    assert_eq!(
        register_entity_type(definition("test:golem"), spawn),
        Err(RegistryError::Duplicate {
            kind: ContentKind::EntityType,
            name: "test:golem".to_string()
        })
    );

    dynamic::freeze().unwrap();

    let golem = EntityType::from_name("test:golem").unwrap();
    assert!(factory(golem).is_some());
    assert!(factory(EntityType::from_name("test:plain").unwrap()).is_none());
    assert!(factory(&EntityType::ZOMBIE).is_none());
    assert_eq!(golem.dimension, [1.0, 2.0]);
    assert_eq!(golem.eye_height, 1.5);
    assert_eq!(golem.to_java_network_id(), EntityType::ZOMBIE.id);
    assert_eq!(
        register_entity_type(definition("test:late"), spawn),
        Err(RegistryError::RegistryFrozen)
    );
}
