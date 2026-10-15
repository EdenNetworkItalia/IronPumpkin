use std::sync::Arc;

use pumpkin_macros::Event;

use super::PlayerEvent;
use crate::entity::player::Player;

/// `NeoForge` `PlayerFlyableFallEvent`: a player that may fly lands and takes no fall damage.
///
/// Fires in `LivingEntity::handle_fall_damage`. The server reads nothing back, as in
/// `NeoForge`. `distance` is the fall distance; `NeoForge` 26.3 passes the multiplier for both
/// values.
#[derive(Event, Clone)]
pub struct PlayerFlyableFallEvent {
    pub player: Arc<Player>,
    pub distance: f32,
    pub multiplier: f32,
}

impl PlayerFlyableFallEvent {
    #[must_use]
    pub const fn new(player: Arc<Player>, distance: f32, multiplier: f32) -> Self {
        Self {
            player,
            distance,
            multiplier,
        }
    }
}

impl PlayerEvent for PlayerFlyableFallEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
