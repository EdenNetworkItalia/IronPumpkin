use std::sync::Arc;

use pumpkin_macros::Event;

use super::PlayerEvent;
use crate::entity::player::Player;

/// `NeoForge` `PlayerSwitchHotbarSlotEvent.Post`: a player selected another hotbar slot.
///
/// Fires in the held-item handlers of Java and Bedrock after the slot changed.
#[derive(Event, Clone)]
pub struct PlayerSwitchHotbarSlotEventPost {
    pub player: Arc<Player>,
    pub old_slot_index: u8,
    pub new_slot_index: u8,
}

impl PlayerSwitchHotbarSlotEventPost {
    #[must_use]
    pub const fn new(player: Arc<Player>, old_slot_index: u8, new_slot_index: u8) -> Self {
        Self {
            player,
            old_slot_index,
            new_slot_index,
        }
    }
}

impl PlayerEvent for PlayerSwitchHotbarSlotEventPost {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
