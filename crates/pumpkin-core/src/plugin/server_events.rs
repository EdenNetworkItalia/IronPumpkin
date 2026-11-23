//! The `IronPumpkin` fire sites of the `NeoForge` command, permission and data pack events of
//! the root `event` package.
//!
//! Each function checks `has_handlers` before it builds an event, so a fire site with no handler
//! allocates nothing.

use std::sync::Arc;

use pumpkin_util::PermissionLvl;
use pumpkin_util::loot_table::DynamicLootTable;
use pumpkin_util::text::TextComponent;
use tracing::error;
use uuid::Uuid;

use crate::command::{CommandSource, dispatcher::Interception};
use crate::entity::player::Player;
use crate::plugin::api::events::{
    player::{PermissionsChangedEvent, player_command_preprocess::PlayerCommandPreprocessEvent},
    server::{
        AddServerReloadListenersEvent, CommandEvent, LootTableLoadEvent, OnDatapackSyncEvent,
        TagsUpdatedEventServerDataLoad,
    },
};
use crate::server::Server;

/// The interceptor of the server dispatcher: fires [`CommandEvent`] for the command of
/// `Commands.performCommand`.
pub fn command_interceptor(source: &CommandSource, input: &str) -> Interception {
    let Some(server) = source.server.as_ref() else {
        return Interception::Run;
    };
    let Some(event) = server.plugin_manager.fire_blocking_with(server, || {
        CommandEvent::new(source.clone(), input.to_owned())
    }) else {
        return Interception::Run;
    };
    if event.cancelled {
        if let Some(message) = event.exception {
            error!("'/{input}' threw an exception: {message}");
            source.send_error(TextComponent::text(message));
        }
        Interception::Cancel
    } else if event.command == input {
        Interception::Run
    } else {
        Interception::Replace(event.command)
    }
}

/// Fires [`PlayerCommandPreprocessEvent`] for a command of `player` before the command event
/// and the dispatcher. Returns the command to run, or `None` when a handler cancelled it.
pub async fn preprocess_command(
    server: &Arc<Server>,
    player: &Arc<Player>,
    command: &str,
) -> Option<String> {
    let event = server
        .plugin_manager
        .fire_with(server, || PlayerCommandPreprocessEvent {
            player: player.clone(),
            command: command.to_owned(),
            cancelled: false,
        })
        .await;
    match event {
        None => Some(command.to_owned()),
        Some(event) if event.cancelled => None,
        Some(event) => Some(event.command),
    }
}

/// Fires [`PermissionsChangedEvent`] for the profile `id` that gets the level `new_level`.
///
/// Returns `true` when a handler cancelled it, so the caller must leave the op list as it is.
/// Call it before the write lock of the op list: a handler can read the list.
///
/// As `EventHooks.onPermissionChanged`, it fires only for an online player and only when the
/// level changes.
pub fn permission_change_cancelled(
    server: &Arc<Server>,
    id: Uuid,
    new_level: PermissionLvl,
) -> bool {
    if !server
        .plugin_manager
        .has_handlers::<PermissionsChangedEvent>()
    {
        return false;
    }
    let Some(player) = server.get_player_by_uuid(id) else {
        return false;
    };
    let old_level = server
        .data
        .operator_config
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_entry(&id)
        .map_or(PermissionLvl::Zero, |entry| entry.level);
    if old_level == new_level {
        return false;
    }
    server
        .plugin_manager
        .fire_blocking_with(server, || {
            PermissionsChangedEvent::new(player, new_level, old_level)
        })
        .is_some_and(|event| event.cancelled)
}

/// Fires the events of a finished data pack load.
///
/// The order is the order of `NeoForge`: the loot table events, the reload listeners, then the
/// tags event. Call it after the data packs loaded and before the data maps load.
pub fn server_data_loaded(server: &Arc<Server>) {
    load_loot_tables(server);
    run_reload_listeners(server);
    server
        .plugin_manager
        .fire_blocking_with(server, || TagsUpdatedEventServerDataLoad);
}

/// Fires [`LootTableLoadEvent`] for each loaded loot table and keeps the table that the handlers
/// leave. A cancelled table, as a table that a handler changed, replaces the loaded one: the
/// cancelled one with an empty table, so the compiled table of the same id does not come back.
fn load_loot_tables(server: &Arc<Server>) {
    if !server.plugin_manager.has_handlers::<LootTableLoadEvent>() {
        return;
    }
    let mut replacements = Vec::new();
    for (name, table) in server.datapack_manager.loaded_loot_tables() {
        let mut event = LootTableLoadEvent::new(name.clone(), table.clone());
        server.plugin_manager.fire_blocking(server, &mut event);
        if event.cancelled {
            replacements.push((name, Arc::new(DynamicLootTable::default())));
        } else if !Arc::ptr_eq(&event.table, &table) {
            replacements.push((name, event.table));
        }
    }
    server.datapack_manager.replace_loot_tables(replacements);
}

/// Fires [`AddServerReloadListenersEvent`] and runs the listeners that the mods added.
fn run_reload_listeners(server: &Arc<Server>) {
    let Some(event) = server
        .plugin_manager
        .fire_blocking_with(server, AddServerReloadListenersEvent::default)
    else {
        return;
    };
    match event.sorted() {
        Ok(listeners) => {
            let packs = server.datapack_manager.get_loaded_packs();
            for listener in listeners {
                listener.reload(server, &packs);
            }
        }
        Err(err) => error!("The reload listeners did not run: {err}"),
    }
}

/// Fires [`OnDatapackSyncEvent`] for the joining `player`, or for all players with `None`.
pub fn datapack_sync(server: &Arc<Server>, player: Option<&Arc<Player>>) {
    server
        .plugin_manager
        .fire_blocking_with(server, || OnDatapackSyncEvent::new(player.cloned()));
}
