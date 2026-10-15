use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use pumpkin_macros::Event;

use super::PlayerEvent;
use crate::entity::player::Player;

/// The file of a mod next to the player data file, as `getPlayerFile` of `NeoForge`. Returns
/// `None` for the suffix `dat`, which the player data file uses.
fn player_file(directory: &Path, uuid: &str, suffix: &str) -> Option<PathBuf> {
    (suffix != "dat").then(|| directory.join(format!("{uuid}.{suffix}")))
}

/// `NeoForge` `PlayerEvent.LoadFromFile`: the server loaded the data of a joining player.
///
/// Fires in `Server::add_player` after the player data is read, also for a player without a
/// data file.
#[derive(Event, Clone)]
pub struct PlayerEventLoadFromFile {
    pub player: Arc<Player>,
    /// The directory of the player data files.
    pub player_directory: PathBuf,
    pub player_uuid: String,
}

impl PlayerEventLoadFromFile {
    #[must_use]
    pub const fn new(player: Arc<Player>, player_directory: PathBuf, player_uuid: String) -> Self {
        Self {
            player,
            player_directory,
            player_uuid,
        }
    }

    /// The file `<uuid>.<suffix>` in the player data directory, or `None` for `dat`.
    #[must_use]
    pub fn player_file(&self, suffix: &str) -> Option<PathBuf> {
        player_file(&self.player_directory, &self.player_uuid, suffix)
    }
}

impl PlayerEvent for PlayerEventLoadFromFile {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}

/// `NeoForge` `PlayerEvent.SaveToFile`: the server saves the data of a player.
///
/// Fires in `ServerPlayerData` when a player leaves, at each periodic save and in
/// `save_all_players`, while the save is enabled.
#[derive(Event, Clone)]
pub struct PlayerEventSaveToFile {
    pub player: Arc<Player>,
    pub player_directory: PathBuf,
    pub player_uuid: String,
}

impl PlayerEventSaveToFile {
    #[must_use]
    pub const fn new(player: Arc<Player>, player_directory: PathBuf, player_uuid: String) -> Self {
        Self {
            player,
            player_directory,
            player_uuid,
        }
    }

    /// The file `<uuid>.<suffix>` in the player data directory, or `None` for `dat`.
    #[must_use]
    pub fn player_file(&self, suffix: &str) -> Option<PathBuf> {
        player_file(&self.player_directory, &self.player_uuid, suffix)
    }
}

impl PlayerEvent for PlayerEventSaveToFile {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
