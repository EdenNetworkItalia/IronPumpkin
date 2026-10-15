use crate::entity::player::Player;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

/// An event that occurs when items are placed into a grindstone.
///
/// `NeoForge`: `net.neoforged.neoforge.event.GrindstoneEvent.OnPlaceItem`, fired by
/// `CommonHooks.onGrindstoneChange` from `GrindstoneMenu.createResult` when an input changes,
/// after the player released the lock of its screen handler. A cancelled event leaves the
/// grindstone with no output. Otherwise an empty `output` keeps the vanilla result and
/// experience, and a set `output` replaces the vanilla result, with `xp` as its experience when
/// it is 0 or more. When the inputs or the result changed while the event fired, the grindstone
/// keeps its state.
#[cancellable]
#[derive(Event, Clone)]
pub struct PrepareGrindstoneEvent {
    /// The player using the grindstone.
    pub player: Arc<Player>,

    /// The id of the vanilla result for the inputs, `None` when vanilla gives no result.
    pub result_item: Option<String>,

    /// The item in the top input slot.
    pub top: ItemStack,

    /// The item in the bottom input slot.
    pub bottom: ItemStack,

    /// The output a handler sets, empty by default.
    pub output: ItemStack,

    /// The experience the output gives, -1 by default for the vanilla amount.
    pub xp: i32,
}

impl PrepareGrindstoneEvent {
    #[must_use]
    pub fn new(
        player: Arc<Player>,
        result_item: Option<String>,
        top: ItemStack,
        bottom: ItemStack,
    ) -> Self {
        Self {
            player,
            result_item,
            top,
            bottom,
            output: ItemStack::EMPTY.clone(),
            xp: -1,
            cancelled: false,
        }
    }
}
