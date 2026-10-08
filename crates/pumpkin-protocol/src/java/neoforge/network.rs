use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use pumpkin_data::packet::CURRENT_MC_VERSION;
use pumpkin_util::{identifier::Identifier, text::TextComponent};

use super::{
    ConnectionProtocol, PacketFlow, read_count, read_identifier, write_count, write_identifier,
};
use crate::ser::{
    NetworkReadExt, NetworkReadSliceExt, NetworkWriteExt, ReadingError, WritingError,
};

/// Mirrors `ModdedNetworkQueryComponent`: one channel a side registered, as sent in a query.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModdedNetworkQueryComponent {
    pub id: Identifier,
    pub version: String,
    pub flow: Option<PacketFlow>,
    pub optional: bool,
}

impl ModdedNetworkQueryComponent {
    fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        Ok(Self {
            id: read_identifier(read)?,
            version: read.get_str_borrowed()?.to_owned(),
            flow: read.get_option(PacketFlow::read)?,
            optional: read.get_bool()?,
        })
    }

    fn write(&self, write: &mut impl Write) -> Result<(), WritingError> {
        write_identifier(write, &self.id)?;
        write.write_string(&self.version)?;
        write.write_option(&self.flow, |w, flow| flow.write(w))?;
        write.write_bool(self.optional)
    }
}

/// Mirrors `ModdedNetworkQueryPayload`: the channels each side registered, per protocol.
///
/// The server sends it to ask for the client's channels and the client replies with its own.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModdedNetworkQueryPayload {
    pub queries: BTreeMap<ConnectionProtocol, BTreeSet<ModdedNetworkQueryComponent>>,
}

impl ModdedNetworkQueryPayload {
    pub const CHANNEL: &'static str = "neoforge:register";

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        let count = read_count(read)?;
        let mut queries = BTreeMap::new();
        for _ in 0..count {
            let protocol = ConnectionProtocol::read(read)?;
            let len = read_count(read)?;
            let mut components = BTreeSet::new();
            for _ in 0..len {
                components.insert(ModdedNetworkQueryComponent::read(read)?);
            }
            queries.insert(protocol, components);
        }
        Ok(Self { queries })
    }

    pub fn write(&self, mut write: impl Write) -> Result<(), WritingError> {
        write_count(&mut write, self.queries.len())?;
        for (protocol, components) in &self.queries {
            protocol.write(&mut write)?;
            write_count(&mut write, components.len())?;
            for component in components {
                component.write(&mut write)?;
            }
        }
        Ok(())
    }
}

/// Mirrors `NetworkChannel`: a channel both sides agreed on and the version they chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkChannel {
    pub id: Identifier,
    pub chosen_version: String,
}

impl NetworkChannel {
    fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        Ok(Self {
            id: read_identifier(read)?,
            chosen_version: read.get_str_borrowed()?.to_owned(),
        })
    }

    fn write(&self, write: &mut impl Write) -> Result<(), WritingError> {
        write_identifier(write, &self.id)?;
        write.write_string(&self.chosen_version)
    }
}

/// Mirrors `NetworkPayloadSetup`: the negotiated channels, per protocol and channel id.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkPayloadSetup {
    pub channels: BTreeMap<ConnectionProtocol, BTreeMap<Identifier, NetworkChannel>>,
}

impl NetworkPayloadSetup {
    fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        let count = read_count(read)?;
        let mut channels = BTreeMap::new();
        for _ in 0..count {
            let protocol = ConnectionProtocol::read(read)?;
            let len = read_count(read)?;
            let mut protocol_channels = BTreeMap::new();
            for _ in 0..len {
                let id = read_identifier(read)?;
                protocol_channels.insert(id, NetworkChannel::read(read)?);
            }
            channels.insert(protocol, protocol_channels);
        }
        Ok(Self { channels })
    }

    fn write(&self, write: &mut impl Write) -> Result<(), WritingError> {
        write_count(write, self.channels.len())?;
        for (protocol, protocol_channels) in &self.channels {
            protocol.write(write)?;
            write_count(write, protocol_channels.len())?;
            for (id, channel) in protocol_channels {
                write_identifier(write, id)?;
                channel.write(write)?;
            }
        }
        Ok(())
    }
}

/// Mirrors `ModdedNetworkPayload`: the server's result of the channel negotiation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModdedNetworkPayload {
    pub setup: NetworkPayloadSetup,
}

impl ModdedNetworkPayload {
    pub const CHANNEL: &'static str = "neoforge:network";

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        Ok(Self {
            setup: NetworkPayloadSetup::read(read)?,
        })
    }

    pub fn write(&self, mut write: impl Write) -> Result<(), WritingError> {
        self.setup.write(&mut write)
    }
}

/// Mirrors `ModdedNetworkSetupFailedPayload`: why the negotiation failed, per channel.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModdedNetworkSetupFailedPayload {
    pub failure_reasons: BTreeMap<Identifier, TextComponent>,
}

impl ModdedNetworkSetupFailedPayload {
    pub const CHANNEL: &'static str = "neoforge:modded_network_setup_failed";

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        let count = read_count(read)?;
        let mut failure_reasons = BTreeMap::new();
        for _ in 0..count {
            let id = read_identifier(read)?;
            // ComponentSerialization.TRUSTED_CONTEXT_FREE_STREAM_CODEC: a nameless network NBT tag.
            failure_reasons.insert(id, read.get_component(&CURRENT_MC_VERSION)?);
        }
        Ok(Self { failure_reasons })
    }

    pub fn write(&self, mut write: impl Write) -> Result<(), WritingError> {
        write_count(&mut write, self.failure_reasons.len())?;
        for (id, reason) in &self.failure_reasons {
            write_identifier(&mut write, id)?;
            write.write_component(reason, &CURRENT_MC_VERSION)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> Identifier {
        Identifier::parse(s).unwrap()
    }

    fn query_sample() -> (ModdedNetworkQueryPayload, Vec<u8>) {
        let payload = ModdedNetworkQueryPayload {
            queries: BTreeMap::from([
                (
                    ConnectionProtocol::Play,
                    BTreeSet::from([ModdedNetworkQueryComponent {
                        id: id("mymod:data"),
                        version: String::new(),
                        flow: None,
                        optional: true,
                    }]),
                ),
                (
                    ConnectionProtocol::Configuration,
                    BTreeSet::from([ModdedNetworkQueryComponent {
                        id: id("mymod:ping"),
                        version: "1".to_owned(),
                        flow: Some(PacketFlow::Clientbound),
                        optional: false,
                    }]),
                ),
            ]),
        };
        let bytes = [
            &[0x02][..], // map size: VarInt 2
            &[0x01],     // key: ConnectionProtocol.PLAY, idMapper ordinal VarInt 1
            &[0x01],     // value: HashSet size VarInt 1
            &[0x0a],     // id: STRING_UTF8 length VarInt 10
            b"mymod:data",
            &[0x00], // version: STRING_UTF8 ""
            &[0x00], // flow: Optional, BOOL false
            &[0x01], // optional: BOOL true
            &[0x04], // key: ConnectionProtocol.CONFIGURATION, ordinal VarInt 4
            &[0x01], // value: set size 1
            &[0x0a],
            b"mymod:ping",
            &[0x01, b'1'], // version: STRING_UTF8 "1"
            &[0x01, 0x01], // flow: Optional present, PacketFlow.CLIENTBOUND writeEnum ordinal VarInt 1
            &[0x00],       // optional: BOOL false
        ]
        .concat();
        (payload, bytes)
    }

    #[test]
    fn query_decodes_java_bytes() {
        let (expected, bytes) = query_sample();
        let mut read = bytes.as_slice();
        assert_eq!(
            ModdedNetworkQueryPayload::read(&mut read).unwrap(),
            expected
        );
        assert!(read.is_empty());
    }

    #[test]
    fn query_writes_java_bytes() {
        let (payload, expected) = query_sample();
        let mut bytes = Vec::new();
        payload.write(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }

    #[test]
    fn query_rejects_unknown_protocol_ordinal() {
        // map size 1, key ordinal 5: ConnectionProtocol.values()[5] throws in Java.
        let bytes = [0x01, 0x05, 0x00];
        assert!(ModdedNetworkQueryPayload::read(&mut &bytes[..]).is_err());
    }

    fn network_sample() -> (ModdedNetworkPayload, Vec<u8>) {
        let payload = ModdedNetworkPayload {
            setup: NetworkPayloadSetup {
                channels: BTreeMap::from([(
                    ConnectionProtocol::Play,
                    BTreeMap::from([(
                        id("mymod:data"),
                        NetworkChannel {
                            id: id("mymod:data"),
                            chosen_version: "1".to_owned(),
                        },
                    )]),
                )]),
            },
        };
        let bytes = [
            &[0x01][..], // outer map size: VarInt 1
            &[0x01],     // key: ConnectionProtocol.PLAY ordinal VarInt 1
            &[0x01],     // inner HashMap size: VarInt 1
            &[0x0a],     // inner key: Identifier, STRING_UTF8 length 10
            b"mymod:data",
            &[0x0a], // NetworkChannel.id: Identifier, length 10
            b"mymod:data",
            &[0x01, b'1'], // NetworkChannel.chosenVersion: STRING_UTF8 "1"
        ]
        .concat();
        (payload, bytes)
    }

    #[test]
    fn network_decodes_java_bytes() {
        let (expected, bytes) = network_sample();
        let mut read = bytes.as_slice();
        assert_eq!(ModdedNetworkPayload::read(&mut read).unwrap(), expected);
        assert!(read.is_empty());
    }

    #[test]
    fn network_writes_java_bytes() {
        let (payload, expected) = network_sample();
        let mut bytes = Vec::new();
        payload.write(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }

    fn setup_failed_sample() -> (ModdedNetworkSetupFailedPayload, Vec<u8>) {
        let payload = ModdedNetworkSetupFailedPayload {
            failure_reasons: BTreeMap::from([(id("mymod:data"), TextComponent::text("nope"))]),
        };
        let bytes = [
            &[0x01][..], // map size: VarInt 1
            &[0x0a],     // key: Identifier, STRING_UTF8 length 10
            b"mymod:data",
            // value: a literal without style encodes as a bare NBT string tag:
            // tag type 8, u16 length 4, then the UTF-8 bytes.
            &[0x08, 0x00, 0x04],
            b"nope",
        ]
        .concat();
        (payload, bytes)
    }

    #[test]
    fn setup_failed_decodes_java_bytes() {
        let (expected, bytes) = setup_failed_sample();
        let mut read = bytes.as_slice();
        assert_eq!(
            ModdedNetworkSetupFailedPayload::read(&mut read).unwrap(),
            expected
        );
        assert!(read.is_empty());
    }

    #[test]
    fn setup_failed_writes_java_bytes() {
        let (payload, expected) = setup_failed_sample();
        let mut bytes = Vec::new();
        payload.write(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }
}
