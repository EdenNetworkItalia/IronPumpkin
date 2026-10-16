use pumpkin_data::{Block, BlockState};
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::player::Player;

use super::BlockEvent;

/// An event that occurs when a block is broken.
///
/// This event contains information about the player breaking the block, the block itself,
/// the experience gained, and whether the block should drop items.
///
/// `NeoForge`: `net.neoforged.neoforge.event.level.block.BreakBlockEvent`, fired in
/// `World::break_block` before the block changes.
#[cancellable]
#[derive(Event, Clone)]
pub struct BlockBreakEvent {
    /// The player who is breaking the block, if applicable.
    pub player: Option<Arc<Player>>,

    /// The block that is being broken.
    pub block: &'static Block,

    /// The state of the block that is being broken (`BlockEvent#getState`).
    pub state: &'static BlockState,

    /// The position of the block that is being broken.
    pub block_position: BlockPos,

    /// The experience that the break drops, read back when the block drops items.
    pub exp: u32,

    /// A boolean indicating whether the block should drop items.
    pub drop: bool,
}

impl BlockBreakEvent {
    /// Creates a new instance of `BlockBreakEvent`.
    ///
    /// # Arguments
    /// - `player`: An optional reference to the player breaking the block.
    /// - `block`: The block that is being broken.
    /// - `state`: The state of the block that is being broken.
    /// - `block_position`: The position of the block that is being broken.
    /// - `exp`: The amount of experience gained from breaking the block.
    /// - `drop`: A boolean indicating whether the block should drop items.
    ///
    /// # Returns
    /// A new instance of `BlockBreakEvent`.
    #[must_use]
    pub const fn new(
        player: Option<Arc<Player>>,
        block: &'static Block,
        state: &'static BlockState,
        block_position: BlockPos,
        exp: u32,
        drop: bool,
    ) -> Self {
        Self {
            player,
            block,
            state,
            block_position,
            exp,
            drop,
            cancelled: false,
        }
    }
}

impl BlockEvent for BlockBreakEvent {
    fn get_block(&self) -> &Block {
        self.block
    }
}
