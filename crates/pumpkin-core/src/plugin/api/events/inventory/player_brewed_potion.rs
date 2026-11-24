use std::sync::Arc;

use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;

use crate::entity::player::Player;

/// Fires when a player takes a potion out of a brewing stand.
///
/// `NeoForge`: `net.neoforged.neoforge.event.brewing.PlayerBrewedPotionEvent`, fired by
/// `EventHooks.onPlayerBrewedPotion` in `BrewingStandMenu.PotionSlot.onTake`, before the
/// `BREWED_POTION` advancement trigger. It fires after the player released the lock of its screen
/// handler, at the end of the click. It has no cancel and no result.
#[derive(Event, Clone)]
pub struct PlayerBrewedPotionEvent {
    pub player: Arc<Player>,
    /// A copy of the potion that the player took. `NeoForge` hands out the live stack; a change to
    /// this copy changes nothing.
    pub stack: ItemStack,
}

impl PlayerBrewedPotionEvent {
    #[must_use]
    pub const fn new(player: Arc<Player>, stack: ItemStack) -> Self {
        Self { player, stack }
    }
}
