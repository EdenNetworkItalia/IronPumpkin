//! The getters of `DeferredRegister` (`getNamespace`, `getEntries`, `getRegistry`,
//! `getRegistryKey`, `getRegistryName`) on `ModInit`, and the `Registry` handle.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use ironpumpkin_mods::{
    ModInit, NativeMod,
    content::{BlockBuilder, ContentKind, ItemBuilder, TagBuilder},
    init_mod,
    pumpkin_data::{Block, item::Item},
    registry::Registry,
};

const KINDS: [(ContentKind, &str); 7] = [
    (ContentKind::Block, "minecraft:block"),
    (ContentKind::Item, "minecraft:item"),
    (ContentKind::EntityType, "minecraft:entity_type"),
    (ContentKind::SoundEvent, "minecraft:sound_event"),
    (ContentKind::MobEffect, "minecraft:mob_effect"),
    (ContentKind::Attribute, "minecraft:attribute"),
    (
        ContentKind::DataComponentType,
        "minecraft:data_component_type",
    ),
];

fn names(entries: &[ironpumpkin_mods::identifier::Identifier]) -> Vec<String> {
    entries.iter().map(ToString::to_string).collect()
}

#[test]
fn entries_list_what_the_mod_registered_in_registration_order() {
    let mut cx = ModInit::new("getters-one");
    assert!(cx.entries(ContentKind::Block).is_empty());
    for name in ["getters-one:b", "getters-one:a"] {
        cx.register_block(BlockBuilder::new(name, Block::STONE.default_state.id))
            .unwrap();
    }
    cx.register_item(ItemBuilder::new("getters-one:ingot", &Item::IRON_INGOT))
        .unwrap();
    // A tag is not an entry.
    cx.register_tag(TagBuilder::new(ContentKind::Item, "getters-one:ingots").value("stick"))
        .unwrap();
    // A failed registration is not an entry: the name is already a block.
    assert!(
        cx.register_block(BlockBuilder::new(
            "getters-one:a",
            Block::STONE.default_state.id
        ))
        .is_err()
    );

    assert_eq!(
        names(&cx.entries(ContentKind::Block)),
        ["getters-one:b", "getters-one:a"]
    );
    assert_eq!(names(&cx.entries(ContentKind::Item)), ["getters-one:ingot"]);
    assert!(cx.entries(ContentKind::EntityType).is_empty());
    // Another mod has its own list.
    assert!(
        ModInit::new("getters-two")
            .entries(ContentKind::Block)
            .is_empty()
    );
}

#[test]
fn the_registry_getters_name_each_registry() {
    let cx = ModInit::new("getters-three");
    for (kind, name) in KINDS {
        assert_eq!(cx.registry_key(kind), kind);
        assert_eq!(cx.registry_name(kind).to_string(), name);
        let registry = cx.registry(kind);
        assert_eq!(registry, Registry::of(kind));
        assert_eq!(registry.key(), kind);
        assert_eq!(registry.name().to_string(), name);
    }
}

#[test]
fn the_registry_handle_answers_the_lookups_of_the_registry_module() {
    let cx = ModInit::new("getters-four");
    let items = cx.registry(ContentKind::Item);
    assert!(items.contains("minecraft:iron_ingot"));
    assert!(!items.contains("getters-four:unknown"));
    let id = items.id_of("minecraft:iron_ingot").unwrap();
    assert_eq!(
        items.key_of(id).unwrap().to_string(),
        "minecraft:iron_ingot"
    );
    assert!(items.is_in_tag("minecraft:iron_ingot", "c:ingots"));
    assert!(
        names(&items.tag_entries("c:ingots").unwrap()).contains(&"minecraft:iron_ingot".to_owned())
    );
    assert!(
        items
            .keys()
            .iter()
            .any(|key| key.to_string() == "minecraft:stick")
    );
}

struct Namespaced;

impl NativeMod for Namespaced {
    fn id(&self) -> &'static str {
        "getters_mod"
    }

    fn display_name(&self) -> &'static str {
        "Getters"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn mods_toml(&self) -> Option<&'static str> {
        Some(
            "license = \"MIT\"\n[[mods]]\nmodId = \"getters_mod\"\nnamespace = \"getters\"\n\
             version = \"1.0.0\"\n",
        )
    }

    fn init(&self, cx: &mut ModInit) {
        assert_eq!(cx.namespace(), "getters");
    }
}

struct Plain;

impl NativeMod for Plain {
    fn id(&self) -> &'static str {
        "getters-plain"
    }

    fn display_name(&self) -> &'static str {
        "Plain"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        assert_eq!(cx.namespace(), "getters-plain");
    }
}

#[test]
fn the_namespace_is_the_namespace_of_the_mods_toml_or_the_mod_id() {
    // The assertions run inside `init`.
    let init = init_mod(&Namespaced).unwrap();
    assert_eq!(init.namespace(), "getters");
    let init = init_mod(&Plain).unwrap();
    assert_eq!(init.namespace(), "getters-plain");
    assert_eq!(ModInit::new("getters-new").namespace(), "getters-new");
}
