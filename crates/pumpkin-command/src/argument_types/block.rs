use crate::{
    argument_types::{
        argument_type::{ArgumentType, JavaClientArgumentType},
        nbt::NbtCompoundArgumentType,
    },
    context::command_context::CommandContext,
    errors::command_syntax_error::CommandSyntaxError,
    errors::error_types::CommandErrorType,
    string_reader::StringReader,
    suggestion::suggestions::{Suggestions, SuggestionsBuilder},
};
use pumpkin_data::{Block, BlockStateId, translation};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::text::TextComponent;

pub const INVALID_BLOCK_ERROR_TYPE: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_ID_INVALID,
    translation::java::ARGUMENT_BLOCK_ID_INVALID,
);

pub const TAG_DISALLOWED_ERROR_TYPE: CommandErrorType<0> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_TAG_DISALLOWED,
    translation::java::ARGUMENT_BLOCK_TAG_DISALLOWED,
);

pub const UNKNOWN_PROPERTY_ERROR_TYPE: CommandErrorType<2> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNKNOWN,
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNKNOWN,
);

pub const DUPLICATE_PROPERTY_ERROR_TYPE: CommandErrorType<2> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_DUPLICATE,
    translation::java::ARGUMENT_BLOCK_PROPERTY_DUPLICATE,
);

pub const INVALID_VALUE_ERROR_TYPE: CommandErrorType<3> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_INVALID,
    translation::java::ARGUMENT_BLOCK_PROPERTY_INVALID,
);

pub const EXPECTED_VALUE_ERROR_TYPE: CommandErrorType<2> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_NOVALUE,
    translation::java::ARGUMENT_BLOCK_PROPERTY_NOVALUE,
);

pub const UNCLOSED_PROPERTIES_ERROR_TYPE: CommandErrorType<0> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNCLOSED,
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNCLOSED,
);

/// A block state parsed from `id[property=value,...]{nbt}`.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockStateArgument {
    pub block: &'static Block,
    pub state: BlockStateId,
    /// The block entity data after the state, if any.
    pub nbt: Option<NbtCompound>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockArgumentType;

impl<S: crate::source::CommandSource> ArgumentType<S> for BlockArgumentType {
    type Item = BlockStateArgument;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Item, CommandSyntaxError> {
        if reader.peek() == Some('#') {
            return Err(TAG_DISALLOWED_ERROR_TYPE.create(reader));
        }
        let start = reader.cursor();
        while let Some(c) = reader.peek() {
            if c.is_alphanumeric() || c == '_' || c == ':' || c == '/' || c == '.' || c == '-' {
                reader.skip();
            } else {
                break;
            }
        }
        let block_name = &reader.string()[start..reader.cursor()];
        let normalized = if block_name.contains(':') {
            block_name.to_string()
        } else {
            format!("minecraft:{block_name}")
        };

        let Some(block) = Block::from_name(&normalized) else {
            reader.set_cursor(start);
            return Err(INVALID_BLOCK_ERROR_TYPE.create(reader, TextComponent::text(normalized)));
        };

        let state = if reader.peek() == Some('[') {
            read_properties(reader, block, &normalized)?
        } else {
            block.default_state.id
        };

        let nbt = if reader.peek() == Some('{') {
            Some(ArgumentType::<crate::source::DummySource>::parse(
                &NbtCompoundArgumentType,
                reader,
            )?)
        } else {
            None
        };

        Ok(BlockStateArgument { block, state, nbt })
    }

    fn client_side_parser(&'_ self) -> JavaClientArgumentType {
        JavaClientArgumentType::BlockState
    }

    fn examples(&self) -> Vec<String> {
        examples!("stone", "minecraft:stone", "stone[foo=bar]", "foo{bar=baz}")
    }

    fn list_suggestions(
        &self,
        _context: &CommandContext<S>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        builder.build()
    }
}

/// The properties of `block` with their values, in the order of its states.
// IronPumpkin: a content-registry block answers through the same lookup as a generated one.
fn property_schema(block: &Block) -> Vec<(&'static str, Vec<&'static str>)> {
    let mut schema: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
    for state in block.states {
        let Some(properties) = block.properties(state.id) else {
            break;
        };
        for (name, value) in properties.to_props() {
            match schema.iter_mut().find(|(known, _)| *known == name) {
                Some((_, values)) => {
                    if !values.contains(&value) {
                        values.push(value);
                    }
                }
                None => schema.push((name, vec![value])),
            }
        }
    }
    schema
}

/// The value of `values` that `input` names. Vanilla reads an integer property with
/// `Integer.parseInt`, so `01` and `+1` name `1`.
fn find_value(values: &[&'static str], input: &str) -> Option<&'static str> {
    if let Some(value) = values.iter().find(|value| **value == input) {
        return Some(value);
    }
    let number = input.parse::<i32>().ok()?;
    if !values.iter().all(|value| value.parse::<i32>().is_ok()) {
        return None;
    }
    values
        .iter()
        .find(|value| value.parse::<i32>().ok() == Some(number))
        .copied()
}

/// Reads `[property=value,...]` as vanilla `BlockStateParser.readProperties` does, and returns
/// the state with those values.
fn read_properties(
    reader: &mut StringReader,
    block: &'static Block,
    id: &str,
) -> Result<BlockStateId, CommandSyntaxError> {
    reader.skip();
    reader.skip_whitespace();
    let schema = property_schema(block);
    let mut set: Vec<(&'static str, &'static str)> = Vec::new();
    while reader.can_read_char() && reader.peek() != Some(']') {
        reader.skip_whitespace();
        let name_start = reader.cursor();
        let input_name = reader.read_string()?;
        let Some((name, values)) = schema.iter().find(|(name, _)| *name == input_name) else {
            reader.set_cursor(name_start);
            return Err(UNKNOWN_PROPERTY_ERROR_TYPE.create(
                reader,
                TextComponent::text(id.to_string()),
                TextComponent::text(input_name),
            ));
        };
        if set.iter().any(|(known, _)| known == name) {
            reader.set_cursor(name_start);
            return Err(DUPLICATE_PROPERTY_ERROR_TYPE.create(
                reader,
                TextComponent::text(input_name),
                TextComponent::text(id.to_string()),
            ));
        }
        reader.skip_whitespace();
        if reader.peek() != Some('=') {
            return Err(EXPECTED_VALUE_ERROR_TYPE.create(
                reader,
                TextComponent::text(input_name),
                TextComponent::text(id.to_string()),
            ));
        }
        reader.skip();
        reader.skip_whitespace();
        let value_start = reader.cursor();
        let input_value = reader.read_string()?;
        let Some(value) = find_value(values, &input_value) else {
            reader.set_cursor(value_start);
            return Err(INVALID_VALUE_ERROR_TYPE.create(
                reader,
                TextComponent::text(id.to_string()),
                TextComponent::text(input_value),
                TextComponent::text(input_name),
            ));
        };
        set.push((name, value));
        reader.skip_whitespace();
        match reader.peek() {
            Some(',') => reader.skip(),
            Some(']') => break,
            Some(_) => return Err(UNCLOSED_PROPERTIES_ERROR_TYPE.create(reader)),
            None => {}
        }
    }
    if !reader.can_read_char() {
        return Err(UNCLOSED_PROPERTIES_ERROR_TYPE.create(reader));
    }
    reader.skip();
    Ok(block.from_properties(&set).to_state_id(block))
}

impl BlockArgumentType {
    /// Returns the parsed block, without its state.
    pub fn get<S: crate::source::CommandSource>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<&'static Block, CommandSyntaxError> {
        Ok(context.get_argument::<BlockStateArgument>(name)?.block)
    }

    /// Returns the parsed block state and its block entity data.
    pub fn get_state<S: crate::source::CommandSource>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<BlockStateArgument, CommandSyntaxError> {
        context.get_argument::<BlockStateArgument>(name).cloned()
    }
}

#[cfg(test)]
mod test {
    use super::{
        BlockArgumentType, BlockStateArgument, DUPLICATE_PROPERTY_ERROR_TYPE,
        EXPECTED_VALUE_ERROR_TYPE, INVALID_BLOCK_ERROR_TYPE, INVALID_VALUE_ERROR_TYPE,
        TAG_DISALLOWED_ERROR_TYPE, UNCLOSED_PROPERTIES_ERROR_TYPE, UNKNOWN_PROPERTY_ERROR_TYPE,
    };
    use crate::argument_types::argument_type::ArgumentType;
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
            .properties(parsed.state)
            .map(|properties| properties.to_props())
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
            &UNKNOWN_PROPERTY_ERROR_TYPE
                .create_without_context(text("minecraft:stone"), text("foo")),
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
        let expected = EXPECTED_VALUE_ERROR_TYPE
            .create_without_context(text("lit"), text("minecraft:redstone_lamp"));
        assert_error("redstone_lamp[lit]", &expected, 17);
        assert_error("redstone_lamp[lit ", &expected, 18);
    }

    #[test]
    fn unclosed_properties() {
        let expected = UNCLOSED_PROPERTIES_ERROR_TYPE.create_without_context();
        assert_error("redstone_lamp[", &expected, 14);
        assert_error("redstone_lamp[lit=true", &expected, 22);
        assert_error("redstone_lamp[lit=true;]", &expected, 22);
        assert_error("redstone_lamp[lit=true,", &expected, 23);
    }
}
