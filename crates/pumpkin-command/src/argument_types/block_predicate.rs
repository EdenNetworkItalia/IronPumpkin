use pumpkin_data::dynamic::{DynamicTaggable, tag_ids};
use pumpkin_data::tag::RegistryKey;
use pumpkin_data::{Block, BlockStateId, translation};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::text::TextComponent;

use crate::argument_types::argument_type::{ArgumentType, JavaClientArgumentType};
use crate::argument_types::block::{
    DUPLICATE_PROPERTY_ERROR_TYPE, EXPECTED_VALUE_ERROR_TYPE, UNCLOSED_PROPERTIES_ERROR_TYPE,
    find_value, property_schema, read_block, read_nbt, read_properties,
};
use crate::argument_types::nbt_path::compare_nbt;
use crate::context::command_context::CommandContext;
use crate::errors::command_syntax_error::CommandSyntaxError;
use crate::errors::error_types::CommandErrorType;
use crate::string_reader::StringReader;
use crate::suggestion::suggestions::{Suggestions, SuggestionsBuilder};

pub const ERROR_UNKNOWN_TAG: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ARGUMENTS_BLOCK_TAG_UNKNOWN,
    translation::java::ARGUMENTS_BLOCK_TAG_UNKNOWN,
);

/// The block id that vanilla `readVagueProperties` puts in its errors. The parser never sets its
/// id for a tag, so the id stays `Identifier.withDefaultNamespace("")`.
const TAG_PROPERTY_ERROR_ID: &str = "minecraft:";

#[derive(Clone, Debug)]
pub enum BlockPredicate {
    /// `id[property=value,...]{nbt}`: the block and the properties given, with their values.
    Block {
        block: &'static Block,
        properties: Vec<(&'static str, &'static str)>,
        nbt: Option<NbtCompound>,
    },
    /// `#tag[property=value,...]{nbt}`: the properties are not validated, each block in the tag
    /// is tested against them.
    Tag {
        tag_name: String,
        properties: Vec<(String, String)>,
        nbt: Option<NbtCompound>,
    },
}

impl BlockPredicate {
    /// Tests a block in the world as vanilla `BlockPredicateArgument` does. `block_entity` returns
    /// the data of the block entity at the position (vanilla `saveWithFullMetadata`); it runs only
    /// when the state matches and the predicate has NBT.
    pub fn test(
        &self,
        state: BlockStateId,
        block_entity: impl FnOnce() -> Option<NbtCompound>,
    ) -> bool {
        self.test_state(state) && (!self.requires_nbt() || self.test_nbt(block_entity().as_ref()))
    }

    /// Tests the NBT part only: `true` when the predicate has no NBT, else the block entity must
    /// exist and contain it.
    #[must_use]
    pub fn test_nbt(&self, block_entity: Option<&NbtCompound>) -> bool {
        let (Self::Block { nbt, .. } | Self::Tag { nbt, .. }) = self;
        nbt.as_ref()
            .is_none_or(|expected| block_entity.is_some_and(|actual| nbt_matches(expected, actual)))
    }

    /// Tests the block and the properties of `state`, without the NBT.
    #[must_use]
    pub fn test_state(&self, state: BlockStateId) -> bool {
        let block = Block::from_state_id(state);
        match self {
            Self::Block {
                block: expected,
                properties,
                ..
            } => {
                if block.id != expected.id {
                    return false;
                }
                if properties.is_empty() {
                    return true;
                }
                let actual = block
                    .properties(state)
                    .map(|actual| actual.to_props())
                    .unwrap_or_default();
                properties.iter().all(|property| actual.contains(property))
            }
            Self::Tag {
                tag_name,
                properties,
                ..
            } => {
                // IronPumpkin: `has_tag_dynamic` also answers for content-registry blocks.
                if !block.has_tag_dynamic(tag_name) {
                    return false;
                }
                if properties.is_empty() {
                    return true;
                }
                let actual = block
                    .properties(state)
                    .map(|actual| actual.to_props())
                    .unwrap_or_default();
                properties.iter().all(|(name, input)| {
                    let Some((_, value)) = actual.iter().find(|(known, _)| known == name) else {
                        return false;
                    };
                    if value == input {
                        return true;
                    }
                    // Vanilla reads the value with `Property.getValue`: `01` names `1`.
                    input.parse::<i32>().is_ok()
                        && property_schema(block)
                            .iter()
                            .find(|(known, _)| known == name)
                            .and_then(|(_, values)| find_value(values, input))
                            == Some(*value)
                })
            }
        }
    }

    #[must_use]
    pub const fn requires_nbt(&self) -> bool {
        match self {
            Self::Block { nbt, .. } | Self::Tag { nbt, .. } => nbt.is_some(),
        }
    }
}

/// Vanilla `NbtUtils.compareNbt(expected, actual, true)` for two compounds.
fn nbt_matches(expected: &NbtCompound, actual: &NbtCompound) -> bool {
    expected.child_tags.iter().all(|(key, expected)| {
        actual
            .child_tags
            .get(key)
            .is_some_and(|actual| compare_nbt(expected, actual))
    })
}

/// Reads `[property=value,...]` after a tag as vanilla `BlockStateParser.readVagueProperties`
/// does: names and values are any string, and only a repeated name is an error.
fn read_vague_properties(
    reader: &mut StringReader,
) -> Result<Vec<(String, String)>, CommandSyntaxError> {
    reader.skip();
    let mut value_start = None;
    let mut properties: Vec<(String, String)> = Vec::new();
    reader.skip_whitespace();
    while reader.can_read_char() && reader.peek() != Some(']') {
        reader.skip_whitespace();
        let name_start = reader.cursor();
        let name = reader.read_string()?;
        if properties.iter().any(|(known, _)| *known == name) {
            reader.set_cursor(name_start);
            return Err(DUPLICATE_PROPERTY_ERROR_TYPE.create(
                reader,
                TextComponent::text(name),
                TextComponent::text(TAG_PROPERTY_ERROR_ID),
            ));
        }
        reader.skip_whitespace();
        if reader.peek() != Some('=') {
            reader.set_cursor(name_start);
            return Err(EXPECTED_VALUE_ERROR_TYPE.create(
                reader,
                TextComponent::text(name),
                TextComponent::text(TAG_PROPERTY_ERROR_ID),
            ));
        }
        reader.skip();
        reader.skip_whitespace();
        value_start = Some(reader.cursor());
        let value = reader.read_string()?;
        properties.push((name, value));
        reader.skip_whitespace();
        if reader.can_read_char() {
            value_start = None;
            match reader.peek() {
                Some(',') => reader.skip(),
                Some(']') => break,
                _ => return Err(UNCLOSED_PROPERTIES_ERROR_TYPE.create(reader)),
            }
        }
    }
    if reader.can_read_char() {
        reader.skip();
        return Ok(properties);
    }
    if let Some(value_start) = value_start {
        reader.set_cursor(value_start);
    }
    Err(UNCLOSED_PROPERTIES_ERROR_TYPE.create(reader))
}

pub struct BlockPredicateArgumentType;

impl<S: crate::source::CommandSource> ArgumentType<S> for BlockPredicateArgumentType {
    type Item = BlockPredicate;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Item, CommandSyntaxError> {
        if reader.peek() != Some('#') {
            let (block, id) = read_block(reader)?;
            let properties = if reader.peek() == Some('[') {
                read_properties(reader, block, &id)?
            } else {
                Vec::new()
            };
            let nbt = read_nbt(reader)?;
            return Ok(BlockPredicate::Block {
                block,
                properties,
                nbt,
            });
        }

        let start = reader.cursor();
        reader.skip();
        let tag_start = reader.cursor();
        while let Some(c) = reader.peek() {
            if c.is_alphanumeric() || c == '_' || c == ':' || c == '/' || c == '.' || c == '-' {
                reader.skip();
            } else {
                break;
            }
        }
        let tag_str = &reader.string()[tag_start..reader.cursor()];
        let normalized = if tag_str.contains(':') {
            tag_str.to_string()
        } else {
            format!("minecraft:{tag_str}")
        };
        // IronPumpkin: `tag_ids` also knows the tags of content-registry blocks.
        if tag_ids(RegistryKey::Block, &normalized).is_none() {
            reader.set_cursor(start);
            return Err(ERROR_UNKNOWN_TAG.create(reader, TextComponent::text(normalized)));
        }
        let properties = if reader.peek() == Some('[') {
            read_vague_properties(reader)?
        } else {
            Vec::new()
        };
        let nbt = read_nbt(reader)?;
        Ok(BlockPredicate::Tag {
            tag_name: normalized,
            properties,
            nbt,
        })
    }

    fn client_side_parser(&self) -> JavaClientArgumentType {
        JavaClientArgumentType::BlockPredicate
    }

    fn examples(&self) -> Vec<String> {
        vec![
            "stone".to_string(),
            "minecraft:stone".to_string(),
            "stone[foo=bar]".to_string(),
            "#stone".to_string(),
            "#stone[foo=bar]{baz=nbt}".to_string(),
        ]
    }

    fn list_suggestions(
        &self,
        _context: &CommandContext<S>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        builder.build()
    }
}

impl BlockPredicateArgumentType {
    pub fn get<S: crate::source::CommandSource>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<BlockPredicate, CommandSyntaxError> {
        Ok(context.get_argument::<BlockPredicate>(name)?.clone())
    }
}

#[cfg(test)]
mod test {
    use super::{BlockPredicate, BlockPredicateArgumentType, ERROR_UNKNOWN_TAG};
    use crate::argument_types::argument_type::ArgumentType;
    use crate::argument_types::block::{
        DUPLICATE_PROPERTY_ERROR_TYPE, EXPECTED_VALUE_ERROR_TYPE, INVALID_VALUE_ERROR_TYPE,
        UNCLOSED_PROPERTIES_ERROR_TYPE, UNKNOWN_PROPERTY_ERROR_TYPE,
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
        ArgumentType::<DummySource>::parse(
            &BlockPredicateArgumentType,
            &mut StringReader::new(input),
        )
    }

    fn state(block: &Block, properties: &[(&str, &str)]) -> BlockStateId {
        block.from_properties(properties).to_state_id(block)
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
            &UNKNOWN_PROPERTY_ERROR_TYPE
                .create_without_context(text("minecraft:stone"), text("foo")),
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
            &EXPECTED_VALUE_ERROR_TYPE
                .create_without_context(text("facing"), text("minecraft:oak_stairs")),
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
            &UNCLOSED_PROPERTIES_ERROR_TYPE.create_without_context(),
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
            &EXPECTED_VALUE_ERROR_TYPE.create_without_context(text("axis"), text("minecraft:")),
            6,
        );
        assert_error(
            "#logs[axis=x,axis=y]",
            &DUPLICATE_PROPERTY_ERROR_TYPE.create_without_context(text("axis"), text("minecraft:")),
            13,
        );
        let unclosed = UNCLOSED_PROPERTIES_ERROR_TYPE.create_without_context();
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
}
