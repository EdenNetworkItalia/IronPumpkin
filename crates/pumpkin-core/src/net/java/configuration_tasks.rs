//! The configuration tasks of a Java connection, like the `configurationTasks` queue of
//! `ServerConfigurationPacketListenerImpl`.
//!
//! The brand, the server links and `update_enabled_features` go out first. Then each task sends
//! its packets when it starts, and either finishes at once or waits for one client reply. The next
//! task starts only when the current one finishes. A reply that the current task does not wait for
//! disconnects the client, like `finishCurrentTask` does when the requested task is not current.

use std::collections::VecDeque;
use std::sync::OnceLock;

use pumpkin_data::dynamic::{self, ContentIds, ContentTables};
use pumpkin_protocol::{
    java::{
        client::config::CFinishConfig,
        neoforge::{
            CommonRegisterPayload, CommonVersionPayload, ConfigFilePayload, ConnectionProtocol,
            ExtensibleEnumAcknowledgePayload, ExtensibleEnumDataPayload,
            FeatureFlagAcknowledgePayload, FeatureFlagDataPayload, FrozenRegistryPayload,
            FrozenRegistrySyncCompletedPayload, FrozenRegistrySyncStartPayload,
            KnownRegistryDataMapsPayload, KnownRegistryDataMapsReplyPayload, NetworkPayloadSetup,
            PacketFlow, RegistrySnapshot, decode_payload,
        },
        server::config::ResourcePackResponseResult,
    },
    ser::{ReadingError, WritingError},
};
use pumpkin_util::{identifier::Identifier, text::TextComponent};
use tracing::{debug, error, info};

use super::{
    configuration_payloads::{
        self, SUPPORTED_COMMON_NETWORKING_VERSIONS, unsupported_common_version,
    },
    neoforge::{ClientChannels, ConnectionType},
    pending::PendingConnection,
};
use crate::{plugin::startup, server::Server};

/// The snapshot of one synced registry: every id with its namespaced name.
pub type RegistrySnapshotFn = fn(&ContentTables) -> Vec<(i32, Identifier)>;

/// The registries of the `NeoForge` registry sync.
///
/// Like `RegistryManager.getRegistryNamesForSyncToClient`. Only these three can have ids that
/// differ from the client's own ids. A client keeps its own ids for any registry that is not
/// listed.
pub const SYNCED_REGISTRIES: [(Identifier, RegistrySnapshotFn); 3] = [
    (
        Identifier::vanilla_static("block"),
        ContentTables::block_snapshot,
    ),
    (
        Identifier::vanilla_static("item"),
        ContentTables::item_snapshot,
    ),
    (
        Identifier::vanilla_static("entity_type"),
        ContentTables::entity_type_snapshot,
    ),
];

/// The channels that `ConfigurationInitialization.configureEarlyTasks` checks before it queues
/// `SyncRegistries`.
const REGISTRY_SYNC_CHANNELS: [Identifier; 3] = [
    Identifier::parse_static(FrozenRegistrySyncStartPayload::CHANNEL),
    Identifier::parse_static(FrozenRegistryPayload::CHANNEL),
    Identifier::parse_static(FrozenRegistrySyncCompletedPayload::CHANNEL),
];

/// The encoded bodies of the registry sync payloads. The frozen tables do not change, so every
/// connection gets the same bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrySyncPayloads {
    /// `neoforge:frozen_registry_sync_start` with the names of [`SYNCED_REGISTRIES`].
    pub start: Vec<u8>,
    /// One `neoforge:frozen_registry` per row of [`SYNCED_REGISTRIES`], in table order.
    pub registries: Vec<Vec<u8>>,
}

impl RegistrySyncPayloads {
    /// Encodes the snapshots of `tables`.
    ///
    /// # Errors
    ///
    /// Returns the error of a payload that cannot be written.
    pub fn encode(tables: &ContentTables) -> Result<Self, WritingError> {
        let mut start = Vec::new();
        FrozenRegistrySyncStartPayload {
            to_access: SYNCED_REGISTRIES
                .iter()
                .map(|(name, _)| name.clone())
                .collect(),
        }
        .write(&mut start)?;
        let registries = SYNCED_REGISTRIES
            .iter()
            .map(|(name, snapshot)| {
                let mut data = Vec::new();
                FrozenRegistryPayload {
                    registry_name: name.clone(),
                    snapshot: RegistrySnapshot::from_ids(snapshot(tables)),
                }
                .write(&mut data)?;
                Ok(data)
            })
            .collect::<Result<_, WritingError>>()?;
        Ok(Self { start, registries })
    }

    /// The payloads of the frozen content tables, encoded on the first call. `None` before the
    /// content freeze, or when the encoding failed.
    #[must_use]
    pub fn get() -> Option<&'static Self> {
        static PAYLOADS: OnceLock<Option<RegistrySyncPayloads>> = OnceLock::new();
        let tables = dynamic::tables()?;
        PAYLOADS
            .get_or_init(|| {
                Self::encode(tables)
                    .inspect_err(|err| error!("Failed to encode the registry sync payloads: {err}"))
                    .ok()
            })
            .as_ref()
    }
}

/// `NetworkRegistry.hasChannel` in the configuration protocol: a negotiated channel, then a
/// `c:register` channel, then an ad hoc channel.
fn has_configuration_channel(
    channels: &ClientChannels,
    payload_setup: &NetworkPayloadSetup,
    channel: &Identifier,
) -> bool {
    payload_setup
        .channels
        .get(&ConnectionProtocol::Configuration)
        .is_some_and(|negotiated| negotiated.contains_key(channel))
        || channels
            .common
            .get(&ConnectionProtocol::Configuration)
            .is_some_and(|common| common.contains(channel))
        || channels.ad_hoc.contains(channel)
}

/// A configuration task, in the order of `runConfiguration`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigurationTask {
    /// `SyncRegistries`: the snapshots of [`SYNCED_REGISTRIES`], then the wait for the client's
    /// echo of `frozen_registry_sync_completed`.
    RegistrySync,
    /// `SynchronizeRegistriesTask`: `select_known_packs`, then the registry data and the tags on
    /// the client's `select_known_packs`.
    KnownPacks,
    /// `ServerResourcePackConfigurationTask`: the resource pack push.
    ResourcePack,
    /// `CommonVersionTask`: `c:version` with the supported versions, then the wait for the
    /// client's list.
    CommonVersion,
    /// `CommonRegisterTask`: `c:register` with the optional play channels that accept
    /// serverbound payloads, then the wait for the client's channels.
    CommonRegister,
    /// `SyncConfig`: one `neoforge:config_file` per synced config, without a reply.
    SyncConfig,
    /// `RegistryDataMapNegotiation`: `neoforge:known_registry_data_maps`, then the wait for the
    /// client's data maps.
    DataMaps,
    /// `CheckExtensibleEnums`: `neoforge:extensible_enum_data`, then the wait for the ack.
    ExtensibleEnums,
    /// `CheckFeatureFlags`: `neoforge:feature_flags`, then the wait for the ack.
    FeatureFlags,
    /// `JoinWorldTask`: `finish_configuration`.
    Finish,
}

/// A client message that finishes a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskReply {
    /// `select_known_packs`.
    KnownPacks,
    /// A terminal resource pack response.
    ResourcePack,
    /// The `finish_configuration` acknowledgement.
    FinishConfiguration,
    /// A custom payload on this channel.
    Payload(&'static str),
}

impl ConfigurationTask {
    /// Every task, to find the task that waits for a custom payload channel. A new task goes here
    /// too: `every_payload_reply_resolves_through_its_channel` checks the replies of this list.
    const ALL: [Self; 10] = [
        Self::RegistrySync,
        Self::KnownPacks,
        Self::ResourcePack,
        Self::CommonVersion,
        Self::CommonRegister,
        Self::SyncConfig,
        Self::DataMaps,
        Self::ExtensibleEnums,
        Self::FeatureFlags,
        Self::Finish,
    ];

    /// The reply the task waits for, or `None` when it finishes once its packets are sent.
    #[must_use]
    pub const fn reply(self) -> Option<TaskReply> {
        match self {
            Self::RegistrySync => Some(TaskReply::Payload(
                FrozenRegistrySyncCompletedPayload::CHANNEL,
            )),
            Self::KnownPacks => Some(TaskReply::KnownPacks),
            Self::ResourcePack => Some(TaskReply::ResourcePack),
            Self::CommonVersion => Some(TaskReply::Payload(CommonVersionPayload::CHANNEL)),
            Self::CommonRegister => Some(TaskReply::Payload(CommonRegisterPayload::CHANNEL)),
            Self::SyncConfig => None,
            Self::DataMaps => Some(TaskReply::Payload(
                KnownRegistryDataMapsReplyPayload::CHANNEL,
            )),
            Self::ExtensibleEnums => Some(TaskReply::Payload(
                ExtensibleEnumAcknowledgePayload::CHANNEL,
            )),
            Self::FeatureFlags => Some(TaskReply::Payload(FeatureFlagAcknowledgePayload::CHANNEL)),
            Self::Finish => Some(TaskReply::FinishConfiguration),
        }
    }

    /// The `ConfigurationTask.Type` id.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::RegistrySync => "neoforge:sync_registries",
            Self::KnownPacks => "synchronize_registries",
            Self::ResourcePack => "server_resource_pack",
            Self::CommonVersion => "neoforge:common_version",
            Self::CommonRegister => "neoforge:common_register",
            Self::SyncConfig => "neoforge:sync_config",
            Self::DataMaps => "neoforge:registry_data_map_negotiation",
            Self::ExtensibleEnums => "neoforge:check_extensible_enum",
            Self::FeatureFlags => "neoforge:check_feature_flags",
            Self::Finish => "join_world",
        }
    }
}

impl TaskReply {
    /// The serverbound packet id of the reply, or the channel of a custom payload.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::KnownPacks => "select_known_packs",
            Self::ResourcePack => "resource_pack",
            Self::FinishConfiguration => "finish_configuration",
            Self::Payload(channel) => channel,
        }
    }

    /// The reply that a custom payload on `channel` is, when a task waits for that channel.
    #[must_use]
    pub fn for_channel(channel: &str) -> Option<Self> {
        ConfigurationTask::ALL
            .into_iter()
            .filter_map(ConfigurationTask::reply)
            .find(|reply| matches!(reply, Self::Payload(expected) if *expected == channel))
    }
}

/// `ServerboundResourcePackPacket.Action.isTerminal`.
#[must_use]
pub const fn is_terminal_resource_pack_response(result: &ResourcePackResponseResult) -> bool {
    !matches!(
        result,
        ResourcePackResponseResult::Accepted | ResourcePackResponseResult::Downloaded
    )
}

/// The queue of one connection.
#[derive(Debug, Default)]
pub struct ConfigurationTasks {
    pending: VecDeque<ConfigurationTask>,
    current: Option<ConfigurationTask>,
}

impl ConfigurationTasks {
    /// The tasks of `runConfiguration` in order, without the tasks whose condition does not hold.
    #[must_use]
    pub fn new(
        channels: &ClientChannels,
        payload_setup: &NetworkPayloadSetup,
        resource_pack: bool,
    ) -> Self {
        let mut pending = VecDeque::new();
        pending.extend(Self::early_tasks(channels, payload_setup));
        pending.push_back(ConfigurationTask::KnownPacks);
        if resource_pack {
            pending.push_back(ConfigurationTask::ResourcePack);
        }
        pending.extend(Self::modded_tasks(channels, payload_setup));
        pending.push_back(ConfigurationTask::Finish);
        Self {
            pending,
            current: None,
        }
    }

    /// `ConfigurationInitialization.configureEarlyTasks`: the tasks before the known packs.
    ///
    /// The registry sync runs only for a `NeoForge` connection that has the three sync channels.
    /// An `Other` connection that declared them as ad hoc channels gets no sync, because only a
    /// `NeoForge` client can apply the snapshots.
    fn early_tasks(
        channels: &ClientChannels,
        payload_setup: &NetworkPayloadSetup,
    ) -> Vec<ConfigurationTask> {
        let sync = channels.connection_type == ConnectionType::NeoForge
            && REGISTRY_SYNC_CHANNELS
                .iter()
                .all(|channel| has_configuration_channel(channels, payload_setup, channel));
        if sync {
            vec![ConfigurationTask::RegistrySync]
        } else {
            Vec::new()
        }
    }

    /// The `RegisterConfigurationTasksEvent` tasks after the resource pack, in the order of
    /// `ConfigurationInitialization.configureModdedClient`.
    ///
    /// A task whose wait depends on the connection type is queued only for a `NeoForge`
    /// connection. The checks of an `Other` connection (mandatory data maps, extensible enums,
    /// modded feature flags) run in `initialize_other_connection` instead.
    fn modded_tasks(
        channels: &ClientChannels,
        payload_setup: &NetworkPayloadSetup,
    ) -> Vec<ConfigurationTask> {
        let has = |channel: &'static str| {
            has_configuration_channel(channels, payload_setup, &Identifier::parse_static(channel))
        };
        let mut tasks = Vec::new();
        if has(CommonVersionPayload::CHANNEL) && has(CommonRegisterPayload::CHANNEL) {
            tasks.extend([
                ConfigurationTask::CommonVersion,
                ConfigurationTask::CommonRegister,
            ]);
        }
        if has(ConfigFilePayload::CHANNEL) {
            tasks.push(ConfigurationTask::SyncConfig);
        }
        if channels.connection_type == ConnectionType::NeoForge {
            // `RegistryDataMapNegotiation` and `CheckFeatureFlags` take the `Other` path when the
            // channel is missing, and that path finishes at once in M3.
            if has(KnownRegistryDataMapsPayload::CHANNEL) {
                tasks.push(ConfigurationTask::DataMaps);
            }
            tasks.push(ConfigurationTask::ExtensibleEnums);
            if has(FeatureFlagDataPayload::CHANNEL) {
                tasks.push(ConfigurationTask::FeatureFlags);
            }
        }
        tasks
    }

    /// `startNextTask`: takes the next task. It becomes the current task when it waits for a
    /// reply. Returns `None` while a task waits or when the queue is empty.
    pub fn start_next(&mut self) -> Option<ConfigurationTask> {
        if self.current.is_some() {
            return None;
        }
        let task = self.pending.pop_front()?;
        if task.reply().is_some() {
            self.current = Some(task);
        }
        Some(task)
    }

    #[must_use]
    pub const fn current(&self) -> Option<ConfigurationTask> {
        self.current
    }

    /// `finishCurrentTask`: finishes the current task when it waits for `reply`. Otherwise
    /// returns the current task as the error and changes nothing.
    pub fn finish(&mut self, reply: TaskReply) -> Result<(), Option<ConfigurationTask>> {
        match self.current {
            Some(task) if task.reply() == Some(reply) => {
                self.current = None;
                Ok(())
            }
            current => Err(current),
        }
    }
}

impl PendingConnection {
    /// Fills the queue and starts the first tasks. Runs after `update_enabled_features`.
    pub async fn start_configuration_tasks(&mut self, server: &Server) {
        self.configuration_tasks = ConfigurationTasks::new(
            &self.client_channels,
            &self.payload_setup,
            server.advanced_config.resource_pack.java.enabled,
        );
        self.run_configuration_tasks(server).await;
    }

    /// Starts tasks until one waits for a reply, the queue is empty or the client is gone.
    pub async fn run_configuration_tasks(&mut self, server: &Server) {
        while !self.is_closed()
            && let Some(task) = self.configuration_tasks.start_next()
        {
            debug!("Client {} starts configuration task {}", self.id, task.id());
            match task {
                ConfigurationTask::RegistrySync => self.send_registry_sync().await,
                ConfigurationTask::KnownPacks => self.send_known_packs(server).await,
                ConfigurationTask::ResourcePack => self.send_resource_pack(server).await,
                ConfigurationTask::CommonVersion => {
                    let version = CommonVersionPayload {
                        versions: SUPPORTED_COMMON_NETWORKING_VERSIONS.to_vec(),
                    };
                    self.send_neoforge_payload(CommonVersionPayload::CHANNEL, |data| {
                        version.write(data)
                    })
                    .await;
                }
                ConfigurationTask::CommonRegister => {
                    // `CommonRegisterTask` sends version 1, the only common register version.
                    let register = CommonRegisterPayload {
                        version: 1,
                        protocol: Some(ConnectionProtocol::Play),
                        channels: server
                            .network_registry
                            .optional_channels(ConnectionProtocol::Play, PacketFlow::Serverbound)
                            .await
                            .into_iter()
                            .collect(),
                    };
                    self.send_neoforge_payload(CommonRegisterPayload::CHANNEL, |data| {
                        register.write(data)
                    })
                    .await;
                }
                ConfigurationTask::SyncConfig => {
                    for file in configuration_payloads::config_files(startup::synced_configs()) {
                        self.send_neoforge_payload(ConfigFilePayload::CHANNEL, |data| {
                            file.write(data)
                        })
                        .await;
                    }
                }
                ConfigurationTask::DataMaps => {
                    let data_maps = configuration_payloads::known_registry_data_maps();
                    self.send_neoforge_payload(KnownRegistryDataMapsPayload::CHANNEL, |data| {
                        data_maps.write(data)
                    })
                    .await;
                }
                ConfigurationTask::ExtensibleEnums => {
                    let enums = configuration_payloads::extensible_enum_data();
                    self.send_neoforge_payload(ExtensibleEnumDataPayload::CHANNEL, |data| {
                        enums.write(data)
                    })
                    .await;
                }
                ConfigurationTask::FeatureFlags => {
                    self.send_neoforge_payload(FeatureFlagDataPayload::CHANNEL, |data| {
                        FeatureFlagDataPayload::default().write(data)
                    })
                    .await;
                }
                ConfigurationTask::Finish => self.send_packet_now(&CFinishConfig).await,
            }
        }
    }

    /// `SyncRegistries.run`: the start payload, one snapshot per synced registry, then the
    /// completed payload.
    async fn send_registry_sync(&mut self) {
        let Some(payloads) = RegistrySyncPayloads::get() else {
            error!(
                "Client {} needs the registry sync, but the content registry is not frozen or its \
                 payloads failed to encode",
                self.id
            );
            self.kick(TextComponent::text("The server cannot sync its registries"))
                .await;
            return;
        };
        self.send_custom_payload(FrozenRegistrySyncStartPayload::CHANNEL, &payloads.start)
            .await;
        for data in &payloads.registries {
            self.send_custom_payload(FrozenRegistryPayload::CHANNEL, data)
                .await;
        }
        self.send_custom_payload(FrozenRegistrySyncCompletedPayload::CHANNEL, &[])
            .await;
    }

    /// Logs a client that left while the current task waited for its reply. A kick logs its own
    /// reason, so a closed connection logs nothing here.
    pub fn log_left_during_task(&self) {
        let Some(task) = self.configuration_tasks.current() else {
            return;
        };
        if self.is_closed() {
            return;
        }
        let name = self
            .gameprofile
            .as_ref()
            .map_or("unknown", |profile| profile.name.as_str());
        info!(
            "Client {} ({name}) disconnected during configuration task {}",
            self.id,
            task.id()
        );
    }

    /// Finishes the current task with `reply`. Disconnects the client and returns false when the
    /// current task does not wait for `reply`.
    pub async fn finish_configuration_task(&mut self, reply: TaskReply) -> bool {
        let Err(current) = self.configuration_tasks.finish(reply) else {
            return true;
        };
        let current = current.map_or("none", ConfigurationTask::id);
        info!(
            "Client {} sent {reply:?} while the configuration task was {current}, disconnecting",
            self.id
        );
        self.kick(TextComponent::text(format!(
            "Unexpected configuration reply {} during task {current}",
            reply.name()
        )))
        .await;
        false
    }

    /// Hands the body of a custom payload reply to the task that it finished, like the payload
    /// handler of each `NeoForge` configuration task.
    ///
    /// # Errors
    ///
    /// Returns the decode error of a malformed body, which disconnects the client like a payload
    /// that `NeoForge` cannot decode.
    pub async fn handle_task_reply(
        &mut self,
        reply: TaskReply,
        data: &[u8],
    ) -> Result<(), ReadingError> {
        debug!(
            "Client {} finished a configuration task with {} ({} bytes)",
            self.id,
            reply.name(),
            data.len()
        );
        let TaskReply::Payload(channel) = reply else {
            return Ok(());
        };
        match channel {
            // The echo means that the client applied the snapshots.
            FrozenRegistrySyncCompletedPayload::CHANNEL => self.content_ids = ContentIds::Real,
            // `NetworkRegistry.checkCommonVersion`.
            CommonVersionPayload::CHANNEL => {
                let client = decode_payload(data, self.packet_joined, CommonVersionPayload::read)?;
                if !client
                    .versions
                    .iter()
                    .any(|version| SUPPORTED_COMMON_NETWORKING_VERSIONS.contains(version))
                {
                    info!(
                        "Client {} has no supported common network version: {:?}",
                        self.id, client.versions
                    );
                    self.kick(unsupported_common_version()).await;
                }
            }
            // `NetworkRegistry.onCommonRegister` replaces the channels of the protocol.
            CommonRegisterPayload::CHANNEL => {
                let client = decode_payload(data, self.packet_joined, CommonRegisterPayload::read)?;
                if let Some(protocol) = client.protocol {
                    self.client_channels
                        .common
                        .insert(protocol, client.channels);
                } else {
                    debug!(
                        "Client {} sent c:register for an unknown protocol, ignoring it",
                        self.id
                    );
                }
            }
            KnownRegistryDataMapsReplyPayload::CHANNEL => {
                let client = decode_payload(
                    data,
                    self.packet_joined,
                    KnownRegistryDataMapsReplyPayload::read,
                )?;
                self.client_channels.known_data_maps = client
                    .data_maps
                    .into_iter()
                    .map(|(registry, ids)| (registry, ids.into_iter().collect()))
                    .collect();
            }
            ExtensibleEnumAcknowledgePayload::CHANNEL => {
                decode_payload(
                    data,
                    self.packet_joined,
                    ExtensibleEnumAcknowledgePayload::read,
                )?;
            }
            FeatureFlagAcknowledgePayload::CHANNEL => {
                decode_payload(
                    data,
                    self.packet_joined,
                    FeatureFlagAcknowledgePayload::read,
                )?;
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pumpkin_protocol::java::neoforge::{ModdedNetworkQueryComponent, NetworkChannel};

    use super::*;
    use crate::net::java::neoforge::ConnectionType;

    /// The queue of a connection whose negotiated configuration channels are those of its
    /// `neoforge:register` reply, as against a server that has every channel.
    fn queue(channels: &ClientChannels, resource_pack: bool) -> ConfigurationTasks {
        let negotiated = channels
            .modded
            .iter()
            .map(|(protocol, query)| {
                let map = query
                    .iter()
                    .map(|component| {
                        let channel = NetworkChannel {
                            id: component.id.clone(),
                            chosen_version: component.version.clone(),
                        };
                        (component.id.clone(), channel)
                    })
                    .collect();
                (*protocol, map)
            })
            .collect();
        let payload_setup = NetworkPayloadSetup {
            channels: negotiated,
        };
        ConfigurationTasks::new(channels, &payload_setup, resource_pack)
    }

    /// Runs the queue to the end, answering every task with its own reply.
    fn order(mut tasks: ConfigurationTasks) -> Vec<ConfigurationTask> {
        let mut started = Vec::new();
        while let Some(task) = tasks.start_next() {
            started.push(task);
            if let Some(reply) = task.reply() {
                assert_eq!(tasks.finish(reply), Ok(()));
            }
        }
        assert_eq!(tasks.current(), None);
        started
    }

    #[test]
    fn vanilla_order() {
        let without_pack = queue(&ClientChannels::default(), false);
        assert_eq!(
            order(without_pack),
            [ConfigurationTask::KnownPacks, ConfigurationTask::Finish]
        );
        let with_pack = queue(&ClientChannels::default(), true);
        assert_eq!(
            order(with_pack),
            [
                ConfigurationTask::KnownPacks,
                ConfigurationTask::ResourcePack,
                ConfigurationTask::Finish
            ]
        );
    }

    #[test]
    fn other_connection_gets_no_registry_sync() {
        // An Other connection that declared the ad hoc channels of the registry sync.
        let channels = ClientChannels {
            connection_type: ConnectionType::Other,
            ad_hoc: [
                Identifier::parse_static("neoforge:frozen_registry_sync_start"),
                Identifier::parse_static("neoforge:frozen_registry"),
                Identifier::parse_static("neoforge:frozen_registry_sync_completed"),
            ]
            .into(),
            ..ClientChannels::default()
        };
        assert_eq!(
            order(queue(&channels, false)),
            [ConfigurationTask::KnownPacks, ConfigurationTask::Finish]
        );
    }

    const SYNC_CHANNELS: [&str; 3] = [
        FrozenRegistrySyncStartPayload::CHANNEL,
        FrozenRegistryPayload::CHANNEL,
        FrozenRegistrySyncCompletedPayload::CHANNEL,
    ];

    /// A `NeoForge` connection whose `neoforge:register` reply lists `channels` for `protocol`.
    fn neoforge(protocol: ConnectionProtocol, channels: &[&'static str]) -> ClientChannels {
        let query = channels
            .iter()
            .map(|&channel| ModdedNetworkQueryComponent {
                id: Identifier::parse_static(channel),
                version: "1".to_owned(),
                flow: None,
                optional: true,
            })
            .collect();
        ClientChannels {
            connection_type: ConnectionType::NeoForge,
            modded: BTreeMap::from([(protocol, query)]),
            ..ClientChannels::default()
        }
    }

    #[test]
    fn neoforge_connection_syncs_registries_before_the_known_packs() {
        let channels = neoforge(ConnectionProtocol::Configuration, &SYNC_CHANNELS);
        assert_eq!(
            order(queue(&channels, true)),
            [
                ConfigurationTask::RegistrySync,
                ConfigurationTask::KnownPacks,
                ConfigurationTask::ResourcePack,
                ConfigurationTask::ExtensibleEnums,
                ConfigurationTask::Finish
            ]
        );
        // `hasChannel` falls back to the ad hoc channels.
        let ad_hoc = ClientChannels {
            connection_type: ConnectionType::NeoForge,
            ad_hoc: SYNC_CHANNELS.map(Identifier::parse_static).into(),
            ..ClientChannels::default()
        };
        assert_eq!(
            order(queue(&ad_hoc, false)),
            [
                ConfigurationTask::RegistrySync,
                ConfigurationTask::KnownPacks,
                ConfigurationTask::ExtensibleEnums,
                ConfigurationTask::Finish
            ]
        );
    }

    #[test]
    fn neoforge_connection_without_every_sync_channel_gets_no_sync() {
        let without_sync = [
            ConfigurationTask::KnownPacks,
            ConfigurationTask::ExtensibleEnums,
            ConfigurationTask::Finish,
        ];
        for missing in SYNC_CHANNELS {
            let declared: Vec<&str> = SYNC_CHANNELS
                .into_iter()
                .filter(|channel| *channel != missing)
                .collect();
            let channels = neoforge(ConnectionProtocol::Configuration, &declared);
            assert_eq!(order(queue(&channels, false)), without_sync, "{missing}");
        }
        // The configuration task needs configuration channels.
        let play = neoforge(ConnectionProtocol::Play, &SYNC_CHANNELS);
        assert_eq!(order(queue(&play, false)), without_sync);
    }

    #[test]
    fn known_packs_wait_for_the_registry_sync_echo() {
        let channels = neoforge(ConnectionProtocol::Configuration, &SYNC_CHANNELS);
        let mut tasks = queue(&channels, false);
        assert_eq!(tasks.start_next(), Some(ConfigurationTask::RegistrySync));
        assert_eq!(tasks.start_next(), None);
        assert_eq!(
            tasks.finish(TaskReply::KnownPacks),
            Err(Some(ConfigurationTask::RegistrySync))
        );
        let echo = TaskReply::for_channel(FrozenRegistrySyncCompletedPayload::CHANNEL);
        assert_eq!(echo, ConfigurationTask::RegistrySync.reply());
        assert_eq!(tasks.finish(echo.unwrap()), Ok(()));
        assert_eq!(tasks.start_next(), Some(ConfigurationTask::KnownPacks));
    }

    #[test]
    fn every_payload_reply_resolves_through_its_channel() {
        for task in ConfigurationTask::ALL {
            if let Some(reply @ TaskReply::Payload(channel)) = task.reply() {
                assert_eq!(TaskReply::for_channel(channel), Some(reply), "{task:?}");
            }
        }
        assert_eq!(TaskReply::for_channel("minecraft:brand"), None);
    }

    #[test]
    fn reply_for_another_task_is_rejected() {
        let mut tasks = queue(&ClientChannels::default(), true);
        // Nothing is current before the queue starts.
        assert_eq!(tasks.finish(TaskReply::KnownPacks), Err(None));
        assert_eq!(tasks.start_next(), Some(ConfigurationTask::KnownPacks));
        // The next task waits for the reply.
        assert_eq!(tasks.start_next(), None);
        assert_eq!(tasks.current(), Some(ConfigurationTask::KnownPacks));
        let current = Err(Some(ConfigurationTask::KnownPacks));
        assert_eq!(tasks.finish(TaskReply::FinishConfiguration), current);
        assert_eq!(tasks.finish(TaskReply::ResourcePack), current);
        assert_eq!(
            tasks.finish(TaskReply::Payload(
                "neoforge:frozen_registry_sync_completed"
            )),
            current
        );
        // The rejected replies left the task current.
        assert_eq!(tasks.finish(TaskReply::KnownPacks), Ok(()));
        // A second answer to a finished task is rejected too.
        assert_eq!(tasks.start_next(), Some(ConfigurationTask::ResourcePack));
        assert_eq!(
            tasks.finish(TaskReply::KnownPacks),
            Err(Some(ConfigurationTask::ResourcePack))
        );
    }

    #[test]
    fn finish_acknowledgement_only_ends_the_finish_task() {
        let mut tasks = queue(&ClientChannels::default(), false);
        // An early acknowledgement does not skip the known packs.
        assert_eq!(tasks.start_next(), Some(ConfigurationTask::KnownPacks));
        assert_eq!(
            tasks.finish(TaskReply::FinishConfiguration),
            Err(Some(ConfigurationTask::KnownPacks))
        );
        assert_eq!(tasks.finish(TaskReply::KnownPacks), Ok(()));
        assert_eq!(tasks.start_next(), Some(ConfigurationTask::Finish));
        assert_eq!(tasks.finish(TaskReply::FinishConfiguration), Ok(()));
        assert_eq!(tasks.start_next(), None);
        assert_eq!(tasks.finish(TaskReply::FinishConfiguration), Err(None));
    }

    #[test]
    fn only_final_resource_pack_responses_finish_the_task() {
        let terminal = [
            ResourcePackResponseResult::DownloadSuccess,
            ResourcePackResponseResult::Declined,
            ResourcePackResponseResult::DownloadFail,
            ResourcePackResponseResult::InvalidUrl,
            ResourcePackResponseResult::ReloadFailed,
            ResourcePackResponseResult::Discarded,
        ];
        assert!(terminal.iter().all(is_terminal_resource_pack_response));
        assert!(!is_terminal_resource_pack_response(
            &ResourcePackResponseResult::Accepted
        ));
        assert!(!is_terminal_resource_pack_response(
            &ResourcePackResponseResult::Downloaded
        ));
    }

    /// The configuration channels of run (b) that the modded tasks check, beside the registry
    /// sync channels.
    const MODDED_CHANNELS: [&str; 3] = [
        ConfigFilePayload::CHANNEL,
        KnownRegistryDataMapsPayload::CHANNEL,
        FeatureFlagDataPayload::CHANNEL,
    ];

    /// The ad hoc channels of the client's `minecraft:register` in run (b) that the common tasks
    /// check.
    const COMMON_CHANNELS: [&str; 2] = [
        CommonVersionPayload::CHANNEL,
        CommonRegisterPayload::CHANNEL,
    ];

    /// The client of run (b): the sync and modded configuration channels, and `c:version` and
    /// `c:register` as ad hoc channels.
    fn run_b_client() -> ClientChannels {
        let declared: Vec<&str> = SYNC_CHANNELS.into_iter().chain(MODDED_CHANNELS).collect();
        ClientChannels {
            ad_hoc: COMMON_CHANNELS.map(Identifier::parse_static).into(),
            ..neoforge(ConnectionProtocol::Configuration, &declared)
        }
    }

    #[test]
    fn has_channel_reads_the_negotiated_setup_then_c_register_then_ad_hoc() {
        let config_file = Identifier::parse_static(ConfigFilePayload::CHANNEL);
        // The client's reply lists the channel, but the negotiation did not keep it.
        let replied = neoforge(
            ConnectionProtocol::Configuration,
            &[ConfigFilePayload::CHANNEL],
        );
        let empty = NetworkPayloadSetup::default();
        assert!(!has_configuration_channel(&replied, &empty, &config_file));
        let negotiated = NetworkPayloadSetup {
            channels: BTreeMap::from([(
                ConnectionProtocol::Configuration,
                BTreeMap::from([(
                    config_file.clone(),
                    NetworkChannel {
                        id: config_file.clone(),
                        chosen_version: "1".to_owned(),
                    },
                )]),
            )]),
        };
        assert!(has_configuration_channel(
            &replied,
            &negotiated,
            &config_file
        ));

        let common = ClientChannels {
            common: BTreeMap::from([(
                ConnectionProtocol::Configuration,
                [config_file.clone()].into(),
            )]),
            ..ClientChannels::default()
        };
        assert!(has_configuration_channel(&common, &empty, &config_file));
        // `c:register` channels count only for their protocol.
        let common_play = ClientChannels {
            common: BTreeMap::from([(ConnectionProtocol::Play, [config_file.clone()].into())]),
            ..ClientChannels::default()
        };
        assert!(!has_configuration_channel(
            &common_play,
            &empty,
            &config_file
        ));
        let ad_hoc = ClientChannels {
            ad_hoc: [config_file.clone()].into(),
            ..ClientChannels::default()
        };
        assert!(has_configuration_channel(&ad_hoc, &empty, &config_file));
    }

    #[test]
    fn neoforge_client_gets_the_modded_tasks_after_the_resource_pack() {
        assert_eq!(
            order(queue(&run_b_client(), true)),
            [
                ConfigurationTask::RegistrySync,
                ConfigurationTask::KnownPacks,
                ConfigurationTask::ResourcePack,
                ConfigurationTask::CommonVersion,
                ConfigurationTask::CommonRegister,
                ConfigurationTask::SyncConfig,
                ConfigurationTask::DataMaps,
                ConfigurationTask::ExtensibleEnums,
                ConfigurationTask::FeatureFlags,
                ConfigurationTask::Finish
            ]
        );
    }

    #[test]
    fn common_tasks_need_both_common_channels() {
        for declared in COMMON_CHANNELS {
            let channels = ClientChannels {
                ad_hoc: [Identifier::parse_static(declared)].into(),
                ..ClientChannels::default()
            };
            assert_eq!(
                order(queue(&channels, false)),
                [ConfigurationTask::KnownPacks, ConfigurationTask::Finish],
                "{declared}"
            );
        }
    }

    #[test]
    fn other_connection_gets_no_modded_checks() {
        // A client without `NeoForge` that declared every channel as an ad hoc channel gets the
        // tasks that only check channels, and no data map, enum or feature flag payload.
        let channels = ClientChannels {
            connection_type: ConnectionType::Other,
            ad_hoc: COMMON_CHANNELS
                .into_iter()
                .chain(MODDED_CHANNELS)
                .map(Identifier::parse_static)
                .collect(),
            ..ClientChannels::default()
        };
        assert_eq!(
            order(queue(&channels, false)),
            [
                ConfigurationTask::KnownPacks,
                ConfigurationTask::CommonVersion,
                ConfigurationTask::CommonRegister,
                ConfigurationTask::SyncConfig,
                ConfigurationTask::Finish
            ]
        );
    }

    #[test]
    fn neoforge_connection_without_the_check_channels_skips_their_tasks() {
        let channels = neoforge(ConnectionProtocol::Configuration, &[]);
        assert_eq!(
            order(queue(&channels, false)),
            [
                ConfigurationTask::KnownPacks,
                ConfigurationTask::ExtensibleEnums,
                ConfigurationTask::Finish
            ]
        );
    }

    #[test]
    fn each_modded_task_waits_for_its_reply() {
        let mut tasks = queue(&run_b_client(), false);
        for (task, reply) in [
            (
                ConfigurationTask::RegistrySync,
                FrozenRegistrySyncCompletedPayload::CHANNEL,
            ),
            (
                ConfigurationTask::CommonVersion,
                CommonVersionPayload::CHANNEL,
            ),
            (
                ConfigurationTask::CommonRegister,
                CommonRegisterPayload::CHANNEL,
            ),
            (
                ConfigurationTask::DataMaps,
                KnownRegistryDataMapsReplyPayload::CHANNEL,
            ),
            (
                ConfigurationTask::ExtensibleEnums,
                ExtensibleEnumAcknowledgePayload::CHANNEL,
            ),
            (
                ConfigurationTask::FeatureFlags,
                FeatureFlagAcknowledgePayload::CHANNEL,
            ),
        ] {
            let mut started = tasks.start_next().unwrap();
            if started == ConfigurationTask::KnownPacks {
                assert_eq!(tasks.finish(TaskReply::KnownPacks), Ok(()));
                started = tasks.start_next().unwrap();
            }
            // The config sync sends its files and finishes at once.
            if started == ConfigurationTask::SyncConfig {
                assert_eq!(tasks.current(), None);
                started = tasks.start_next().unwrap();
            }
            assert_eq!(started, task);
            assert_eq!(tasks.start_next(), None, "{task:?} does not wait");
            let reply = TaskReply::for_channel(reply).unwrap();
            assert_eq!(task.reply(), Some(reply));
            assert_eq!(tasks.finish(reply), Ok(()));
        }
        assert_eq!(tasks.start_next(), Some(ConfigurationTask::Finish));
    }

    /// A connection in the configuration state, and the client end of its socket.
    async fn configuring_connection() -> (PendingConnection, tokio::net::TcpStream) {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let client = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let (stream, address) = listener.accept().await.unwrap();
        let connection = PendingConnection::new(
            stream,
            address,
            1,
            crate::net::PacketRateLimiter::new(false, 0.0, 0.0),
            std::sync::Weak::new(),
        );
        connection
            .connection_state
            .store(pumpkin_protocol::ConnectionState::Config);
        (connection, client)
    }

    /// The reason of the configuration disconnect that the client end received.
    async fn disconnect_reason(client: tokio::net::TcpStream) -> String {
        use pumpkin_protocol::{
            java::{client::config::CConfigDisconnect, packet_decoder::TCPNetworkDecoder},
            packet::MultiVersionJavaPacket,
        };

        let mut decoder = TCPNetworkDecoder::new(tokio::io::BufReader::new(client));
        let packet = decoder.get_raw_packet().await.unwrap();
        assert_eq!(
            packet.id,
            CConfigDisconnect::to_id(pumpkin_data::packet::CURRENT_MC_VERSION)
        );
        let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(std::io::Cursor::new(
            &packet.payload[..],
        ));
        let tag = pumpkin_nbt::tag::NbtTag::deserialize(&mut reader).unwrap();
        TextComponent::from_nbt(&tag).get_text()
    }

    #[tokio::test]
    async fn common_version_reply_with_version_one_is_accepted() {
        let (mut connection, _client) = configuring_connection().await;
        // The client's `c:version` of run (b), seq 87.
        let reply = TaskReply::for_channel(CommonVersionPayload::CHANNEL).unwrap();
        connection
            .handle_task_reply(reply, &[0x01, 0x01])
            .await
            .unwrap();
        assert!(!connection.is_closed());
    }

    #[tokio::test]
    async fn common_version_reply_without_version_one_disconnects() {
        let (mut connection, client) = configuring_connection().await;
        let reply = TaskReply::for_channel(CommonVersionPayload::CHANNEL).unwrap();
        connection
            .handle_task_reply(reply, &[0x01, 0x02])
            .await
            .unwrap();
        assert!(connection.is_closed());
        assert_eq!(
            disconnect_reason(client).await,
            unsupported_common_version().get_text()
        );
    }

    #[tokio::test]
    async fn malformed_common_version_reply_is_an_error() {
        let (mut connection, _client) = configuring_connection().await;
        let reply = TaskReply::for_channel(CommonVersionPayload::CHANNEL).unwrap();
        // A list of one version with a stray byte after it.
        assert!(
            connection
                .handle_task_reply(reply, &[0x01, 0x01, 0x00])
                .await
                .is_err()
        );
    }

    /// The client's `c:register` of run (b), seq 89: its 9 optional clientbound play channels.
    const RUN_B_COMMON_REGISTER_REPLY: &str = concat!(
        "0104706c6179091c6e656f666f7267653a616476616e6365645f6164645f656e74697479246e656f666f72",
        "67653a616476616e6365645f636f6e7461696e65725f7365745f646174611d6e656f666f7267653a616476",
        "616e6365645f6f70656e5f73637265656e1d6e656f666f7267653a617578696c696172795f6c696768745f",
        "64617461146e656f666f7267653a636f6e6669675f66696c65176e656f666f7267653a7265636970655f63",
        "6f6e74656e741f6e656f666f7267653a72656769737472795f646174615f6d61705f73796e630e6e656f66",
        "6f7267653a73706c6974196e656f666f7267653a73796e635f6174746163686d656e7473",
    );

    #[tokio::test]
    async fn common_register_reply_keeps_the_client_play_channels() {
        let (mut connection, _client) = configuring_connection().await;
        let data = hex::decode(RUN_B_COMMON_REGISTER_REPLY).unwrap();
        let reply = TaskReply::for_channel(CommonRegisterPayload::CHANNEL).unwrap();
        connection.handle_task_reply(reply, &data).await.unwrap();
        let play = &connection.client_channels.common[&ConnectionProtocol::Play];
        assert_eq!(play.len(), 9);
        assert!(play.contains(&Identifier::parse_static("neoforge:split")));
        assert_eq!(connection.take_negotiated_state().common_channels.len(), 1);
    }

    #[tokio::test]
    async fn empty_data_map_reply_keeps_no_data_map() {
        let (mut connection, _client) = configuring_connection().await;
        // The client's reply of run (b), seq 92.
        let reply = TaskReply::for_channel(KnownRegistryDataMapsReplyPayload::CHANNEL).unwrap();
        connection.handle_task_reply(reply, &[0x00]).await.unwrap();
        assert!(connection.client_channels.known_data_maps.is_empty());
        assert!(!connection.is_closed());
    }

    #[tokio::test]
    async fn data_map_reply_keeps_the_client_data_maps() {
        let (mut connection, _client) = configuring_connection().await;
        let mut data = Vec::new();
        KnownRegistryDataMapsReplyPayload {
            data_maps: std::iter::once((
                Identifier::parse_static("minecraft:block"),
                vec![Identifier::parse_static("neoforge:waxables")],
            ))
            .collect(),
        }
        .write(&mut data)
        .unwrap();
        let reply = TaskReply::for_channel(KnownRegistryDataMapsReplyPayload::CHANNEL).unwrap();
        connection.handle_task_reply(reply, &data).await.unwrap();
        let state = connection.take_negotiated_state();
        assert_eq!(
            state.known_data_maps[&Identifier::parse_static("minecraft:block")],
            [Identifier::parse_static("neoforge:waxables")].into()
        );
    }
}
