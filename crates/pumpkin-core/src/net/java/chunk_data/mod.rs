pub mod light;
pub mod util;
pub mod v1_18;

pub use light::ChunkLightExt;

use std::io::Write;
use std::sync::Arc;

use pumpkin_data::packet::clientbound::play::LEVEL_CHUNK_WITH_LIGHT;
use pumpkin_data::{Block, BlockStateId};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::ClientPacket;
use pumpkin_protocol::packet::MultiVersionJavaPacket;
use pumpkin_protocol::ser::WritingError;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector2::Vector2;
use pumpkin_util::version::JavaMinecraftVersion;
use pumpkin_world::attachments::ATTACHMENTS_NBT_KEY;
use pumpkin_world::chunk::ChunkData;
use rustc_hash::FxHashSet;

use crate::block::entities::{
    BlockEntity, UpdateTagSource, block_entity_from_nbt, block_entity_type_id, update_tag_source,
};
use crate::world::World;

/// Sent by the server to provide the client with the full data for a chunk.
///
/// This includes heightmaps, the actual block and biome data (organized into sections),
/// block entities (like signs or chests), and the light level information for both
/// sky and block light.
pub struct CChunkData<'a> {
    pub chunk: &'a ChunkData,
    pub block_entities: Vec<ChunkBlockEntity>,
}

/// One block entity of the chunk packet: vanilla `ClientboundLevelChunkPacketData.BlockEntityInfo`.
pub struct ChunkBlockEntity {
    pub position: BlockPos,
    pub type_id: u16,
    /// The update tag of the block entity (`BlockEntity.getUpdateTag`).
    pub tag: NbtCompound,
}

impl MultiVersionJavaPacket for CChunkData<'_> {
    fn to_id(version: JavaMinecraftVersion) -> i32 {
        LEVEL_CHUNK_WITH_LIGHT.to_id(version)
    }
}

impl<'a> CChunkData<'a> {
    /// The packet of `chunk` with the block entities of `world` in it.
    #[must_use]
    pub fn new(world: &World, chunk: &'a ChunkData) -> Self {
        let live: Vec<_> = world
            .block_entities
            .get(&Vector2::new(chunk.x, chunk.z))
            .map(|entities| entities.values().cloned().collect())
            .unwrap_or_default();
        Self::with_live_block_entities(chunk, &live, |entity, state| {
            world.block_entity_update_tag(entity, state)
        })
    }

    /// The packet of `chunk`, with `live` in place of the saved data at their positions.
    /// `update_tag` gives the update tag of a live block entity in a block state.
    #[must_use]
    pub fn with_live_block_entities(
        chunk: &'a ChunkData,
        live: &[Arc<dyn BlockEntity>],
        update_tag: impl Fn(&Arc<dyn BlockEntity>, BlockStateId) -> Option<NbtCompound>,
    ) -> Self {
        let live_positions: FxHashSet<BlockPos> =
            live.iter().map(|entity| entity.get_position()).collect();
        let mut block_entities: Vec<ChunkBlockEntity> = live
            .iter()
            .filter_map(|entity| {
                Some(ChunkBlockEntity {
                    position: entity.get_position(),
                    type_id: entity.get_id()?,
                    tag: update_tag(entity, block_state_at(chunk, entity.get_position()))
                        .unwrap_or_default(),
                })
            })
            .collect();
        let pending: Vec<(BlockPos, u16, Option<NbtCompound>)> = chunk
            .pending_block_entities
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|(position, _)| !live_positions.contains(position))
            .filter_map(|(position, nbt)| {
                let id = nbt.get_string("id")?;
                let saved = (update_tag_source(id) != UpdateTagSource::Empty).then(|| nbt.clone());
                Some((*position, block_entity_type_id(id)?, saved))
            })
            .collect();
        block_entities.extend(pending.into_iter().map(|(position, type_id, saved)| {
            let tag = saved
                .and_then(|nbt| {
                    let entity = block_entity_from_nbt(&nbt)?;
                    let mut tag = entity.update_tag(block_state_at(chunk, position))?;
                    if update_tag_source(entity.resource_location()) == UpdateTagSource::SavedData
                        && let Some(attachments) = nbt.get_compound(ATTACHMENTS_NBT_KEY)
                    {
                        tag.put_compound(ATTACHMENTS_NBT_KEY, attachments.clone());
                    }
                    Some(tag)
                })
                .unwrap_or_default();
            ChunkBlockEntity {
                position,
                type_id,
                tag,
            }
        }));
        Self {
            chunk,
            block_entities,
        }
    }
}

fn block_state_at(chunk: &ChunkData, position: BlockPos) -> BlockStateId {
    let relative = position.chunk_relative_position();
    chunk
        .section
        .get_block_absolute_y(relative.x as usize, relative.y, relative.z as usize)
        .unwrap_or(Block::AIR.default_state.id)
}

impl ClientPacket for CChunkData<'_> {
    fn write_packet_data(
        &self,
        write: impl Write,
        version: &pumpkin_protocol::EncodingKey,
    ) -> Result<(), WritingError> {
        v1_18::write_chunk_data(self.chunk, &self.block_entities, write, version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_nbt::tag::NbtTag;
    use pumpkin_world::chunk::ChunkData;

    fn saved(id: &str, pos: BlockPos) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_string("id", id.to_string());
        nbt.put_int("x", pos.0.x);
        nbt.put_int("y", pos.0.y);
        nbt.put_int("z", pos.0.z);
        nbt
    }

    fn saved_chest(pos: BlockPos) -> NbtCompound {
        let mut item = NbtCompound::new();
        item.put_string("id", "minecraft:diamond".to_string());
        item.put_int("count", 64);
        item.put_byte("Slot", 0);
        let mut nbt = saved("minecraft:chest", pos);
        nbt.put_list("Items", vec![NbtTag::Compound(item)]);
        nbt
    }

    fn saved_sign(pos: BlockPos, line: &str) -> NbtCompound {
        let mut front = NbtCompound::new();
        front.put_list(
            "messages",
            vec![
                NbtTag::String(line.into()),
                NbtTag::String(String::new().into()),
                NbtTag::String(String::new().into()),
                NbtTag::String(String::new().into()),
            ],
        );
        let mut nbt = saved("minecraft:sign", pos);
        nbt.put_compound("front_text", front);
        nbt
    }

    fn saved_banner(pos: BlockPos) -> NbtCompound {
        let mut layer = NbtCompound::new();
        layer.put_string("pattern", "minecraft:creeper".to_string());
        layer.put_string("color", "lime".to_string());
        let mut nbt = saved("minecraft:banner", pos);
        nbt.put_list("patterns", vec![NbtTag::Compound(layer)]);
        nbt
    }

    fn own_update_tag(entity: &Arc<dyn BlockEntity>, state: BlockStateId) -> Option<NbtCompound> {
        entity.update_tag(state)
    }

    fn chunk_with(pending: Vec<NbtCompound>) -> ChunkData {
        let chunk = ChunkData::empty(0, 0);
        let mut map = chunk.pending_block_entities.lock().unwrap();
        for nbt in pending {
            let pos = BlockPos::new(
                nbt.get_int("x").unwrap(),
                nbt.get_int("y").unwrap(),
                nbt.get_int("z").unwrap(),
            );
            map.insert(pos, nbt);
        }
        drop(map);
        chunk
    }

    fn entry<'a>(packet: &'a CChunkData<'_>, pos: BlockPos) -> &'a ChunkBlockEntity {
        packet
            .block_entities
            .iter()
            .find(|entity| entity.position == pos)
            .expect("the block entity is in the packet")
    }

    fn tag_bytes(tag: &NbtCompound) -> Vec<u8> {
        pumpkin_nbt::Nbt::from(tag.clone()).write_unnamed().to_vec()
    }

    fn contains(bytes: &[u8], needle: &[u8]) -> bool {
        bytes.windows(needle.len()).any(|window| window == needle)
    }

    fn encode(packet: &CChunkData<'_>) -> Vec<u8> {
        let mut buf = Vec::new();
        packet
            .write_packet_data(&mut buf, &JavaMinecraftVersion::V_26_3.into())
            .unwrap();
        buf
    }

    #[test]
    fn saved_chest_items_stay_out_of_the_packet() {
        let pos = BlockPos::new(0, 64, 0);
        let chunk = chunk_with(vec![saved_chest(pos)]);
        let packet = CChunkData::with_live_block_entities(&chunk, &[], own_update_tag);
        let chest = entry(&packet, pos);
        assert_eq!(Some(chest.type_id), block_entity_type_id("minecraft:chest"));
        assert!(chest.tag.is_empty());
        let buf = encode(&packet);
        assert!(!contains(&buf, b"Items"));
        assert!(!contains(&buf, b"diamond"));
    }

    #[test]
    fn live_chest_items_stay_out_of_the_packet() {
        let pos = BlockPos::new(1, 64, 0);
        let chunk = chunk_with(Vec::new());
        let chest: Arc<dyn BlockEntity> = block_entity_from_nbt(&saved_chest(pos)).unwrap();
        let packet = CChunkData::with_live_block_entities(&chunk, &[chest], own_update_tag);
        assert!(entry(&packet, pos).tag.is_empty());
        assert!(!contains(&encode(&packet), b"diamond"));
    }

    #[test]
    fn an_empty_update_tag_goes_out_as_no_tag() {
        let pos = BlockPos::new(3, -2, 5);
        let chunk = chunk_with(vec![saved_chest(pos)]);
        let packet = CChunkData::with_live_block_entities(&chunk, &[], own_update_tag);
        let buf = encode(&packet);
        let chest = block_entity_type_id("minecraft:chest").unwrap();
        // packed xz, y as i16, the type id (one VarInt byte), then TAG_End
        let entry = [0x35, 0xFF, 0xFE, u8::try_from(chest).unwrap(), 0x00];
        assert!(contains(&buf, &entry));
    }

    #[test]
    fn pending_sign_sends_its_text() {
        let pos = BlockPos::new(2, 70, 3);
        let chunk = chunk_with(vec![saved_sign(pos, "pending line")]);
        let packet = CChunkData::with_live_block_entities(&chunk, &[], own_update_tag);
        let sign = entry(&packet, pos);
        assert_eq!(Some(sign.type_id), block_entity_type_id("minecraft:sign"));
        assert!(contains(&tag_bytes(&sign.tag), b"pending line"));
    }

    #[test]
    fn live_sign_replaces_its_saved_data() {
        let pos = BlockPos::new(2, 70, 3);
        let chunk = chunk_with(vec![saved_sign(pos, "saved line")]);
        let live = block_entity_from_nbt(&saved_sign(pos, "live line")).unwrap();
        let packet = CChunkData::with_live_block_entities(&chunk, &[live], own_update_tag);
        assert_eq!(packet.block_entities.len(), 1);
        let tag = tag_bytes(&entry(&packet, pos).tag);
        assert!(contains(&tag, b"live line"));
        assert!(!contains(&tag, b"saved line"));
    }

    #[test]
    fn pending_banner_sends_its_patterns() {
        let pos = BlockPos::new(4, 64, 4);
        let chunk = chunk_with(vec![saved_banner(pos)]);
        let packet = CChunkData::with_live_block_entities(&chunk, &[], own_update_tag);
        let banner = entry(&packet, pos);
        assert_eq!(
            Some(banner.type_id),
            block_entity_type_id("minecraft:banner")
        );
        let patterns = banner.tag.get_list("patterns").unwrap();
        assert_eq!(patterns.len(), 1);
    }

    #[test]
    fn moving_piston_sends_its_update_tag_without_a_data_packet() {
        let pos = BlockPos::new(5, 64, 5);
        let mut nbt = saved("minecraft:piston", pos);
        nbt.put_compound(
            "blockState",
            pumpkin_world::block::state_nbt::block_state_to_nbt(
                pumpkin_data::Block::STONE.default_state.id,
            ),
        );
        nbt.put_byte("facing", 2);
        nbt.put_float("progress", 0.5);
        nbt.put_bool("extending", true);
        nbt.put_bool("source", false);
        let chunk = chunk_with(vec![nbt]);
        let packet = CChunkData::with_live_block_entities(&chunk, &[], own_update_tag);
        let piston = entry(&packet, pos);
        assert_eq!(
            piston
                .tag
                .get_compound("blockState")
                .and_then(|state| state.get_string("id")),
            Some("minecraft:stone")
        );
        assert_eq!(piston.tag.get_byte("facing"), Some(2));
        assert_eq!(piston.tag.get_float("progress"), Some(0.5));
        assert_eq!(piston.tag.get_bool("extending"), Some(true));
        assert_eq!(piston.tag.get_bool("source"), Some(false));

        let live =
            block_entity_from_nbt(&chunk.pending_block_entities.lock().unwrap()[&pos]).unwrap();
        assert!(!live.sends_update_packet());
        let sign = block_entity_from_nbt(&saved_sign(pos, "line")).unwrap();
        assert!(sign.sends_update_packet());
    }

    #[test]
    fn trial_spawner_sends_only_its_vanilla_update_tag() {
        use pumpkin_data::block_properties::{TrialSpawnerLikeProperties, TrialSpawnerState};
        let pos = BlockPos::new(6, 64, 6);
        let mut entity = NbtCompound::new();
        entity.put_string("id", "minecraft:zombie".to_string());
        let mut spawn_data = NbtCompound::new();
        spawn_data.put_compound("entity", entity);
        let mut nbt = saved("minecraft:trial_spawner", pos);
        nbt.put_long("next_mob_spawns_at", 1234);
        nbt.put_long("cooldown_ends_at", 99);
        nbt.put_int("total_mobs_spawned", 3);
        nbt.put_compound("spawn_data", spawn_data);
        nbt.put_list(
            "registered_players",
            vec![NbtTag::IntArray(vec![1, 2, 3, 4])],
        );
        let spawner = block_entity_from_nbt(&nbt).unwrap();
        let block = &pumpkin_data::Block::TRIAL_SPAWNER;
        let mut props = TrialSpawnerLikeProperties::default(block);
        props.trial_spawner_state = TrialSpawnerState::Active;
        let active = spawner.update_tag(props.to_state_id(block)).unwrap();
        props.trial_spawner_state = TrialSpawnerState::WaitingForPlayers;
        let waiting = spawner.update_tag(props.to_state_id(block)).unwrap();

        assert_eq!(active.get_long("next_mob_spawns_at"), Some(1234));
        assert!(waiting.get_long("next_mob_spawns_at").is_none());
        for tag in [&active, &waiting] {
            assert_eq!(
                tag.get_compound("spawn_data")
                    .and_then(|data| data.get_compound("entity"))
                    .and_then(|entity| entity.get_string("id")),
                Some("minecraft:zombie")
            );
            assert!(tag.get("registered_players").is_none());
            assert!(tag.get("cooldown_ends_at").is_none());
            assert!(tag.get("total_mobs_spawned").is_none());
        }
    }

    #[test]
    fn a_saved_id_without_namespace_reads_as_minecraft() {
        let pos = BlockPos::new(2, 70, 3);
        let mut sign = saved_sign(pos, "bare id line");
        sign.put_string("id", "sign".to_string());
        let chunk = chunk_with(vec![sign]);
        let packet = CChunkData::with_live_block_entities(&chunk, &[], own_update_tag);
        let sign = entry(&packet, pos);
        assert_eq!(Some(sign.type_id), block_entity_type_id("minecraft:sign"));
        assert!(contains(&tag_bytes(&sign.tag), b"bare id line"));
    }

    #[test]
    fn the_update_tag_table_matches_each_block_entity_type() {
        let pos = BlockPos::new(0, 64, 0);
        for (id, name) in pumpkin_data::block_properties::BLOCK_ENTITY_TYPES
            .iter()
            .enumerate()
        {
            let Some(entity) =
                crate::block::entities::create_block_entity(u16::try_from(id).unwrap(), pos)
            else {
                continue;
            };
            let tag = entity.update_tag(pumpkin_data::Block::AIR.default_state.id);
            if update_tag_source(name) == UpdateTagSource::Empty {
                assert!(
                    tag.is_none_or(|tag| tag.is_empty()),
                    "{name} has an update tag"
                );
            } else {
                assert!(tag.is_some(), "{name} has no update tag");
            }
        }
    }

    #[test]
    fn saved_attachments_go_out_only_with_a_saved_data_update_tag() {
        let mut attachments = NbtCompound::new();
        attachments.put_int("examplemod:charge", 7);
        let sign_pos = BlockPos::new(1, 64, 1);
        let mut sign = saved_sign(sign_pos, "line");
        sign.put_compound(ATTACHMENTS_NBT_KEY, attachments.clone());
        let campfire_pos = BlockPos::new(2, 64, 2);
        let mut campfire = saved("minecraft:campfire", campfire_pos);
        campfire.put_compound(ATTACHMENTS_NBT_KEY, attachments.clone());
        let chunk = chunk_with(vec![sign, campfire]);
        let packet = CChunkData::with_live_block_entities(&chunk, &[], own_update_tag);
        assert_eq!(
            entry(&packet, sign_pos)
                .tag
                .get_compound(ATTACHMENTS_NBT_KEY),
            Some(&attachments)
        );
        assert!(
            entry(&packet, campfire_pos)
                .tag
                .get_compound(ATTACHMENTS_NBT_KEY)
                .is_none()
        );
    }

    #[test]
    fn unknown_block_entity_types_stay_out_of_the_packet() {
        let chunk = chunk_with(vec![
            saved("examplemod:chest", BlockPos::new(0, 64, 0)),
            saved("minecraft:no_such_type", BlockPos::new(1, 64, 0)),
        ]);
        let packet = CChunkData::with_live_block_entities(&chunk, &[], own_update_tag);
        assert!(packet.block_entities.is_empty());
    }

    #[test]
    fn the_type_id_matches_the_full_name() {
        let chest = block_entity_type_id("minecraft:chest");
        assert!(chest.is_some());
        assert_eq!(block_entity_type_id("chest"), chest);
        assert_eq!(block_entity_type_id("examplemod:chest"), None);
        assert_eq!(block_entity_type_id("minecraft:examplemod:chest"), None);
        assert_ne!(block_entity_type_id("minecraft:furnace"), None);
    }

    #[test]
    fn chunk_data_all_versions() {
        let chunk = ChunkData::empty(0, 0);
        let packet = CChunkData::with_live_block_entities(&chunk, &[], own_update_tag);

        let versions = [JavaMinecraftVersion::V_26_3];

        for version in versions {
            let mut buf = Vec::new();
            let id = CChunkData::to_id(version);
            assert_ne!(id, -1, "Packet ID for version {version:?} must be valid");
            assert!(
                packet.write_packet_data(&mut buf, &version.into()).is_ok(),
                "Failed to serialize chunk data for version {version:?}"
            );
            assert!(
                !buf.is_empty(),
                "Serialized buffer must not be empty for version {version:?}"
            );
        }
    }

    #[test]
    fn populated_chunk_data_all_versions() {
        let chunk = ChunkData::empty(0, 0);
        chunk
            .section
            .set_block_absolute_y(0, 64, 0, pumpkin_data::Block::STONE.default_state.id);
        chunk
            .section
            .set_block_absolute_y(1, 64, 1, pumpkin_data::Block::DIRT.default_state.id);

        let mut nbt = pumpkin_nbt::compound::NbtCompound::new();
        nbt.put_string("id", "minecraft:chest".to_string());
        chunk.pending_block_entities.lock().unwrap().insert(
            pumpkin_util::math::position::BlockPos(pumpkin_util::math::vector3::Vector3::new(
                0, 64, 0,
            )),
            nbt,
        );

        let packet = CChunkData::with_live_block_entities(&chunk, &[], own_update_tag);

        let versions = [JavaMinecraftVersion::V_26_3];

        for version in versions {
            let mut buf = Vec::new();
            let id = CChunkData::to_id(version);
            assert_ne!(id, -1, "Packet ID for version {version:?} must be valid");
            assert!(
                packet.write_packet_data(&mut buf, &version.into()).is_ok(),
                "Failed to serialize populated chunk data for version {version:?}"
            );
            assert!(
                !buf.is_empty(),
                "Serialized buffer must not be empty for version {version:?}"
            );
        }
    }
}
