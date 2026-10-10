use std::collections::BTreeSet;
use std::io::Write;

use pumpkin_util::identifier::Identifier;

use super::{read_count, read_identifier, write_count, write_identifier};
use crate::ser::{ReadingError, WritingError};

/// Mirrors `FeatureFlagDataPayload`: the modded feature flags the server has enabled.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FeatureFlagDataPayload {
    pub modded_flags: BTreeSet<Identifier>,
}

impl FeatureFlagDataPayload {
    pub const CHANNEL: &'static str = "neoforge:feature_flags";

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        let count = read_count(read)?;
        let mut modded_flags = BTreeSet::new();
        for _ in 0..count {
            modded_flags.insert(read_identifier(read)?);
        }
        Ok(Self { modded_flags })
    }

    pub fn write(&self, mut write: impl Write) -> Result<(), WritingError> {
        write_count(&mut write, self.modded_flags.len())?;
        for flag in &self.modded_flags {
            write_identifier(&mut write, flag)?;
        }
        Ok(())
    }
}

/// Mirrors `FeatureFlagAcknowledgePayload`: the client's modded feature flags match. It has no body
/// (`StreamCodec.unit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FeatureFlagAcknowledgePayload;

impl FeatureFlagAcknowledgePayload {
    pub const CHANNEL: &'static str = "neoforge:feature_flags_ack";

    pub const fn read(_read: &mut &[u8]) -> Result<Self, ReadingError> {
        Ok(Self)
    }

    pub fn write(&self, _write: impl Write) -> Result<(), WritingError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::decode_exact;
    use super::*;

    fn sample() -> (FeatureFlagDataPayload, Vec<u8>) {
        let payload = FeatureFlagDataPayload {
            modded_flags: BTreeSet::from([
                Identifier::parse("mymod:alpha").unwrap(),
                Identifier::parse("mymod:beta").unwrap(),
            ]),
        };
        let bytes = [
            &[0x02][..], // HashSet size: VarInt 2
            &[0x0b],     // Identifier, STRING_UTF8 length 11
            b"mymod:alpha",
            &[0x0a], // Identifier, length 10
            b"mymod:beta",
        ]
        .concat();
        (payload, bytes)
    }

    #[test]
    fn decodes_java_bytes() {
        let (expected, bytes) = sample();
        let mut read = bytes.as_slice();
        assert_eq!(FeatureFlagDataPayload::read(&mut read).unwrap(), expected);
        assert!(read.is_empty());
    }

    #[test]
    fn writes_java_bytes() {
        let (payload, expected) = sample();
        let mut bytes = Vec::new();
        payload.write(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }

    #[test]
    fn rejects_invalid_identifier() {
        // Identifier.parse throws on an upper-case namespace, so the decode fails.
        let bytes = [&[0x01, 0x07][..], b"Bad:one"].concat();
        assert!(FeatureFlagDataPayload::read(&mut bytes.as_slice()).is_err());
    }

    #[test]
    fn decodes_captured_payload() {
        // Run (b), seq 95: the server has no modded flags.
        let payload = decode_exact(&[0x00], FeatureFlagDataPayload::read).unwrap();
        assert!(payload.modded_flags.is_empty());
    }

    #[test]
    fn ack_is_empty() {
        // The serverbound body of run (b), seq 96, is empty.
        decode_exact(&[], FeatureFlagAcknowledgePayload::read).unwrap();
        let mut bytes = Vec::new();
        FeatureFlagAcknowledgePayload.write(&mut bytes).unwrap();
        assert!(bytes.is_empty());
    }
}
