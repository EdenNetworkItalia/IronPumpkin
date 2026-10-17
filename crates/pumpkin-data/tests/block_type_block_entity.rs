//! A typed block whose block type has a block entity needs that block entity too. An integration
//! test has its own process, so it can install the process-wide content tables.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use pumpkin_data::block_properties::BLOCK_ENTITY_TYPES;
use pumpkin_data::dynamic::{self, BlockDefinition};
use pumpkin_data::{Block, BlockId, dynamic_types};

fn register(name: &str, block_type: &str) {
    dynamic_types::register_block(
        BlockDefinition {
            name: name.to_string(),
            display: Block::BARREL.default_state.id,
            properties: Vec::new(),
            tags: Vec::new(),
        },
        block_type,
    )
    .unwrap();
}

fn missing(name: &str) -> Option<BlockId> {
    dynamic_types::block_type_without_block_entity(Block::from_name(name).unwrap())
        .map(|block_type| block_type.id)
}

#[test]
fn a_typed_block_without_the_block_entity_of_its_type_is_found() {
    register("test:crate", "minecraft:barrel");
    register("test:barrel", "minecraft:barrel");
    register("test:stairs", "minecraft:oak_stairs");
    let barrel = BLOCK_ENTITY_TYPES
        .iter()
        .position(|ty| *ty == "barrel")
        .unwrap();
    assert!(
        dynamic_types::add_block_entity_block("test:barrel", u16::try_from(barrel).unwrap())
            .unwrap()
    );
    dynamic::freeze().unwrap();

    assert_eq!(missing("test:crate"), Some(Block::BARREL.id));
    assert_eq!(missing("test:barrel"), None);
    assert_eq!(missing("test:stairs"), None);
    assert_eq!(missing("minecraft:barrel"), None);
}
