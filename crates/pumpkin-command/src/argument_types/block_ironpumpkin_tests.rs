use super::{
    BlockArgumentType, BlockStateArgument, DUPLICATE_PROPERTY_ERROR_TYPE, INVALID_BLOCK_ERROR_TYPE,
    INVALID_VALUE_ERROR_TYPE, TAG_DISALLOWED_ERROR_TYPE, UNKNOWN_PROPERTY_ERROR_TYPE,
};
use crate::argument_types::argument_type::ArgumentType;
use crate::argument_types::block_predicate::{ERROR_NO_VALUE, ERROR_UNCLOSED_PROPERTIES};
use crate::errors::command_syntax_error::CommandSyntaxError;
use crate::source::DummySource;
use crate::string_reader::StringReader;
use pumpkin_data::Block;
use pumpkin_util::text::TextComponent;

fn parse(input: &str) -> Result<(BlockStateArgument, usize), CommandSyntaxError> {
    let mut reader = StringReader::new(input);
    ArgumentType::<DummySource>::parse(&BlockArgumentType, &mut reader)
        .map(|parsed| (parsed, reader.cursor()))
}

/// The property values of the parsed state, sorted by name.
fn props(parsed: &BlockStateArgument) -> Vec<(&'static str, &'static str)> {
    let mut props = parsed
        .block
        .string_properties(parsed.state)
        .unwrap_or_default();
    props.sort_unstable();
    props
}

fn text(value: &str) -> TextComponent {
    TextComponent::text(value.to_string())
}

/// Asserts the error type, the message with its translation arguments and the cursor.
fn assert_error(input: &str, expected: &CommandSyntaxError, cursor: usize) {
    let error = parse(input).expect_err(input);
    assert_eq!(error.error_type, expected.error_type, "{input}");
    assert_eq!(error.message, expected.message, "{input}");
    assert_eq!(
        error.context.map(|context| context.cursor),
        Some(cursor),
        "{input}"
    );
}

#[test]
fn parses_a_block_without_properties() {
    let (parsed, cursor) = parse("stone").unwrap();
    assert_eq!(parsed.state, Block::STONE.default_state.id);
    assert_eq!(cursor, 5);

    let (parsed, _) = parse("minecraft:stone[]").unwrap();
    assert_eq!(parsed.state, Block::STONE.default_state.id);
}

#[test]
fn parses_properties_into_the_state() {
    let (parsed, cursor) = parse("minecraft:redstone_lamp[lit=true] keep").unwrap();
    assert_eq!(parsed.block, &Block::REDSTONE_LAMP);
    assert_eq!(props(&parsed), [("lit", "true")]);
    assert_ne!(parsed.state, Block::REDSTONE_LAMP.default_state.id);
    assert_eq!(cursor, 33);

    let (parsed, _) = parse("hopper[ facing = east , enabled=false ]").unwrap();
    assert_eq!(props(&parsed), [("enabled", "false"), ("facing", "east")]);

    // Unset properties keep the default.
    let (parsed, _) = parse("light[level=3]").unwrap();
    assert_eq!(props(&parsed), [("level", "3"), ("waterlogged", "false")]);

    // Vanilla reads values with `readString` and integers with `Integer.parseInt`.
    let (parsed, _) = parse("light[level=\"07\",waterlogged=\"true\"]").unwrap();
    assert_eq!(props(&parsed), [("level", "7"), ("waterlogged", "true")]);
    let (parsed, _) = parse("light[level=+12]").unwrap();
    assert_eq!(props(&parsed), [("level", "12"), ("waterlogged", "false")]);
}

#[test]
fn parses_block_entity_nbt() {
    let (parsed, cursor) = parse("chest[facing=west]{Lock:\"key\"} replace").unwrap();
    assert_eq!(parsed.block, &Block::CHEST);
    assert_eq!(parsed.nbt.unwrap().get_string("Lock"), Some("key"));
    assert_eq!(cursor, 30);
    assert!(parse("chest{Lock:").is_err());
}

#[test]
fn unknown_block() {
    assert_error(
        "setblock_nothing[lit=true]",
        &INVALID_BLOCK_ERROR_TYPE.create_without_context(text("minecraft:setblock_nothing")),
        0,
    );
}

#[test]
fn tags_are_disallowed() {
    assert_error(
        "#minecraft:logs",
        &TAG_DISALLOWED_ERROR_TYPE.create_without_context(),
        0,
    );
}

#[test]
fn unknown_property() {
    assert_error(
        "redstone_lamp[lit=true,power=3]",
        &UNKNOWN_PROPERTY_ERROR_TYPE
            .create_without_context(text("minecraft:redstone_lamp"), text("power")),
        23,
    );
    assert_error(
        "stone[foo=bar]",
        &UNKNOWN_PROPERTY_ERROR_TYPE.create_without_context(text("minecraft:stone"), text("foo")),
        6,
    );
}

#[test]
fn invalid_value() {
    let lamp = || text("minecraft:redstone_lamp");
    assert_error(
        "redstone_lamp[lit=yes]",
        &INVALID_VALUE_ERROR_TYPE.create_without_context(lamp(), text("yes"), text("lit")),
        18,
    );
    assert_error(
        "redstone_lamp[lit=]",
        &INVALID_VALUE_ERROR_TYPE.create_without_context(lamp(), text(""), text("lit")),
        18,
    );
    assert_error(
        "light[level=16]",
        &INVALID_VALUE_ERROR_TYPE.create_without_context(
            text("minecraft:light"),
            text("16"),
            text("level"),
        ),
        12,
    );
    assert_error(
        "hopper[facing=up]",
        &INVALID_VALUE_ERROR_TYPE.create_without_context(
            text("minecraft:hopper"),
            text("up"),
            text("facing"),
        ),
        14,
    );
}

#[test]
fn duplicate_property() {
    assert_error(
        "redstone_lamp[lit=true,lit=false]",
        &DUPLICATE_PROPERTY_ERROR_TYPE
            .create_without_context(text("lit"), text("minecraft:redstone_lamp")),
        23,
    );
}

#[test]
fn missing_value() {
    let expected =
        ERROR_NO_VALUE.create_without_context(text("lit"), text("minecraft:redstone_lamp"));
    assert_error("redstone_lamp[lit]", &expected, 17);
    assert_error("redstone_lamp[lit ", &expected, 18);
}

#[test]
fn unclosed_properties() {
    let expected = ERROR_UNCLOSED_PROPERTIES.create_without_context();
    assert_error("redstone_lamp[", &expected, 14);
    assert_error("redstone_lamp[lit=true", &expected, 22);
    assert_error("redstone_lamp[lit=true;]", &expected, 22);
    assert_error("redstone_lamp[lit=true,", &expected, 23);
}
