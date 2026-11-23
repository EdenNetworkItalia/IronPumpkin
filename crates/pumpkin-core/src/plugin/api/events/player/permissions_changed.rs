use pumpkin_macros::{Event, cancellable};
use pumpkin_util::PermissionLvl;
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// An online player is opped or deopped.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.player.PermissionsChangedEvent`, fired by
/// `PlayerList.op` and `PlayerList.deop` before the op list changes, only when the level
/// changes. The old level is the level of the op list entry, not the level that the player has
/// now. A cancel keeps the old level: the op list does not change. The op and deop commands and
/// the op calls of the Wasm plugins fire it. The permission level that a plugin sets on a
/// player does not touch the op list and does not fire it.
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
