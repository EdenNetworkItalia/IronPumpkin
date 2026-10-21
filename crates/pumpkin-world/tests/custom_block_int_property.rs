//! A custom int property above the table of `Property::value_str` survives a chunk save and load
//! and a block state compound. The content registry is process-wide and freezes once, so this file
//! is its own test binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use pumpkin_data::Block;
use pumpkin_data::dynamic::{self, BlockDefinition, BlockPropertyDefinition};
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::vector2::Vector2;
use pumpkin_world::block::state_nbt::{block_state_from_nbt, block_state_to_nbt};
use pumpkin_world::chunk::ChunkData;
use pumpkin_world::chunk::format::anvil::SingleChunkDataSerializer;

#[test]
fn an_int_above_25_survives_a_save_and_a_load() {
    dynamic::register_block(BlockDefinition {
        name: "test:sprout".to_string(),
        display: Block::STONE.default_state.id,
        properties: vec![
            BlockPropertyDefinition::int("age", 0, 30, 0),
            BlockPropertyDefinition::enumeration("color", &["red", "blue"], "red"),
        ],
        tags: Vec::new(),
    })
    .unwrap();
    dynamic::freeze().unwrap();
    let sprout = Block::from_name("test:sprout").unwrap();
    let old = sprout
        .from_string_properties(&[("age", "30"), ("color", "blue")])
        .to_state_id(sprout);
    assert_eq!(
        sprout.string_properties(old).unwrap(),
        [("age", "30"), ("color", "blue")]
    );

    let compound = block_state_to_nbt(old);
    assert_eq!(block_state_from_nbt(&NbtTag::Compound(compound)), Some(old));

    let chunk = ChunkData::empty(0, 0);
    chunk.set_block_absolute_y(0, 64, 0, old);
    let bytes = chunk.to_bytes().unwrap();
    let loaded = ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).unwrap();
    assert_eq!(loaded.section.get_block_absolute_y(0, 64, 0), Some(old));
}
