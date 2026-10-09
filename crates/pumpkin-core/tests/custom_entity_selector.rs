//! The `type` option of an entity selector accepts custom entity types. The content registry is
//! process-wide and freezes once, so this file is its own test binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use pumpkin_core::command::argument_types::entity_selector::parser::EntitySelectorParser;
use pumpkin_core::command::string_reader::StringReader;
use pumpkin_data::dynamic::{self, EntityTypeDefinition};
use pumpkin_data::entity::EntityType;

fn parse(selector: &str) -> bool {
    let mut reader = StringReader::new(selector);
    EntitySelectorParser::new(&mut reader, true)
        .parse_and_consume()
        .is_ok()
}

#[test]
fn type_option_resolves_custom_entity_types() {
    dynamic::register_entity_type(EntityTypeDefinition {
        name: "test:golem".to_string(),
        display: &EntityType::ZOMBIE,
        dimensions: None,
        eye_height: None,
    })
    .unwrap();
    dynamic::freeze().unwrap();

    assert!(parse("@e[type=test:golem]"));
    assert!(parse("@e[type=!test:golem]"));
    assert!(!parse("@e[type=test:missing]"));
    assert!(!parse("@e[type=other:golem]"));

    assert!(parse("@e[type=iron_golem]"));
    assert!(parse("@e[type=minecraft:iron_golem]"));
    assert!(!parse("@e[type=minecraft:golem]"));
}
