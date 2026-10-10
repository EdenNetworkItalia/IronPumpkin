use std::io::Write;

use indexmap::IndexMap;
use pumpkin_util::identifier::Identifier;

use super::{MAX_PREALLOCATED, read_count, read_identifier, write_count, write_identifier};
use crate::ser::{NetworkReadExt, NetworkWriteExt, ReadingError, WritingError};

/// Mirrors `KnownRegistryDataMapsPayload.KnownDataMap`: one data map the server has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownDataMap {
    pub id: Identifier,
    pub mandatory: bool,
}

impl KnownDataMap {
    fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        Ok(Self {
            id: read_identifier(read)?,
            mandatory: read.get_bool()?,
        })
    }

    fn write(&self, write: &mut impl Write) -> Result<(), WritingError> {
        write_identifier(write, &self.id)?;
        write.write_bool(self.mandatory)
    }
}

/// Mirrors `KnownRegistryDataMapsPayload`: the data maps of the server, per registry key.
///
/// `NeoForge` writes the map in `HashMap` order. The map keeps the decoded order so that a decoded
/// payload writes back to the same bytes; a repeated registry replaces the earlier value, like
/// `HashMap.put` in `ByteBufCodecs.map`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KnownRegistryDataMapsPayload {
    pub data_maps: IndexMap<Identifier, Vec<KnownDataMap>>,
}

impl KnownRegistryDataMapsPayload {
    pub const CHANNEL: &'static str = "neoforge:known_registry_data_maps";

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        let count = read_count(read)?;
        let mut data_maps = IndexMap::with_capacity(count.min(MAX_PREALLOCATED));
        for _ in 0..count {
            let registry = read_identifier(read)?;
            let len = read_count(read)?;
            let mut maps = Vec::with_capacity(len.min(MAX_PREALLOCATED));
            for _ in 0..len {
                maps.push(KnownDataMap::read(read)?);
            }
            data_maps.insert(registry, maps);
        }
        Ok(Self { data_maps })
    }

    pub fn write(&self, mut write: impl Write) -> Result<(), WritingError> {
        write_count(&mut write, self.data_maps.len())?;
        for (registry, maps) in &self.data_maps {
            write_identifier(&mut write, registry)?;
            write_count(&mut write, maps.len())?;
            for map in maps {
                map.write(&mut write)?;
            }
        }
        Ok(())
    }
}

/// Mirrors `KnownRegistryDataMapsReplyPayload`: the data map ids the client knows, per registry
/// key. The map keeps the decoded order like [`KnownRegistryDataMapsPayload`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KnownRegistryDataMapsReplyPayload {
    pub data_maps: IndexMap<Identifier, Vec<Identifier>>,
}

impl KnownRegistryDataMapsReplyPayload {
    pub const CHANNEL: &'static str = "neoforge:known_registry_data_maps_reply";

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        let count = read_count(read)?;
        let mut data_maps = IndexMap::with_capacity(count.min(MAX_PREALLOCATED));
        for _ in 0..count {
            let registry = read_identifier(read)?;
            let len = read_count(read)?;
            let mut ids = Vec::with_capacity(len.min(MAX_PREALLOCATED));
            for _ in 0..len {
                ids.push(read_identifier(read)?);
            }
            data_maps.insert(registry, ids);
        }
        Ok(Self { data_maps })
    }

    pub fn write(&self, mut write: impl Write) -> Result<(), WritingError> {
        write_count(&mut write, self.data_maps.len())?;
        for (registry, ids) in &self.data_maps {
            write_identifier(&mut write, registry)?;
            write_count(&mut write, ids.len())?;
            for id in ids {
                write_identifier(&mut write, id)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{decode_exact, tests::hex_fixture};
    use super::*;

    fn id(s: &str) -> Identifier {
        Identifier::parse(s).unwrap()
    }

    /// The clientbound body of run (b), seq 91.
    fn captured() -> Vec<u8> {
        hex_fixture(include_str!("fixtures/run-b-known-registry-data-maps.hex"))
    }

    #[test]
    fn decodes_captured_payload() {
        let payload = decode_exact(&captured(), KnownRegistryDataMapsPayload::read).unwrap();
        let decoded: Vec<(String, Vec<String>)> = payload
            .data_maps
            .iter()
            .map(|(registry, maps)| {
                (
                    registry.to_string(),
                    maps.iter().map(|m| m.id.to_string()).collect(),
                )
            })
            .collect();
        let expected = [
            (
                "minecraft:block",
                &[
                    "neoforge:transformables",
                    "neoforge:oxidizables",
                    "neoforge:waxables",
                ][..],
            ),
            ("minecraft:item", &[]),
            (
                "minecraft:entity_type",
                &[
                    "neoforge:acceptable_villager_distances",
                    "neoforge:monster_room_mobs",
                    "neoforge:parrot_imitations",
                ],
            ),
            ("minecraft:worldgen/biome", &["neoforge:villager_types"]),
            ("minecraft:game_event", &["neoforge:vibration_frequencies"]),
            (
                "minecraft:villager_profession",
                &["neoforge:raid_hero_gifts"],
            ),
            (
                "minecraft:block_transformer",
                &["neoforge:block_transform_appenders"],
            ),
        ];
        let expected: Vec<(String, Vec<String>)> = expected
            .iter()
            .map(|(registry, maps)| {
                (
                    (*registry).to_owned(),
                    maps.iter().map(|m| (*m).to_owned()).collect(),
                )
            })
            .collect();
        assert_eq!(decoded, expected);
        assert_eq!(payload.data_maps.values().map(Vec::len).sum::<usize>(), 10);
        assert!(payload.data_maps.values().flatten().all(|m| !m.mandatory));
    }

    #[test]
    fn writes_captured_payload_back() {
        let bytes = captured();
        let payload = decode_exact(&bytes, KnownRegistryDataMapsPayload::read).unwrap();
        let mut written = Vec::new();
        payload.write(&mut written).unwrap();
        assert_eq!(written, bytes);
    }

    fn data_maps_sample() -> (KnownRegistryDataMapsPayload, Vec<u8>) {
        let payload = KnownRegistryDataMapsPayload {
            data_maps: IndexMap::from([(
                id("minecraft:item"),
                vec![KnownDataMap {
                    id: id("mymod:fuel"),
                    mandatory: true,
                }],
            )]),
        };
        let bytes = [
            &[0x01][..], // map size: VarInt 1
            &[0x0e],     // key: registryKey, Identifier length 14
            b"minecraft:item",
            &[0x01], // value: list size VarInt 1
            &[0x0a], // KnownDataMap.id: Identifier length 10
            b"mymod:fuel",
            &[0x01], // KnownDataMap.mandatory: BOOL true
        ]
        .concat();
        (payload, bytes)
    }

    #[test]
    fn data_maps_decodes_java_bytes() {
        let (expected, bytes) = data_maps_sample();
        let mut read = bytes.as_slice();
        assert_eq!(
            KnownRegistryDataMapsPayload::read(&mut read).unwrap(),
            expected
        );
        assert!(read.is_empty());
    }

    #[test]
    fn data_maps_writes_java_bytes() {
        let (payload, expected) = data_maps_sample();
        let mut bytes = Vec::new();
        payload.write(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }

    #[test]
    fn data_maps_repeated_registry_replaces_the_earlier_value() {
        let bytes = [
            &[0x02][..], // map size: VarInt 2
            &[0x0e],
            b"minecraft:item",
            &[0x00], // empty list
            &[0x0e],
            b"minecraft:item",
            &[0x01, 0x0a], // one data map
            b"mymod:fuel",
            &[0x00],
        ]
        .concat();
        let payload = decode_exact(&bytes, KnownRegistryDataMapsPayload::read).unwrap();
        assert_eq!(payload.data_maps.len(), 1);
        assert_eq!(payload.data_maps[&id("minecraft:item")].len(), 1);
    }

    #[test]
    fn data_maps_rejects_hostile_lengths() {
        // A map size of 2^31 - 1 with no entries behind it.
        let bytes = [0xff, 0xff, 0xff, 0xff, 0x07];
        assert!(KnownRegistryDataMapsPayload::read(&mut &bytes[..]).is_err());
        // One registry whose list claims 100 data maps in 2 remaining bytes.
        let bytes = [&[0x01, 0x0e][..], b"minecraft:item", &[0x64, 0x00, 0x00]].concat();
        assert!(KnownRegistryDataMapsPayload::read(&mut bytes.as_slice()).is_err());
    }

    #[test]
    fn reply_decodes_captured_payload() {
        // The serverbound body of run (b), seq 92: the client knows no data map.
        let bytes = [0x00];
        let payload = decode_exact(&bytes, KnownRegistryDataMapsReplyPayload::read).unwrap();
        assert!(payload.data_maps.is_empty());
        let mut written = Vec::new();
        payload.write(&mut written).unwrap();
        assert_eq!(written, bytes);
    }

    fn reply_sample() -> (KnownRegistryDataMapsReplyPayload, Vec<u8>) {
        let payload = KnownRegistryDataMapsReplyPayload {
            data_maps: IndexMap::from([(
                id("minecraft:block"),
                vec![id("neoforge:waxables"), id("mymod:glow")],
            )]),
        };
        let bytes = [
            &[0x01][..], // map size: VarInt 1
            &[0x0f],     // key: registryKey, Identifier length 15
            b"minecraft:block",
            &[0x02], // value: collection size VarInt 2
            &[0x11], // Identifier length 17
            b"neoforge:waxables",
            &[0x0a], // Identifier length 10
            b"mymod:glow",
        ]
        .concat();
        (payload, bytes)
    }

    #[test]
    fn reply_decodes_java_bytes() {
        let (expected, bytes) = reply_sample();
        let mut read = bytes.as_slice();
        assert_eq!(
            KnownRegistryDataMapsReplyPayload::read(&mut read).unwrap(),
            expected
        );
        assert!(read.is_empty());
    }

    #[test]
    fn reply_writes_java_bytes() {
        let (payload, expected) = reply_sample();
        let mut bytes = Vec::new();
        payload.write(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }

    #[test]
    fn reply_rejects_hostile_lengths() {
        let bytes = [0xff, 0xff, 0xff, 0xff, 0x07];
        assert!(KnownRegistryDataMapsReplyPayload::read(&mut &bytes[..]).is_err());
        // One registry whose collection claims 50 ids in 1 remaining byte.
        let bytes = [&[0x01, 0x0f][..], b"minecraft:block", &[0x32, 0x00]].concat();
        assert!(KnownRegistryDataMapsReplyPayload::read(&mut bytes.as_slice()).is_err());
    }
}
