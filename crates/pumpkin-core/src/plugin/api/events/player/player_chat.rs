use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use crate::entity::player::Player;

use super::PlayerEvent;

/// An event that occurs when a player sends a chat message.
///
/// This event contains information about the sender, message, and recipients.
///
/// `NeoForge`: `net.neoforged.neoforge.event.ServerChatEvent`, fired by
/// `CommonHooks.onServerChatSubmittedEvent` when the server gets a chat message. A cancelled
/// event sends the message to nobody. `message` is the text the server formats and sends: plain
/// text, where `NeoForge` has a component.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerChatEvent {
    /// The player who sent the message.
    pub player: Arc<Player>,

    /// The message being sent.
    pub message: String,

    /// The message as the player sent it (`ServerChatEvent#getRawText`). Handlers do not change
    /// it.
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
        Self {
            player,
            raw_text: message.clone(),
            message,
            recipients,
            signature,
            cancelled: false,
        }
    }

    /// The name of the player who sent the message (`ServerChatEvent#getUsername`).
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
