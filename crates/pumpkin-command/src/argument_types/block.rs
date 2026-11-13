use crate::{
    argument_types::{
        argument_type::{ArgumentType, JavaClientArgumentType},
        block_predicate::{ERROR_NO_VALUE, ERROR_UNCLOSED_PROPERTIES, parse_nbt},
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
            let properties = read_properties(reader, block, &normalized)?;
            block.from_string_properties(&properties).to_state_id(block)
        } else {
            block.default_state.id
        };
        let nbt = parse_nbt(reader)?;
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
pub(crate) fn property_schema(block: &Block) -> Vec<(&'static str, Vec<&'static str>)> {
    let mut schema: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
    for state in block.states {
        let Some(properties) = block.string_properties(state.id) else {
            break;
        };
        for (name, value) in properties {
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
pub(crate) fn find_value(values: &[&'static str], input: &str) -> Option<&'static str> {
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
/// the given properties with their values.
pub(crate) fn read_properties(
    reader: &mut StringReader,
    block: &'static Block,
    id: &str,
) -> Result<Vec<(&'static str, &'static str)>, CommandSyntaxError> {
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
            return Err(ERROR_NO_VALUE.create(
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
            Some(_) => return Err(ERROR_UNCLOSED_PROPERTIES.create(reader)),
            None => {}
        }
    }
    if !reader.can_read_char() {
        return Err(ERROR_UNCLOSED_PROPERTIES.create(reader));
    }
    reader.skip();
    Ok(set)
}

impl BlockArgumentType {
    pub fn get<S: crate::source::CommandSource>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<BlockStateArgument, CommandSyntaxError> {
        context.get_argument::<BlockStateArgument>(name).cloned()
    }
}

#[cfg(test)]
#[path = "block_ironpumpkin_tests.rs"]
mod test;
