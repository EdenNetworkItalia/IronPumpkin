use super::*;
use pumpkin_data::dynamic::{self, BlockDefinition, EntityTypeDefinition};

#[test]
fn names_keep_the_namespace_of_custom_content() {
    dynamic::register_block(BlockDefinition {
        name: "test:lamp".to_string(),
        display: Block::STONE.default_state.id,
        properties: Vec::new(),
        tags: Vec::new(),
    })
    .expect("register test block");
    dynamic::register_entity_type(EntityTypeDefinition {
        name: "test:golem".to_string(),
        display: &EntityType::ZOMBIE,
        dimensions: None,
        eye_height: None,
        tags: Vec::new(),
    })
    .expect("register test entity type");
    dynamic::freeze().expect("freeze registry");

    let lamp = from_wasm_block_name("test:lamp");
    assert_eq!(to_wasm_block_name(lamp), "test:lamp");
    assert_eq!(to_wasm_block_name(&Block::STONE), "minecraft:stone");
    assert_eq!(from_wasm_block_name("minecraft:stone"), &Block::STONE);

    let golem = from_wasm_entity_type("test:golem");
    assert_eq!(to_wasm_entity_type(golem), "test:golem");
    assert_eq!(to_wasm_entity_type(&EntityType::PIG), "minecraft:pig");
    assert_eq!(from_wasm_entity_type("minecraft:pig"), &EntityType::PIG);
}
