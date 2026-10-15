use std::sync::Arc;

use pumpkin_data::BlockState;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;

use super::PlayerEvent;
use crate::entity::player::Player;

/// `NeoForge` `PlayerEvent.BreakSpeed`: the mining speed of a player on a block.
///
/// Fires in `block::calc_block_breaking` on every mining step, also each tick while the player
/// mines. `new_speed` is read back. A cancel makes the block unbreakable for this step.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerEventBreakSpeed {
    pub player: Arc<Player>,
    pub state: &'static BlockState,
    /// The speed before the handlers ran.
    pub original_speed: f32,
    /// The speed the server uses.
    pub new_speed: f32,
    /// The position of the block.
    pub pos: Option<BlockPos>,
}

impl PlayerEventBreakSpeed {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        state: &'static BlockState,
        original_speed: f32,
        pos: Option<BlockPos>,
    ) -> Self {
        Self {
            player,
            state,
            original_speed,
            new_speed: original_speed,
            pos,
            cancelled: false,
        }
    }
}

impl PlayerEvent for PlayerEventBreakSpeed {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}

/// `NeoForge` `PlayerEvent.HarvestCheck`: whether the player gets the drops of a block.
///
/// Fires in `Player::can_harvest`, for the mining speed and for the drops of a broken block.
/// `can_harvest` is read back.
#[derive(Event, Clone)]
pub struct PlayerEventHarvestCheck {
    pub player: Arc<Player>,
    pub state: &'static BlockState,
    pub pos: BlockPos,
    /// Whether the player harvests the block. Starts with the vanilla value.
    pub can_harvest: bool,
}

impl PlayerEventHarvestCheck {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        state: &'static BlockState,
        pos: BlockPos,
        can_harvest: bool,
    ) -> Self {
        Self {
            player,
            state,
            pos,
            can_harvest,
        }
    }
}

impl PlayerEvent for PlayerEventHarvestCheck {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
