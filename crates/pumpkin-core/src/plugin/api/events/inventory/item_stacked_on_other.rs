use std::sync::Arc;

use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};

use crate::entity::player::Player;

/// `NeoForge` `ClickAction`: the primary click is a left click and the secondary click is a right
/// click with the default key mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickAction {
    Primary,
    Secondary,
}

/// Fires when a player clicks a slot with a carried item, before the item overrides of the click
/// (the bundle logic in Pumpkin) run.
///
/// `NeoForge`: `net.neoforged.neoforge.event.ItemStackedOnOtherEvent`, fired by
/// `CommonHooks.onItemStackedOn` in `AbstractContainerMenu.tryItemClickBehaviourOverride` for a
/// pickup click (a quick move fires nothing). It fires after the player released the lock of its
/// screen handler, before the click takes it again.
///
/// A cancel skips the two item overrides. With `cancellation_result` true (the default) the click
/// ends there: the carried item and the slot do not swap. With `false` the rest of the click
/// runs as if the items had no override.
///
/// `carried_item` and `stacked_on_item` are settable: `NeoForge` hands out the live stacks and
/// the `SlotAccess` of the carried item, so a handler changes them in place. The click takes a
/// changed stack only when the carried stack and the stack in the slot are still the ones that
/// the event got. `NeoForge` also gives the `Slot`: here `slot` is its index in the screen
/// handler.
#[cancellable]
#[derive(Event, Clone)]
pub struct ItemStackedOnOtherEvent {
    pub player: Arc<Player>,
    /// The stack on the cursor, which may be empty.
    pub carried_item: ItemStack,
    /// The stack in the clicked slot, which may be empty.
    pub stacked_on_item: ItemStack,
    /// The index of the clicked slot in the screen handler of the player.
    pub slot: usize,
    pub click_action: ClickAction,
    /// What a cancelled event does to the click: `true` ends it, `false` goes on without the item
    /// overrides.
    pub cancellation_result: bool,
}

impl ItemStackedOnOtherEvent {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        carried_item: ItemStack,
        stacked_on_item: ItemStack,
        slot: usize,
        click_action: ClickAction,
    ) -> Self {
        Self {
            player,
            carried_item,
            stacked_on_item,
            slot,
            click_action,
            cancellation_result: true,
            cancelled: false,
        }
    }

    /// Cancels the event and sets the cancellation result (`cancelWithResult`).
    pub const fn cancel_with_result(&mut self, result: bool) {
        self.cancellation_result = result;
        self.cancelled = true;
    }
}
