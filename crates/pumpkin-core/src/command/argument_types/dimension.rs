use std::sync::Arc;

use pumpkin_data::dimension::Dimension;
use pumpkin_data::translation;
use pumpkin_util::identifier::Identifier;
use pumpkin_util::text::TextComponent;

use crate::command::CommandSource;
use crate::command::argument_types::FromStringReader;
use crate::command::argument_types::argument_type::{ArgumentType, JavaClientArgumentType};
use crate::command::context::command_context::CommandContext;
use crate::command::errors::command_syntax_error::CommandSyntaxError;
use crate::command::errors::error_types::CommandErrorType;
use crate::command::string_reader::StringReader;
use crate::command::suggestion::suggestions::{Suggestions, SuggestionsBuilder};
use crate::world::World;

pub static ERROR_INVALID_VALUE: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ARGUMENT_DIMENSION_INVALID,
    translation::java::ARGUMENT_DIMENSION_INVALID,
);

/// An argument type that parses the key of a loaded dimension.
///
/// Get the world with [`DimensionArgument::get_dimension`] once it has been parsed.
pub struct DimensionArgument;

impl ArgumentType<CommandSource> for DimensionArgument {
    type Item = Identifier;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Item, CommandSyntaxError> {
        Identifier::from_reader(reader)
    }

    fn list_suggestions(
        &self,
        context: &CommandContext,
        suggestions_builder: SuggestionsBuilder,
    ) -> Suggestions {
        suggest_levels(&context.server().worlds.load(), suggestions_builder)
    }

    fn client_side_parser(&self) -> JavaClientArgumentType {
        JavaClientArgumentType::Dimension
    }

    fn examples(&self) -> Vec<String> {
        vec![
            Dimension::OVERWORLD.minecraft_name.to_string(),
            Dimension::THE_NETHER.minecraft_name.to_string(),
        ]
    }
}

impl DimensionArgument {
    /// Returns the loaded world whose dimension is the parsed argument `name`, or
    /// [`ERROR_INVALID_VALUE`] when no such world is loaded.
    pub fn get_dimension(
        context: &CommandContext,
        name: &str,
    ) -> Result<Arc<World>, CommandSyntaxError> {
        let location = context.get_argument::<Identifier>(name)?;
        get_level(&context.server().worlds.load(), location)
    }
}

/// `MinecraftServer.getLevel` for a dimension key.
fn get_level(
    worlds: &[Arc<World>],
    location: &Identifier,
) -> Result<Arc<World>, CommandSyntaxError> {
    let key = location.to_string();
    worlds
        .iter()
        .find(|world| world.dimension.minecraft_name == key)
        .cloned()
        .ok_or_else(|| ERROR_INVALID_VALUE.create_without_context(TextComponent::text(key)))
}

fn suggest_levels(worlds: &[Arc<World>], builder: SuggestionsBuilder) -> Suggestions {
    builder
        .filter_and_suggest_iter(worlds.iter().map(|world| world.dimension.minecraft_name))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use arc_swap::ArcSwap;
    use pumpkin_config::world::LevelConfig;
    use pumpkin_util::world_seed::Seed;
    use pumpkin_world::dimension::into_level;
    use pumpkin_world::world_info::LevelData;
    use std::sync::Weak;

    fn loaded_worlds(root: &std::path::Path, dimensions: &[Dimension]) -> Vec<Arc<World>> {
        let level_info = Arc::new(ArcSwap::from_pointee(LevelData::default(Seed(0))));
        let block_registry = crate::block::registry::default_registry();
        dimensions
            .iter()
            .map(|dimension| {
                let level = into_level(
                    dimension.clone(),
                    &LevelConfig::default(),
                    root.to_path_buf(),
                    0,
                );
                Arc::new(World::load(
                    level,
                    level_info.clone(),
                    dimension.clone(),
                    block_registry.clone(),
                    Weak::new(),
                ))
            })
            .collect()
    }

    #[tokio::test]
    async fn resolves_loaded_dimensions_only() {
        let root = tempfile::TempDir::new().unwrap();
        let worlds = loaded_worlds(root.path(), &[Dimension::OVERWORLD, Dimension::THE_NETHER]);

        let nether = get_level(&worlds, &Identifier::vanilla_static("the_nether")).unwrap();
        assert!(Arc::ptr_eq(&nether, &worlds[1]));

        let Err(error) = get_level(&worlds, &Identifier::vanilla_static("the_end")) else {
            panic!("an unloaded dimension resolved to a world");
        };
        let message = serde_json::to_value(error.message).unwrap();
        assert_eq!(message["translate"], "argument.dimension.invalid");
        assert_eq!(message["with"][0]["text"], "minecraft:the_end");

        let parsed = DimensionArgument
            .parse(&mut StringReader::new("minecraft:the_nether"))
            .unwrap();
        assert_eq!(parsed, Identifier::vanilla_static("the_nether"));
    }

    #[tokio::test]
    async fn suggests_loaded_dimensions() {
        let root = tempfile::TempDir::new().unwrap();
        let worlds = loaded_worlds(root.path(), &[Dimension::OVERWORLD, Dimension::THE_NETHER]);

        let texts = |input: &str| -> Vec<String> {
            suggest_levels(&worlds, SuggestionsBuilder::new(input, 0))
                .suggestions
                .iter()
                .map(|suggestion| suggestion.text.cached_text().clone())
                .collect()
        };
        assert_eq!(texts(""), ["minecraft:overworld", "minecraft:the_nether"]);
        assert_eq!(texts("the"), ["minecraft:the_nether"]);
        assert!(texts("minecraft:the_end").is_empty());
    }
}
