use std::sync::Arc;

use pumpkin_macros::Event;

use super::PlayerEvent;
use crate::entity::{EntityBase, player::Player};

/// `NeoForge` `PlayerEvent.StartTracking`: an entity became visible to a player.
///
/// Fires in `TrackedEntity::add_pairing` after the spawn packets are queued for the player.
#[derive(Event, Clone)]
pub struct PlayerEventStartTracking {
    pub player: Arc<Player>,
    pub target: Arc<dyn EntityBase>,
}

impl PlayerEventStartTracking {
    #[must_use]
    pub fn new(player: Arc<Player>, target: Arc<dyn EntityBase>) -> Self {
        Self { player, target }
    }
}

impl PlayerEvent for PlayerEventStartTracking {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}

/// `NeoForge` `PlayerEvent.StopTracking`: an entity is no longer visible to a player.
///
/// Fires in `TrackedEntity::remove_player`, `forget_player` and `broadcast_removed` for each
/// player that saw the entity.
#[derive(Event, Clone)]
pub struct PlayerEventStopTracking {
    pub player: Arc<Player>,
    pub target: Arc<dyn EntityBase>,
}

impl PlayerEventStopTracking {
    #[must_use]
    pub fn new(player: Arc<Player>, target: Arc<dyn EntityBase>) -> Self {
        Self { player, target }
    }
}

impl PlayerEvent for PlayerEventStopTracking {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
