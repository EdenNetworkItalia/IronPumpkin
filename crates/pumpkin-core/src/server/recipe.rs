use std::sync::{Arc, RwLock};

use pumpkin_inventory::crafting::recipe_provider::RecipeProvider;
use pumpkin_inventory::recipe_index;
use pumpkin_inventory::recipe_index::LoadError;
pub use pumpkin_protocol::codec::recipe::DynamicRecipe;
use pumpkin_protocol::java::client::play::CUpdateRecipes;
use tracing::warn;

use crate::net::java::JavaClient;

/// The recipe that the manager keeps for `recipe`. A recipe that a codec rejects or that
/// vanilla `RecipeManager.finalizeRecipeLoading` ignores becomes a placeholder: vanilla keeps it
/// in the recipe map, so its id still replaces a lower recipe with that id. A recipe without an
/// id has nothing to replace and is dropped.
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
    recipe.recipe_id().map(|recipe_id| DynamicRecipe::Ignored {
        recipe_id: recipe_id.to_string(),
    })
}

/// Puts `recipe` in `recipes`. A recipe replaces the one with the same id in its place, as the
/// data pack loader does for a later pack. A recipe without an id has nothing to replace.
fn put(recipes: &mut Vec<DynamicRecipe>, recipe: DynamicRecipe) {
    let existing = recipe.recipe_id().and_then(|id| {
        recipes
            .iter()
            .position(|other| other.recipe_id() == Some(id))
    });
    match existing {
        Some(index) => recipes[index] = recipe,
        None => recipes.push(recipe),
    }
}

/// Adds `recipe` to `recipes` as [`loaded`] keeps it.
fn insert(recipes: &mut Vec<DynamicRecipe>, recipe: DynamicRecipe) {
    if let Some(recipe) = loaded(recipe) {
        put(recipes, recipe);
    }
}

#[derive(Default)]
struct Recipes {
    /// The recipes that plugins added, as they added them. A data pack reload loads them again.
    plugin: Vec<DynamicRecipe>,
    /// The data pack recipes, then the plugin recipes. The crafting grid reads this list on each
    /// change, so a reader shares it and clones no recipe.
    loaded: Arc<[DynamicRecipe]>,
}

/// The data pack and plugin recipes. Each change rebuilds the [`recipe_index`].
pub struct RecipeManager {
    recipes: RwLock<Recipes>,
}

impl Default for RecipeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RecipeManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            recipes: RwLock::new(Recipes::default()),
        }
    }

    /// Adds a recipe. The players online get it at the next [`Server::sync_recipes`].
    ///
    /// [`Server::sync_recipes`]: crate::server::Server::sync_recipes
    pub fn add_recipe(&self, recipe: DynamicRecipe) {
        self.add_recipes([recipe]);
    }

    /// Adds plugin recipes. They stay in the manager across [`Self::set_recipes`].
    pub fn add_recipes(&self, new_recipes: impl IntoIterator<Item = DynamicRecipe>) {
        let mut state = self
            .recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut recipes = state.loaded.to_vec();
        for recipe in new_recipes {
            put(&mut state.plugin, recipe.clone());
            insert(&mut recipes, recipe);
        }
        recipe_index::install(&recipes);
        state.loaded = recipes.into();
    }

    /// Replaces the data pack recipes. The plugin recipes stay, after the data pack recipes.
    pub fn set_recipes(&self, new_recipes: Vec<DynamicRecipe>) {
        let mut state = self
            .recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut recipes = Vec::new();
        for recipe in new_recipes.into_iter().chain(state.plugin.iter().cloned()) {
            insert(&mut recipes, recipe);
        }
        recipe_index::install(&recipes);
        state.loaded = recipes.into();
    }

    /// Removes every recipe, the plugin recipes included.
    pub fn clear(&self) {
        let mut state = self
            .recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *state = Recipes::default();
        recipe_index::install(&[]);
    }

    /// The loaded recipes, with the placeholders of the ignored recipes.
    pub fn get_dynamic_recipes_internal(&self) -> Arc<[DynamicRecipe]> {
        self.get_dynamic_recipes()
    }
}

impl RecipeProvider for RecipeManager {
    fn get_dynamic_recipes(&self) -> Arc<[DynamicRecipe]> {
        self.recipes
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .loaded
            .clone()
    }
}

/// Sends vanilla `ClientboundUpdateRecipesPacket`, as `PlayerList` does at join and after a data
/// pack reload: the recipe property sets and the stonecutter recipes of the [`recipe_index`].
pub fn send_update_recipes(client: &JavaClient) {
    match recipe_index::current().update_recipes_body(client.encoding_key()) {
        Ok(body) => {
            client.try_send_packet(&CUpdateRecipes::new(&body));
        }
        Err(err) => warn!("Failed to encode the recipe sync packet: {err}"),
    }
}

/// Whether `client` gets the recipe book, as at join.
pub fn gets_recipe_book(server: &crate::server::Server, client: &JavaClient) -> bool {
    server.advanced_config.recipe.send_recipes
        && client.version.load() >= pumpkin_util::version::JavaMinecraftVersion::V_1_21_2
}

/// Sends the whole recipe book of the [`recipe_index`], which replaces the book of the client.
pub fn send_recipe_book(client: &JavaClient) {
    let index = recipe_index::current();
    let packet = pumpkin_protocol::codec::recipe_book::CRecipeBookEntries::all(
        true,
        index.book(),
        index.dynamic_recipes(),
    );
    if let Ok(data) = client.serialize_packet(&packet) {
        client.try_enqueue_packet(data);
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

    fn ids(manager: &RecipeManager) -> Vec<String> {
        manager
            .get_dynamic_recipes_internal()
            .iter()
            .filter_map(|r| r.recipe_id().map(ToString::to_string))
            .collect()
    }

    #[test]
    fn the_manager_keeps_a_placeholder_for_a_recipe_that_cannot_be_placed() {
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
        assert_eq!(
            ids(&manager),
            [
                "test:kept",
                "test:dropped",
                "test:added",
                "test:unknown_result"
            ]
        );
        let recipes = manager.get_dynamic_recipes_internal();
        assert!(
            recipes[1..]
                .iter()
                .all(|r| matches!(r, DynamicRecipe::Ignored { .. }))
        );
        assert!(
            recipe_index::current()
                .stonecutter_recipes(pumpkin_data::dynamic::ContentIds::Real)
                .all(|r| r.recipe_id != "test:dropped")
        );
    }

    /// Vanilla keeps an ignored recipe in the recipe map, so it still replaces the generated
    /// recipe with its id.
    #[test]
    fn an_ignored_recipe_still_replaces_the_generated_recipe_with_its_id() {
        let id = "minecraft:stone_slab_from_stone_stonecutting";
        let generated = || {
            recipe_index::current()
                .stonecutter_recipes(pumpkin_data::dynamic::ContentIds::Real)
                .any(|r| r.recipe_id == id)
        };
        let manager = RecipeManager::new();
        manager.set_recipes(Vec::new());
        assert!(generated(), "{id} is a generated recipe");
        manager.set_recipes(vec![stonecutting(id, "test:missing")]);
        assert!(!generated());
    }

    /// A reload loads the data pack recipes again. The recipes that plugins added stay, and a
    /// plugin recipe replaces a data pack recipe with its id.
    #[test]
    fn a_reload_keeps_the_recipes_that_plugins_added() {
        let manager = RecipeManager::new();
        manager.set_recipes(vec![stonecutting("test:pack", "minecraft:dirt")]);
        manager.add_recipe(stonecutting("test:plugin", "minecraft:gravel"));
        manager.add_recipe(stonecutting("test:shadowed", "minecraft:gravel"));
        manager.set_recipes(vec![
            stonecutting("test:reloaded", "minecraft:dirt"),
            stonecutting("test:shadowed", "minecraft:dirt"),
        ]);
        assert_eq!(
            ids(&manager),
            ["test:reloaded", "test:shadowed", "test:plugin"]
        );
        let recipes = manager.get_dynamic_recipes_internal();
        let Some(DynamicRecipe::Stonecutting(shadowed)) = recipes
            .iter()
            .find(|r| r.recipe_id() == Some("test:shadowed"))
        else {
            panic!("a stonecutter recipe");
        };
        assert_eq!(
            shadowed.ingredient,
            OwnedRecipeIngredient::Simple("minecraft:gravel".to_string())
        );
        assert!(
            recipe_index::current()
                .stonecutter_recipes(pumpkin_data::dynamic::ContentIds::Real)
                .any(|r| r.recipe_id == "test:plugin")
        );
        manager.clear();
        assert!(ids(&manager).is_empty());
    }

    /// The crafting grid reads the recipes on each change. A read shares the list.
    #[test]
    fn reading_the_recipes_clones_no_recipe() {
        let manager = RecipeManager::new();
        manager.set_recipes(vec![stonecutting("test:a", "minecraft:dirt")]);
        assert_eq!(
            manager.get_dynamic_recipes().as_ptr(),
            manager.get_dynamic_recipes().as_ptr()
        );
    }

    /// A recipe replaces the one with the same id in its place, as a later data pack does.
    #[test]
    fn a_recipe_replaces_the_recipe_with_the_same_id() {
        let manager = RecipeManager::new();
        manager.set_recipes(vec![
            stonecutting("test:a", "minecraft:dirt"),
            stonecutting("test:b", "minecraft:dirt"),
        ]);
        manager.add_recipe(stonecutting("test:a", "minecraft:gravel"));
        assert_eq!(ids(&manager), ["test:a", "test:b"]);
        let recipes = manager.get_dynamic_recipes_internal();
        let Some(DynamicRecipe::Stonecutting(first)) = recipes.first() else {
            panic!("a stonecutter recipe");
        };
        assert_eq!(
            first.ingredient,
            OwnedRecipeIngredient::Simple("minecraft:gravel".to_string())
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
