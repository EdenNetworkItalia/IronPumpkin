//! The data map host of the server: the loader on startup and `/reload`, the negotiation and
//! the sync to `NeoForge` clients.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use indexmap::IndexMap;
use pumpkin_core::{
    plugin::data_maps::{DataMapHost, DataMapsUpdatedEvent, UpdateCause, set_host},
    server::Server,
};
use pumpkin_protocol::java::neoforge::{
    KnownDataMap, KnownRegistryDataMapsPayload, RegistryDataMapSyncPayload,
};
use pumpkin_util::identifier::Identifier;
use tracing::error;

use super::{Registered, consumers, loader, registered, registries::ServerLookup};

struct Host;

static HOST: Host = Host;

/// Installs the host of the server, once per process.
pub(super) fn install_host() {
    let _ = set_host(&HOST);
}

impl DataMapHost for Host {
    fn load(&self, server: &Arc<Server>) {
        let mut packs = vec![consumers::default_pack()];
        packs.extend(
            server
                .datapack_manager
                .get_loaded_packs()
                .into_iter()
                .map(|pack| loader::Pack {
                    id: pack.id,
                    root: Some(pack.root_path),
                    memory: Vec::new(),
                }),
        );
        let updated = loader::load(&registered(), &packs, &ServerLookup { server });
        consumers::publish();
        for registry in updated {
            let mut event = DataMapsUpdatedEvent::new(registry, UpdateCause::ServerReload);
            server.plugin_manager.fire_blocking(server, &mut event);
        }
    }

    fn known_data_maps(&self) -> KnownRegistryDataMapsPayload {
        known_data_maps(&registered())
    }

    fn mandatory_data_maps(&self) -> Vec<String> {
        mandatory_data_maps(&registered())
    }

    fn sync_payloads(&self, known: &BTreeMap<Identifier, BTreeSet<Identifier>>) -> Vec<Vec<u8>> {
        sync_payloads(&registered(), known)
    }
}

/// `RegistryDataMapNegotiation.run`: every registry with data map types and its synced types.
pub(super) fn known_data_maps(registered: &Registered) -> KnownRegistryDataMapsPayload {
    KnownRegistryDataMapsPayload {
        data_maps: registered
            .by_registry
            .iter()
            .map(|(registry, types)| {
                let maps = types
                    .iter()
                    .filter_map(|info| {
                        info.sync.map(|mandatory| KnownDataMap {
                            id: info.id.clone(),
                            mandatory,
                        })
                    })
                    .collect();
                (registry.clone(), maps)
            })
            .collect(),
    }
}

/// The types with `mandatorySync`, as `RegistryDataMapNegotiation` names them to a vanilla
/// client: `id (registry)`.
pub(super) fn mandatory_data_maps(registered: &Registered) -> Vec<String> {
    registered
        .by_registry
        .values()
        .flatten()
        .filter(|info| info.sync == Some(true))
        .map(|info| format!("{} ({})", info.id, info.registry))
        .collect()
}

/// `NeoForgeEventHandler.onDpSync`: per registry, the synced maps that the client knows, with
/// every value as the JSON of the network codec. A registry without such a map sends nothing.
pub(super) fn sync_payloads(
    registered: &Registered,
    known: &BTreeMap<Identifier, BTreeSet<Identifier>>,
) -> Vec<Vec<u8>> {
    let mut payloads = Vec::new();
    for (registry, types) in &registered.by_registry {
        let Some(client_maps) = known.get(registry).filter(|maps| !maps.is_empty()) else {
            continue;
        };
        let mut data_maps = IndexMap::new();
        for id in client_maps {
            let Some(info) = types
                .iter()
                .find(|info| &info.id == id && info.sync.is_some())
            else {
                continue;
            };
            let mut values: Vec<(Identifier, String)> = Vec::new();
            for (entry, value) in info.values().iter() {
                match info.ops.encode(value) {
                    Ok(json) => values.push((entry.clone(), json.to_string())),
                    Err(message) => {
                        error!(
                            "Could not encode the value of data map {id} for {entry}: {message}"
                        );
                    }
                }
            }
            values.sort_by(|a, b| a.0.cmp(&b.0));
            data_maps.insert(id.clone(), values.into_iter().collect());
        }
        if data_maps.is_empty() {
            continue;
        }
        let payload = RegistryDataMapSyncPayload {
            registry: registry.clone(),
            data_maps,
        };
        let mut body = Vec::new();
        match payload.write(&mut body) {
            Ok(()) => payloads.push(body),
            Err(error) => error!("Could not write the data maps of registry {registry}: {error}"),
        }
    }
    payloads
}
