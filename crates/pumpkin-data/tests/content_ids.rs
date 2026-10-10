//! The mappers and the tag lists follow the content id mode: a `Real` client gets the allocated id
//! of custom content, a `Display` client the display id, and both get the display id of a
//! placeholder. The content registry is process-wide and freezes once, so this file is its own
//! test binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::sync::Once;

use pumpkin_data::data_component_impl::IDSetContent;
use pumpkin_data::dynamic::{
    self, BlockDefinition, BlockPropertyDefinition, ContentIds, ContentKind, EntityTypeDefinition,
    ItemDefinition, TagDefinition,
};
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::tag::RegistryKey;
use pumpkin_data::{Block, BlockId, BlockStateId};

const MODES: [ContentIds; 2] = [ContentIds::Display, ContentIds::Real];

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn content() {
    static FREEZE: Once = Once::new();
    FREEZE.call_once(|| {
        dynamic::register_block(BlockDefinition {
            name: "test:lamp".to_string(),
            display: Block::REDSTONE_LAMP.default_state.id,
            properties: vec![BlockPropertyDefinition::bool("lit", false)],
            tags: strings(&["minecraft:mineable/pickaxe", "test:lamps"]),
        })
        .unwrap();
        dynamic::register_placeholder_block(BlockDefinition {
            name: "test:gone".to_string(),
            display: Block::STONE.default_state.id,
            properties: vec![BlockPropertyDefinition::bool("lit", false)],
            tags: strings(&["minecraft:mineable/pickaxe", "test:lamps"]),
        })
        .unwrap();
        dynamic::register_tag(TagDefinition {
            kind: ContentKind::Block,
            name: "test:lamps".to_string(),
            values: strings(&["minecraft:redstone_lamp"]),
        })
        .unwrap();
        dynamic::register_item(ItemDefinition {
            name: "test:ruby".to_string(),
            display: &Item::DIAMOND,
            block: None,
            tags: Vec::new(),
        })
        .unwrap();
        dynamic::register_placeholder_item(ItemDefinition {
            name: "test:ghost".to_string(),
            display: &Item::EMERALD,
            block: None,
            tags: Vec::new(),
        })
        .unwrap();
        dynamic::register_entity_type(EntityTypeDefinition {
            name: "test:golem".to_string(),
            display: &EntityType::ZOMBIE,
            dimensions: None,
            eye_height: None,
            tags: Vec::new(),
        })
        .unwrap();
        dynamic::register_placeholder_entity_type(EntityTypeDefinition {
            name: "test:shade".to_string(),
            display: &EntityType::PIG,
            dimensions: None,
            eye_height: None,
            tags: Vec::new(),
        })
        .unwrap();
        dynamic::freeze().unwrap();
    });
}

fn block(name: &str) -> &'static Block {
    content();
    Block::from_name(name).unwrap()
}

#[test]
fn block_state_mapper() {
    let lamp = block("test:lamp");
    let gone = block("test:gone");
    // The placeholder sorts first, so it takes the first server state ids. The client never sees
    // its states, so the lamp's client-side state ids start at the generated count.
    assert!(gone.states[0].id < lamp.states[0].id);
    for (index, state) in lamp.states.iter().enumerate() {
        assert!(state.id.as_u16() >= BlockStateId::COUNT);
        assert_eq!(
            state.id.to_java_network_id(ContentIds::Display),
            Block::REDSTONE_LAMP.default_state.id.as_u16()
        );
        assert_eq!(
            state.id.to_java_network_id(ContentIds::Real),
            BlockStateId::COUNT + index as u16
        );
    }
    for state in gone.states {
        for ids in MODES {
            assert_eq!(
                state.id.to_java_network_id(ids),
                Block::STONE.default_state.id.as_u16()
            );
        }
    }
    for ids in MODES {
        let stone = Block::STONE.default_state.id;
        assert_eq!(stone.to_java_network_id(ids), stone.as_u16());
    }
}

#[test]
fn block_mapper() {
    let lamp = block("test:lamp");
    let gone = block("test:gone");
    assert_eq!(
        lamp.to_java_network_id(ContentIds::Display),
        Block::REDSTONE_LAMP.id.as_u16()
    );
    assert_eq!(lamp.to_java_network_id(ContentIds::Real), lamp.id.as_u16());
    assert!(lamp.id.as_u16() >= BlockId::COUNT);
    for ids in MODES {
        assert_eq!(gone.to_java_network_id(ids), Block::STONE.id.as_u16());
        assert_eq!(
            Block::CHEST.to_java_network_id(ids),
            Block::CHEST.id.as_u16()
        );
        assert_eq!(
            IDSetContent::registry_id(lamp, ids),
            lamp.to_java_network_id(ids)
        );
    }
}

#[test]
fn item_mapper() {
    content();
    let ruby = Item::from_registry_key("test:ruby").unwrap();
    let ghost = Item::from_registry_key("test:ghost").unwrap();
    assert_eq!(
        ruby.to_java_network_id(ContentIds::Display),
        Item::DIAMOND.id
    );
    assert_eq!(ruby.to_java_network_id(ContentIds::Real), ruby.id);
    assert!(ruby.id >= Item::COUNT);
    for ids in MODES {
        assert_eq!(ghost.to_java_network_id(ids), Item::EMERALD.id);
        assert_eq!(Item::STICK.to_java_network_id(ids), Item::STICK.id);
        assert_eq!(
            IDSetContent::registry_id(ruby, ids),
            ruby.to_java_network_id(ids)
        );
    }
}

#[test]
fn entity_type_mapper() {
    content();
    let golem = EntityType::from_name("test:golem").unwrap();
    let shade = EntityType::from_name("test:shade").unwrap();
    assert_eq!(
        golem.to_java_network_id(ContentIds::Display),
        EntityType::ZOMBIE.id
    );
    assert_eq!(golem.to_java_network_id(ContentIds::Real), golem.id);
    assert!(golem.id >= EntityType::COUNT);
    for ids in MODES {
        assert_eq!(shade.to_java_network_id(ids), EntityType::PIG.id);
        assert_eq!(EntityType::PIG.to_java_network_id(ids), EntityType::PIG.id);
        assert_eq!(
            IDSetContent::registry_id(golem, ids),
            golem.to_java_network_id(ids)
        );
    }
}

#[test]
fn network_tags_follow_the_mode() {
    let lamp = block("test:lamp").id.as_u16();
    let gone = block("test:gone").id.as_u16();
    let (name, generated) = pumpkin_data::tag::get_latest_map(RegistryKey::Block)
        .entries()
        .find(|(name, _)| name.ends_with("mineable/pickaxe"))
        .map(|(name, values)| (*name, values.1))
        .unwrap();
    // The custom table holds both custom members, the placeholder too, in id order.
    assert_eq!(
        dynamic::tag_ids(RegistryKey::Block, name).unwrap().1,
        [gone, lamp]
    );

    let display =
        dynamic::network_tag_ids(RegistryKey::Block, name, generated, ContentIds::Display);
    assert_eq!(&*display, generated);
    let real = dynamic::network_tag_ids(RegistryKey::Block, name, generated, ContentIds::Real);
    assert_eq!(real[..generated.len()], *generated);
    assert_eq!(real[generated.len()..], [lamp]);

    assert!(dynamic::network_mod_tags(RegistryKey::Block, ContentIds::Display).is_empty());
    assert_eq!(
        dynamic::network_mod_tags(RegistryKey::Block, ContentIds::Real),
        [("test:lamps", vec![Block::REDSTONE_LAMP.id.as_u16(), lamp])]
    );
    assert!(dynamic::network_mod_tags(RegistryKey::Item, ContentIds::Real).is_empty());
}
