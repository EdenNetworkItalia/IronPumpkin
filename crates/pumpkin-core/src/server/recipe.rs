use std::sync::RwLock;

use pumpkin_inventory::crafting::recipe_provider::RecipeProvider;
use pumpkin_inventory::recipe_index;
pub use pumpkin_protocol::codec::recipe::DynamicRecipe;
use pumpkin_protocol::codec::recipe::{PropertySetSync, StonecutterSync};
use pumpkin_protocol::java::client::play::CUpdateRecipes;
use tracing::warn;

use crate::net::java::JavaClient;

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
        let mut recipes = self
            .dynamic_recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        recipes.push(recipe);
        recipe_index::install(&recipes);
    }

    pub fn add_recipes(&self, new_recipes: impl IntoIterator<Item = DynamicRecipe>) {
        let mut recipes = self
            .dynamic_recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        recipes.extend(new_recipes);
        recipe_index::install(&recipes);
    }

    pub fn set_recipes(&self, new_recipes: Vec<DynamicRecipe>) {
        let mut recipes = self
            .dynamic_recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *recipes = new_recipes;
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
    let stonecutter: Vec<StonecutterSync<'_>> = index
        .stonecutter_recipes()
        .iter()
        .map(|recipe| StonecutterSync {
            ingredient: &recipe.ingredient,
            result_item: &recipe.result.item_id,
            result_count: recipe.result.count,
        })
        .collect();
    match CUpdateRecipes::encode(&property_sets, &stonecutter, client.encoding_key()) {
        Ok(body) => {
            client.try_send_packet(&CUpdateRecipes::new(&body));
        }
        Err(err) => warn!("Failed to encode the recipe sync packet: {err}"),
    }
}
