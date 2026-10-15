use std::sync::Arc;

use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};

use super::PlayerEvent;
use crate::entity::player::Player;

/// `NeoForge` `AnvilCraftEvent.Pre`: a player takes the result of an anvil.
///
/// Fires after a take of the anvil result, before the cost is paid and the inputs are used, after
/// the click released the screen-handler lock. The inputs are still in the anvil, so a handler
/// can reach them through `player.current_screen_handler` or close the screen.
///
/// A cancel skips the vanilla end of the take, as `NeoForge` returns from `AnvilMenu#onTake`:
/// the player keeps the output, no levels are paid, the inputs stay, the anvil takes no damage
/// and the Post does not fire. The handler that cancels is responsible for that work. A handler
/// that closes the anvil or changes an input without a cancel gets the same result.
#[cancellable]
#[derive(Event, Clone)]
pub struct AnvilCraftEventPre {
    pub player: Arc<Player>,
    pub left: ItemStack,
    pub right: ItemStack,
    pub output: ItemStack,
}

impl AnvilCraftEventPre {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        left: ItemStack,
        right: ItemStack,
        output: ItemStack,
    ) -> Self {
        Self {
            player,
            left,
            right,
            output,
            cancelled: false,
        }
    }
}

impl PlayerEvent for AnvilCraftEventPre {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}

/// `NeoForge` `AnvilCraftEvent.Post`: a player took the result of an anvil.
///
/// Fires at the end of the take, after the levels are paid, the inputs are used and the anvil
/// update event of the new inputs fired, outside the screen-handler lock. `left` and `right`
/// are the inputs before the take; the anvil has the inputs after it.
#[derive(Event, Clone)]
pub struct AnvilCraftEventPost {
    pub player: Arc<Player>,
    pub left: ItemStack,
    pub right: ItemStack,
    pub output: ItemStack,
}

impl AnvilCraftEventPost {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        left: ItemStack,
        right: ItemStack,
        output: ItemStack,
    ) -> Self {
        Self {
            player,
            left,
            right,
            output,
        }
    }
}

impl PlayerEvent for AnvilCraftEventPost {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
