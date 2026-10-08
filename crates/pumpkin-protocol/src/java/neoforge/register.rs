use std::collections::BTreeSet;
use std::io::Write;

use pumpkin_util::identifier::Identifier;

use crate::ser::{NetworkWriteExt, ReadingError, WritingError};

/// Mirrors `MinecraftRegisterPayload`: channels the sender starts listening on.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MinecraftRegisterPayload {
    pub channels: BTreeSet<Identifier>,
}

impl MinecraftRegisterPayload {
    pub const CHANNEL: &'static str = "minecraft:register";

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        Ok(Self {
            channels: read_channels(read),
        })
    }

    pub fn write(&self, write: impl Write) -> Result<(), WritingError> {
        write_channels(write, &self.channels)
    }
}

/// Mirrors `MinecraftUnregisterPayload`: channels the sender stops listening on.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MinecraftUnregisterPayload {
    pub channels: BTreeSet<Identifier>,
}

impl MinecraftUnregisterPayload {
    pub const CHANNEL: &'static str = "minecraft:unregister";

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        Ok(Self {
            channels: read_channels(read),
        })
    }

    pub fn write(&self, write: impl Write) -> Result<(), WritingError> {
        write_channels(write, &self.channels)
    }
}

/// `DinnerboneProtocolUtils.readChannels`: NUL-terminated identifiers up to the end of the body.
///
/// The last one may lack its NUL. Empty and invalid entries are skipped instead of failing the
/// payload, as `NeoForge` does.
fn read_channels(read: &mut &[u8]) -> BTreeSet<Identifier> {
    let channels = read
        .split(|&b| b == 0)
        .filter_map(|entry| core::str::from_utf8(entry).ok())
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| Identifier::parse(entry).ok())
        .collect();
    *read = &[];
    channels
}

/// `DinnerboneProtocolUtils.writeChannels`: each identifier followed by a NUL.
fn write_channels(
    mut write: impl Write,
    channels: &BTreeSet<Identifier>,
) -> Result<(), WritingError> {
    for channel in channels {
        write.write_slice(channel.to_string().as_bytes())?;
        write.write_u8(0)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_decodes_java_bytes() {
        // Two NUL-terminated channels, an empty entry and an invalid one, the last without NUL.
        let bytes = b"neoforge:register\0\0Bad Channel\0c:version";
        let mut read = &bytes[..];
        let payload = MinecraftRegisterPayload::read(&mut read).unwrap();
        assert_eq!(
            payload.channels,
            BTreeSet::from([
                Identifier::parse_static("c:version"),
                Identifier::parse_static("neoforge:register"),
            ])
        );
        assert!(read.is_empty());
    }

    #[test]
    fn unregister_writes_java_bytes() {
        let payload = MinecraftUnregisterPayload {
            channels: BTreeSet::from([
                Identifier::parse_static("minecraft:register"),
                Identifier::parse_static("minecraft:unregister"),
            ]),
        };
        let mut bytes = Vec::new();
        payload.write(&mut bytes).unwrap();
        assert_eq!(bytes, b"minecraft:register\0minecraft:unregister\0");
    }
}
