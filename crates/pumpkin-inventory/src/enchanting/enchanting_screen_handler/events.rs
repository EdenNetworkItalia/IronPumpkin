//! The `IronPumpkin` steps of the enchanting table: the player fires the enchanting events
//! between them, after it released the screen-handler lock, so a handler can use the screen
//! handler (close it, read the slots).

use pumpkin_data::Enchantment;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::Sound;
use pumpkin_data::statistic::{CustomStatistic, StatisticCategory};

use super::{EnchantingTableScreenHandler, is_lapis};
use crate::screen_handler::{InventoryPlayer, ScreenHandler};

/// The item and the rows that the enchantment level event of one row update sees. The player
/// fires it after it releases the screen-handler lock.
#[derive(Clone)]
pub struct EnchantingRows {
    pub item: ItemStack,
    pub level_requirements: [i32; 3],
    pub enchantment_id: [i32; 3],
    pub enchantment_level: [i32; 3],
}

/// A click on an enchantment row that passed the checks.
///
/// The player fires the enchant events between [`EnchantingTableScreenHandler::enchant_item`]
/// and [`EnchantingTableScreenHandler::finish_enchant`] with the screen handler unlocked.
#[derive(Clone)]
pub struct EnchantRequest {
    pub id: i32,
    pub level_requirement: i32,
    pub item: ItemStack,
    pub lapis: ItemStack,
    pub seed: i32,
    pub enchantments: Vec<(&'static Enchantment, i32)>,
}

impl EnchantingTableScreenHandler {
    /// Whether an enchantment level event is pending, and clears it.
    pub const fn take_prepare_event(&mut self) -> bool {
        std::mem::replace(&mut self.prepare_event_pending, false)
    }

    /// The item and the rows for the enchantment level event.
    #[must_use]
    pub fn prepare_rows(&self) -> EnchantingRows {
        EnchantingRows {
            item: self.inventory.get_stack(0),
            level_requirements: self.level_requirements,
            enchantment_id: self.enchantment_id,
            enchantment_level: self.enchantment_level,
        }
    }

    /// Applies the rows the handlers of the enchantment level event left, or empty rows for a
    /// cancelled event. Returns `false` and changes nothing when the item or the rows changed
    /// while the event fired.
    pub fn apply_prepare_event(
        &mut self,
        rows: &EnchantingRows,
        result: Option<EnchantingRows>,
    ) -> bool {
        if !self.inventory.get_stack(0).are_equal(&rows.item)
            || self.level_requirements != rows.level_requirements
            || self.enchantment_id != rows.enchantment_id
            || self.enchantment_level != rows.enchantment_level
        {
            return false;
        }
        let result = result.unwrap_or(EnchantingRows {
            item: ItemStack::EMPTY.clone(),
            level_requirements: [0; 3],
            enchantment_id: [-1; 3],
            enchantment_level: [-1; 3],
        });
        self.level_requirements = result.level_requirements;
        self.enchantment_id = result.enchantment_id;
        self.enchantment_level = result.enchantment_level;
        self.send_property_updates();
        true
    }

    /// Checks a click on row `id` and picks its enchantments, as `on_button_click` does first.
    #[must_use]
    pub fn enchant_request(&self, player: &dyn InventoryPlayer, id: i32) -> Option<EnchantRequest> {
        if !(0..3).contains(&id) {
            return None;
        }

        let level_req = self.level_requirements[id as usize];
        if player.experience_level() < level_req && !player.is_creative() {
            return None;
        }

        let lapis_stack = self.inventory.get_stack(1);
        let lapis_cost = (id + 1) as u8;

        if !player.is_creative()
            && (lapis_stack.is_empty()
                || !is_lapis(&lapis_stack)
                || lapis_stack.item_count < lapis_cost)
        {
            return None;
        }

        let item_stack = self.inventory.get_stack(0);

        if item_stack.is_empty() || item_stack.has_enchantments() {
            return None;
        }

        let mut random = self.create_enchantment_random(id as usize);
        let enchantments = Self::get_enchantment_list(&mut random, &item_stack, level_req);

        if enchantments.is_empty() {
            return None;
        }

        Some(EnchantRequest {
            id,
            level_requirement: level_req,
            item: item_stack,
            lapis: lapis_stack,
            seed: self.enchantment_seed,
            enchantments,
        })
    }

    /// Takes the levels and puts the item of `request` with its enchantments into slot 0.
    /// Returns the enchanted item.
    pub fn enchant_item(
        &mut self,
        player: &dyn InventoryPlayer,
        request: &EnchantRequest,
    ) -> ItemStack {
        if !player.is_creative() {
            player.add_experience_levels(-(request.id + 1));
        }

        let mut item_stack = request.item.clone();
        for &(enchant, level) in &request.enchantments {
            item_stack.add_enchantment(enchant, level as u16);
        }
        self.inventory.set_stack(0, item_stack.clone());
        item_stack
    }

    /// Consumes the lapis of `request` when slot 1 still holds it, and updates the rows, as
    /// `on_button_click` does last.
    pub fn finish_enchant(&mut self, player: &dyn InventoryPlayer, request: &EnchantRequest) {
        let mut lapis_stack = self.inventory.get_stack(1);
        if !player.is_creative() && lapis_stack.are_equal(&request.lapis) {
            lapis_stack.decrement((request.id + 1) as u8);
            self.inventory.set_stack(1, lapis_stack);
        }

        // Update seed
        player.set_enchantment_seed(rand::random());
        self.enchantment_seed = player.enchantment_seed();

        self.update_enchantments(player);
        self.send_content_updates();

        // Vanilla plays the table use sound at a random pitch in [0.9, 1.0)
        let pitch = rand::random::<f32>().mul_add(0.1, 0.9);
        player.play_block_sound(Sound::BlockEnchantmentTableUse, pitch);

        player.increment_stat(
            StatisticCategory::Custom,
            CustomStatistic::EnchantItem as i32,
            1,
        );
    }
}
