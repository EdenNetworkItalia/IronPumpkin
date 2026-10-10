use std::io::Write;

use indexmap::IndexMap;

use super::{MAX_PREALLOCATED, read_count, write_count};
use crate::{
    VarInt,
    ser::{NetworkReadExt, NetworkReadSliceExt, NetworkWriteExt, ReadingError, WritingError},
};

/// FML `NetworkedEnum.NetworkCheck`, sent by `name()` and read with `valueOf`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetworkCheck {
    Clientbound,
    Serverbound,
    Bidirectional,
}

impl NetworkCheck {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Clientbound => "CLIENTBOUND",
            Self::Serverbound => "SERVERBOUND",
            Self::Bidirectional => "BIDIRECTIONAL",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Clientbound, Self::Serverbound, Self::Bidirectional]
            .into_iter()
            .find(|check| check.name() == name)
    }

    fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        let name = read.get_str_borrowed()?;
        Self::from_name(name)
            .ok_or_else(|| ReadingError::Message(format!("Invalid NetworkCheck {name}")))
    }
}

/// Mirrors `CheckExtensibleEnums.ExtensionData`: the values mods added to an extensible enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionData {
    pub vanilla_count: i32,
    pub total_count: i32,
    pub entries: Vec<String>,
}

impl ExtensionData {
    fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        let vanilla_count = read.get_var_int()?.0;
        let total_count = read.get_var_int()?.0;
        let count = read_count(read)?;
        let mut entries = Vec::with_capacity(count.min(MAX_PREALLOCATED));
        for _ in 0..count {
            entries.push(read.get_str_borrowed()?.to_owned());
        }
        Ok(Self {
            vanilla_count,
            total_count,
            entries,
        })
    }

    fn write(&self, write: &mut impl Write) -> Result<(), WritingError> {
        write.write_var_int(&VarInt(self.vanilla_count))?;
        write.write_var_int(&VarInt(self.total_count))?;
        write_count(write, self.entries.len())?;
        for entry in &self.entries {
            write.write_string(entry)?;
        }
        Ok(())
    }
}

/// Mirrors `CheckExtensibleEnums.EnumEntry`: one networked extensible enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumEntry {
    pub class_name: String,
    pub network_check: NetworkCheck,
    /// `None` when no mod extended the enum.
    pub data: Option<ExtensionData>,
}

impl EnumEntry {
    fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        Ok(Self {
            class_name: read.get_str_borrowed()?.to_owned(),
            network_check: NetworkCheck::read(read)?,
            data: read.get_option(ExtensionData::read)?,
        })
    }

    fn write(&self, write: &mut impl Write) -> Result<(), WritingError> {
        write.write_string(&self.class_name)?;
        write.write_string(self.network_check.name())?;
        write.write_option(&self.data, |w, data| data.write(w))
    }
}

/// Mirrors `ExtensibleEnumDataPayload`: the networked extensible enums of the server, by class name.
///
/// The wire form is a list of entries, written in the `HashMap` order of `NeoForge`. The map keeps
/// the decoded order so that a decoded payload writes back to the same bytes. A repeated class name
/// fails the decode, like `Collectors.toMap`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExtensibleEnumDataPayload {
    pub enum_entries: IndexMap<String, EnumEntry>,
}

impl ExtensibleEnumDataPayload {
    pub const CHANNEL: &'static str = "neoforge:extensible_enum_data";

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        let count = read_count(read)?;
        let mut enum_entries = IndexMap::with_capacity(count.min(MAX_PREALLOCATED));
        for _ in 0..count {
            let entry = EnumEntry::read(read)?;
            if let Some(entry) = enum_entries.insert(entry.class_name.clone(), entry) {
                return Err(ReadingError::Message(format!(
                    "Duplicate enum entry {}",
                    entry.class_name
                )));
            }
        }
        Ok(Self { enum_entries })
    }

    pub fn write(&self, mut write: impl Write) -> Result<(), WritingError> {
        write_count(&mut write, self.enum_entries.len())?;
        for entry in self.enum_entries.values() {
            entry.write(&mut write)?;
        }
        Ok(())
    }
}

/// Mirrors `ExtensibleEnumAcknowledgePayload`: the client accepted the enum entries. It has no body
/// (`StreamCodec.unit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExtensibleEnumAcknowledgePayload;

impl ExtensibleEnumAcknowledgePayload {
    pub const CHANNEL: &'static str = "neoforge:extensible_enum_ack";

    pub const fn read(_read: &mut &[u8]) -> Result<Self, ReadingError> {
        Ok(Self)
    }

    pub fn write(&self, _write: impl Write) -> Result<(), WritingError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{decode_exact, tests::hex_fixture};
    use super::*;

    /// The clientbound body of run (b), seq 93.
    fn captured() -> Vec<u8> {
        hex_fixture(include_str!("fixtures/run-b-extensible-enum-data.hex"))
    }

    #[test]
    fn decodes_captured_payload() {
        let payload = decode_exact(&captured(), ExtensibleEnumDataPayload::read).unwrap();
        assert_eq!(payload.enum_entries.len(), 9);
        assert!(payload.enum_entries.values().all(|e| e.data.is_none()));
        assert!(
            payload
                .enum_entries
                .iter()
                .all(|(name, entry)| *name == entry.class_name)
        );
        let checks = |check| {
            payload
                .enum_entries
                .values()
                .filter(|e| e.network_check == check)
                .count()
        };
        assert_eq!(checks(NetworkCheck::Clientbound), 6);
        assert_eq!(checks(NetworkCheck::Bidirectional), 3);
        let bidirectional: Vec<&str> = payload
            .enum_entries
            .values()
            .filter(|e| e.network_check == NetworkCheck::Bidirectional)
            .map(|e| e.class_name.as_str())
            .collect();
        assert_eq!(
            bidirectional,
            [
                "net.minecraft.world.item.ItemUseAnimation",
                "net.minecraft.world.item.component.FireworkExplosion$Shape",
                "net.minecraft.world.item.Rarity",
            ]
        );
    }

    #[test]
    fn writes_captured_payload_back() {
        let bytes = captured();
        let payload = decode_exact(&bytes, ExtensibleEnumDataPayload::read).unwrap();
        let mut written = Vec::new();
        payload.write(&mut written).unwrap();
        assert_eq!(written, bytes);
    }

    fn extended_sample() -> (ExtensibleEnumDataPayload, Vec<u8>) {
        let entry = EnumEntry {
            class_name: "a.B".to_owned(),
            network_check: NetworkCheck::Serverbound,
            data: Some(ExtensionData {
                vanilla_count: 3,
                total_count: 4,
                entries: vec!["MYMOD_X".to_owned()],
            }),
        };
        let payload = ExtensibleEnumDataPayload {
            enum_entries: IndexMap::from([(entry.class_name.clone(), entry)]),
        };
        let bytes = [
            &[0x01][..], // collection size: VarInt 1
            &[0x03],     // className: STRING_UTF8 length 3
            b"a.B",
            &[0x0b], // networkCheck: STRING_UTF8 of name(), length 11
            b"SERVERBOUND",
            &[0x01], // data: optional present
            &[0x03], // vanillaCount: VAR_INT 3
            &[0x04], // totalCount: VAR_INT 4
            &[0x01], // entries: collection size VarInt 1
            &[0x07], // STRING_UTF8 length 7
            b"MYMOD_X",
        ]
        .concat();
        (payload, bytes)
    }

    #[test]
    fn extended_decodes_java_bytes() {
        let (expected, bytes) = extended_sample();
        let mut read = bytes.as_slice();
        assert_eq!(
            ExtensibleEnumDataPayload::read(&mut read).unwrap(),
            expected
        );
        assert!(read.is_empty());
    }

    #[test]
    fn extended_writes_java_bytes() {
        let (payload, expected) = extended_sample();
        let mut bytes = Vec::new();
        payload.write(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }

    #[test]
    fn rejects_unknown_network_check() {
        // NetworkCheck.valueOf throws on a name that is not a constant.
        let bytes = [&[0x01, 0x03][..], b"a.B", &[0x04], b"BOTH", &[0x00]].concat();
        assert!(ExtensibleEnumDataPayload::read(&mut bytes.as_slice()).is_err());
    }

    #[test]
    fn rejects_duplicate_class_name() {
        let entry = [&[0x03][..], b"a.B", &[0x0b], b"CLIENTBOUND", &[0x00]].concat();
        let bytes = [&[0x02][..], &entry, &entry].concat();
        assert!(ExtensibleEnumDataPayload::read(&mut bytes.as_slice()).is_err());
    }

    #[test]
    fn rejects_hostile_lengths() {
        // A collection size of 2^31 - 1 with no entries behind it.
        let bytes = [0xff, 0xff, 0xff, 0xff, 0x07];
        assert!(ExtensibleEnumDataPayload::read(&mut &bytes[..]).is_err());
        // Extension data whose entry list claims 100 names in 1 remaining byte.
        let bytes = [
            &[0x01, 0x03][..],
            b"a.B",
            &[0x0b],
            b"CLIENTBOUND",
            &[0x01, 0x03, 0x04, 0x64, 0x00],
        ]
        .concat();
        assert!(ExtensibleEnumDataPayload::read(&mut bytes.as_slice()).is_err());
    }

    #[test]
    fn ack_is_empty() {
        // The serverbound body of run (b), seq 94, is empty.
        decode_exact(&[], ExtensibleEnumAcknowledgePayload::read).unwrap();
        let mut bytes = Vec::new();
        ExtensibleEnumAcknowledgePayload.write(&mut bytes).unwrap();
        assert!(bytes.is_empty());
    }
}
