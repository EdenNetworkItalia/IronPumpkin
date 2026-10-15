use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::player::Player;

/// An event triggered when an item is prepared for enchanting in an enchanting table.
///
/// `NeoForge`: `net.neoforged.neoforge.event.enchanting.EnchantmentLevelSetEvent`, fired by
/// `EventHooks.onEnchantmentLevelSet` once per row in `EnchantmentMenu.slotsChanged`. Pumpkin
/// fires one event for the three rows: `level_requirements[row]` is `getEnchantLevel` of the
/// row, `original_level_requirements[row]` is `getOriginalLevel`, and `bookshelf_count` is
/// `getPower`. The world is `player.world()`. The event fires after the clues are chosen, so a
/// changed level keeps the clue of the vanilla level.
#[cancellable]
#[derive(Event, Clone)]
pub struct PrepareItemEnchantEvent {
    /// The player preparing the enchantment.
    pub player: Arc<Player>,

    /// The item being enchanted.
    pub item: ItemStack,

    /// The required level costs for each of the 3 slots.
    pub level_requirements: [i32; 3],

    /// The vanilla level costs of the 3 slots, before any handler.
    pub original_level_requirements: [i32; 3],

    /// The enchantment clue ID for each of the 3 slots (-1 if none).
    pub enchantment_id: [i32; 3],

    /// The enchantment clue level for each of the 3 slots (-1 if none).
    pub enchantment_level: [i32; 3],

    /// The bookshelf count surrounding the enchanting table.
    pub bookshelf_count: i32,
}

impl PrepareItemEnchantEvent {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        item: ItemStack,
        level_requirements: [i32; 3],
        enchantment_id: [i32; 3],
        enchantment_level: [i32; 3],
        bookshelf_count: i32,
    ) -> Self {
        Self {
            player,
            item,
            level_requirements,
            original_level_requirements: level_requirements,
            enchantment_id,
            enchantment_level,
            bookshelf_count,
            cancelled: false,
        }
    }
}
