use pumpkin_data::item_stack::ItemStack;
use pumpkin_inventory::recipe_index::CookingEntry;

use crate::block::entities::{BlockEntity, PropertyDelegate};
pub use pumpkin_inventory::ExperienceContainer;
use pumpkin_inventory::{Clearable, Inventory};

/// Trait for extracting smelting experience from cooking block entities.
pub trait CookingBlockEntityBase:
    Sync + Send + Inventory + PropertyDelegate + BlockEntity + Clearable
{
    fn get_cooking_time_spent(&self) -> u16;
    fn get_cooking_total_time(&self) -> u16;
    fn get_lit_time_remaining(&self) -> u16;
    fn get_lit_total_time(&self) -> u16;

    /// Track that a recipe was used (for XP calculation on extraction)
    fn add_recipe_used(&self, recipe: &CookingEntry);
    /// Extract and reset accumulated experience, returning the total as an integer
    /// Calculates XP from tracked recipes and clears the `recipes_used` map
    fn extract_experience_from_recipes(&self) -> i32;

    fn get_input_item(&self) -> ItemStack;
    fn get_fuel_item(&self) -> ItemStack;
    fn get_output_item(&self) -> ItemStack;

    fn set_cooking_time_spent(&self, spent_time: u16);
    fn set_cooking_total_time(&self, total_time: u16);
    fn set_lit_time_remaining(&self, remaining_time: u16);
    fn set_lit_total_time(&self, total_time: u16);

    fn is_burning(&self) -> bool;
    fn can_accept_recipe_output(&self, recipe: Option<&CookingEntry>, max_count: u8) -> bool;
    fn craft_recipe(&self, recipe: Option<&CookingEntry>) -> bool;
}

#[macro_export]
macro_rules! impl_cooking_block_entity_base {
    ($struct_name:ty) => {
        impl CookingBlockEntityBase for $struct_name {
            fn get_cooking_time_spent(&self) -> u16 {
                self.cooking_time_spent.load(Ordering::Relaxed)
            }

            fn get_cooking_total_time(&self) -> u16 {
                self.cooking_total_time.load(Ordering::Relaxed)
            }

            fn get_lit_time_remaining(&self) -> u16 {
                self.lit_time_remaining.load(Ordering::Relaxed)
            }

            fn get_lit_total_time(&self) -> u16 {
                self.lit_total_time.load(Ordering::Relaxed)
            }

            fn get_input_item(&self) -> ItemStack {
                let items = self
                    .items
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items[0].clone()
            }

            fn get_fuel_item(&self) -> ItemStack {
                let items = self
                    .items
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items[1].clone()
            }

            fn get_output_item(&self) -> ItemStack {
                let items = self
                    .items
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items[2].clone()
            }

            fn set_cooking_time_spent(&self, spent_time: u16) {
                self.cooking_time_spent.store(spent_time, Ordering::Relaxed);
            }

            fn set_cooking_total_time(&self, total_time: u16) {
                self.cooking_total_time.store(total_time, Ordering::Relaxed);
            }

            fn set_lit_time_remaining(&self, remaining_time: u16) {
                self.lit_time_remaining
                    .store(remaining_time, Ordering::Relaxed);
            }

            fn set_lit_total_time(&self, total_time: u16) {
                self.lit_total_time.store(total_time, Ordering::Relaxed);
            }

            fn is_burning(&self) -> bool {
                self.get_lit_time_remaining() > 0
            }

            fn add_recipe_used(&self, recipe: &pumpkin_inventory::recipe_index::CookingEntry) {
                // Track recipe usage by recipe ID for XP calculation
                let recipe_id = recipe.recipe_id.clone();
                let mut recipes = self
                    .recipes_used
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                *recipes.entry(recipe_id).or_insert(0) += 1;
            }

            fn extract_experience_from_recipes(&self) -> i32 {
                // Calculate total XP from tracked recipes and clear the map (vanilla behavior)
                let mut recipes = self
                    .recipes_used
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let index = pumpkin_inventory::recipe_index::current();
                let mut total_xp: f32 = 0.0;
                for (recipe_id, count) in recipes.iter() {
                    if let Some(xp) = index.cooking_experience(recipe_id) {
                        total_xp += xp * (*count as f32);
                    }
                }
                recipes.clear();
                total_xp.floor() as i32
            }

            fn can_accept_recipe_output(
                &self,
                recipe: Option<&pumpkin_inventory::recipe_index::CookingEntry>,
                max_count: u8,
            ) -> bool {
                let Some(recipe) = recipe else { return false };
                let Ok(items) = self.items.try_read() else {
                    return false;
                };
                if items[0].is_empty() {
                    return false;
                }
                // Vanilla `AbstractFurnaceBlockEntity.canBurn`.
                let result = recipe.result.assemble();
                if result.is_empty() {
                    return false;
                }
                let output = &items[2];
                if output.is_empty() {
                    return true;
                }
                if !output.are_items_and_components_equal(&result) {
                    return false;
                }
                let count = u32::from(output.item_count) + u32::from(result.item_count);
                count <= u32::from(max_count.min(result.get_max_stack_size()))
            }
            fn craft_recipe(
                &self,
                recipe: Option<&pumpkin_inventory::recipe_index::CookingEntry>,
            ) -> bool {
                let can_accept_output =
                    self.can_accept_recipe_output(recipe, self.get_max_count_per_stack());
                if let Some(recipe) = recipe {
                    if can_accept_output {
                        let Ok(mut items) = self.items.try_write() else {
                            return false;
                        };
                        let output_item_stack = recipe.result.assemble();
                        if output_item_stack.is_empty() {
                            return false;
                        }

                        // Vanilla `AbstractFurnaceBlockEntity.burn`.
                        if items[2].is_empty() {
                            items[2] = output_item_stack;
                        } else if items[2].are_items_and_components_equal(&output_item_stack) {
                            items[2].increment(output_item_stack.item_count);
                        }

                        // Track recipe usage for XP calculation (vanilla RecipesUsed format)
                        self.add_recipe_used(recipe);

                        if items[0].item.id == pumpkin_data::item::Item::WET_SPONGE.id
                            && !items[1].is_empty()
                            && items[1].item.id == pumpkin_data::item::Item::BUCKET.id
                        {
                            items[1] = ItemStack::new(1, &pumpkin_data::item::Item::WATER_BUCKET);
                        }

                        items[0].decrement(1);
                        return true;
                    }
                }

                false
            }
        }
    };
}

#[macro_export]
macro_rules! impl_property_delegate_for_cooking {
    ($struct_name:ty) => {
        impl $crate::block::entities::PropertyDelegate for $struct_name {
            fn get_property(&self, index: i32) -> i32 {
                match index {
                    0 => self.get_lit_time_remaining() as i32,
                    1 => self.get_lit_total_time() as i32,
                    2 => self.get_cooking_time_spent() as i32,
                    3 => self.get_cooking_total_time() as i32,
                    _ => 0,
                }
            }

            fn set_property(&self, index: i32, value: i32) {
                let value = value as u16;
                match index {
                    0 => self.set_lit_time_remaining(value),
                    1 => self.set_lit_total_time(value),
                    2 => self.set_cooking_time_spent(value),
                    3 => self.set_cooking_total_time(value),
                    _ => {}
                }
            }

            fn get_properties_size(&self) -> i32 {
                4
            }
        }
    };
}

#[macro_export]
macro_rules! impl_clearable_for_cooking {
    ($struct_name:ty) => {
        impl pumpkin_inventory::Clearable for $struct_name {
            fn clear(&self) {
                let mut items = self
                    .items
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items.fill_with(|| ItemStack::EMPTY.clone());
                self.mark_dirty();
            }
        }
    };
}

#[macro_export]
macro_rules! impl_experience_container_for_cooking {
    ($struct_name:ty) => {
        impl $crate::block::entities::furnace_like_block_entity::ExperienceContainer
            for $struct_name
        {
            fn extract_experience(&self) -> i32 {
                // Delegate to the CookingBlockEntityBase method
                self.extract_experience_from_recipes()
            }
        }
    };
}

#[macro_export]
macro_rules! impl_inventory_for_cooking {
    ($struct_name:ty, $recipe_kind:expr) => {
        impl pumpkin_inventory::Inventory for $struct_name {
            fn size(&self) -> usize {
                Self::INVENTORY_SIZE
            }

            fn is_empty(&self) -> bool {
                let items = self
                    .items
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items.iter().all(|s| s.is_empty())
            }

            fn get_stack(&self, slot: usize) -> ItemStack {
                let items = self
                    .items
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items[slot].clone()
            }

            fn remove_stack(&self, slot: usize) -> ItemStack {
                let mut items = self
                    .items
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let removed = std::mem::replace(&mut items[slot], ItemStack::EMPTY.clone());
                self.mark_dirty();
                removed
            }

            fn remove_stack_specific(&self, slot: usize, amount: u8) -> ItemStack {
                let mut items = self
                    .items
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let res = if !items[slot].is_empty() && amount > 0 {
                    items[slot].split(amount)
                } else {
                    ItemStack::EMPTY.clone()
                };
                self.mark_dirty();
                res
            }

            fn set_stack(&self, slot: usize, stack: ItemStack) {
                let mut items = self
                    .items
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let is_same_item = !stack.is_empty()
                    && ItemStack::are_items_and_components_equal(&items[slot], &stack);

                items[slot] = stack.clone();
                self.on_stack_set(slot, is_same_item, &stack);

                // Always consider the inventory changed when setting a stack
                self.mark_dirty();
            }

            fn update_stack(
                &self,
                slot: usize,
                update: &mut dyn FnMut(&mut ItemStack) -> bool,
            ) -> bool {
                let mut items = self
                    .items
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let Some(stack) = items.get_mut(slot) else {
                    return false;
                };
                let old = (slot == 0).then(|| stack.clone());
                if !update(stack) {
                    return false;
                }
                let is_same_item = old.is_some_and(|old| {
                    !stack.is_empty() && ItemStack::are_items_and_components_equal(&old, stack)
                });
                self.on_stack_set(slot, is_same_item, stack);
                true
            }

            fn mark_dirty(&self) {
                self.dirty.store(true, Ordering::Relaxed);
                self.comparator_dirty.store(true, Ordering::Relaxed);
            }

            fn is_valid_slot_for(&self, slot: usize, stack: &ItemStack) -> bool {
                match slot {
                    2 => false,
                    1 => stack
                        .get_data_component::<pumpkin_data::data_component_impl::CookingFuelImpl>()
                        .is_some()
                        || (stack.item.id == pumpkin_data::item::Item::BUCKET.id
                            && self.get_stack(1).item.id != pumpkin_data::item::Item::BUCKET.id),
                    _ => slot < Self::INVENTORY_SIZE,
                }
            }

            fn is_sided(&self) -> bool {
                true
            }

            fn get_slots_for_face(
                &self,
                direction: pumpkin_data::BlockDirection,
            ) -> &'static [usize] {
                match direction {
                    pumpkin_data::BlockDirection::Down => &[2, 1],
                    pumpkin_data::BlockDirection::Up => &[0],
                    _ => &[1],
                }
            }

            fn can_take_item_through_face(
                &self,
                slot: usize,
                stack: &ItemStack,
                direction: pumpkin_data::BlockDirection,
            ) -> bool {
                use pumpkin_data::tag::Taggable;
                if direction == pumpkin_data::BlockDirection::Down && slot == 1 {
                    stack
                        .item
                        .has_tag(&pumpkin_data::tag::Item::MINECRAFT_FURNACE_FUEL_BOTTOM_TAKEABLE)
                } else {
                    true
                }
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }

        impl $struct_name {
            /// Vanilla `quickCheck.getRecipeFor`: the last recipe if it still takes `item`, else
            /// the first recipe in vanilla order. The found recipe becomes the last recipe.
            fn find_recipe<'a>(
                &self,
                index: &'a pumpkin_inventory::recipe_index::RecipeIndex,
                item: &pumpkin_data::item::Item,
            ) -> Option<&'a pumpkin_inventory::recipe_index::CookingEntry> {
                let mut last = self
                    .last_recipe
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let recipe = index.cooking_recipe_with_hint(item, &$recipe_kind, last.as_deref());
                if let Some(recipe) = recipe
                    && last.as_deref() != Some(recipe.recipe_id.as_str())
                {
                    *last = Some(recipe.recipe_id.clone());
                }
                recipe
            }

            /// Vanilla `AbstractFurnaceBlockEntity.getTotalCookTime`: the cooking time of the
            /// recipe for `input`, or 200 ticks without a recipe.
            fn total_cook_time_for(&self, input: &ItemStack) -> u16 {
                let index = pumpkin_inventory::recipe_index::current();
                self.find_recipe(&index, input.item).map_or(200, |recipe| {
                    u16::try_from(recipe.cooking_time.max(0)).unwrap_or(u16::MAX)
                })
            }

            /// The reset of vanilla `AbstractFurnaceBlockEntity.setItem` when `input` replaces
            /// the input item: a new total cooking time and no progress.
            pub fn reset_cooking(&self, input: &ItemStack) {
                self.set_cooking_total_time(self.total_cook_time_for(input));
                self.set_cooking_time_spent(0);
            }

            /// Vanilla `AbstractFurnaceBlockEntity.setItem`: a different input item restarts cooking.
            fn on_stack_set(&self, slot: usize, is_same_item: bool, stack: &ItemStack) {
                if slot == 0 && !is_same_item {
                    self.reset_cooking(stack);
                }
            }
        }
    };
}

#[macro_export]
macro_rules! impl_block_entity_for_cooking {
    ($struct_name:ty,$recipe_kind:expr) => {
        impl $crate::block::entities::BlockEntity for $struct_name {
            #[expect(clippy::too_many_lines)]
            fn tick(
                &self,
                world: &Arc<$crate::world::World>,
            ) {
                let is_burning = self.is_burning();
                let mut is_dirty = false;
                if self.is_burning() {
                    self.lit_time_remaining.fetch_sub(1, Ordering::Relaxed);
                }

                let (top_item, bottom_item) = if let Ok(items_guard) = self.items.try_read() {
                    (items_guard[0].clone(), items_guard[1].clone())
                } else {
                    return;
                };

                let is_top_items_empty = top_item.is_empty();

                let recipe_index = pumpkin_inventory::recipe_index::current();
                let furnace_recipe = if is_top_items_empty {
                    None
                } else {
                    self.find_recipe(&recipe_index, top_item.item)
                };

                let can_accept_output = self
                    .can_accept_recipe_output(furnace_recipe, self.get_max_count_per_stack());

                let bottom_items_is_empty = bottom_item.is_empty();
                if self.is_burning() || !bottom_items_is_empty && !is_top_items_empty {
                    if !self.is_burning() && can_accept_output {
                        let is_fast = matches!(
                            $recipe_kind,
                            CookingRecipeKind::Blasting | CookingRecipeKind::Smoking
                        );
                        let fuel_component = bottom_item
                            .get_data_component::<pumpkin_data::data_component_impl::CookingFuelImpl>();
                        let adjusted_fuel_ticks = fuel_component
                            .map_or(0, |fuel| fuel.get_burn_ticks(is_fast));

                        let mut burn_ticks = adjusted_fuel_ticks;
                        let mut burn_cancelled = false;
                        if let Some(server) = world.server.upgrade() {
                            let mut burn_event = $crate::plugin::api::events::inventory::furnace_burn::FurnaceBurnEvent::new(
                                self.position,
                                bottom_item.item.registry_key.to_string(),
                                adjusted_fuel_ticks as u32,
                            );
                            server.plugin_manager.fire_blocking(&server, &mut burn_event);
                            if burn_event.cancelled {
                                burn_cancelled = true;
                            } else {
                                burn_ticks = burn_event.burn_time as u16;
                            }
                        }
                        if !burn_cancelled {
                            self.set_lit_time_remaining(burn_ticks);
                            self.set_lit_total_time(burn_ticks);

                            if self.is_burning() {
                                is_dirty = true;
                                if let Ok(mut items_guard) = self.items.try_write() {
                                    if !items_guard[1].is_empty() {
                                        let remainder = items_guard[1].item.crafting_remainder;
                                        items_guard[1].decrement(1);
                                        if let Some(remainder_item) = remainder
                                        {
                                            let remainder_stack = ItemStack::new(1, remainder_item);
                                            if items_guard[1].is_empty() {
                                                items_guard[1] = remainder_stack;
                                            } else {
                                                let pos = self.position.to_centered_f64();
                                                let entity = $crate::entity::Entity::new(
                                                    world.clone(),
                                                    pos,
                                                    &pumpkin_data::entity::EntityType::ITEM,
                                                );
                                                world.spawn_entity(std::sync::Arc::new(
                                                    $crate::entity::item::ItemEntity::new(
                                                        entity,
                                                        remainder_stack,
                                                    ),
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if self.is_burning() && can_accept_output {
                        let mut start_cancelled = false;
                        if self.get_cooking_time_spent() == 0 {
                            if let Some(server) = world.server.upgrade() {
                                let mut start_event = $crate::plugin::api::events::inventory::furnace_start_smelt::FurnaceStartSmeltEvent::new(
                                    self.position,
                                    top_item.item.registry_key.to_string(),
                                    self.get_cooking_total_time() as u32,
                                );
                                server.plugin_manager.fire_blocking(&server, &mut start_event);
                                if start_event.cancelled {
                                    start_cancelled = true;
                                } else {
                                    self.set_cooking_total_time(start_event.cooking_time as u16);
                                }
                            }
                        }
                        if !start_cancelled {
                            self.cooking_time_spent.fetch_add(1, Ordering::Relaxed);

                            if self.get_cooking_time_spent() >= self.get_cooking_total_time() {
                                self.set_cooking_time_spent(0);
                                if let Some(cooking_recipe) = furnace_recipe {
                                    let cooking_total_time = cooking_recipe.cooking_time.max(0);
                                    self.set_cooking_total_time(
                                        u16::try_from(cooking_total_time).unwrap_or(u16::MAX),
                                    );

                                    let mut smelt_cancelled = false;
                                    if let Some(server) = world.server.upgrade() {
                                        let mut smelt_event = $crate::plugin::api::events::inventory::furnace_smelt::FurnaceSmeltEvent::new(
                                            self.position,
                                            top_item.item.registry_key.to_string(),
                                            cooking_recipe.result.item_id.clone(),
                                        );
                                        server.plugin_manager.fire_blocking(&server, &mut smelt_event);
                                        if smelt_event.cancelled {
                                            smelt_cancelled = true;
                                        }
                                    }
                                    if !smelt_cancelled {
                                        self.craft_recipe(Some(cooking_recipe));
                                        is_dirty = true;
                                    }
                                }
                            }
                        }
                    } else {
                        self.set_cooking_time_spent(0);
                    }
                } else if !self.is_burning() && self.get_cooking_time_spent() > 0 {
                    let _ = self.cooking_time_spent.try_update(
                        Ordering::Acquire,
                        Ordering::Acquire,
                        |v| {
                            Some(
                                v.saturating_sub(2)
                                    .min(self.cooking_total_time.load(Ordering::Acquire)),
                            )
                        },
                    );
                }

                if is_burning != self.is_burning() {
                    is_dirty = true;
                    let (furnace_block, furnace_block_state) =
                        world.get_block_and_state(&self.position);
                    let mut props =
                        pumpkin_data::block_properties::FurnaceLikeProperties::from_state_id(furnace_block_state.id);

                    props.lit = self.is_burning();
                    world.set_block_state(
                        &self.position,
                        props.to_state_id(furnace_block),
                        $crate::world::BlockFlags::NOTIFY_ALL,
                    );
                }

                if is_dirty {
                    self.mark_dirty();
                }
            }

            fn resource_location(&self) -> &'static str {
                Self::ID
            }

            fn get_position(&self) -> BlockPos {
                self.position
            }

            fn from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound, position: BlockPos) -> Self
            where
                Self: Sized,
            {
                let cooking_total_time = AtomicU16::new(
                    nbt.get_short("cooking_total_time")
                        .map_or(0, |cooking_total_time| cooking_total_time as u16),
                );
                let cooking_time_spent = AtomicU16::new(
                    nbt.get_short("cooking_time_spent")
                        .map_or(0, |cooking_time_spent| cooking_time_spent as u16),
                );
                let lit_total_time = AtomicU16::new(
                    nbt.get_short("lit_total_time")
                        .map_or(0, |lit_total_time| lit_total_time as u16),
                );
                let lit_time_remaining = AtomicU16::new(
                    nbt.get_short("lit_time_remaining")
                        .map_or(0, |lit_time_remaining| lit_time_remaining as u16),
                );
                // Load RecipesUsed from NBT (vanilla format: map of recipe ID -> craft count)
                let mut recipes_used_map = HashMap::new();
                if let Some(recipes_compound) = nbt.get_compound("RecipesUsed") {
                    for (recipe_id, tag) in &recipes_compound.child_tags {
                        if let pumpkin_nbt::tag::NbtTag::Int(count) = tag {
                            recipes_used_map.insert(recipe_id.to_string(), *count as u32);
                        }
                    }
                }

                let mut furnace = Self {
                    position,
                    dirty: AtomicBool::new(false),
                    comparator_dirty: AtomicBool::new(false),
                    items: std::sync::RwLock::new(from_fn(|_| ItemStack::EMPTY.clone())),
                    cooking_total_time,
                    cooking_time_spent,
                    lit_total_time,
                    lit_time_remaining,
                    recipes_used: std::sync::Mutex::new(recipes_used_map),
                    last_recipe: std::sync::Mutex::new(None),
                    metadata: $crate::block::entities::ContainerMetadata::from_nbt(nbt),
                };
                pumpkin_inventory::sync_read_items_from_nbt(nbt, furnace.items.get_mut().unwrap_or_else(std::sync::PoisonError::into_inner));

                furnace
            }

            fn write_nbt(&self, nbt: &mut pumpkin_nbt::compound::NbtCompound) {
                nbt.put_short("cooking_total_time", self.get_cooking_total_time() as i16);
                nbt.put_short("cooking_time_spent", self.get_cooking_time_spent() as i16);
                nbt.put_short("lit_total_time", self.get_lit_total_time() as i16);
                nbt.put_short("lit_time_remaining", self.get_lit_time_remaining() as i16);

                // Save RecipesUsed in vanilla format (map of recipe ID -> craft count)
                {
                    let recipes = self
                        .recipes_used
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if !recipes.is_empty() {
                        let mut recipes_compound = pumpkin_nbt::compound::NbtCompound::new();
                        for (recipe_id, count) in recipes.iter() {
                            recipes_compound.put(
                                recipe_id.as_str(),
                                pumpkin_nbt::tag::NbtTag::Int(*count as i32),
                            );
                        }
                        nbt.put(
                            "RecipesUsed",
                            pumpkin_nbt::tag::NbtTag::Compound(recipes_compound),
                        );
                    }
                }

                self.write_inventory_nbt(nbt, true);
                self.metadata.write_nbt(nbt);
            }

            fn apply_item_components(&self, stack: &ItemStack) {
                self.metadata.apply_item_components(stack);
                let mut items = self
                    .items
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                $crate::block::entities::apply_container_items_to_slice(stack, &mut *items);
            }

            fn collect_item_components(&self, stack: &mut ItemStack) {
                self.metadata.collect_name_only(stack);
            }

            fn collect_pick_item_components(&self, stack: &mut ItemStack) {
                self.metadata.collect_item_components(stack);
                let items = self
                    .items
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                $crate::block::entities::collect_container_items_from_slice(&*items, stack);
            }

            fn get_lock(&self) -> Option<pumpkin_data::data_component_impl::LockImpl> {
                self.metadata.get_lock()
            }

            fn get_custom_name(&self) -> Option<pumpkin_util::text::TextComponent> {
                self.metadata.get_custom_name()
            }

            fn get_inventory(
                self: Arc<Self>,
            ) -> Option<Arc<dyn pumpkin_inventory::Inventory>> {
                Some(self)
            }

            fn is_comparator_dirty(&self) -> bool {
                self.comparator_dirty.load(Ordering::Relaxed)
            }

            fn clear_comparator_dirty(&self) {
                self.comparator_dirty.store(false, Ordering::Relaxed);
            }


            fn is_dirty(&self) -> bool {
                self.dirty.load(Ordering::Relaxed)
            }

            fn chunk_data_nbt(&self) -> Option<pumpkin_nbt::compound::NbtCompound> {
                let mut nbt = pumpkin_nbt::compound::NbtCompound::new();
                nbt.put_short("cooking_total_time", self.get_cooking_total_time() as i16);
                nbt.put_short("cooking_time_spent", self.get_cooking_time_spent() as i16);
                nbt.put_short("lit_total_time", self.get_lit_total_time() as i16);
                nbt.put_short("lit_time_remaining", self.get_lit_time_remaining() as i16);

                if let Ok(recipes) = self.recipes_used.lock() {
                    if !recipes.is_empty() {
                        let mut recipes_compound = pumpkin_nbt::compound::NbtCompound::new();
                        for (recipe_id, count) in recipes.iter() {
                            recipes_compound.put(
                                recipe_id.as_str(),
                                pumpkin_nbt::tag::NbtTag::Int(*count as i32),
                            );
                        }
                        nbt.put(
                            "RecipesUsed",
                            pumpkin_nbt::tag::NbtTag::Compound(recipes_compound),
                        );
                    }
                }

                if let Ok(guard) = self.items.try_read() {
                    pumpkin_inventory::sync_write_items_to_nbt(&*guard, &mut nbt);
                }
                Some(nbt)
            }

            fn bedrock_block_actor_data(&self, _state_id: pumpkin_data::BlockStateId) -> Option<pumpkin_nbt::compound::NbtCompound> {
                let bedrock_id = match $recipe_kind {
                    pumpkin_data::recipes::CookingRecipeKind::Smelting => "Furnace",
                    pumpkin_data::recipes::CookingRecipeKind::Blasting => "BlastFurnace",
                    pumpkin_data::recipes::CookingRecipeKind::Smoking => "Smoker",
                    _ => "Furnace",
                };
                Some($crate::block::entities::furnace_like_block_entity::build_furnace_actor(
                    bedrock_id,
                    self.position,
                    self.get_cooking_time_spent(),
                    self.get_lit_time_remaining(),
                    self.get_lit_total_time(),
                    0,
                    None,
                ))
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }

            fn to_property_delegate(
                self: Arc<Self>,
            ) -> Option<Arc<dyn $crate::block::entities::PropertyDelegate>> {
                Some(self as Arc<dyn $crate::block::entities::PropertyDelegate>)
            }

            fn to_experience_container(
                self: Arc<Self>,
            ) -> Option<
                Arc<dyn $crate::block::entities::furnace_like_block_entity::ExperienceContainer>,
            > {
                Some(
                    self as Arc<
                        dyn $crate::block::entities::furnace_like_block_entity::ExperienceContainer,
                    >,
                )
            }
        }
    };
}

/// Builds the Bedrock block-actor NBT for cooking appliances (Furnace, Smoker, `BlastFurnace`).
#[must_use]
pub fn build_furnace_actor(
    id: &'static str,
    position: pumpkin_util::math::position::BlockPos,
    cook_time: u16,
    lit_remaining: u16,
    lit_total: u16,
    stored_xp: i32,
    custom_name: Option<&str>,
) -> pumpkin_nbt::compound::NbtCompound {
    super::bedrock::BedrockBlockActorBuilder::new(id, position)
        .custom_name(custom_name)
        .put_short("BurnTime", lit_remaining as i16)
        .put_short("BurnDuration", lit_total as i16)
        .put_short("CookTime", cook_time as i16)
        .put_short("StoredXPInt", stored_xp.clamp(0, i16::MAX as i32) as i16)
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_util::math::position::BlockPos;

    #[test]
    fn furnace_actor_builder() {
        let pos = BlockPos::new(10, 60, 20);
        let nbt = build_furnace_actor("Smoker", pos, 150, 40, 200, 5, Some("Custom Smoker"));
        assert_eq!(nbt.get_string("id"), Some("Smoker"));
        assert_eq!(nbt.get_string("CustomName"), Some("Custom Smoker"));
        assert_eq!(nbt.get_short("CookTime"), Some(150));
        assert_eq!(nbt.get_short("BurnTime"), Some(40));
        assert_eq!(nbt.get_short("BurnDuration"), Some(200));
        assert_eq!(nbt.get_short("StoredXPInt"), Some(5));
        assert_eq!(nbt.get_bool("isMovable"), Some(true));
    }
}
