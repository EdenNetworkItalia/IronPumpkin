//! The network events of `NeoForge` and the types that a payload handler and a configuration task
//! work with.
//!
//! A mod registers its channels in a handler of [`RegisterPayloadHandlersEvent`], which fires once
//! at startup, and its configuration tasks in a handler of [`RegisterConfigurationTasksEvent`],
//! which fires once per Java connection. A payload is raw bytes: the mod decodes and encodes the
//! body itself.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::{Arc, Mutex, PoisonError},
};

use pumpkin_macros::Event;
use pumpkin_protocol::java::neoforge::NetworkPayloadSetup;
use pumpkin_util::{
    identifier::{Identifier, VANILLA_NAMESPACE},
    text::TextComponent,
};
use uuid::Uuid;

pub use crate::net::java::neoforge::ConnectionType;
use crate::{entity::player::Player, net::java::neoforge::is_builtin_channel};
pub use pumpkin_protocol::java::neoforge::{ConnectionProtocol, PacketFlow};

/// The thread a payload handler runs on.
///
/// `NeoForge`: `net.neoforged.neoforge.network.registration.HandlerThread`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HandlerThread {
    /// The world tick of the sender in the play phase, where the server handles the other play
    /// packets of the player. The configuration phase has no world tick, so a handler of a
    /// configuration payload runs on the connection task.
    #[default]
    Main,
    /// The connection task of the sender, concurrently with the world tick.
    Network,
}

/// Why a network call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkError {
    /// The channel is already registered for one of the protocols of the call.
    Duplicate(String),
    /// The registration step is over: `RegisterPayloadHandlersEvent` fires once at startup.
    RegistrationClosed,
    /// The channel is not a valid identifier.
    InvalidChannel(String),
    /// The channel uses the `minecraft` namespace, which `NeoForge` reserves for vanilla.
    ReservedNamespace(String),
    /// The registrar version is blank.
    BlankVersion(String),
    /// The client of the connection did not negotiate the channel, so it cannot receive the
    /// payload.
    ChannelNotNegotiated(String),
    /// A configuration task can finish only during the configuration phase.
    NotConfiguration,
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate(id) => write!(f, "payload {id} is already registered"),
            Self::RegistrationClosed => f.write_str("payload registration is closed"),
            Self::InvalidChannel(id) => write!(f, "{id} is not a valid channel"),
            Self::ReservedNamespace(id) => {
                write!(f, "payload {id} uses the domain \"{VANILLA_NAMESPACE}\"")
            }
            Self::BlankVersion(id) => write!(f, "payload {id} has a blank version"),
            Self::ChannelNotNegotiated(id) => {
                write!(f, "payload {id} may not be sent to the client")
            }
            Self::NotConfiguration => {
                f.write_str("a configuration task can finish only in the configuration phase")
            }
        }
    }
}

impl std::error::Error for NetworkError {}

/// The handler of a channel: the payload bytes and the context of the payload.
///
/// `NeoForge`: `IPayloadHandler`.
pub type PayloadHandler = Arc<dyn Fn(&[u8], &mut PayloadContext<'_>) + Send + Sync>;

/// One channel that a mod registered.
pub(crate) struct PayloadRegistration {
    pub(crate) id: Identifier,
    pub(crate) protocols: &'static [ConnectionProtocol],
    pub(crate) flow: Option<PacketFlow>,
    pub(crate) version: String,
    pub(crate) optional: bool,
    pub(crate) thread: HandlerThread,
    pub(crate) handler: Option<PayloadHandler>,
}

#[derive(Default)]
pub(crate) struct Registrations {
    /// The channels of each protocol that are registered: `NeoForge`'s own and the mods'.
    taken: BTreeSet<(ConnectionProtocol, Identifier)>,
    pub(crate) entries: Vec<PayloadRegistration>,
    closed: bool,
}

const CONFIGURATION: &[ConnectionProtocol] = &[ConnectionProtocol::Configuration];
const PLAY: &[ConnectionProtocol] = &[ConnectionProtocol::Play];
const COMMON: &[ConnectionProtocol] =
    &[ConnectionProtocol::Play, ConnectionProtocol::Configuration];

/// Fires once at startup, after `load_complete` of every mod and before the synced configs load.
///
/// A handler registers the channels of its mod with [`RegisterPayloadHandlersEvent::registrar`].
/// The registration closes when the event returns: a later call on a registrar fails with
/// [`NetworkError::RegistrationClosed`].
///
/// `NeoForge`: `net.neoforged.neoforge.network.event.RegisterPayloadHandlersEvent`, a mod bus
/// event that `NetworkRegistry.setup` posts once.
#[derive(Event)]
pub struct RegisterPayloadHandlersEvent {
    registrations: Arc<Mutex<Registrations>>,
}

impl RegisterPayloadHandlersEvent {
    /// An event whose registrars refuse the channels of `taken` as duplicates.
    pub(crate) fn new(taken: BTreeSet<(ConnectionProtocol, Identifier)>) -> Self {
        Self {
            registrations: Arc::new(Mutex::new(Registrations {
                taken,
                ..Registrations::default()
            })),
        }
    }

    /// A registrar for channels of `version`, required, with handlers on [`HandlerThread::Main`].
    ///
    /// `NeoForge`: `RegisterPayloadHandlersEvent#registrar`.
    #[must_use]
    pub fn registrar(&self, version: &str) -> PayloadRegistrar {
        PayloadRegistrar {
            registrations: Arc::clone(&self.registrations),
            version: version.to_owned(),
            optional: false,
            thread: HandlerThread::Main,
        }
    }

    /// Closes the registration and returns the channels in registration order.
    pub(crate) fn close(&self) -> Vec<PayloadRegistration> {
        let mut registrations = self
            .registrations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        registrations.closed = true;
        std::mem::take(&mut registrations.entries)
    }
}

/// Registers channels with one version, one optional flag and one handler thread. The methods
/// that change them return a copy, as in `NeoForge`.
///
/// A server-bound or bidirectional channel takes the handler that runs for each payload the
/// client sends on it. A client-bound channel has no server handler.
///
/// `NeoForge`: `net.neoforged.neoforge.network.registration.PayloadRegistrar`.
#[derive(Clone)]
pub struct PayloadRegistrar {
    registrations: Arc<Mutex<Registrations>>,
    version: String,
    optional: bool,
    thread: HandlerThread,
}

/// The `Result` of a registrar call, with the registrar for the next call.
pub type RegistrarResult<'a> = Result<&'a PayloadRegistrar, NetworkError>;

impl PayloadRegistrar {
    /// A client-bound channel of the play phase.
    ///
    /// # Errors
    ///
    /// See [`PayloadRegistrar::play_to_server`].
    pub fn play_to_client(&self, channel: &str) -> RegistrarResult<'_> {
        self.register(channel, PLAY, Some(PacketFlow::Clientbound), None)
    }

    /// A server-bound channel of the play phase.
    ///
    /// # Errors
    ///
    /// [`NetworkError::RegistrationClosed`] after the registration step,
    /// [`NetworkError::InvalidChannel`], [`NetworkError::ReservedNamespace`],
    /// [`NetworkError::BlankVersion`], and [`NetworkError::Duplicate`] when a protocol of the call
    /// already has the channel. A failed call registers nothing.
    pub fn play_to_server(
        &self,
        channel: &str,
        handler: impl Fn(&[u8], &mut PayloadContext<'_>) + Send + Sync + 'static,
    ) -> RegistrarResult<'_> {
        self.register(
            channel,
            PLAY,
            Some(PacketFlow::Serverbound),
            Some(Arc::new(handler)),
        )
    }

    /// A channel of the play phase in both directions.
    ///
    /// # Errors
    ///
    /// See [`PayloadRegistrar::play_to_server`].
    pub fn play_bidirectional(
        &self,
        channel: &str,
        handler: impl Fn(&[u8], &mut PayloadContext<'_>) + Send + Sync + 'static,
    ) -> RegistrarResult<'_> {
        self.register(channel, PLAY, None, Some(Arc::new(handler)))
    }

    /// A client-bound channel of the configuration phase.
    ///
    /// # Errors
    ///
    /// See [`PayloadRegistrar::play_to_server`].
    pub fn configuration_to_client(&self, channel: &str) -> RegistrarResult<'_> {
        self.register(channel, CONFIGURATION, Some(PacketFlow::Clientbound), None)
    }

    /// A server-bound channel of the configuration phase.
    ///
    /// # Errors
    ///
    /// See [`PayloadRegistrar::play_to_server`].
    pub fn configuration_to_server(
        &self,
        channel: &str,
        handler: impl Fn(&[u8], &mut PayloadContext<'_>) + Send + Sync + 'static,
    ) -> RegistrarResult<'_> {
        self.register(
            channel,
            CONFIGURATION,
            Some(PacketFlow::Serverbound),
            Some(Arc::new(handler)),
        )
    }

    /// A channel of the configuration phase in both directions.
    ///
    /// # Errors
    ///
    /// See [`PayloadRegistrar::play_to_server`].
    pub fn configuration_bidirectional(
        &self,
        channel: &str,
        handler: impl Fn(&[u8], &mut PayloadContext<'_>) + Send + Sync + 'static,
    ) -> RegistrarResult<'_> {
        self.register(channel, CONFIGURATION, None, Some(Arc::new(handler)))
    }

    /// A client-bound channel of both phases.
    ///
    /// # Errors
    ///
    /// See [`PayloadRegistrar::play_to_server`].
    pub fn common_to_client(&self, channel: &str) -> RegistrarResult<'_> {
        self.register(channel, COMMON, Some(PacketFlow::Clientbound), None)
    }

    /// A server-bound channel of both phases.
    ///
    /// # Errors
    ///
    /// See [`PayloadRegistrar::play_to_server`].
    pub fn common_to_server(
        &self,
        channel: &str,
        handler: impl Fn(&[u8], &mut PayloadContext<'_>) + Send + Sync + 'static,
    ) -> RegistrarResult<'_> {
        self.register(
            channel,
            COMMON,
            Some(PacketFlow::Serverbound),
            Some(Arc::new(handler)),
        )
    }

    /// A channel of both phases in both directions.
    ///
    /// # Errors
    ///
    /// See [`PayloadRegistrar::play_to_server`].
    pub fn common_bidirectional(
        &self,
        channel: &str,
        handler: impl Fn(&[u8], &mut PayloadContext<'_>) + Send + Sync + 'static,
    ) -> RegistrarResult<'_> {
        self.register(channel, COMMON, None, Some(Arc::new(handler)))
    }

    /// A copy whose handlers run on `thread`. The default is [`HandlerThread::Main`].
    #[must_use]
    pub fn executes_on(&self, thread: HandlerThread) -> Self {
        Self {
            thread,
            ..self.clone()
        }
    }

    /// A copy that registers its channels with `version`. A `NeoForge` client with another
    /// version of a channel fails the negotiation.
    #[must_use]
    pub fn versioned(&self, version: &str) -> Self {
        Self {
            version: version.to_owned(),
            ..self.clone()
        }
    }

    /// A copy that registers optional channels. A client without a required channel cannot join.
    #[must_use]
    pub fn optional(&self) -> Self {
        Self {
            optional: true,
            ..self.clone()
        }
    }

    /// `NetworkRegistry.register`: the checks, then one registration for every protocol.
    fn register(
        &self,
        channel: &str,
        protocols: &'static [ConnectionProtocol],
        flow: Option<PacketFlow>,
        handler: Option<PayloadHandler>,
    ) -> RegistrarResult<'_> {
        let mut registrations = self
            .registrations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if registrations.closed {
            return Err(NetworkError::RegistrationClosed);
        }
        let id = Identifier::parse(channel)
            .map_err(|_| NetworkError::InvalidChannel(channel.to_owned()))?;
        if self.version.trim().is_empty() {
            return Err(NetworkError::BlankVersion(id.to_string()));
        }
        if id.namespace() == VANILLA_NAMESPACE {
            return Err(NetworkError::ReservedNamespace(id.to_string()));
        }
        if protocols
            .iter()
            .any(|protocol| registrations.taken.contains(&(*protocol, id.clone())))
        {
            return Err(NetworkError::Duplicate(id.to_string()));
        }
        for protocol in protocols {
            registrations.taken.insert((*protocol, id.clone()));
        }
        registrations.entries.push(PayloadRegistration {
            id,
            protocols,
            flow,
            version: self.version.trim().to_owned(),
            optional: self.optional,
            thread: self.thread,
            handler,
        });
        drop(registrations);
        Ok(self)
    }
}

/// The channels that one connection can send on, like `NetworkRegistry.hasChannel`: the
/// negotiated channels, then the client's `c:register` channels, then its ad hoc channels.
#[derive(Debug, Clone, Copy)]
pub struct NegotiatedChannels<'a> {
    pub(crate) connection_type: ConnectionType,
    pub(crate) payload_setup: &'a NetworkPayloadSetup,
    pub(crate) common: &'a BTreeMap<ConnectionProtocol, BTreeSet<Identifier>>,
    pub(crate) ad_hoc: &'a BTreeSet<Identifier>,
}

impl NegotiatedChannels<'_> {
    /// `NetworkRegistry.hasChannel` for `protocol`.
    #[must_use]
    pub fn has_channel(&self, protocol: ConnectionProtocol, channel: &Identifier) -> bool {
        self.payload_setup
            .channels
            .get(&protocol)
            .is_some_and(|channels| channels.contains_key(channel))
            || self
                .common
                .get(&protocol)
                .is_some_and(|channels| channels.contains(channel))
            || self.ad_hoc.contains(channel)
    }

    /// `NetworkRegistry.checkPacket`: a vanilla or builtin channel can always be sent, any other
    /// channel only when [`NegotiatedChannels::has_channel`] holds.
    ///
    /// # Errors
    ///
    /// [`NetworkError::InvalidChannel`] or [`NetworkError::ChannelNotNegotiated`].
    pub fn check_send(
        &self,
        protocol: ConnectionProtocol,
        channel: &str,
    ) -> Result<(), NetworkError> {
        let id = Identifier::parse(channel)
            .map_err(|_| NetworkError::InvalidChannel(channel.to_owned()))?;
        if id.namespace() == VANILLA_NAMESPACE
            || is_builtin_channel(&id)
            || self.has_channel(protocol, &id)
        {
            Ok(())
        } else {
            Err(NetworkError::ChannelNotNegotiated(id.to_string()))
        }
    }

    /// What is on the other side of the connection.
    #[must_use]
    pub const fn connection_type(&self) -> ConnectionType {
        self.connection_type
    }
}

/// What a payload handler can do with the connection of the payload. The server applies the
/// replies, the disconnect and the finished task after the handler returns, in this order.
///
/// `NeoForge`: `IPayloadContext`.
pub struct PayloadContext<'a> {
    channel: &'a str,
    protocol: ConnectionProtocol,
    player: Option<&'a Arc<Player>>,
    channels: NegotiatedChannels<'a>,
    actions: PayloadActions,
}

/// The replies, the disconnect and the finished configuration tasks of one handler call.
#[derive(Default)]
pub(crate) struct PayloadActions {
    pub(crate) replies: Vec<(String, Vec<u8>)>,
    pub(crate) disconnect: Option<TextComponent>,
    pub(crate) finished: Vec<&'static str>,
}

impl<'a> PayloadContext<'a> {
    pub(crate) fn new(
        channel: &'a str,
        protocol: ConnectionProtocol,
        player: Option<&'a Arc<Player>>,
        channels: NegotiatedChannels<'a>,
    ) -> Self {
        Self {
            channel,
            protocol,
            player,
            channels,
            actions: PayloadActions::default(),
        }
    }

    pub(crate) fn into_actions(self) -> PayloadActions {
        self.actions
    }

    /// The channel of the payload.
    #[must_use]
    pub const fn channel(&self) -> &str {
        self.channel
    }

    /// The player who sent the payload, or `None` in the configuration phase, where the player
    /// does not exist yet (`NeoForge` throws there).
    ///
    /// `NeoForge`: `IPayloadContext#player`.
    #[must_use]
    pub const fn player(&self) -> Option<&'a Arc<Player>> {
        self.player
    }

    /// Always [`PacketFlow::Serverbound`]: the server handles the payloads of the client.
    ///
    /// `NeoForge`: `IPayloadContext#flow`.
    #[must_use]
    pub const fn flow(&self) -> PacketFlow {
        PacketFlow::Serverbound
    }

    /// The phase of the connection: [`ConnectionProtocol::Configuration`] or
    /// [`ConnectionProtocol::Play`].
    ///
    /// `NeoForge`: `IPayloadContext#protocol`.
    #[must_use]
    pub const fn protocol(&self) -> ConnectionProtocol {
        self.protocol
    }

    /// The channels the client of the connection can receive.
    #[must_use]
    pub const fn channels(&self) -> NegotiatedChannels<'a> {
        self.channels
    }

    /// Sends a payload back to the client of the payload.
    ///
    /// `NeoForge`: `IPayloadContext#reply`.
    ///
    /// # Errors
    ///
    /// [`NetworkError::ChannelNotNegotiated`] when the client cannot receive `channel` in this
    /// phase, as `NetworkRegistry.checkPacket` throws.
    pub fn reply(&mut self, channel: &str, data: impl Into<Vec<u8>>) -> Result<(), NetworkError> {
        self.channels.check_send(self.protocol, channel)?;
        self.actions.replies.push((channel.to_owned(), data.into()));
        Ok(())
    }

    /// Disconnects the client with `reason`.
    ///
    /// `NeoForge`: `IPayloadContext#disconnect`.
    pub fn disconnect(&mut self, reason: TextComponent) {
        self.actions.disconnect = Some(reason);
    }

    /// Finishes the current configuration task of the connection when its type is `task_type`.
    /// When another task, or no task, is current, the server disconnects the client.
    ///
    /// `NeoForge`: `IPayloadContext#finishCurrentTask`.
    ///
    /// # Errors
    ///
    /// [`NetworkError::NotConfiguration`] in the play phase.
    pub fn finish_current_task(&mut self, task_type: &'static str) -> Result<(), NetworkError> {
        if self.protocol != ConnectionProtocol::Configuration {
            return Err(NetworkError::NotConfiguration);
        }
        self.actions.finished.push(task_type);
        Ok(())
    }
}

/// A configuration task of a mod.
///
/// It runs during the configuration phase, after the tasks of `NeoForge` and before the end of
/// the configuration, and the next task waits until it finishes. A task finishes through
/// [`TaskContext::finish`] in [`CustomConfigurationTask::run`], or later through
/// [`PayloadContext::finish_current_task`] in the handler of the client's answer.
///
/// `NeoForge`: `ICustomConfigurationTask` (and `ConfigurationTask`).
pub trait CustomConfigurationTask: Send + Sync {
    /// The type of the task, such as `"mymod:licence"`.
    ///
    /// `NeoForge`: `ConfigurationTask#type`.
    fn task_type(&self) -> &'static str;

    /// Starts the task: sends its payloads with [`TaskContext::send`].
    ///
    /// `NeoForge`: `ICustomConfigurationTask#run`.
    fn run(&self, context: &mut TaskContext<'_>);
}

/// What a configuration task can do when it starts. The server sends the payloads after
/// [`CustomConfigurationTask::run`] returns, then finishes the task if it called
/// [`TaskContext::finish`].
pub struct TaskContext<'a> {
    channels: NegotiatedChannels<'a>,
    pub(crate) sent: Vec<(String, Vec<u8>)>,
    pub(crate) finished: bool,
}

impl<'a> TaskContext<'a> {
    pub(crate) const fn new(channels: NegotiatedChannels<'a>) -> Self {
        Self {
            channels,
            sent: Vec::new(),
            finished: false,
        }
    }

    /// Sends a configuration payload to the client.
    ///
    /// # Errors
    ///
    /// [`NetworkError::ChannelNotNegotiated`] when the client cannot receive `channel`.
    pub fn send(&mut self, channel: &str, data: impl Into<Vec<u8>>) -> Result<(), NetworkError> {
        self.channels
            .check_send(ConnectionProtocol::Configuration, channel)?;
        self.sent.push((channel.to_owned(), data.into()));
        Ok(())
    }

    /// Finishes the task at once, for a task that waits for no answer.
    pub const fn finish(&mut self) {
        self.finished = true;
    }

    /// The channels the client of the connection can receive.
    #[must_use]
    pub const fn channels(&self) -> NegotiatedChannels<'a> {
        self.channels
    }
}

/// Fires once for each Java connection, after the channel negotiation.
///
/// The server fires it when it queues the configuration tasks. A handler adds the tasks of its mod
/// with [`RegisterConfigurationTasksEvent::register`]; they run in registration order after the
/// tasks of `NeoForge` and before the end of the configuration.
///
/// `NeoForge`: `net.neoforged.neoforge.network.event.RegisterConfigurationTasksEvent`, posted on
/// the mod bus by `ServerConfigurationPacketListenerImpl.runConfiguration`.
#[derive(Event)]
pub struct RegisterConfigurationTasksEvent {
    player_name: String,
    player_uuid: Uuid,
    connection_type: ConnectionType,
    payload_setup: NetworkPayloadSetup,
    common: BTreeMap<ConnectionProtocol, BTreeSet<Identifier>>,
    ad_hoc: BTreeSet<Identifier>,
    tasks: Mutex<Vec<Arc<dyn CustomConfigurationTask>>>,
}

impl RegisterConfigurationTasksEvent {
    pub(crate) fn new(
        player_name: String,
        player_uuid: Uuid,
        channels: NegotiatedChannels<'_>,
    ) -> Self {
        Self {
            player_name,
            player_uuid,
            connection_type: channels.connection_type,
            payload_setup: channels.payload_setup.clone(),
            common: channels.common.clone(),
            ad_hoc: channels.ad_hoc.clone(),
            tasks: Mutex::new(Vec::new()),
        }
    }

    /// Queues `task` after the tasks registered before it.
    ///
    /// `NeoForge`: `RegisterConfigurationTasksEvent#register`.
    pub fn register(&self, task: Arc<dyn CustomConfigurationTask>) {
        self.tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(task);
    }

    /// What is on the other side of the connection.
    ///
    /// `NeoForge`: `getListener().getConnectionType()`.
    #[must_use]
    pub const fn connection_type(&self) -> ConnectionType {
        self.connection_type
    }

    /// Whether the client can receive `channel` in the configuration phase.
    ///
    /// `NeoForge`: `getListener().hasChannel(..)`.
    #[must_use]
    pub fn has_channel(&self, channel: &str) -> bool {
        Identifier::parse(channel).is_ok_and(|id| {
            self.channels()
                .has_channel(ConnectionProtocol::Configuration, &id)
        })
    }

    /// The channels of the connection, as [`NegotiatedChannels`].
    #[must_use]
    pub const fn channels(&self) -> NegotiatedChannels<'_> {
        NegotiatedChannels {
            connection_type: self.connection_type,
            payload_setup: &self.payload_setup,
            common: &self.common,
            ad_hoc: &self.ad_hoc,
        }
    }

    /// The name of the player who logs in.
    #[must_use]
    pub fn player_name(&self) -> &str {
        &self.player_name
    }

    /// The UUID of the player who logs in.
    #[must_use]
    pub const fn player_uuid(&self) -> Uuid {
        self.player_uuid
    }

    pub(crate) fn take_tasks(&mut self) -> Vec<Arc<dyn CustomConfigurationTask>> {
        std::mem::take(self.tasks.get_mut().unwrap_or_else(PoisonError::into_inner))
    }
}
