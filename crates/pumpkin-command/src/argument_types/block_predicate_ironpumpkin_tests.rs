use super::{
    BlockPredicate, BlockPredicateArgumentType, ERROR_NO_VALUE, ERROR_UNCLOSED_PROPERTIES,
    ERROR_UNKNOWN_TAG,
};
use crate::argument_types::argument_type::ArgumentType;
use crate::argument_types::block::{
    DUPLICATE_PROPERTY_ERROR_TYPE, INVALID_VALUE_ERROR_TYPE, UNKNOWN_PROPERTY_ERROR_TYPE,
};
use crate::argument_types::nbt::NbtCompoundArgumentType;
use crate::errors::command_syntax_error::CommandSyntaxError;
use crate::source::DummySource;
use crate::string_reader::StringReader;
use pumpkin_data::{Block, BlockStateId};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::text::TextComponent;
use std::cell::Cell;

fn parse(input: &str) -> Result<BlockPredicate, CommandSyntaxError> {
    ArgumentType::<DummySource>::parse(&BlockPredicateArgumentType, &mut StringReader::new(input))
}

fn state(block: &Block, properties: &[(&str, &str)]) -> BlockStateId {
    block.from_string_properties(properties).to_state_id(block)
}

fn snbt(input: &str) -> NbtCompound {
    ArgumentType::<DummySource>::parse(&NbtCompoundArgumentType, &mut StringReader::new(input))
        .unwrap()
}

/// Tests a state that has no block entity.
fn test_state(predicate: &str, state: BlockStateId) -> bool {
    parse(predicate).unwrap().test(state, || None)
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
fn block_matches_the_given_properties_only() {
    let east = state(&Block::OAK_STAIRS, &[("facing", "east"), ("half", "top")]);
    let west = state(&Block::OAK_STAIRS, &[("facing", "west")]);
    assert!(test_state("oak_stairs[facing=east]", east));
    assert!(!test_state("oak_stairs[facing=east]", west));
    assert!(test_state("oak_stairs[facing=east,half=top]", east));
    assert!(!test_state("oak_stairs[facing=east,half=bottom]", east));
    // A property that is not given matches every value.
    assert!(test_state("minecraft:oak_stairs", east));
    assert!(test_state("oak_stairs[]", west));
    // Another block with the same property never matches.
    let spruce = state(&Block::SPRUCE_STAIRS, &[("facing", "east")]);
    assert!(!test_state("oak_stairs[facing=east]", spruce));
    assert!(!test_state("oak_stairs", Block::STONE.default_state.id));
    // Integer values are read like vanilla `Integer.parseInt`.
    let candles = state(&Block::CANDLE, &[("candles", "3")]);
    assert!(test_state("candle[candles=03]", candles));
    assert!(!test_state("candle[candles=2]", candles));
}

#[test]
fn tag_tests_membership_and_vague_properties() {
    let log_x = state(&Block::OAK_LOG, &[("axis", "x")]);
    let log_y = state(&Block::OAK_LOG, &[("axis", "y")]);
    assert!(test_state("#minecraft:logs", log_x));
    assert!(test_state("#logs[axis=x]", log_x));
    assert!(!test_state("#logs[axis=x]", log_y));
    assert!(!test_state("#logs", Block::STONE.default_state.id));
    // Tag properties are not validated: a missing property or an unknown value is false.
    assert!(!test_state("#logs[facing=east]", log_x));
    assert!(!test_state("#logs[axis=up]", log_x));
    let candles = state(&Block::CANDLE, &[("candles", "2")]);
    assert!(test_state("#candles[candles=+2]", candles));
    assert!(test_state("#candles[candles=\"02\"]", candles));
    assert!(!test_state("#candles[candles=3]", candles));
    assert!(!test_state("#candles[candles=two]", candles));
}

#[test]
fn nbt_is_a_partial_match_on_the_block_entity() {
    let chest = Block::CHEST.default_state.id;
    let actual = snbt(
        "{id:\"minecraft:chest\",x:1,y:2,z:3,Lock:\"key\",\
         Items:[{Slot:0b,id:\"minecraft:stone\",count:1},{Slot:1b,id:\"minecraft:dirt\",count:2}]}",
    );
    let matches = |predicate: &str| {
        parse(predicate)
            .unwrap()
            .test(chest, || Some(actual.clone()))
    };
    assert!(matches("chest{Lock:\"key\"}"));
    assert!(matches("chest{}"));
    assert!(matches("#c:chests/wooden{Items:[{id:\"minecraft:dirt\"}]}"));
    assert!(matches("chest[facing=north]{Items:[{Slot:1b},{Slot:0b}]}"));
    assert!(!matches("chest{Lock:\"other\"}"));
    assert!(!matches("chest{Lock:1}"));
    assert!(!matches("chest{Items:[]}"));
    // Vanilla `compareNbt` rejects an actual list shorter than the expected one.
    assert!(!matches("chest{Items:[{Slot:0b},{Slot:0b},{Slot:0b}]}"));
    // No block entity: an NBT predicate is false, a predicate without NBT ignores it.
    assert!(!parse("chest{}").unwrap().test(chest, || None));
    assert!(parse("chest").unwrap().test(chest, || None));
    // The block entity is read only when the state matches and the predicate has NBT.
    let predicate = parse("chest{Lock:\"key\"}").unwrap();
    assert!(predicate.requires_nbt());
    let reads = Cell::new(0);
    let read = || {
        reads.set(reads.get() + 1);
        Some(actual.clone())
    };
    assert!(!predicate.test(Block::STONE.default_state.id, read));
    assert!(parse("chest").unwrap().test(chest, read));
    assert_eq!(reads.get(), 0);
    assert!(predicate.test(chest, read));
    assert_eq!(reads.get(), 1);
}

#[test]
fn block_errors_are_the_block_state_errors() {
    assert_error(
        "stone[foo=bar]",
        &UNKNOWN_PROPERTY_ERROR_TYPE.create_without_context(text("minecraft:stone"), text("foo")),
        6,
    );
    assert_error(
        "oak_stairs[facing=up]",
        &INVALID_VALUE_ERROR_TYPE.create_without_context(
            text("minecraft:oak_stairs"),
            text("up"),
            text("facing"),
        ),
        18,
    );
    assert_error(
        "oak_stairs[facing]",
        &ERROR_NO_VALUE.create_without_context(text("facing"), text("minecraft:oak_stairs")),
        17,
    );
    assert_error(
        "oak_stairs[facing=east,facing=west]",
        &DUPLICATE_PROPERTY_ERROR_TYPE
            .create_without_context(text("facing"), text("minecraft:oak_stairs")),
        23,
    );
    assert_error(
        "oak_stairs[facing=east",
        &ERROR_UNCLOSED_PROPERTIES.create_without_context(),
        22,
    );
}

#[test]
fn tag_errors_follow_read_vague_properties() {
    assert_error(
        "#minecraft:no_such_tag[axis=x]",
        &ERROR_UNKNOWN_TAG.create_without_context(text("minecraft:no_such_tag")),
        0,
    );
    // The id of a tag predicate is `minecraft:`: vanilla never sets it for a tag.
    assert_error(
        "#logs[axis]",
        &ERROR_NO_VALUE.create_without_context(text("axis"), text("minecraft:")),
        6,
    );
    assert_error(
        "#logs[axis=x,axis=y]",
        &DUPLICATE_PROPERTY_ERROR_TYPE.create_without_context(text("axis"), text("minecraft:")),
        13,
    );
    let unclosed = ERROR_UNCLOSED_PROPERTIES.create_without_context();
    assert_error("#logs[", &unclosed, 6);
    assert_error("#logs[axis=x", &unclosed, 11);
    assert_error("#logs[axis=x;]", &unclosed, 12);
    assert_error("#logs[axis=x,", &unclosed, 13);
}

#[test]
fn parse_stops_after_the_predicate() {
    let mut reader = StringReader::new("#logs[axis=x]{} run say");
    ArgumentType::<DummySource>::parse(&BlockPredicateArgumentType, &mut reader).unwrap();
    assert_eq!(reader.cursor(), 15);
    let mut reader = StringReader::new("oak_stairs[facing=east] run say");
    ArgumentType::<DummySource>::parse(&BlockPredicateArgumentType, &mut reader).unwrap();
    assert_eq!(reader.cursor(), 23);
}
