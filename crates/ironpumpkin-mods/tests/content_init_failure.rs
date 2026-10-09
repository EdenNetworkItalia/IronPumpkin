//! A content registration error stops the server: `ModInit` keeps the first one, even when the
//! mod ignores the `Result`, and `init_mod` returns it. `init_mods` logs it and exits; the exit
//! needs a server, so these tests stop at `init_mod`. Nothing here freezes the registry, so the
//! tests share the process and use distinct names.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::sync::Arc;

use ironpumpkin_mods::{
    ContentRegistrationError, ModInit, NativeMod,
    content::{BlockBuilder, ContentKind, EntityTypeBuilder, ItemBuilder, RegistryError},
    entity::{Entity, EntityBase},
    init_mod,
    pumpkin_data::{Block, entity::EntityType, item::Item},
};

struct TestMod {
    id: &'static str,
    init: fn(&mut ModInit),
}

impl NativeMod for TestMod {
    fn id(&self) -> &'static str {
        self.id
    }

    fn display_name(&self) -> &'static str {
        "Test mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        (self.init)(cx);
    }
}

fn spawn(entity: Entity) -> Arc<dyn EntityBase> {
    Arc::new(entity)
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

fn duplicate(kind: ContentKind, id: &'static str, name: &str) -> ContentRegistrationError {
    ContentRegistrationError {
        mod_id: id,
        kind,
        name: name.to_string(),
        error: RegistryError::Duplicate {
            kind,
            name: name.to_string(),
        },
    }
}

#[test]
fn a_mod_that_registers_its_content_starts() {
    let clean = TestMod {
        id: "clean-mod",
        init: |cx| {
            cx.register_block(block("clean-mod:lamp")).unwrap();
            cx.register_item(item("clean-mod:lamp")).unwrap();
            cx.register_entity_type(entity_type("clean-mod:golem"), spawn)
                .unwrap();
        },
    };
    assert!(init_mod(&clean).is_ok());
}

#[test]
fn a_duplicate_that_the_mod_ignores_still_fails_the_init() {
    let blocks = TestMod {
        id: "block-mod",
        init: |cx| {
            let _ = cx.register_block(block("block-mod:twice"));
            let _ = cx.register_block(block("block-mod:twice"));
        },
    };
    let items = TestMod {
        id: "item-mod",
        init: |cx| {
            let _ = cx.register_item(item("item-mod:twice"));
            let _ = cx.register_item(item("item-mod:twice"));
        },
    };
    let entity_types = TestMod {
        id: "entity-mod",
        init: |cx| {
            let _ = cx.register_entity_type(entity_type("entity-mod:twice"), spawn);
            let _ = cx.register_entity_type(entity_type("entity-mod:twice"), spawn);
        },
    };

    assert_eq!(
        init_mod(&blocks).err(),
        Some(duplicate(
            ContentKind::Block,
            "block-mod",
            "block-mod:twice"
        ))
    );
    assert_eq!(
        init_mod(&items).err(),
        Some(duplicate(ContentKind::Item, "item-mod", "item-mod:twice"))
    );
    assert_eq!(
        init_mod(&entity_types).err(),
        Some(duplicate(
            ContentKind::EntityType,
            "entity-mod",
            "entity-mod:twice"
        ))
    );
}

#[test]
fn the_mod_still_gets_the_error_and_the_server_keeps_it() {
    let reacting = TestMod {
        id: "react-mod",
        init: |cx| {
            cx.register_block(block("react-mod:lamp")).unwrap();
            let err = cx.register_block(block("react-mod:lamp")).unwrap_err();
            assert_eq!(
                err,
                RegistryError::Duplicate {
                    kind: ContentKind::Block,
                    name: "react-mod:lamp".to_string()
                }
            );
        },
    };
    assert_eq!(
        init_mod(&reacting).err(),
        Some(duplicate(ContentKind::Block, "react-mod", "react-mod:lamp"))
    );
}

#[test]
fn only_the_first_error_is_kept() {
    let two_errors = TestMod {
        id: "first-mod",
        init: |cx| {
            let _ = cx.register_item(item("minecraft:stick"));
            let _ = cx.register_block(block("first-mod:lamp"));
            let _ = cx.register_block(block("first-mod:lamp"));
        },
    };
    assert_eq!(
        init_mod(&two_errors).err(),
        Some(ContentRegistrationError {
            mod_id: "first-mod",
            kind: ContentKind::Item,
            name: "minecraft:stick".to_string(),
            error: RegistryError::ReservedNamespace {
                kind: ContentKind::Item,
                name: "minecraft:stick".to_string()
            },
        })
    );
}

#[test]
fn the_error_reads_as_one_line_with_the_mod_id_kind_and_name() {
    let broken = TestMod {
        id: "line-mod",
        init: |cx| {
            let _ = cx.register_entity_type(entity_type("line-mod:golem"), spawn);
            let _ = cx.register_entity_type(entity_type("line-mod:golem"), spawn);
        },
    };
    assert_eq!(
        init_mod(&broken).err().unwrap().to_string(),
        "native mod \"line-mod\" cannot register the entity type \"line-mod:golem\": entity type \"line-mod:golem\" is already registered"
    );
}
