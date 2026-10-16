//! The block events of `NeoForge` (`event.level.BlockEvent`, `event.level.block`) at fire sites
//! where Pumpkin has no event.

use std::sync::Arc;

use pumpkin_data::BlockState;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;

use crate::entity::EntityBase;
use crate::world::World;

/// An entity that is not a player places a block.
///
/// `NeoForge`: `BlockEvent.EntityPlaceEvent` with a non-player entity, fired by
/// `EventHooks.onBlockPlace` in `EndermanLeaveBlockGoal.tick` before the block changes. A
/// placement by a player fires [`BlockPlaceEvent`](crate::plugin::block::block_place::BlockPlaceEvent).
/// A cancel keeps the block. `NeoForge` 26.3 builds the snapshot at the block below the target, so
/// its `getPos` and `getState` name that block; here `pos` is the target.
#[cancellable]
#[derive(Event, Clone)]
pub struct BlockEventEntityPlaceEvent {
    /// The entity that places the block (`getEntity`).
    pub entity: Arc<dyn EntityBase>,
    pub world: Arc<World>,
    pub pos: BlockPos,
    /// The state that the placement sets (`getPlacedBlock`).
    pub placed_state: &'static BlockState,
    /// The state that the placement replaces (`getBlockSnapshot().getState()`).
    pub replaced_state: &'static BlockState,
    /// The state of the block below `pos` (`getPlacedAgainst`).
    pub placed_against: &'static BlockState,
}

impl BlockEventEntityPlaceEvent {
    #[must_use]
    pub const fn new(
        entity: Arc<dyn EntityBase>,
        world: Arc<World>,
        pos: BlockPos,
        placed_state: &'static BlockState,
        replaced_state: &'static BlockState,
        placed_against: &'static BlockState,
    ) -> Self {
        Self {
            entity,
            world,
            pos,
            placed_state,
            replaced_state,
            placed_against,
            cancelled: false,
        }
    }
}
