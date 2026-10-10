use std::{
    net::SocketAddr,
    num::NonZero,
    sync::{Arc, Weak},
};

use bytes::Bytes;
use crossbeam::atomic::AtomicCell;
use pumpkin_config::networking::compression::CompressionInfo;
use pumpkin_data::packet::CURRENT_MC_VERSION;
use pumpkin_protocol::{
    ClientPacket, ConnectionState, PacketDecodeError, RawPacket, ServerPacket,
    codec::var_int::VarInt,
    java::{
        client::config::{CConfigDisconnect, CPluginMessage},
        client::login::CLoginDisconnect,
        client::play::CPlayDisconnect,
        neoforge::{NetworkPayloadSetup, decode_exact},
        packet_decoder::TCPNetworkDecoder,
        packet_encoder::TCPNetworkEncoder,
        server::config::{
            SAcceptCodeOfConduct, SAcknowledgeFinishConfig, SClientInformationConfig,
            SConfigCookieResponse, SConfigPong, SConfigResourcePack, SKnownPacks, SPluginMessage,
        },
    },
    packet::MultiVersionJavaPacket,
    ser::{NetworkReadExt, NetworkReadSliceExt, NetworkWriteExt, ReadingError},
};
use pumpkin_util::{
    Hand, identifier::Identifier, text::TextComponent, version::JavaMinecraftVersion,
};
use tokio::{
    io::{BufReader, BufWriter},
    net::{
        TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::{
    entity::player::{ChatMode, ParticleStatus},
    net::{
        EncryptionError, GameProfile, PacketHandlerResult, PacketRateLimiter, PlayerConfig,
        can_not_join,
    },
    plugin::server::{
        config_custom_payload::ConfigCustomPayloadEvent,
        packet::{ConnectionPacketReceivedEvent, ConnectionPacketSentEvent},
    },
    server::Server,
};

use super::{
    JavaClient,
    configuration_tasks::{ConfigurationTasks, TaskReply, is_terminal_resource_pack_response},
    neoforge::{self, ClientChannels},
};

/// The channel of vanilla `BrandPayload`.
pub const BRAND_CHANNEL: &str = "minecraft:brand";

/// `BrandPayload.STREAM_CODEC`: `FriendlyByteBuf.readUtf()`, a `VarInt` byte length, then UTF-8 of
/// at most 32767 UTF-16 units, and nothing after it.
fn read_brand(data: &[u8]) -> Result<String, ReadingError> {
    decode_exact(data, |read| Ok(read.get_str_borrowed()?.to_owned()))
}

/// How long a connection may stay silent before login finishes.
///
/// Once a player is in game, [`JavaClient::progress_player_packets`] keeps the
/// connection honest with keep-alives. Nothing plays that role beforehand, and
/// accepted sockets have no TCP keep-alive either, so a peer that stops talking
/// without closing would otherwise hold its descriptor for the lifetime of the
/// server. The timer covers silence rather than the whole handshake: it is reset
/// on every packet, so a slow but progressing login is never cut off.
const HANDSHAKE_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub struct PendingConnection {
    pub id: u64,
    pub address: SocketAddr,
    pub server_address: String,
    pub version: AtomicCell<JavaMinecraftVersion>,
    pub connection_state: AtomicCell<ConnectionState>,
    pub close_token: CancellationToken,
    pub network_writer: TCPNetworkEncoder<BufWriter<OwnedWriteHalf>>,
    pub network_reader: TCPNetworkDecoder<BufReader<OwnedReadHalf>>,
    pub gameprofile: Option<GameProfile>,
    pub config: Option<PlayerConfig>,
    pub brand: Option<String>,
    pub packet_limiter: PacketRateLimiter,
    pub verify_token: Option<[u8; 4]>,
    pub vine_challenge: Option<[u8; 16]>,
    pub client_channels: ClientChannels,
    /// The `NeoForge` probe ping went out and its pong has not come back.
    pub neoforge_probe_pending: bool,
    /// The channels negotiated with a `NeoForge` client, like `ChannelAttributes.setPayloadSetup`.
    /// Empty for any other client.
    pub payload_setup: NetworkPayloadSetup,
    /// Which ids the client gets for custom content in play. `Real` only for a `NeoForge` client
    /// that completed the registry sync.
    pub content_ids: pumpkin_data::dynamic::ContentIds,
    /// The configuration tasks: filled after `update_enabled_features`, and each reply finishes
    /// the current task.
    pub configuration_tasks: ConfigurationTasks,
    /// For the connection packet events.
    server: Weak<Server>,
}

impl PendingConnection {
    #[must_use]
    pub fn new(
        tcp_stream: TcpStream,
        address: SocketAddr,
        id: u64,
        packet_limiter: PacketRateLimiter,
        server: Weak<Server>,
    ) -> Self {
        let (read, write) = tcp_stream.into_split();
        Self {
            id,
            address,
            server_address: String::new(),
            version: AtomicCell::new(CURRENT_MC_VERSION),
            connection_state: AtomicCell::new(ConnectionState::HandShake),
            close_token: CancellationToken::new(),
            network_writer: TCPNetworkEncoder::new(BufWriter::new(write)),
            network_reader: TCPNetworkDecoder::new(BufReader::new(read)),
            gameprofile: None,
            config: None,
            brand: None,
            packet_limiter,
            verify_token: None,
            vine_challenge: None,
            client_channels: ClientChannels::default(),
            neoforge_probe_pending: false,
            payload_setup: NetworkPayloadSetup::default(),
            content_ids: pumpkin_data::dynamic::ContentIds::default(),
            configuration_tasks: ConfigurationTasks::default(),
            server,
        }
    }

    pub fn close(&self) {
        self.close_token.cancel();
    }

    pub fn is_closed(&self) -> bool {
        self.close_token.is_cancelled()
    }

    pub async fn await_close_interrupt(&self) {
        self.close_token.cancelled().await;
    }

    pub fn set_encryption(&mut self, shared_secret: &[u8]) -> Result<(), EncryptionError> {
        let crypt_key: [u8; 16] = shared_secret
            .try_into()
            .map_err(|_| EncryptionError::SharedWrongLength)?;
        self.network_reader
            .set_encryption(&crypt_key)
            .map_err(|_| EncryptionError::AlreadyEncrypted)?;
        self.network_writer
            .set_encryption(&crypt_key)
            .map_err(|_| EncryptionError::AlreadyEncrypted)?;
        Ok(())
    }

    pub fn set_compression(&mut self, compression: &CompressionInfo) {
        if compression.level > 9 {
            error!("Invalid compression level! Clients will not be able to read this!");
        }

        self.network_reader
            .set_compression(compression.threshold as usize);

        self.network_writer
            .set_compression((compression.threshold as usize, compression.level));
    }

    pub async fn get_packet(&mut self) -> Option<RawPacket> {
        let close_token = self.close_token.clone();
        let packet_result = tokio::select! {
            () = close_token.cancelled() => {
                debug!("Canceling pending connection packet processing");
                return None;
            },
            () = tokio::time::sleep(HANDSHAKE_IDLE_TIMEOUT) => {
                debug!(
                    "Client {} sent nothing for {}s before finishing login, dropping it",
                    self.id,
                    HANDSHAKE_IDLE_TIMEOUT.as_secs()
                );
                return None;
            },
            res = self.network_reader.get_raw_packet() => res,
        };

        match packet_result {
            Ok(packet) => Some(packet),
            Err(err) => {
                if !matches!(err, PacketDecodeError::ConnectionClosed) {
                    debug!("Failed to decode packet from client {}: {}", self.id, err);
                    let text = format!("Error while reading incoming packet {err}");
                    self.kick(TextComponent::text(text)).await;
                }
                None
            }
        }
    }

    /// Server for the connection packet events, only for clients below 26.3 after handshake.
    fn translating_server(&self) -> Option<Arc<Server>> {
        if self.version.load() == CURRENT_MC_VERSION
            || self.connection_state.load() == ConnectionState::HandShake
        {
            return None;
        }
        self.server.upgrade()
    }

    /// Encoded as 26.3. `ConnectionPacketSentEvent` can rewrite it.
    pub async fn send_packet_now<P: ClientPacket>(&mut self, packet: &P) {
        let mut packet_buf = Vec::new();
        if let Err(err) =
            JavaClient::write_packet_for_version(packet, CURRENT_MC_VERSION, &mut packet_buf)
        {
            error!("Failed to write packet: {err:?}");
            return;
        }
        let Some(payload) = self.translate_outgoing(Bytes::from(packet_buf)).await else {
            return;
        };
        if let Err(err) = self.network_writer.write_packet(payload).await {
            warn!("Failed to send packet to client {}: {}", self.id, err);
        }
        let _ = self.network_writer.flush().await;
    }

    /// `ConnectionPacketSentEvent` with the 26.3 packet. `None` when cancelled.
    async fn translate_outgoing(&self, packet_data: Bytes) -> Option<Bytes> {
        let Some(server) = self.translating_server() else {
            return Some(packet_data);
        };
        if !server
            .plugin_manager
            .has_handlers::<ConnectionPacketSentEvent>()
        {
            return Some(packet_data);
        }

        let mut reader = &packet_data[..];
        let Ok(packet_id) = reader.get_var_int() else {
            return Some(packet_data);
        };
        let payload = packet_data.slice(packet_data.len() - reader.len()..);
        let mut event = ConnectionPacketSentEvent::new(
            self.id,
            self.version.load(),
            self.connection_state.load(),
            packet_id.0,
            payload,
        );
        server.plugin_manager.fire(&server, &mut event).await;
        if event.cancelled {
            return None;
        }

        let mut framed = Vec::with_capacity(5 + event.payload.len());
        framed.write_var_int(&VarInt(event.packet_id)).ok()?;
        framed.extend_from_slice(&event.payload);
        Some(framed.into())
    }

    /// `ConnectionPacketReceivedEvent` with the client's packet; handlers rewrite it to 26.3.
    /// `None` when cancelled.
    async fn translate_incoming(&self, packet: &RawPacket) -> Option<RawPacket> {
        let unchanged = || RawPacket {
            id: packet.id,
            payload: packet.payload.clone(),
        };
        let Some(server) = self.translating_server() else {
            return Some(unchanged());
        };
        if !server
            .plugin_manager
            .has_handlers::<ConnectionPacketReceivedEvent>()
        {
            return Some(unchanged());
        }

        let mut event = ConnectionPacketReceivedEvent::new(
            self.id,
            self.version.load(),
            self.connection_state.load(),
            packet.id,
            packet.payload.clone(),
        );
        server.plugin_manager.fire(&server, &mut event).await;
        (!event.cancelled).then(|| RawPacket {
            id: event.packet_id,
            payload: event.payload,
        })
    }

    pub async fn kick(&mut self, reason: TextComponent) {
        match self.connection_state.load() {
            ConnectionState::Login => {
                self.send_packet_now(&CLoginDisconnect::new(
                    serde_json::to_string(&reason.0).unwrap_or_else(|_| String::new()),
                ))
                .await;
            }
            ConnectionState::Config => {
                self.send_packet_now(&CConfigDisconnect::new(&reason)).await;
            }
            ConnectionState::Play => {
                self.send_packet_now(&CPlayDisconnect::new(&reason)).await;
            }
            _ => {}
        }
        debug!("Closing connection for {}", self.id);
        self.close();
    }

    pub async fn handle_login_sequence(&mut self, server: &Arc<Server>) -> PacketHandlerResult {
        // Like vanilla `Connection.channelRead0`, a closed connection handles nothing more,
        // including packets that were already buffered when the kick closed it.
        while let Some(packet) = self.get_packet().await
            && !self.is_closed()
        {
            if !self.packet_limiter.check_packet() {
                warn!(
                    "Pending client {} exceeded packet rate limit (rate: {}/s)",
                    self.id,
                    self.packet_limiter.max_rate()
                );
                self.kick(TextComponent::text(
                    server
                        .advanced_config
                        .networking
                        .java
                        .packet_limiter
                        .kick_message
                        .clone(),
                ))
                .await;
                return PacketHandlerResult::Stop;
            }

            match self.handle_packet(server, &packet).await {
                Ok(result) => {
                    if let Some(result) = result {
                        return result;
                    }
                }
                Err(error) => {
                    let text = format!("Error while reading incoming packet {error}");
                    debug!(
                        "Failed to read incoming packet with id {}: {}",
                        packet.id, error
                    );
                    self.kick(TextComponent::text(text)).await;
                }
            }
        }
        self.log_left_during_task();
        PacketHandlerResult::Stop
    }

    pub async fn handle_packet(
        &mut self,
        server: &Arc<Server>,
        packet: &RawPacket,
    ) -> Result<Option<PacketHandlerResult>, ReadingError> {
        let Some(packet) = self.translate_incoming(packet).await else {
            return Ok(None);
        };
        let packet = &packet;
        match self.connection_state.load() {
            ConnectionState::HandShake => self.handle_handshake_packet(server, packet).await,
            ConnectionState::Status => self.handle_status_packet(server, packet).await,
            ConnectionState::Login | ConnectionState::Transfer => {
                self.handle_login_packet(server, packet).await
            }
            ConnectionState::Config => self.handle_config_packet(server, packet).await,
            ConnectionState::Play => Ok(None),
        }
    }

    async fn handle_handshake_packet(
        &mut self,
        server: &Arc<Server>,
        packet: &RawPacket,
    ) -> Result<Option<PacketHandlerResult>, ReadingError> {
        debug!("Handling handshake group");
        let mut payload = &packet.payload[..];
        match packet.id {
            0 => {
                self.handle_handshake(
                    server,
                    pumpkin_protocol::java::server::handshake::SHandShake::read(
                        &mut payload,
                        &CURRENT_MC_VERSION,
                    )?,
                )
                .await;
                Ok(None)
            }
            _ => Err(ReadingError::Message(format!(
                "Failed to handle packet id {} in Handshake State",
                packet.id
            ))),
        }
    }

    async fn handle_status_packet(
        &mut self,
        server: &Arc<Server>,
        packet: &RawPacket,
    ) -> Result<Option<PacketHandlerResult>, ReadingError> {
        debug!("Handling status group");
        let mut payload = &packet.payload[..];
        let version = CURRENT_MC_VERSION;

        match packet.id {
            id if id == pumpkin_protocol::java::server::status::SStatusRequest::to_id(version) => {
                self.handle_status_request(server).await;
                Ok(None)
            }
            id if id
                == pumpkin_protocol::java::server::status::SStatusPingRequest::to_id(version) =>
            {
                self.handle_ping_request(
                    pumpkin_protocol::java::server::status::SStatusPingRequest::read(
                        &mut payload,
                        &version,
                    )?,
                )
                .await;
                Ok(None)
            }
            _ => Err(ReadingError::Message(format!(
                "Failed to handle java client packet id {} in Status State",
                packet.id
            ))),
        }
    }

    async fn handle_login_packet(
        &mut self,
        server: &Arc<Server>,
        packet: &RawPacket,
    ) -> Result<Option<PacketHandlerResult>, ReadingError> {
        debug!("Handling login group");
        let mut payload = &packet.payload[..];
        let version = CURRENT_MC_VERSION;

        match packet.id {
            id if id == pumpkin_protocol::java::server::login::SLoginStart::to_id(version) => {
                Ok(self
                    .handle_login_start(
                        server,
                        pumpkin_protocol::java::server::login::SLoginStart::read(
                            &mut payload,
                            &version,
                        )?,
                    )
                    .await)
            }
            id if id
                == pumpkin_protocol::java::server::login::SEncryptionResponse::to_id(version) =>
            {
                Ok(self
                    .handle_encryption_response(
                        server,
                        pumpkin_protocol::java::server::login::SEncryptionResponse::read(
                            &mut payload,
                            &version,
                        )?,
                    )
                    .await)
            }
            id if id
                == pumpkin_protocol::java::server::login::SLoginPluginResponse::to_id(version) =>
            {
                Ok(self
                    .handle_plugin_response(
                        server,
                        pumpkin_protocol::java::server::login::SLoginPluginResponse::read(
                            &mut payload,
                            &version,
                        )?,
                    )
                    .await)
            }
            id if id
                == pumpkin_protocol::java::server::login::SLoginCookieResponse::to_id(version) =>
            {
                self.handle_login_cookie_response(
                    &pumpkin_protocol::java::server::login::SLoginCookieResponse::read(
                        &mut payload,
                        &version,
                    )?,
                );
                Ok(None)
            }
            id if id
                == pumpkin_protocol::java::server::login::SLoginAcknowledged::to_id(version) =>
            {
                Ok(self.handle_login_acknowledged(server).await)
            }
            _ => Err(ReadingError::Message(format!(
                "Failed to handle packet id {} in Login State",
                packet.id
            ))),
        }
    }

    async fn handle_config_packet(
        &mut self,
        server: &Arc<Server>,
        packet: &RawPacket,
    ) -> Result<Option<PacketHandlerResult>, ReadingError> {
        debug!("Handling config group");
        let mut payload = &packet.payload[..];
        let version = CURRENT_MC_VERSION;

        match packet.id {
            id if id == SClientInformationConfig::to_id(version) => {
                self.handle_client_information_config(SClientInformationConfig::read(
                    &mut payload,
                    &version,
                )?)
                .await;
                Ok(None)
            }
            id if id == SPluginMessage::to_id(version) => {
                self.handle_plugin_message(server, SPluginMessage::read(&mut payload, &version)?)
                    .await?;
                Ok(None)
            }
            id if id == SAcknowledgeFinishConfig::to_id(version) => {
                if !self
                    .finish_configuration_task(TaskReply::FinishConfiguration)
                    .await
                {
                    return Ok(Some(PacketHandlerResult::Stop));
                }
                let Some(profile) = self.gameprofile.clone() else {
                    return Ok(Some(PacketHandlerResult::Stop));
                };
                let config = self.config.clone().unwrap_or_default();
                self.connection_state.store(ConnectionState::Play);
                if let Some(reason) = can_not_join(&profile, &self.address, server).await {
                    self.kick(reason).await;
                    Ok(Some(PacketHandlerResult::Stop))
                } else {
                    Ok(Some(PacketHandlerResult::ReadyToPlay(
                        profile,
                        config,
                        self.take_negotiated_state(),
                    )))
                }
            }
            id if id == SKnownPacks::to_id(version) => {
                if self.finish_configuration_task(TaskReply::KnownPacks).await {
                    self.handle_known_packs(server).await;
                    self.run_configuration_tasks(server).await;
                }
                Ok(None)
            }
            id if id == SConfigResourcePack::to_id(version) => {
                self.handle_resource_pack_response(
                    server,
                    SConfigResourcePack::read(&mut payload, &version)?,
                )
                .await;
                Ok(None)
            }
            id if id == SConfigCookieResponse::to_id(version) => {
                self.handle_config_cookie_response(&SConfigCookieResponse::read(
                    &mut payload,
                    &version,
                )?);
                Ok(None)
            }
            id if id == SConfigPong::to_id(version) => {
                let pong = SConfigPong::read(&mut payload, &version)?;
                self.handle_neoforge_probe_pong(server, &pong).await;
                Ok(None)
            }
            id if id == SAcceptCodeOfConduct::to_id(version) => {
                let _accept = SAcceptCodeOfConduct::read(&mut payload, &version)?;
                Ok(None)
            }
            _ => Err(ReadingError::Message(format!(
                "Failed to handle packet id {} in Config State",
                packet.id
            ))),
        }
    }

    pub async fn handle_client_information_config(
        &mut self,
        client_information: SClientInformationConfig<'_>,
    ) {
        debug!("Handling client settings");
        if client_information.view_distance <= 0 {
            self.kick(TextComponent::text(
                "Cannot have zero or negative view distance!",
            ))
            .await;
            return;
        }

        if let (Ok(main_hand), Ok(chat_mode)) = (
            Hand::try_from(client_information.main_hand.0),
            ChatMode::try_from(client_information.chat_mode.0),
        ) {
            self.config = Some(PlayerConfig {
                locale: client_information.locale.to_string(),
                view_distance: NonZero::new(client_information.view_distance as u8)
                    .unwrap_or(NonZero::<u8>::MIN),
                chat_mode,
                chat_colors: client_information.chat_colors,
                skin_parts: client_information.skin_parts,
                main_hand,
                text_filtering: client_information.text_filtering,
                server_listing: client_information.server_listing,
                particle_status: ParticleStatus::from(client_information.particle_status.0),
            });
        } else {
            self.kick(TextComponent::text("Invalid hand or chat type"))
                .await;
        }
    }

    pub async fn handle_plugin_message(
        &mut self,
        server: &Arc<Server>,
        plugin_message: SPluginMessage<'_>,
    ) -> Result<(), ReadingError> {
        debug!("Handling plugin message");
        if plugin_message.channel == BRAND_CHANNEL {
            let brand = read_brand(plugin_message.data)?;
            debug!("Got a client brand {brand:?}");
            self.brand = Some(brand);
        } else if let Some(reply) = TaskReply::for_channel(plugin_message.channel) {
            if self.finish_configuration_task(reply).await {
                self.handle_task_reply(reply, plugin_message.data);
                self.run_configuration_tasks(server).await;
            }
        } else {
            let handled = neoforge::detects_neoforge_clients(&server.basic_config)
                && self
                    .handle_neoforge_payload(server, plugin_message.channel, plugin_message.data)
                    .await?;
            if !handled {
                debug!(
                    "Client {} sent a payload on unknown configuration channel {}",
                    self.id, plugin_message.channel
                );
            }
        }
        self.fire_config_custom_payload(server, &plugin_message)
            .await;
        Ok(())
    }

    /// Fires `ConfigCustomPayloadEvent` after the built-in handling, so handlers see the channels
    /// this payload declared and cannot change what the server does with it.
    async fn fire_config_custom_payload(
        &mut self,
        server: &Arc<Server>,
        plugin_message: &SPluginMessage<'_>,
    ) {
        if self.is_closed()
            || !server
                .plugin_manager
                .has_handlers::<ConfigCustomPayloadEvent>()
        {
            return;
        }
        let (player_name, player_uuid) = self
            .gameprofile
            .as_ref()
            .map(|profile| (profile.name.clone(), profile.id))
            .unwrap_or_default();
        let channels = &self.client_channels;
        let mut event = ConfigCustomPayloadEvent {
            connection_id: self.id,
            player_name,
            player_uuid,
            version: self.version.load(),
            connection_type: channels.connection_type,
            modded_channels: channels.modded.clone(),
            ad_hoc_channels: channels.ad_hoc.iter().cloned().collect(),
            channel: plugin_message.channel.to_string(),
            data: Bytes::copy_from_slice(plugin_message.data),
            responses: Vec::new(),
        };
        server.plugin_manager.fire(server, &mut event).await;
        for (channel, data) in event.responses {
            if self.is_closed() {
                break;
            }
            self.send_custom_payload(&channel, &data).await;
        }
    }

    /// Sends a custom payload with the configuration-phase `CPluginMessage`.
    ///
    /// A payload on a channel that is not a valid identifier is dropped with a warning.
    pub async fn send_custom_payload(&mut self, channel: &str, data: &[u8]) {
        if Identifier::parse(channel).is_err() {
            warn!(
                "Dropping a custom payload for client {} on invalid channel {channel}",
                self.id
            );
            return;
        }
        self.send_packet_now(&CPluginMessage::new(channel, data))
            .await;
    }

    pub async fn handle_resource_pack_response(
        &mut self,
        server: &Server,
        packet: SConfigResourcePack,
    ) {
        use pumpkin_protocol::java::server::config::ResourcePackResponseResult;
        let result = packet.response_result();
        if let ResourcePackResponseResult::Unknown(_) = result {
            // Vanilla fails to decode an action it does not know and disconnects.
            self.kick(TextComponent::text("Invalid resource pack response"))
                .await;
            return;
        }
        if !is_terminal_resource_pack_response(&result)
            || !self
                .finish_configuration_task(TaskReply::ResourcePack)
                .await
        {
            return;
        }
        let force = server.advanced_config.resource_pack.java.force;
        let kick_reason = match result {
            ResourcePackResponseResult::Declined if force => {
                Some("Required resource pack was declined")
            }
            ResourcePackResponseResult::DownloadFail if force => {
                Some("Failed to download resource pack")
            }
            ResourcePackResponseResult::InvalidUrl => Some("Invalid resource pack URL"),
            ResourcePackResponseResult::ReloadFailed => Some("Failed to reload resource pack"),
            _ => None,
        };
        if let Some(reason) = kick_reason {
            self.kick(TextComponent::text(reason)).await;
        } else {
            self.run_configuration_tasks(server).await;
        }
    }

    pub fn handle_config_cookie_response(&self, packet: &SConfigCookieResponse<'_>) {
        debug!(
            "Received cookie_response[config]: key: \"{}\", payload_length: \"{:?}\"",
            packet.key,
            packet.payload.as_ref().map(|p| p.len())
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Serverbound `minecraft:brand` bodies recorded by `pumpkin-neoforge-client` from a
    // vanilla client and a NeoForge 26.3 client.
    #[test]
    fn brand_decodes_recorded_bytes() {
        let vanilla = [0x07, 0x76, 0x61, 0x6e, 0x69, 0x6c, 0x6c, 0x61];
        assert_eq!(read_brand(&vanilla).unwrap(), "vanilla");
        let neoforge = [0x08, 0x6e, 0x65, 0x6f, 0x66, 0x6f, 0x72, 0x67, 0x65];
        assert_eq!(read_brand(&neoforge).unwrap(), "neoforge");
    }

    #[test]
    fn brand_rejects_bytes_after_the_string() {
        let data = [0x07, 0x76, 0x61, 0x6e, 0x69, 0x6c, 0x6c, 0x61, 0x00];
        assert!(matches!(read_brand(&data), Err(ReadingError::TooLarge(_))));
    }
}
