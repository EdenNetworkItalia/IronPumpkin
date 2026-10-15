//! The mod network API (`openspec/changes/neoforge-shaped-plugin-api`, spec `neoforge-network`)
//! against a server of the support module: the payload registration step, the negotiation of a
//! required channel, the per-channel dispatch in play and in configuration, the payload context,
//! the send helpers and the finish request of a mod task.
//!
//! The configuration task scenarios that need a client run in the boot test `neoforge_network`
//! of `ironpumpkin-test-mod`, with the headless client.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use bytes::Bytes;
use pumpkin_core::{
    entity::player::Player,
    net::{
        ClientPlatform,
        java::{
            JavaClient,
            neoforge::{ConnectionType, NegotiatedState},
            pending::PendingConnection,
        },
        packet_limiter::PacketRateLimiter,
    },
    plugin::{
        BoxFuture, EventHandler, EventPriority, PluginMetadata,
        api::events::network::{
            ConnectionProtocol, HandlerThread, NetworkError, PacketFlow, PayloadContext,
            PayloadRegistrar, RegisterPayloadHandlersEvent,
        },
        network_events::{
            handle_play_payload_on_network, register_payload_handlers, send_to_all_players,
            send_to_player, send_to_players_in_dimension,
        },
        server::config_custom_payload::ConfigCustomPayloadEvent,
        startup::create_context,
    },
    server::Server,
};
use pumpkin_data::packet::CURRENT_MC_VERSION;
use pumpkin_protocol::{
    ConnectionState, RawPacket,
    java::{
        client::{config::CConfigDisconnect, play::CCustomPayload},
        neoforge::{
            ModdedNetworkQueryPayload, ModdedNetworkSetupFailedPayload, NetworkChannel,
            NetworkPayloadSetup,
        },
        packet_decoder::TCPNetworkDecoder,
        server::{config::SPluginMessage, play::SCustomPayload},
    },
    packet::MultiVersionJavaPacket,
    ser::NetworkReadExt,
};
use pumpkin_util::{identifier::Identifier, text::TextComponent};
use support::{RUNTIME, player_with_peer, server};

const SET_LEVEL: &str = "nettest:set_level";
const LEVEL: &str = "nettest:level";
const OPTIONAL_PLAY: &str = "nettest:optional_play";
const NETWORK_PLAY: &str = "nettest:network";
const LICENCE: &str = "nettest:licence";
const LICENCE_TASK: &str = "nettest:licence";

/// What the handlers received: the channel, the bytes and the player name or `-` without one.
static RECEIVED: Mutex<Vec<(String, Vec<u8>, String)>> = Mutex::new(Vec::new());
/// The protocol and the flow of each payload that a handler received, by channel.
static CONTEXTS: Mutex<Vec<(String, ConnectionProtocol, PacketFlow)>> = Mutex::new(Vec::new());
/// The data on [`SET_LEVEL`] that makes its handler disconnect the sender.
const KICK_ME: &[u8] = &[0xff];
/// The result of each registrar call of the handler, by label.
static RESULTS: Mutex<Vec<(&str, Result<(), NetworkError>)>> = Mutex::new(Vec::new());
/// A registrar that the handler keeps for a call after the step.
static LATE: Mutex<Option<PayloadRegistrar>> = Mutex::new(None);

fn receive(data: &[u8], context: &mut PayloadContext<'_>) {
    let player = context
        .player()
        .map_or_else(|| "-".to_owned(), |player| player.gameprofile.name.clone());
    RECEIVED
        .lock()
        .unwrap()
        .push((context.channel().to_owned(), data.to_vec(), player));
    CONTEXTS.lock().unwrap().push((
        context.channel().to_owned(),
        context.protocol(),
        context.flow(),
    ));
}

fn contexts(channel: &str) -> Vec<(ConnectionProtocol, PacketFlow)> {
    CONTEXTS
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, ..)| c == channel)
        .map(|(_, protocol, flow)| (*protocol, *flow))
        .collect()
}

fn received(channel: &str) -> Vec<(Vec<u8>, String)> {
    RECEIVED
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, ..)| c == channel)
        .map(|(_, data, player)| (data.clone(), player.clone()))
        .collect()
}

struct Payloads;

impl EventHandler<RegisterPayloadHandlersEvent> for Payloads {
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a RegisterPayloadHandlersEvent,
    ) -> BoxFuture<'a, ()> {
        let record = |label, result: Result<&PayloadRegistrar, NetworkError>| {
            RESULTS.lock().unwrap().push((label, result.map(|_| ())));
        };
        let registrar = event.registrar("1");
        record(
            "set_level",
            registrar.play_to_server(SET_LEVEL, |data, context| {
                receive(data, context);
                if data == KICK_ME {
                    context.disconnect(TextComponent::text("Level out of range"));
                } else {
                    let _ = context.reply(LEVEL, data);
                }
            }),
        );
        record("level", registrar.play_to_client(LEVEL));
        record(
            "optional_play",
            registrar.optional().play_to_server(OPTIONAL_PLAY, receive),
        );
        record(
            "network",
            registrar
                .optional()
                .executes_on(HandlerThread::Network)
                .play_to_server(NETWORK_PLAY, receive),
        );
        record(
            "licence",
            registrar
                .versioned("2")
                .configuration_bidirectional(LICENCE, |data, context| {
                    receive(data, context);
                    let _ = context.finish_current_task(LICENCE_TASK);
                }),
        );
        record("duplicate", registrar.play_to_server(SET_LEVEL, receive));
        record(
            "neoforge duplicate",
            registrar.configuration_to_client("neoforge:split"),
        );
        record("vanilla", registrar.play_to_client("minecraft:nettest"));
        record(
            "blank",
            event.registrar(" ").play_to_client("nettest:blank"),
        );
        record("invalid", registrar.play_to_client("nettest:Bad Channel"));
        *LATE.lock().unwrap() = Some(registrar);
        Box::pin(async {})
    }
}

/// Registers the handler and runs the registration step once per process.
static REGISTERED: LazyLock<()> = LazyLock::new(|| {
    let server = server();
    let metadata = PluginMetadata {
        name: "nettest".to_owned(),
        version: "1.0.0".to_owned(),
        authors: Vec::new(),
        description: String::new(),
        dependencies: Vec::new(),
        permissions: Vec::new(),
    };
    create_context(server, metadata).register_event_with::<RegisterPayloadHandlersEvent, _>(
        Arc::new(Payloads),
        EventPriority::Normal,
        false,
        false,
    );
    RUNTIME.block_on(register_payload_handlers(server));
});

fn registered() -> &'static Arc<Server> {
    LazyLock::force(&REGISTERED);
    server()
}

/// A negotiated state with `channels` on `protocol`.
fn negotiated(protocol: ConnectionProtocol, channels: &[&'static str]) -> NegotiatedState {
    let channels = channels
        .iter()
        .map(|channel| {
            let id = Identifier::parse_static(channel);
            let channel = NetworkChannel {
                id: id.clone(),
                chosen_version: "1".to_owned(),
            };
            (id, channel)
        })
        .collect();
    NegotiatedState {
        connection_type: ConnectionType::NeoForge,
        payload_setup: NetworkPayloadSetup {
            channels: BTreeMap::from([(protocol, channels)]),
        },
        ..NegotiatedState::default()
    }
}

fn java_client(player: &Player) -> &JavaClient {
    let ClientPlatform::Java(client) = player.client.as_ref() else {
        panic!("not a Java player");
    };
    client
}

/// A serverbound play `custom_payload` packet.
fn play_payload(channel: &str, data: &[u8]) -> RawPacket {
    let mut payload = Vec::new();
    pumpkin_protocol::ClientPacket::write_packet_data(
        &SCustomPayload { channel, data },
        &mut payload,
        &CURRENT_MC_VERSION.into(),
    )
    .unwrap();
    RawPacket {
        id: SCustomPayload::to_id(CURRENT_MC_VERSION),
        payload: Bytes::from(payload),
    }
}

/// Hands `packet` to the play packet handler of `player`, as the world tick does.
fn handle_play(player: &Arc<Player>, packet: &RawPacket) {
    java_client(player)
        .handle_play_packet(player, server(), packet)
        .unwrap();
}

/// Scenario "Play to server" of `neoforge-network`, and the registrar settings.
#[test]
fn play_to_server_takes_part_in_the_negotiation_with_the_registrar_version() {
    let server = registered();
    let play = RUNTIME.block_on(
        server
            .network_registry
            .registrations(ConnectionProtocol::Play),
    );
    let find = |id: &str| {
        play.iter()
            .find(|r| r.id == Identifier::parse(id).unwrap())
            .unwrap()
            .clone()
    };
    let set_level = find(SET_LEVEL);
    assert_eq!(set_level.version, "1");
    assert_eq!(set_level.flow, Some(PacketFlow::Serverbound));
    assert!(!set_level.optional);
    assert_eq!(find(LEVEL).flow, Some(PacketFlow::Clientbound));
    assert!(find(OPTIONAL_PLAY).optional);
    let configuration = RUNTIME.block_on(
        server
            .network_registry
            .registrations(ConnectionProtocol::Configuration),
    );
    let licence = configuration
        .iter()
        .find(|r| r.id == Identifier::parse_static(LICENCE))
        .unwrap();
    assert_eq!(licence.version, "2");
    assert_eq!(licence.flow, None);
    // A play channel is not a configuration channel.
    assert!(
        !configuration
            .iter()
            .any(|r| r.id == Identifier::parse_static(SET_LEVEL))
    );
}

/// Requirement "Channel registration": `duplicate` and `registration-closed`, and the checks of
/// `NetworkRegistry.register`.
#[test]
fn registration_refuses_what_neoforge_refuses_and_closes_after_the_step() {
    registered();
    let results = RESULTS.lock().unwrap().clone();
    let result = |label: &str| results.iter().find(|(l, _)| *l == label).unwrap().1.clone();
    for label in ["set_level", "level", "optional_play", "network", "licence"] {
        assert_eq!(result(label), Ok(()), "{label}");
    }
    assert_eq!(
        result("duplicate"),
        Err(NetworkError::Duplicate(SET_LEVEL.to_owned()))
    );
    assert_eq!(
        result("neoforge duplicate"),
        Err(NetworkError::Duplicate("neoforge:split".to_owned()))
    );
    assert_eq!(
        result("vanilla"),
        Err(NetworkError::ReservedNamespace(
            "minecraft:nettest".to_owned()
        ))
    );
    assert_eq!(
        result("blank"),
        Err(NetworkError::BlankVersion("nettest:blank".to_owned()))
    );
    assert!(matches!(
        result("invalid"),
        Err(NetworkError::InvalidChannel(_))
    ));
    let late = LATE.lock().unwrap().clone().unwrap();
    assert_eq!(
        late.play_to_client("nettest:late").map(|_| ()),
        Err(NetworkError::RegistrationClosed)
    );
}

/// Scenario "Handler runs" of `neoforge-network`: per-channel dispatch on the world tick.
#[test]
fn a_play_payload_reaches_the_handler_of_its_channel_once() {
    registered();
    let (player, _peer) = player_with_peer(
        "NetHandler",
        negotiated(ConnectionProtocol::Play, &[SET_LEVEL, LEVEL]),
    );
    handle_play(&player, &play_payload(SET_LEVEL, &[3]));
    assert_eq!(received(SET_LEVEL), [(vec![3], "NetHandler".to_owned())]);
    assert_eq!(
        contexts(SET_LEVEL),
        [(ConnectionProtocol::Play, PacketFlow::Serverbound)]
    );
    assert!(received(OPTIONAL_PLAY).is_empty());
    assert!(!java_client(&player).is_closed());
}

/// Requirement "Payload context": a handler disconnects the sender with a reason.
#[test]
fn a_handler_disconnects_the_sender() {
    registered();
    let (player, _peer) = player_with_peer(
        "NetKick",
        negotiated(ConnectionProtocol::Play, &[SET_LEVEL, LEVEL]),
    );
    handle_play(&player, &play_payload(SET_LEVEL, KICK_ME));
    assert_eq!(
        received(SET_LEVEL),
        [(KICK_ME.to_vec(), "NetKick".to_owned())]
    );
    assert!(java_client(&player).is_closed());
}

/// Acceptance criterion: a payload on a required mod channel that the client did not negotiate
/// disconnects it, as `NetworkRegistry.handleModdedPayload` does. An optional channel is readable
/// ad hoc.
#[test]
fn a_payload_on_a_required_channel_the_client_did_not_negotiate_disconnects_it() {
    registered();
    let (optional, _peer) = player_with_peer("NetAdHoc", NegotiatedState::default());
    handle_play(&optional, &play_payload(OPTIONAL_PLAY, b"ad hoc"));
    assert_eq!(
        received(OPTIONAL_PLAY),
        [(b"ad hoc".to_vec(), "NetAdHoc".to_owned())]
    );
    assert!(!java_client(&optional).is_closed());

    let (vanilla, _peer) = player_with_peer("NetVanilla", NegotiatedState::default());
    handle_play(&vanilla, &play_payload(SET_LEVEL, &[1]));
    assert!(received(SET_LEVEL).is_empty());
    assert!(java_client(&vanilla).is_closed());
}

/// The client of `player` reads its packets until a play `custom_payload` on `channel`, and
/// returns its data.
fn read_play_payload(peer: tokio::net::TcpStream, channel: &str) -> Vec<u8> {
    RUNTIME.block_on(async {
        let mut decoder = TCPNetworkDecoder::new(tokio::io::BufReader::new(peer));
        loop {
            let packet = tokio::time::timeout(Duration::from_secs(10), decoder.get_raw_packet())
                .await
                .unwrap()
                .unwrap();
            if packet.id != CCustomPayload::to_id(CURRENT_MC_VERSION) {
                continue;
            }
            let mut body = &packet.payload[..];
            let packet_channel = body.get_str().unwrap();
            if &*packet_channel == channel {
                return body.to_vec();
            }
        }
    })
}

/// Scenario "Reply" of `neoforge-network`.
#[test]
fn a_reply_reaches_the_client_of_the_sender() {
    registered();
    let (player, peer) = player_with_peer(
        "NetReply",
        negotiated(ConnectionProtocol::Play, &[SET_LEVEL, LEVEL]),
    );
    handle_play(&player, &play_payload(SET_LEVEL, &[7, 8]));
    assert_eq!(read_play_payload(peer, LEVEL), [7, 8]);
}

/// Scenario "Vanilla client" of `neoforge-network`, and the broadcasts that skip such clients.
#[test]
fn send_to_player_fails_for_a_vanilla_client_and_broadcasts_skip_it() {
    let _serial = support::serial();
    registered();
    let (vanilla, _vanilla_peer) = player_with_peer("NetSendVanilla", NegotiatedState::default());
    assert_eq!(
        send_to_player(&vanilla, LEVEL, &[1]),
        Err(NetworkError::ChannelNotNegotiated(LEVEL.to_owned()))
    );
    // A vanilla channel needs no negotiation.
    assert_eq!(send_to_player(&vanilla, "minecraft:brand", &[0]), Ok(()));

    let (modded, peer) = player_with_peer(
        "NetSendModded",
        negotiated(ConnectionProtocol::Play, &[LEVEL]),
    );
    assert_eq!(send_to_player(&modded, LEVEL, &[2]), Ok(()));
    let world = support::world();
    let negotiated_players = world
        .players
        .load()
        .iter()
        .filter(|player| send_to_player(player, LEVEL, &[]).is_ok())
        .count();
    assert_eq!(negotiated_players, 1);
    assert_eq!(send_to_players_in_dimension(&world, LEVEL, &[3]), 1);
    assert_eq!(send_to_all_players(registered(), LEVEL, &[4]), 1);
    // The payload of `send_to_player` comes first.
    assert_eq!(read_play_payload(peer, LEVEL), [2]);
}

/// Requirement "Receiving payloads": a handler of `HandlerThread::Network` runs on the connection
/// task, and a handler of `HandlerThread::Main` waits for the world tick.
#[test]
fn a_network_thread_handler_runs_on_the_connection_task() {
    let server = registered();
    let (player, _peer) = player_with_peer(
        "NetNetwork",
        negotiated(ConnectionProtocol::Play, &[NETWORK_PLAY, SET_LEVEL]),
    );
    let client = java_client(&player);
    assert!(handle_play_payload_on_network(
        client,
        &player,
        server,
        &play_payload(NETWORK_PLAY, b"now")
    ));
    assert_eq!(
        received(NETWORK_PLAY),
        [(b"now".to_vec(), "NetNetwork".to_owned())]
    );
    assert!(!handle_play_payload_on_network(
        client,
        &player,
        server,
        &play_payload(SET_LEVEL, &[1])
    ));
    assert!(received(SET_LEVEL).is_empty());
}

/// A connection in the configuration state, and the client end of its socket.
fn configuring_connection() -> (PendingConnection, tokio::net::TcpStream) {
    RUNTIME.block_on(async {
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
            PacketRateLimiter::new(false, 0.0, 0.0),
            Arc::downgrade(server()),
        );
        connection.connection_state.store(ConnectionState::Config);
        (connection, client)
    })
}

/// The configuration packets that the client end received, until a disconnect: the custom
/// payload channels, then the disconnect reason.
fn read_until_disconnect(client: tokio::net::TcpStream) -> (Vec<String>, String) {
    RUNTIME.block_on(async {
        let mut decoder = TCPNetworkDecoder::new(tokio::io::BufReader::new(client));
        let mut channels = Vec::new();
        loop {
            let packet = tokio::time::timeout(Duration::from_secs(10), decoder.get_raw_packet())
                .await
                .unwrap()
                .unwrap();
            if packet.id == CConfigDisconnect::to_id(CURRENT_MC_VERSION) {
                let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(
                    std::io::Cursor::new(&packet.payload[..]),
                );
                let tag = pumpkin_nbt::tag::NbtTag::deserialize(&mut reader).unwrap();
                let reason = TextComponent::from_nbt(&tag);
                let text = serde_json::to_string(&reason.0).unwrap();
                return (channels, text);
            }
            let mut body = &packet.payload[..];
            if let Ok(channel) = body.get_str() {
                channels.push(channel.into());
            }
        }
    })
}

/// Scenario "Missing required channel" of `neoforge-network`: the server has required mod
/// channels, and the client's `neoforge:register` reply has none of them.
#[test]
fn a_neoforge_client_without_a_required_mod_channel_fails_the_negotiation() {
    let server = registered();
    let (mut connection, client) = configuring_connection();
    connection.neoforge_probe_pending = true;
    let mut query = Vec::new();
    ModdedNetworkQueryPayload::default()
        .write(&mut query)
        .unwrap();
    RUNTIME.block_on(async {
        assert!(
            connection
                .handle_neoforge_payload(server, ModdedNetworkQueryPayload::CHANNEL, &query)
                .await
                .unwrap()
        );
    });
    assert!(connection.is_closed());
    let (channels, reason) = read_until_disconnect(client);
    assert_eq!(channels, [ModdedNetworkSetupFailedPayload::CHANNEL]);
    assert!(
        reason.contains("multiplayer.disconnect.incompatible"),
        "{reason}"
    );
}

/// Scenario "Unexpected reply" of `neoforge-network`: the handler of the mod channel asks to
/// finish its task while no task waits, which disconnects the client with the message of the
/// vanilla `finishCurrentTask`.
#[test]
fn a_finish_request_while_no_mod_task_waits_disconnects_the_client() {
    let server = registered();
    let (mut connection, client) = configuring_connection();
    connection.payload_setup =
        negotiated(ConnectionProtocol::Configuration, &[LICENCE]).payload_setup;
    RUNTIME.block_on(async {
        connection
            .handle_plugin_message(
                server,
                SPluginMessage {
                    channel: LICENCE,
                    data: b"answer",
                },
            )
            .await
            .unwrap();
    });
    assert_eq!(received(LICENCE), [(b"answer".to_vec(), "-".to_owned())]);
    assert_eq!(
        contexts(LICENCE),
        [(ConnectionProtocol::Configuration, PacketFlow::Serverbound)]
    );
    assert!(connection.is_closed());
    let (_, reason) = read_until_disconnect(client);
    assert!(
        reason.contains(
            "Unexpected request for task finish, current task: null, requested: nettest:licence"
        ),
        "{reason}"
    );
}

/// Scenario "Unregistered channel" of `neoforge-network`: a configuration payload on a channel
/// that no mod registered reaches the v0.2 `config-custom-payload-event`, and a payload on a mod
/// channel reaches only its handler.
#[test]
fn a_configuration_payload_on_an_unregistered_channel_reaches_the_plugin_event() {
    struct Raw(Mutex<Vec<String>>);

    impl EventHandler<ConfigCustomPayloadEvent> for Raw {
        fn handle<'a>(
            &'a self,
            _server: &'a Arc<Server>,
            event: &'a ConfigCustomPayloadEvent,
        ) -> BoxFuture<'a, ()> {
            self.0.lock().unwrap().push(event.channel.clone());
            Box::pin(async {})
        }
    }

    let server = registered();
    let raw = Arc::new(Raw(Mutex::new(Vec::new())));
    let metadata = PluginMetadata {
        name: "nettest-raw".to_owned(),
        version: "1.0.0".to_owned(),
        authors: Vec::new(),
        description: String::new(),
        dependencies: Vec::new(),
        permissions: Vec::new(),
    };
    create_context(server, metadata).register_event_with::<ConfigCustomPayloadEvent, _>(
        Arc::clone(&raw),
        EventPriority::Normal,
        false,
        false,
    );
    let (mut connection, _client) = configuring_connection();
    connection.payload_setup =
        negotiated(ConnectionProtocol::Configuration, &[LICENCE]).payload_setup;
    // The handler of the mod channel asks to finish a task that is current.
    connection
        .configuration_tasks
        .add_mod_tasks(vec![Arc::new(Licence)]);
    assert!(connection.configuration_tasks.start_next().is_some());
    RUNTIME.block_on(async {
        for (channel, data) in [("other:raw", &b"raw"[..]), (LICENCE, b"answer")] {
            connection
                .handle_plugin_message(server, SPluginMessage { channel, data })
                .await
                .unwrap();
        }
    });
    assert_eq!(*raw.0.lock().unwrap(), ["other:raw"]);
    assert_eq!(received(LICENCE), [(b"answer".to_vec(), "-".to_owned())]);
    // Scenario "Reply to a mod task": the answer finished the task without a kick.
    assert!(!connection.is_closed());
    assert_eq!(connection.configuration_tasks.current(), None);
}

/// The licence task of [`LICENCE_TASK`], which waits for the answer.
struct Licence;

impl pumpkin_core::plugin::api::events::network::CustomConfigurationTask for Licence {
    fn task_type(&self) -> &'static str {
        LICENCE_TASK
    }

    fn run(&self, _context: &mut pumpkin_core::plugin::api::events::network::TaskContext<'_>) {}
}

#[test]
fn the_channels_of_a_connection_follow_has_channel() {
    let state = NegotiatedState {
        common_channels: BTreeMap::from([(
            ConnectionProtocol::Play,
            BTreeSet::from([Identifier::parse_static("a:common")]),
        )]),
        ad_hoc_channels: BTreeSet::from([Identifier::parse_static("a:ad_hoc")]),
        ..negotiated(ConnectionProtocol::Play, &["a:negotiated"])
    };
    let channels = pumpkin_core::plugin::network_events::negotiated_channels(&state);
    for channel in [
        "a:negotiated",
        "a:common",
        "a:ad_hoc",
        "minecraft:brand",
        "c:version",
    ] {
        assert_eq!(
            channels.check_send(ConnectionProtocol::Play, channel),
            Ok(()),
            "{channel}"
        );
    }
    assert_eq!(
        channels.check_send(ConnectionProtocol::Configuration, "a:common"),
        Err(NetworkError::ChannelNotNegotiated("a:common".to_owned()))
    );
    assert_eq!(channels.connection_type(), ConnectionType::NeoForge);
}
