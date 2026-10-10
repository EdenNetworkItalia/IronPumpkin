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
            ConnectionProtocol, FrozenRegistryPayload, FrozenRegistrySyncCompletedPayload,
            FrozenRegistrySyncStartPayload, RegistrySnapshot,
        },
        server::config::ResourcePackResponseResult,
    },
    ser::WritingError,
};
use pumpkin_util::{identifier::Identifier, text::TextComponent};
use tracing::{debug, error, info};

use super::{
    neoforge::{ClientChannels, ConnectionType},
    pending::PendingConnection,
};
use crate::server::Server;

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

/// `NetworkRegistry.hasChannel` in the configuration protocol: a channel of the client's
/// `neoforge:register` reply, or an ad hoc channel.
fn has_configuration_channel(channels: &ClientChannels, channel: &Identifier) -> bool {
    channels
        .modded
        .get(&ConnectionProtocol::Configuration)
        .is_some_and(|query| query.iter().any(|component| component.id == *channel))
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
    const ALL: [Self; 4] = [
        Self::RegistrySync,
        Self::KnownPacks,
        Self::ResourcePack,
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
    pub fn new(channels: &ClientChannels, resource_pack: bool) -> Self {
        let mut pending = VecDeque::new();
        pending.extend(Self::early_tasks(channels));
        pending.push_back(ConfigurationTask::KnownPacks);
        if resource_pack {
            pending.push_back(ConfigurationTask::ResourcePack);
        }
        pending.extend(Self::modded_tasks(channels));
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
    fn early_tasks(channels: &ClientChannels) -> Vec<ConfigurationTask> {
        let sync = channels.connection_type == ConnectionType::NeoForge
            && REGISTRY_SYNC_CHANNELS
                .iter()
                .all(|channel| has_configuration_channel(channels, channel));
        if sync {
            vec![ConfigurationTask::RegistrySync]
        } else {
            Vec::new()
        }
    }

    /// The `RegisterConfigurationTasksEvent` tasks, after the resource pack. The common version,
    /// common register, config, data map, enum and feature flag tasks belong here.
    ///
    /// A task whose wait depends on the connection type is queued only for a `NeoForge`
    /// connection. The checks of an `Other` connection (mandatory data maps, extensible enums,
    /// modded feature flags) run in `initialize_other_connection` instead.
    const fn modded_tasks(_channels: &ClientChannels) -> Vec<ConfigurationTask> {
        Vec::new()
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
    /// handler of each `NeoForge` configuration task. The common version, common register and
    /// data map tasks add their arms here.
    pub fn handle_task_reply(&mut self, reply: TaskReply, data: &[u8]) {
        debug!(
            "Client {} finished a configuration task with {} ({} bytes)",
            self.id,
            reply.name(),
            data.len()
        );
        // The echo means that the client applied the snapshots.
        if Some(reply) == ConfigurationTask::RegistrySync.reply() {
            self.content_ids = ContentIds::Real;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pumpkin_protocol::java::neoforge::ModdedNetworkQueryComponent;

    use super::*;
    use crate::net::java::neoforge::ConnectionType;

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
        let without_pack = ConfigurationTasks::new(&ClientChannels::default(), false);
        assert_eq!(
            order(without_pack),
            [ConfigurationTask::KnownPacks, ConfigurationTask::Finish]
        );
        let with_pack = ConfigurationTasks::new(&ClientChannels::default(), true);
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
            order(ConfigurationTasks::new(&channels, false)),
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
            order(ConfigurationTasks::new(&channels, true)),
            [
                ConfigurationTask::RegistrySync,
                ConfigurationTask::KnownPacks,
                ConfigurationTask::ResourcePack,
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
            order(ConfigurationTasks::new(&ad_hoc, false)),
            [
                ConfigurationTask::RegistrySync,
                ConfigurationTask::KnownPacks,
                ConfigurationTask::Finish
            ]
        );
    }

    #[test]
    fn neoforge_connection_without_every_sync_channel_gets_no_sync() {
        let without_sync = [ConfigurationTask::KnownPacks, ConfigurationTask::Finish];
        for missing in SYNC_CHANNELS {
            let declared: Vec<&str> = SYNC_CHANNELS
                .into_iter()
                .filter(|channel| *channel != missing)
                .collect();
            let channels = neoforge(ConnectionProtocol::Configuration, &declared);
            assert_eq!(
                order(ConfigurationTasks::new(&channels, false)),
                without_sync,
                "{missing}"
            );
        }
        // The configuration task needs configuration channels.
        let play = neoforge(ConnectionProtocol::Play, &SYNC_CHANNELS);
        assert_eq!(order(ConfigurationTasks::new(&play, false)), without_sync);
    }

    #[test]
    fn known_packs_wait_for_the_registry_sync_echo() {
        let channels = neoforge(ConnectionProtocol::Configuration, &SYNC_CHANNELS);
        let mut tasks = ConfigurationTasks::new(&channels, false);
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
        let mut tasks = ConfigurationTasks::new(&ClientChannels::default(), true);
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
        let mut tasks = ConfigurationTasks::new(&ClientChannels::default(), false);
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
}
