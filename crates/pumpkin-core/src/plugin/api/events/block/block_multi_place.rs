use pumpkin_data::{BlockState, BlockStateId};
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::{entity::player::Player, world::World};

/// An event that occurs when multiple blocks are placed at once (e.g. bed, door).
///
/// `NeoForge`: `BlockEvent.EntityMultiPlaceEvent` placed by a player, fired in
/// `BlockRegistry::place_block` after `BlockPlaceEvent` and before the blocks change.
#[cancellable]
#[derive(Event, Clone)]
pub struct BlockMultiPlaceEvent {
    pub player: Arc<Player>,
    pub world: Arc<World>,
    /// The blocks that the placement sets, the placed block first.
    pub placed_blocks: Vec<(BlockPos, BlockStateId)>,
    /// The states that the placement replaces, in the order of `placed_blocks`
    /// (`getReplacedBlockSnapshots`).
    pub replaced_blocks: Vec<(BlockPos, &'static BlockState)>,
}

impl BlockMultiPlaceEvent {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        world: Arc<World>,
        placed_blocks: Vec<(BlockPos, BlockStateId)>,
        replaced_blocks: Vec<(BlockPos, &'static BlockState)>,
    ) -> Self {
        Self {
            player,
            world,
            placed_blocks,
            replaced_blocks,
            cancelled: false,
        }
    }
}
