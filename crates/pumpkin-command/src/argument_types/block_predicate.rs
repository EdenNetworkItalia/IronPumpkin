use std::collections::HashMap;

use pumpkin_data::dynamic::tag_ids;
use pumpkin_data::tag::RegistryKey;
use pumpkin_data::{Block, BlockStateId, translation};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::text::TextComponent;

use crate::argument_types::argument_type::{ArgumentType, JavaClientArgumentType};
use crate::argument_types::block::{
    DUPLICATE_PROPERTY_ERROR_TYPE, INVALID_BLOCK_ERROR_TYPE, find_value, property_schema,
    read_properties,
};
use crate::argument_types::nbt::NbtCompoundArgumentType;
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

pub const ERROR_NO_VALUE: CommandErrorType<2> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_NOVALUE,
    translation::java::ARGUMENT_BLOCK_PROPERTY_NOVALUE,
);

pub const ERROR_UNCLOSED_PROPERTIES: CommandErrorType<0> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNCLOSED,
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNCLOSED,
);

/// The block id in the errors of vanilla `readVagueProperties`: the parser never sets its id for a
/// tag, so it stays `Identifier.withDefaultNamespace("")`.
const TAG_PROPERTY_ERROR_ID: &str = "minecraft:";

#[derive(Clone, Debug)]
pub enum BlockPredicate {
    Block {
        block: &'static Block,
        properties: HashMap<String, String>,
        nbt: Option<NbtCompound>,
    },
    Tag {
        tag_name: String,
        block_ids: Vec<u16>,
        properties: HashMap<String, String>,
        nbt: Option<NbtCompound>,
    },
}

impl BlockPredicate {
    /// Tests a block state; `block_entity_nbt` is only called when the predicate has NBT.
    #[must_use]
    pub fn test(
        &self,
        state_id: BlockStateId,
        block_entity_nbt: impl FnOnce() -> Option<NbtCompound>,
    ) -> bool {
        let block = Block::from_state_id(state_id);
        let (matches_block, properties, nbt) = match self {
            Self::Block {
                block: expected,
                properties,
                nbt,
            } => (block.id == expected.id, properties, nbt),
            Self::Tag {
                block_ids,
                properties,
                nbt,
                ..
            } => (block_ids.contains(&block.id.as_u16()), properties, nbt),
        };
        if !matches_block {
            return false;
        }

        if !properties.is_empty() {
            // IronPumpkin: `string_properties` also has the properties of content-registry blocks.
            let Some(state_properties) = block.string_properties(state_id) else {
                return false;
            };
            let all_match = properties.iter().all(|(key, value)| {
                state_properties.iter().any(|(name, actual)| {
                    name == key && (actual == value || same_value(block, name, actual, value))
                })
            });
            if !all_match {
                return false;
            }
        }

        nbt.as_ref().is_none_or(|expected| {
            block_entity_nbt().is_some_and(|actual| {
                compare_nbt(
                    &NbtTag::Compound(expected.clone()),
                    &NbtTag::Compound(actual),
                )
            })
        })
    }

    #[must_use]
    pub const fn requires_nbt(&self) -> bool {
        match self {
            Self::Block { nbt, .. } | Self::Tag { nbt, .. } => nbt.is_some(),
        }
    }
}

/// Whether `input` names the value `actual` as vanilla `Property.getValue` reads a tag property:
/// an integer property uses `Integer.parseInt`, so `+2` and `02` name `2`.
fn same_value(block: &Block, property: &str, actual: &str, input: &str) -> bool {
    input.parse::<i32>().is_ok()
        && property_schema(block)
            .iter()
            .find(|(name, _)| *name == property)
            .and_then(|(_, values)| find_value(values, input))
            == Some(actual)
}

/// Reads `[property=value,...]` after a tag as vanilla `BlockStateParser.readVagueProperties`
/// does: names and values are any string, and only a repeated name is an error.
fn parse_properties(
    reader: &mut StringReader,
) -> Result<HashMap<String, String>, CommandSyntaxError> {
    let mut properties = HashMap::new();
    if reader.peek() == Some('[') {
        reader.skip();
        let mut start_val = None;
        reader.skip_whitespace();
        while reader.can_read_char() && reader.peek() != Some(']') {
            reader.skip_whitespace();
            let start_key = reader.cursor();
            let key = reader.read_string()?;
            if properties.contains_key(&key) {
                reader.set_cursor(start_key);
                return Err(DUPLICATE_PROPERTY_ERROR_TYPE.create(
                    reader,
                    TextComponent::text(key),
                    TextComponent::text(TAG_PROPERTY_ERROR_ID),
                ));
            }
            reader.skip_whitespace();
            if reader.peek() != Some('=') {
                reader.set_cursor(start_key);
                return Err(ERROR_NO_VALUE.create(
                    reader,
                    TextComponent::text(key),
                    TextComponent::text(TAG_PROPERTY_ERROR_ID),
                ));
            }
            reader.skip();
            reader.skip_whitespace();
            start_val = Some(reader.cursor());
            let val = reader.read_string()?;
            properties.insert(key, val);
            reader.skip_whitespace();
            if reader.can_read_char() {
                start_val = None;
                if reader.peek() == Some(',') {
                    reader.skip();
                } else if reader.peek() == Some(']') {
                    break;
                } else {
                    return Err(ERROR_UNCLOSED_PROPERTIES.create(reader));
                }
            }
        }
        if reader.peek() == Some(']') {
            reader.skip();
        } else {
            if let Some(start_val) = start_val {
                reader.set_cursor(start_val);
            }
            return Err(ERROR_UNCLOSED_PROPERTIES.create(reader));
        }
    }
    Ok(properties)
}

pub(crate) fn parse_nbt(
    reader: &mut StringReader,
) -> Result<Option<NbtCompound>, CommandSyntaxError> {
    if reader.peek() == Some('{') {
        let tag =
            ArgumentType::<crate::source::DummySource>::parse(&NbtCompoundArgumentType, reader)?;
        Ok(Some(tag))
    } else {
        Ok(None)
    }
}

pub struct BlockPredicateArgumentType;

impl<S: crate::source::CommandSource> ArgumentType<S> for BlockPredicateArgumentType {
    type Item = BlockPredicate;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Item, CommandSyntaxError> {
        if reader.peek() == Some('#') {
            let tag_start = reader.cursor();
            reader.skip();
            let start = reader.cursor();
            while let Some(c) = reader.peek() {
                if c.is_alphanumeric() || c == '_' || c == ':' || c == '/' || c == '.' || c == '-' {
                    reader.skip();
                } else {
                    break;
                }
            }
            let tag_str = &reader.string()[start..reader.cursor()];
            let normalized = if tag_str.contains(':') {
                tag_str.to_string()
            } else {
                format!("minecraft:{tag_str}")
            };
            // IronPumpkin: `tag_ids` also has the content-registry members of the tag.
            let Some((generated, custom)) = tag_ids(RegistryKey::Block, &normalized) else {
                reader.set_cursor(tag_start);
                return Err(ERROR_UNKNOWN_TAG.create(reader, TextComponent::text(normalized)));
            };
            let block_ids = generated.iter().chain(custom).copied().collect();

            let properties = parse_properties(reader)?;
            let nbt = parse_nbt(reader)?;

            Ok(BlockPredicate::Tag {
                tag_name: normalized,
                block_ids,
                properties,
                nbt,
            })
        } else {
            let start = reader.cursor();
            while let Some(c) = reader.peek() {
                if c.is_alphanumeric() || c == '_' || c == ':' || c == '/' || c == '.' || c == '-' {
                    reader.skip();
                } else {
                    break;
                }
            }
            let block_str = &reader.string()[start..reader.cursor()];
            let normalized = if block_str.contains(':') {
                block_str.to_string()
            } else {
                format!("minecraft:{block_str}")
            };

            let block = Block::from_name(&normalized).ok_or_else(|| {
                reader.set_cursor(start);
                INVALID_BLOCK_ERROR_TYPE.create(reader, TextComponent::text(normalized.clone()))
            })?;

            let properties = if reader.peek() == Some('[') {
                read_properties(reader, block, &normalized)?
                    .into_iter()
                    .map(|(name, value)| (name.to_string(), value.to_string()))
                    .collect()
            } else {
                HashMap::new()
            };
            let nbt = parse_nbt(reader)?;

            Ok(BlockPredicate::Block {
                block,
                properties,
                nbt,
            })
        }
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
        ArgumentType::<DummySource>::parse(
            &BlockPredicateArgumentType,
            &mut StringReader::new(input),
        )
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
}
