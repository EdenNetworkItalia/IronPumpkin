use pumpkin_data::Enchantment;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;
use std::sync::Arc;

use crate::entity::player::Player;
use crate::plugin::player::PlayerEvent;

/// Fires after an enchanting table applied enchantments to an item.
///
/// It cannot be cancelled: [`EnchantItemEvent`](super::EnchantItemEvent) fires before the enchantments apply and can
/// change or cancel them.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.player.PlayerEnchantItemEvent`, fired by
/// `CommonHooks.onPlayerEnchantItem` in `EnchantmentMenu.clickMenuButton` after the item went
/// back into the table.
#[derive(Event, Clone)]
pub struct PlayerEnchantItemEvent {
    /// The player who enchanted the item.
    pub player: Arc<Player>,

    /// The item after the enchantments applied (`getEnchantedItem`).
    pub enchanted_item: ItemStack,

    /// The enchantments and levels the item got (`getEnchantments`).
    pub enchantments: Vec<(&'static Enchantment, i32)>,
}

impl PlayerEnchantItemEvent {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        enchanted_item: ItemStack,
        enchantments: Vec<(&'static Enchantment, i32)>,
    ) -> Self {
        Self {
            player,
            enchanted_item,
            enchantments,
        }
    }
}

impl PlayerEvent for PlayerEnchantItemEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
