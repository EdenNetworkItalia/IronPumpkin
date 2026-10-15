use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;

/// An event that occurs when a brewing stand finishes brewing potion(s).
///
/// `NeoForge`: `net.neoforged.neoforge.event.brewing.PotionBrewEvent.Pre`, fired by
/// `EventHooks.onPotionAttemptBrew` before `BrewingStandBlockEntity.doBrew` brews. A cancelled
/// event skips the vanilla brewing and its sound, and the brewing stand takes `stacks` as the
/// handlers left them; without a cancel, changes to `stacks` are ignored.
#[cancellable]
#[derive(Event, Clone)]
pub struct BrewEvent {
    /// The block position of the brewing stand.
    pub block_pos: BlockPos,

    /// The remaining fuel power.
    pub fuel_level: u8,

    /// A copy of the brewing stand inventory: the three bottles, the ingredient and the fuel
    /// (`PotionBrewEvent#getItem`, `Pre#setItem`).
    pub stacks: [ItemStack; 5],
}

impl BrewEvent {
    #[must_use]
    pub const fn new(block_pos: BlockPos, fuel_level: u8, stacks: [ItemStack; 5]) -> Self {
        Self {
            block_pos,
            fuel_level,
            stacks,
            cancelled: false,
        }
    }
}
