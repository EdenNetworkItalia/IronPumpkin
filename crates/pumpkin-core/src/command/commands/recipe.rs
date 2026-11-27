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

static ERROR_GIVE_FAILED: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_RECIPE_GIVE_FAILED,
    "Could not give any recipes",
);

static ERROR_TAKE_FAILED: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_RECIPE_TAKE_FAILED,
    "Could not take any recipes",
);

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

/// The recipes that `/recipe give` and `/recipe take` act on.
struct Selection {
    /// The display ids of the recipe book entries. Each id is the id of the whole book.
    display_ids: Vec<i32>,
    /// The recipes that a player learns. The stonecutter and smithing recipes count, but they
    /// have no book entry.
    count: usize,
}

/// The recipes that `recipe` names: every learnable recipe for `*`, else the generated, data
/// pack or plugin recipe with that id. It is `None` for an id that names no learnable recipe.
fn select(index: &RecipeIndex, recipe: &str) -> Option<Selection> {
    if recipe == "*" {
        let display_ids: Vec<i32> = index.book().display_ids().collect();
        let count = display_ids.len() + index.unbooked_recipe_ids().len();
        return Some(Selection { display_ids, count });
    }
    let display_ids: Vec<i32> = index
        .dynamic_recipes()
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            let id = get_recipe_id(r);
            id == recipe || id.strip_prefix("minecraft:").unwrap_or(&id) == recipe
        })
        .filter_map(|(i, _)| index.book().dynamic_display_id(i))
        .chain(index.book().generated_display_id(recipe))
        .collect();
    let unbooked = index.unbooked_recipe_ids().iter().any(|id| id == recipe);
    if display_ids.is_empty() && !unbooked {
        return None;
    }
    Some(Selection {
        count: display_ids.len().max(usize::from(unbooked)),
        display_ids,
    })
}

/// The feedback of vanilla `CommandResponseTracker`: the total is the recipes of every target.
fn send_feedback(
    context: &CommandContext,
    targets: &[std::sync::Arc<crate::entity::player::Player>],
    count: usize,
    single: &'static str,
    multiple: &'static str,
) {
    let total = TextComponent::text((targets.len() * count).to_string());
    let msg = if let [target] = targets {
        TextComponent::translate_cross(single, single, [total, target.get_display_name()])
    } else {
        TextComponent::translate_cross(
            multiple,
            multiple,
            [total, TextComponent::text(targets.len().to_string())],
        )
    };
    context.source.send_feedback(msg, true);
}

struct RecipeSuggestionProvider;

impl SuggestionProvider for RecipeSuggestionProvider {
    fn suggest(
        &self,
        context: &CommandContext,
        mut builder: SuggestionsBuilder,
    ) -> SuggestionProviderResult {
        if context.source.server.is_some() {
            let index = pumpkin_inventory::recipe_index::current();
            let dynamic = index
                .dynamic_recipes()
                .iter()
                .filter(|recipe| !matches!(recipe, DynamicRecipe::Ignored { .. }))
                .map(get_recipe_id);
            let generated = index.book().generated_recipe_ids().map(str::to_string);
            let unbooked = index.unbooked_recipe_ids().iter().cloned();
            for id in dynamic.chain(generated).chain(unbooked) {
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
        let selection = select(&index, &recipe_str).ok_or_else(|| {
            ERROR_RECIPE_NOT_FOUND.create_without_context(TextComponent::text(recipe_str.clone()))
        })?;
        // Vanilla `RESPONSE_GIVE` fails when no target learns a recipe.
        if selection.count == 0 {
            return Err(ERROR_GIVE_FAILED.create_without_context());
        }

        if !selection.display_ids.is_empty() {
            let packet = pumpkin_protocol::codec::recipe_book::CRecipeBookEntries::subset(
                index.book(),
                index.dynamic_recipes(),
                &selection.display_ids,
            );
            for player in &targets {
                player.try_send_client_packet(&packet);
            }
        }

        let recipe_count = selection.count;
        send_feedback(
            context,
            &targets,
            recipe_count,
            translation::java::COMMANDS_RECIPE_GIVE_SUCCESS_SINGLE,
            translation::java::COMMANDS_RECIPE_GIVE_SUCCESS_MULTIPLE,
        );

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
        let selection = select(&index, &recipe_str).ok_or_else(|| {
            ERROR_RECIPE_NOT_FOUND.create_without_context(TextComponent::text(recipe_str.clone()))
        })?;
        // Vanilla `RESPONSE_TAKE` fails when no target forgets a recipe.
        if selection.count == 0 {
            return Err(ERROR_TAKE_FAILED.create_without_context());
        }

        if !selection.display_ids.is_empty() {
            let recipe_ids_to_remove: Vec<VarInt> =
                selection.display_ids.iter().copied().map(VarInt).collect();
            let packet = CRecipeBookRemove::new(&recipe_ids_to_remove);
            for player in &targets {
                player.try_send_client_packet(&packet);
            }
        }

        let taken_count = selection.count;
        send_feedback(
            context,
            &targets,
            taken_count,
            translation::java::COMMANDS_RECIPE_TAKE_SUCCESS_SINGLE,
            translation::java::COMMANDS_RECIPE_TAKE_SUCCESS_MULTIPLE,
        );

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

    fn ids(index: &RecipeIndex, recipe: &str) -> Vec<i32> {
        select(index, recipe)
            .map(|s| s.display_ids)
            .unwrap_or_default()
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
            assert_eq!(ids(&index, id).len(), 1, "{id}");
        }
        assert!(select(&index, "minecraft:no_such_recipe").is_none());
        let all = select(&index, "*").expect("every recipe");
        assert_eq!(all.display_ids.len(), index.book().display_ids().count());
        assert_eq!(
            all.count,
            all.display_ids.len() + index.unbooked_recipe_ids().len()
        );
    }

    /// Vanilla takes any recipe that is not special: the stonecutter and smithing recipes too.
    /// They have no book entry, so only the count shows that a player learned them.
    #[test]
    fn a_stonecutter_or_smithing_recipe_is_a_learnable_recipe_without_a_book_entry() {
        let index = RecipeIndex::build(&[]);
        for id in [
            "minecraft:stone_slab_from_stone_stonecutting",
            "minecraft:netherite_sword_smithing",
        ] {
            let selection = select(&index, id).unwrap_or_else(|| panic!("{id} is learnable"));
            assert!(selection.display_ids.is_empty(), "{id}");
            assert_eq!(selection.count, 1, "{id}");
        }
    }

    /// A data pack recipe with the id of a vanilla recipe takes its book entry, so one id is one
    /// entry.
    #[test]
    fn a_replaced_generated_recipe_has_one_book_entry() {
        let id = "minecraft:iron_ingot_from_smelting_raw_iron";
        let plain = RecipeIndex::build(&[]);
        let replaced = RecipeIndex::build(&[smelting(id)]);
        assert_eq!(ids(&replaced, id).len(), 1);
        assert_eq!(ids(&replaced, id), ids(&plain, id));
    }
}
