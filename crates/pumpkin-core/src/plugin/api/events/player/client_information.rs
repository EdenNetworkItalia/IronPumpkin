use std::sync::Arc;

use pumpkin_macros::Event;

use super::PlayerEvent;
use crate::entity::player::Player;
use crate::net::PlayerConfig;

/// `NeoForge` `ClientInformationUpdatedEvent`: a player in play sent its client settings.
///
/// Fires in `JavaClient::handle_client_information` after the player stored the new settings,
/// also when nothing changed.
#[derive(Event, Clone)]
pub struct ClientInformationUpdatedEvent {
    pub player: Arc<Player>,
    pub old_information: Arc<PlayerConfig>,
    pub updated_information: Arc<PlayerConfig>,
}

impl ClientInformationUpdatedEvent {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        old_information: Arc<PlayerConfig>,
        updated_information: Arc<PlayerConfig>,
    ) -> Self {
        Self {
            player,
            old_information,
            updated_information,
        }
    }
}

impl PlayerEvent for ClientInformationUpdatedEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
