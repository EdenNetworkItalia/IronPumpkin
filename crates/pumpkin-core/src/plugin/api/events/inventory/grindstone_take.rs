use std::sync::Arc;

use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;

use crate::entity::player::Player;

/// Fires when a player takes the result of a grindstone, after the player released the lock of
/// its screen handler.
///
/// `NeoForge`: `net.neoforged.neoforge.event.GrindstoneEvent.OnTakeItem`, fired by
/// `CommonHooks.onGrindstoneTake` from the result slot of `GrindstoneMenu`. The player keeps the
/// result in every case. A cancelled event gives no experience and puts `top` and `bottom` back
/// into the grindstone. Otherwise the grindstone gives `xp` and its inputs become `new_top` and
/// `new_bottom`. The inputs change only when the grindstone is still open with empty inputs.
#[cancellable]
#[derive(Event, Clone)]
pub struct GrindstoneEventOnTakeItem {
    /// The player who took the result (`getPlayer`).
    pub player: Arc<Player>,

    /// The position of the grindstone, the zero position when the player has no open container
    /// (`getContainerAccess`).
    pub pos: BlockPos,

    /// The item in the top input slot when the player took the result (`getTopItem`).
    pub top: ItemStack,

    /// The item in the bottom input slot when the player took the result (`getBottomItem`).
    pub bottom: ItemStack,

    /// The experience the grindstone gives (`getXp`): the amount a handler of
    /// [`PrepareGrindstoneEvent`](super::prepare_grindstone::PrepareGrindstoneEvent) set, or
    /// the vanilla amount.
    pub xp: i32,

    /// The item of the top input slot after the take, empty by default (`getNewTopItem`).
    pub new_top: ItemStack,

    /// The item of the bottom input slot after the take, empty by default (`getNewBottomItem`).
    pub new_bottom: ItemStack,
}

impl GrindstoneEventOnTakeItem {
    #[must_use]
    pub fn new(
        player: Arc<Player>,
        pos: BlockPos,
        top: ItemStack,
        bottom: ItemStack,
        xp: i32,
    ) -> Self {
        Self {
            player,
            pos,
            top,
            bottom,
            xp,
            new_top: ItemStack::EMPTY.clone(),
            new_bottom: ItemStack::EMPTY.clone(),
            cancelled: false,
        }
    }
}
