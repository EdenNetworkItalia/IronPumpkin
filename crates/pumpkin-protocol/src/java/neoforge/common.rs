use std::collections::BTreeSet;
use std::io::Write;

use pumpkin_util::identifier::Identifier;

use super::{
    ConnectionProtocol, MAX_PREALLOCATED, read_count, read_identifier, write_count,
    write_identifier,
};
use crate::{
    VarInt,
    ser::{NetworkReadExt, NetworkReadSliceExt, NetworkWriteExt, ReadingError, WritingError},
};

/// Mirrors `CommonVersionPayload`: the common networking versions the sender supports.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommonVersionPayload {
    pub versions: Vec<i32>,
}

impl CommonVersionPayload {
    pub const CHANNEL: &'static str = "c:version";

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        let count = read_count(read)?;
        let mut versions = Vec::with_capacity(count.min(MAX_PREALLOCATED));
        for _ in 0..count {
            versions.push(read.get_var_int()?.0);
        }
        Ok(Self { versions })
    }

    pub fn write(&self, mut write: impl Write) -> Result<(), WritingError> {
        write.write_list(&self.versions, |w, version| {
            w.write_var_int(&VarInt(*version))
        })
    }
}

/// Mirrors `CommonRegisterPayload`: the channels the sender has for one protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonRegisterPayload {
    pub version: i32,
    /// `None` when the protocol id is unknown: `NeoForge`'s `protocolById` returns `null` there
    /// instead of failing the decode.
    pub protocol: Option<ConnectionProtocol>,
    pub channels: BTreeSet<Identifier>,
}

impl CommonRegisterPayload {
    pub const CHANNEL: &'static str = "c:register";

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        let version = read.get_var_int()?.0;
        let protocol = ConnectionProtocol::from_id(read.get_str_borrowed()?);
        let count = read_count(read)?;
        let mut channels = BTreeSet::new();
        for _ in 0..count {
            channels.insert(read_identifier(read)?);
        }
        Ok(Self {
            version,
            protocol,
            channels,
        })
    }

    pub fn write(&self, mut write: impl Write) -> Result<(), WritingError> {
        let protocol = self.protocol.ok_or_else(|| {
            WritingError::Message("c:register needs a connection protocol".to_owned())
        })?;
        write.write_var_int(&VarInt(self.version))?;
        write.write_string(protocol.id())?;
        write_count(&mut write, self.channels.len())?;
        for channel in &self.channels {
            write_identifier(&mut write, channel)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version_sample() -> (CommonVersionPayload, Vec<u8>) {
        let payload = CommonVersionPayload {
            versions: vec![1, 300],
        };
        let bytes = vec![
            0x02, // list size: VarInt 2
            0x01, // VAR_INT 1
            0xac, 0x02, // VAR_INT 300: low 7 bits 0x2c | 0x80, then 300 >> 7 = 2
        ];
        (payload, bytes)
    }

    #[test]
    fn version_decodes_java_bytes() {
        let (expected, bytes) = version_sample();
        let mut read = bytes.as_slice();
        assert_eq!(CommonVersionPayload::read(&mut read).unwrap(), expected);
        assert!(read.is_empty());
    }

    #[test]
    fn version_writes_java_bytes() {
        let (payload, expected) = version_sample();
        let mut bytes = Vec::new();
        payload.write(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }

    fn register_sample() -> (CommonRegisterPayload, Vec<u8>) {
        let payload = CommonRegisterPayload {
            version: 1,
            protocol: Some(ConnectionProtocol::Play),
            channels: BTreeSet::from([Identifier::parse("mymod:data").unwrap()]),
        };
        let bytes = [
            &[0x01][..], // version: VAR_INT 1
            &[0x04],     // protocol: STRING_UTF8 of ConnectionProtocol.id(), length 4
            b"play",
            &[0x01], // channels: HashSet size VarInt 1
            &[0x0a], // Identifier, STRING_UTF8 length 10
            b"mymod:data",
        ]
        .concat();
        (payload, bytes)
    }

    #[test]
    fn register_decodes_java_bytes() {
        let (expected, bytes) = register_sample();
        let mut read = bytes.as_slice();
        assert_eq!(CommonRegisterPayload::read(&mut read).unwrap(), expected);
        assert!(read.is_empty());
    }

    #[test]
    fn register_writes_java_bytes() {
        let (payload, expected) = register_sample();
        let mut bytes = Vec::new();
        payload.write(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }

    #[test]
    fn register_keeps_unknown_protocol_as_none() {
        // version 1, protocol "bogus", no channels.
        let bytes = [&[0x01, 0x05][..], b"bogus", &[0x00]].concat();
        let payload = CommonRegisterPayload::read(&mut bytes.as_slice()).unwrap();
        assert_eq!(payload.protocol, None);
    }
}
