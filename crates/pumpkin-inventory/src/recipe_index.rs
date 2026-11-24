//! The loaded recipes in the order of the vanilla `RecipeManager`.
//!
//! The index holds the generated recipes and the data pack recipes in one list. The furnaces, the
//! campfire, the stonecutter and the recipe sync packet read it, so the server and the client see
//! the same recipes in the same order.

use std::collections::BTreeMap;
use std::mem::discriminant;
use std::sync::{Arc, LazyLock, PoisonError, RwLock};

use pumpkin_data::data_component::DataComponent;
use pumpkin_data::data_component_impl::DataComponentImpl;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::potion_brewing::BREWING_RECIPES;
use pumpkin_data::recipes::{
    CookingRecipe, CookingRecipeKind, CookingRecipeType, RECIPES_COOKING,
    RECIPES_SMITHING_TRANSFORM, RECIPES_SMITHING_TRIM, RECIPES_STONECUTTING, RecipeIngredientTypes,
    RecipeResultStruct,
};
use pumpkin_protocol::codec::recipe::{
    DynamicRecipe, OwnedCookingRecipe, OwnedCookingRecipeType, OwnedRecipeIngredient,
    OwnedRecipeResult,
};

type Components = &'static [(DataComponent, &'static dyn DataComponentImpl)];

/// The item stack that a recipe produces.
#[derive(Clone)]
pub struct RecipeOutput {
    pub item_id: String,
    pub count: u8,
    pub components: Components,
}

impl RecipeOutput {
    fn generated(result: &RecipeResultStruct) -> Self {
        Self {
            item_id: result.id.to_string(),
            count: result.count,
            components: result.components,
        }
    }

    fn owned(result: &OwnedRecipeResult) -> Self {
        Self {
            item_id: result.item_id.clone(),
            count: result.count,
            components: &[],
        }
    }

    #[must_use]
    pub fn item(&self) -> Option<&'static Item> {
        Item::from_registry_key(&self.item_id)
    }

    /// The result stack, or an empty stack when the item does not exist.
    #[must_use]
    pub fn assemble(&self) -> ItemStack {
        let Some(item) = self.item() else {
            return ItemStack::EMPTY.clone();
        };
        let mut stack = ItemStack::new(self.count, item);
        for (component, value) in self.components {
            stack.patch.push((*component, Some(value.clone_dyn())));
        }
        stack
    }
}

/// A furnace, blast furnace, smoker or campfire recipe.
#[derive(Clone)]
pub struct CookingEntry {
    pub recipe_id: String,
    pub kind: CookingRecipeKind,
    pub ingredient: OwnedRecipeIngredient,
    pub cooking_time: i32,
    pub experience: f32,
    pub result: RecipeOutput,
}

/// A stonecutter recipe.
#[derive(Clone)]
pub struct StonecutterEntry {
    pub recipe_id: String,
    pub ingredient: OwnedRecipeIngredient,
    pub result: RecipeOutput,
}

/// A vanilla `RecipePropertySet`: the items of the listed ingredients.
pub struct PropertySet {
    /// The `minecraft:recipe_property_set` key, for example `minecraft:furnace_input`.
    pub key: &'static str,
    pub ingredients: Vec<OwnedRecipeIngredient>,
}

pub const SMITHING_BASE: &str = "minecraft:smithing_base";
pub const SMITHING_TEMPLATE: &str = "minecraft:smithing_template";
pub const SMITHING_ADDITION: &str = "minecraft:smithing_addition";
pub const FURNACE_INPUT: &str = "minecraft:furnace_input";
pub const BLAST_FURNACE_INPUT: &str = "minecraft:blast_furnace_input";
pub const SMOKER_INPUT: &str = "minecraft:smoker_input";
pub const CAMPFIRE_INPUT: &str = "minecraft:campfire_input";
pub const BREWING_INPUT: &str = "minecraft:brewing_input";
pub const BREWING_REAGENT: &str = "minecraft:brewing_reagent";

enum Entry {
    Cooking(CookingEntry),
    Stonecutter(StonecutterEntry),
    Smithing {
        template: OwnedRecipeIngredient,
        base: OwnedRecipeIngredient,
        addition: OwnedRecipeIngredient,
    },
    Brewing {
        input: OwnedRecipeIngredient,
        reagent: OwnedRecipeIngredient,
    },
    /// A recipe that no index list holds. It still replaces a generated recipe with its id.
    Other,
}

/// The loaded recipes that the furnaces, the campfire, the stonecutter and the recipe sync use.
pub struct RecipeIndex {
    cooking: Vec<CookingEntry>,
    stonecutter: Vec<StonecutterEntry>,
    property_sets: Vec<PropertySet>,
}

/// The vanilla recipe registry sorts the recipe files by `Identifier.compareTo`: the path first,
/// then the namespace. The path of the file includes the `.json` suffix.
fn sort_key(recipe_id: &str) -> (String, String) {
    let (namespace, path) = recipe_id
        .split_once(':')
        .unwrap_or(("minecraft", recipe_id));
    (format!("{path}.json"), namespace.to_string())
}

fn owned_ingredient(ingredient: &RecipeIngredientTypes) -> OwnedRecipeIngredient {
    match ingredient {
        RecipeIngredientTypes::Simple(id) => OwnedRecipeIngredient::Simple((*id).to_string()),
        RecipeIngredientTypes::Tagged(tag) => {
            OwnedRecipeIngredient::Tagged(tag.strip_prefix('#').unwrap_or(tag).to_string())
        }
        RecipeIngredientTypes::OneOf(ids) => {
            OwnedRecipeIngredient::OneOf(ids.iter().map(|id| (*id).to_string()).collect())
        }
    }
}

fn simple_ingredient(item: &Item) -> OwnedRecipeIngredient {
    OwnedRecipeIngredient::Simple(format!("minecraft:{}", item.registry_key))
}

const fn generated_cooking(recipe: &CookingRecipeType) -> (&CookingRecipe, CookingRecipeKind) {
    match recipe {
        CookingRecipeType::Blasting(r) => (r, CookingRecipeKind::Blasting),
        CookingRecipeType::Smelting(r) => (r, CookingRecipeKind::Smelting),
        CookingRecipeType::Smoking(r) => (r, CookingRecipeKind::Smoking),
        CookingRecipeType::CampfireCooking(r) => (r, CookingRecipeKind::CampfireCooking),
    }
}

const fn owned_cooking(
    recipe: &OwnedCookingRecipeType,
) -> (&OwnedCookingRecipe, CookingRecipeKind) {
    match recipe {
        OwnedCookingRecipeType::Blasting(r) => (r, CookingRecipeKind::Blasting),
        OwnedCookingRecipeType::Smelting(r) => (r, CookingRecipeKind::Smelting),
        OwnedCookingRecipeType::Smoking(r) => (r, CookingRecipeKind::Smoking),
        OwnedCookingRecipeType::CampfireCooking(r) => (r, CookingRecipeKind::CampfireCooking),
    }
}

fn dynamic_entry(recipe: &DynamicRecipe) -> Entry {
    match recipe {
        DynamicRecipe::Cooking(cooking) => {
            let (r, kind) = owned_cooking(cooking);
            Entry::Cooking(CookingEntry {
                recipe_id: r.recipe_id.clone(),
                kind,
                ingredient: r.ingredient.clone(),
                cooking_time: r.cooking_time,
                experience: r.experience,
                result: RecipeOutput::owned(&r.result),
            })
        }
        DynamicRecipe::Stonecutting(r) => Entry::Stonecutter(StonecutterEntry {
            recipe_id: r.recipe_id.clone(),
            ingredient: r.ingredient.clone(),
            result: RecipeOutput::owned(&r.result),
        }),
        DynamicRecipe::SmithingTransform(r) => Entry::Smithing {
            template: r.template.clone(),
            base: r.base.clone(),
            addition: r.addition.clone(),
        },
        DynamicRecipe::SmithingTrim(r) => Entry::Smithing {
            template: r.template.clone(),
            base: r.base.clone(),
            addition: r.addition.clone(),
        },
        DynamicRecipe::Brewing(r) => Entry::Brewing {
            input: r.input_item.clone(),
            reagent: r.reagent.clone(),
        },
        DynamicRecipe::Crafting(_) | DynamicRecipe::SpecialCrafting(_) => Entry::Other,
    }
}

/// The generated recipes that the index holds, keyed in vanilla order.
fn generated_entries() -> BTreeMap<(String, String), Entry> {
    let mut recipes = BTreeMap::new();
    for recipe in RECIPES_COOKING {
        let (r, kind) = generated_cooking(recipe);
        recipes.insert(
            sort_key(r.recipe_id),
            Entry::Cooking(CookingEntry {
                recipe_id: r.recipe_id.to_string(),
                kind,
                ingredient: owned_ingredient(&r.ingredient),
                cooking_time: r.cookingtime,
                experience: r.experience,
                result: RecipeOutput::generated(&r.result),
            }),
        );
    }
    for r in RECIPES_STONECUTTING {
        recipes.insert(
            sort_key(r.recipe_id),
            Entry::Stonecutter(StonecutterEntry {
                recipe_id: r.recipe_id.to_string(),
                ingredient: owned_ingredient(&r.ingredient),
                result: RecipeOutput::generated(&r.result),
            }),
        );
    }
    for r in RECIPES_SMITHING_TRANSFORM {
        recipes.insert(
            sort_key(r.recipe_id),
            Entry::Smithing {
                template: owned_ingredient(&r.template),
                base: owned_ingredient(&r.base),
                addition: owned_ingredient(&r.addition),
            },
        );
    }
    for r in RECIPES_SMITHING_TRIM {
        recipes.insert(
            sort_key(r.recipe_id),
            Entry::Smithing {
                template: owned_ingredient(&r.template),
                base: owned_ingredient(&r.base),
                addition: owned_ingredient(&r.addition),
            },
        );
    }
    recipes
}

impl RecipeIndex {
    /// Builds the index from the generated recipes and `dynamic`. A dynamic recipe replaces the
    /// generated recipe with the same id, and a later dynamic recipe replaces an earlier one, as a
    /// higher data pack replaces a lower one.
    #[must_use]
    pub fn build(dynamic: &[DynamicRecipe]) -> Self {
        let mut recipes = generated_entries();
        for recipe in dynamic {
            if let Some(id) = recipe.recipe_id() {
                recipes.insert(sort_key(id), dynamic_entry(recipe));
            }
        }

        let mut index = Self {
            cooking: Vec::new(),
            stonecutter: Vec::new(),
            property_sets: [
                SMITHING_BASE,
                SMITHING_TEMPLATE,
                SMITHING_ADDITION,
                FURNACE_INPUT,
                BLAST_FURNACE_INPUT,
                SMOKER_INPUT,
                CAMPFIRE_INPUT,
                BREWING_INPUT,
                BREWING_REAGENT,
            ]
            .into_iter()
            .map(|key| PropertySet {
                key,
                ingredients: Vec::new(),
            })
            .collect(),
        };
        // The generated brewing recipes have no id, so a data pack recipe cannot replace one.
        for r in &BREWING_RECIPES {
            index.add_to_set(BREWING_INPUT, simple_ingredient(r.from_item()));
            index.add_to_set(BREWING_REAGENT, simple_ingredient(r.reagent()));
        }
        for entry in recipes.into_values() {
            match entry {
                Entry::Cooking(cooking) => {
                    let key = match cooking.kind {
                        CookingRecipeKind::Smelting => FURNACE_INPUT,
                        CookingRecipeKind::Blasting => BLAST_FURNACE_INPUT,
                        CookingRecipeKind::Smoking => SMOKER_INPUT,
                        CookingRecipeKind::CampfireCooking => CAMPFIRE_INPUT,
                    };
                    index.add_to_set(key, cooking.ingredient.clone());
                    index.cooking.push(cooking);
                }
                Entry::Stonecutter(stonecutter) => index.stonecutter.push(stonecutter),
                Entry::Smithing {
                    template,
                    base,
                    addition,
                } => {
                    index.add_to_set(SMITHING_TEMPLATE, template);
                    index.add_to_set(SMITHING_BASE, base);
                    index.add_to_set(SMITHING_ADDITION, addition);
                }
                Entry::Brewing { input, reagent } => {
                    index.add_to_set(BREWING_INPUT, input);
                    index.add_to_set(BREWING_REAGENT, reagent);
                }
                Entry::Other => {}
            }
        }
        index
    }

    fn add_to_set(&mut self, key: &'static str, ingredient: OwnedRecipeIngredient) {
        if let Some(set) = self.property_sets.iter_mut().find(|set| set.key == key)
            && !set.ingredients.contains(&ingredient)
        {
            set.ingredients.push(ingredient);
        }
    }

    /// The first cooking recipe of `kind` that takes `item`, as vanilla `RecipeManager.getRecipeFor`.
    #[must_use]
    pub fn cooking_recipe(&self, item: &Item, kind: &CookingRecipeKind) -> Option<&CookingEntry> {
        self.cooking
            .iter()
            .find(|r| discriminant(&r.kind) == discriminant(kind) && r.ingredient.match_item(item))
    }

    /// The experience of the cooking recipe with `recipe_id`.
    #[must_use]
    pub fn cooking_experience(&self, recipe_id: &str) -> Option<f32> {
        self.cooking
            .iter()
            .find(|r| r.recipe_id == recipe_id)
            .map(|r| r.experience)
    }

    #[must_use]
    pub fn cooking_recipes(&self) -> &[CookingEntry] {
        &self.cooking
    }

    /// The stonecutter recipes in vanilla order, the order of the client's recipe buttons.
    #[must_use]
    pub fn stonecutter_recipes(&self) -> &[StonecutterEntry] {
        &self.stonecutter
    }

    /// The stonecutter recipes that take `item`, as vanilla `SingleInputSet.selectByInput`.
    pub fn stonecutter_recipes_for<'a>(
        &'a self,
        item: &'a Item,
    ) -> impl Iterator<Item = &'a StonecutterEntry> + 'a {
        self.stonecutter
            .iter()
            .filter(move |r| r.ingredient.match_item(item))
    }

    #[must_use]
    pub fn property_sets(&self) -> &[PropertySet] {
        &self.property_sets
    }

    #[must_use]
    pub fn property_set(&self, key: &str) -> Option<&PropertySet> {
        self.property_sets.iter().find(|set| set.key == key)
    }
}

impl PropertySet {
    /// Vanilla `RecipePropertySet.test`.
    #[must_use]
    pub fn test(&self, item: &Item) -> bool {
        self.ingredients.iter().any(|i| i.match_item(item))
    }
}

static INDEX: LazyLock<RwLock<Arc<RecipeIndex>>> =
    LazyLock::new(|| RwLock::new(Arc::new(RecipeIndex::build(&[]))));

/// The current recipe index. Before the first [`install`], it holds the generated recipes only.
#[must_use]
pub fn current() -> Arc<RecipeIndex> {
    INDEX.read().unwrap_or_else(PoisonError::into_inner).clone()
}

/// Rebuilds the index from the generated recipes and `dynamic`.
pub fn install(dynamic: &[DynamicRecipe]) {
    let index = Arc::new(RecipeIndex::build(dynamic));
    *INDEX.write().unwrap_or_else(PoisonError::into_inner) = index;
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_protocol::codec::recipe::OwnedStonecutterRecipe;

    fn smelting(id: &str, ingredient: &str, result: &str) -> DynamicRecipe {
        DynamicRecipe::Cooking(OwnedCookingRecipeType::Smelting(OwnedCookingRecipe {
            recipe_id: id.to_string(),
            category: pumpkin_data::recipes::RecipeCategoryTypes::Misc,
            group: None,
            ingredient: OwnedRecipeIngredient::Simple(ingredient.to_string()),
            cooking_time: 50,
            experience: 3.0,
            result: OwnedRecipeResult {
                item_id: result.to_string(),
                count: 1,
            },
        }))
    }

    #[test]
    fn sort_key_compares_path_before_namespace() {
        assert!(sort_key("zzz:a") < sort_key("aaa:b"));
        assert!(sort_key("aaa:a") < sort_key("zzz:a"));
        // `-` sorts before `.`, so the `.json` suffix changes the order of these two.
        assert!(sort_key("minecraft:a-b") < sort_key("minecraft:a"));
    }

    #[test]
    fn generated_recipes_only() {
        let index = RecipeIndex::build(&[]);
        let recipe = index
            .cooking_recipe(&Item::RAW_IRON, &CookingRecipeKind::Smelting)
            .expect("vanilla smelts raw iron");
        assert_eq!(recipe.result.item_id, "minecraft:iron_ingot");
        assert!(
            index
                .property_set(FURNACE_INPUT)
                .is_some_and(|s| s.test(&Item::RAW_IRON))
        );
        assert!(
            index
                .property_set(BREWING_REAGENT)
                .is_some_and(|s| s.test(&Item::NETHER_WART))
        );
        assert!(
            index
                .property_set(SMITHING_ADDITION)
                .is_some_and(|s| s.test(&Item::NETHERITE_INGOT))
        );
        assert!(index.stonecutter_recipes_for(&Item::STONE).count() > 5);
    }

    #[test]
    fn data_pack_recipe_replaces_the_generated_recipe_with_its_id() {
        let id = "minecraft:iron_ingot_from_smelting_raw_iron";
        let index = RecipeIndex::build(&[smelting(id, "minecraft:raw_iron", "minecraft:diamond")]);
        let recipe = index
            .cooking_recipe(&Item::RAW_IRON, &CookingRecipeKind::Smelting)
            .expect("the override smelts raw iron");
        assert_eq!(recipe.result.item_id, "minecraft:diamond");
        assert_eq!(index.cooking_experience(id), Some(3.0));
        let count = index
            .cooking_recipes()
            .iter()
            .filter(|r| r.recipe_id == id)
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn first_recipe_in_vanilla_order_wins() {
        // The path compares first: `z:a` sorts before the vanilla raw iron recipe, `a:zzz` after.
        let early = smelting("z:a", "minecraft:raw_iron", "minecraft:diamond");
        let late = smelting("a:zzz", "minecraft:raw_iron", "minecraft:emerald");
        let index = RecipeIndex::build(&[late, early]);
        let recipe = index
            .cooking_recipe(&Item::RAW_IRON, &CookingRecipeKind::Smelting)
            .expect("a recipe smelts raw iron");
        assert_eq!(recipe.recipe_id, "z:a");
    }

    #[test]
    fn a_data_pack_cooking_recipe_adds_to_its_property_set() {
        let mut recipe = smelting("test:dirt", "minecraft:dirt", "minecraft:diamond");
        if let DynamicRecipe::Cooking(OwnedCookingRecipeType::Smelting(r)) = recipe {
            recipe = DynamicRecipe::Cooking(OwnedCookingRecipeType::Smoking(r));
        }
        let index = RecipeIndex::build(&[recipe]);
        assert!(
            index
                .property_set(SMOKER_INPUT)
                .is_some_and(|s| s.test(&Item::DIRT))
        );
        assert!(
            !index
                .property_set(FURNACE_INPUT)
                .is_some_and(|s| s.test(&Item::DIRT))
        );
        assert!(
            index
                .cooking_recipe(&Item::DIRT, &CookingRecipeKind::Smoking)
                .is_some()
        );
        assert!(
            index
                .cooking_recipe(&Item::DIRT, &CookingRecipeKind::Smelting)
                .is_none()
        );
    }

    #[test]
    fn data_pack_stonecutter_recipe_sorts_among_generated_recipes() {
        let recipe = DynamicRecipe::Stonecutting(OwnedStonecutterRecipe {
            recipe_id: "test:stone_bricks_from_stone_stonecutting".to_string(),
            group: None,
            ingredient: OwnedRecipeIngredient::Simple("minecraft:stone".to_string()),
            result: OwnedRecipeResult {
                item_id: "minecraft:diamond".to_string(),
                count: 1,
            },
        });
        let index = RecipeIndex::build(&[recipe]);
        let ids: Vec<&str> = index
            .stonecutter_recipes_for(&Item::STONE)
            .map(|r| r.recipe_id.as_str())
            .collect();
        let at = ids
            .iter()
            .position(|id| *id == "test:stone_bricks_from_stone_stonecutting")
            .expect("the data pack recipe is in the list");
        // The paths are equal, so the namespace decides.
        assert_eq!(
            ids[at - 1],
            "minecraft:stone_bricks_from_stone_stonecutting"
        );
    }
}
