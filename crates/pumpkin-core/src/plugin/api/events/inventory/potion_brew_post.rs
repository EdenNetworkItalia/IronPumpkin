use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;
use pumpkin_util::math::position::BlockPos;

/// Fires after a brewing stand brewed, and after a handler of
/// [`BrewEvent`](super::brew::BrewEvent) cancelled the vanilla brew.
///
/// `NeoForge`: `net.neoforged.neoforge.event.brewing.PotionBrewEvent.Post`, fired by
/// `EventHooks.onPotionBrewed` in `BrewingStandBlockEntity.doBrew` and by
/// `EventHooks.onPotionAttemptBrew` after a cancelled `PotionBrewEvent.Pre`.
#[derive(Event, Clone)]
pub struct PotionBrewEventPost {
    /// The block position of the brewing stand.
    pub block_pos: BlockPos,

    /// A copy of the brewing stand inventory after the brew: the three bottles, the ingredient
    /// and the fuel. The vanilla brew already consumed the ingredient. `NeoForge` hands out the
    /// live stacks, so the stand takes changed stacks when its inventory did not change while the
    /// event fired.
    pub stacks: [ItemStack; 5],
}

impl PotionBrewEventPost {
    #[must_use]
    pub const fn new(block_pos: BlockPos, stacks: [ItemStack; 5]) -> Self {
        Self { block_pos, stacks }
    }
}
