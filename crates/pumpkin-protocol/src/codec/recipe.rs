use pumpkin_data::recipes::RecipeCategoryTypes;

use pumpkin_data::dynamic::{DynamicTaggable, namespaced_name, tag_ids};
use pumpkin_data::item::Item;
use pumpkin_data::tag::RegistryKey;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnedRecipeIngredient {
    Simple(String),
    Tagged(String),
    OneOf(Vec<String>),
}

impl OwnedRecipeIngredient {
    #[must_use]
    pub fn match_item(&self, item: &Item) -> bool {
        match self {
            Self::Simple(id) => {
                let name = namespaced_name(item.registry_key);
                name == *id
            }
            Self::Tagged(tag) => item.has_tag_dynamic(tag),
            Self::OneOf(ids) => {
                let name = namespaced_name(item.registry_key);
                ids.iter().any(|id| *id == name)
            }
        }
    }

    /// Vanilla `Ingredient.testOptionalIngredient`: an absent ingredient takes only an empty
    /// slot, which holds air.
    #[must_use]
    pub fn match_optional(ingredient: Option<&Self>, item: &Item) -> bool {
        ingredient.map_or(item.id == Item::AIR.id, |ingredient| {
            ingredient.match_item(item)
        })
    }

    /// The items that [`Self::match_item`] accepts, in tag order and without duplicates. A tag
    /// resolves through the server's tag lookup, so a mod tag and the custom members of a tag
    /// are in the list.
    #[must_use]
    pub fn items(&self) -> Vec<&'static Item> {
        let mut items: Vec<&'static Item> = Vec::new();
        let mut push = |item: &'static Item| {
            if !items.iter().any(|known| known.id == item.id) {
                items.push(item);
            }
        };
        match self {
            Self::Simple(id) => {
                if let Some(item) = Item::from_registry_key(id) {
                    push(item);
                }
            }
            Self::Tagged(tag) => {
                if let Some((generated, custom)) = tag_ids(RegistryKey::Item, tag) {
                    for item in generated
                        .iter()
                        .chain(custom)
                        .filter_map(|&id| Item::from_id(id))
                    {
                        push(item);
                    }
                }
            }
            Self::OneOf(ids) => {
                for item in ids.iter().filter_map(|id| Item::from_registry_key(id)) {
                    push(item);
                }
            }
        }
        items
    }
}

#[derive(Clone, Debug)]
pub struct OwnedRecipeResult {
    pub item_id: String,
    pub count: u8,
    // TODO: Add components/enchantments if needed for the display result
}

#[derive(Clone, Debug)]
pub enum OwnedCraftingRecipe {
    Shaped {
        recipe_id: Option<String>,
        category: RecipeCategoryTypes,
        group: Option<String>,
        show_notification: bool,
        key: Vec<(char, OwnedRecipeIngredient)>,
        pattern: Vec<String>,
        result: OwnedRecipeResult,
    },
    Shapeless {
        recipe_id: Option<String>,
        category: RecipeCategoryTypes,
        group: Option<String>,
        ingredients: Vec<OwnedRecipeIngredient>,
        result: OwnedRecipeResult,
    },
}

#[derive(Clone, Debug)]
pub struct OwnedCookingRecipe {
    pub recipe_id: String,
    pub category: RecipeCategoryTypes,
    pub group: Option<String>,
    pub ingredient: OwnedRecipeIngredient,
    pub cooking_time: i32,
    pub experience: f32,
    pub result: OwnedRecipeResult,
}

#[derive(Clone, Debug)]
pub enum OwnedCookingRecipeType {
    Blasting(OwnedCookingRecipe),
    Smelting(OwnedCookingRecipe),
    Smoking(OwnedCookingRecipe),
    CampfireCooking(OwnedCookingRecipe),
}

#[derive(Clone, Debug)]
pub struct OwnedBrewingRecipe {
    pub recipe_id: String,
    pub input_item: OwnedRecipeIngredient,
    pub input_potion: Option<String>,
    pub reagent: OwnedRecipeIngredient,
    pub output_item: String,
    pub output_potion: Option<String>,
}

#[derive(Clone, Debug)]
pub struct OwnedStonecutterRecipe {
    pub recipe_id: String,
    pub group: Option<String>,
    pub ingredient: OwnedRecipeIngredient,
    pub result: OwnedRecipeResult,
}

#[derive(Clone, Debug)]
pub struct OwnedSmithingTransformRecipe {
    pub recipe_id: String,
    /// Optional, as in vanilla `SmithingTransformRecipe`: without it the slot must be empty.
    pub template: Option<OwnedRecipeIngredient>,
    pub base: OwnedRecipeIngredient,
    /// Optional, as in vanilla `SmithingTransformRecipe`: without it the slot must be empty.
    pub addition: Option<OwnedRecipeIngredient>,
    pub result: OwnedRecipeResult,
}

#[derive(Clone, Debug)]
pub struct OwnedSmithingTrimRecipe {
    pub recipe_id: String,
    pub template: OwnedRecipeIngredient,
    pub base: OwnedRecipeIngredient,
    pub addition: OwnedRecipeIngredient,
    pub pattern: String,
}

#[derive(Clone, Debug)]
pub struct OwnedSpecialCraftingRecipe {
    pub recipe_id: String,
    pub category: RecipeCategoryTypes,
}

#[derive(Clone, Debug)]
pub enum DynamicRecipe {
    Crafting(OwnedCraftingRecipe),
    Cooking(OwnedCookingRecipeType),
    Brewing(OwnedBrewingRecipe),
    Stonecutting(OwnedStonecutterRecipe),
    SmithingTransform(OwnedSmithingTransformRecipe),
    SmithingTrim(OwnedSmithingTrimRecipe),
    SpecialCrafting(OwnedSpecialCraftingRecipe),
}

impl DynamicRecipe {
    #[must_use]
    pub fn recipe_id(&self) -> Option<&str> {
        match self {
            Self::Crafting(
                OwnedCraftingRecipe::Shaped { recipe_id, .. }
                | OwnedCraftingRecipe::Shapeless { recipe_id, .. },
            ) => recipe_id.as_deref(),
            Self::Cooking(
                OwnedCookingRecipeType::Blasting(r)
                | OwnedCookingRecipeType::Smelting(r)
                | OwnedCookingRecipeType::Smoking(r)
                | OwnedCookingRecipeType::CampfireCooking(r),
            ) => Some(&r.recipe_id),
            Self::Brewing(r) => Some(&r.recipe_id),
            Self::Stonecutting(r) => Some(&r.recipe_id),
            Self::SmithingTransform(r) => Some(&r.recipe_id),
            Self::SmithingTrim(r) => Some(&r.recipe_id),
            Self::SpecialCrafting(r) => Some(&r.recipe_id),
        }
    }
}

/// A vanilla `RecipePropertySet` to sync: the items of `ingredients`.
pub struct PropertySetSync<'a> {
    /// The `minecraft:recipe_property_set` key, for example `minecraft:furnace_input`.
    pub key: &'a str,
    pub ingredients: &'a [OwnedRecipeIngredient],
}

/// A stonecutter recipe to sync.
pub struct StonecutterSync<'a> {
    pub ingredient: &'a OwnedRecipeIngredient,
    pub result_item: &'a str,
    pub result_count: u8,
}
