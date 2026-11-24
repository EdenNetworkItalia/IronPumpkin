use crate::command::argument_builder::{ArgumentBuilder, argument, command, literal};
use crate::command::argument_types::entity::EntityArgumentType;
use crate::command::argument_types::registry_id::RegistryIdArgumentType;
use crate::command::context::command_context::CommandContext;
use crate::command::errors::error_types::CommandErrorType;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use crate::command::suggestion::provider::{SuggestionProvider, SuggestionProviderResult};
use crate::command::suggestion::suggestions::SuggestionsBuilder;
use crate::entity::EntityBase;
use pumpkin_data::translation;
use pumpkin_inventory::recipe_index::RecipeIndex;
use pumpkin_protocol::codec::recipe::DynamicRecipe;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_protocol::java::client::play::CRecipeBookRemove;
use pumpkin_util::PermissionLvl;
use pumpkin_util::identifier::Identifier;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;

const DESCRIPTION: &str = "Gives or takes player recipes.";
const PERMISSION: &str = "minecraft:command.recipe";

static ERROR_RECIPE_NOT_FOUND: CommandErrorType<1> =
    CommandErrorType::new(translation::java::RECIPE_NOTFOUND, "Unknown recipe: %s");

fn get_recipe_id(recipe: &DynamicRecipe) -> String {
    recipe.recipe_id().map_or_else(
        || match recipe {
            DynamicRecipe::Crafting(
                pumpkin_protocol::codec::recipe::OwnedCraftingRecipe::Shaped { result, .. }
                | pumpkin_protocol::codec::recipe::OwnedCraftingRecipe::Shapeless { result, .. },
            ) => result.item_id.clone(),
            _ => String::new(),
        },
        str::to_string,
    )
}

/// The display ids of the recipe book entries that `recipe` names: every entry for `*`, else
/// the generated, data pack or plugin recipe with that id. Each id is the id of the whole book.
fn book_ids(index: &RecipeIndex, recipe: &str) -> Vec<i32> {
    if recipe == "*" {
        return index.book().display_ids().collect();
    }
    let dynamic = index
        .dynamic_recipes()
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            let id = get_recipe_id(r);
            id == recipe || id.strip_prefix("minecraft:").unwrap_or(&id) == recipe
        })
        .filter_map(|(i, _)| index.book().dynamic_display_id(i));
    dynamic
        .chain(index.book().generated_display_id(recipe))
        .collect()
}

struct RecipeSuggestionProvider;

impl SuggestionProvider for RecipeSuggestionProvider {
    fn suggest(
        &self,
        context: &CommandContext,
        mut builder: SuggestionsBuilder,
    ) -> SuggestionProviderResult {
        if let Some(server) = &context.source.server {
            let recipes = server.recipe_manager.get_dynamic_recipes_internal();
            for recipe in recipes
                .iter()
                .filter(|recipe| !matches!(recipe, DynamicRecipe::Ignored { .. }))
            {
                let id = get_recipe_id(recipe);
                builder = builder.suggest(id);
            }
        }
        builder.build()
    }
}

struct RecipeGiveExecutor {
    all: bool,
}

impl CommandExecutor for RecipeGiveExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let targets = EntityArgumentType::get_players(context, "targets")?;
        let recipe_str = if self.all {
            "*".to_string()
        } else {
            context.get_argument::<Identifier>("recipe")?.to_string()
        };

        // The recipes live in the server.
        context.source.server.as_ref().ok_or_else(|| {
            ERROR_RECIPE_NOT_FOUND.create_without_context(TextComponent::text(recipe_str.clone()))
        })?;

        let index = pumpkin_inventory::recipe_index::current();
        let ids = book_ids(&index, &recipe_str);
        if recipe_str != "*" && ids.is_empty() {
            return Err(
                ERROR_RECIPE_NOT_FOUND.create_without_context(TextComponent::text(recipe_str))
            );
        }

        let recipe_count = ids.len();
        let packet = pumpkin_protocol::codec::recipe_book::CRecipeBookEntries::subset(
            index.book(),
            index.dynamic_recipes(),
            &ids,
        );
        for player in &targets {
            player.try_send_client_packet(&packet);
        }

        let recipe_count_str = recipe_count.to_string();
        if targets.len() == 1 {
            let msg = TextComponent::translate_cross(
                translation::java::COMMANDS_RECIPE_GIVE_SUCCESS_SINGLE,
                translation::java::COMMANDS_RECIPE_GIVE_SUCCESS_SINGLE,
                [
                    TextComponent::text(recipe_count_str),
                    targets[0].get_display_name(),
                ],
            );
            context.source.send_feedback(msg, true);
        } else {
            let msg = TextComponent::translate_cross(
                translation::java::COMMANDS_RECIPE_GIVE_SUCCESS_MULTIPLE,
                translation::java::COMMANDS_RECIPE_GIVE_SUCCESS_MULTIPLE,
                [
                    TextComponent::text(recipe_count_str),
                    TextComponent::text(targets.len().to_string()),
                ],
            );
            context.source.send_feedback(msg, true);
        }

        Ok((targets.len() * recipe_count) as i32)
    }
}

struct RecipeTakeExecutor {
    all: bool,
}

impl CommandExecutor for RecipeTakeExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let targets = EntityArgumentType::get_players(context, "targets")?;
        let recipe_str = if self.all {
            "*".to_string()
        } else {
            context.get_argument::<Identifier>("recipe")?.to_string()
        };

        // The recipes live in the server.
        context.source.server.as_ref().ok_or_else(|| {
            ERROR_RECIPE_NOT_FOUND.create_without_context(TextComponent::text(recipe_str.clone()))
        })?;

        let index = pumpkin_inventory::recipe_index::current();
        let recipe_ids_to_remove: Vec<VarInt> = book_ids(&index, &recipe_str)
            .into_iter()
            .map(VarInt)
            .collect();

        if recipe_ids_to_remove.is_empty() {
            return Err(
                ERROR_RECIPE_NOT_FOUND.create_without_context(TextComponent::text(recipe_str))
            );
        }

        let taken_count = recipe_ids_to_remove.len();

        let packet = CRecipeBookRemove::new(&recipe_ids_to_remove);
        for player in &targets {
            player.try_send_client_packet(&packet);
        }

        let taken_count_str = taken_count.to_string();
        if targets.len() == 1 {
            let msg = TextComponent::translate_cross(
                translation::java::COMMANDS_RECIPE_TAKE_SUCCESS_SINGLE,
                translation::java::COMMANDS_RECIPE_TAKE_SUCCESS_SINGLE,
                [
                    TextComponent::text(taken_count_str),
                    targets[0].get_display_name(),
                ],
            );
            context.source.send_feedback(msg, true);
        } else {
            let msg = TextComponent::translate_cross(
                translation::java::COMMANDS_RECIPE_TAKE_SUCCESS_MULTIPLE,
                translation::java::COMMANDS_RECIPE_TAKE_SUCCESS_MULTIPLE,
                [
                    TextComponent::text(taken_count_str),
                    TextComponent::text(targets.len().to_string()),
                ],
            );
            context.source.send_feedback(msg, true);
        }

        Ok((targets.len() * taken_count) as i32)
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    let builder = command("recipe", DESCRIPTION)
        .requires(PERMISSION)
        .then(
            literal("give").then(
                argument("targets", EntityArgumentType::Players)
                    .then(literal("*").executes(RecipeGiveExecutor { all: true }))
                    .then(
                        argument(
                            "recipe",
                            RegistryIdArgumentType::resource_key(Identifier::vanilla_static(
                                "recipe",
                            )),
                        )
                        .suggests(RecipeSuggestionProvider)
                        .executes(RecipeGiveExecutor { all: false }),
                    ),
            ),
        )
        .then(
            literal("take").then(
                argument("targets", EntityArgumentType::Players)
                    .then(literal("*").executes(RecipeTakeExecutor { all: true }))
                    .then(
                        argument(
                            "recipe",
                            RegistryIdArgumentType::resource_key(Identifier::vanilla_static(
                                "recipe",
                            )),
                        )
                        .suggests(RecipeSuggestionProvider)
                        .executes(RecipeTakeExecutor { all: false }),
                    ),
            ),
        );

    dispatcher.register(builder);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::recipes::RecipeCategoryTypes;
    use pumpkin_protocol::codec::recipe::{
        OwnedCookingRecipe, OwnedCookingRecipeType, OwnedRecipeIngredient, OwnedRecipeResult,
    };

    fn smelting(id: &str) -> DynamicRecipe {
        DynamicRecipe::Cooking(OwnedCookingRecipeType::Smelting(OwnedCookingRecipe {
            recipe_id: id.to_string(),
            category: RecipeCategoryTypes::Misc,
            group: None,
            ingredient: OwnedRecipeIngredient::Simple("minecraft:dirt".to_string()),
            cooking_time: 50,
            experience: 0.0,
            result: OwnedRecipeResult {
                item_id: "minecraft:diamond".to_string(),
                count: 1,
                components: None,
            },
        }))
    }

    /// `/recipe give` and `/recipe take` name a vanilla recipe by its id.
    #[test]
    fn a_generated_recipe_has_a_book_entry_by_its_id() {
        let index = RecipeIndex::build(&[smelting("test:diamond")]);
        for id in [
            "minecraft:stick",
            "minecraft:iron_ingot_from_smelting_raw_iron",
            "test:diamond",
        ] {
            assert_eq!(book_ids(&index, id).len(), 1, "{id}");
        }
        assert!(book_ids(&index, "minecraft:no_such_recipe").is_empty());
        assert_eq!(
            book_ids(&index, "*").len(),
            index.book().display_ids().count()
        );
    }

    /// A data pack recipe with the id of a vanilla recipe takes its book entry, so one id is one
    /// entry.
    #[test]
    fn a_replaced_generated_recipe_has_one_book_entry() {
        let id = "minecraft:iron_ingot_from_smelting_raw_iron";
        let plain = RecipeIndex::build(&[]);
        let replaced = RecipeIndex::build(&[smelting(id)]);
        assert_eq!(book_ids(&replaced, id).len(), 1);
        assert_eq!(book_ids(&replaced, id), book_ids(&plain, id));
    }
}
