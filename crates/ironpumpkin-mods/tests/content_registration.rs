//! A native mod registers content through `ModInit`, and the registry holds it after the freeze.
//! The content registry freezes once per process, so this file is its own test binary. It uses
//! `ironpumpkin-mods` only, like a mod crate does.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::sync::{Arc, OnceLock};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    content::{
        BlockBuilder, ContentKind, DynamicTaggable, EntityTypeBuilder, ItemBuilder, RegistryError,
        TagBuilder,
    },
    entity::{Entity, EntityBase, custom},
    mods,
    pumpkin_data::{
        Block,
        dynamic::{self, ContentTables},
        entity::EntityType,
        item::Item,
        tag::RegistryKey,
    },
    register_mod,
};

fn spawn(entity: Entity) -> Arc<dyn EntityBase> {
    Arc::new(entity)
}

struct SampleMod;

impl NativeMod for SampleMod {
    fn id(&self) -> &'static str {
        "sample-mod"
    }

    fn display_name(&self) -> &'static str {
        "Sample Mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        cx.register_block(
            BlockBuilder::new(
                "sample-mod:copper_lamp",
                Block::REDSTONE_LAMP.default_state.id,
            )
            .bool_property("lit", false)
            .int_property("level", 1, 3, 2)
            .tag("minecraft:mineable/pickaxe")
            .tag("sample-mod:lamps"),
        )
        .unwrap();
        cx.register_tag(
            TagBuilder::new(ContentKind::Block, "sample-mod:lamps")
                .value("minecraft:redstone_lamp"),
        )
        .unwrap();
        // The tag lists the wand before the wand is registered.
        cx.register_tag(
            TagBuilder::new(ContentKind::Item, "sample-mod:tools")
                .value("stick")
                .value("sample-mod:wand"),
        )
        .unwrap();
        cx.register_item(
            ItemBuilder::new("sample-mod:copper_lamp", &Item::REDSTONE_LAMP)
                .places("sample-mod:copper_lamp"),
        )
        .unwrap();
        cx.register_item(ItemBuilder::new("sample-mod:wand", &Item::STICK).tag("minecraft:swords"))
            .unwrap();
        cx.register_entity_type(
            EntityTypeBuilder::new("sample-mod:golem", &EntityType::ZOMBIE)
                .dimensions(1.5, 2.5)
                .eye_height(2.0)
                .tag("minecraft:skeletons"),
            spawn,
        )
        .unwrap();
    }
}

register_mod!(SampleMod);

/// Runs the init of every linked mod and freezes the registry, as the server does at startup.
fn boot() -> &'static ContentTables {
    static TABLES: OnceLock<&'static ContentTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mods = mods().unwrap();
        assert_eq!(mods.len(), 1);
        for native_mod in mods {
            let mut init = ModInit::new(native_mod.id());
            native_mod.init(&mut init);
        }
        dynamic::freeze().unwrap()
    })
}

#[test]
fn a_mod_registers_a_block_and_an_item_that_the_registry_holds() {
    let tables = boot();
    assert_eq!(tables.blocks().len(), 1);
    assert_eq!(tables.items().len(), 2);

    let lamp = Block::from_name("sample-mod:copper_lamp").unwrap();
    assert!(lamp.id.as_u16() >= Block::FIRST_CUSTOM_ID);
    assert_eq!(lamp.states.len(), 6);
    let defaults = lamp.properties(lamp.default_state.id).unwrap().to_props();
    assert_eq!(defaults, vec![("level", "2"), ("lit", "false")]);
    assert_eq!(
        lamp.to_java_network_id(),
        Block::REDSTONE_LAMP.to_java_network_id()
    );

    assert!(lamp.has_tag_dynamic("minecraft:mineable/pickaxe"));
    assert!(!lamp.has_tag_dynamic("minecraft:mineable/shovel"));

    let lamp_item = Item::from_registry_key("sample-mod:copper_lamp").unwrap();
    assert_eq!(Block::from_item_id(lamp_item.id), Some(lamp));
    assert_eq!(lamp.item_id, lamp_item.id);

    let wand = Item::from_registry_key("sample-mod:wand").unwrap();
    assert_eq!(Block::from_item_id(wand.id), None);
    assert!(wand.has_tag_dynamic("minecraft:swords"));
    assert!(!Item::STICK.has_tag_dynamic("minecraft:swords"));
}

#[test]
fn a_mod_tag_holds_vanilla_and_custom_members() {
    boot();
    let lamp = Block::from_name("sample-mod:copper_lamp").unwrap();
    assert!(lamp.has_tag_dynamic("sample-mod:lamps"));
    assert!(Block::REDSTONE_LAMP.has_tag_dynamic("#sample-mod:lamps"));
    assert!(!Block::STONE.has_tag_dynamic("sample-mod:lamps"));

    let wand = Item::from_registry_key("sample-mod:wand").unwrap();
    assert_eq!(
        dynamic::tag_ids(RegistryKey::Item, "sample-mod:tools"),
        Some((&[Item::STICK.id][..], &[wand.id][..]))
    );
    assert_eq!(
        dynamic::tag_ids(RegistryKey::Block, "sample-mod:tools"),
        None
    );
}

#[test]
fn a_mod_registers_an_entity_type_with_its_factory() {
    boot();
    let golem = EntityType::from_name("sample-mod:golem").unwrap();
    assert_eq!(golem.dimension, [1.5, 2.5]);
    assert_eq!(golem.eye_height, 2.0);
    assert_eq!(golem.to_java_network_id(), EntityType::ZOMBIE.id);
    assert!(custom::factory(golem).is_some());
    assert!(golem.has_tag_dynamic("minecraft:skeletons"));
}

#[test]
fn registration_after_the_freeze_is_rejected() {
    boot();
    let mut init = ModInit::new("sample-mod");
    assert_eq!(
        init.register_block(BlockBuilder::new(
            "sample-mod:late",
            Block::STONE.default_state.id
        )),
        Err(RegistryError::RegistryFrozen)
    );
    assert_eq!(
        init.register_item(ItemBuilder::new("sample-mod:late", &Item::STICK)),
        Err(RegistryError::RegistryFrozen)
    );
    assert_eq!(
        init.register_entity_type(
            EntityTypeBuilder::new("sample-mod:late", &EntityType::ZOMBIE),
            spawn
        ),
        Err(RegistryError::RegistryFrozen)
    );
    assert_eq!(
        init.register_tag(TagBuilder::new(ContentKind::Block, "sample-mod:late")),
        Err(RegistryError::RegistryFrozen)
    );
    assert!(Block::from_name("sample-mod:late").is_none());
}
