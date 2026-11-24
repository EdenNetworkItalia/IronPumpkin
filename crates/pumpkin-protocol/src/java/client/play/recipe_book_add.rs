use crate::EncodingKey;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::packet::clientbound::play::RECIPE_BOOK_ADD;
use pumpkin_data::recipes::{
    CookingRecipeType, CraftingRecipeTypes, RecipeCategoryTypes, RecipeIngredientTypes,
    RecipeResultStruct,
};
use pumpkin_macros::java_packet;
use std::borrow::Cow;
use std::io::Write;

use crate::codec::item_stack_seralizer::ItemStackTemplateSerializer;
use crate::codec::recipe::OwnedRecipeIngredient;
use crate::{ClientPacket, VarInt, WritingError, ser::NetworkWriteExt};

// Recipe Display type IDs
const RECIPE_DISPLAY_SHAPELESS: i32 = 0;
const RECIPE_DISPLAY_SHAPED: i32 = 1;
const RECIPE_DISPLAY_FURNACE: i32 = 2;

// Slot Display base type IDs (26.2)
const SLOT_DISPLAY_EMPTY: u32 = 0;
const SLOT_DISPLAY_ANY_FUEL: u32 = 1;
const SLOT_DISPLAY_ITEM: u32 = 4;
const SLOT_DISPLAY_ITEM_STACK: u32 = 5;
const SLOT_DISPLAY_COMPOSITE: u32 = 10;

const ENTRY_FLAG_NOTIFICATION: u8 = 0x01;
const ENTRY_FLAG_HIGHLIGHT: u8 = 0x02;

// RecipeBookCategory IDs
const CATEGORY_CRAFTING_BUILDING: i32 = 0;
const CATEGORY_CRAFTING_REDSTONE: i32 = 1;
const CATEGORY_CRAFTING_EQUIPMENT: i32 = 2;
const CATEGORY_CRAFTING_MISC: i32 = 3;
const CATEGORY_FURNACE_FOOD: i32 = 4;
const CATEGORY_FURNACE_BLOCKS: i32 = 5;
const CATEGORY_FURNACE_MISC: i32 = 6;
const CATEGORY_BLAST_FURNACE_BLOCKS: i32 = 7;
const CATEGORY_BLAST_FURNACE_MISC: i32 = 8;
const CATEGORY_SMOKER_FOOD: i32 = 9;
const CATEGORY_CAMPFIRE: i32 = 12;

use crate::codec::recipe::{DynamicRecipe, OwnedCookingRecipeType, OwnedCraftingRecipe};
use crate::codec::recipe_book::{BookRecipe, RecipeBook, owned_cooking};

/// Clientbound packet that adds recipes to the client's recipe book.
/// `replace = true` means the client replaces its current recipe list.
#[java_packet(RECIPE_BOOK_ADD)]
pub struct CRecipeBookAdd<'a> {
    pub replace: bool,
    pub dynamic_recipes: &'a [DynamicRecipe],
}

impl<'a> CRecipeBookAdd<'a> {
    #[must_use]
    pub const fn new(replace: bool, dynamic_recipes: &'a [DynamicRecipe]) -> Self {
        Self {
            replace,
            dynamic_recipes,
        }
    }
}

fn write_item_slot_display(
    write: &mut impl Write,
    item: &Item,
    version: EncodingKey,
) -> Result<(), WritingError> {
    write.write_var_int(&VarInt(SLOT_DISPLAY_ITEM as i32))?;
    write.write_var_int(&VarInt(i32::from(
        item.to_java_network_id(version.content_ids()),
    )))?;
    Ok(())
}

pub(super) fn write_item_stack_slot_display(
    write: &mut impl Write,
    item: &Item,
    count: u8,
    version: EncodingKey,
) -> Result<(), WritingError> {
    let static_item = Item::from_id(item.id)
        .ok_or_else(|| WritingError::Message(format!("item id {} must exist", item.id)))?;
    let stack = ItemStack::new(count, static_item);
    // A result without an `id` lowers to air, which vanilla rejects as an item stack
    if stack.is_empty() {
        return write_empty_slot_display(write, version);
    }
    write.write_var_int(&VarInt(SLOT_DISPLAY_ITEM_STACK as i32))?;
    ItemStackTemplateSerializer::from(stack).write_with_version(write, &version)
}

pub(super) fn write_empty_slot_display(
    write: &mut impl Write,
    _version: EncodingKey,
) -> Result<(), WritingError> {
    write.write_var_int(&VarInt(SLOT_DISPLAY_EMPTY as i32))?;
    Ok(())
}

fn write_any_fuel_slot_display(
    write: &mut impl Write,
    _version: EncodingKey,
) -> Result<(), WritingError> {
    write.write_var_int(&VarInt(SLOT_DISPLAY_ANY_FUEL as i32))?;
    Ok(())
}

/// The items of `tag` as the server matches them: the generated and the custom members of a
/// generated or mod tag, one item per network id of the connection. `None` when the tag is
/// unknown or empty.
pub(super) fn resolve_item_tag(tag: &str, version: EncodingKey) -> Option<Vec<&'static Item>> {
    let mut ids = Vec::new();
    let items: Vec<&'static Item> = OwnedRecipeIngredient::Tagged(tag.to_string())
        .items()
        .into_iter()
        .filter(|item| {
            let id = item.to_java_network_id(version.content_ids());
            let new = !ids.contains(&id);
            ids.push(id);
            new
        })
        .collect();
    if items.is_empty() { None } else { Some(items) }
}

/// The network ids of the items of `items` that the client matches as the server does, without
/// duplicates. A `Display` client sees a custom item as its display item, so the list leaves the
/// custom item out: the client would else match the display item.
pub(super) fn synced_item_ids(items: &[&'static Item], version: EncodingKey) -> Vec<i32> {
    let ids = version.content_ids();
    let items: Vec<&'static Item> = items
        .iter()
        .copied()
        .filter(|item| item.id < Item::COUNT || item.to_java_network_id(ids) >= Item::COUNT)
        .collect();
    network_item_ids(&items, version)
}

/// The network ids of `items` for the connection, without duplicates. A custom item can map to
/// its display item, which can already be in the list.
fn network_item_ids(items: &[&'static Item], version: EncodingKey) -> Vec<i32> {
    let mut ids = Vec::with_capacity(items.len());
    for item in items {
        let id = i32::from(item.to_java_network_id(version.content_ids()));
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

fn write_ingredient_slot_display(
    write: &mut impl Write,
    ingredient: &RecipeIngredientTypes,
    version: EncodingKey,
) -> Result<(), WritingError> {
    match ingredient {
        RecipeIngredientTypes::Simple(id) => {
            let key = id.strip_prefix("minecraft:").unwrap_or(id);
            if let Some(item) = Item::from_registry_key(key) {
                write_item_slot_display(write, item, version)?;
            } else {
                write_empty_slot_display(write, version)?;
            }
        }
        RecipeIngredientTypes::Tagged(tag) => {
            if let Some(items) = resolve_item_tag(tag, version) {
                if items.len() == 1 {
                    write_item_slot_display(write, items[0], version)?;
                } else {
                    write.write_var_int(&VarInt(SLOT_DISPLAY_COMPOSITE as i32))?;
                    write.write_var_int(&VarInt(items.len() as i32))?;
                    for item in &items {
                        write_item_slot_display(write, item, version)?;
                    }
                }
            } else {
                write_empty_slot_display(write, version)?;
            }
        }
        RecipeIngredientTypes::OneOf(ids) => {
            let mut items: Vec<&Item> = Vec::new();
            for id in *ids {
                let key = id.strip_prefix("minecraft:").unwrap_or(id);
                if let Some(item) = Item::from_registry_key(key) {
                    items.push(item);
                }
            }
            if items.is_empty() {
                write_empty_slot_display(write, version)?;
            } else if items.len() == 1 {
                write_item_slot_display(write, items[0], version)?;
            } else {
                write.write_var_int(&VarInt(SLOT_DISPLAY_COMPOSITE as i32))?;
                write.write_var_int(&VarInt(items.len() as i32))?;
                for item in &items {
                    write_item_slot_display(write, item, version)?;
                }
            }
        }
    }
    Ok(())
}

/// Write a single Ingredient as a `HolderSet`<Item> for craftingRequirements.
///
/// Vanilla wire format for `ByteBufCodecs.holderSet(Registries.ITEM)`:
///   VarInt(0)     -> named tag reference (followed by `ResourceLocation`)
///   VarInt(n + 1) -> direct list of n item IDs
fn write_ingredient_holderset(
    write: &mut impl Write,
    ingredient: &RecipeIngredientTypes,
    version: EncodingKey,
) -> Result<(), WritingError> {
    match ingredient {
        RecipeIngredientTypes::Simple(id) => {
            let key = id.strip_prefix("minecraft:").unwrap_or(id);
            // 1 item -> VarInt(1 + 1) = VarInt(2)
            write.write_var_int(&VarInt(2))?;
            if let Some(item) = Item::from_registry_key(key) {
                write.write_var_int(&VarInt(i32::from(
                    item.to_java_network_id(version.content_ids()),
                )))?;
            } else {
                // Non-empty fallback item to prevent client UnsupportedOperationException
                write.write_var_int(&VarInt(0))?;
            }
        }
        RecipeIngredientTypes::Tagged(tag) => {
            if let Some(items) = resolve_item_tag(tag, version) {
                write.write_var_int(&VarInt(items.len() as i32 + 1))?;
                for item in &items {
                    write.write_var_int(&VarInt(i32::from(
                        item.to_java_network_id(version.content_ids()),
                    )))?;
                }
            } else {
                let tag = tag.strip_prefix('#').unwrap_or(tag);
                let full_tag = if tag.contains(':') {
                    tag.to_string()
                } else {
                    format!("minecraft:{tag}")
                };
                write.write_var_int(&VarInt(0))?;
                write.write_string(&full_tag)?;
            }
        }
        RecipeIngredientTypes::OneOf(ids) => {
            let items: Vec<i32> = ids
                .iter()
                .filter_map(|id| {
                    let key = id.strip_prefix("minecraft:").unwrap_or(id);
                    Item::from_registry_key(key)
                        .map(|item| i32::from(item.to_java_network_id(version.content_ids())))
                })
                .collect();
            if items.is_empty() {
                write.write_var_int(&VarInt(2))?;
                write.write_var_int(&VarInt(0))?;
            } else {
                write.write_var_int(&VarInt(items.len() as i32 + 1))?;
                for id in &items {
                    write.write_var_int(&VarInt(*id))?;
                }
            }
        }
    }
    Ok(())
}

/// Write the `craftingRequirements: Option<List<Ingredient>>` field (present).
fn write_crafting_requirements(
    write: &mut impl Write,
    slots: &[&RecipeIngredientTypes],
    version: EncodingKey,
) -> Result<(), WritingError> {
    write.write_bool(true)?; // present
    write.write_var_int(&VarInt(slots.len() as i32))?;
    for slot in slots {
        write_ingredient_holderset(write, slot, version)?;
    }
    Ok(())
}

fn write_result_slot_display(
    write: &mut impl Write,
    result: &RecipeResultStruct,
    version: EncodingKey,
) -> Result<(), WritingError> {
    let key = result.id.strip_prefix("minecraft:").unwrap_or(result.id);
    if let Some(item) = Item::from_registry_key(key) {
        write_item_stack_slot_display(write, item, result.count, version)?;
    } else {
        write_empty_slot_display(write, version)?;
    }
    Ok(())
}

fn write_optional_var_int(write: &mut impl Write, value: Option<i32>) -> Result<(), WritingError> {
    let encoded = value.map_or(Ok(0), |v| {
        v.checked_add(1)
            .ok_or_else(|| WritingError::Message(format!("group id {v} overflow")))
    })?;
    write.write_var_int(&VarInt(encoded))?;
    Ok(())
}
const fn entry_flags(replace: bool, notification: bool, highlight: bool) -> u8 {
    if replace {
        return 0;
    }

    (if notification {
        ENTRY_FLAG_NOTIFICATION
    } else {
        0
    }) | (if highlight { ENTRY_FLAG_HIGHLIGHT } else { 0 })
}

const fn crafting_category(cat: &RecipeCategoryTypes) -> i32 {
    match cat {
        RecipeCategoryTypes::Equipment => CATEGORY_CRAFTING_EQUIPMENT,
        RecipeCategoryTypes::Building | RecipeCategoryTypes::Blocks => CATEGORY_CRAFTING_BUILDING,
        RecipeCategoryTypes::Restone => CATEGORY_CRAFTING_REDSTONE,
        RecipeCategoryTypes::Food | RecipeCategoryTypes::Misc => CATEGORY_CRAFTING_MISC,
    }
}

/// Write a single `RecipeDisplayEntry` + flags byte.
/// Returns `Ok(true)` if written, `Ok(false)` if skipped (special recipe).
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn write_entry(
    write: &mut impl Write,
    display_id: i32,
    version: EncodingKey,
    group_id: Option<i32>,
    flags: u8,
    crafting_table: &Item,
    furnace: &Item,
    blast_furnace: &Item,
    smoker: &Item,
    campfire: &Item,
    crafting_recipe: Option<&CraftingRecipeTypes>,
    cooking_recipe: Option<(&CookingRecipeType, i32)>,
) -> Result<bool, WritingError> {
    if let Some(recipe) = crafting_recipe {
        match recipe {
            CraftingRecipeTypes::CraftingShaped {
                category,
                pattern,
                key,
                result,
                ..
            } => {
                // Compute width and height from pattern
                let height = pattern.len() as i32;
                let width = pattern.first().map_or(0, |r| r.len()) as i32;

                // RecipeDisplayId
                write.write_var_int(&VarInt(display_id))?;
                // RecipeDisplay type = shaped (1)
                write.write_var_int(&VarInt(RECIPE_DISPLAY_SHAPED))?;
                // width, height
                write.write_var_int(&VarInt(width))?;
                write.write_var_int(&VarInt(height))?;
                // ingredients: flat list, row by row
                write.write_var_int(&VarInt(width * height))?;
                for row in *pattern {
                    for ch in row.chars() {
                        if ch == ' ' {
                            write_empty_slot_display(write, version)?;
                        } else if let Some((_, ingredient)) = key.iter().find(|(k, _)| *k == ch) {
                            write_ingredient_slot_display(write, ingredient, version)?;
                        } else {
                            write_empty_slot_display(write, version)?;
                        }
                    }
                }
                // result
                write_result_slot_display(write, result, version)?;
                // craftingStation
                write_item_slot_display(write, crafting_table, version)?;
                // group: OptionalVarInt
                write_optional_var_int(write, group_id)?;
                // category
                write.write_var_int(&VarInt(crafting_category(category)))?;
                // craftingRequirements: one HolderSet per non-empty grid slot
                // (Ingredient cannot be empty, so empty slots must be excluded)
                {
                    let mut slots: Vec<&RecipeIngredientTypes> = Vec::new();
                    for row in *pattern {
                        for ch in row.chars() {
                            if ch != ' '
                                && let Some((_, ing)) = key.iter().find(|(k, _)| *k == ch)
                            {
                                slots.push(ing);
                            }
                        }
                    }
                    write_crafting_requirements(write, &slots, version)?;
                };
                write.write_u8(flags)?;
            }
            CraftingRecipeTypes::CraftingShapeless {
                category,
                ingredients,
                result,
                ..
            } => {
                // RecipeDisplayId
                write.write_var_int(&VarInt(display_id))?;
                // RecipeDisplay type = shapeless (0)
                write.write_var_int(&VarInt(RECIPE_DISPLAY_SHAPELESS))?;
                // ingredients list
                write.write_var_int(&VarInt(ingredients.len() as i32))?;
                for ing in *ingredients {
                    write_ingredient_slot_display(write, ing, version)?;
                }
                // result
                write_result_slot_display(write, result, version)?;
                // craftingStation
                write_item_slot_display(write, crafting_table, version)?;
                // group: OptionalVarInt
                write_optional_var_int(write, group_id)?;
                // category
                write.write_var_int(&VarInt(crafting_category(category)))?;
                // craftingRequirements: one HolderSet per ingredient
                {
                    let slots: Vec<&RecipeIngredientTypes> = ingredients.iter().collect();
                    write_crafting_requirements(write, &slots, version)?;
                };
                write.write_u8(flags)?;
            }
            CraftingRecipeTypes::CraftingTransmute {
                category,
                input,
                material,
                result,
                ..
            } => {
                // Transmute shown as shapeless with 2 ingredients
                write.write_var_int(&VarInt(display_id))?;
                write.write_var_int(&VarInt(RECIPE_DISPLAY_SHAPELESS))?;
                // 2 ingredients
                write.write_var_int(&VarInt(2))?;
                write_ingredient_slot_display(write, input, version)?;
                write_ingredient_slot_display(write, material, version)?;
                write_result_slot_display(write, result, version)?;
                write_item_slot_display(write, crafting_table, version)?;
                write_optional_var_int(write, group_id)?;
                write.write_var_int(&VarInt(crafting_category(category)))?;
                // craftingRequirements: input + material
                write_crafting_requirements(write, &[input, material], version)?;
                write.write_u8(flags)?;
            }
            // Skip special/decorated_pot recipes as they have no useful display
            CraftingRecipeTypes::CraftingDecoratedPot { .. }
            | CraftingRecipeTypes::CraftingSpecial(_) => {
                return Ok(false);
            }
        }
        return Ok(true);
    }

    if let Some((recipe, book_category)) = cooking_recipe {
        let (cooking, station) = match recipe {
            CookingRecipeType::Smelting(r) => (r, furnace),
            CookingRecipeType::Blasting(r) => (r, blast_furnace),
            CookingRecipeType::Smoking(r) => (r, smoker),
            CookingRecipeType::CampfireCooking(r) => (r, campfire),
        };

        write.write_var_int(&VarInt(display_id))?;
        // RecipeDisplay type = furnace (2)
        write.write_var_int(&VarInt(RECIPE_DISPLAY_FURNACE))?;
        // ingredient
        write_ingredient_slot_display(write, &cooking.ingredient, version)?;
        // fuel: AnyFuel
        write_any_fuel_slot_display(write, version)?;
        // result
        write_result_slot_display(write, &cooking.result, version)?;
        // craftingStation
        write_item_slot_display(write, station, version)?;
        // duration
        write.write_var_int(&VarInt(cooking.cookingtime))?;
        // experience
        write.write_f32_be(cooking.experience)?;
        // group: OptionalVarInt
        write_optional_var_int(write, group_id)?;
        // category
        write.write_var_int(&VarInt(book_category))?;
        // craftingRequirements: the single ingredient
        write_crafting_requirements(write, &[&cooking.ingredient], version)?;
        write.write_u8(flags)?;
        return Ok(true);
    }

    Ok(false)
}

impl ClientPacket for CRecipeBookAdd<'_> {
    fn write_packet_data(
        &self,
        write: impl Write,
        version: &EncodingKey,
    ) -> Result<(), WritingError> {
        let book = RecipeBook::new(self.dynamic_recipes);
        write_recipe_book(
            write,
            *version,
            self.replace,
            &book,
            self.dynamic_recipes,
            None,
        )
    }
}

/// Writes the body of vanilla `ClientboundRecipeBookAddPacket`.
///
/// It writes the entries of `book` with the display ids `only`, or every entry.
#[allow(clippy::too_many_lines)]
pub fn write_recipe_book(
    write: impl Write,
    version: EncodingKey,
    replace: bool,
    book: &RecipeBook,
    dynamic_recipes: &[DynamicRecipe],
    only: Option<&[i32]>,
) -> Result<(), WritingError> {
    {
        let mut write = write;

        // Station items (these IDs are stable across all versions we support)
        let crafting_table = Item::from_registry_key("crafting_table")
            .ok_or_else(|| WritingError::Message("crafting_table item must exist".into()))?;
        let furnace = Item::from_registry_key("furnace")
            .ok_or_else(|| WritingError::Message("furnace item must exist".into()))?;
        let blast_furnace = Item::from_registry_key("blast_furnace")
            .ok_or_else(|| WritingError::Message("blast_furnace item must exist".into()))?;
        let smoker = Item::from_registry_key("smoker")
            .ok_or_else(|| WritingError::Message("smoker item must exist".into()))?;
        let campfire = Item::from_registry_key("campfire")
            .ok_or_else(|| WritingError::Message("campfire item must exist".into()))?;

        let entries: Vec<(i32, BookRecipe<'_>)> = book
            .display_ids()
            .filter(|id| only.is_none_or(|only| only.contains(id)))
            .filter_map(|id| Some((id, book.recipe(id, dynamic_recipes)?)))
            .collect();
        write.write_var_int(&VarInt(entries.len() as i32))?;

        let highlight = !replace;
        for (display_id, recipe) in entries {
            let group_id = book.group(display_id);
            match recipe {
                BookRecipe::Crafting(recipe) => {
                    let notification = match recipe {
                        CraftingRecipeTypes::CraftingShaped {
                            show_notification, ..
                        } => *show_notification,
                        _ => true,
                    };
                    write_entry(
                        &mut write,
                        display_id,
                        version,
                        group_id,
                        entry_flags(replace, notification, highlight),
                        crafting_table,
                        furnace,
                        blast_furnace,
                        smoker,
                        campfire,
                        Some(recipe),
                        None,
                    )?;
                }
                BookRecipe::Cooking(recipe) => {
                    let book_category = match recipe {
                        CookingRecipeType::Smelting(r) => match r.category {
                            RecipeCategoryTypes::Food => CATEGORY_FURNACE_FOOD,
                            RecipeCategoryTypes::Blocks => CATEGORY_FURNACE_BLOCKS,
                            _ => CATEGORY_FURNACE_MISC,
                        },
                        CookingRecipeType::Blasting(r) => match r.category {
                            RecipeCategoryTypes::Blocks => CATEGORY_BLAST_FURNACE_BLOCKS,
                            _ => CATEGORY_BLAST_FURNACE_MISC,
                        },
                        CookingRecipeType::Smoking(_) => CATEGORY_SMOKER_FOOD,
                        CookingRecipeType::CampfireCooking(_) => CATEGORY_CAMPFIRE,
                    };
                    write_entry(
                        &mut write,
                        display_id,
                        version,
                        group_id,
                        entry_flags(replace, true, highlight),
                        crafting_table,
                        furnace,
                        blast_furnace,
                        smoker,
                        campfire,
                        None,
                        Some((recipe, book_category)),
                    )?;
                }
                BookRecipe::Dynamic(DynamicRecipe::Crafting(crafting)) => {
                    let notification = match crafting {
                        OwnedCraftingRecipe::Shaped {
                            show_notification, ..
                        } => *show_notification,
                        OwnedCraftingRecipe::Shapeless { .. } => true,
                    };
                    write_dynamic_crafting_entry(
                        &mut write,
                        display_id,
                        version,
                        group_id,
                        entry_flags(replace, notification, highlight),
                        crafting_table,
                        crafting,
                    )?;
                }
                BookRecipe::Dynamic(DynamicRecipe::Cooking(cooking)) => {
                    let (book_category, station) = match cooking {
                        OwnedCookingRecipeType::Smelting(r) => (
                            match r.category {
                                RecipeCategoryTypes::Food => CATEGORY_FURNACE_FOOD,
                                RecipeCategoryTypes::Blocks => CATEGORY_FURNACE_BLOCKS,
                                _ => CATEGORY_FURNACE_MISC,
                            },
                            furnace,
                        ),
                        OwnedCookingRecipeType::Blasting(r) => (
                            match r.category {
                                RecipeCategoryTypes::Blocks => CATEGORY_BLAST_FURNACE_BLOCKS,
                                _ => CATEGORY_BLAST_FURNACE_MISC,
                            },
                            blast_furnace,
                        ),
                        OwnedCookingRecipeType::Smoking(_) => (CATEGORY_SMOKER_FOOD, smoker),
                        OwnedCookingRecipeType::CampfireCooking(_) => (CATEGORY_CAMPFIRE, campfire),
                    };
                    write_dynamic_cooking_entry(
                        &mut write,
                        display_id,
                        version,
                        group_id,
                        entry_flags(replace, true, highlight),
                        station,
                        owned_cooking(cooking),
                        book_category,
                    )?;
                }
                // `RecipeBook` holds crafting and cooking recipes only.
                BookRecipe::Dynamic(_) => {}
            }
        }

        // replace flag
        write.write_bool(replace)?;
        Ok(())
    }
}

fn write_dynamic_ingredient_slot_display(
    write: &mut impl Write,
    ingredient: &crate::codec::recipe::OwnedRecipeIngredient,
    version: EncodingKey,
) -> Result<(), WritingError> {
    match ingredient {
        crate::codec::recipe::OwnedRecipeIngredient::Simple(id) => {
            let key = id.strip_prefix("minecraft:").unwrap_or(id);
            if let Some(item) = Item::from_registry_key(key) {
                write_item_slot_display(write, item, version)?;
            } else {
                write_empty_slot_display(write, version)?;
            }
        }
        crate::codec::recipe::OwnedRecipeIngredient::Tagged(tag) => {
            if let Some(items) = resolve_item_tag(tag, version) {
                if items.len() == 1 {
                    write_item_slot_display(write, items[0], version)?;
                } else {
                    write.write_var_int(&VarInt(SLOT_DISPLAY_COMPOSITE as i32))?;
                    write.write_var_int(&VarInt(items.len() as i32))?;
                    for item in &items {
                        write_item_slot_display(write, item, version)?;
                    }
                }
            } else {
                write_empty_slot_display(write, version)?;
            }
        }
        crate::codec::recipe::OwnedRecipeIngredient::OneOf(ids) => {
            let items: Vec<&Item> = ids
                .iter()
                .filter_map(|id| {
                    let key = id.strip_prefix("minecraft:").unwrap_or(id);
                    Item::from_registry_key(key)
                })
                .collect();

            if items.is_empty() {
                write_empty_slot_display(write, version)?;
            } else if items.len() == 1 {
                write_item_slot_display(write, items[0], version)?;
            } else {
                write.write_var_int(&VarInt(SLOT_DISPLAY_COMPOSITE as i32))?;
                write.write_var_int(&VarInt(items.len() as i32))?;
                for item in &items {
                    write_item_slot_display(write, item, version)?;
                }
            }
        }
    }
    Ok(())
}

/// Writes `ingredient` as `ByteBufCodecs.holderSet`, always as a direct list: a client knows
/// the items of the list, but a `Display` client does not know a mod tag.
pub(super) fn write_dynamic_ingredient_holderset(
    write: &mut impl Write,
    ingredient: &OwnedRecipeIngredient,
    version: EncodingKey,
) -> Result<(), WritingError> {
    let ids = synced_item_ids(&ingredient.items(), version);
    // Vanilla `Ingredient` rejects an empty list, and `RecipeManager` drops such recipes.
    if ids.is_empty() {
        return Err(WritingError::Message(format!(
            "recipe ingredient {ingredient:?} holds no item"
        )));
    }
    write.write_var_int(&VarInt(ids.len() as i32 + 1))?;
    for id in ids {
        write.write_var_int(&VarInt(id))?;
    }
    Ok(())
}

fn write_dynamic_result_slot_display(
    write: &mut impl Write,
    result: &crate::codec::recipe::OwnedRecipeResult,
    version: EncodingKey,
) -> Result<(), WritingError> {
    let key = result
        .item_id
        .strip_prefix("minecraft:")
        .unwrap_or(&result.item_id);
    if let Some(item) = Item::from_registry_key(key) {
        write_item_stack_slot_display(write, item, result.count, version)?;
    } else {
        write_empty_slot_display(write, version)?;
    }
    Ok(())
}

fn write_dynamic_crafting_entry(
    write: &mut impl Write,
    display_id: i32,
    version: EncodingKey,
    group_id: Option<i32>,
    flags: u8,
    crafting_table: &Item,
    recipe: &crate::codec::recipe::OwnedCraftingRecipe,
) -> Result<(), WritingError> {
    match recipe {
        crate::codec::recipe::OwnedCraftingRecipe::Shaped {
            category,
            pattern,
            key,
            result,
            ..
        } => {
            let height = pattern.len() as i32;
            let width = pattern.first().map_or(0, String::len) as i32;

            write.write_var_int(&VarInt(display_id))?;
            write.write_var_int(&VarInt(RECIPE_DISPLAY_SHAPED))?;
            write.write_var_int(&VarInt(width))?;
            write.write_var_int(&VarInt(height))?;
            write.write_var_int(&VarInt(width * height))?;
            for row in pattern {
                for ch in row.chars() {
                    if ch == ' ' {
                        write_empty_slot_display(write, version)?;
                    } else if let Some((_, ingredient)) = key.iter().find(|(k, _)| *k == ch) {
                        write_dynamic_ingredient_slot_display(write, ingredient, version)?;
                    } else {
                        write_empty_slot_display(write, version)?;
                    }
                }
            }
            write_dynamic_result_slot_display(write, result, version)?;
            write_item_slot_display(write, crafting_table, version)?;
            write_optional_var_int(write, group_id)?;
            write.write_var_int(&VarInt(crafting_category(category)))?;

            let mut slots: Vec<&crate::codec::recipe::OwnedRecipeIngredient> = Vec::new();
            for row in pattern {
                for ch in row.chars() {
                    if ch != ' '
                        && let Some((_, ing)) = key.iter().find(|(k, _)| *k == ch)
                    {
                        slots.push(ing);
                    }
                }
            }
            write.write_bool(true)?; // present
            write.write_var_int(&VarInt(slots.len() as i32))?;
            for ing in slots {
                write_dynamic_ingredient_holderset(write, ing, version)?;
            }
            write.write_u8(flags)?;
        }
        crate::codec::recipe::OwnedCraftingRecipe::Shapeless {
            category,
            ingredients,
            result,
            ..
        } => {
            write.write_var_int(&VarInt(display_id))?;
            write.write_var_int(&VarInt(RECIPE_DISPLAY_SHAPELESS))?;
            write.write_var_int(&VarInt(ingredients.len() as i32))?;
            for ing in ingredients {
                write_dynamic_ingredient_slot_display(write, ing, version)?;
            }
            write_dynamic_result_slot_display(write, result, version)?;
            write_item_slot_display(write, crafting_table, version)?;
            write_optional_var_int(write, group_id)?;
            write.write_var_int(&VarInt(crafting_category(category)))?;

            write.write_bool(true)?;
            write.write_var_int(&VarInt(ingredients.len() as i32))?;
            for ing in ingredients {
                write_dynamic_ingredient_holderset(write, ing, version)?;
            }
            write.write_u8(flags)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_dynamic_cooking_entry(
    write: &mut impl Write,
    display_id: i32,
    version: EncodingKey,
    group_id: Option<i32>,
    flags: u8,
    station: &Item,
    cooking: &crate::codec::recipe::OwnedCookingRecipe,
    book_category: i32,
) -> Result<(), WritingError> {
    write.write_var_int(&VarInt(display_id))?;
    write.write_var_int(&VarInt(RECIPE_DISPLAY_FURNACE))?;
    write_dynamic_ingredient_slot_display(write, &cooking.ingredient, version)?;
    write_any_fuel_slot_display(write, version)?;
    write_dynamic_result_slot_display(write, &cooking.result, version)?;
    write_item_slot_display(write, station, version)?;
    write.write_var_int(&VarInt(cooking.cooking_time))?;
    write.write_f32_be(cooking.experience)?;
    write_optional_var_int(write, group_id)?;
    write.write_var_int(&VarInt(book_category))?;
    write.write_bool(true)?;
    write.write_var_int(&VarInt(1))?;
    write_dynamic_ingredient_holderset(write, &cooking.ingredient, version)?;
    write.write_u8(flags)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::recipe::OwnedCookingRecipe;
    use crate::codec::recipe_book::{CRecipeBookEntries, generated_cooking};
    use pumpkin_data::recipes::{RECIPES_CRAFTING, RECIPES_CRAFTING_IDS};
    use pumpkin_util::version::JavaMinecraftVersion;

    #[test]
    fn air_result_writes_the_empty_slot_display() {
        let result = RecipeResultStruct {
            id: "minecraft:air",
            count: 1,
            components: &[],
        };
        for version in [JavaMinecraftVersion::V_1_21_2, JavaMinecraftVersion::V_26_3] {
            let mut bytes = Vec::new();
            write_result_slot_display(&mut bytes, &result, version.into()).unwrap();
            assert_eq!(bytes, [SLOT_DISPLAY_EMPTY as u8]);
        }
    }

    use crate::codec::recipe::{OwnedRecipeIngredient, OwnedRecipeResult};

    fn diamond() -> OwnedRecipeResult {
        OwnedRecipeResult {
            item_id: "minecraft:diamond".to_string(),
            count: 1,
            components: None,
        }
    }

    fn shapeless(id: &str) -> DynamicRecipe {
        DynamicRecipe::Crafting(OwnedCraftingRecipe::Shapeless {
            recipe_id: Some(id.to_string()),
            category: RecipeCategoryTypes::Misc,
            group: None,
            ingredients: vec![OwnedRecipeIngredient::Simple("minecraft:dirt".to_string())],
            result: diamond(),
        })
    }

    /// The display id of the generated crafting recipe `id` in a book without dynamic recipes.
    fn generated_crafting_display_id(id: &str) -> i32 {
        RECIPES_CRAFTING
            .iter()
            .zip(RECIPES_CRAFTING_IDS)
            .filter(|(recipe, _)| {
                !matches!(
                    recipe,
                    CraftingRecipeTypes::CraftingSpecial(_)
                        | CraftingRecipeTypes::CraftingDecoratedPot { .. }
                )
            })
            .position(|(_, recipe_id)| *recipe_id == id)
            .unwrap() as i32
    }

    fn body(packet: &impl ClientPacket) -> Vec<u8> {
        let mut body = Vec::new();
        packet
            .write_packet_data(&mut body, &JavaMinecraftVersion::V_26_3.into())
            .unwrap();
        body
    }

    fn var_ints(values: &[i32]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for value in values {
            bytes.write_var_int(&VarInt(*value)).unwrap();
        }
        bytes
    }

    #[test]
    fn vanilla_recipes_serialize_for_every_recipe_book_version() {
        let book = RecipeBook::new(&[]);
        let packet = CRecipeBookEntries::all(true, &book, &[]);
        for version in [JavaMinecraftVersion::V_1_21_2, JavaMinecraftVersion::V_26_3] {
            packet
                .write_packet_data(Vec::new(), &version.into())
                .unwrap();
        }
    }

    #[test]
    fn a_dynamic_recipe_takes_the_display_id_of_the_generated_recipe_it_replaces() {
        use crate::codec::recipe::OwnedStonecutterRecipe;
        let id = "minecraft:iron_ingot_from_smelting_raw_iron";
        let dynamic = [
            DynamicRecipe::Stonecutting(OwnedStonecutterRecipe {
                recipe_id: "test:cut".to_string(),
                group: None,
                ingredient: OwnedRecipeIngredient::Simple("minecraft:stone".to_string()),
                result: diamond(),
            }),
            DynamicRecipe::Cooking(OwnedCookingRecipeType::Smelting(OwnedCookingRecipe {
                recipe_id: id.to_string(),
                category: RecipeCategoryTypes::Misc,
                group: None,
                ingredient: OwnedRecipeIngredient::Simple("minecraft:raw_iron".to_string()),
                cooking_time: 10,
                experience: 1.0,
                result: diamond(),
            })),
            shapeless("minecraft:stick"),
        ];
        let empty = RecipeBook::new(&[]);
        let book = RecipeBook::new(&dynamic);
        // One entry per id, and no other entry moves.
        assert_eq!(book.display_ids().count(), empty.display_ids().count());
        assert_eq!(book.dynamic_display_id(0), None);
        let stick = generated_crafting_display_id("minecraft:stick");
        assert_eq!(book.dynamic_display_id(2), Some(stick));
        let iron = book.dynamic_display_id(1).unwrap();
        assert!(matches!(
            empty.recipe(iron, &[]),
            Some(BookRecipe::Cooking(recipe)) if generated_cooking(recipe).recipe_id == id
        ));
        let body = body(&CRecipeBookEntries::all(true, &book, &dynamic));
        let expected = var_ints(&[empty.display_ids().count() as i32]);
        assert_eq!(body[..expected.len()], expected[..]);
    }

    /// A plugin recipe with a new id takes the next display id, so the client keeps every id
    /// it knows.
    #[test]
    fn a_recipe_with_a_new_id_appends_its_entry() {
        let empty = RecipeBook::new(&[]);
        let book = RecipeBook::new(&[shapeless("test:first"), shapeless("test:second")]);
        let next = empty.display_ids().count() as i32;
        assert_eq!(book.dynamic_display_id(0), Some(next));
        assert_eq!(book.dynamic_display_id(1), Some(next + 1));
    }

    /// Vanilla keeps an ignored recipe in the recipe map: it still replaces the generated recipe
    /// with its id, and the book has no entry for that id.
    #[test]
    fn an_ignored_recipe_empties_the_entry_of_the_recipe_it_replaces() {
        let dynamic = [DynamicRecipe::Ignored {
            recipe_id: "minecraft:stick".to_string(),
        }];
        let book = RecipeBook::new(&dynamic);
        let stick = generated_crafting_display_id("minecraft:stick");
        assert!(book.recipe(stick, &dynamic).is_none());
        assert_eq!(
            book.display_ids().count(),
            RecipeBook::new(&[]).display_ids().count() - 1
        );
    }

    /// `/recipe give` sends a subset: each entry keeps its display id of the whole book.
    #[test]
    fn a_subset_keeps_the_display_ids_of_the_whole_book() {
        let dynamic = [shapeless("test:first"), shapeless("test:second")];
        let book = RecipeBook::new(&dynamic);
        let second = book.dynamic_display_id(1).unwrap();
        let ids = [second];
        let body = body(&CRecipeBookEntries::subset(&book, &dynamic, &ids));
        let expected = var_ints(&[1, second]);
        assert_eq!(body[..expected.len()], expected[..]);
    }

    /// The plugin packet builds the book of its recipes and writes the same bytes.
    #[test]
    fn the_plugin_packet_writes_the_book_of_its_recipes() {
        let dynamic = [shapeless("test:first"), shapeless("minecraft:stick")];
        let book = RecipeBook::new(&dynamic);
        assert_eq!(
            body(&CRecipeBookAdd::new(true, &dynamic)),
            body(&CRecipeBookEntries::all(true, &book, &dynamic))
        );
    }
}
