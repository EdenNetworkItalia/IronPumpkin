use crate::entity::player::Player;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

/// An event that occurs when an item is prepared in an anvil.
///
/// `NeoForge`: `net.neoforged.neoforge.event.AnvilUpdateEvent`, fired by
/// `CommonHooks.onAnvilUpdate` after `AnvilMenu.createResult` computed the vanilla result, when
/// the left input is not empty. The anvil applies `output`, `repair_cost` and `material_cost` as
/// the last handler left them; a cancelled event leaves the anvil with no output and no cost.
#[cancellable]
#[derive(Event, Clone)]
pub struct PrepareAnvilEvent {
    /// The player using the anvil.
    pub player: Arc<Player>,

    /// The name in the anvil text field, empty when the client sent none. The server cannot
    /// change it, so the anvil ignores a change.
    pub rename_text: String,

    /// The level cost of the operation (`AnvilUpdateEvent#getXpCost`), the vanilla cost by default.
    pub repair_cost: u32,

    /// The item in the left input slot.
    pub left: ItemStack,

    /// The item in the right input slot.
    pub right: ItemStack,

    /// The item the anvil puts in the output slot, the vanilla result by default.
    pub output: ItemStack,

    /// How many right inputs the operation consumes, the vanilla count by default. Zero consumes
    /// the whole right stack, as in vanilla.
    pub material_cost: i32,

    /// The vanilla result, cost and material cost (`AnvilUpdateEvent#getVanillaResult`).
    pub vanilla_output: ItemStack,
    pub vanilla_repair_cost: u32,
    pub vanilla_material_cost: i32,
}

impl PrepareAnvilEvent {
    /// Creates the event for the vanilla result of the anvil.
    #[must_use]
    pub fn new(
        player: Arc<Player>,
        rename_text: String,
        left: ItemStack,
        right: ItemStack,
        vanilla_output: ItemStack,
        vanilla_repair_cost: u32,
        vanilla_material_cost: i32,
    ) -> Self {
        Self {
            player,
            rename_text,
            repair_cost: vanilla_repair_cost,
            left,
            right,
            output: vanilla_output.clone(),
            material_cost: vanilla_material_cost,
            vanilla_output,
            vanilla_repair_cost,
            vanilla_material_cost,
            cancelled: false,
        }
    }
}
