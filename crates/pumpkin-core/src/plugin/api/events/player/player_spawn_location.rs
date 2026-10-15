use super::PlayerEvent;
use crate::entity::player::Player;
use pumpkin_data::dimension::Dimension;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::vector3::Vector3;
use std::sync::Arc;

/// An event that occurs when determining player spawn position.
///
/// `NeoForge` `PlayerRespawnPositionEvent`: `spawn_pos`, `yaw`, `pitch`, `dimension` and
/// `copy_original_spawn_position` are read back.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerSpawnLocationEvent {
    /// The player.
    pub player: Arc<Player>,
    /// Spawn position.
    pub spawn_pos: Vector3<f64>,
    pub yaw: f32,
    pub pitch: f32,
    /// The dimension of the respawn.
    pub dimension: Dimension,
    /// The position and dimension before the handlers ran.
    pub original_spawn_pos: Vector3<f64>,
    pub original_dimension: Dimension,
    /// Whether the player leaves the End through the exit portal.
    pub from_end_fight: bool,
    /// Whether the player keeps its respawn point. Starts as `true` when the respawn block
    /// exists; `false` clears the respawn point.
    pub copy_original_spawn_position: bool,
}

impl PlayerSpawnLocationEvent {
    #[must_use]
    pub fn new(
        player: Arc<Player>,
        spawn_pos: Vector3<f64>,
        yaw: f32,
        pitch: f32,
        dimension: Dimension,
        from_end_fight: bool,
        copy_original_spawn_position: bool,
    ) -> Self {
        Self {
            player,
            spawn_pos,
            yaw,
            pitch,
            original_spawn_pos: spawn_pos,
            original_dimension: dimension.clone(),
            dimension,
            from_end_fight,
            copy_original_spawn_position,
            cancelled: false,
        }
    }
}

impl PlayerEvent for PlayerSpawnLocationEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
