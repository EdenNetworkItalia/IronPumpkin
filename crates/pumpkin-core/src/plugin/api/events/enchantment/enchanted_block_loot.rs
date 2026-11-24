use std::sync::Arc;

use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::{BlockState, Enchantment};
use pumpkin_macros::Event;
use pumpkin_util::math::position::BlockPos;

use crate::world::World;

/// Fires when the loot of a block asks for an enchantment level of the tool.
///
/// `NeoForge`: `net.neoforged.neoforge.event.enchanting.EnchantedBlockLootEvent`, fired by
/// `EventHooks.getBlockLootEnchantmentLevel` in `ApplyBonusCount` and `BonusLevelTableCondition`
/// when the loot context has a block state and an origin. `enchantment_level` is settable: the
/// loot function or condition reads it after the event. A negative level is ignored, as
/// `setEnchantmentLevel` throws for it.
#[derive(Event, Clone)]
pub struct EnchantedBlockLootEvent {
    pub world: Arc<World>,
    /// The block position of the loot origin.
    pub pos: BlockPos,
    pub state: &'static BlockState,
    /// The tool that broke the block.
    pub tool: ItemStack,
    pub enchantment: &'static Enchantment,
    /// The level of the enchantment on the tool, with the result of `GetEnchantmentLevelEvent`.
    pub enchantment_level: i32,
}

impl EnchantedBlockLootEvent {
    #[must_use]
    pub const fn new(
        world: Arc<World>,
        pos: BlockPos,
        state: &'static BlockState,
        tool: ItemStack,
        enchantment: &'static Enchantment,
        enchantment_level: i32,
    ) -> Self {
        Self {
            world,
            pos,
            state,
            tool,
            enchantment,
            enchantment_level,
        }
    }
}
