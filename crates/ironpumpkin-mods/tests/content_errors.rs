//! The registry errors reach a mod as a `Result` from `ModInit`, one test per kind of error that
//! the builders can produce. Nothing here freezes the registry, so the tests share the process
//! and use distinct names. The freeze is in `content_registration.rs`.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::sync::Arc;

use ironpumpkin_mods::{
    ModInit,
    content::{BlockBuilder, ContentKind, EntityTypeBuilder, ItemBuilder, RegistryError},
    entity::{Entity, EntityBase},
    pumpkin_data::{Block, entity::EntityType, item::Item},
};

fn spawn(entity: Entity) -> Arc<dyn EntityBase> {
    Arc::new(entity)
}

fn init() -> ModInit {
    ModInit::new("error-mod")
}

fn block(name: &str) -> BlockBuilder {
    BlockBuilder::new(name, Block::STONE.default_state.id)
}

fn item(name: &str) -> ItemBuilder {
    ItemBuilder::new(name, &Item::STICK)
}

fn entity_type(name: &str) -> EntityTypeBuilder {
    EntityTypeBuilder::new(name, &EntityType::ZOMBIE)
}

#[test]
fn a_name_registered_twice_is_a_duplicate() {
    let mut cx = init();
    cx.register_block(block("error-mod:twice")).unwrap();
    cx.register_item(item("error-mod:twice")).unwrap();
    cx.register_entity_type(entity_type("error-mod:twice"), spawn)
        .unwrap();

    let duplicate = |kind| {
        Err(RegistryError::Duplicate {
            kind,
            name: "error-mod:twice".to_string(),
        })
    };
    assert_eq!(
        cx.register_block(block("error-mod:twice")),
        duplicate(ContentKind::Block)
    );
    assert_eq!(
        cx.register_item(item("error-mod:twice")),
        duplicate(ContentKind::Item)
    );
    assert_eq!(
        cx.register_entity_type(entity_type("error-mod:twice"), spawn),
        duplicate(ContentKind::EntityType)
    );
}

#[test]
fn the_minecraft_namespace_and_a_missing_namespace_are_reserved() {
    let mut cx = init();
    for name in ["minecraft:lamp", "lamp", ":lamp"] {
        assert_eq!(
            cx.register_block(block(name)),
            Err(RegistryError::ReservedNamespace {
                kind: ContentKind::Block,
                name: name.to_string()
            })
        );
        assert_eq!(
            cx.register_item(item(name)),
            Err(RegistryError::ReservedNamespace {
                kind: ContentKind::Item,
                name: name.to_string()
            })
        );
    }
}

#[test]
fn a_malformed_name_is_invalid() {
    let mut cx = init();
    assert_eq!(
        cx.register_entity_type(entity_type("error-mod:Bad Name"), spawn),
        Err(RegistryError::InvalidName {
            kind: ContentKind::EntityType,
            name: "error-mod:Bad Name".to_string()
        })
    );
}

#[test]
fn an_invalid_property_is_rejected_with_its_reason() {
    let mut cx = init();
    let invalid = |block: &str, property: &str, reason| {
        Err(RegistryError::InvalidProperty {
            block: block.to_string(),
            property: property.to_string(),
            reason,
        })
    };

    // One value is not a choice.
    assert_eq!(
        cx.register_block(block("error-mod:one_value").int_property("age", 2, 2, 2)),
        invalid("error-mod:one_value", "age", "it needs at least two values")
    );
    assert_eq!(
        cx.register_block(block("error-mod:bad_default").enum_property("mode", &["a", "b"], "c")),
        invalid(
            "error-mod:bad_default",
            "mode",
            "the default is not one of the values"
        )
    );
    assert_eq!(
        cx.register_block(block("error-mod:bad_word").enum_property("Mode", &["a", "b"], "a")),
        invalid(
            "error-mod:bad_word",
            "Mode",
            "the name must use a-z 0-9 and _"
        )
    );
    assert_eq!(
        cx.register_block(
            block("error-mod:twice_property")
                .bool_property("lit", false)
                .bool_property("lit", true),
        ),
        invalid("error-mod:twice_property", "lit", "the name is used twice")
    );
}

#[test]
fn dimensions_that_are_not_positive_and_finite_are_rejected() {
    let mut cx = init();
    let invalid = |name: &str| {
        Err(RegistryError::InvalidDimensions {
            name: name.to_string(),
        })
    };
    assert_eq!(
        cx.register_entity_type(entity_type("error-mod:flat").dimensions(1.0, 0.0), spawn),
        invalid("error-mod:flat")
    );
    assert_eq!(
        cx.register_entity_type(
            entity_type("error-mod:nan").dimensions(f32::NAN, 1.0),
            spawn
        ),
        invalid("error-mod:nan")
    );
    assert_eq!(
        cx.register_entity_type(entity_type("error-mod:blind").eye_height(-1.0), spawn),
        invalid("error-mod:blind")
    );
    // A rejected type leaves its name free.
    cx.register_entity_type(entity_type("error-mod:flat").dimensions(1.0, 1.0), spawn)
        .unwrap();
}

#[test]
fn a_tag_that_the_registry_does_not_have_is_rejected() {
    let mut cx = init();
    let unknown = |kind, name: &str, tag: &str| {
        Err(RegistryError::UnknownTag {
            kind,
            name: name.to_string(),
            tag: tag.to_string(),
        })
    };
    assert_eq!(
        cx.register_block(block("error-mod:untagged").tag("minecraft:no_such_tag")),
        unknown(
            ContentKind::Block,
            "error-mod:untagged",
            "minecraft:no_such_tag"
        )
    );
    // `minecraft:swords` is an item tag, not a block tag.
    assert_eq!(
        cx.register_block(block("error-mod:wrong_registry").tag("minecraft:swords")),
        unknown(
            ContentKind::Block,
            "error-mod:wrong_registry",
            "minecraft:swords"
        )
    );
    assert_eq!(
        cx.register_item(item("error-mod:untagged").tag("mymod:own_tag")),
        unknown(ContentKind::Item, "error-mod:untagged", "mymod:own_tag")
    );
    assert_eq!(
        cx.register_entity_type(
            entity_type("error-mod:untagged").tag("minecraft:swords"),
            spawn
        ),
        unknown(
            ContentKind::EntityType,
            "error-mod:untagged",
            "minecraft:swords"
        )
    );
}
