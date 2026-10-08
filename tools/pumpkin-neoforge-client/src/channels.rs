//! The channels the client claims to have, read from a TOML or JSON file.
//!
//! The file maps a connection protocol id (`configuration`, `play`) to a list of channels, each
//! with the fields of `NeoForge`'s `ModdedNetworkQueryComponent`:
//!
//! ```toml
//! [[configuration]]
//! id = "mymod:sync"
//! version = "1"
//! flow = "clientbound" # or "serverbound"; leave it out for a bidirectional channel
//! optional = true
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use pumpkin_protocol::java::neoforge::{
    CommonRegisterPayload, CommonVersionPayload, ConnectionProtocol, MinecraftRegisterPayload,
    MinecraftUnregisterPayload, ModdedNetworkPayload, ModdedNetworkQueryComponent,
    ModdedNetworkQueryPayload, ModdedNetworkSetupFailedPayload, PacketFlow,
};
use pumpkin_util::identifier::Identifier;
use serde::Deserialize;

use crate::Error;

/// `NetworkRegistry.BUILTIN_PAYLOADS`: the channels a `NeoForge` side always listens on before
/// the negotiation.
const BUILTIN_CHANNELS: [Identifier; 7] = [
    Identifier::parse_static(MinecraftRegisterPayload::CHANNEL),
    Identifier::parse_static(MinecraftUnregisterPayload::CHANNEL),
    Identifier::parse_static(ModdedNetworkQueryPayload::CHANNEL),
    Identifier::parse_static(ModdedNetworkPayload::CHANNEL),
    Identifier::parse_static(ModdedNetworkSetupFailedPayload::CHANNEL),
    Identifier::parse_static(CommonVersionPayload::CHANNEL),
    Identifier::parse_static(CommonRegisterPayload::CHANNEL),
];

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Flow {
    Clientbound,
    Serverbound,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelEntry {
    id: String,
    #[serde(default)]
    version: String,
    flow: Option<Flow>,
    #[serde(default)]
    optional: bool,
}

/// The channel map of the emulated `NeoForge` client.
#[derive(Debug, Clone, Default)]
pub struct ChannelMap {
    query: ModdedNetworkQueryPayload,
}

impl ChannelMap {
    /// Reads a channel map. Files ending in `.json` are JSON, everything else is TOML.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let raw: BTreeMap<String, Vec<ChannelEntry>> =
            if path.extension().is_some_and(|ext| ext == "json") {
                serde_json::from_str(&text)?
            } else {
                toml::from_str(&text)?
            };
        Self::from_entries(raw)
    }

    fn from_entries(raw: BTreeMap<String, Vec<ChannelEntry>>) -> Result<Self, Error> {
        let mut queries = BTreeMap::new();
        for (protocol_id, entries) in raw {
            let protocol = ConnectionProtocol::from_id(&protocol_id)
                .ok_or_else(|| format!("unknown connection protocol {protocol_id:?}"))?;
            let mut components = BTreeSet::new();
            for entry in entries {
                components.insert(ModdedNetworkQueryComponent {
                    id: Identifier::parse(&entry.id)
                        .map_err(|e| format!("bad channel id {:?}: {e}", entry.id))?,
                    version: entry.version,
                    flow: entry.flow.map(|flow| match flow {
                        Flow::Clientbound => PacketFlow::Clientbound,
                        Flow::Serverbound => PacketFlow::Serverbound,
                    }),
                    optional: entry.optional,
                });
            }
            queries.insert(protocol, components);
        }
        Ok(Self {
            query: ModdedNetworkQueryPayload { queries },
        })
    }

    /// The reply to the server's `neoforge:register` probe (`ModdedNetworkQueryPayload.fromRegistry`).
    #[must_use]
    pub const fn query(&self) -> &ModdedNetworkQueryPayload {
        &self.query
    }

    /// Optional channels of `protocol` that the client can receive, like `PayloadRegistration`
    /// filtered by `matchesFlow(CLIENTBOUND)` and `optional()`.
    fn optional_clientbound(
        &self,
        protocol: ConnectionProtocol,
    ) -> impl Iterator<Item = Identifier> + '_ {
        self.query
            .queries
            .get(&protocol)
            .into_iter()
            .flatten()
            .filter(|c| c.optional && c.flow != Some(PacketFlow::Serverbound))
            .map(|c| c.id.clone())
    }

    /// `ClientNetworkRegistry.sendInitialListeningChannels`: the builtin channels plus the optional
    /// clientbound configuration channels.
    #[must_use]
    pub fn initial_listening_channels(&self) -> BTreeSet<Identifier> {
        Self::builtin_channels()
            .into_iter()
            .chain(self.optional_clientbound(ConnectionProtocol::Configuration))
            .collect()
    }

    /// `NetworkRegistry.getCommonPlayChannels(CLIENTBOUND)`, sent back in `c:register`.
    #[must_use]
    pub fn common_play_channels(&self) -> BTreeSet<Identifier> {
        self.optional_clientbound(ConnectionProtocol::Play)
            .collect()
    }

    /// Channels a non-`NeoForge` server can never negotiate, which make the real client disconnect
    /// in `ClientNetworkRegistry.configureOtherConnection`.
    #[must_use]
    pub fn required_channels(&self) -> Vec<String> {
        self.query
            .queries
            .values()
            .flatten()
            .filter(|c| !c.optional)
            .map(|c| c.id.to_string())
            .collect()
    }

    /// The client's registration of a configuration channel, if it has one.
    #[must_use]
    pub fn configuration_registration(
        &self,
        id: &Identifier,
    ) -> Option<&ModdedNetworkQueryComponent> {
        self.query
            .queries
            .get(&ConnectionProtocol::Configuration)?
            .iter()
            .find(|c| &c.id == id)
    }

    /// `NetworkRegistry.getInitialListeningChannels`, without any configuration channel.
    #[must_use]
    pub fn builtin_channels() -> BTreeSet<Identifier> {
        BUILTIN_CHANNELS.into_iter().collect()
    }

    /// Whether `id` is one of `NetworkRegistry.BUILTIN_PAYLOADS`.
    #[must_use]
    pub fn is_builtin(id: &Identifier) -> bool {
        BUILTIN_CHANNELS.contains(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listening_channels_follow_flow_and_optional() {
        let raw: BTreeMap<String, Vec<ChannelEntry>> = toml::from_str(
            r#"
            [[configuration]]
            id = "a:to_client"
            version = "1"
            flow = "clientbound"
            optional = true

            [[configuration]]
            id = "a:to_server"
            flow = "serverbound"
            optional = true

            [[configuration]]
            id = "a:both"
            optional = true

            [[configuration]]
            id = "a:required"
            flow = "clientbound"

            [[play]]
            id = "a:play"
            optional = true
            "#,
        )
        .unwrap();
        let map = ChannelMap::from_entries(raw).unwrap();
        let listening = map.initial_listening_channels();
        let has = |id| listening.contains(&Identifier::parse_static(id));
        assert!(has("a:to_client"));
        assert!(has("a:both"));
        assert!(!has("a:to_server"));
        assert!(!has("a:required"));
        assert!(has("c:version"));
        assert_eq!(
            map.common_play_channels(),
            BTreeSet::from([Identifier::parse_static("a:play")])
        );
        assert_eq!(map.required_channels(), vec!["a:required".to_owned()]);
    }
}
