//! One client connection: handshake, offline login and the configuration phase.
//!
//! The configuration phase answers like the vanilla client, plus the `NeoForge` client behaviour of
//! `ClientConfigurationPacketListenerImpl`, `ClientCommonPacketListenerImpl` and
//! `ClientNetworkRegistry` when a channel map is set.

use std::collections::{BTreeSet, VecDeque};
use std::time::Duration;

use bytes::Bytes;
use pumpkin_data::packet::{
    CURRENT_MC_VERSION, PacketId,
    clientbound::{config, login},
};
use pumpkin_protocol::{
    ClientPacket, ConnectionState, RawPacket, ServerPacket, VarInt,
    java::{
        client::{
            config::CConfigPing,
            login::{CEncryptionRequest, CLoginDisconnect, CSetCompression},
        },
        neoforge::{
            CommonRegisterPayload, CommonVersionPayload, ConnectionProtocol,
            ExtensibleEnumAcknowledgePayload, ExtensibleEnumDataPayload,
            FeatureFlagAcknowledgePayload, FeatureFlagDataPayload, FrozenRegistryPayload,
            FrozenRegistrySyncCompletedPayload, FrozenRegistrySyncStartPayload,
            KnownRegistryDataMapsPayload, KnownRegistryDataMapsReplyPayload,
            MinecraftRegisterPayload, MinecraftUnregisterPayload, ModdedNetworkPayload,
            ModdedNetworkQueryPayload, ModdedNetworkSetupFailedPayload, PacketFlow,
            SplitPacketPayload, decode_exact, decode_payload,
        },
        packet_decoder::TCPNetworkDecoder,
        packet_encoder::TCPNetworkEncoder,
        server::{
            config::{
                SAcceptCodeOfConduct, SAcknowledgeFinishConfig, SClientInformationConfig,
                SConfigCookieResponse, SConfigPong, SKeepAlive, SKnownPacks, SPluginMessage,
            },
            handshake::SHandShake,
            login::{
                SEncryptionResponse, SLoginAcknowledged, SLoginCookieResponse,
                SLoginPluginResponse, SLoginStart,
            },
        },
    },
    ser::{NetworkReadExt, NetworkReadSliceExt, NetworkWriteExt},
};
use pumpkin_util::{identifier::Identifier, uuid::offline_player_uuid};
use rsa::{Pkcs1v15Encrypt, RsaPublicKey, pkcs8::DecodePublicKey};
use tokio::{
    io::{BufReader, BufWriter},
    net::{
        TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
};
use tracing::{debug, info, warn};

use crate::{
    Error,
    channels::ChannelMap,
    record::{
        Decode, Direction, Entry, State, decode_payload as describe_payload, read_disconnect_reason,
    },
};

/// Compression level for the packets this client sends. Any zlib level is valid.
const COMPRESSION_LEVEL: u32 = 6;
/// `GenericPacketSplitter.STATE_LAST`.
const SPLIT_STATE_LAST: u8 = 2;
/// `GenericPacketSplitter.STATE_FIRST`.
const SPLIT_STATE_FIRST: u8 = 1;
/// `NetworkRegistry.SUPPORTED_COMMON_NETWORKING_VERSIONS`.
const COMMON_NETWORKING_VERSION: i32 = 1;

const BRAND: &str = "minecraft:brand";

/// What to connect to and how to behave.
#[derive(Debug, Clone)]
pub struct Options {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// The brand sent in `minecraft:brand`: `vanilla` for a vanilla client, `neoforge` for `NeoForge`.
    pub brand: String,
    /// When set, the client also behaves like a `NeoForge` client with these channels.
    pub channels: Option<ChannelMap>,
    pub timeout: Duration,
}

/// How the session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The server sent `finish_configuration` and the client acknowledged it.
    Finished,
    /// The server ended the login, or the client's own `NeoForge` checks ended the connection.
    Disconnected(String),
    /// The server sent a configuration `disconnect` with this reason.
    Kicked(pumpkin_util::text::TextComponent),
}

/// `ConnectionType` of `NeoForge`'s client listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionType {
    Other,
    NeoForge,
}

/// The clientbound login packets this client knows by name.
const LOGIN_PACKETS: [(PacketId, &str); 6] = [
    (login::COOKIE_REQUEST, "cookie_request"),
    (login::CUSTOM_QUERY, "custom_query"),
    (login::HELLO, "hello"),
    (login::LOGIN_COMPRESSION, "login_compression"),
    (login::LOGIN_DISCONNECT, "login_disconnect"),
    (login::LOGIN_FINISHED, "login_finished"),
];

/// The clientbound configuration packets this client knows by name.
const CONFIG_PACKETS: [(PacketId, &str); 21] = [
    (config::CLEAR_DIALOG, "clear_dialog"),
    (config::CODE_OF_CONDUCT, "code_of_conduct"),
    (config::COOKIE_REQUEST, "cookie_request"),
    (config::CUSTOM_PAYLOAD, "custom_payload"),
    (config::CUSTOM_REPORT_DETAILS, "custom_report_details"),
    (config::DISCONNECT, "disconnect"),
    (config::FINISH_CONFIGURATION, "finish_configuration"),
    (config::KEEP_ALIVE, "keep_alive"),
    (config::PING, "ping"),
    (config::POST_EFFECTS, "post_effects"),
    (config::REGISTRY_DATA, "registry_data"),
    (config::RESET_CHAT, "reset_chat"),
    (config::RESOURCE_PACK_POP, "resource_pack_pop"),
    (config::RESOURCE_PACK_PUSH, "resource_pack_push"),
    (config::SELECT_KNOWN_PACKS, "select_known_packs"),
    (config::SERVER_LINKS, "server_links"),
    (config::SHOW_DIALOG, "show_dialog"),
    (config::STORE_COOKIE, "store_cookie"),
    (config::TRANSFER, "transfer"),
    (config::UPDATE_ENABLED_FEATURES, "update_enabled_features"),
    (config::UPDATE_TAGS, "update_tags"),
];

fn packet_name(table: &[(PacketId, &str)], id: i32) -> String {
    table
        .iter()
        .find(|(packet_id, _)| *packet_id == id)
        .map_or_else(
            || format!("unknown_0x{id:02x}"),
            |(_, name)| (*name).to_owned(),
        )
}

/// The channel and data of a configuration `custom_payload` body. Vanilla's 1 MiB clientbound
/// bound does not apply to `neoforge:split`, which `NeoForge` reads with its own codec.
fn read_custom_payload(body: &[u8]) -> Result<(&str, &[u8]), Error> {
    if let Some(data) = SplitPacketPayload::data_of(body) {
        return Ok((SplitPacketPayload::CHANNEL, data));
    }
    let payload = SPluginMessage::read(&mut &body[..], &CURRENT_MC_VERSION)?;
    Ok((payload.channel, payload.data))
}

/// `GenericPacketSplitter.receivedPacket`: the slices form a whole packet, id included. Like
/// `NeoForge`, the joined size has no cap, and bytes after the body are ignored: the last part of
/// a `NeoForge` sender carries the unused capacity of its buffer (see `SplitPacketReassembler`).
fn join_split(buffer: &mut Vec<u8>, data: &[u8]) -> Result<Option<RawPacket>, Error> {
    let slice = decode_exact(data, SplitPacketPayload::read)?.payload;
    let Some((&state, content)) = slice.split_first() else {
        return Err("empty neoforge:split slice".into());
    };
    if state == SPLIT_STATE_FIRST && !buffer.is_empty() {
        warn!("neoforge:split received out of order, dropping the earlier slices");
        buffer.clear();
    }
    buffer.extend_from_slice(content);
    if state != SPLIT_STATE_LAST {
        return Ok(None);
    }
    let full = std::mem::take(buffer);
    let mut read = &full[..];
    let id = read.get_var_int()?.0;
    debug!(id, len = full.len(), "reassembled a split packet");
    Ok(Some(RawPacket {
        id,
        payload: Bytes::copy_from_slice(read),
    }))
}

/// A player name such as `Probe042117`, unique enough for back-to-back runs.
#[must_use]
pub fn random_username() -> String {
    format!("Probe{:06}", rand::random::<u32>() % 1_000_000)
}

/// Connects, logs in and runs the configuration phase. Every recorded packet goes to `sink` as
/// soon as it is seen. `options.timeout` covers the connect too.
pub async fn run(options: &Options, sink: &mut dyn FnMut(&Entry)) -> Result<Outcome, Error> {
    tokio::time::timeout(options.timeout, connect_and_drive(options, sink))
        .await
        .map_err(|_| format!("no outcome within {:?}", options.timeout))?
}

async fn connect_and_drive(
    options: &Options,
    sink: &mut dyn FnMut(&Entry),
) -> Result<Outcome, Error> {
    let stream = TcpStream::connect((options.host.as_str(), options.port))
        .await
        .map_err(|e| format!("cannot connect to {}:{}: {e}", options.host, options.port))?;
    stream.set_nodelay(true)?;
    let (read, write) = stream.into_split();
    Session {
        options,
        sink,
        seq: 0,
        decoder: TCPNetworkDecoder::new(BufReader::new(read)),
        encoder: TCPNetworkEncoder::new(BufWriter::new(write)),
        connection_type: ConnectionType::Other,
        setup: None,
        adhoc_channels: BTreeSet::new(),
        to_synchronize: BTreeSet::new(),
        split_buffer: Vec::new(),
        pending: VecDeque::new(),
    }
    .drive()
    .await
}

struct Session<'a> {
    options: &'a Options,
    sink: &'a mut dyn FnMut(&Entry),
    seq: usize,
    decoder: TCPNetworkDecoder<BufReader<OwnedReadHalf>>,
    encoder: TCPNetworkEncoder<BufWriter<OwnedWriteHalf>>,
    connection_type: ConnectionType,
    /// Configuration channels of the `NetworkPayloadSetup`. `None` until the connection is
    /// initialized (`ClientNetworkRegistry.isConnectionInitialized`); empty for a non-`NeoForge`
    /// server.
    setup: Option<BTreeSet<Identifier>>,
    /// Channels the server registered with `minecraft:register` (`NetworkRegistry.onMinecraftRegister`).
    adhoc_channels: BTreeSet<Identifier>,
    /// Registries announced by `frozen_registry_sync_start` and not received yet
    /// (`ClientPayloadHandler.toSynchronize`).
    to_synchronize: BTreeSet<Identifier>,
    split_buffer: Vec<u8>,
    /// Packets reassembled from `neoforge:split`, handled before the next read.
    pending: VecDeque<RawPacket>,
}

impl Session<'_> {
    async fn drive(&mut self) -> Result<Outcome, Error> {
        if let Some(outcome) = self.login().await? {
            return Ok(outcome);
        }
        self.configuration().await
    }

    async fn send(&mut self, packet: &impl ClientPacket) -> Result<(), Error> {
        let bytes = packet.serialize_packet(&CURRENT_MC_VERSION)?;
        self.encoder.write_packet(bytes).await?;
        self.encoder.flush().await?;
        Ok(())
    }

    async fn send_payload(&mut self, channel: &str, data: &[u8]) -> Result<(), Error> {
        self.record(
            Direction::Serverbound,
            State::Configuration,
            "custom_payload".to_owned(),
            Some(channel),
            data,
            false,
        );
        self.send(&SPluginMessage { channel, data }).await
    }

    async fn send_channels(
        &mut self,
        channel: &str,
        channels: BTreeSet<Identifier>,
    ) -> Result<(), Error> {
        let mut data = Vec::new();
        if channel == MinecraftUnregisterPayload::CHANNEL {
            MinecraftUnregisterPayload { channels }.write(&mut data)?;
        } else {
            MinecraftRegisterPayload { channels }.write(&mut data)?;
        }
        self.send_payload(channel, &data).await
    }

    /// Sends a modded reply after `NetworkRegistry.checkPacket`: the server must have the channel
    /// in the payload setup or as an ad hoc channel, or the real client throws and disconnects.
    async fn reply(&mut self, channel: &str, data: &[u8]) -> Result<Option<Outcome>, Error> {
        let id = Identifier::parse(channel).map_err(|e| e.to_string())?;
        let negotiated = self.setup.as_ref().is_some_and(|s| s.contains(&id));
        if !negotiated && !self.adhoc_channels.contains(&id) {
            return Ok(Some(Outcome::Disconnected(format!(
                "client: payload {channel} may not be sent to the server"
            ))));
        }
        self.send_payload(channel, data).await?;
        Ok(None)
    }

    fn record(
        &mut self,
        direction: Direction,
        state: State,
        packet: String,
        channel: Option<&str>,
        data: &[u8],
        reassembled: bool,
    ) {
        let (decode, summary, error) = channel.map_or((Decode::NotPayload, None, None), |c| {
            describe_payload(c, data, reassembled)
        });
        let entry = Entry {
            seq: self.seq,
            direction,
            state,
            packet,
            channel: channel.map(str::to_owned),
            length: data.len(),
            reassembled,
            decode,
            summary,
            error,
            data: hex::encode(data),
        };
        self.seq += 1;
        (self.sink)(&entry);
    }

    async fn login(&mut self) -> Result<Option<Outcome>, Error> {
        let options = self.options;
        self.send(&SHandShake {
            protocol_version: VarInt(CURRENT_MC_VERSION.protocol_version()),
            server_address: options.host.clone().into(),
            server_port: options.port,
            next_state: ConnectionState::Login,
        })
        .await?;
        self.send(&SLoginStart {
            name: options.username.clone().into(),
            uuid: offline_player_uuid(&options.username),
        })
        .await?;

        loop {
            let packet = self.decoder.get_raw_packet().await?;
            let mut body = &packet.payload[..];
            let version = &CURRENT_MC_VERSION;
            self.record(
                Direction::Clientbound,
                State::Login,
                packet_name(&LOGIN_PACKETS, packet.id),
                None,
                body,
                false,
            );
            if packet.id == login::LOGIN_COMPRESSION {
                let threshold = CSetCompression::read(&mut body, version)?.threshold.0;
                // A negative threshold turns compression off, which is the default already.
                if let Ok(threshold) = usize::try_from(threshold) {
                    self.decoder.set_compression(threshold);
                    self.encoder.set_compression((threshold, COMPRESSION_LEVEL));
                }
            } else if packet.id == login::LOGIN_FINISHED {
                self.send(&SLoginAcknowledged).await?;
                // ClientHandshakePacketListenerImpl.handleLoginFinished sends the brand and the
                // client information right after the acknowledgement.
                let mut brand = Vec::new();
                brand.write_string(&options.brand)?;
                self.send_payload(BRAND, &brand).await?;
                self.send(&SClientInformationConfig {
                    locale: "en_us",
                    view_distance: 2,
                    chat_mode: VarInt(0),
                    chat_colors: true,
                    skin_parts: 0x7f,
                    main_hand: VarInt(1),
                    text_filtering: false,
                    server_listing: true,
                    particle_status: VarInt(0),
                })
                .await?;
                return Ok(None);
            } else if packet.id == login::LOGIN_DISCONNECT {
                let reason = CLoginDisconnect::read(&mut body, version)?.json_reason;
                return Ok(Some(Outcome::Disconnected(reason)));
            } else if packet.id == login::CUSTOM_QUERY {
                // No login query channel is understood, like a vanilla client.
                let message_id = body.get_var_int()?;
                self.send(&SLoginPluginResponse {
                    message_id,
                    data: None,
                })
                .await?;
            } else if packet.id == login::COOKIE_REQUEST {
                let key = body.get_str_borrowed()?;
                self.send(&SLoginCookieResponse { key, payload: None })
                    .await?;
            } else if packet.id == login::HELLO {
                self.enable_encryption(body).await?;
            } else {
                return Err(format!("unexpected login packet 0x{:02x}", packet.id).into());
            }
        }
    }

    /// `ClientHandshakePacketListenerImpl.handleHello` without the session server join, which
    /// an offline login skips. Pumpkin asks for encryption even in offline mode by default.
    async fn enable_encryption(&mut self, mut body: &[u8]) -> Result<(), Error> {
        let request = CEncryptionRequest::read(&mut body, &CURRENT_MC_VERSION)?;
        if request.should_authenticate {
            return Err("the server is in online mode: only offline login is supported".into());
        }
        let public_key = RsaPublicKey::from_public_key_der(request.public_key)
            .map_err(|e| format!("bad server public key: {e}"))?;
        let secret: [u8; 16] = rand::random();
        let mut rng = rand::rng();
        self.send(&SEncryptionResponse {
            shared_secret: public_key
                .encrypt(&mut rng, Pkcs1v15Encrypt, &secret)?
                .into(),
            verify_token: public_key
                .encrypt(&mut rng, Pkcs1v15Encrypt, request.verify_token)?
                .into(),
        })
        .await?;
        self.encoder.set_encryption(&secret)?;
        self.decoder.set_encryption(&secret)?;
        Ok(())
    }

    async fn configuration(&mut self) -> Result<Outcome, Error> {
        loop {
            let (packet, reassembled) = match self.pending.pop_front() {
                Some(packet) => (packet, true),
                None => (self.decoder.get_raw_packet().await?, false),
            };
            if let Some(outcome) = self.handle_config_packet(&packet, reassembled).await? {
                return Ok(outcome);
            }
        }
    }

    async fn handle_config_packet(
        &mut self,
        packet: &RawPacket,
        reassembled: bool,
    ) -> Result<Option<Outcome>, Error> {
        let version = &CURRENT_MC_VERSION;
        let name = packet_name(&CONFIG_PACKETS, packet.id);
        let mut body = &packet.payload[..];

        if packet.id == config::CUSTOM_PAYLOAD {
            // Same layout as the serverbound packet.
            let (channel, data) = read_custom_payload(body)?;
            self.record(
                Direction::Clientbound,
                State::Configuration,
                name,
                Some(channel),
                data,
                reassembled,
            );
            return self.handle_custom_payload(channel, data, reassembled).await;
        }

        self.record(
            Direction::Clientbound,
            State::Configuration,
            name,
            None,
            body,
            reassembled,
        );
        if packet.id == config::PING {
            let id = CConfigPing::read(&mut body, version)?.id;
            self.send(&SConfigPong { id }).await?;
        } else if packet.id == config::KEEP_ALIVE {
            let keep_alive_id = SKeepAlive::read(&mut body, version)?.keep_alive_id;
            self.send(&SKeepAlive { keep_alive_id }).await?;
        } else if packet.id == config::SELECT_KNOWN_PACKS {
            // A vanilla client only has the `minecraft` packs; the offered list has the same layout
            // as the serverbound reply.
            let offered = SKnownPacks::read(&mut body, version)?;
            let known_packs = offered
                .known_packs
                .into_iter()
                .filter(|pack| pack.namespace == "minecraft")
                .collect();
            self.send(&SKnownPacks { known_packs }).await?;
        } else if packet.id == config::UPDATE_ENABLED_FEATURES {
            // ClientConfigurationPacketListenerImpl.handleEnabledFeatures: fallback detection of a
            // non-NeoForge server.
            return self.initialize_other_connection().await;
        } else if packet.id == config::COOKIE_REQUEST {
            let key = body.get_str_borrowed()?;
            self.send(&SConfigCookieResponse {
                key,
                has_payload: false,
                payload: None,
            })
            .await?;
        } else if packet.id == config::CODE_OF_CONDUCT {
            self.send(&SAcceptCodeOfConduct).await?;
        } else if packet.id == config::DISCONNECT {
            return Ok(Some(Outcome::Kicked(read_disconnect_reason(body)?)));
        } else if packet.id == config::FINISH_CONFIGURATION {
            // ClientConfigurationPacketListenerImpl.handleConfigurationFinished: the fallback for a
            // delayed brand runs before NetworkRegistry.onConfigurationFinished.
            if self.connection_type == ConnectionType::Other
                && let Some(outcome) = self.initialize_other_connection().await?
            {
                return Ok(Some(outcome));
            }
            self.on_configuration_finished().await?;
            self.send(&SAcknowledgeFinishConfig).await?;
            return Ok(Some(Outcome::Finished));
        }
        Ok(None)
    }

    /// The custom payload dispatch of `ClientConfigurationPacketListenerImpl.handleCustomPayload`
    /// followed by `ClientCommonPacketListenerImpl.handleCustomPayload`.
    /// `reassembled` is whether the payload came in a packet joined from `neoforge:split` parts,
    /// which can end with padding.
    async fn handle_custom_payload(
        &mut self,
        channel: &str,
        data: &[u8],
        reassembled: bool,
    ) -> Result<Option<Outcome>, Error> {
        let options = self.options;
        let Some(channels) = options.channels.as_ref() else {
            // A vanilla client ignores every channel it has no handler for.
            return Ok(None);
        };
        match channel {
            MinecraftRegisterPayload::CHANNEL => {
                let registered =
                    decode_payload(data, reassembled, MinecraftRegisterPayload::read)?.channels;
                self.adhoc_channels.extend(registered);
                if self.setup.is_none() {
                    self.send_channels(channel, channels.initial_listening_channels())
                        .await?;
                }
            }
            MinecraftUnregisterPayload::CHANNEL => {
                let forgotten =
                    decode_payload(data, reassembled, MinecraftUnregisterPayload::read)?.channels;
                self.adhoc_channels.retain(|id| !forgotten.contains(id));
            }
            ModdedNetworkQueryPayload::CHANNEL => {
                self.connection_type = ConnectionType::NeoForge;
                let mut reply = Vec::new();
                channels.query().write(&mut reply)?;
                self.send_payload(channel, &reply).await?;
            }
            ModdedNetworkPayload::CHANNEL => {
                let setup = decode_payload(data, reassembled, ModdedNetworkPayload::read)?.setup;
                if self.setup.is_none() {
                    let configuration: BTreeSet<Identifier> = setup
                        .channels
                        .get(&ConnectionProtocol::Configuration)
                        .into_iter()
                        .flat_map(|c| c.keys().cloned())
                        .collect();
                    let mut listening = ChannelMap::builtin_channels();
                    listening.extend(configuration.iter().cloned());
                    self.setup = Some(configuration);
                    self.send_channels(MinecraftRegisterPayload::CHANNEL, listening)
                        .await?;
                }
            }
            // The server disconnects right after it; the reasons are in the recording.
            ModdedNetworkSetupFailedPayload::CHANNEL => {}
            BRAND if self.connection_type == ConnectionType::Other => {
                return self.initialize_other_connection().await;
            }
            CommonVersionPayload::CHANNEL => {
                let versions =
                    decode_payload(data, reassembled, CommonVersionPayload::read)?.versions;
                if !versions.contains(&COMMON_NETWORKING_VERSION) {
                    return Ok(Some(Outcome::Disconnected(format!(
                        "client: unsupported common network versions {versions:?}"
                    ))));
                }
                let mut reply = Vec::new();
                CommonVersionPayload {
                    versions: vec![COMMON_NETWORKING_VERSION],
                }
                .write(&mut reply)?;
                self.send_payload(channel, &reply).await?;
            }
            CommonRegisterPayload::CHANNEL => {
                let mut reply = Vec::new();
                CommonRegisterPayload {
                    version: COMMON_NETWORKING_VERSION,
                    protocol: Some(ConnectionProtocol::Play),
                    channels: channels.common_play_channels(),
                }
                .write(&mut reply)?;
                self.send_payload(channel, &reply).await?;
            }
            _ => {
                return self
                    .handle_modded_payload(channels, channel, data, reassembled)
                    .await;
            }
        }
        Ok(None)
    }

    /// `ClientNetworkRegistry.handleModdedPayload`, after the codec lookup of
    /// `NetworkRegistry.getCodec`.
    async fn handle_modded_payload(
        &mut self,
        channels: &ChannelMap,
        channel: &str,
        data: &[u8],
        reassembled: bool,
    ) -> Result<Option<Outcome>, Error> {
        let Ok(id) = Identifier::parse(channel) else {
            return Ok(None);
        };
        if id.namespace() == "minecraft" {
            return Ok(None);
        }
        // getCodec: an unregistered channel, or one registered serverbound, decodes to a
        // DiscardedPayload, which is ignored.
        let Some(registration) = channels
            .configuration_registration(&id)
            .filter(|r| r.flow != Some(PacketFlow::Serverbound))
        else {
            warn!(channel, "no clientbound registration, payload discarded");
            return Ok(None);
        };
        let Some(setup) = self.setup.as_ref() else {
            return Ok(Some(Outcome::Disconnected(
                "client: modded payload before channel negotiation (No Payload Setup)".to_owned(),
            )));
        };
        // hasAdhocChannel: an optional registration can be read without a negotiated channel.
        if !setup.contains(&id) && !registration.optional {
            return Ok(Some(Outcome::Disconnected(format!(
                "client: no channel for {channel}"
            ))));
        }

        match channel {
            FrozenRegistrySyncStartPayload::CHANNEL => {
                self.to_synchronize =
                    decode_payload(data, reassembled, FrozenRegistrySyncStartPayload::read)?
                        .to_access
                        .into_iter()
                        .collect();
            }
            FrozenRegistryPayload::CHANNEL => {
                let name =
                    decode_payload(data, reassembled, FrozenRegistryPayload::read)?.registry_name;
                self.to_synchronize.remove(&name);
            }
            FrozenRegistrySyncCompletedPayload::CHANNEL => {
                if !self.to_synchronize.is_empty() {
                    let missing: Vec<String> = self
                        .to_synchronize
                        .iter()
                        .map(ToString::to_string)
                        .collect();
                    return Ok(Some(Outcome::Disconnected(format!(
                        "client: registries not synced: {}",
                        missing.join(", ")
                    ))));
                }
                return self.reply(channel, &[]).await;
            }
            FeatureFlagDataPayload::CHANNEL => {
                decode_payload(data, reassembled, FeatureFlagDataPayload::read)?;
                let mut reply = Vec::new();
                FeatureFlagAcknowledgePayload.write(&mut reply)?;
                return self
                    .reply(FeatureFlagAcknowledgePayload::CHANNEL, &reply)
                    .await;
            }
            ExtensibleEnumDataPayload::CHANNEL => {
                decode_payload(data, reassembled, ExtensibleEnumDataPayload::read)?;
                let mut reply = Vec::new();
                ExtensibleEnumAcknowledgePayload.write(&mut reply)?;
                return self
                    .reply(ExtensibleEnumAcknowledgePayload::CHANNEL, &reply)
                    .await;
            }
            // An empty map: the client knows no data maps, which only fails for mandatory ones.
            KnownRegistryDataMapsPayload::CHANNEL => {
                decode_payload(data, reassembled, KnownRegistryDataMapsPayload::read)?;
                let mut reply = Vec::new();
                KnownRegistryDataMapsReplyPayload::default().write(&mut reply)?;
                return self
                    .reply(KnownRegistryDataMapsReplyPayload::CHANNEL, &reply)
                    .await;
            }
            SplitPacketPayload::CHANNEL => self.on_split(data)?,
            _ => {}
        }
        Ok(None)
    }

    fn on_split(&mut self, data: &[u8]) -> Result<(), Error> {
        if let Some(packet) = join_split(&mut self.split_buffer, data)? {
            self.pending.push_back(packet);
        }
        Ok(())
    }

    /// `ClientNetworkRegistry.initializeOtherConnection`, for a server that never sent the probe.
    async fn initialize_other_connection(&mut self) -> Result<Option<Outcome>, Error> {
        let options = self.options;
        let Some(channels) = options.channels.as_ref() else {
            return Ok(None);
        };
        if self.connection_type != ConnectionType::Other || self.setup.is_some() {
            return Ok(None);
        }
        self.setup = Some(BTreeSet::new());
        info!("no neoforge:register probe, falling back to a non-NeoForge connection");
        let required = channels.required_channels();
        if !required.is_empty() {
            return Ok(Some(Outcome::Disconnected(format!(
                "client: the server is not running NeoForge and these channels are required: {}",
                required.join(", ")
            ))));
        }
        self.send_channels(
            MinecraftRegisterPayload::CHANNEL,
            channels.initial_listening_channels(),
        )
        .await?;
        Ok(None)
    }

    /// `NetworkRegistry.onConfigurationFinished`, sent before the finish acknowledgement.
    async fn on_configuration_finished(&mut self) -> Result<(), Error> {
        let options = self.options;
        let Some(channels) = options.channels.as_ref() else {
            return Ok(());
        };
        let Some(setup) = self.setup.as_ref() else {
            warn!("configuration finished before the channel negotiation");
            return Ok(());
        };
        let mut forgotten = ChannelMap::builtin_channels();
        forgotten.extend(setup.iter().cloned());
        let mut listening = BTreeSet::from([
            Identifier::parse_static(MinecraftRegisterPayload::CHANNEL),
            Identifier::parse_static(MinecraftUnregisterPayload::CHANNEL),
        ]);
        if self.connection_type == ConnectionType::NeoForge {
            listening.insert(Identifier::parse_static(ModdedNetworkQueryPayload::CHANNEL));
        } else {
            listening.extend(channels.common_play_channels());
        }
        self.send_channels(MinecraftUnregisterPayload::CHANNEL, forgotten)
            .await?;
        self.send_channels(MinecraftRegisterPayload::CHANNEL, listening)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frames `packet` like the server egress for a client that declared `neoforge:split`, then
    /// reads the frames back as this client does: the states of the parts and the joined packets.
    async fn receive_split(
        packet: &[u8],
        compression: Option<usize>,
    ) -> Result<(Vec<u8>, Vec<RawPacket>), Error> {
        let mut wire = Vec::new();
        let mut encoder = TCPNetworkEncoder::new(&mut wire);
        if let Some(threshold) = compression {
            encoder.set_compression((threshold, COMPRESSION_LEVEL));
        }
        encoder.set_split_payload_id(Some(config::CUSTOM_PAYLOAD.0));
        encoder.write_packet(Bytes::copy_from_slice(packet)).await?;
        let mut decoder = TCPNetworkDecoder::new(&wire[..]);
        if let Some(threshold) = compression {
            decoder.set_compression(threshold);
        }
        let (mut states, mut joined, mut buffer) = (Vec::new(), Vec::new(), Vec::new());
        loop {
            let part = match decoder.get_raw_packet().await {
                Ok(part) => part,
                Err(pumpkin_protocol::PacketDecodeError::ConnectionClosed) => break,
                Err(err) => return Err(err.into()),
            };
            assert_eq!(part.id, config::CUSTOM_PAYLOAD);
            let (channel, data) = read_custom_payload(&part.payload)?;
            assert_eq!(channel, SplitPacketPayload::CHANNEL);
            states.push(decode_exact(data, SplitPacketPayload::read)?.payload[0]);
            joined.extend(join_split(&mut buffer, data)?);
        }
        Ok((states, joined))
    }

    #[tokio::test]
    async fn joins_a_9_mib_packet_split_by_the_server() -> Result<(), Error> {
        // config::REGISTRY_DATA with a counting body.
        let mut packet = vec![u8::try_from(config::REGISTRY_DATA.0)?];
        packet.extend((1..9 * 1024 * 1024).map(|i| i as u8));
        for (compression, expected) in [(None, &[1, 0, 0, 0, 2][..]), (Some(256), &[1, 2])] {
            let (states, joined) = receive_split(&packet, compression).await?;
            assert_eq!(states, expected);
            assert_eq!(joined.len(), 1);
            assert_eq!(joined[0].id, config::REGISTRY_DATA);
            assert_eq!(joined[0].payload, packet[1..]);
        }
        Ok(())
    }

    /// A `NeoForge` sender pads the last part with the unused capacity of its buffer.
    #[test]
    fn decodes_a_joined_payload_with_a_padded_last_part() -> Result<(), Error> {
        let mut data = Vec::new();
        CommonVersionPayload {
            versions: vec![1, 2],
        }
        .write(&mut data)?;
        let mut packet = vec![u8::try_from(config::CUSTOM_PAYLOAD.0)?];
        packet.write_string(CommonVersionPayload::CHANNEL)?;
        packet.extend_from_slice(&data);
        let middle = packet.len() / 2;
        let mut last = packet[middle..].to_vec();
        last.extend([0; 16]);

        let mut buffer = Vec::new();
        let mut joined = None;
        for (state, slice) in [
            (SPLIT_STATE_FIRST, &packet[..middle]),
            (SPLIT_STATE_LAST, &last),
        ] {
            let mut part = Vec::new();
            SplitPacketPayload {
                payload: [&[state][..], slice].concat().into_boxed_slice(),
            }
            .write(&mut part)?;
            joined = join_split(&mut buffer, &part)?;
        }
        let joined = joined.ok_or("the last part joins the packet")?;
        let (channel, data) = read_custom_payload(&joined.payload)?;
        assert_eq!(channel, CommonVersionPayload::CHANNEL);
        assert!(decode_exact(data, CommonVersionPayload::read).is_err());
        let versions = decode_payload(data, true, CommonVersionPayload::read)?.versions;
        assert_eq!(versions, [1, 2]);
        Ok(())
    }
}
