#[allow(clippy::wildcard_imports)]
use super::*;

impl JavaClient {
    #[allow(clippy::too_many_lines)]
    pub fn handle_place_recipe(
        &self,
        server: &Arc<Server>,
        player: &Arc<Player>,
        packet: &SPlaceRecipe,
    ) {
        use crate::net::java::recipe_helper::{
            GenericIngredient, compute_biggest_craftable, take_n_ingredient,
        };
        use crate::server::recipe::DynamicRecipe;
        use pumpkin_data::recipes::{CookingRecipeType, CraftingRecipeTypes};
        use pumpkin_data::screen::WindowType;
        use pumpkin_protocol::codec::recipe::OwnedCookingRecipeType;
        use pumpkin_protocol::codec::recipe_book::BookRecipe;

        let use_max = packet.use_max_items;

        let mut click_event = crate::plugin::api::events::player::player_recipe_book_click::PlayerRecipeBookClickEvent::new(
            player.clone(),
            format!("display_{}", packet.recipe_display_id.0),
            use_max,
        );
        server
            .plugin_manager
            .fire_blocking(server, &mut click_event);
        if click_event.cancelled {
            return;
        }

        // The display ids of `CRecipeBookAdd`.
        let index = pumpkin_inventory::recipe_index::current();
        let Some(book_recipe) = index
            .book()
            .recipe(packet.recipe_display_id.0, index.dynamic_recipes())
        else {
            return;
        };

        let (window_type, grid_width, crafting_inv) = {
            let screen_handler_arc = player
                .current_screen_handler
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            let handler = screen_handler_arc
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let win_type = handler.window_type();
            match win_type {
                Some(WindowType::Crafting) => (
                    win_type,
                    3usize,
                    handler.get_behaviour().slots[1].get_inventory(),
                ),
                None => (
                    win_type,
                    2usize,
                    handler.get_behaviour().slots[1].get_inventory(),
                ),
                Some(WindowType::Furnace | WindowType::BlastFurnace | WindowType::Smoker) => (
                    win_type,
                    1usize,
                    handler.get_behaviour().slots[0].get_inventory(),
                ),
                _ => return,
            }
        };

        let is_cooking_window = matches!(
            window_type,
            Some(WindowType::Furnace | WindowType::BlastFurnace | WindowType::Smoker)
        );
        let grid_size = if is_cooking_window {
            1
        } else {
            grid_width * grid_width
        };
        let mut ingredient_slots: Vec<Option<GenericIngredient<'_>>> = vec![None; grid_size];

        if is_cooking_window {
            if let BookRecipe::Cooking(cooking_recipe) = book_recipe {
                let ingredient = match (window_type, cooking_recipe) {
                    (Some(WindowType::Furnace), CookingRecipeType::Smelting(r))
                    | (Some(WindowType::BlastFurnace), CookingRecipeType::Blasting(r))
                    | (Some(WindowType::Smoker), CookingRecipeType::Smoking(r)) => &r.ingredient,
                    _ => return,
                };
                ingredient_slots[0] = Some(GenericIngredient::Vanilla(ingredient));
            } else {
                let BookRecipe::Dynamic(DynamicRecipe::Cooking(cooking)) = book_recipe else {
                    return;
                };
                let ingredient = match (window_type, cooking) {
                    (Some(WindowType::Furnace), OwnedCookingRecipeType::Smelting(r))
                    | (Some(WindowType::BlastFurnace), OwnedCookingRecipeType::Blasting(r))
                    | (Some(WindowType::Smoker), OwnedCookingRecipeType::Smoking(r)) => {
                        &r.ingredient
                    }
                    _ => return,
                };
                ingredient_slots[0] = Some(GenericIngredient::Dynamic(ingredient));
            }
        } else if let BookRecipe::Crafting(recipe) = book_recipe {
            match recipe {
                CraftingRecipeTypes::CraftingShaped { pattern, key, .. } => {
                    for (row, row_str) in pattern.iter().enumerate() {
                        for (col, ch) in row_str.chars().enumerate() {
                            if ch != ' '
                                && let Some(ing) =
                                    key.iter().find_map(|(k, v)| (*k == ch).then_some(v))
                                && row * grid_width + col < grid_size
                            {
                                ingredient_slots[row * grid_width + col] =
                                    Some(GenericIngredient::Vanilla(ing));
                            }
                        }
                    }
                }
                CraftingRecipeTypes::CraftingShapeless { ingredients, .. } => {
                    for (i, ing) in ingredients.iter().enumerate().take(grid_size) {
                        ingredient_slots[i] = Some(GenericIngredient::Vanilla(ing));
                    }
                }
                CraftingRecipeTypes::CraftingTransmute {
                    input,
                    material,
                    material_count,
                    ..
                } => {
                    let needed_materials = usize::from(material_count.0.max(1));
                    if grid_size > needed_materials {
                        ingredient_slots[0] = Some(GenericIngredient::Vanilla(input));
                        for slot in ingredient_slots.iter_mut().skip(1).take(needed_materials) {
                            *slot = Some(GenericIngredient::Vanilla(material));
                        }
                    }
                }
                _ => return,
            }
        } else {
            let BookRecipe::Dynamic(DynamicRecipe::Crafting(crafting)) = book_recipe else {
                return;
            };

            match crafting {
                pumpkin_protocol::codec::recipe::OwnedCraftingRecipe::Shaped {
                    pattern,
                    key,
                    ..
                } => {
                    for (row, row_str) in pattern.iter().enumerate() {
                        for (col, ch) in row_str.chars().enumerate() {
                            if ch != ' '
                                && let Some((_, ing)) = key.iter().find(|(k, _)| *k == ch)
                                && row * grid_width + col < grid_size
                            {
                                ingredient_slots[row * grid_width + col] =
                                    Some(GenericIngredient::Dynamic(ing));
                            }
                        }
                    }
                }

                pumpkin_protocol::codec::recipe::OwnedCraftingRecipe::Shapeless {
                    ingredients,
                    ..
                } => {
                    for (i, ing) in ingredients.iter().enumerate().take(grid_size) {
                        ingredient_slots[i] = Some(GenericIngredient::Dynamic(ing));
                    }
                }
            }
        }

        // Check if this exact recipe is already placed (determines stacking vs fresh fill).
        let recipe_matches = {
            let mut ok = true;
            for (idx, ing) in ingredient_slots.iter().enumerate() {
                let stack = crafting_inv.get_stack(idx);
                match ing {
                    None => {
                        if !stack.is_empty() {
                            ok = false;
                            break;
                        }
                    }
                    Some(ingredient) => {
                        if stack.is_empty() || !ingredient.match_item(stack.item) {
                            ok = false;
                            break;
                        }
                    }
                }
            }
            ok
        };

        // Read minimum count from occupied slots before clearing (needed for stacking).
        let current_min = if recipe_matches && !use_max {
            let mut min = u8::MAX;
            for (idx, ing) in ingredient_slots.iter().enumerate() {
                if ing.is_some() {
                    let stack = crafting_inv.get_stack(idx);
                    if !stack.is_empty() {
                        min = min.min(stack.item_count);
                    }
                }
            }
            if min == u8::MAX { 0 } else { min }
        } else {
            0
        };

        // Always clear the grid first, returning items to inventory.
        for i in 0..grid_size {
            let stack = crafting_inv.remove_stack(i);
            if !stack.is_empty() {
                player.inventory.offer(stack, false, player.as_ref());
            }
        }

        // Determine how many of each ingredient to place per slot.
        let active_ingredients: Vec<GenericIngredient<'_>> =
            ingredient_slots.iter().flatten().copied().collect();
        let amount_to_craft = if use_max {
            compute_biggest_craftable(&active_ingredients, &player.inventory)
        } else if recipe_matches {
            current_min.saturating_add(1)
        } else {
            1
        };

        if amount_to_craft == 0 {
            let screen_handler_arc = player
                .current_screen_handler
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            screen_handler_arc
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .send_content_updates();
            return;
        }

        // Fill each grid slot with exactly `amount_to_craft` matching items.
        for (idx, ing) in ingredient_slots.iter().enumerate() {
            let Some(ingredient) = ing else { continue };
            let taken = take_n_ingredient(&player.inventory, ingredient, amount_to_craft);
            if !taken.is_empty() {
                crafting_inv.set_stack(idx, taken);
            }
        }

        let screen_handler_arc = player
            .current_screen_handler
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        screen_handler_arc
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .send_content_updates();
    }
}
