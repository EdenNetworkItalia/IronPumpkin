//! The block predicate tests the properties of a content-registry block against its own schema,
//! by id and through a tag. The content registry is process-wide and freezes once, so this file
//! is its own test binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use pumpkin_command::argument_types::argument_type::ArgumentType;
use pumpkin_command::argument_types::block::{
    INVALID_VALUE_ERROR_TYPE, UNKNOWN_PROPERTY_ERROR_TYPE,
};
use pumpkin_command::argument_types::block_predicate::{
    BlockPredicate, BlockPredicateArgumentType,
};
use pumpkin_command::errors::command_syntax_error::CommandSyntaxError;
use pumpkin_command::source::DummySource;
use pumpkin_command::string_reader::StringReader;
use pumpkin_data::dynamic::{self, BlockDefinition, BlockPropertyDefinition};
use pumpkin_data::{Block, BlockStateId};
use pumpkin_util::text::TextComponent;

fn parse(input: &str) -> Result<BlockPredicate, CommandSyntaxError> {
    ArgumentType::<DummySource>::parse(&BlockPredicateArgumentType, &mut StringReader::new(input))
}

fn matches(predicate: &str, state: BlockStateId) -> bool {
    parse(predicate).unwrap().test(state, || None)
}

fn state(block: &Block, properties: &[(&str, &str)]) -> BlockStateId {
    block.from_properties(properties).to_state_id(block)
}

fn text(value: &str) -> TextComponent {
    TextComponent::text(value.to_string())
}

#[test]
fn custom_block_properties_are_matched_by_id_and_by_tag() {
    dynamic::register_block(BlockDefinition {
        name: "acc-mod:crystal".to_string(),
        display: Block::AMETHYST_BLOCK.default_state.id,
        properties: vec![
            BlockPropertyDefinition::bool("lit", false),
            BlockPropertyDefinition::int("charge", 0, 3, 0),
        ],
        tags: vec!["minecraft:logs".to_string()],
    })
    .unwrap();
    dynamic::freeze().unwrap();

    let crystal = Block::from_name("acc-mod:crystal").unwrap();
    let lit_2 = state(crystal, &[("lit", "true"), ("charge", "2")]);
    let unlit_2 = state(crystal, &[("lit", "false"), ("charge", "2")]);
    assert_ne!(lit_2, unlit_2);

    // By id: only the given properties count.
    assert!(matches("acc-mod:crystal", unlit_2));
    assert!(matches("acc-mod:crystal[lit=true]", lit_2));
    assert!(!matches("acc-mod:crystal[lit=true]", unlit_2));
    assert!(matches("acc-mod:crystal[charge=02,lit=true]", lit_2));
    assert!(!matches("acc-mod:crystal[charge=3]", lit_2));
    assert!(!matches(
        "acc-mod:crystal",
        Block::AMETHYST_BLOCK.default_state.id
    ));
    assert!(!matches("amethyst_block", lit_2));

    // By tag: the custom block joined `minecraft:logs`.
    assert!(matches("#minecraft:logs", lit_2));
    assert!(matches("#logs[lit=true]", lit_2));
    assert!(!matches("#logs[lit=true]", unlit_2));
    assert!(matches("#logs[charge=+2]", unlit_2));
    assert!(!matches("#logs[axis=y]", lit_2));
    assert!(!matches(
        "#logs[lit=true]",
        state(&Block::OAK_LOG, &[("axis", "y")])
    ));
    assert!(matches(
        "#logs[axis=y]",
        state(&Block::OAK_LOG, &[("axis", "y")])
    ));

    // The schema validates the values given by id.
    let error = parse("acc-mod:crystal[charge=4]").unwrap_err();
    assert_eq!(
        error.message,
        INVALID_VALUE_ERROR_TYPE
            .create_without_context(text("acc-mod:crystal"), text("4"), text("charge"))
            .message
    );
    let error = parse("acc-mod:crystal[axis=y]").unwrap_err();
    assert_eq!(
        error.message,
        UNKNOWN_PROPERTY_ERROR_TYPE
            .create_without_context(text("acc-mod:crystal"), text("axis"))
            .message
    );

    // A vanilla block after the freeze.
    let east = state(&Block::OAK_STAIRS, &[("facing", "east")]);
    assert!(matches("oak_stairs[facing=east]", east));
    assert!(!matches("oak_stairs[facing=west]", east));
}
