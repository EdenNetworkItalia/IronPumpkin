//! The mod network API (`openspec/changes/neoforge-shaped-plugin-api`, spec `neoforge-network`)
//! against a server of the support module: the payload registration step, the negotiation of a
//! required channel, the per-channel dispatch in play and in configuration, the payload context,
//! the send helpers and the finish request of a mod task.
//!
//! The configuration task scenarios that need a client run in the boot test `neoforge_network`
//! of `ironpumpkin-test-mod`, with the headless client.
//!
//! The tests share one server and the process-global records of the handlers, so each test
//! reads only the records of its own player, or of its own data in the configuration phase.
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
        api::events::{
            network::{
                ConnectionProtocol, HandlerThread, NetworkError, PacketFlow, PayloadContext,
                PayloadRegistrar, RegisterPayloadHandlersEvent,
            },
            player::player_custom_payload::PlayerCustomPayloadEvent,
        },
        network_events::{
            register_payload_handlers, send_to_all_players, send_to_player,
            send_to_players_in_dimension,
        },
        server::config_custom_payload::ConfigCustomPayloadEvent,
        startup::create_context,
    },
    server::Server,
};
use pumpkin_data::{packet::CURRENT_MC_VERSION, translation};
use pumpkin_protocol::{
    ClientPacket, ConnectionState, RawPacket,
    java::{
        client::{
            config::CConfigDisconnect,
            play::{CCustomPayload, CPlayDisconnect},
        },
        neoforge::{
            ModdedNetworkQueryPayload, ModdedNetworkSetupFailedPayload, NetworkChannel,
            NetworkPayloadSetup,
        },
        packet_decoder::TCPNetworkDecoder,
        packet_encoder::TCPNetworkEncoder,
        server::{config::SPluginMessage, play::SCustomPayload},
    },
    packet::MultiVersionJavaPacket,
    ser::NetworkReadExt,
};
use pumpkin_util::{identifier::Identifier, text::TextComponent};
use support::{RUNTIME, player_with_peer, server};

const SET_LEVEL: &str = "nettest:set_level";
const LEVEL: &str = "nettest:level";
const BROADCAST: &str = "nettest:broadcast";
const OPTIONAL_PLAY: &str = "nettest:optional_play";
const NETWORK_PLAY: &str = "nettest:network";
const LICENCE: &str = "nettest:licence";
const LICENCE_TASK: &str = "nettest:licence";
const COMMON_BIDIRECTIONAL: &str = "nettest:common";
const COMMON_TO_SERVER: &str = "nettest:common_to_server";
const COMMON_TO_CLIENT: &str = "nettest:common_to_client";
const CONFIGURATION_TO_SERVER: &str = "nettest:configuration_to_server";
const CONFIGURATION_TO_CLIENT: &str = "nettest:configuration_to_client";
/// The version that `NeoForge` 26.3 puts in its `multiplayer.disconnect.incompatible` reasons.
const NEOFORGE: &str = "NeoForge 26.3.0.64-beta";

/// What the handlers received: the channel, the bytes, the player name or `-` without one, the
/// protocol and the flow.
type Received = (String, Vec<u8>, String, ConnectionProtocol, PacketFlow);
static RECEIVED: Mutex<Vec<Received>> = Mutex::new(Vec::new());
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
    RECEIVED.lock().unwrap().push((
        context.channel().to_owned(),
        data.to_vec(),
        player,
        context.protocol(),
        context.flow(),
    ));
}

/// The play payloads that the handler of `channel` received from `player`: the bytes, the
/// protocol and the flow.
fn received(channel: &str, player: &str) -> Vec<(Vec<u8>, ConnectionProtocol, PacketFlow)> {
    RECEIVED
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, _, p, ..)| c == channel && p == player)
        .map(|(_, data, _, protocol, flow)| (data.clone(), *protocol, *flow))
        .collect()
}

/// The configuration payloads with `data` that the handler of `channel` received. A configuration
/// payload has no player, so each test sends its own `data`.
fn configuration_received(channel: &str, data: &[u8]) -> Vec<(ConnectionProtocol, PacketFlow)> {
    RECEIVED
        .lock()
        .unwrap()
        .iter()
        .filter(|(c, d, p, ..)| c == channel && d == data && p == "-")
        .map(|(.., protocol, flow)| (*protocol, *flow))
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
        record("broadcast", registrar.play_to_client(BROADCAST));
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
        record(
            "common_bidirectional",
            registrar.common_bidirectional(COMMON_BIDIRECTIONAL, receive),
        );
        record(
            "common_to_server",
            registrar.common_to_server(COMMON_TO_SERVER, receive),
        );
        record(
            "common_to_client",
            registrar.common_to_client(COMMON_TO_CLIENT),
        );
        record(
            "configuration_to_server",
            registrar.configuration_to_server(CONFIGURATION_TO_SERVER, receive),
        );
        record(
            "configuration_to_client",
            registrar.configuration_to_client(CONFIGURATION_TO_CLIENT),
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
    create_context(server, metadata("nettest"))
        .register_event_with::<RegisterPayloadHandlersEvent, _>(
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

fn metadata(name: &str) -> PluginMetadata {
    PluginMetadata {
        name: name.to_owned(),
        version: "1.0.0".to_owned(),
        authors: Vec::new(),
        description: String::new(),
        dependencies: Vec::new(),
        permissions: Vec::new(),
    }
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

/// The `multiplayer.disconnect.incompatible` reason of `NetworkRegistry.handleModdedPayload`.
fn incompatible(detail: &str) -> TextComponent {
    translated(
        translation::java::MULTIPLAYER_DISCONNECT_INCOMPATIBLE,
        &format!("{NEOFORGE} ({detail})"),
    )
}

/// The Java-only translation that the server sends, with one text argument.
#[expect(
    deprecated,
    reason = "the server builds these reasons with `TextComponent::translate`, not the cross macro"
)]
fn translated(key: &'static str, argument: &str) -> TextComponent {
    TextComponent::translate(key, [TextComponent::text(argument.to_owned())])
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
    ClientPacket::write_packet_data(
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

/// Records the channel of each `PlayerCustomPayloadEvent`, the plugin event of a play payload
/// that no mod handler took, with the name of the player.
struct PlayEvents(Mutex<Vec<(String, String)>>);

impl EventHandler<PlayerCustomPayloadEvent> for PlayEvents {
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a PlayerCustomPayloadEvent,
    ) -> BoxFuture<'a, ()> {
        self.0
            .lock()
            .unwrap()
            .push((event.player.gameprofile.name.clone(), event.channel.clone()));
        Box::pin(async {})
    }
}

/// Records the channel of each `ConfigCustomPayloadEvent`, the plugin event of a configuration
/// payload that no mod handler took.
struct ConfigurationEvents(Mutex<Vec<String>>);

impl EventHandler<ConfigCustomPayloadEvent> for ConfigurationEvents {
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a ConfigCustomPayloadEvent,
    ) -> BoxFuture<'a, ()> {
        self.0.lock().unwrap().push(event.channel.clone());
        Box::pin(async {})
    }
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

/// Requirement "Channel registration": the `common*` calls register the channel for both phases,
/// the `configuration*` calls for the configuration phase only, and the handler of a common
/// channel gets its payloads in both phases.
#[test]
fn common_channels_register_in_both_phases_and_configuration_channels_in_one() {
    let server = registered();
    let flows = |protocol| {
        RUNTIME
            .block_on(server.network_registry.registrations(protocol))
            .into_iter()
            .map(|r| (r.id.to_string(), r.flow))
            .collect::<BTreeMap<_, _>>()
    };
    let play = flows(ConnectionProtocol::Play);
    let configuration = flows(ConnectionProtocol::Configuration);
    for (channel, flow) in [
        (COMMON_BIDIRECTIONAL, None),
        (COMMON_TO_SERVER, Some(PacketFlow::Serverbound)),
        (COMMON_TO_CLIENT, Some(PacketFlow::Clientbound)),
    ] {
        assert_eq!(play.get(channel), Some(&flow), "{channel}");
        assert_eq!(configuration.get(channel), Some(&flow), "{channel}");
    }
    for (channel, flow) in [
        (CONFIGURATION_TO_SERVER, Some(PacketFlow::Serverbound)),
        (CONFIGURATION_TO_CLIENT, Some(PacketFlow::Clientbound)),
    ] {
        assert_eq!(configuration.get(channel), Some(&flow), "{channel}");
        assert_eq!(play.get(channel), None, "{channel}");
    }

    let (player, _peer) = player_with_peer(
        "NetCommon",
        negotiated(ConnectionProtocol::Play, &[COMMON_BIDIRECTIONAL]),
    );
    handle_play(&player, &play_payload(COMMON_BIDIRECTIONAL, b"common play"));
    assert_eq!(
        received(COMMON_BIDIRECTIONAL, "NetCommon"),
        [(
            b"common play".to_vec(),
            ConnectionProtocol::Play,
            PacketFlow::Serverbound
        )]
    );
    assert!(!java_client(&player).is_closed());

    let (mut connection, _client) = configuring_connection();
    connection.payload_setup = negotiated(
        ConnectionProtocol::Configuration,
        &[COMMON_BIDIRECTIONAL, CONFIGURATION_TO_SERVER],
    )
    .payload_setup;
    let payloads = [
        (COMMON_BIDIRECTIONAL, &b"common configuration"[..]),
        (CONFIGURATION_TO_SERVER, b"configuration to server"),
    ];
    RUNTIME.block_on(async {
        for (channel, data) in payloads {
            connection
                .handle_plugin_message(server, SPluginMessage { channel, data })
                .await
                .unwrap();
        }
    });
    for (channel, data) in payloads {
        assert_eq!(
            configuration_received(channel, data),
            [(ConnectionProtocol::Configuration, PacketFlow::Serverbound)],
            "{channel}"
        );
    }
    assert!(!connection.is_closed());
}

/// Requirement "Channel registration": `duplicate` and `registration-closed`, and the checks of
/// `NetworkRegistry.register`.
#[test]
fn registration_refuses_what_neoforge_refuses_and_closes_after_the_step() {
    registered();
    let results = RESULTS.lock().unwrap().clone();
    let result = |label: &str| results.iter().find(|(l, _)| *l == label).unwrap().1.clone();
    for label in [
        "set_level",
        "level",
        "broadcast",
        "optional_play",
        "network",
        "licence",
        "common_bidirectional",
        "common_to_server",
        "common_to_client",
        "configuration_to_server",
        "configuration_to_client",
    ] {
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
    assert_eq!(
        received(SET_LEVEL, "NetHandler"),
        [(vec![3], ConnectionProtocol::Play, PacketFlow::Serverbound)]
    );
    assert!(received(OPTIONAL_PLAY, "NetHandler").is_empty());
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
        received(SET_LEVEL, "NetKick"),
        [(
            KICK_ME.to_vec(),
            ConnectionProtocol::Play,
            PacketFlow::Serverbound
        )]
    );
    assert!(java_client(&player).is_closed());
}

/// Acceptance criterion: a payload on a required mod channel that the client did not negotiate
/// disconnects it with the reason of `NetworkRegistry.handleModdedPayload`. An optional channel
/// is readable ad hoc.
#[test]
fn a_payload_on_a_required_channel_the_client_did_not_negotiate_disconnects_it() {
    registered();
    let (optional, _peer) = player_with_peer("NetAdHoc", NegotiatedState::default());
    handle_play(&optional, &play_payload(OPTIONAL_PLAY, b"ad hoc"));
    assert_eq!(
        received(OPTIONAL_PLAY, "NetAdHoc"),
        [(
            b"ad hoc".to_vec(),
            ConnectionProtocol::Play,
            PacketFlow::Serverbound
        )]
    );
    assert!(!java_client(&optional).is_closed());

    let (vanilla, peer) = player_with_peer("NetVanilla", NegotiatedState::default());
    // The support joins the player without a login, and a kick sends its packet only in play.
    java_client(&vanilla)
        .connection_state
        .store(ConnectionState::Play);
    handle_play(&vanilla, &play_payload(SET_LEVEL, &[1]));
    assert!(received(SET_LEVEL, "NetVanilla").is_empty());
    assert!(java_client(&vanilla).is_closed());
    let (_, reason) = read_until_disconnect(peer, CPlayDisconnect::to_id(CURRENT_MC_VERSION));
    assert_eq!(reason, incompatible(&format!("No Channel for {SET_LEVEL}")));
}

/// Requirement "Receiving payloads": `getCodec` refuses to decode a payload on a client-bound
/// channel, so the server drops it: no handler, no plugin event and no disconnect, in play and
/// in the configuration phase, also before the channel negotiation.
#[test]
fn a_payload_on_a_client_bound_mod_channel_is_dropped() {
    let server = registered();
    let play_events = Arc::new(PlayEvents(Mutex::new(Vec::new())));
    create_context(server, metadata("nettest-play-events"))
        .register_event_with::<PlayerCustomPayloadEvent, _>(
            Arc::clone(&play_events),
            EventPriority::Normal,
            false,
            false,
        );
    let configuration_events = Arc::new(ConfigurationEvents(Mutex::new(Vec::new())));
    create_context(server, metadata("nettest-configuration-events"))
        .register_event_with::<ConfigCustomPayloadEvent, _>(
            Arc::clone(&configuration_events),
            EventPriority::Normal,
            false,
            false,
        );

    let (player, _peer) = player_with_peer(
        "NetDropped",
        negotiated(ConnectionProtocol::Play, &[LEVEL, COMMON_TO_CLIENT]),
    );
    for channel in [LEVEL, COMMON_TO_CLIENT, "other:raw"] {
        handle_play(&player, &play_payload(channel, b"dropped"));
    }
    let events: Vec<String> = play_events
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|(name, _)| name == "NetDropped")
        .map(|(_, channel)| channel.clone())
        .collect();
    // The channel that no mod registered shows that the observer works.
    assert_eq!(events, ["other:raw"]);
    assert!(received(LEVEL, "NetDropped").is_empty());
    assert!(!java_client(&player).is_closed());

    let (mut connection, _client) = configuring_connection();
    // No payload setup yet: a mod channel that the client may send on would kick.
    connection.neoforge_probe_pending = true;
    RUNTIME.block_on(async {
        for channel in [CONFIGURATION_TO_CLIENT, COMMON_TO_CLIENT] {
            connection
                .handle_plugin_message(
                    server,
                    SPluginMessage {
                        channel,
                        data: b"dropped",
                    },
                )
                .await
                .unwrap();
        }
    });
    assert!(!connection.is_closed());
    let events = configuration_events.0.lock().unwrap().clone();
    assert!(
        !events
            .iter()
            .any(|channel| channel == CONFIGURATION_TO_CLIENT || channel == COMMON_TO_CLIENT),
        "{events:?}"
    );
}

/// Requirement "Receiving payloads": a payload on a mod channel before the client's
/// `neoforge:register` reply finds no payload setup, and `handleModdedPayload` disconnects with
/// "No Payload Setup". After the reply, while the probe pong is still on its way, the payload
/// reaches its handler.
#[test]
fn a_mod_payload_before_the_negotiation_disconnects_with_no_payload_setup() {
    let server = registered();
    let (mut early, client) = configuring_connection();
    early.neoforge_probe_pending = true;
    RUNTIME.block_on(async {
        early
            .handle_plugin_message(
                server,
                SPluginMessage {
                    channel: COMMON_BIDIRECTIONAL,
                    data: b"too early",
                },
            )
            .await
            .unwrap();
    });
    assert!(configuration_received(COMMON_BIDIRECTIONAL, b"too early").is_empty());
    assert!(early.is_closed());
    let (_, reason) = read_until_disconnect(client, CConfigDisconnect::to_id(CURRENT_MC_VERSION));
    assert_eq!(reason, incompatible("No Payload Setup"));

    let (mut registered, _client) = configuring_connection();
    registered.neoforge_probe_pending = true;
    registered.client_channels.connection_type = ConnectionType::NeoForge;
    registered.payload_setup =
        negotiated(ConnectionProtocol::Configuration, &[COMMON_BIDIRECTIONAL]).payload_setup;
    RUNTIME.block_on(async {
        registered
            .handle_plugin_message(
                server,
                SPluginMessage {
                    channel: COMMON_BIDIRECTIONAL,
                    data: b"after the register reply",
                },
            )
            .await
            .unwrap();
    });
    assert_eq!(
        configuration_received(COMMON_BIDIRECTIONAL, b"after the register reply"),
        [(ConnectionProtocol::Configuration, PacketFlow::Serverbound)]
    );
    assert!(!registered.is_closed());
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
/// Only the players of this test negotiate [`BROADCAST`].
#[test]
fn send_to_player_fails_for_a_vanilla_client_and_broadcasts_skip_it() {
    registered();
    let (vanilla, _vanilla_peer) = player_with_peer("NetSendVanilla", NegotiatedState::default());
    assert_eq!(
        send_to_player(&vanilla, BROADCAST, &[1]),
        Err(NetworkError::ChannelNotNegotiated(BROADCAST.to_owned()))
    );
    // A vanilla channel needs no negotiation.
    assert_eq!(send_to_player(&vanilla, "minecraft:brand", &[0]), Ok(()));

    let (modded, peer) = player_with_peer(
        "NetSendModded",
        negotiated(ConnectionProtocol::Play, &[BROADCAST]),
    );
    assert_eq!(send_to_player(&modded, BROADCAST, &[2]), Ok(()));
    let world = support::world();
    assert_eq!(send_to_players_in_dimension(&world, BROADCAST, &[3]), 1);
    assert_eq!(send_to_all_players(registered(), BROADCAST, &[4]), 1);
    // The payload of `send_to_player` comes first.
    assert_eq!(read_play_payload(peer, BROADCAST), [2]);
}

/// Requirement "Receiving payloads": the read loop of the connection hands a payload on a
/// channel of `HandlerThread::Network` to its handler at once, and queues a payload on a channel
/// of `HandlerThread::Main` for the world tick.
#[test]
fn a_network_thread_handler_runs_on_the_read_loop_and_a_main_handler_waits_for_the_tick() {
    let server = registered();
    let (player, peer) = player_with_peer(
        "NetReadLoop",
        negotiated(ConnectionProtocol::Play, &[NETWORK_PLAY, SET_LEVEL, LEVEL]),
    );
    let (_read, write) = peer.into_split();
    let read_loop = {
        let player = Arc::clone(&player);
        RUNTIME.spawn(async move {
            java_client(&player)
                .progress_player_packets(&player, server)
                .await;
        })
    };
    RUNTIME.block_on(async {
        let mut encoder = TCPNetworkEncoder::new(write);
        // The read loop takes the packets in order, so the main packet is queued when the
        // network handler runs.
        for (channel, data) in [(SET_LEVEL, &[5][..]), (NETWORK_PLAY, b"read loop")] {
            let packet = SCustomPayload { channel, data }
                .serialize_packet(&CURRENT_MC_VERSION.into())
                .unwrap();
            encoder.write_packet(packet).await.unwrap();
        }
        encoder.flush().await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while received(NETWORK_PLAY, "NetReadLoop").is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        // The end of the stream ends the read loop.
        drop(encoder);
        tokio::time::timeout(Duration::from_secs(10), read_loop)
            .await
            .unwrap()
            .unwrap();
    });
    assert_eq!(
        received(NETWORK_PLAY, "NetReadLoop"),
        [(
            b"read loop".to_vec(),
            ConnectionProtocol::Play,
            PacketFlow::Serverbound
        )]
    );
    assert!(received(SET_LEVEL, "NetReadLoop").is_empty());
    let queued = player.inbound_packets.pop().unwrap();
    assert!(player.inbound_packets.pop().is_none());
    handle_play(&player, &queued);
    assert_eq!(
        received(SET_LEVEL, "NetReadLoop"),
        [(vec![5], ConnectionProtocol::Play, PacketFlow::Serverbound)]
    );
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

/// The packets that the client end received until the disconnect packet `disconnect_id`: the
/// custom payload channels, then the disconnect reason.
fn read_until_disconnect(
    client: tokio::net::TcpStream,
    disconnect_id: i32,
) -> (Vec<String>, TextComponent) {
    RUNTIME.block_on(async {
        let mut decoder = TCPNetworkDecoder::new(tokio::io::BufReader::new(client));
        let mut channels = Vec::new();
        loop {
            let packet = tokio::time::timeout(Duration::from_secs(10), decoder.get_raw_packet())
                .await
                .unwrap()
                .unwrap();
            if packet.id == disconnect_id {
                let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(
                    std::io::Cursor::new(&packet.payload[..]),
                );
                let tag = pumpkin_nbt::tag::NbtTag::deserialize(&mut reader).unwrap();
                return (channels, TextComponent::from_nbt(&tag));
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
    let (channels, reason) =
        read_until_disconnect(client, CConfigDisconnect::to_id(CURRENT_MC_VERSION));
    assert_eq!(channels, [ModdedNetworkSetupFailedPayload::CHANNEL]);
    let reason = serde_json::to_string(&reason.0).unwrap();
    assert!(
        reason.contains("multiplayer.disconnect.incompatible"),
        "{reason}"
    );
}

/// Scenario "Unexpected reply" of `neoforge-network`: the handler of the mod channel asks to
/// finish its task while no task waits. The `IllegalStateException` of `finishCurrentTask`
/// reaches `Connection.exceptionCaught`, which disconnects the client with
/// `disconnect.genericReason`.
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
                    data: b"stray answer",
                },
            )
            .await
            .unwrap();
    });
    assert_eq!(
        configuration_received(LICENCE, b"stray answer"),
        [(ConnectionProtocol::Configuration, PacketFlow::Serverbound)]
    );
    assert!(connection.is_closed());
    let (_, reason) = read_until_disconnect(client, CConfigDisconnect::to_id(CURRENT_MC_VERSION));
    assert_eq!(
        reason,
        translated(
            translation::java::DISCONNECT_GENERICREASON,
            "Internal Exception: java.lang.IllegalStateException: Unexpected request for task \
             finish, current task: null, requested: nettest:licence"
        )
    );
}

/// Scenario "Unregistered channel" of `neoforge-network`: a configuration payload on a channel
/// that no mod registered reaches the v0.2 `config-custom-payload-event`, and a payload on a mod
/// channel reaches only its handler.
#[test]
fn a_configuration_payload_on_an_unregistered_channel_reaches_the_plugin_event() {
    let server = registered();
    let raw = Arc::new(ConfigurationEvents(Mutex::new(Vec::new())));
    create_context(server, metadata("nettest-raw"))
        .register_event_with::<ConfigCustomPayloadEvent, _>(
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
    let events = raw.0.lock().unwrap().clone();
    assert!(
        events.iter().any(|channel| channel == "other:raw"),
        "{events:?}"
    );
    assert!(
        !events.iter().any(|channel| channel == LICENCE),
        "{events:?}"
    );
    assert_eq!(
        configuration_received(LICENCE, b"answer"),
        [(ConnectionProtocol::Configuration, PacketFlow::Serverbound)]
    );
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
