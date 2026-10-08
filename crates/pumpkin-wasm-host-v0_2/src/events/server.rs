use crate::{
    events::{ToFromWasmEvent, cleanup_event, consume_text_component},
    generated_packets,
    pumpkin::plugin::event::{
        ClientboundPacket, ConfigCustomPayloadEventData,
        ConnectionProtocol as WitConnectionProtocol, ConnectionType as WitConnectionType, Event,
        MapInitializeEventData, ModdedChannel, OutgoingPayload, PacketFlow as WitPacketFlow,
        PacketReceivedEventData, PacketSentEventData, ServerBroadcastEventData,
        ServerCommandEventData, ServerListPingAddress, ServerListPingEventData,
        ServerLoadEventData, ServerLoadType, ServerTickEndEventData, ServerTickStartEventData,
        ServerboundPacket,
    },
    uuid::UuidExt,
};
use bytes::Bytes;
use pumpkin_core::net::{ClientPlatform, java::neoforge::ConnectionType};
use pumpkin_core::plugin::server::{
    config_custom_payload::ConfigCustomPayloadEvent,
    list_ping::ServerListPingEvent,
    map_initialize::MapInitializeEvent,
    packet::{PacketReceivedEvent, PacketSentEvent},
    server_broadcast::ServerBroadcastEvent,
    server_command::ServerCommandEvent,
    server_load::{LoadType, ServerLoadEvent},
    server_tick_end::ServerTickEndEvent,
    server_tick_start::ServerTickStartEvent,
};
use pumpkin_protocol::java::neoforge::{
    ConnectionProtocol, ModdedNetworkQueryComponent, PacketFlow,
};
use pumpkin_util::{identifier::Identifier, version::JavaMinecraftVersion};
use pumpkin_wasm_host_common::state::PluginHostState;
use std::collections::{BTreeMap, BTreeSet};

impl ToFromWasmEvent for PacketReceivedEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player_res = state
            .add(self.player.clone())
            .expect("failed to add player resource");

        let packet = match self.player.client.as_ref() {
            // Typed view is 26.3
            // for older clients only valid after the multiversion plugin ran.
            ClientPlatform::Java(_) => generated_packets::deserialize_java_serverbound_packet(
                self.packet_id,
                &self.payload,
                pumpkin_data::packet::CURRENT_MC_VERSION,
            )
            .map_or(ServerboundPacket::Unknown, ServerboundPacket::Java),
            ClientPlatform::Bedrock(_) => {
                generated_packets::deserialize_bedrock_serverbound_packet(
                    self.packet_id,
                    &self.payload,
                )
                .map_or(ServerboundPacket::Unknown, ServerboundPacket::Bedrock)
            }
        };

        Event::PacketReceivedEvent(PacketReceivedEventData {
            player: player_res,
            packet,
            packet_id: self.packet_id,
            raw_payload: self.payload.to_vec(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
        if let Event::PacketReceivedEvent(data) = event {
            self.packet_id = data.packet_id;
            self.payload = data.raw_payload.into();
            self.cancelled = data.cancelled;
        }
    }
    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::PacketReceivedEvent(_) => {
                // TODO: Implement converting from WIT variant back to raw if needed.
                // For now, we only support cancellation.
                panic!(
                    "Modifying packets from WASM is not yet supported in this simple implementation."
                );
            }
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for PacketSentEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player_res = state
            .add(self.player.clone())
            .expect("failed to add player resource");

        let packet = match self.player.client.as_ref() {
            ClientPlatform::Java(_) => {
                generated_packets::clientbound_java_any_to_wit(self.packet.as_ref())
                    .map_or(ClientboundPacket::Unknown, ClientboundPacket::Java)
            }
            ClientPlatform::Bedrock(_) => {
                generated_packets::clientbound_bedrock_any_to_wit(self.packet.as_ref())
                    .map_or(ClientboundPacket::Unknown, ClientboundPacket::Bedrock)
            }
        };

        Event::PacketSentEvent(PacketSentEventData {
            player: player_res,
            packet,
            packet_id: self.packet_id,
            raw_payload: self.payload.iter().copied().collect(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_event(&event, state);
        if let Event::PacketSentEvent(data) = event {
            self.packet_id = data.packet_id;
            self.payload = data.raw_payload.into();
            self.cancelled = data.cancelled;
        }
    }
    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::PacketSentEvent(_) => {
                panic!("Modifying packets from WASM is not yet supported.");
            }
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for ServerCommandEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::ServerCommandEvent(ServerCommandEventData {
            command: self.command.clone(),
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::ServerCommandEvent(data) => Self {
                command: data.command,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for ServerBroadcastEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let message = state
            .add(self.message.clone())
            .expect("failed to add text-component resource");
        let sender = state
            .add(self.sender.clone())
            .expect("failed to add text-component resource");

        Event::ServerBroadcastEvent(ServerBroadcastEventData {
            message,
            sender,
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::ServerBroadcastEvent(data) => Self {
                message: consume_text_component(state, &data.message),
                sender: consume_text_component(state, &data.sender),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for ServerListPingEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let motd = state
            .add(self.motd.clone())
            .expect("failed to add text-component resource");

        Event::ServerListPingEvent(ServerListPingEventData {
            hostname: self.hostname().to_string(),
            address: ServerListPingAddress {
                host: self.address().host().to_string(),
                port: self.address().port(),
            },
            motd,
            max_players: self.max_players,
            num_players: self.num_players,
            favicon: self.favicon.clone(),
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::ServerListPingEvent(data) => Self {
                hostname: data.hostname,
                address:
                    pumpkin_core::plugin::api::events::server::list_ping::ServerListPingAddress::new(
                        data.address.host,
                        data.address.port,
                    ),
                motd: consume_text_component(state, &data.motd),
                max_players: data.max_players,
                num_players: data.num_players,
                favicon: data.favicon,
            },
            _ => panic!("unexpected event type"),
        }
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if !matches!(&event, Event::ServerListPingEvent(_)) {
            cleanup_event(&event, state);
            panic!("unexpected event type");
        }

        let returned = Self::from_wasm_event(event, state);
        self.motd = returned.motd;
        self.max_players = returned.max_players;
        self.num_players = returned.num_players;
        self.favicon = returned.favicon;
    }
}

impl ToFromWasmEvent for ServerLoadEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::ServerLoadEvent(ServerLoadEventData {
            load_type: match self.load_type {
                LoadType::Startup => ServerLoadType::Startup,
                LoadType::Reload => ServerLoadType::Reload,
            },
        })
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::ServerLoadEvent(data) => Self {
                load_type: match data.load_type {
                    ServerLoadType::Startup => LoadType::Startup,
                    ServerLoadType::Reload => LoadType::Reload,
                },
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for ServerTickEndEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::ServerTickEndEvent(ServerTickEndEventData {
            tick: self.tick,
            duration_nanos: self.duration_nanos,
        })
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::ServerTickEndEvent(data) => Self {
                tick: data.tick,
                duration_nanos: data.duration_nanos,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for ServerTickStartEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::ServerTickStartEvent(ServerTickStartEventData { tick: self.tick })
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::ServerTickStartEvent(data) => Self { tick: data.tick },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for MapInitializeEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::MapInitializeEvent(MapInitializeEventData {
            map_id: self.map_id,
        })
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::MapInitializeEvent(data) => Self {
                map_id: data.map_id,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

fn to_wit_modded_channels(
    channels: &BTreeMap<ConnectionProtocol, BTreeSet<ModdedNetworkQueryComponent>>,
) -> Vec<ModdedChannel> {
    channels
        .iter()
        .flat_map(|(protocol, components)| {
            components.iter().map(|component| ModdedChannel {
                protocol: match protocol {
                    ConnectionProtocol::Handshaking => WitConnectionProtocol::Handshaking,
                    ConnectionProtocol::Play => WitConnectionProtocol::Play,
                    ConnectionProtocol::Status => WitConnectionProtocol::Status,
                    ConnectionProtocol::Login => WitConnectionProtocol::Login,
                    ConnectionProtocol::Configuration => WitConnectionProtocol::Configuration,
                },
                id: component.id.to_string(),
                version: component.version.clone(),
                flow: component.flow.map(|flow| match flow {
                    PacketFlow::Serverbound => WitPacketFlow::Serverbound,
                    PacketFlow::Clientbound => WitPacketFlow::Clientbound,
                }),
                optional: component.optional,
            })
        })
        .collect()
}

fn from_wit_modded_channels(
    channels: Vec<ModdedChannel>,
) -> BTreeMap<ConnectionProtocol, BTreeSet<ModdedNetworkQueryComponent>> {
    let mut map: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
    for channel in channels {
        let Ok(id) = Identifier::parse(&channel.id) else {
            continue;
        };
        let protocol = match channel.protocol {
            WitConnectionProtocol::Handshaking => ConnectionProtocol::Handshaking,
            WitConnectionProtocol::Play => ConnectionProtocol::Play,
            WitConnectionProtocol::Status => ConnectionProtocol::Status,
            WitConnectionProtocol::Login => ConnectionProtocol::Login,
            WitConnectionProtocol::Configuration => ConnectionProtocol::Configuration,
        };
        map.entry(protocol)
            .or_default()
            .insert(ModdedNetworkQueryComponent {
                id,
                version: channel.version,
                flow: channel.flow.map(|flow| match flow {
                    WitPacketFlow::Serverbound => PacketFlow::Serverbound,
                    WitPacketFlow::Clientbound => PacketFlow::Clientbound,
                }),
                optional: channel.optional,
            });
    }
    map
}

impl ToFromWasmEvent for ConfigCustomPayloadEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::ConfigCustomPayloadEvent(ConfigCustomPayloadEventData {
            connection_id: self.connection_id,
            player_name: self.player_name.clone(),
            player_uuid: crate::pumpkin::plugin::uuid::Uuid::to_wit(&self.player_uuid),
            protocol_version: self.version.protocol_version(),
            connection_type: match self.connection_type {
                ConnectionType::NeoForge => WitConnectionType::Neoforge,
                ConnectionType::Other => WitConnectionType::Other,
            },
            modded_channels: to_wit_modded_channels(&self.modded_channels),
            ad_hoc_channels: self
                .ad_hoc_channels
                .iter()
                .map(ToString::to_string)
                .collect(),
            channel: self.channel.clone(),
            data: self.data.to_vec(),
            responses: self
                .responses
                .iter()
                .map(|(channel, data)| OutgoingPayload {
                    channel: channel.clone(),
                    data: data.to_vec(),
                })
                .collect(),
        })
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::ConfigCustomPayloadEvent(data) => Self {
                connection_id: data.connection_id,
                player_name: data.player_name,
                player_uuid: crate::pumpkin::plugin::uuid::Uuid::from_wit(&data.player_uuid),
                version: JavaMinecraftVersion::from_protocol(
                    data.protocol_version.try_into().unwrap_or_default(),
                ),
                connection_type: match data.connection_type {
                    WitConnectionType::Neoforge => ConnectionType::NeoForge,
                    WitConnectionType::Other => ConnectionType::Other,
                },
                modded_channels: from_wit_modded_channels(data.modded_channels),
                ad_hoc_channels: data
                    .ad_hoc_channels
                    .iter()
                    .filter_map(|channel| Identifier::parse(channel).ok())
                    .collect(),
                channel: data.channel,
                data: Bytes::from(data.data),
                responses: data
                    .responses
                    .into_iter()
                    .map(|payload| (payload.channel, Bytes::from(payload.data)))
                    .collect(),
            },
            _ => panic!("unexpected event type"),
        }
    }

    // Only the guest's own additions to the response queue come back; the queued responses of
    // earlier handlers and the identity and channel fields stay as they were.
    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        if let Event::ConfigCustomPayloadEvent(data) = event {
            let queued = self.responses.len();
            self.responses.extend(
                data.responses
                    .into_iter()
                    .skip(queued)
                    .map(|payload| (payload.channel, Bytes::from(payload.data))),
            );
        } else {
            cleanup_event(&event, state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_util::text::TextComponent;
    use wasmtime::component::Resource;

    #[test]
    fn server_list_ping_applies_and_consumes_returned_resources() {
        let mut state = PluginHostState::new();
        let original_motd = TextComponent::text("Original");
        let returned_motd = TextComponent::text("Returned");
        let mut event = ServerListPingEvent::new(
            "original.example".to_string(),
            "127.0.0.1:25565"
                .parse()
                .expect("test address should parse"),
            original_motd,
            20,
            1,
            None,
        );
        let motd = state
            .add(returned_motd.clone())
            .expect("text component resource should be inserted");
        let motd_rep = motd.rep();
        let returned = Event::ServerListPingEvent(ServerListPingEventData {
            hostname: "replacement.example".to_string(),
            address: ServerListPingAddress {
                host: "192.0.2.1".to_string(),
                port: 25_566,
            },
            motd,
            max_players: 40,
            num_players: 2,
            favicon: Some("data:image/png;base64,test".to_string()),
        });

        event.apply_wasm_event(returned, &mut state);

        assert_eq!(event.hostname(), "original.example");
        assert_eq!(event.address().host(), "127.0.0.1");
        assert_eq!(event.address().port(), 25_565);
        assert_eq!(event.motd, returned_motd);
        assert_eq!(event.max_players, 40);
        assert_eq!(event.num_players, 2);
        assert_eq!(event.favicon.as_deref(), Some("data:image/png;base64,test"));
        assert!(
            state
                .resource_table
                .get::<TextComponent>(&Resource::new_own(motd_rep))
                .is_err()
        );
    }

    #[test]
    fn config_custom_payload_appends_only_guest_responses() {
        let mut state = PluginHostState::new();
        let component = ModdedNetworkQueryComponent {
            id: Identifier::parse_static("probe:sync"),
            version: "2".to_string(),
            flow: Some(PacketFlow::Clientbound),
            optional: true,
        };
        let modded_channels = BTreeMap::from([(
            ConnectionProtocol::Configuration,
            BTreeSet::from([component]),
        )]);
        let mut event = ConfigCustomPayloadEvent {
            connection_id: 7,
            player_name: "Probe".to_string(),
            player_uuid: uuid::Uuid::from_u128(1),
            version: pumpkin_data::packet::CURRENT_MC_VERSION,
            connection_type: ConnectionType::NeoForge,
            modded_channels: modded_channels.clone(),
            ad_hoc_channels: Vec::new(),
            channel: "minecraft:brand".to_string(),
            data: Bytes::from_static(b"neoforge"),
            responses: vec![("native:reply".to_string(), Bytes::from_static(b"n"))],
        };
        let Event::ConfigCustomPayloadEvent(mut returned) = event.to_wasm_event(&mut state) else {
            panic!("expected a config custom payload event");
        };
        assert_eq!(
            from_wit_modded_channels(returned.modded_channels.clone()),
            modded_channels
        );
        returned.player_name = "Rewritten".to_string();
        returned.connection_type = WitConnectionType::Other;
        returned.modded_channels.clear();
        returned.channel = "probe:other".to_string();
        returned.responses = vec![OutgoingPayload {
            channel: "guest:reply".to_string(),
            data: b"g".to_vec(),
        }];
        returned.responses.push(OutgoingPayload {
            channel: "guest:second".to_string(),
            data: b"s".to_vec(),
        });

        event.apply_wasm_event(Event::ConfigCustomPayloadEvent(returned), &mut state);

        assert_eq!(event.player_name, "Probe");
        assert_eq!(event.connection_type, ConnectionType::NeoForge);
        assert_eq!(event.modded_channels, modded_channels);
        assert_eq!(event.channel, "minecraft:brand");
        assert_eq!(
            event.responses,
            vec![
                ("native:reply".to_string(), Bytes::from_static(b"n")),
                ("guest:second".to_string(), Bytes::from_static(b"s")),
            ]
        );
    }
}
