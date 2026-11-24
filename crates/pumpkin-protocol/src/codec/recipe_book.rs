//! The recipe book layout of the server and the packet that writes a part of it.
//!
//! These types live outside `java/client/play`, so the plugin WIT generator, which turns each
//! public type of a packet file into a WIT type, does not see them.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Write;

use pumpkin_data::packet::clientbound::play::RECIPE_BOOK_ADD;
use pumpkin_data::recipes::{
    CookingRecipeType, CraftingRecipeTypes, RECIPES_COOKING, RECIPES_CRAFTING, RECIPES_CRAFTING_IDS,
};
use pumpkin_util::version::JavaMinecraftVersion;

use crate::codec::recipe::{
    DynamicRecipe, OwnedCookingRecipe, OwnedCookingRecipeType, OwnedCraftingRecipe,
};
use crate::java::client::play::write_recipe_book;
use crate::packet::MultiVersionJavaPacket;
use crate::{ClientPacket, EncodingKey, WritingError};

/// Vanilla `ClientboundRecipeBookAddPacket` with the entries of a cached [`RecipeBook`].
///
/// It holds every entry, or the entries with the display ids `only`. A partial packet keeps the
/// display ids of the whole book, as the client expects.
pub struct CRecipeBookEntries<'a> {
    pub replace: bool,
    pub book: &'a RecipeBook,
    /// The list that `book` was built from.
    pub dynamic_recipes: &'a [DynamicRecipe],
    pub only: Option<&'a [i32]>,
}

impl<'a> CRecipeBookEntries<'a> {
    /// Every entry of `book`.
    #[must_use]
    pub const fn all(
        replace: bool,
        book: &'a RecipeBook,
        dynamic_recipes: &'a [DynamicRecipe],
    ) -> Self {
        Self {
            replace,
            book,
            dynamic_recipes,
            only: None,
        }
    }

    /// The entries with the display ids `ids`, added to the book of the client.
    #[must_use]
    pub const fn subset(
        book: &'a RecipeBook,
        dynamic_recipes: &'a [DynamicRecipe],
        ids: &'a [i32],
    ) -> Self {
        Self {
            replace: false,
            book,
            dynamic_recipes,
            only: Some(ids),
        }
    }
}

impl MultiVersionJavaPacket for CRecipeBookEntries<'_> {
    fn to_id(version: JavaMinecraftVersion) -> i32 {
        RECIPE_BOOK_ADD.to_id(version)
    }
}

impl ClientPacket for CRecipeBookEntries<'_> {
    fn write_packet_data(
        &self,
        write: impl Write,
        version: &EncodingKey,
    ) -> Result<(), WritingError> {
        write_recipe_book(
            write,
            *version,
            self.replace,
            self.book,
            self.dynamic_recipes,
            self.only,
        )
    }
}

/// A recipe of the recipe book.
#[derive(Clone, Copy)]
pub enum BookRecipe<'a> {
    Crafting(&'static CraftingRecipeTypes),
    Cooking(&'static CookingRecipeType),
    Dynamic(&'a DynamicRecipe),
}

impl BookRecipe<'_> {
    fn group(&self) -> Option<&str> {
        match self {
            Self::Crafting(
                CraftingRecipeTypes::CraftingShaped { group, .. }
                | CraftingRecipeTypes::CraftingShapeless { group, .. }
                | CraftingRecipeTypes::CraftingTransmute { group, .. },
            ) => *group,
            Self::Cooking(recipe) => generated_cooking(recipe).group,
            Self::Dynamic(DynamicRecipe::Crafting(
                OwnedCraftingRecipe::Shaped { group, .. }
                | OwnedCraftingRecipe::Shapeless { group, .. },
            )) => group.as_deref(),
            Self::Dynamic(DynamicRecipe::Cooking(cooking)) => {
                owned_cooking(cooking).group.as_deref()
            }
            Self::Crafting(_) | Self::Dynamic(_) => None,
        }
    }
}

pub(crate) const fn generated_cooking(
    recipe: &CookingRecipeType,
) -> &pumpkin_data::recipes::CookingRecipe {
    match recipe {
        CookingRecipeType::Smelting(r)
        | CookingRecipeType::Blasting(r)
        | CookingRecipeType::Smoking(r)
        | CookingRecipeType::CampfireCooking(r) => r,
    }
}

pub(crate) const fn owned_cooking(recipe: &OwnedCookingRecipeType) -> &OwnedCookingRecipe {
    match recipe {
        OwnedCookingRecipeType::Smelting(r)
        | OwnedCookingRecipeType::Blasting(r)
        | OwnedCookingRecipeType::Smoking(r)
        | OwnedCookingRecipeType::CampfireCooking(r) => r,
    }
}

#[derive(Clone, Copy)]
enum BookSlot {
    Crafting(usize),
    Cooking(usize),
    Dynamic(usize),
}

/// The display ids of the recipe book, as vanilla `RecipeManager.allDisplays`.
///
/// The index of an entry is its display id. The generated crafting recipes come first, then the
/// generated cooking recipes, then the dynamic recipes with a new id.
///
/// A dynamic recipe with the id of a generated recipe takes the display id of that recipe, so
/// one id has one entry and a recipe that a plugin adds later moves no other entry. A generated
/// recipe that a recipe without a book entry replaces leaves its display id empty.
pub struct RecipeBook {
    slots: Vec<Option<BookSlot>>,
    groups: Vec<Option<i32>>,
    /// The display id of each dynamic recipe, by its index in the dynamic recipe list.
    dynamic_ids: Vec<Option<i32>>,
}

impl RecipeBook {
    /// The layout for the dynamic recipes `dynamic`. Each lookup takes the same list.
    #[must_use]
    pub fn new(dynamic: &[DynamicRecipe]) -> Self {
        let by_id: HashMap<&str, usize> = dynamic
            .iter()
            .enumerate()
            .filter_map(|(index, recipe)| Some((recipe.recipe_id()?, index)))
            .collect();
        let shown = |index: usize| {
            matches!(
                dynamic[index],
                DynamicRecipe::Crafting(_) | DynamicRecipe::Cooking(_)
            )
        };
        let mut replaced = vec![false; dynamic.len()];
        let mut slot_for = |id: &str, generated: BookSlot| match by_id.get(id) {
            Some(&index) => {
                replaced[index] = true;
                shown(index).then_some(BookSlot::Dynamic(index))
            }
            None => Some(generated),
        };
        let mut slots = Vec::new();
        for (index, (recipe, id)) in RECIPES_CRAFTING
            .iter()
            .zip(RECIPES_CRAFTING_IDS)
            .enumerate()
        {
            if !matches!(
                recipe,
                CraftingRecipeTypes::CraftingSpecial(_)
                    | CraftingRecipeTypes::CraftingDecoratedPot { .. }
            ) {
                slots.push(slot_for(id, BookSlot::Crafting(index)));
            }
        }
        for (index, recipe) in RECIPES_COOKING.iter().enumerate() {
            slots.push(slot_for(
                generated_cooking(recipe).recipe_id,
                BookSlot::Cooking(index),
            ));
        }
        for (index, replaced) in replaced.iter().enumerate() {
            if !replaced && shown(index) {
                slots.push(Some(BookSlot::Dynamic(index)));
            }
        }

        let mut book = Self {
            groups: Vec::with_capacity(slots.len()),
            dynamic_ids: vec![None; dynamic.len()],
            slots,
        };
        let mut group_ids = HashMap::new();
        let mut next_group_id = 0;
        for display_id in 0..book.slots.len() {
            let recipe = book.recipe(display_id as i32, dynamic);
            if let Some(BookSlot::Dynamic(index)) = book.slots[display_id] {
                book.dynamic_ids[index] = Some(display_id as i32);
            }
            let group = recipe
                .as_ref()
                .and_then(BookRecipe::group)
                .map(|group| Cow::Owned(group.to_string()));
            book.groups.push(resolve_group_id_owned(
                &mut group_ids,
                &mut next_group_id,
                group,
            ));
        }
        book
    }

    /// The recipe with `display_id`. `dynamic` is the list the book was built from.
    #[must_use]
    pub fn recipe<'a>(
        &self,
        display_id: i32,
        dynamic: &'a [DynamicRecipe],
    ) -> Option<BookRecipe<'a>> {
        let slot = (*self.slots.get(usize::try_from(display_id).ok()?)?)?;
        Some(match slot {
            BookSlot::Crafting(index) => BookRecipe::Crafting(&RECIPES_CRAFTING[index]),
            BookSlot::Cooking(index) => BookRecipe::Cooking(&RECIPES_COOKING[index]),
            BookSlot::Dynamic(index) => BookRecipe::Dynamic(dynamic.get(index)?),
        })
    }

    /// The display id of the dynamic recipe at `index` of the dynamic recipe list.
    #[must_use]
    pub fn dynamic_display_id(&self, index: usize) -> Option<i32> {
        self.dynamic_ids.get(index).copied().flatten()
    }

    /// The recipe book group of the entry with `display_id`.
    #[must_use]
    pub fn group(&self, display_id: i32) -> Option<i32> {
        usize::try_from(display_id)
            .ok()
            .and_then(|index| self.groups.get(index).copied())
            .flatten()
    }

    /// The display id of the generated recipe `recipe_id`. A dynamic recipe with that id has
    /// replaced it and holds the display id, so it is not found here.
    #[must_use]
    pub fn generated_display_id(&self, recipe_id: &str) -> Option<i32> {
        let position = self.slots.iter().position(|slot| {
            let id = match slot {
                Some(BookSlot::Crafting(index)) => RECIPES_CRAFTING_IDS[*index],
                Some(BookSlot::Cooking(index)) => {
                    generated_cooking(&RECIPES_COOKING[*index]).recipe_id
                }
                Some(BookSlot::Dynamic(_)) | None => return false,
            };
            id == recipe_id
        })?;
        i32::try_from(position).ok()
    }

    /// The display ids that hold a recipe.
    pub fn display_ids(&self) -> impl Iterator<Item = i32> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.is_some())
            .map(|(display_id, _)| display_id as i32)
    }
}

fn resolve_group_id_owned<'a>(
    group_ids: &mut HashMap<Cow<'a, str>, i32>,
    next_group_id: &mut i32,
    group: Option<Cow<'a, str>>,
) -> Option<i32> {
    let key = group?;
    Some(*group_ids.entry(key).or_insert_with(|| {
        let id = *next_group_id;
        *next_group_id += 1;
        id
    }))
}
