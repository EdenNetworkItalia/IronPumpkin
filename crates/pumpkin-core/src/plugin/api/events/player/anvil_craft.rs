use std::sync::Arc;

use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};

use super::PlayerEvent;
use crate::entity::player::Player;

/// `NeoForge` `AnvilCraftEvent.Pre`: a player takes the result of an anvil.
///
/// Fires in the result slot of `AnvilScreenHandler` before the cost is paid and the inputs are
/// used, under the screen-handler lock. A cancel skips the cost and the use of the inputs, as
/// `NeoForge` returns from `AnvilMenu#onTake`: the player keeps the output.
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
/// Fires at the end of the take, under the screen-handler lock. `left` and `right` are the
/// inputs before the take.
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
