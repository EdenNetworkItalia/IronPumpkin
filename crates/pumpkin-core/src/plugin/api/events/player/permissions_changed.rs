use pumpkin_macros::{Event, cancellable};
use pumpkin_util::PermissionLvl;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// An online player is opped or deopped.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.player.PermissionsChangedEvent`, fired by
/// `PlayerList.op` and `PlayerList.deop` before the op list changes, only when the level
/// changes. The op and deop commands, the op calls of the Wasm plugins and the operator methods
/// of the server management protocol fire it, and so does `Player::change_permission_lvl`,
/// which the permission level call of the Wasm plugins uses. A cancel keeps the old level. For
/// the op list paths the old level is the level of the op list entry, not the level that the
/// player has now. `Player::change_permission_lvl` changes the level of the player alone, so
/// its old level is the level that the player has now.
#[cancellable]
#[derive(Event, Clone)]
pub struct PermissionsChangedEvent {
    /// The player whose level changes.
    pub player: Arc<Player>,
    /// The level after the change (`getNewLevel`).
    pub new_level: PermissionLvl,
    /// The level in the op list before the change (`getOldLevel`).
    pub old_level: PermissionLvl,
}

impl PermissionsChangedEvent {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        new_level: PermissionLvl,
        old_level: PermissionLvl,
    ) -> Self {
        Self {
            player,
            new_level,
            old_level,
            cancelled: false,
        }
    }
}

impl PlayerEvent for PermissionsChangedEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
