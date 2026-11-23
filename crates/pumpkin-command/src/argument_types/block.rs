use pumpkin_data::{Block, BlockStateId, Property, translation};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::identifier::Identifier;
use pumpkin_util::text::TextComponent;

use crate::{
    argument_types::{
        FromStringReader,
        argument_type::{ArgumentType, JavaClientArgumentType},
        nbt::NbtCompoundArgumentType,
    },
    context::command_context::CommandContext,
    errors::command_syntax_error::CommandSyntaxError,
    errors::error_types::CommandErrorType,
    string_reader::StringReader,
    suggestion::suggestions::{Suggestions, SuggestionsBuilder},
};

pub const ERROR_NO_TAGS_ALLOWED: CommandErrorType<0> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_TAG_DISALLOWED,
    translation::java::ARGUMENT_BLOCK_TAG_DISALLOWED,
);

pub const INVALID_BLOCK_ERROR_TYPE: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_ID_INVALID,
    translation::java::ARGUMENT_BLOCK_ID_INVALID,
);

pub const ERROR_UNKNOWN_PROPERTY: CommandErrorType<2> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNKNOWN,
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNKNOWN,
);

pub const ERROR_DUPLICATE_PROPERTY: CommandErrorType<2> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_DUPLICATE,
    translation::java::ARGUMENT_BLOCK_PROPERTY_DUPLICATE,
);

pub const ERROR_INVALID_VALUE: CommandErrorType<3> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_INVALID,
    translation::java::ARGUMENT_BLOCK_PROPERTY_INVALID,
);

pub const ERROR_EXPECTED_VALUE: CommandErrorType<2> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_NOVALUE,
    translation::java::ARGUMENT_BLOCK_PROPERTY_NOVALUE,
);

pub const ERROR_UNCLOSED_PROPERTIES: CommandErrorType<0> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNCLOSED,
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNCLOSED,
);

/// Parsed block state argument matching vanilla `BlockInput`.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockInput {
    pub block: &'static Block,
    pub state_id: BlockStateId,
    pub defined_properties: Vec<Property>,
    /// `IronPumpkin`: every given property as a name and value. A content-registry block can have
    /// properties without a [`Property`] variant, which `defined_properties` leaves out.
    pub defined_string_properties: Vec<(&'static str, &'static str)>,
    pub nbt: Option<NbtCompound>,
}

impl BlockInput {
    #[must_use]
    pub const fn new(
        block: &'static Block,
        state_id: BlockStateId,
        defined_properties: Vec<Property>,
        nbt: Option<NbtCompound>,
    ) -> Self {
        Self {
            block,
            state_id,
            defined_properties,
            defined_string_properties: Vec::new(),
            nbt,
        }
    }

    #[must_use]
    pub fn overwrite_defined_properties(&self, state_id: BlockStateId) -> BlockStateId {
        if state_id == self.state_id {
            return state_id;
        }
        with_defined_properties(
            self.block,
            state_id,
            &self.defined_properties,
            &self.defined_string_properties,
        )
    }
}

/// `state_id` with the given properties set, as `Block::apply_properties` does.
// IronPumpkin: a content-registry block sets its properties without a `Property` variant by name.
fn with_defined_properties(
    block: &'static Block,
    state_id: BlockStateId,
    defined: &[Property],
    defined_strings: &[(&'static str, &'static str)],
) -> BlockStateId {
    if defined_strings.len() <= defined.len() {
        return block.apply_properties(state_id, defined);
    }
    if Block::from_state_id(state_id).id != block.id {
        return state_id;
    }
    let mut properties = block.string_properties(state_id).unwrap_or_default();
    for (name, value) in defined_strings {
        if let Some(slot) = properties.iter_mut().find(|(known, _)| known == name) {
            slot.1 = value;
        }
    }
    block.from_string_properties(&properties).to_state_id(block)
}

pub(crate) fn parse_nbt_compound(
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

/// The given properties, as `Property` values and as names and values.
type DefinedProperties = (Vec<Property>, Vec<(&'static str, &'static str)>);

fn read_block_properties(
    reader: &mut StringReader,
    block: &'static Block,
    block_id: &str,
) -> Result<DefinedProperties, CommandSyntaxError> {
    reader.skip();
    reader.skip_whitespace();
    // IronPumpkin: the schema also has the properties of a content-registry block.
    let schema = property_schema(block);
    let mut defined: Vec<Property> = Vec::new();
    let mut defined_strings: Vec<(&'static str, &'static str)> = Vec::new();

    while reader.can_read_char() && reader.peek() != Some(']') {
        reader.skip_whitespace();
        let key_start = reader.cursor();
        let key = reader.read_string()?;

        let Some((name, values)) = schema.iter().find(|(name, _)| *name == key) else {
            reader.set_cursor(key_start);
            return Err(ERROR_UNKNOWN_PROPERTY.create(
                reader,
                TextComponent::text(block_id.to_string()),
                TextComponent::text(key),
            ));
        };

        if defined_strings.iter().any(|(known, _)| known == name) {
            reader.set_cursor(key_start);
            return Err(ERROR_DUPLICATE_PROPERTY.create(
                reader,
                TextComponent::text(key),
                TextComponent::text(block_id.to_string()),
            ));
        }

        reader.skip_whitespace();
        if reader.peek() != Some('=') {
            return Err(ERROR_EXPECTED_VALUE.create(
                reader,
                TextComponent::text(key),
                TextComponent::text(block_id.to_string()),
            ));
        }
        reader.skip();
        reader.skip_whitespace();

        let val_start = reader.cursor();
        let raw_value = reader.read_string()?;
        let Some(value) = find_value(values, &raw_value) else {
            reader.set_cursor(val_start);
            return Err(ERROR_INVALID_VALUE.create(
                reader,
                TextComponent::text(block_id.to_string()),
                TextComponent::text(raw_value),
                TextComponent::text(key),
            ));
        };
        // A property of a content-registry block can have no `Property` variant.
        if let Ok(prop) = block.parse_property(name, value) {
            defined.push(prop);
        }
        defined_strings.push((name, value));

        reader.skip_whitespace();
        if reader.can_read_char() {
            if reader.peek() != Some(',') {
                if reader.peek() != Some(']') {
                    return Err(ERROR_UNCLOSED_PROPERTIES.create(reader));
                }
                break;
            }
            reader.skip();
        }
    }

    if reader.can_read_char() {
        reader.skip();
        Ok((defined, defined_strings))
    } else {
        Err(ERROR_UNCLOSED_PROPERTIES.create(reader))
    }
}

pub(crate) fn parse_block_input(
    reader: &mut StringReader,
    allow_nbt: bool,
) -> Result<BlockInput, CommandSyntaxError> {
    let initial_cursor = reader.cursor();
    parse_block_input_inner(reader, allow_nbt).inspect_err(|_| reader.set_cursor(initial_cursor))
}

fn parse_block_input_inner(
    reader: &mut StringReader,
    allow_nbt: bool,
) -> Result<BlockInput, CommandSyntaxError> {
    if reader.peek() == Some('#') {
        return Err(ERROR_NO_TAGS_ALLOWED.create(reader));
    }

    let id_start = reader.cursor();
    let id = Identifier::from_reader(reader)?;
    let id_str = id.to_string();
    let Some(block) = Block::from_name(&id_str) else {
        reader.set_cursor(id_start);
        return Err(INVALID_BLOCK_ERROR_TYPE.create(reader, TextComponent::text(id_str)));
    };

    let (defined_properties, defined_string_properties) = if reader.peek() == Some('[') {
        read_block_properties(reader, block, &id_str)?
    } else {
        (Vec::new(), Vec::new())
    };

    let state_id = with_defined_properties(
        block,
        block.default_state.id,
        &defined_properties,
        &defined_string_properties,
    );
    let nbt = if allow_nbt {
        parse_nbt_compound(reader)?
    } else {
        None
    };

    Ok(BlockInput {
        block,
        state_id,
        defined_properties,
        defined_string_properties,
        nbt,
    })
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockArgumentType;

impl<S: crate::source::CommandSource> ArgumentType<S> for BlockArgumentType {
    type Item = BlockInput;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Item, CommandSyntaxError> {
        parse_block_input(reader, true)
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

impl BlockArgumentType {
    pub fn get<S: crate::source::CommandSource>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<BlockInput, CommandSyntaxError> {
        Ok(context.get_argument::<BlockInput>(name)?.clone())
    }
}

#[cfg(test)]
mod test {
    use super::{
        BlockArgumentType, ERROR_DUPLICATE_PROPERTY, ERROR_EXPECTED_VALUE, ERROR_INVALID_VALUE,
        ERROR_NO_TAGS_ALLOWED, ERROR_UNCLOSED_PROPERTIES, ERROR_UNKNOWN_PROPERTY,
        INVALID_BLOCK_ERROR_TYPE,
    };
    use crate::{
        argument_types::argument_type::ArgumentType, source::DummySource,
        string_reader::StringReader,
    };
    use pumpkin_data::{Block, Property};

    #[test]
    fn parses_block_state_properties_and_nbt() {
        let mut reader = StringReader::new(
            "minecraft:chest[facing=south,waterlogged=true]{CustomName:\"Loot\"} rest",
        );
        let input = ArgumentType::<DummySource>::parse(&BlockArgumentType, &mut reader).unwrap();
        assert_eq!(input.block.id, Block::CHEST.id);
        assert_ne!(input.state_id, Block::CHEST.default_state.id);
        assert!(Block::CHEST.is_waterlogged(input.state_id));
        assert_eq!(input.defined_properties.len(), 2);
        assert!(
            input
                .defined_properties
                .contains(&Property::Waterlogged(true))
        );
        assert_eq!(
            input
                .nbt
                .as_ref()
                .and_then(|nbt| nbt.get_string("CustomName")),
            Some("Loot")
        );
        assert_eq!(reader.remaining_part(), " rest");
    }

    #[test]
    fn validates_block_specific_property_domains_and_errors() {
        let mut hopper_ok = StringReader::new("hopper[facing=north]");
        assert!(ArgumentType::<DummySource>::parse(&BlockArgumentType, &mut hopper_ok).is_ok());

        let mut hopper_up = StringReader::new("hopper[facing=up]");
        assert_parse_err_reset!(hopper_up, BlockArgumentType, &ERROR_INVALID_VALUE);

        let mut fence_invalid = StringReader::new("oak_fence[west=none]");
        assert_parse_err_reset!(fence_invalid, BlockArgumentType, &ERROR_INVALID_VALUE);

        let mut wall_valid = StringReader::new("cobblestone_wall[west=none]");
        assert!(ArgumentType::<DummySource>::parse(&BlockArgumentType, &mut wall_valid).is_ok());

        let mut unknown_prop = StringReader::new("stone[facing=north]");
        assert_parse_err_reset!(unknown_prop, BlockArgumentType, &ERROR_UNKNOWN_PROPERTY);

        let mut duplicate_prop = StringReader::new("oak_fence[west=true,west=false]");
        assert_parse_err_reset!(duplicate_prop, BlockArgumentType, &ERROR_DUPLICATE_PROPERTY);

        let mut missing_val = StringReader::new("oak_fence[west]");
        assert_parse_err_reset!(missing_val, BlockArgumentType, &ERROR_EXPECTED_VALUE);

        let mut unclosed = StringReader::new("oak_fence[west=true");
        assert_parse_err_reset!(unclosed, BlockArgumentType, &ERROR_UNCLOSED_PROPERTIES);

        let mut disallowed_tag = StringReader::new("#minecraft:planks");
        assert_parse_err_reset!(disallowed_tag, BlockArgumentType, &ERROR_NO_TAGS_ALLOWED);

        let mut invalid_id = StringReader::new("minecraft:not_a_real_block");
        assert_parse_err_reset!(invalid_id, BlockArgumentType, &INVALID_BLOCK_ERROR_TYPE);
    }
}

#[cfg(test)]
#[path = "block_ironpumpkin_tests.rs"]
mod ironpumpkin_tests;
