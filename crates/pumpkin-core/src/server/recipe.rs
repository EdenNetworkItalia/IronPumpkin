use std::sync::RwLock;

use pumpkin_inventory::crafting::recipe_provider::RecipeProvider;
use pumpkin_inventory::recipe_index;
use pumpkin_inventory::recipe_index::LoadError;
pub use pumpkin_protocol::codec::recipe::DynamicRecipe;
use pumpkin_protocol::codec::recipe::{PropertySetSync, StonecutterSync};
use pumpkin_protocol::java::client::play::CUpdateRecipes;
use tracing::warn;

use crate::net::java::JavaClient;

/// The recipe that the manager keeps for `recipe`, or `None` for a recipe that a codec rejects
/// or that vanilla `RecipeManager.finalizeRecipeLoading` ignores. It logs the vanilla message.
fn loaded(recipe: DynamicRecipe) -> Option<DynamicRecipe> {
    let Some(error) = recipe_index::load_error(&recipe) else {
        return Some(recipe);
    };
    let id = recipe.recipe_id().unwrap_or("<unnamed>");
    match error {
        LoadError::Parse(message) => warn!("Failed to parse {id}: {message}"),
        LoadError::Unplaceable => {
            warn!("Recipe {id} can't be placed due to empty ingredients and will be ignored");
        }
    }
    None
}

/// Adds `recipe` to `recipes` when it loads.
fn insert(recipes: &mut Vec<DynamicRecipe>, recipe: DynamicRecipe) {
    if let Some(recipe) = loaded(recipe) {
        recipes.push(recipe);
    }
}

/// The data pack and plugin recipes. Each change rebuilds the [`recipe_index`].
pub struct RecipeManager {
    dynamic_recipes: RwLock<Vec<DynamicRecipe>>,
}

impl Default for RecipeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RecipeManager {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            dynamic_recipes: RwLock::new(Vec::new()),
        }
    }

    pub fn add_recipe(&self, recipe: DynamicRecipe) {
        self.add_recipes([recipe]);
    }

    pub fn add_recipes(&self, new_recipes: impl IntoIterator<Item = DynamicRecipe>) {
        let mut recipes = self
            .dynamic_recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for recipe in new_recipes {
            insert(&mut recipes, recipe);
        }
        recipe_index::install(&recipes);
    }

    pub fn set_recipes(&self, new_recipes: Vec<DynamicRecipe>) {
        let mut recipes = self
            .dynamic_recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        recipes.clear();
        for recipe in new_recipes {
            insert(&mut recipes, recipe);
        }
        recipe_index::install(&recipes);
    }

    pub fn clear(&self) {
        let mut recipes = self
            .dynamic_recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        recipes.clear();
        recipe_index::install(&recipes);
    }

    pub fn get_dynamic_recipes_internal(&self) -> Vec<DynamicRecipe> {
        self.dynamic_recipes
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl RecipeProvider for RecipeManager {
    fn get_dynamic_recipes(&self) -> Vec<DynamicRecipe> {
        self.dynamic_recipes
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// Sends vanilla `ClientboundUpdateRecipesPacket`, as `PlayerList` does at join and after a data
/// pack reload: the recipe property sets and the stonecutter recipes of the [`recipe_index`].
pub fn send_update_recipes(client: &JavaClient) {
    let index = recipe_index::current();
    let property_sets: Vec<PropertySetSync<'_>> = index
        .property_sets()
        .iter()
        .map(|set| PropertySetSync {
            key: set.key,
            ingredients: &set.ingredients,
        })
        .collect();
    let encoding = client.encoding_key();
    let stonecutter: Vec<StonecutterSync<'_>> = index
        .stonecutter_recipes(encoding.content_ids())
        .map(|recipe| StonecutterSync {
            ingredient: &recipe.ingredient,
            result_item: &recipe.result.item_id,
            result_count: recipe.result.count,
        })
        .collect();
    match CUpdateRecipes::encode(&property_sets, &stonecutter, encoding) {
        Ok(body) => {
            client.try_send_packet(&CUpdateRecipes::new(&body));
        }
        Err(err) => warn!("Failed to encode the recipe sync packet: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_protocol::codec::recipe::{
        OwnedRecipeIngredient, OwnedRecipeResult, OwnedStonecutterRecipe,
    };

    fn stonecutting(id: &str, ingredient: &str) -> DynamicRecipe {
        DynamicRecipe::Stonecutting(OwnedStonecutterRecipe {
            recipe_id: id.to_string(),
            group: None,
            ingredient: OwnedRecipeIngredient::Simple(ingredient.to_string()),
            result: OwnedRecipeResult {
                item_id: "minecraft:stone".to_string(),
                count: 1,
                components: None,
            },
        })
    }

    #[test]
    fn the_manager_drops_a_recipe_that_cannot_be_placed() {
        let manager = RecipeManager::new();
        manager.set_recipes(vec![
            stonecutting("test:kept", "minecraft:dirt"),
            stonecutting("test:dropped", "test:missing"),
        ]);
        manager.add_recipe(stonecutting("test:added", "minecraft:air"));
        let mut unknown_result = stonecutting("test:unknown_result", "minecraft:dirt");
        if let DynamicRecipe::Stonecutting(r) = &mut unknown_result {
            r.result.item_id = "test:missing".to_string();
        }
        manager.add_recipe(unknown_result);
        let ids: Vec<String> = manager
            .get_dynamic_recipes_internal()
            .iter()
            .filter_map(|r| r.recipe_id().map(ToString::to_string))
            .collect();
        assert_eq!(ids, ["test:kept"]);
        assert!(
            recipe_index::current()
                .stonecutter_recipes(pumpkin_data::dynamic::ContentIds::Real)
                .all(|r| r.recipe_id != "test:dropped")
        );
    }

    /// The vanilla messages: `Registry.java:43`, `Item.java:118`, `HolderSetCodec.java:76`.
    #[test]
    fn a_load_error_has_the_vanilla_message() {
        let parse = |ingredient: &str| match recipe_index::load_error(&stonecutting(
            "test:r", ingredient,
        )) {
            Some(LoadError::Parse(message)) => message,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            parse("test:missing"),
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:item]: test:missing"
        );
        assert_eq!(parse("minecraft:air"), "Item must not be minecraft:air");
        let missing_tag = DynamicRecipe::Stonecutting(OwnedStonecutterRecipe {
            recipe_id: "test:r".to_string(),
            group: None,
            ingredient: OwnedRecipeIngredient::Tagged("test:no_such_tag".to_string()),
            result: OwnedRecipeResult {
                item_id: "minecraft:stone".to_string(),
                count: 1,
                components: None,
            },
        });
        assert_eq!(
            recipe_index::load_error(&missing_tag),
            Some(LoadError::Parse(
                "Missing tag: 'test:no_such_tag' in 'minecraft:item'".to_string()
            ))
        );
        let empty = DynamicRecipe::Stonecutting(OwnedStonecutterRecipe {
            recipe_id: "test:r".to_string(),
            group: None,
            ingredient: OwnedRecipeIngredient::OneOf(Vec::new()),
            result: OwnedRecipeResult {
                item_id: "minecraft:stone".to_string(),
                count: 1,
                components: None,
            },
        });
        assert_eq!(
            recipe_index::load_error(&empty),
            Some(LoadError::Unplaceable)
        );
    }
}
