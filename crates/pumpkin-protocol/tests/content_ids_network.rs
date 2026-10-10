//! Every content id egress follows the content id mode of the encoding key: a `Real` client gets
//! the allocated id of custom content, a `Display` client the display id, and both get the display
//! id of a placeholder. The content registry is process-wide and freezes once, so this file is its
//! own test binary.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails on the first missing value"
)]

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Once;

use pumpkin_data::data_component::DataComponent;
use pumpkin_data::data_component_impl::{
    ContainerImpl, DataComponentImpl, EntityDataImpl, IDSet, RepairableImpl,
};
use pumpkin_data::dynamic::{
    self, BlockDefinition, BlockPropertyDefinition, ContentIds, ContentKind, EntityTypeDefinition,
    ItemDefinition, TagDefinition,
};
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::particle::Particle;
use pumpkin_data::recipes::RecipeCategoryTypes;
use pumpkin_data::tag::RegistryKey;
use pumpkin_data::tracked_data::block_display;
use pumpkin_data::world::WorldEvent;
use pumpkin_data::{Block, BlockStateId};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::data_component::serialize;
use pumpkin_protocol::codec::item_stack_seralizer::ItemStackSerializer;
use pumpkin_protocol::codec::recipe::{
    DynamicRecipe, OwnedCraftingRecipe, OwnedRecipeIngredient, OwnedRecipeResult,
};
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_protocol::java::client::config::CUpdateTags;
use pumpkin_protocol::java::client::play::{
    CBlockEvent, CBlockUpdate, CLevelEvent, CMultiBlockUpdate, CParticle, CRecipeBookAdd,
    CSpawnEntity, CWorldEvent, Metadata,
};
use pumpkin_protocol::ser::NetworkReadExt;
use pumpkin_protocol::{ClientPacket, EncodingKey};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::version::JavaMinecraftVersion;

const DISPLAY: EncodingKey = EncodingKey::new(JavaMinecraftVersion::V_26_3, ContentIds::Display);
const REAL: EncodingKey = EncodingKey::new(JavaMinecraftVersion::V_26_3, ContentIds::Real);

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

/// Registers a lamp block, a ruby item and a golem entity type, a placeholder of each, and the
/// mod tag `test:lamps`, then freezes the registry.
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
            properties: Vec::new(),
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
        dynamic::freeze().expect("freeze registry");
    });
}

fn lamp() -> &'static Block {
    content();
    Block::from_name("test:lamp").unwrap()
}

fn gone() -> &'static Block {
    content();
    Block::from_name("test:gone").unwrap()
}

fn item(name: &str) -> &'static Item {
    content();
    Item::from_registry_key(name).unwrap()
}

fn entity_type(name: &str) -> &'static EntityType {
    content();
    EntityType::from_name(name).unwrap()
}

fn bytes(packet: &impl ClientPacket, key: EncodingKey) -> Vec<u8> {
    let mut buf = Vec::new();
    packet.write_packet_data(&mut buf, &key).unwrap();
    buf
}

fn var_int(value: i32) -> Vec<u8> {
    let mut buf = Vec::new();
    VarInt(value).encode(&mut buf).unwrap();
    buf
}

fn raw_state(id: BlockStateId) -> i32 {
    i32::from(id.as_u16())
}

/// The state id a `Real` client knows, which skips the states of placeholders.
fn real_state(id: BlockStateId) -> Vec<u8> {
    var_int(i32::from(id.to_java_network_id(ContentIds::Real)))
}

/// Checks a packet that carries a custom id: a `Display` client gets the bytes of the packet with
/// the display id, a `Real` client other bytes, and a placeholder encodes the same in both modes.
fn assert_follows_mode<P: ClientPacket>(custom: &P, with_display: &P, placeholder: &P) {
    assert_eq!(bytes(custom, DISPLAY), bytes(with_display, DISPLAY));
    assert_ne!(bytes(custom, REAL), bytes(custom, DISPLAY));
    assert_eq!(bytes(placeholder, REAL), bytes(placeholder, DISPLAY));
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn block_state_packets_follow_the_mode() {
    let lit = lamp().states[0].id;
    let shown = Block::REDSTONE_LAMP.default_state.id;
    let ghost = gone().default_state.id;
    let pos = BlockPos::new(1, 64, -3);

    let update = |state: BlockStateId| CBlockUpdate::new(pos, VarInt(raw_state(state)));
    assert_follows_mode(&update(lit), &update(shown), &update(ghost));
    assert!(bytes(&update(lit), REAL).ends_with(&real_state(lit)));

    let multi = |state: BlockStateId| CMultiBlockUpdate::new(&[(pos, state)]);
    assert_follows_mode(&multi(lit), &multi(shown), &multi(ghost));

    let destroy = WorldEvent::ParticlesAndSoundDestroyBlock as i32;
    let world_event = |state| CWorldEvent::new(destroy, pos, raw_state(state), false);
    let level_event = |state| CLevelEvent::new(destroy, pos, raw_state(state), false);
    assert_follows_mode(&world_event(lit), &world_event(shown), &world_event(ghost));
    assert_follows_mode(&level_event(lit), &level_event(shown), &level_event(ghost));

    let state_data = |state| var_int(raw_state(state));
    let particle = |data: &[u8], key: EncodingKey| {
        let mut buf = Vec::new();
        CParticle::new(
            false,
            false,
            Vector3::new(0.5, 65.0, 0.5),
            Vector3::new(0.1, 0.1, 0.1),
            0.0,
            4,
            VarInt(i32::from(Particle::Block.to_id())),
            data,
        )
        .write_packet_data(&mut buf, &key)
        .unwrap();
        buf
    };
    assert_eq!(
        particle(&state_data(lit), DISPLAY),
        particle(&state_data(shown), DISPLAY)
    );
    assert_ne!(
        particle(&state_data(lit), REAL),
        particle(&state_data(shown), REAL)
    );
    assert!(contains(
        &particle(&state_data(lit), REAL),
        &real_state(lit)
    ));
    assert_eq!(
        particle(&state_data(ghost), REAL),
        particle(&state_data(ghost), DISPLAY)
    );

    let metadata = |state: BlockStateId, key: EncodingKey| {
        let mut buf = Vec::new();
        Metadata::new(block_display::DATA_BLOCK_STATE_ID, VarInt(raw_state(state)))
            .write(&mut buf, &key)
            .unwrap();
        buf
    };
    assert_eq!(metadata(lit, DISPLAY), metadata(shown, DISPLAY));
    assert!(metadata(lit, REAL).ends_with(&real_state(lit)));
    assert_eq!(metadata(ghost, REAL), metadata(ghost, DISPLAY));
}

#[test]
fn block_events_follow_the_mode() {
    let pos = BlockPos::new(0, 70, 0);
    let event = |block: &Block| CBlockEvent::new(pos, 1, 2, VarInt(i32::from(block.id.as_u16())));
    let lamp = lamp();
    assert_follows_mode(&event(lamp), &event(&Block::REDSTONE_LAMP), &event(gone()));
    assert!(bytes(&event(lamp), REAL).ends_with(&var_int(i32::from(lamp.id.as_u16()))));
}

fn spawn(entity_type: &EntityType, data: i32) -> CSpawnEntity {
    CSpawnEntity::new(
        VarInt(7),
        uuid::Uuid::nil(),
        VarInt(i32::from(entity_type.id)),
        Vector3::new(0.5, 70.0, 0.5),
        0.0,
        0.0,
        0.0,
        VarInt(data),
        Vector3::new(0.0, 0.0, 0.0),
    )
}

#[test]
fn entity_spawns_follow_the_mode() {
    let golem = entity_type("test:golem");
    assert_follows_mode(
        &spawn(golem, 0),
        &spawn(&EntityType::ZOMBIE, 0),
        &spawn(entity_type("test:shade"), 0),
    );
    assert!(golem.id >= EntityType::COUNT);
    assert!(contains(
        &bytes(&spawn(golem, 0), REAL),
        &var_int(i32::from(golem.id))
    ));

    // The data of a falling block is a block state.
    let lit = raw_state(lamp().states[0].id);
    let shown = raw_state(Block::REDSTONE_LAMP.default_state.id);
    let falling = |state| spawn(&EntityType::FALLING_BLOCK, state);
    assert_eq!(
        bytes(&falling(lit), DISPLAY),
        bytes(&falling(shown), DISPLAY)
    );
    assert_ne!(bytes(&falling(lit), REAL), bytes(&falling(shown), REAL));
}

fn item_stack_bytes(stack: &ItemStack, key: EncodingKey) -> Vec<u8> {
    let serializer = ItemStackSerializer(Cow::Borrowed(stack));
    let mut out = Vec::new();
    serializer.write_with_version(&mut out, &key).unwrap();
    serializer
        .write_length_prefixed_with_version(&mut out, &key)
        .unwrap();
    serializer
        .write_item_cost_with_version(&mut out, &key)
        .unwrap();
    serializer
        .write_template_with_version(&mut out, &key)
        .unwrap();
    out
}

#[test]
fn item_stacks_follow_the_mode() {
    let ruby = item("test:ruby");
    let ghost = item("test:ghost");
    let custom = ItemStack::new(3, ruby);
    assert_eq!(
        item_stack_bytes(&custom, DISPLAY),
        item_stack_bytes(&ItemStack::new(3, &Item::DIAMOND), DISPLAY)
    );
    let real = item_stack_bytes(&custom, REAL);
    assert!(real.starts_with(&[var_int(3), var_int(i32::from(ruby.id))].concat()));
    let placeholder = ItemStack::new(3, ghost);
    assert_eq!(
        item_stack_bytes(&placeholder, REAL),
        item_stack_bytes(&ItemStack::new(3, &Item::EMERALD), DISPLAY)
    );
}

fn component(id: DataComponent, value: &dyn DataComponentImpl, ids: ContentIds) -> Vec<u8> {
    let mut out = Vec::new();
    serialize(id, value, &mut out, ids).unwrap();
    out
}

#[test]
fn data_components_follow_the_mode() {
    let ruby = item("test:ruby");
    let container = |item: &'static Item| ContainerImpl {
        items: vec![(0, ItemStack::new(1, item))],
    };
    assert_eq!(
        component(
            DataComponent::Container,
            &container(ruby),
            ContentIds::Display
        ),
        component(
            DataComponent::Container,
            &container(&Item::DIAMOND),
            ContentIds::Display
        )
    );
    let real = component(DataComponent::Container, &container(ruby), ContentIds::Real);
    assert!(contains(&real, &var_int(i32::from(ruby.id))));

    let repairable = |item: &'static Item| RepairableImpl {
        items: IDSet::IDs(Cow::Owned(vec![item])),
    };
    assert_eq!(
        component(
            DataComponent::Repairable,
            &repairable(ruby),
            ContentIds::Display
        ),
        component(
            DataComponent::Repairable,
            &repairable(&Item::DIAMOND),
            ContentIds::Display
        )
    );
    assert_eq!(
        component(
            DataComponent::Repairable,
            &repairable(ruby),
            ContentIds::Real
        ),
        [var_int(2), var_int(i32::from(ruby.id))].concat()
    );

    let golem = entity_type("test:golem");
    let entity_data = |name: &str| {
        let mut nbt = NbtCompound::new();
        nbt.put_string("id", name.to_string());
        EntityDataImpl { nbt: Some(nbt) }
    };
    assert_eq!(
        component(
            DataComponent::EntityData,
            &entity_data("test:golem"),
            ContentIds::Display
        ),
        component(
            DataComponent::EntityData,
            &entity_data("minecraft:zombie"),
            ContentIds::Display
        )
    );
    assert!(
        component(
            DataComponent::EntityData,
            &entity_data("test:golem"),
            ContentIds::Real
        )
        .starts_with(&var_int(i32::from(golem.id)))
    );
}

#[test]
fn recipe_book_follows_the_mode() {
    let ruby = item("test:ruby");
    let recipe = |result: &str| {
        [DynamicRecipe::Crafting(OwnedCraftingRecipe::Shapeless {
            recipe_id: None,
            category: RecipeCategoryTypes::Misc,
            group: None,
            ingredients: vec![OwnedRecipeIngredient::Simple("minecraft:stick".to_string())],
            result: OwnedRecipeResult {
                item_id: result.to_string(),
                count: 1,
            },
        })]
    };
    let book =
        |result: &str, key: EncodingKey| bytes(&CRecipeBookAdd::new(false, &recipe(result)), key);
    assert_eq!(
        book("test:ruby", DISPLAY),
        book("minecraft:diamond", DISPLAY)
    );
    // Vanilla recipes with a tag ingredient also list the ruby for a `Real` client, as a member
    // of the tags of its display item, so compare within one mode.
    let real = book("test:ruby", REAL);
    assert_ne!(real, book("minecraft:diamond", REAL));
    assert!(contains(&real, &var_int(i32::from(ruby.id))));
    assert_eq!(book("test:ghost", REAL), book("minecraft:emerald", REAL));
}

/// The block tags of an `update_tags` body: tag name -> member ids.
fn block_tags(key: EncodingKey) -> BTreeMap<String, Vec<i32>> {
    content();
    let body = bytes(&CUpdateTags::new(&[RegistryKey::Block]), key);
    let mut read = body.as_slice();
    assert_eq!(read.get_var_int().unwrap().0, 1);
    assert_eq!(&*read.get_str().unwrap(), "minecraft:block");
    let count = read.get_var_int().unwrap().0;
    let mut tags = BTreeMap::new();
    for _ in 0..count {
        let name = read.get_str().unwrap().to_string();
        let len = read.get_var_int().unwrap().0;
        let ids = (0..len).map(|_| read.get_var_int().unwrap().0).collect();
        tags.insert(name, ids);
    }
    assert!(read.is_empty());
    tags
}

#[test]
fn update_tags_follow_the_mode() {
    let lamp = i32::from(lamp().id.as_u16());
    let gone = i32::from(gone().id.as_u16());
    let redstone_lamp = i32::from(Block::REDSTONE_LAMP.id.as_u16());
    let display = block_tags(DISPLAY);
    let real = block_tags(REAL);

    // Display: the generated lists only.
    let pickaxe = display
        .iter()
        .find(|(name, _)| name.ends_with("mineable/pickaxe"))
        .map(|(name, _)| name.clone())
        .unwrap();
    assert!(!display.contains_key("test:lamps"));
    assert!(
        display
            .values()
            .flatten()
            .all(|&id| id < i32::from(pumpkin_data::BlockId::COUNT))
    );

    // Real: the generated lists plus the custom members, without the placeholder, and the mod
    // tag.
    assert_eq!(real.len(), display.len() + 1);
    let mut expected = display[&pickaxe].clone();
    expected.push(lamp);
    assert_eq!(real[&pickaxe], expected);
    assert!(!real.values().flatten().any(|&id| id == gone));
    assert_eq!(real["test:lamps"], [redstone_lamp, lamp]);
    for (name, ids) in &display {
        assert!(real[name].starts_with(ids));
    }
}
