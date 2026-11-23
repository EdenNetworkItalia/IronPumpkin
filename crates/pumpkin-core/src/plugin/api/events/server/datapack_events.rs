//! The data pack events of `NeoForge` that Pumpkin has no event for. They fire after the server
//! loaded its data packs, at the start and after each reload.

use std::collections::BTreeSet;
use std::sync::Arc;

use pumpkin_macros::{Event, cancellable};
use pumpkin_util::loot_table::DynamicLootTable;
use thiserror::Error;

use crate::data::datapack::LoadedDatapack;
use crate::entity::player::Player;
use crate::server::Server;

/// `NeoForge` `TagsUpdatedEvent`, a concrete parent that the server never fires.
///
/// The server fires [`TagsUpdatedEventServerDataLoad`], and a
/// [`ParentEventHandler`](crate::plugin::ParentEventHandler) registered for this event gets it.
/// The client subclass has no use on a server.
#[derive(Event, Clone, Copy, Default)]
pub struct TagsUpdatedEvent;

/// The server loaded its tags: at the start, and after each reload of the data packs.
///
/// `NeoForge`: `TagsUpdatedEvent.ServerDataLoad`, fired by
/// `ReloadableServerResources.updateComponentsAndStaticRegistryTags`. The event has no
/// registry access or server resources: the tags are in the data pack manager of the server.
#[derive(Event, Clone, Copy, Default)]
#[event_parents(TagsUpdatedEvent)]
pub struct TagsUpdatedEventServerDataLoad;

/// The server sends data pack content to one player or to all players, before it sends the tags
/// and the data maps.
///
/// `NeoForge`: `OnDatapackSyncEvent`, fired by `PlayerList.placeNewPlayer` for the joining player
/// and by `PlayerList.reloadResources` for all players. `sendRecipes` is not supported: the
/// recipe content payload has no sender.
#[derive(Event, Clone)]
pub struct OnDatapackSyncEvent {
    /// The joining player, or `None` when the event syncs all players, as after a reload
    /// (`getPlayer`). Use [`Self::relevant_players`] for the players to sync.
    pub player: Option<Arc<Player>>,
}

impl OnDatapackSyncEvent {
    #[must_use]
    pub const fn new(player: Option<Arc<Player>>) -> Self {
        Self { player }
    }

    /// The players that need the data: the joining player or every online player
    /// (`getRelevantPlayers`).
    #[must_use]
    pub fn relevant_players(&self, server: &Server) -> Vec<Arc<Player>> {
        self.player
            .as_ref()
            .map_or_else(|| server.get_all_players(), |player| vec![player.clone()])
    }
}

/// A loot table loaded from a data pack, before the server uses it.
///
/// `NeoForge`: `LootTableLoadEvent`, fired by `EventHooks.loadLootTable` for each table of the
/// data packs at each load and reload. A handler replaces the table with `table`, or cancels the
/// event. A cancelled table, and a table without pools and functions, load as an empty table.
///
/// It fires for the tables that the data packs of the mods and of the world load. The loot
/// tables of the vanilla data are compiled in and fire no event. A handler gets `table` as a
/// shared pointer: `Arc::make_mut` changes a copy, and the server keeps the table of the handler
/// only when it is another pointer.
#[cancellable]
#[derive(Event, Clone)]
pub struct LootTableLoadEvent {
    /// The id of the table, such as `mymod:blocks/ruby_ore` (`getName`).
    pub name: String,
    /// The table (`getTable`, `setTable`).
    pub table: Arc<DynamicLootTable>,
}

impl LootTableLoadEvent {
    #[must_use]
    pub const fn new(name: String, table: Arc<DynamicLootTable>) -> Self {
        Self {
            name,
            table,
            cancelled: false,
        }
    }
}

/// A reload listener of a mod.
pub trait ReloadListener: Send + Sync {
    /// Runs after the server loaded the data packs, in the order of the event, before the tags
    /// event. `packs` are the loaded data packs in load order. A mod reads its own data files
    /// from their `root_path`.
    fn reload(&self, server: &Arc<Server>, packs: &[LoadedDatapack]);
}

/// A rejected call of [`AddServerReloadListenersEvent`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReloadListenerError {
    #[error("two reload listeners have the key {0}")]
    DuplicateKey(String),
    #[error("unknown reload listener {0}")]
    UnknownKey(String),
    #[error("the reload listener dependencies have a cycle")]
    Cycle,
}

/// The server loads or reloads its data packs: a mod adds its reload listeners.
///
/// `NeoForge`: `AddServerReloadListenersEvent`, fired by `EventHooks.onResourceReload` for each
/// load. The vanilla loaders of Pumpkin are not reload listeners, so every listener of a mod runs
/// after them, in registration order, as `NeoForge` runs a listener "after all vanilla
/// listeners". [`Self::add_dependency`] orders two listeners. A cycle fails the sort: the server
/// logs the error and runs none of the listeners of that load. The listeners also run at the
/// first load, after the worlds load.
#[derive(Event, Clone, Default)]
pub struct AddServerReloadListenersEvent {
    listeners: Vec<(String, Arc<dyn ReloadListener>)>,
    dependencies: Vec<(usize, usize)>,
}

impl AddServerReloadListenersEvent {
    /// Adds a listener that `key` names (`addListener`).
    ///
    /// # Errors
    ///
    /// [`ReloadListenerError::DuplicateKey`] when a listener has the key.
    pub fn add_listener(
        &mut self,
        key: impl Into<String>,
        listener: Arc<dyn ReloadListener>,
    ) -> Result<(), ReloadListenerError> {
        let key = key.into();
        if self.index(&key).is_some() {
            return Err(ReloadListenerError::DuplicateKey(key));
        }
        self.listeners.push((key, listener));
        Ok(())
    }

    /// The listener `first` must run before the listener `second` (`addDependency`).
    ///
    /// # Errors
    ///
    /// [`ReloadListenerError::UnknownKey`] when no listener has one of the keys.
    pub fn add_dependency(&mut self, first: &str, second: &str) -> Result<(), ReloadListenerError> {
        let first = self
            .index(first)
            .ok_or_else(|| ReloadListenerError::UnknownKey(first.to_owned()))?;
        let second = self
            .index(second)
            .ok_or_else(|| ReloadListenerError::UnknownKey(second.to_owned()))?;
        self.dependencies.push((first, second));
        Ok(())
    }

    /// The keys of the listeners in registration order (`getRegistry`).
    #[must_use]
    pub fn keys(&self) -> Vec<&str> {
        self.listeners.iter().map(|(key, _)| key.as_str()).collect()
    }

    fn index(&self, key: &str) -> Option<usize> {
        self.listeners.iter().position(|(other, _)| other == key)
    }

    /// The listeners in run order: a listener follows the listeners that it depends on, and
    /// the others keep their registration order.
    ///
    /// # Errors
    ///
    /// [`ReloadListenerError::Cycle`].
    pub fn sorted(&self) -> Result<Vec<Arc<dyn ReloadListener>>, ReloadListenerError> {
        let mut waiting: BTreeSet<usize> = (0..self.listeners.len()).collect();
        let mut order = Vec::with_capacity(waiting.len());
        while !waiting.is_empty() {
            let ready = waiting
                .iter()
                .copied()
                .find(|&node| {
                    !self
                        .dependencies
                        .iter()
                        .any(|&(first, second)| second == node && waiting.contains(&first))
                })
                .ok_or(ReloadListenerError::Cycle)?;
            waiting.remove(&ready);
            order.push(self.listeners[ready].1.clone());
        }
        Ok(order)
    }
}
