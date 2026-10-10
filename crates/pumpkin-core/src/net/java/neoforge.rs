//! The `NeoForge` channel probe at the start of the configuration phase.
//!
//! A `NeoForge` server sends `minecraft:unregister`, `minecraft:register`, an empty
//! `neoforge:register` and a ping before vanilla's configuration packets
//! (`ServerConfigurationPacketListenerImpl.startConfiguration` patch). A `NeoForge` client answers
//! the query with its own `neoforge:register`, and it does so before the pong because it handles
//! packets in order. A vanilla client discards the three unknown payloads and only answers the
//! ping. The vanilla configuration starts on the pong (`handlePong` patch).

use std::collections::{BTreeMap, BTreeSet};

use pumpkin_config::BasicConfiguration;
use pumpkin_data::{dynamic::ContentIds, translation};
use pumpkin_protocol::{
    java::{
        client::config::CConfigPing,
        neoforge::{
            CommonRegisterPayload, CommonVersionPayload, ConfigFilePayload, ConnectionProtocol,
            FeatureFlagDataPayload, FrozenRegistryPayload, FrozenRegistrySyncCompletedPayload,
            FrozenRegistrySyncStartPayload, MinecraftRegisterPayload, MinecraftUnregisterPayload,
            ModdedNetworkPayload, ModdedNetworkQueryComponent, ModdedNetworkQueryPayload,
            ModdedNetworkSetupFailedPayload, NetworkChannel, NetworkPayloadSetup, PacketFlow,
            SplitPacketPayload, decode_payload,
        },
        server::config::SConfigPong,
    },
    ser::{ReadingError, WritingError},
};
use pumpkin_util::{
    identifier::{Identifier, VANILLA_NAMESPACE},
    text::TextComponent,
};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use super::pending::PendingConnection;
use crate::{
    plugin::startup::{self, NativeModInfo},
    server::Server,
};

/// The `NeoForge` version this server presents, in place of `NeoForgeVersion.getVersion()`: the
/// version of the server the captures come from.
pub const EMULATED_NEOFORGE_VERSION: &str = "26.3.0.64-beta";

/// The id of the probe ping. Its pong starts the vanilla configuration.
const PROBE_PING_ID: i32 = 0;

/// Whether connections run the `NeoForge` probe: `detect_neoforge_clients`, or a loaded
/// client-required native mod, which needs the probe to tell a vanilla client from a `NeoForge`
/// client.
#[must_use]
pub fn detects_neoforge_clients(config: &BasicConfiguration) -> bool {
    detection_on(config.detect_neoforge_clients, startup::native_mods())
}

/// Whether a connection joins the `neoforge:split` parts its client sends. The rule matches the
/// egress, which splits only for a client that declared the channel, so a client that never
/// declared it cannot make the server buffer parts.
#[must_use]
pub(crate) fn joins_split_packets(
    detection: bool,
    payload_setup: &NetworkPayloadSetup,
    ad_hoc_channels: &BTreeSet<Identifier>,
) -> bool {
    detection && SplitPacketPayload::is_declared(payload_setup, ad_hoc_channels)
}

fn detection_on(option: bool, mods: &[NativeModInfo]) -> bool {
    option || mods.iter().any(|native_mod| native_mod.client_required)
}

fn client_required_mod_ids(mods: &[NativeModInfo]) -> Vec<&str> {
    mods.iter()
        .filter(|native_mod| native_mod.client_required)
        .map(|native_mod| native_mod.id.as_str())
        .collect()
}

/// Warns at startup, after the native mods are set, when a client-required mod turns on the
/// detection that the configuration turns off.
pub(crate) fn warn_if_detection_forced(config: &BasicConfiguration) {
    if let Some(warning) =
        forced_detection_warning(config.detect_neoforge_clients, startup::native_mods())
    {
        warn!("{warning}");
    }
}

fn forced_detection_warning(option: bool, mods: &[NativeModInfo]) -> Option<String> {
    let ids = client_required_mod_ids(mods);
    (!option && !ids.is_empty()).then(|| {
        format!(
            "detect_neoforge_clients is false, but the client-required native mods {} need it: \
             NeoForge client detection is on",
            ids.join(", ")
        )
    })
}

/// The reason `NetworkRegistry.initializeOtherConnection` gives a client that cannot join a
/// `NeoForge` server. The fallback is for a client without `NeoForge`, which has no translation
/// for the key.
fn vanilla_client_not_supported() -> TextComponent {
    TextComponent::translate_with_fallback(
        "neoforge.network.negotiation.failure.vanilla.client.not_supported",
        "You are trying to connect to a server that is running NeoForge, but you are not. Please \
         install NeoForge Version: %s to connect to this server.",
        [TextComponent::text(EMULATED_NEOFORGE_VERSION)],
    )
}

/// `NetworkRegistry.BUILTIN_PAYLOADS`: the channels a `NeoForge` server listens on before negotiation.
const BUILTIN_CHANNELS: [Identifier; 7] = [
    Identifier::parse_static(MinecraftRegisterPayload::CHANNEL),
    Identifier::parse_static(MinecraftUnregisterPayload::CHANNEL),
    Identifier::parse_static(ModdedNetworkQueryPayload::CHANNEL),
    Identifier::parse_static(ModdedNetworkPayload::CHANNEL),
    Identifier::parse_static(ModdedNetworkSetupFailedPayload::CHANNEL),
    Identifier::parse_static(CommonVersionPayload::CHANNEL),
    Identifier::parse_static(CommonRegisterPayload::CHANNEL),
];

/// The fixed part of `NetworkRegistry.getInitialServerUnregisterChannels`.
const INITIAL_UNREGISTER_CHANNELS: [Identifier; 2] = [
    Identifier::parse_static(MinecraftRegisterPayload::CHANNEL),
    Identifier::parse_static(MinecraftUnregisterPayload::CHANNEL),
];

/// Upper bound for the ad-hoc channels of one client. `NeoForge` has none, but the set lives as
/// long as the connection and every `minecraft:register` adds to it.
const MAX_AD_HOC_CHANNELS: usize = 1024;

/// The keys of `NetworkRegistry.PAYLOAD_REGISTRATIONS`, in the order the negotiation visits them.
const NEGOTIATED_PROTOCOLS: [ConnectionProtocol; 2] =
    [ConnectionProtocol::Configuration, ConnectionProtocol::Play];

const CONFIGURATION: &[ConnectionProtocol] = &[ConnectionProtocol::Configuration];
const PLAY: &[ConnectionProtocol] = &[ConnectionProtocol::Play];
/// The protocols of the `PayloadRegistrar.common*` methods.
const COMMON: &[ConnectionProtocol] =
    &[ConnectionProtocol::Play, ConnectionProtocol::Configuration];

/// The registrar version of `NetworkInitialization.register` and `GenericPacketSplitter.register`.
const NEOFORGE_PAYLOAD_VERSION: &str = "1";

/// The payloads `NetworkInitialization.register` and `GenericPacketSplitter.register` register,
/// all optional, as channel, protocols and flow.
const NEOFORGE_PAYLOADS: [(&str, &[ConnectionProtocol], Option<PacketFlow>); 18] = [
    (
        ConfigFilePayload::CHANNEL,
        COMMON,
        Some(PacketFlow::Clientbound),
    ),
    (
        FrozenRegistrySyncStartPayload::CHANNEL,
        CONFIGURATION,
        Some(PacketFlow::Clientbound),
    ),
    (
        FrozenRegistryPayload::CHANNEL,
        CONFIGURATION,
        Some(PacketFlow::Clientbound),
    ),
    (
        FrozenRegistrySyncCompletedPayload::CHANNEL,
        CONFIGURATION,
        None,
    ),
    (
        "neoforge:known_registry_data_maps",
        CONFIGURATION,
        Some(PacketFlow::Clientbound),
    ),
    (
        "neoforge:extensible_enum_data",
        CONFIGURATION,
        Some(PacketFlow::Clientbound),
    ),
    (
        FeatureFlagDataPayload::CHANNEL,
        CONFIGURATION,
        Some(PacketFlow::Clientbound),
    ),
    (
        "neoforge:known_registry_data_maps_reply",
        CONFIGURATION,
        Some(PacketFlow::Serverbound),
    ),
    (
        "neoforge:extensible_enum_ack",
        CONFIGURATION,
        Some(PacketFlow::Serverbound),
    ),
    (
        "neoforge:feature_flags_ack",
        CONFIGURATION,
        Some(PacketFlow::Serverbound),
    ),
    (
        "neoforge:advanced_add_entity",
        PLAY,
        Some(PacketFlow::Clientbound),
    ),
    (
        "neoforge:advanced_open_screen",
        PLAY,
        Some(PacketFlow::Clientbound),
    ),
    (
        "neoforge:auxiliary_light_data",
        PLAY,
        Some(PacketFlow::Clientbound),
    ),
    (
        "neoforge:registry_data_map_sync",
        PLAY,
        Some(PacketFlow::Clientbound),
    ),
    (
        "neoforge:advanced_container_set_data",
        PLAY,
        Some(PacketFlow::Clientbound),
    ),
    (
        "neoforge:recipe_content",
        PLAY,
        Some(PacketFlow::Clientbound),
    ),
    (
        "neoforge:sync_attachments",
        PLAY,
        Some(PacketFlow::Clientbound),
    ),
    (SplitPacketPayload::CHANNEL, COMMON, None),
];

/// The `requestingSide` values of `NetworkComponentNegotiator.validateComponent`.
const CLIENT_SIDE: &str = "client";
const SERVER_SIDE: &str = "server";

/// Mirrors `NeoForge`'s `ConnectionType`: what is on the other side of the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionType {
    NeoForge,
    /// Vanilla or a platform other than `NeoForge`.
    #[default]
    Other,
}

/// What the client declared about its channels during the configuration phase.
#[derive(Debug, Default)]
pub struct ClientChannels {
    pub connection_type: ConnectionType,
    /// The channels of the client's `neoforge:register` reply, per protocol.
    pub modded: BTreeMap<ConnectionProtocol, BTreeSet<ModdedNetworkQueryComponent>>,
    /// The `NeoForge` ad-hoc channels: `minecraft:register` minus `minecraft:unregister`.
    pub ad_hoc: BTreeSet<Identifier>,
}

impl ClientChannels {
    /// Applies a payload on one of the channel declaration channels.
    ///
    /// Returns `Ok(false)` when `channel` is not one of them.
    /// `joined` is whether the payload came in a packet joined from `neoforge:split` parts.
    pub fn handle_payload(
        &mut self,
        channel: &str,
        data: &[u8],
        joined: bool,
    ) -> Result<bool, ReadingError> {
        match channel {
            ModdedNetworkQueryPayload::CHANNEL => {
                let query = decode_payload(data, joined, ModdedNetworkQueryPayload::read)?;
                self.connection_type = ConnectionType::NeoForge;
                self.modded = query.queries;
            }
            MinecraftRegisterPayload::CHANNEL => {
                let register = decode_payload(data, joined, MinecraftRegisterPayload::read)?;
                self.ad_hoc.extend(register.channels);
                if self.ad_hoc.len() > MAX_AD_HOC_CHANNELS {
                    return Err(ReadingError::TooLarge(format!(
                        "more than {MAX_AD_HOC_CHANNELS} registered channels"
                    )));
                }
            }
            MinecraftUnregisterPayload::CHANNEL => {
                let unregister = decode_payload(data, joined, MinecraftUnregisterPayload::read)?;
                self.ad_hoc.retain(|id| !unregister.channels.contains(id));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    #[must_use]
    pub fn modded_channel_count(&self) -> usize {
        self.modded.values().map(BTreeSet::len).sum()
    }
}

type PayloadRegistrations =
    BTreeMap<ConnectionProtocol, BTreeMap<Identifier, ModdedNetworkQueryComponent>>;

/// Mirrors `NetworkRegistry.PAYLOAD_REGISTRATIONS`: the modded channels this server speaks, per
/// protocol. It starts with the payloads `NeoForge` itself registers.
///
/// It only matters with `detect_neoforge_clients` on. A connection reads it when it sends the
/// probe (`minecraft:unregister`) and again when it negotiates: on the client's `neoforge:register`
/// reply, or on the pong for any other client.
pub struct NetworkRegistry {
    payload_registrations: RwLock<PayloadRegistrations>,
}

impl Default for NetworkRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkRegistry {
    #[must_use]
    pub fn new() -> Self {
        let mut registrations = PayloadRegistrations::new();
        for (channel, protocols, flow) in NEOFORGE_PAYLOADS {
            let id = Identifier::parse_static(channel);
            for protocol in protocols {
                registrations.entry(*protocol).or_default().insert(
                    id.clone(),
                    ModdedNetworkQueryComponent {
                        id: id.clone(),
                        version: NEOFORGE_PAYLOAD_VERSION.to_owned(),
                        flow,
                        optional: true,
                    },
                );
            }
        }
        Self {
            payload_registrations: RwLock::new(registrations),
        }
    }

    /// Mirrors `NetworkRegistry.register`, without codec and handlers: the server negotiates `id`
    /// on each of `protocols` with the clients that connect after this call. It has an effect only
    /// with `detect_neoforge_clients` on.
    ///
    /// `flow` is `None` for a bidirectional channel. A client without a channel that is not
    /// `optional` cannot join.
    pub async fn register(
        &self,
        id: Identifier,
        protocols: &[ConnectionProtocol],
        flow: Option<PacketFlow>,
        version: &str,
        optional: bool,
    ) -> Result<(), String> {
        if protocols.is_empty() {
            return Err(format!("Cannot register payload {id} with no protocols."));
        }
        if version.trim().is_empty() {
            return Err(format!(
                "Cannot register payload {id} with a blank version."
            ));
        }
        if id.namespace() == VANILLA_NAMESPACE {
            return Err(format!(
                "Cannot register payload {id} using the domain \"{VANILLA_NAMESPACE}\"."
            ));
        }
        let mut registrations = self.payload_registrations.write().await;
        for protocol in protocols {
            if !NEGOTIATED_PROTOCOLS.contains(protocol) {
                return Err(format!(
                    "Cannot register payload {id} for unsupported protocol: {protocol:?}"
                ));
            }
            if registrations
                .get(protocol)
                .is_some_and(|by_protocol| by_protocol.contains_key(&id))
            {
                return Err(format!(
                    "Cannot register payload {id} as it is already registered."
                ));
            }
        }
        for protocol in protocols {
            registrations.entry(*protocol).or_default().insert(
                id.clone(),
                ModdedNetworkQueryComponent {
                    id: id.clone(),
                    version: version.trim().to_owned(),
                    flow,
                    optional,
                },
            );
        }
        Ok(())
    }

    /// Removes `id` from every protocol, for a plugin that unloads. Returns whether it was
    /// registered. Connections that already negotiated keep it.
    pub async fn unregister(&self, id: &Identifier) -> bool {
        let mut registrations = self.payload_registrations.write().await;
        let mut removed = false;
        for by_protocol in registrations.values_mut() {
            removed |= by_protocol.remove(id).is_some();
        }
        removed
    }

    /// The registrations of `protocol`, as the server side of the negotiation.
    pub async fn registrations(
        &self,
        protocol: ConnectionProtocol,
    ) -> Vec<ModdedNetworkQueryComponent> {
        self.payload_registrations
            .read()
            .await
            .get(&protocol)
            .map(|by_protocol| by_protocol.values().cloned().collect())
            .unwrap_or_default()
    }

    /// The optional channels of `protocol` that carry payloads in `flow`, like the
    /// `matchesFlow(...)` and `optional()` filters of `NetworkRegistry`.
    pub async fn optional_channels(
        &self,
        protocol: ConnectionProtocol,
        flow: PacketFlow,
    ) -> Vec<Identifier> {
        self.registrations(protocol)
            .await
            .into_iter()
            .filter(|registration| {
                registration.optional && registration.flow.is_none_or(|f| f == flow)
            })
            .map(|registration| registration.id)
            .collect()
    }
}

/// Mirrors `NetworkComponentNegotiator.negotiate` for one protocol.
///
/// Returns the agreed channels with the server's version, or the failure reason per channel.
/// The reasons use the `NeoForge` translation keys, which a `NeoForge` client resolves.
/// `NeoForge` wraps a reason with the display name of the mod that owns the namespace; this
/// server has no mod list, so its reasons are never wrapped.
fn negotiate(
    server: &[ModdedNetworkQueryComponent],
    client: &[ModdedNetworkQueryComponent],
) -> Result<BTreeMap<Identifier, NetworkChannel>, BTreeMap<Identifier, TextComponent>> {
    let client = remove_disabled_optional_components(client, server);
    let server = remove_disabled_optional_components(server, &client);

    let missing_reasons = |side: &[ModdedNetworkQueryComponent],
                           other: &[ModdedNetworkQueryComponent],
                           key: &'static str| {
        side.iter()
            .filter(|c| !other.iter().any(|o| o.id == c.id))
            .map(|c| (c.id.clone(), TextComponent::translate(key, [])))
            .collect::<BTreeMap<_, _>>()
    };
    let missing_on_server = missing_reasons(
        &client,
        &server,
        "neoforge.network.negotiation.failure.missing.client.server",
    );
    if !missing_on_server.is_empty() {
        return Err(missing_on_server);
    }
    let missing_on_client = missing_reasons(
        &server,
        &client,
        "neoforge.network.negotiation.failure.missing.server.client",
    );
    if !missing_on_client.is_empty() {
        return Err(missing_on_client);
    }

    let mut channels = BTreeMap::new();
    let mut failure_reasons = BTreeMap::new();
    for server_component in &server {
        // Java keeps one client component per id: the last in HashSet order, here the last in set
        // order.
        let Some(client_component) = client.iter().rfind(|c| c.id == server_component.id) else {
            continue;
        };
        let failure = validate_component(server_component, client_component, CLIENT_SIDE)
            .or_else(|| validate_component(client_component, server_component, SERVER_SIDE));
        if let Some(reason) = failure {
            failure_reasons.insert(server_component.id.clone(), reason);
            continue;
        }
        // Both sides have the same version here, so the server's is as good as the client's.
        channels.insert(
            server_component.id.clone(),
            NetworkChannel {
                id: server_component.id.clone(),
                chosen_version: server_component.version.clone(),
            },
        );
    }
    if failure_reasons.is_empty() {
        Ok(channels)
    } else {
        Err(failure_reasons)
    }
}

/// `NetworkComponentNegotiator.buildDisabledOptionalComponents` and the `removeAll` after it: drops
/// the optional components of `current` that `other` lacks.
fn remove_disabled_optional_components(
    current: &[ModdedNetworkQueryComponent],
    other: &[ModdedNetworkQueryComponent],
) -> Vec<ModdedNetworkQueryComponent> {
    current
        .iter()
        .filter(|c| !c.optional || other.iter().any(|o| o.id == c.id))
        .cloned()
        .collect()
}

/// Mirrors `NetworkComponentNegotiator.validateComponent`: the failure reason when `right` is not
/// compatible with `left`.
fn validate_component(
    left: &ModdedNetworkQueryComponent,
    right: &ModdedNetworkQueryComponent,
    requesting_side: &str,
) -> Option<TextComponent> {
    if let Some(left_flow) = left.flow {
        match right.flow {
            None => {
                return Some(TextComponent::translate(
                    format!("neoforge.network.negotiation.failure.flow.{requesting_side}.missing"),
                    [TextComponent::text(flow_name(left_flow))],
                ));
            }
            Some(right_flow) if right_flow != left_flow => {
                return Some(TextComponent::translate(
                    format!("neoforge.network.negotiation.failure.flow.{requesting_side}.mismatch"),
                    [
                        TextComponent::text(flow_name(left_flow)),
                        TextComponent::text(flow_name(right_flow)),
                    ],
                ));
            }
            Some(_) => {}
        }
    }
    if left.version != right.version {
        let (client_version, server_version) = if requesting_side == CLIENT_SIDE {
            (&right.version, &left.version)
        } else {
            (&left.version, &right.version)
        };
        return Some(TextComponent::translate(
            "neoforge.network.negotiation.failure.version.mismatch",
            [
                TextComponent::text(client_version.clone()),
                TextComponent::text(server_version.clone()),
            ],
        ));
    }
    None
}

/// `PacketFlow.toString()`: the name of the Java enum constant.
const fn flow_name(flow: PacketFlow) -> &'static str {
    match flow {
        PacketFlow::Serverbound => "SERVERBOUND",
        PacketFlow::Clientbound => "CLIENTBOUND",
    }
}

/// What a connection negotiated before play, and what the player of that connection keeps.
///
/// Only a Java connection that went through the `NeoForge` probe holds more than the default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NegotiatedState {
    pub connection_type: ConnectionType,
    /// The channels negotiated with a `NeoForge` client. Empty for any other client.
    pub payload_setup: NetworkPayloadSetup,
    /// The `NeoForge` ad-hoc channels of the client: `minecraft:register` minus
    /// `minecraft:unregister`.
    pub ad_hoc_channels: BTreeSet<Identifier>,
    pub content_ids: ContentIds,
}

impl NegotiatedState {
    /// How many channels were negotiated on `protocol`.
    #[must_use]
    pub fn channel_count(&self, protocol: ConnectionProtocol) -> usize {
        self.payload_setup
            .channels
            .get(&protocol)
            .map_or(0, BTreeMap::len)
    }

    /// Logs the channels of a `NeoForge` connection when its player enters play. Any other
    /// connection logs nothing.
    pub fn log_entered_play(&self, player_name: &str) {
        if self.connection_type == ConnectionType::NeoForge {
            info!(
                "{player_name} entered play as a NeoForge client: {} configuration and {} play channels",
                self.channel_count(ConnectionProtocol::Configuration),
                self.channel_count(ConnectionProtocol::Play),
            );
        }
    }
}

/// The state of a connection that negotiated nothing, like every Bedrock connection.
pub(crate) static NOT_NEGOTIATED: NegotiatedState = NegotiatedState {
    connection_type: ConnectionType::Other,
    payload_setup: NetworkPayloadSetup {
        channels: BTreeMap::new(),
    },
    ad_hoc_channels: BTreeSet::new(),
    content_ids: ContentIds::Display,
};

impl PendingConnection {
    /// What this connection negotiated, moved out for the play phase: the connection ends right
    /// after.
    pub(super) fn take_negotiated_state(&mut self) -> NegotiatedState {
        NegotiatedState {
            connection_type: self.client_channels.connection_type,
            payload_setup: std::mem::take(&mut self.payload_setup),
            ad_hoc_channels: std::mem::take(&mut self.client_channels.ad_hoc),
            content_ids: self.content_ids,
        }
    }

    /// The `NeoForge` part of `startConfiguration`. The vanilla part waits for the pong.
    pub async fn send_neoforge_probe(&mut self, server: &Server) {
        let mut channels = BTreeSet::from(INITIAL_UNREGISTER_CHANNELS);
        channels.extend(
            server
                .network_registry
                .optional_channels(ConnectionProtocol::Play, PacketFlow::Serverbound)
                .await,
        );
        let unregister = MinecraftUnregisterPayload { channels };
        self.send_neoforge_payload(MinecraftUnregisterPayload::CHANNEL, |data| {
            unregister.write(data)
        })
        .await;
        let register = MinecraftRegisterPayload {
            channels: BUILTIN_CHANNELS.into(),
        };
        self.send_neoforge_payload(MinecraftRegisterPayload::CHANNEL, |data| {
            register.write(data)
        })
        .await;
        self.send_neoforge_payload(ModdedNetworkQueryPayload::CHANNEL, |data| {
            ModdedNetworkQueryPayload::default().write(data)
        })
        .await;
        self.send_packet_now(&CConfigPing::new(PROBE_PING_ID)).await;
        self.neoforge_probe_pending = true;
    }

    async fn send_neoforge_payload(
        &mut self,
        channel: &str,
        write: impl FnOnce(&mut Vec<u8>) -> Result<(), WritingError>,
    ) {
        let mut data = Vec::new();
        if let Err(err) = write(&mut data) {
            error!("Failed to write the {channel} payload: {err}");
            return;
        }
        self.send_custom_payload(channel, &data).await;
    }

    /// The `handleCustomPayload` patch of `ServerConfigurationPacketListenerImpl` for the channel
    /// declaration channels: the client's `neoforge:register` reply starts the negotiation.
    ///
    /// Returns `Ok(false)` when `channel` is not a channel declaration channel.
    pub async fn handle_neoforge_payload(
        &mut self,
        server: &Server,
        channel: &str,
        data: &[u8],
    ) -> Result<bool, ReadingError> {
        let is_query = channel == ModdedNetworkQueryPayload::CHANNEL;
        if is_query
            && (!self.neoforge_probe_pending
                || self.client_channels.connection_type == ConnectionType::NeoForge)
        {
            info!(
                "Client {} sent neoforge:register outside the probe, disconnecting",
                self.id
            );
            self.kick(TextComponent::text("Unexpected neoforge:register"))
                .await;
            return Ok(true);
        }
        if !self
            .client_channels
            .handle_payload(channel, data, self.packet_joined)?
        {
            return Ok(false);
        }
        if is_query {
            self.initialize_neoforge_connection(server).await;
        }
        Ok(true)
    }

    fn player_name(&self) -> &str {
        self.gameprofile
            .as_ref()
            .map_or("unknown", |profile| profile.name.as_str())
    }

    /// `NeoForge`'s `handlePong`: the probe pong initializes a client that did not answer the probe
    /// and starts the vanilla configuration.
    pub async fn handle_neoforge_probe_pong(&mut self, server: &Server, pong: &SConfigPong) {
        if pong.id != PROBE_PING_ID || !self.neoforge_probe_pending {
            return;
        }
        self.neoforge_probe_pending = false;
        info!(
            "Client {} ({}) connection type: {:?}, {} NeoForge channels declared",
            self.id,
            self.player_name(),
            self.client_channels.connection_type,
            self.client_channels.modded_channel_count()
        );
        if self.client_channels.connection_type == ConnectionType::Other
            && !self.initialize_other_connection(server).await
        {
            return;
        }
        self.run_configuration(server).await;
    }

    /// `NetworkRegistry.initializeNeoForgeConnection`. A failed negotiation disconnects the client.
    async fn initialize_neoforge_connection(&mut self, server: &Server) {
        let mut setup = NetworkPayloadSetup::default();
        for protocol in NEGOTIATED_PROTOCOLS {
            let client: Vec<_> = self
                .client_channels
                .modded
                .get(&protocol)
                .into_iter()
                .flatten()
                .cloned()
                .collect();
            let server_channels = server.network_registry.registrations(protocol).await;
            match negotiate(&server_channels, &client) {
                Ok(channels) => {
                    setup.channels.insert(protocol, channels);
                }
                Err(failure_reasons) => {
                    info!(
                        "Client {} failed the NeoForge {} channel negotiation on {:?}",
                        self.id,
                        protocol.id(),
                        failure_reasons
                            .keys()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                    );
                    let failed = ModdedNetworkSetupFailedPayload { failure_reasons };
                    self.send_neoforge_payload(ModdedNetworkSetupFailedPayload::CHANNEL, |data| {
                        failed.write(data)
                    })
                    .await;
                    self.kick(TextComponent::translate(
                        translation::java::MULTIPLAYER_DISCONNECT_INCOMPATIBLE,
                        [TextComponent::text(format!(
                            "NeoForge {EMULATED_NEOFORGE_VERSION}"
                        ))],
                    ))
                    .await;
                    return;
                }
            }
        }
        let mut channels = BTreeSet::from(BUILTIN_CHANNELS);
        channels.extend(
            setup
                .channels
                .get(&ConnectionProtocol::Configuration)
                .into_iter()
                .flat_map(BTreeMap::keys)
                .cloned(),
        );
        let network = ModdedNetworkPayload { setup };
        self.send_neoforge_payload(ModdedNetworkPayload::CHANNEL, |data| network.write(data))
            .await;
        let register = MinecraftRegisterPayload { channels };
        self.send_neoforge_payload(MinecraftRegisterPayload::CHANNEL, |data| {
            register.write(data)
        })
        .await;
        self.payload_setup = network.setup;
    }

    /// `NetworkRegistry.initializeOtherConnection`. Returns false when the client was disconnected.
    ///
    /// A client-required native mod also disconnects the client, as on a `NeoForge` server, where
    /// a client without `NeoForge` cannot have the mod.
    async fn initialize_other_connection(&mut self, server: &Server) -> bool {
        let required_mods = client_required_mod_ids(startup::native_mods());
        if !required_mods.is_empty() {
            info!(
                "Kicking {} ({}): a client without NeoForge cannot have the client-required native mods {}",
                self.id,
                self.player_name(),
                required_mods.join(", ")
            );
            self.kick(vanilla_client_not_supported()).await;
            return false;
        }
        for protocol in NEGOTIATED_PROTOCOLS {
            let server_channels = server.network_registry.registrations(protocol).await;
            if negotiate(&server_channels, &[]).is_err() {
                self.kick(vanilla_client_not_supported()).await;
                return false;
            }
        }
        let mut channels = BTreeSet::from(BUILTIN_CHANNELS);
        channels.extend(
            server
                .network_registry
                .optional_channels(ConnectionProtocol::Configuration, PacketFlow::Serverbound)
                .await,
        );
        let register = MinecraftRegisterPayload { channels };
        self.send_neoforge_payload(MinecraftRegisterPayload::CHANNEL, |data| {
            register.write(data)
        })
        .await;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_protocol::java::neoforge::PacketFlow;

    #[test]
    fn a_connection_negotiates_nothing_by_default() {
        assert_eq!(NOT_NEGOTIATED, NegotiatedState::default());
    }

    fn native_mod(id: &str, client_required: bool) -> NativeModInfo {
        NativeModInfo {
            id: id.to_owned(),
            display_name: id.to_owned(),
            version: "1.0.0".to_owned(),
            client_required,
        }
    }

    #[test]
    fn a_client_required_mod_forces_detection_on() {
        let required = [native_mod("test-mod", true)];
        let optional = [native_mod("server-only", false)];
        assert!(detection_on(false, &required));
        assert!(!detection_on(false, &optional));
        assert!(!detection_on(false, &[]));
        assert!(detection_on(true, &optional));
        assert!(detection_on(true, &[]));
    }

    #[test]
    fn split_packets_are_joined_only_with_detection_and_a_declared_channel() {
        let forced = detection_on(false, &[native_mod("test-mod", true)]);
        let split = Identifier::parse_static(SplitPacketPayload::CHANNEL);
        let declared = NetworkPayloadSetup {
            channels: BTreeMap::from([(
                ConnectionProtocol::Play,
                BTreeMap::from([(
                    split.clone(),
                    NetworkChannel {
                        id: split.clone(),
                        chosen_version: NEOFORGE_PAYLOAD_VERSION.to_owned(),
                    },
                )]),
            )]),
        };
        let undeclared = NetworkPayloadSetup::default();
        let none = BTreeSet::new();
        let ad_hoc = BTreeSet::from([split]);

        assert!(joins_split_packets(forced, &declared, &none));
        assert!(joins_split_packets(forced, &undeclared, &ad_hoc));
        assert!(!joins_split_packets(forced, &undeclared, &none));
        assert!(!joins_split_packets(false, &declared, &ad_hoc));
    }

    #[test]
    fn the_forced_detection_warning_names_the_client_required_mods() {
        let mods = [
            native_mod("server-only", false),
            native_mod("test-mod", true),
        ];
        let warning = forced_detection_warning(false, &mods).unwrap();
        assert!(warning.contains("test-mod"), "{warning}");
        assert!(!warning.contains("server-only"), "{warning}");
        assert_eq!(forced_detection_warning(true, &mods), None);
        assert_eq!(
            forced_detection_warning(false, &[native_mod("server-only", false)]),
            None
        );
    }

    #[test]
    fn the_vanilla_client_kick_carries_the_fallback_and_the_version() {
        let bytes = vanilla_client_not_supported().encode();
        let mut reader =
            pumpkin_nbt::deserializer::NbtReadHelperJava::new(std::io::Cursor::new(&bytes[..]));
        let tag = pumpkin_nbt::tag::NbtTag::deserialize(&mut reader).unwrap();
        let reason = TextComponent::from_nbt(&tag);
        let pumpkin_util::text::TextContent::Translate {
            translate,
            fallback,
            with,
            ..
        } = *reason.0.content
        else {
            panic!("not a translate component: {reason:?}");
        };
        assert_eq!(
            translate,
            "neoforge.network.negotiation.failure.vanilla.client.not_supported"
        );
        assert_eq!(
            fallback.as_deref(),
            Some(
                "You are trying to connect to a server that is running NeoForge, but you are not. \
                 Please install NeoForge Version: %s to connect to this server."
            )
        );
        assert_eq!(with.len(), 1);
        assert_eq!(
            TextComponent(with[0].clone()).get_text(),
            EMULATED_NEOFORGE_VERSION
        );
    }

    /// The server has no translation for the `NeoForge` key, so the console shows the fallback.
    #[test]
    fn the_console_renders_the_vanilla_client_kick_from_the_fallback() {
        let expected = format!(
            "You are trying to connect to a server that is running NeoForge, but you are not. \
             Please install NeoForge Version: {EMULATED_NEOFORGE_VERSION} to connect to this server."
        );
        assert_eq!(vanilla_client_not_supported().get_text(), expected);
        assert!(
            vanilla_client_not_supported()
                .to_pretty_console()
                .contains(&expected)
        );
    }

    fn component(
        id: &'static str,
        version: &str,
        flow: Option<PacketFlow>,
        optional: bool,
    ) -> ModdedNetworkQueryComponent {
        ModdedNetworkQueryComponent {
            id: Identifier::parse_static(id),
            version: version.to_owned(),
            flow,
            optional,
        }
    }

    fn translate_key(reason: &TextComponent) -> String {
        serde_json::to_value(&reason.0).unwrap()["translate"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn failure_keys(
        result: Result<BTreeMap<Identifier, NetworkChannel>, BTreeMap<Identifier, TextComponent>>,
    ) -> Vec<(String, String)> {
        result
            .unwrap_err()
            .iter()
            .map(|(id, reason)| (id.to_string(), translate_key(reason)))
            .collect()
    }

    #[test]
    fn negotiate_agrees_on_the_same_version() {
        let server = [component(
            "a:sync",
            "2",
            Some(PacketFlow::Clientbound),
            false,
        )];
        let client = [component(
            "a:sync",
            "2",
            Some(PacketFlow::Clientbound),
            false,
        )];
        let channels = negotiate(&server, &client).unwrap();
        assert_eq!(
            channels.values().collect::<Vec<_>>(),
            [&NetworkChannel {
                id: Identifier::parse_static("a:sync"),
                chosen_version: "2".to_owned(),
            }]
        );
    }

    #[test]
    fn negotiate_fails_on_a_different_version() {
        let server = [component("a:sync", "2", None, true)];
        let client = [component("a:sync", "1", None, true)];
        let reasons = negotiate(&server, &client).unwrap_err();
        let reason = serde_json::to_value(&reasons[&Identifier::parse_static("a:sync")].0).unwrap();
        assert_eq!(
            reason["translate"],
            "neoforge.network.negotiation.failure.version.mismatch"
        );
        // validateComponent(server, client, "client") passes the client version first.
        assert_eq!(reason["with"][0]["text"], "1");
        assert_eq!(reason["with"][1]["text"], "2");
    }

    #[test]
    fn negotiate_drops_optional_channels_the_other_side_lacks() {
        let server = [
            component("a:shared", "1", None, false),
            component("a:server_only", "1", None, true),
        ];
        let client = [
            component("a:shared", "1", None, false),
            component("probe:fake", "1", Some(PacketFlow::Clientbound), true),
        ];
        let channels = negotiate(&server, &client).unwrap();
        assert_eq!(
            channels.keys().collect::<Vec<_>>(),
            [&Identifier::parse_static("a:shared")]
        );
    }

    #[test]
    fn negotiate_fails_on_a_missing_required_channel() {
        let server = [component("a:shared", "1", None, true)];
        let client = [component("b:required", "1", None, false)];
        assert_eq!(
            failure_keys(negotiate(&server, &client)),
            [(
                "b:required".to_owned(),
                "neoforge.network.negotiation.failure.missing.client.server".to_owned()
            )]
        );
        assert_eq!(
            failure_keys(negotiate(&client, &server)),
            [(
                "b:required".to_owned(),
                "neoforge.network.negotiation.failure.missing.server.client".to_owned()
            )]
        );
        // A vanilla client declares nothing, so only required server channels fail.
        assert!(negotiate(&server, &[]).unwrap().is_empty());
        assert!(negotiate(&client, &[]).is_err());
    }

    #[test]
    fn negotiate_reports_missing_server_channels_first() {
        // Both sides have a required channel the other lacks: Java returns the client's first.
        let server = [component("a:server_only", "1", None, false)];
        let client = [component("b:client_only", "1", None, false)];
        assert_eq!(
            failure_keys(negotiate(&server, &client)),
            [(
                "b:client_only".to_owned(),
                "neoforge.network.negotiation.failure.missing.client.server".to_owned()
            )]
        );
    }

    #[test]
    fn negotiate_uses_one_client_component_per_id() {
        // A client set can hold one id twice with different versions; the last in set order wins.
        let server = [component("a:sync", "2", None, false)];
        let client = [
            component("a:sync", "1", None, false),
            component("a:sync", "2", None, false),
        ];
        let channels = negotiate(&server, &client).unwrap();
        assert_eq!(
            channels[&Identifier::parse_static("a:sync")].chosen_version,
            "2"
        );
        let server = [component("a:sync", "1", None, false)];
        assert_eq!(
            failure_keys(negotiate(&server, &client)),
            [(
                "a:sync".to_owned(),
                "neoforge.network.negotiation.failure.version.mismatch".to_owned()
            )]
        );
    }

    #[tokio::test]
    async fn unregister_removes_a_channel_from_every_protocol() {
        let registry = NetworkRegistry::new();
        let id = Identifier::parse_static("a:sync");
        let both = [ConnectionProtocol::Configuration, ConnectionProtocol::Play];
        assert!(
            registry
                .register(id.clone(), &both, None, "1", false)
                .await
                .is_ok()
        );
        assert!(registry.unregister(&id).await);
        for protocol in both {
            assert!(
                !registry
                    .registrations(protocol)
                    .await
                    .iter()
                    .any(|r| r.id == id)
            );
        }
        assert!(!registry.unregister(&id).await);
        // A plugin that loads again can register the channel again.
        assert!(registry.register(id, &both, None, "1", false).await.is_ok());
    }

    #[test]
    fn negotiate_checks_the_flow() {
        let clientbound = component("a:sync", "1", Some(PacketFlow::Clientbound), false);
        let serverbound = component("a:sync", "1", Some(PacketFlow::Serverbound), false);
        let bidirectional = component("a:sync", "1", None, false);
        assert_eq!(
            failure_keys(negotiate(
                std::slice::from_ref(&clientbound),
                std::slice::from_ref(&serverbound)
            )),
            [(
                "a:sync".to_owned(),
                "neoforge.network.negotiation.failure.flow.client.mismatch".to_owned()
            )]
        );
        assert_eq!(
            failure_keys(negotiate(
                std::slice::from_ref(&clientbound),
                std::slice::from_ref(&bidirectional)
            )),
            [(
                "a:sync".to_owned(),
                "neoforge.network.negotiation.failure.flow.client.missing".to_owned()
            )]
        );
        assert_eq!(
            failure_keys(negotiate(
                std::slice::from_ref(&bidirectional),
                std::slice::from_ref(&clientbound)
            )),
            [(
                "a:sync".to_owned(),
                "neoforge.network.negotiation.failure.flow.server.missing".to_owned()
            )]
        );
    }

    #[tokio::test]
    async fn register_refuses_what_neoforge_refuses() {
        let registry = NetworkRegistry::new();
        let id = Identifier::parse_static("a:sync");
        let configuration = [ConnectionProtocol::Configuration];
        assert!(
            registry
                .register(id.clone(), &configuration, None, "1", false)
                .await
                .is_ok()
        );
        assert!(
            registry
                .register(id.clone(), &configuration, None, "1", false)
                .await
                .is_err()
        );
        assert!(
            registry
                .register(
                    Identifier::parse_static("minecraft:sync"),
                    &configuration,
                    None,
                    "1",
                    false
                )
                .await
                .is_err()
        );
        assert!(
            registry
                .register(
                    Identifier::parse_static("a:blank"),
                    &configuration,
                    None,
                    " ",
                    false
                )
                .await
                .is_err()
        );
        // A protocol NeoForge has no payloads for fails the whole call.
        let id = Identifier::parse_static("a:login");
        assert!(
            registry
                .register(
                    id.clone(),
                    &[ConnectionProtocol::Play, ConnectionProtocol::Login],
                    None,
                    "1",
                    false
                )
                .await
                .is_err()
        );
        assert!(
            !registry
                .registrations(ConnectionProtocol::Play)
                .await
                .iter()
                .any(|r| r.id == id)
        );
        let sync = registry
            .registrations(ConnectionProtocol::Configuration)
            .await
            .into_iter()
            .find(|r| r.id == Identifier::parse_static("a:sync"));
        assert!(sync.is_some_and(|r| !r.optional));
    }

    #[test]
    fn query_reply_records_neoforge_and_channels() {
        // A reply with one configuration channel: map size 1, CONFIGURATION ordinal 4, set size 1,
        // "testmod:hello", version "1", flow CLIENTBOUND, not optional.
        let bytes = [
            &[0x01, 0x04, 0x01, 0x0d][..],
            b"testmod:hello",
            &[0x01, b'1', 0x01, 0x01, 0x00],
        ]
        .concat();
        let mut channels = ClientChannels::default();
        assert!(
            channels
                .handle_payload(ModdedNetworkQueryPayload::CHANNEL, &bytes, false)
                .unwrap()
        );
        assert_eq!(channels.connection_type, ConnectionType::NeoForge);
        assert_eq!(channels.modded_channel_count(), 1);
        let component = channels.modded[&ConnectionProtocol::Configuration]
            .first()
            .unwrap();
        assert_eq!(component.id, Identifier::parse_static("testmod:hello"));
        assert_eq!(component.flow, Some(PacketFlow::Clientbound));
    }

    #[test]
    fn query_reply_with_trailing_bytes_is_rejected() {
        let mut channels = ClientChannels::default();
        assert!(
            channels
                .handle_payload(ModdedNetworkQueryPayload::CHANNEL, &[0x00, 0x00], false)
                .is_err()
        );
        assert_eq!(channels.connection_type, ConnectionType::Other);
    }

    #[test]
    fn unregister_removes_ad_hoc_channels() {
        let mut channels = ClientChannels::default();
        assert!(
            channels
                .handle_payload(MinecraftRegisterPayload::CHANNEL, b"a:one\0a:two\0", false)
                .unwrap()
        );
        assert!(
            channels
                .handle_payload(MinecraftUnregisterPayload::CHANNEL, b"a:one\0", false)
                .unwrap()
        );
        assert_eq!(
            channels.ad_hoc,
            BTreeSet::from([Identifier::parse_static("a:two")])
        );
        assert_eq!(channels.connection_type, ConnectionType::Other);
    }

    #[test]
    fn ad_hoc_channels_are_capped() {
        let register = |range: std::ops::Range<usize>| {
            range
                .flat_map(|i| format!("a:c{i}\0").into_bytes())
                .collect::<Vec<u8>>()
        };
        let mut channels = ClientChannels::default();
        let full = register(0..MAX_AD_HOC_CHANNELS);
        assert!(
            channels
                .handle_payload(MinecraftRegisterPayload::CHANNEL, &full, false)
                .is_ok()
        );
        // Registering a known channel again does not count.
        assert!(
            channels
                .handle_payload(MinecraftRegisterPayload::CHANNEL, b"a:c0\0", false)
                .is_ok()
        );
        let one_more = register(MAX_AD_HOC_CHANNELS..MAX_AD_HOC_CHANNELS + 1);
        assert!(
            channels
                .handle_payload(MinecraftRegisterPayload::CHANNEL, &one_more, false)
                .is_err()
        );
    }
}
