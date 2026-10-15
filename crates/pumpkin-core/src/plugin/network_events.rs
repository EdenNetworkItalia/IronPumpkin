//! The `IronPumpkin` host side of the mod network API: the payload registration step, the
//! per-channel dispatch of the configuration and play payloads, the mod configuration tasks and
//! the send helpers.
//!
//! The upstream files only call into this module: `NetworkRegistry` keeps the registered
//! channels in [`ModPayloads`], `pending.rs` hands the configuration payloads to
//! [`handle_configuration_payload`], the play packet handler and the connection task hand the play
//! payloads to [`handle_play_payload`] and [`handle_play_payload_on_network`], and the
//! configuration task queue fires [`RegisterConfigurationTasksEvent`] through
//! [`register_configuration_tasks`] and runs a mod task through [`run_mod_task`].

use std::{
    collections::{BTreeSet, HashMap},
    fmt,
    sync::{Arc, OnceLock},
};

use pumpkin_data::translation;
use pumpkin_protocol::{
    ServerPacket,
    java::{client::play::CCustomPayload, server::play::SCustomPayload},
    packet::MultiVersionJavaPacket,
};
use pumpkin_util::{identifier::Identifier, text::TextComponent};
use tracing::{debug, error, info};

use crate::{
    entity::player::Player,
    net::{
        DisconnectReason,
        java::{
            JavaClient,
            configuration_tasks::TaskReply,
            neoforge::{EMULATED_NEOFORGE_VERSION, NegotiatedState},
            pending::PendingConnection,
        },
    },
    plugin::api::events::network::{
        ConnectionProtocol, CustomConfigurationTask, HandlerThread, NegotiatedChannels,
        NetworkError, PacketFlow, PayloadActions, PayloadContext, PayloadHandler,
        PayloadRegistration, RegisterConfigurationTasksEvent, RegisterPayloadHandlersEvent,
        TaskContext,
    },
    server::Server,
    world::World,
};

/// A channel that a mod registered for one protocol.
struct ModPayload {
    flow: Option<PacketFlow>,
    optional: bool,
    thread: HandlerThread,
    handler: Option<PayloadHandler>,
}

#[derive(Default)]
struct ByProtocol {
    configuration: HashMap<String, ModPayload>,
    play: HashMap<String, ModPayload>,
    /// Whether a play channel has a handler on [`HandlerThread::Network`].
    play_network: bool,
}

/// The channels of the mods with their handlers, like `NetworkRegistry.PAYLOAD_REGISTRATIONS`
/// and `SERVERBOUND_HANDLERS`. Empty until the registration step, and fixed after it.
#[derive(Default)]
pub struct ModPayloads(OnceLock<ByProtocol>);

impl ModPayloads {
    fn get(&self, protocol: ConnectionProtocol, channel: &str) -> Option<&ModPayload> {
        let by_protocol = self.0.get()?;
        match protocol {
            ConnectionProtocol::Configuration => by_protocol.configuration.get(channel),
            ConnectionProtocol::Play => by_protocol.play.get(channel),
            _ => None,
        }
    }

    fn has_play_network_handlers(&self) -> bool {
        self.0
            .get()
            .is_some_and(|by_protocol| by_protocol.play_network)
    }
}

/// The configuration tasks that mods registered for one connection, in registration order.
#[derive(Default, Clone)]
pub struct ModTasks(Vec<Arc<dyn CustomConfigurationTask>>);

impl ModTasks {
    /// Adds `task` and returns its index.
    pub(crate) fn push(&mut self, task: Arc<dyn CustomConfigurationTask>) -> usize {
        self.0.push(task);
        self.0.len() - 1
    }

    pub(crate) fn get(&self, index: usize) -> Option<Arc<dyn CustomConfigurationTask>> {
        self.0.get(index).cloned()
    }
}

impl fmt::Debug for ModTasks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.0.iter().map(|task| task.task_type()))
            .finish()
    }
}

/// The payload registration step, once at startup.
///
/// It fires [`RegisterPayloadHandlersEvent`] when a handler listens, then offers the registered
/// channels in the negotiation and keeps their handlers. It runs before the synced configs load, as `NetworkRegistry.setup` does on `NeoForge`.
pub async fn register_payload_handlers(server: &Arc<Server>) {
    let registrations = if server
        .plugin_manager
        .has_handlers::<RegisterPayloadHandlersEvent>()
    {
        let mut taken = BTreeSet::new();
        for protocol in [ConnectionProtocol::Configuration, ConnectionProtocol::Play] {
            for registration in server.network_registry.registrations(protocol).await {
                taken.insert((protocol, registration.id));
            }
        }
        let mut event = RegisterPayloadHandlersEvent::new(taken);
        server.plugin_manager.fire(server, &mut event).await;
        event.close()
    } else {
        Vec::new()
    };
    let mut by_protocol = ByProtocol::default();
    for registration in registrations {
        if let Err(err) = server
            .network_registry
            .register(
                registration.id.clone(),
                registration.protocols,
                registration.flow,
                &registration.version,
                registration.optional,
            )
            .await
        {
            error!("[ironpumpkin] {err}");
            continue;
        }
        add(&mut by_protocol, &registration);
    }
    if server
        .network_registry
        .mod_payloads
        .0
        .set(by_protocol)
        .is_err()
    {
        error!("[ironpumpkin] the payload handlers are registered once");
    }
}

fn add(by_protocol: &mut ByProtocol, registration: &PayloadRegistration) {
    let channel = registration.id.to_string();
    for protocol in registration.protocols {
        let payload = ModPayload {
            flow: registration.flow,
            optional: registration.optional,
            thread: registration.thread,
            handler: registration.handler.clone(),
        };
        match protocol {
            ConnectionProtocol::Configuration => {
                by_protocol.configuration.insert(channel.clone(), payload);
            }
            ConnectionProtocol::Play => {
                by_protocol.play_network |=
                    registration.thread == HandlerThread::Network && registration.handler.is_some();
                by_protocol.play.insert(channel.clone(), payload);
            }
            _ => {}
        }
    }
}

/// The `multiplayer.disconnect.incompatible` reason of `NetworkRegistry.handleModdedPayload`.
fn incompatible(detail: &str) -> TextComponent {
    TextComponent::translate(
        translation::java::MULTIPLAYER_DISCONNECT_INCOMPATIBLE,
        [TextComponent::text(format!(
            "NeoForge {EMULATED_NEOFORGE_VERSION} ({detail})"
        ))],
    )
}

/// What the dispatch does with a payload on a mod channel.
enum Route<'a> {
    /// Not a mod channel of this protocol: the plugin events get it.
    NotModded,
    /// A client-bound channel: `getCodec` refuses to decode it, so the server drops it.
    Discarded,
    /// The client cannot send on the channel: disconnect with this reason.
    Refused(TextComponent),
    Handle(&'a PayloadHandler),
}

/// `NetworkRegistry.getCodec` and `handleModdedPayload`: the channel must be negotiated, or be
/// optional, which makes it readable ad hoc.
fn route<'a>(
    server: &'a Server,
    protocol: ConnectionProtocol,
    channel: &str,
    negotiated: &NegotiatedChannels<'_>,
) -> Route<'a> {
    let Some(payload) = server.network_registry.mod_payloads.get(protocol, channel) else {
        return Route::NotModded;
    };
    if payload.flow == Some(PacketFlow::Clientbound) {
        return Route::Discarded;
    }
    let Some(handler) = payload.handler.as_ref() else {
        return Route::Refused(incompatible(&format!("No Handler for {channel}")));
    };
    let is_negotiated = Identifier::parse(channel).is_ok_and(|id| {
        negotiated
            .payload_setup
            .channels
            .get(&protocol)
            .is_some_and(|channels| channels.contains_key(&id))
    });
    if !is_negotiated && !payload.optional {
        return Route::Refused(incompatible(&format!("No Channel for {channel}")));
    }
    Route::Handle(handler)
}

/// The channels of a connection in the configuration phase.
const fn configuring_channels(connection: &PendingConnection) -> NegotiatedChannels<'_> {
    NegotiatedChannels {
        connection_type: connection.client_channels.connection_type,
        payload_setup: &connection.payload_setup,
        common: &connection.client_channels.common,
        ad_hoc: &connection.client_channels.ad_hoc,
    }
}

/// The channels a player negotiated before play.
#[must_use]
pub const fn negotiated_channels(state: &NegotiatedState) -> NegotiatedChannels<'_> {
    NegotiatedChannels {
        connection_type: state.connection_type,
        payload_setup: &state.payload_setup,
        common: &state.common_channels,
        ad_hoc: &state.ad_hoc_channels,
    }
}

/// Hands a configuration payload on a mod channel to the handler of its channel. Returns false
/// for any other channel, which the plugin events then get.
pub(crate) async fn handle_configuration_payload(
    connection: &mut PendingConnection,
    server: &Server,
    channel: &str,
    data: &[u8],
) -> bool {
    if server
        .network_registry
        .mod_payloads
        .get(ConnectionProtocol::Configuration, channel)
        .is_none()
    {
        return false;
    }
    if connection.neoforge_probe_pending {
        info!(
            "Client {} sent the modded payload {channel} before the channel negotiation, disconnecting",
            connection.id
        );
        connection.kick(incompatible("No Payload Setup")).await;
        return true;
    }
    let negotiated = configuring_channels(connection);
    let actions = match route(
        server,
        ConnectionProtocol::Configuration,
        channel,
        &negotiated,
    ) {
        Route::NotModded => return false,
        Route::Discarded => {
            debug!(
                "Client {} sent a payload on the client-bound channel {channel}",
                connection.id
            );
            return true;
        }
        Route::Refused(reason) => {
            info!(
                "Client {} sent the modded payload {channel} on a channel it did not negotiate, disconnecting",
                connection.id
            );
            connection.kick(reason).await;
            return true;
        }
        Route::Handle(handler) => {
            let mut context =
                PayloadContext::new(channel, ConnectionProtocol::Configuration, None, negotiated);
            handler(data, &mut context);
            context.into_actions()
        }
    };
    for (reply_channel, reply) in actions.replies {
        if connection.is_closed() {
            return true;
        }
        connection.send_custom_payload(&reply_channel, &reply).await;
    }
    if let Some(reason) = actions.disconnect {
        connection.kick(reason).await;
        return true;
    }
    for task_type in actions.finished {
        if !connection
            .finish_configuration_task(TaskReply::ModTask(task_type))
            .await
        {
            return true;
        }
        connection.run_configuration_tasks(server).await;
    }
    true
}

/// Hands a play payload on a mod channel to the handler of its channel, on the world tick of the
/// player. Returns false for any other channel, which the plugin events then get.
pub(crate) fn handle_play_payload(
    player: &Arc<Player>,
    server: &Server,
    channel: &str,
    data: &[u8],
) -> bool {
    let state = player.negotiated_state();
    let negotiated = negotiated_channels(state);
    let actions = match route(server, ConnectionProtocol::Play, channel, &negotiated) {
        Route::NotModded => return false,
        Route::Discarded => return true,
        Route::Refused(reason) => {
            info!(
                "{} sent the modded payload {channel} on a channel it did not negotiate, disconnecting",
                player.gameprofile.name
            );
            player.kick(DisconnectReason::Kicked, &reason);
            return true;
        }
        Route::Handle(handler) => {
            let mut context =
                PayloadContext::new(channel, ConnectionProtocol::Play, Some(player), negotiated);
            handler(data, &mut context);
            context.into_actions()
        }
    };
    apply_play_actions(player, actions);
    true
}

fn apply_play_actions(player: &Player, actions: PayloadActions) {
    for (channel, data) in actions.replies {
        player.try_send_client_packet(&CCustomPayload::new(&channel, &data));
    }
    if let Some(reason) = actions.disconnect {
        player.kick(DisconnectReason::Kicked, &reason);
    }
}

/// Hands a play payload to its mod handler on the connection task.
///
/// Only a channel whose handler runs on [`HandlerThread::Network`] is handled here. Returns false for any other packet, which goes to the
/// world tick of the player.
pub fn handle_play_payload_on_network(
    client: &JavaClient,
    player: &Arc<Player>,
    server: &Server,
    packet: &pumpkin_protocol::RawPacket,
) -> bool {
    if !server
        .network_registry
        .mod_payloads
        .has_play_network_handlers()
        || packet.id != SCustomPayload::to_id(client.version.load())
    {
        return false;
    }
    let Ok(payload) = SCustomPayload::read(&mut &packet.payload[..], &client.version.load()) else {
        return false;
    };
    if server
        .network_registry
        .mod_payloads
        .get(ConnectionProtocol::Play, payload.channel)
        .is_none_or(|mod_payload| mod_payload.thread != HandlerThread::Network)
    {
        return false;
    }
    handle_play_payload(player, server, payload.channel, payload.data)
}

/// Fires [`RegisterConfigurationTasksEvent`] when a handler listens, and queues the tasks of the
/// mods before the end of the configuration.
pub(crate) async fn register_configuration_tasks(connection: &mut PendingConnection) {
    let Some(server) = connection.server_arc() else {
        return;
    };
    if !server
        .plugin_manager
        .has_handlers::<RegisterConfigurationTasksEvent>()
    {
        return;
    }
    let (player_name, player_uuid) = connection
        .gameprofile
        .as_ref()
        .map(|profile| (profile.name.clone(), profile.id))
        .unwrap_or_default();
    let mut event = RegisterConfigurationTasksEvent::new(
        player_name,
        player_uuid,
        configuring_channels(connection),
    );
    server.plugin_manager.fire(&server, &mut event).await;
    connection
        .configuration_tasks
        .add_mod_tasks(event.take_tasks());
}

/// Starts the mod task `index` of the connection: runs it, sends its payloads, and finishes it
/// when it asked to.
pub(crate) async fn run_mod_task(connection: &mut PendingConnection, index: usize) {
    let Some(task) = connection.configuration_tasks.mod_task(index) else {
        return;
    };
    let mut context = TaskContext::new(configuring_channels(connection));
    task.run(&mut context);
    let TaskContext { sent, finished, .. } = context;
    for (channel, data) in sent {
        if connection.is_closed() {
            return;
        }
        connection.send_custom_payload(&channel, &data).await;
    }
    if finished {
        connection
            .finish_configuration_task(TaskReply::ModTask(task.task_type()))
            .await;
    }
}

/// Sends a payload to `player`.
///
/// `NeoForge`: `PacketDistributor#sendToPlayer`.
///
/// # Errors
///
/// [`NetworkError::ChannelNotNegotiated`] when the client of the player did not negotiate
/// `channel`, such as a vanilla client on a mod channel, and [`NetworkError::InvalidChannel`].
pub fn send_to_player(player: &Player, channel: &str, data: &[u8]) -> Result<(), NetworkError> {
    negotiated_channels(player.negotiated_state()).check_send(ConnectionProtocol::Play, channel)?;
    player.try_send_client_packet(&CCustomPayload::new(channel, data));
    Ok(())
}

/// Sends a payload to every player of the server whose client negotiated `channel`, skips the
/// others, and returns how many players got it.
///
/// `NeoForge`: `PacketDistributor#sendToAllPlayers`.
pub fn send_to_all_players(server: &Server, channel: &str, data: &[u8]) -> usize {
    server
        .worlds
        .load()
        .iter()
        .map(|world| send_to_players_in_dimension(world, channel, data))
        .sum()
}

/// Sends a payload to every player of `world` whose client negotiated `channel`, skips the
/// others, and returns how many players got it.
///
/// `NeoForge`: `PacketDistributor#sendToPlayersInDimension`.
pub fn send_to_players_in_dimension(world: &World, channel: &str, data: &[u8]) -> usize {
    world
        .players
        .load()
        .iter()
        .filter(|player| send_to_player(player, channel, data).is_ok())
        .count()
}
