use std::collections::{BTreeMap, BTreeSet};

use bytes::Bytes;
use pumpkin_macros::Event;
use pumpkin_protocol::java::neoforge::{ConnectionProtocol, ModdedNetworkQueryComponent};
use pumpkin_util::{identifier::Identifier, version::JavaMinecraftVersion};
use uuid::Uuid;

use crate::net::java::neoforge::ConnectionType;

/// A custom payload that a Java client sent during the configuration phase, before a `Player`
/// exists.
///
/// It fires after the server handled the payload itself, so the channel fields already include
/// what this payload declared. Only blocking handlers can answer, with
/// [`Self::send_custom_payload`].
#[derive(Event, Clone)]
pub struct ConfigCustomPayloadEvent {
    pub connection_id: u64,
    pub player_name: String,
    pub player_uuid: Uuid,
    pub version: JavaMinecraftVersion,
    pub connection_type: ConnectionType,
    /// The channels of the client's `neoforge:register` reply, per protocol.
    pub modded_channels: BTreeMap<ConnectionProtocol, BTreeSet<ModdedNetworkQueryComponent>>,
    /// The channels of `minecraft:register` minus those of `minecraft:unregister`.
    pub ad_hoc_channels: Vec<Identifier>,
    /// The payload channel identifier (e.g. `minecraft:brand`).
    pub channel: String,
    pub data: Bytes,
    /// Payloads that the server sends to this client, in order, after the handlers ran.
    pub responses: Vec<(String, Bytes)>,
}

impl ConfigCustomPayloadEvent {
    /// Queues a configuration-phase custom payload for this client.
    pub fn send_custom_payload(&mut self, channel: impl Into<String>, data: impl Into<Bytes>) {
        self.responses.push((channel.into(), data.into()));
    }
}
