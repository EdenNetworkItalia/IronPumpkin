//! The configuration tasks of a Java connection, like the `configurationTasks` queue of
//! `ServerConfigurationPacketListenerImpl`.
//!
//! The brand, the server links and `update_enabled_features` go out first. Then each task sends
//! its packets when it starts, and either finishes at once or waits for one client reply. The next
//! task starts only when the current one finishes. A reply that the current task does not wait for
//! disconnects the client, like `finishCurrentTask` does when the requested task is not current.

use std::collections::VecDeque;

use pumpkin_protocol::java::{
    client::config::CFinishConfig, server::config::ResourcePackResponseResult,
};
use pumpkin_util::text::TextComponent;
use tracing::{debug, info};

use super::{neoforge::ClientChannels, pending::PendingConnection};
use crate::server::Server;

/// A configuration task, in the order of `runConfiguration`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigurationTask {
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
    const ALL: [Self; 3] = [Self::KnownPacks, Self::ResourcePack, Self::Finish];

    /// The reply the task waits for, or `None` when it finishes once its packets are sent.
    #[must_use]
    pub const fn reply(self) -> Option<TaskReply> {
        match self {
            Self::KnownPacks => Some(TaskReply::KnownPacks),
            Self::ResourcePack => Some(TaskReply::ResourcePack),
            Self::Finish => Some(TaskReply::FinishConfiguration),
        }
    }

    /// The `ConfigurationTask.Type` id.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
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

    /// `ConfigurationInitialization.configureEarlyTasks`: the tasks before the known packs. The
    /// registry sync task belongs here.
    const fn early_tasks(_channels: &ClientChannels) -> Vec<ConfigurationTask> {
        Vec::new()
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
                ConfigurationTask::KnownPacks => self.send_known_packs(server).await,
                ConfigurationTask::ResourcePack => self.send_resource_pack(server).await,
                ConfigurationTask::Finish => self.send_packet_now(&CFinishConfig).await,
            }
        }
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
    /// handler of each `NeoForge` configuration task. No task waits for a custom payload yet: the
    /// registry sync, common version, common register and data map tasks add their arms here.
    pub fn handle_task_reply(&mut self, reply: TaskReply, data: &[u8]) {
        debug!(
            "Client {} finished a configuration task with {} ({} bytes)",
            self.id,
            reply.name(),
            data.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_util::identifier::Identifier;

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
