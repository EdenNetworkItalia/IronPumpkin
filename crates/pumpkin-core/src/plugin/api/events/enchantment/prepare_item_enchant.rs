use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::player::Player;

/// An event triggered when an item is prepared for enchanting in an enchanting table.
///
/// It fires once for the three rows, after the clues are chosen, so a changed level keeps the
/// clue of the vanilla level.
#[cancellable]
#[derive(Event, Clone)]
pub struct PrepareItemEnchantEvent {
    /// The player preparing the enchantment.
    pub player: Arc<Player>,

    /// The item being enchanted.
    pub item: ItemStack,

    /// The required level costs for each of the 3 slots.
    pub level_requirements: [i32; 3],

    /// The vanilla level costs of the 3 slots. Handlers cannot change it.
    original_level_requirements: [i32; 3],

    /// The enchantment clue ID for each of the 3 slots (-1 if none).
    pub enchantment_id: [i32; 3],

    /// The enchantment clue level for each of the 3 slots (-1 if none).
    pub enchantment_level: [i32; 3],

    /// The bookshelf count surrounding the enchanting table.
    pub bookshelf_count: i32,

    /// The position of the enchanting table.
    pub pos: BlockPos,
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
        pos: BlockPos,
    ) -> Self {
        Self {
            player,
            item,
            level_requirements,
            original_level_requirements: level_requirements,
            enchantment_id,
            enchantment_level,
            bookshelf_count,
            pos,
            cancelled: false,
        }
    }

    /// The vanilla level costs of the 3 slots, before any handler.
    #[must_use]
    pub const fn original_level_requirements(&self) -> [i32; 3] {
        self.original_level_requirements
    }
}
