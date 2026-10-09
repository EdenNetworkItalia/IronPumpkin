//! The block argument parses the properties of a content-registry block against its own schema.
//! The content registry is process-wide and freezes once, so this file is its own test binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use pumpkin_command::argument_types::argument_type::ArgumentType;
use pumpkin_command::argument_types::block::{
    BlockArgumentType, BlockStateArgument, INVALID_VALUE_ERROR_TYPE, UNKNOWN_PROPERTY_ERROR_TYPE,
};
use pumpkin_command::errors::command_syntax_error::CommandSyntaxError;
use pumpkin_command::source::DummySource;
use pumpkin_command::string_reader::StringReader;
use pumpkin_data::dynamic::{self, BlockDefinition, BlockPropertyDefinition};
use pumpkin_data::{Block, BlockStateId};
use pumpkin_util::text::TextComponent;

fn parse(input: &str) -> Result<BlockStateArgument, CommandSyntaxError> {
    ArgumentType::<DummySource>::parse(&BlockArgumentType, &mut StringReader::new(input))
}

/// The property values of `state`, sorted by name.
fn props(block: &Block, state: BlockStateId) -> Vec<(&'static str, &'static str)> {
    let mut props = block.properties(state).unwrap().to_props();
    props.sort_unstable();
    props
}

fn text(value: &str) -> TextComponent {
    TextComponent::text(value.to_string())
}

#[test]
fn custom_block_properties_follow_the_registered_schema() {
    dynamic::register_block(BlockDefinition {
        name: "acc-mod:crystal".to_string(),
        display: Block::AMETHYST_BLOCK.default_state.id,
        properties: vec![
            BlockPropertyDefinition::bool("lit", false),
            BlockPropertyDefinition::int("charge", 0, 3, 0),
            BlockPropertyDefinition::enumeration("tint", &["clear", "rose", "smoke"], "clear"),
        ],
        tags: Vec::new(),
    })
    .unwrap();
    dynamic::register_block(BlockDefinition {
        name: "acc-mod:plain".to_string(),
        display: Block::STONE.default_state.id,
        properties: Vec::new(),
        tags: Vec::new(),
    })
    .unwrap();
    dynamic::freeze().unwrap();

    let crystal = Block::from_name("acc-mod:crystal").unwrap();

    let parsed = parse("acc-mod:crystal").unwrap();
    assert_eq!(parsed.block, crystal);
    assert_eq!(parsed.state, crystal.default_state.id);

    let parsed = parse("acc-mod:crystal[lit=true,charge=2]").unwrap();
    assert_eq!(parsed.block, crystal);
    assert_eq!(
        props(crystal, parsed.state),
        [("charge", "2"), ("lit", "true"), ("tint", "clear")]
    );
    let parsed = parse("acc-mod:crystal[tint=smoke,charge=03]").unwrap();
    assert_eq!(
        props(crystal, parsed.state),
        [("charge", "3"), ("lit", "false"), ("tint", "smoke")]
    );

    let error = parse("acc-mod:crystal[charge=4]").unwrap_err();
    assert_eq!(
        error.message,
        INVALID_VALUE_ERROR_TYPE
            .create_without_context(text("acc-mod:crystal"), text("4"), text("charge"))
            .message
    );
    let error = parse("acc-mod:crystal[tint=blue]").unwrap_err();
    assert_eq!(
        error.message,
        INVALID_VALUE_ERROR_TYPE
            .create_without_context(text("acc-mod:crystal"), text("blue"), text("tint"))
            .message
    );
    let error = parse("acc-mod:crystal[power=1]").unwrap_err();
    assert_eq!(
        error.message,
        UNKNOWN_PROPERTY_ERROR_TYPE
            .create_without_context(text("acc-mod:crystal"), text("power"))
            .message
    );

    let plain = Block::from_name("acc-mod:plain").unwrap();
    assert_eq!(
        parse("acc-mod:plain[]").unwrap().state,
        plain.default_state.id
    );
    let error = parse("acc-mod:plain[lit=true]").unwrap_err();
    assert_eq!(
        error.message,
        UNKNOWN_PROPERTY_ERROR_TYPE
            .create_without_context(text("acc-mod:plain"), text("lit"))
            .message
    );

    // Generated blocks keep their own schema after the freeze.
    let lamp = parse("redstone_lamp[lit=true]").unwrap();
    assert_eq!(props(&Block::REDSTONE_LAMP, lamp.state), [("lit", "true")]);
}
