use pumpkin_data::{Block, BlockState};
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::player::Player;

use super::BlockEvent;

/// An event that occurs when a block is placed.
///
/// This event contains information about the player placing the block, the block being placed,
/// the block being placed against, and whether the player can build.
///
/// `NeoForge`: `BlockEvent.EntityPlaceEvent` placed by a player, fired in
/// `BlockRegistry::place_block` before the block changes.
#[cancellable]
#[derive(Event, Clone)]
pub struct BlockPlaceEvent {
    /// The player placing the block.
    pub player: Arc<Player>,

    /// The block that is being placed.
    pub block_placed: &'static Block,

    /// The block that the new block is being placed against.
    pub block_placed_against: &'static Block,

    /// The position where the block is being placed.
    pub block_position: BlockPos,

    /// A boolean indicating whether the player can build.
    pub can_build: bool,

    /// The state that the placement sets (`EntityPlaceEvent#getPlacedBlock`).
    pub placed_state: &'static BlockState,

    /// The state that the placement replaces (`getBlockSnapshot().getState()`).
    pub replaced_state: &'static BlockState,
}

impl BlockPlaceEvent {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        block_placed: &'static Block,
        block_placed_against: &'static Block,
        block_position: BlockPos,
        can_build: bool,
        placed_state: &'static BlockState,
        replaced_state: &'static BlockState,
    ) -> Self {
        Self {
            player,
            block_placed,
            block_placed_against,
            block_position,
            can_build,
            placed_state,
            replaced_state,
            cancelled: false,
        }
    }
}

impl BlockEvent for BlockPlaceEvent {
    fn get_block(&self) -> &Block {
        self.block_placed
    }
}
