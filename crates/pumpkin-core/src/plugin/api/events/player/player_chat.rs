use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::player::Player;

use super::PlayerEvent;

/// An event that occurs when a player sends a chat message.
///
/// This event contains information about the sender, message, and recipients.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerChatEvent {
    /// The player who sent the message.
    pub player: Arc<Player>,

    /// The message being sent.
    pub message: String,

    /// The message as the player sent it. Handlers do not change it. It is empty when no handler
    /// listened as the event was created, so a server without a chat handler copies nothing.
    pub raw_text: String,

    /// The recipients of the message. If empty, the message is broadcasted to all players.
    pub recipients: Vec<Arc<Player>>,

    /// The optional 256-byte cryptographic signature of the message.
    pub signature: Option<Vec<u8>>,
}

impl PlayerChatEvent {
    /// Creates a new instance of `PlayerChatEvent`.
    ///
    /// # Arguments
    /// - `player`: A reference to the player sending the message.
    /// - `message`: The message being sent.
    /// - `recipients`: The recipients of the message. If empty, the message is broadcasted to all players.
    /// - `signature`: The optional cryptographic signature of the message.
    ///
    /// # Returns
    /// A new instance of `PlayerChatEvent`.
    pub fn new(
        player: Arc<Player>,
        message: String,
        recipients: Vec<Arc<Player>>,
        signature: Option<Vec<u8>>,
    ) -> Self {
        let raw_text = if player
            .world()
            .server
            .upgrade()
            .is_some_and(|server| server.plugin_manager.has_handlers::<Self>())
        {
            message.clone()
        } else {
            String::new()
        };
        Self {
            player,
            raw_text,
            message,
            recipients,
            signature,
            cancelled: false,
        }
    }

    /// The name of the player who sent the message.
    #[must_use]
    pub fn username(&self) -> &str {
        &self.player.gameprofile.name
    }
}

impl PlayerEvent for PlayerChatEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
