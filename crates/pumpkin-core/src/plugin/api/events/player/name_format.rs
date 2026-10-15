use std::sync::Arc;

use pumpkin_macros::Event;
use pumpkin_util::text::TextComponent;

use super::PlayerEvent;
use crate::entity::player::Player;

/// `NeoForge` `PlayerEvent.NameFormat`: the display name of a player.
///
/// Fires in `Player::get_display_name` when no display name is set through
/// `Player::set_display_name`. `display_name` is read back. The name is not cached, so the
/// event fires on every read and `refreshDisplayName` needs no counterpart.
#[derive(Event, Clone)]
pub struct PlayerEventNameFormat {
    pub player: Arc<Player>,
    pub username: TextComponent,
    pub display_name: TextComponent,
}

impl PlayerEventNameFormat {
    #[must_use]
    pub fn new(player: Arc<Player>, username: TextComponent) -> Self {
        Self {
            player,
            display_name: username.clone(),
            username,
        }
    }
}

impl PlayerEvent for PlayerEventNameFormat {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}

/// `NeoForge` `PlayerEvent.TabListNameFormat`: the name of a player in the tab list.
///
/// Fires in `Player::get_tab_list_name` when no name is set through
/// `Player::set_tab_list_name`. `display_name` is read back, and `None` shows the player name.
/// `Player::set_tab_list_name` sends a change to the clients, as `refreshTabListName` does.
#[derive(Event, Clone)]
pub struct PlayerEventTabListNameFormat {
    pub player: Arc<Player>,
    pub display_name: Option<TextComponent>,
}

impl PlayerEventTabListNameFormat {
    #[must_use]
    pub const fn new(player: Arc<Player>) -> Self {
        Self {
            player,
            display_name: None,
        }
    }
}

impl PlayerEvent for PlayerEventTabListNameFormat {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
