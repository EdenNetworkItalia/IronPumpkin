use crate::command::CommandSource;
use crate::command::argument_types::FromStringReader;
use crate::command::argument_types::argument_type::{ArgumentType, JavaClientArgumentType};
use crate::command::context::command_context::CommandContext;
use crate::command::errors::command_syntax_error::CommandSyntaxError;
use crate::command::errors::error_types::CommandErrorType;
use crate::command::string_reader::StringReader;
use crate::command::suggestion::suggestions::{Suggestions, SuggestionsBuilder};
use crate::world::World;
use pumpkin_data::{Advancement, translation};
use pumpkin_util::identifier::Identifier;
use pumpkin_util::resource::ResourceKey;
use pumpkin_util::text::TextComponent;
use std::string::ToString;
use std::sync::Arc;

pub static ADVANCEMENT_REGISTRY: &Identifier = &Identifier::vanilla_static("advancement");
pub static DIMENSION_REGISTRY: &Identifier = &Identifier::vanilla_static("dimension");
pub static BIOME_REGISTRY: &Identifier = &Identifier::vanilla_static("worldgen/biome");

pub const ERROR_INVALID_ADVANCEMENT: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ADVANCEMENT_ADVANCEMENTNOTFOUND,
    translation::java::ADVANCEMENT_ADVANCEMENTNOTFOUND,
);

pub const ERROR_INVALID_BIOME: CommandErrorType<1> =
    CommandErrorType::new("commands.fillbiome.invalid", "commands.fillbiome.invalid");

pub const ERROR_INVALID_DIMENSION: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ARGUMENT_DIMENSION_INVALID,
    translation::java::ARGUMENT_DIMENSION_INVALID,
);

pub const ERROR_NOT_SUMMONABLE_ENTITY: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ENTITY_NOT_SUMMONABLE,
    translation::java::ENTITY_NOT_SUMMONABLE,
);

/// Represents an argument type used to get a resource key from an identifier.
///
/// if you want an [`Advancement`] put the [`ADVANCEMENT_REGISTRY`]
/// and get it with the [`ResourceKeyArgument::get_advancement`] when it has been parsed
///
/// TODO Recipe
///
/// if you juste want the [`ResourceKey`] use the [`ResourceKeyArgument::get_registry_key`] function
pub struct ResourceKeyArgument(pub &'static Identifier);

pub static ERROR_INVALID: CommandErrorType<0> = CommandErrorType::new(
    translation::java::ARGUMENT_ID_INVALID,
    translation::java::ARGUMENT_ID_INVALID,
);

impl ArgumentType<CommandSource> for ResourceKeyArgument {
    type Item = ResourceKey;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Item, CommandSyntaxError> {
        let identifier = Identifier::from_reader(reader)?;
        Ok(ResourceKey::new(self.0.clone(), identifier))
    }

    fn list_suggestions(
        &self,
        context: &CommandContext,
        suggestions_builder: SuggestionsBuilder,
    ) -> Suggestions {
        if self.0 == ADVANCEMENT_REGISTRY {
            let advancements = context.server().advancement_manager.get_advancements();
            suggestions_builder
                .filter_and_suggest_iter(advancements.iter().map(ToString::to_string))
                .build()
        } else if self.0 == BIOME_REGISTRY {
            let biomes = pumpkin_data::biome::Biome::ALL
                .iter()
                .map(|biome| format!("minecraft:{}", biome.registry_id));
            suggestions_builder.filter_and_suggest_iter(biomes).build()
        } else if self.0 == DIMENSION_REGISTRY {
            suggest_levels(&context.server().worlds.load(), suggestions_builder)
        } else {
            Suggestions::empty()
        }
    }

    fn client_side_parser(&self) -> JavaClientArgumentType {
        // Vanilla has its own parser for dimensions and takes biomes as plain
        // `ResourceArgument`s; the rest are resource keys.
        if self.0 == DIMENSION_REGISTRY {
            JavaClientArgumentType::Dimension
        } else if self.0 == BIOME_REGISTRY {
            JavaClientArgumentType::Resource {
                identifier: self.0.clone(),
            }
        } else {
            JavaClientArgumentType::ResourceKey {
                identifier: self.0.clone(),
            }
        }
    }
}

impl ResourceKeyArgument {
    /// Returns a [`CommandContext`]'s parsed resource key argument as an [`Advancement`].
    ///
    /// # Arguments
    /// * `context` - The [`CommandContext`] that has the parsed [`ResourceKey`] with the provided argument name.
    /// * `name` - The name of the argument that was parsed.
    ///
    /// # Returns
    /// The `Advancement` containing the advancement get from the resource key argument, wrapped in an `Ok`,
    /// or an `Err` with the appropriate [`CommandSyntaxError`] if it could not be resolved or
    /// that the key correspond to an unknown advancement.
    pub fn get_advancement(
        context: &CommandContext,
        name: &str,
    ) -> Result<&'static Advancement, CommandSyntaxError> {
        let resource_key: &ResourceKey = Self::get_registry_key(
            context,
            name,
            ADVANCEMENT_REGISTRY,
            &ERROR_INVALID_ADVANCEMENT,
        )?;
        Advancement::from_name(resource_key.identifier.path()).ok_or_else(|| {
            ERROR_INVALID_ADVANCEMENT.create_without_context(TextComponent::text(
                resource_key.identifier.path().to_string(),
            ))
        })
    }

    /// Returns a [`CommandContext`]'s parsed resource key argument as a [`Biome`](pumpkin_data::biome::Biome).
    pub fn get_biome(
        context: &CommandContext,
        name: &str,
    ) -> Result<&'static pumpkin_data::biome::Biome, CommandSyntaxError> {
        let resource_key: &ResourceKey =
            Self::get_registry_key(context, name, BIOME_REGISTRY, &ERROR_INVALID_BIOME)?;
        let path = resource_key.identifier.path();
        pumpkin_data::biome::Biome::from_name(path).ok_or_else(|| {
            ERROR_INVALID_BIOME
                .create_without_context(TextComponent::text(resource_key.identifier.to_string()))
        })
    }

    /// Returns the loaded world of a [`CommandContext`]'s parsed dimension argument, or
    /// [`ERROR_INVALID_DIMENSION`] when no such world is loaded, like vanilla
    /// `DimensionArgument.getDimension`.
    pub fn get_dimension(
        context: &CommandContext,
        name: &str,
    ) -> Result<Arc<World>, CommandSyntaxError> {
        let resource_key =
            Self::get_registry_key(context, name, DIMENSION_REGISTRY, &ERROR_INVALID_DIMENSION)?;
        get_level(&context.server().worlds.load(), &resource_key.identifier)
    }

    /// Returns a [`CommandContext`]'s parsed resource key argument in the form of a [`ResourceKey`].
    ///
    /// # Arguments
    /// * `context` - The [`CommandContext`] that has the parsed [`ResourceKey`] with the provided argument name.
    /// * `name` - The name of the argument that was parsed.
    ///
    /// # Returns
    /// The `ResourceKey` containing the key get from the argument, wrapped in an `Ok`,
    /// or an `Err` with the appropriate [`CommandSyntaxError`] if it could not be resolved
    pub fn get_registry_key<'a>(
        context: &'a CommandContext,
        name: &str,
        registry: &Identifier,
        error: &'static CommandErrorType<1>,
    ) -> Result<&'a ResourceKey, CommandSyntaxError> {
        let argument = context.get_argument::<ResourceKey>(name)?;
        argument.cast(registry).ok_or_else(|| {
            error
                .create_without_context(TextComponent::text(argument.identifier.path().to_string()))
        })
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
        .ok_or_else(|| ERROR_INVALID_DIMENSION.create_without_context(TextComponent::text(key)))
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
    use pumpkin_data::dimension::Dimension;
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

        let parsed = ResourceKeyArgument(DIMENSION_REGISTRY)
            .parse(&mut StringReader::new("minecraft:the_nether"))
            .unwrap();
        assert_eq!(parsed.identifier, Identifier::vanilla_static("the_nether"));
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
