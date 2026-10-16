use std::sync::Arc;

use pumpkin_data::{BlockDirection, item_stack::ItemStack};
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::Hand;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::PlayerEvent;
use crate::entity::player::Player;
use crate::plugin::api::events::results::InteractionResult;

/// `NeoForge` `UseItemOnBlockEvent`: one phase of a right click on a block.
///
/// Fires in `handle_use_item_on` before each phase runs, after `PlayerInteractEvent` with the
/// action right click block. A cancel skips the phase and gives `cancellation_result` in place
/// of its result. In the item before block phase any result other than `Pass` ends the use.
#[cancellable]
#[derive(Event, Clone)]
pub struct UseItemOnBlockEvent {
    pub player: Arc<Player>,
    pub hand: Hand,
    /// A copy of the stack in `hand` at the start of the phase.
    pub item: ItemStack,
    pub pos: BlockPos,
    pub face: BlockDirection,
    /// The clicked point, in world coordinates.
    pub hit_vector: Vector3<f64>,
    pub phase: UsePhase,
    /// Starts as `TryWithEmptyHand`, as in `NeoForge`.
    pub cancellation_result: InteractionResult,
}

impl UseItemOnBlockEvent {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        hand: Hand,
        item: ItemStack,
        pos: BlockPos,
        face: BlockDirection,
        hit_vector: Vector3<f64>,
        phase: UsePhase,
    ) -> Self {
        Self {
            player,
            hand,
            item,
            pos,
            face,
            hit_vector,
            phase,
            cancellation_result: InteractionResult::TryWithEmptyHand,
            cancelled: false,
        }
    }
}

/// The phase of a right click on a block, as `NeoForge` `UseItemOnBlockEvent.UsePhase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsePhase {
    /// The item use before the block use (`onItemUseFirst`). Pumpkin items have none.
    ItemBeforeBlock,
    /// The block use (`useItemOn`).
    Block,
    /// The item use on the block (`useOn`), which also places a block item.
    ItemAfterBlock,
}

impl PlayerEvent for UseItemOnBlockEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
