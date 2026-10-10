use std::collections::BTreeMap;
use std::io::Write;

use pumpkin_util::identifier::Identifier;

use super::{MAX_PREALLOCATED, read_count, read_identifier, write_count, write_identifier};
use crate::{
    VarInt,
    ser::{NetworkReadExt, NetworkWriteExt, ReadingError, WritingError},
};

/// Mirrors `FrozenRegistrySyncStartPayload`: the registries the server is about to send.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FrozenRegistrySyncStartPayload {
    pub to_access: Vec<Identifier>,
}

impl FrozenRegistrySyncStartPayload {
    pub const CHANNEL: &'static str = "neoforge:frozen_registry_sync_start";

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        let count = read_count(read)?;
        let mut to_access = Vec::with_capacity(count.min(MAX_PREALLOCATED));
        for _ in 0..count {
            to_access.push(read_identifier(read)?);
        }
        Ok(Self { to_access })
    }

    pub fn write(&self, mut write: impl Write) -> Result<(), WritingError> {
        write.write_list(&self.to_access, |w, id| write_identifier(w, id))
    }
}

/// Mirrors `RegistrySnapshot`'s network form: numeric ids and aliases of one registry.
///
/// Both maps are sorted like the Java side (`Int2ObjectRBTreeMap`, and a `TreeMap` ordered by
/// `Identifier::compareNamespaced`), so the encoded entry order matches `NeoForge`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RegistrySnapshot {
    pub ids: BTreeMap<i32, Identifier>,
    pub aliases: BTreeMap<Identifier, Identifier>,
}

impl RegistrySnapshot {
    /// A snapshot with these numeric ids and no aliases, the shape of
    /// `pumpkin_data::dynamic::ContentTables::block_snapshot` and its item and entity type
    /// siblings.
    pub fn from_ids(ids: impl IntoIterator<Item = (i32, Identifier)>) -> Self {
        Self {
            ids: ids.into_iter().collect(),
            aliases: BTreeMap::new(),
        }
    }

    fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        let count = read_count(read)?;
        let mut ids = BTreeMap::new();
        for _ in 0..count {
            let id = read.get_var_int()?.0;
            ids.insert(id, read_identifier(read)?);
        }
        let count = read_count(read)?;
        let mut aliases = BTreeMap::new();
        for _ in 0..count {
            let from = read_identifier(read)?;
            aliases.insert(from, read_identifier(read)?);
        }
        Ok(Self { ids, aliases })
    }

    fn write(&self, write: &mut impl Write) -> Result<(), WritingError> {
        write_count(write, self.ids.len())?;
        for (id, key) in &self.ids {
            write.write_var_int(&VarInt(*id))?;
            write_identifier(write, key)?;
        }
        write_count(write, self.aliases.len())?;
        for (from, to) in &self.aliases {
            write_identifier(write, from)?;
            write_identifier(write, to)?;
        }
        Ok(())
    }
}

/// Mirrors `FrozenRegistryPayload`: the snapshot of one frozen registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenRegistryPayload {
    pub registry_name: Identifier,
    pub snapshot: RegistrySnapshot,
}

impl FrozenRegistryPayload {
    pub const CHANNEL: &'static str = "neoforge:frozen_registry";

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        Ok(Self {
            registry_name: read_identifier(read)?,
            snapshot: RegistrySnapshot::read(read)?,
        })
    }

    pub fn write(&self, mut write: impl Write) -> Result<(), WritingError> {
        write_identifier(&mut write, &self.registry_name)?;
        self.snapshot.write(&mut write)
    }
}

/// Mirrors `FrozenRegistrySyncCompletedPayload`: all frozen registries were sent. It has no body
/// (`StreamCodec.unit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrozenRegistrySyncCompletedPayload;

impl FrozenRegistrySyncCompletedPayload {
    pub const CHANNEL: &'static str = "neoforge:frozen_registry_sync_completed";

    pub const fn read(_read: &mut &[u8]) -> Result<Self, ReadingError> {
        Ok(Self)
    }

    pub fn write(&self, _write: impl Write) -> Result<(), WritingError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> Identifier {
        Identifier::parse(s).unwrap()
    }

    fn sync_start_sample() -> (FrozenRegistrySyncStartPayload, Vec<u8>) {
        let payload = FrozenRegistrySyncStartPayload {
            to_access: vec![id("minecraft:item"), id("mymod:things")],
        };
        let bytes = [
            &[0x02][..], // list size: VarInt 2
            &[0x0e],     // Identifier, STRING_UTF8 length 14
            b"minecraft:item",
            &[0x0c], // Identifier, length 12
            b"mymod:things",
        ]
        .concat();
        (payload, bytes)
    }

    #[test]
    fn sync_start_decodes_java_bytes() {
        let (expected, bytes) = sync_start_sample();
        let mut read = bytes.as_slice();
        assert_eq!(
            FrozenRegistrySyncStartPayload::read(&mut read).unwrap(),
            expected
        );
        assert!(read.is_empty());
    }

    #[test]
    fn sync_start_writes_java_bytes() {
        let (payload, expected) = sync_start_sample();
        let mut bytes = Vec::new();
        payload.write(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }

    fn frozen_registry_sample() -> (FrozenRegistryPayload, Vec<u8>) {
        let payload = FrozenRegistryPayload {
            registry_name: id("minecraft:item"),
            snapshot: RegistrySnapshot {
                ids: BTreeMap::from([(0, id("minecraft:air")), (200, id("mymod:gem"))]),
                aliases: BTreeMap::from([(id("mymod:old_gem"), id("mymod:gem"))]),
            },
        };
        let bytes = [
            &[0x0e][..], // registryName: Identifier, length 14
            b"minecraft:item",
            &[0x02], // ids: map size VarInt 2
            &[0x00], // key: VAR_INT 0
            &[0x0d], // value: Identifier, length 13
            b"minecraft:air",
            &[0xc8, 0x01], // key: VAR_INT 200 (0x48 | 0x80, 200 >> 7 = 1)
            &[0x09],       // value: Identifier, length 9
            b"mymod:gem",
            &[0x01], // aliases: map size VarInt 1
            &[0x0d], // key: Identifier, length 13
            b"mymod:old_gem",
            &[0x09], // value: Identifier, length 9
            b"mymod:gem",
        ]
        .concat();
        (payload, bytes)
    }

    #[test]
    fn frozen_registry_decodes_java_bytes() {
        let (expected, bytes) = frozen_registry_sample();
        let mut read = bytes.as_slice();
        assert_eq!(FrozenRegistryPayload::read(&mut read).unwrap(), expected);
        assert!(read.is_empty());
    }

    #[test]
    fn frozen_registry_writes_java_bytes() {
        let (payload, expected) = frozen_registry_sample();
        let mut bytes = Vec::new();
        payload.write(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }
}
