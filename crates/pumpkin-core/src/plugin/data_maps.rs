//! The server side of the `NeoForge` registry data maps that `ironpumpkin-mods` provides.
//!
//! The data map types, the loader and the values live in `ironpumpkin-mods`, which installs a
//! [`DataMapHost`] at startup. The server calls the host after each data pack load, in the
//! configuration phase of each connection and when a player joins. Without a host the server
//! advertises the `NeoForge` built-in data maps and syncs nothing.

pub mod builtin;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use pumpkin_macros::Event;
use pumpkin_protocol::java::neoforge::{ConnectionProtocol, KnownRegistryDataMapsPayload};
use pumpkin_util::{
    identifier::Identifier,
    text::{TextComponent, color::NamedColor},
};
use tracing::warn;

use crate::{
    entity::player::Player,
    net::java::{
        configuration_payloads,
        neoforge::{ConnectionType, NegotiatedState},
    },
    plugin::network_events::{negotiated_channels, send_to_player},
    server::Server,
};

/// `RegistryDataMapSyncPayload.TYPE`.
pub const SYNC_CHANNEL: &str = "neoforge:registry_data_map_sync";

/// What `ironpumpkin-mods` does for the data maps.
pub trait DataMapHost: Send + Sync {
    /// Loads the data map files of the enabled data packs of `server`, as
    /// `DataMapLoader.apply` does, and fires [`DataMapsUpdatedEvent`] for each registry it
    /// updated.
    fn load(&self, server: &Arc<Server>);

    /// The payload of `RegistryDataMapNegotiation.run` for a client with the channel.
    fn known_data_maps(&self) -> KnownRegistryDataMapsPayload;

    /// The data maps with `mandatorySync`, each as `id (registry)`.
    fn mandatory_data_maps(&self) -> Vec<String>;

    /// The bodies of the sync payloads for a client that knows the data maps `known`, per
    /// registry, as `NeoForgeEventHandler.onDpSync` builds them: one per registry.
    fn sync_payloads(&self, known: &BTreeMap<Identifier, BTreeSet<Identifier>>) -> Vec<Vec<u8>>;
}

static HOST: OnceLock<&'static dyn DataMapHost> = OnceLock::new();

/// Installs the host. Returns false when a host is installed already.
pub fn set_host(host: &'static dyn DataMapHost) -> bool {
    HOST.set(host).is_ok()
}

/// `DataMapsUpdatedEvent.UpdateCause`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateCause {
    /// The data maps of a client changed after a sync. A server never fires it.
    ClientSync,
    /// The server loaded the data maps from the data packs.
    ServerReload,
}

/// Fires after the data maps of a registry loaded, at startup and after each `/reload`.
///
/// `NeoForge`: `net.neoforged.neoforge.registries.datamaps.DataMapsUpdatedEvent`, posted by
/// `DataMapLoader.apply` for each registry that has at least one data map file. The values are
/// read through the data map types of `ironpumpkin-mods`.
#[derive(Event, Debug, Clone)]
pub struct DataMapsUpdatedEvent {
    registry: Identifier,
    cause: UpdateCause,
}

impl DataMapsUpdatedEvent {
    #[must_use]
    pub const fn new(registry: Identifier, cause: UpdateCause) -> Self {
        Self { registry, cause }
    }

    /// `getRegistryKey`: the registry whose data maps changed.
    #[must_use]
    pub const fn registry_key(&self) -> &Identifier {
        &self.registry
    }

    /// `getCause`.
    #[must_use]
    pub const fn cause(&self) -> UpdateCause {
        self.cause
    }
}

/// The payload of `RegistryDataMapNegotiation.run`.
pub(crate) fn known_data_maps() -> KnownRegistryDataMapsPayload {
    HOST.get()
        .map_or_else(configuration_payloads::known_registry_data_maps, |host| {
            host.known_data_maps()
        })
}

/// The data maps with `mandatorySync`, each as `id (registry)`; empty without a host.
pub(crate) fn mandatory_data_maps() -> Vec<String> {
    HOST.get()
        .map(|host| host.mandatory_data_maps())
        .unwrap_or_default()
}

/// The disconnect reason of `RegistryDataMapNegotiation.run` for a client without the channel,
/// or `None` when no data map has `mandatorySync`.
pub(crate) fn vanilla_refusal() -> Option<TextComponent> {
    refusal(&mandatory_data_maps())
}

/// Whether the join sync of a player read the data map values. A `/reload` syncs only the
/// players with the flag: a player without it reads the new values in its own join sync.
#[derive(Debug, Default)]
pub struct JoinedFlag(AtomicBool);

impl JoinedFlag {
    #[must_use]
    pub const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    /// The flag is set before the values are read and checked after they are stored, both
    /// `SeqCst`: a join and a `/reload` that run at once cannot both miss the new values.
    fn set(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    fn is_set(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

impl Clone for JoinedFlag {
    fn clone(&self) -> Self {
        Self(AtomicBool::new(self.is_set()))
    }
}

impl PartialEq for JoinedFlag {
    fn eq(&self, other: &Self) -> bool {
        self.is_set() == other.is_set()
    }
}

impl Eq for JoinedFlag {}

/// Whether the client gets the sync payloads: a `NeoForge` client with the play channel that
/// named data maps in its reply.
fn syncs(state: &NegotiatedState) -> bool {
    state.connection_type == ConnectionType::NeoForge
        && !state.known_data_maps.is_empty()
        && negotiated_channels(state).has_channel(
            ConnectionProtocol::Play,
            &Identifier::parse_static(SYNC_CHANNEL),
        )
}

/// The literal text of `RegistryDataMapNegotiation`: a vanilla client lacks the translations.
fn refusal(mandatory: &[String]) -> Option<TextComponent> {
    if mandatory.is_empty() {
        return None;
    }
    Some(
        TextComponent::text(
            "This server does not support vanilla clients as it has mandatory registry data maps: ",
        )
        .add_child(TextComponent::text(mandatory.join(", ")).color_named(NamedColor::Gold)),
    )
}

/// Loads the data maps after a data pack load. After a `/reload`, the players in play get the
/// new values, as `PlayerList.reloadResources` posts `OnDatapackSyncEvent`.
pub(crate) fn after_load(server: &Arc<Server>, reload: bool) {
    let Some(host) = HOST.get() else {
        return;
    };
    host.load(server);
    if reload {
        for player in server.get_all_players() {
            if player.negotiated_state().data_maps_joined.is_set() {
                send_sync(*host, &player);
            }
        }
    }
}

/// The join sync: sends the synced data maps to the client of `player`, as
/// `NeoForgeEventHandler.onDpSync` does on `PlayerList.placeNewPlayer`.
pub(crate) fn sync_player(player: &Player) {
    let Some(host) = HOST.get() else {
        return;
    };
    player.negotiated_state().data_maps_joined.set();
    send_sync(*host, player);
}

/// Sends the values only to a client with the channel that answered the negotiation.
fn send_sync(host: &dyn DataMapHost, player: &Player) {
    let state = player.negotiated_state();
    if !syncs(state) {
        return;
    }
    for body in host.sync_payloads(&state.known_data_maps) {
        if let Err(error) = send_to_player(player, SYNC_CHANNEL, &body) {
            warn!(
                "Failed to send the data maps to {}: {error}",
                player.gameprofile.name
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vanilla_client_is_refused_only_for_mandatory_maps() {
        assert!(refusal(&[]).is_none());
        let reason = refusal(&[
            "mymod:fuel (minecraft:item)".to_owned(),
            "mymod:heat (minecraft:block)".to_owned(),
        ])
        .expect("a mandatory map refuses");
        assert_eq!(
            reason.get_text(),
            "This server does not support vanilla clients as it has mandatory registry data maps: mymod:fuel (minecraft:item), mymod:heat (minecraft:block)"
        );
    }

    #[test]
    fn only_a_neoforge_client_with_the_channel_and_known_maps_gets_the_sync() {
        use pumpkin_protocol::java::neoforge::NetworkChannel;

        let mut state = NegotiatedState {
            connection_type: ConnectionType::NeoForge,
            ..NegotiatedState::default()
        };
        state.known_data_maps.insert(
            Identifier::vanilla_static("item"),
            [Identifier::parse_static("mymod:fuel")].into(),
        );
        assert!(!syncs(&state), "no play channel");
        let channel = Identifier::parse_static(SYNC_CHANNEL);
        state.payload_setup.channels.insert(
            ConnectionProtocol::Play,
            [(
                channel.clone(),
                NetworkChannel {
                    id: channel,
                    chosen_version: "1".to_owned(),
                },
            )]
            .into(),
        );
        assert!(syncs(&state));
        let known = std::mem::take(&mut state.known_data_maps);
        assert!(!syncs(&state), "no known map");
        state.known_data_maps = known;
        state.connection_type = ConnectionType::Other;
        assert!(!syncs(&state), "a vanilla client");
    }

    #[test]
    fn the_joined_flag_starts_clear_and_clones_its_value() {
        let flag = JoinedFlag::new();
        assert!(!flag.is_set());
        assert_eq!(flag, JoinedFlag::default());
        flag.set();
        assert!(flag.is_set());
        let copy = flag.clone();
        assert!(copy.is_set());
        assert_ne!(flag, JoinedFlag::new());
        assert!(!NegotiatedState::default().data_maps_joined.is_set());
    }

    #[test]
    fn without_a_host_the_known_maps_are_the_neoforge_built_ins() {
        if HOST.get().is_none() {
            assert_eq!(
                known_data_maps(),
                configuration_payloads::known_registry_data_maps()
            );
        }
    }
}
