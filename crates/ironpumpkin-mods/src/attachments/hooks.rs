//! The engine side of attachments: saving, the player clone, the initial syncs and the deferred
//! block entity syncs.

use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::PathBuf,
    sync::Arc,
};

use pumpkin_core::{
    block::entities::BlockEntity,
    entity::{Entity, EntityBase, player::Player},
    plugin::attachments::{
        ATTACHMENT_HOOKS, ATTACHMENTS_NBT_KEY, AttachmentHooks, AttachmentValue, Attachments,
        new_cell,
    },
    world::World,
};
use pumpkin_nbt::{
    compound::NbtCompound,
    nbt_compress::{read_gzip_compound_tag, write_gzip_compound_tag},
};
use pumpkin_util::math::vector2::Vector2;
use pumpkin_world::{
    CURRENT_MC_DATA_VERSION,
    attachments::{CHUNK_ATTACHMENT_CODEC, ChunkAttachmentCodec},
    chunk::ChunkData,
};
use tracing::error;

use super::{
    AttachmentHolder, sync,
    types::{TYPES, Types},
};

struct Hooks;

static HOOKS: Hooks = Hooks;

/// Installs the engine hooks. Called once, at the freeze, when a mod registered a type.
pub(super) fn install() {
    let _ = ATTACHMENT_HOOKS.set(&HOOKS);
    let _ = CHUNK_ATTACHMENT_CODEC.set(&HOOKS);
}

fn types() -> Option<&'static Types> {
    TYPES.get()
}

/// Writes the attachments of `slot`, `None` when nothing is saved.
fn write_slot(slot: &Attachments) -> Option<NbtCompound> {
    types()?.write_map(&slot.cells())
}

/// Reads `nbt` into `slot`. The reads run with the lock released, so a serializer can read the
/// holder.
fn read_into(slot: &Attachments, holder: &AttachmentHolder<'_>, nbt: &NbtCompound) {
    let Some(types) = types() else {
        return;
    };
    store(slot, types.read_map(holder, nbt));
}

fn store(slot: &Attachments, values: Vec<(u32, AttachmentValue)>) {
    if values.is_empty() {
        return;
    }
    let mut guard = slot.lock();
    let map = guard.get_or_insert_with(Box::default);
    for (index, value) in values {
        map.insert(index, new_cell(value));
    }
}

/// `LevelAttachmentsSavedData`: `data/neoforge/data_attachments.dat` in the dimension folder.
fn level_file(world: &World) -> PathBuf {
    world
        .level
        .level_folder
        .dim_folder
        .join("data")
        .join("neoforge")
        .join("data_attachments.dat")
}

fn initial_sync(holder: &AttachmentHolder<'_>, slot: &Attachments, to: &Player) {
    let Some(types) = types() else {
        return;
    };
    sync::send_initial(holder, slot, types, to);
}

impl AttachmentHooks for Hooks {
    fn write_entity(&self, entity: &Entity, nbt: &mut NbtCompound) {
        if let Some(attachments) = write_slot(&entity.attachments) {
            nbt.put_compound(ATTACHMENTS_NBT_KEY, attachments);
        }
    }

    fn read_entity(&self, entity: &Entity, nbt: &NbtCompound) {
        if let Some(attachments) = nbt.get_compound(ATTACHMENTS_NBT_KEY) {
            read_into(
                &entity.attachments,
                &AttachmentHolder::Entity(entity),
                attachments,
            );
        }
    }

    fn write_block_entity(
        &self,
        world: &World,
        block_entity: &Arc<dyn BlockEntity>,
        nbt: &mut NbtCompound,
    ) {
        if let Some(slot) = world.attachments.block_entity(block_entity)
            && let Some(attachments) = write_slot(&slot)
        {
            nbt.put_compound(ATTACHMENTS_NBT_KEY, attachments);
        }
    }

    fn read_block_entity(
        &self,
        world: &World,
        block_entity: &Arc<dyn BlockEntity>,
        nbt: &NbtCompound,
    ) {
        let (Some(types), Some(attachments)) = (types(), nbt.get_compound(ATTACHMENTS_NBT_KEY))
        else {
            return;
        };
        let values = types.read_map(
            &AttachmentHolder::BlockEntity(block_entity.as_ref()),
            attachments,
        );
        if !values.is_empty() {
            store(
                &world.attachments.block_entity_or_insert(block_entity),
                values,
            );
        }
    }

    fn load_level(&self, world: &World) {
        let path = level_file(world);
        let Ok(file) = File::open(&path) else {
            return;
        };
        match read_gzip_compound_tag(BufReader::new(file)) {
            Ok(root) => {
                if let Some(data) = root.get_compound("data") {
                    read_into(
                        &world.attachments.level,
                        &AttachmentHolder::Level(world),
                        data,
                    );
                }
            }
            Err(err) => error!("Failed to read {}: {err}", path.display()),
        }
    }

    fn save_level(&self, world: &World) {
        let path = level_file(world);
        let data = write_slot(&world.attachments.level);
        // `LevelAttachmentsSavedData` is always dirty; a level that never had a saved
        // attachment writes no file.
        if data.is_none() && !path.exists() {
            return;
        }
        let mut root = NbtCompound::new();
        root.put_compound("data", data.unwrap_or_default());
        root.put_int("DataVersion", CURRENT_MC_DATA_VERSION);
        let write = || -> std::io::Result<()> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let temp = path.with_extension("dat_tmp");
            write_gzip_compound_tag(root, BufWriter::new(File::create(&temp)?))
                .map_err(std::io::Error::other)?;
            std::fs::rename(&temp, &path)
        };
        if let Err(err) = write() {
            error!("Failed to save {}: {err}", path.display());
        }
    }

    fn clone_player(&self, player: &Player, was_death: bool) {
        let Some(types) = types() else {
            return;
        };
        let entity = player.get_entity();
        // Pumpkin keeps the player, so the old attachments leave the holder first and the copy
        // handlers see the new player as `NeoForge` shows them the fresh `ServerPlayer`.
        let Some(old) = entity.attachments.take() else {
            return;
        };
        let copies = types.copy_map(&old.cells(), &AttachmentHolder::Entity(entity), |entry| {
            !was_death || entry.copy_on_death()
        });
        store(&entity.attachments, copies);
    }

    fn copy_on_conversion(&self, from: &Entity, to: &Entity) {
        let Some(types) = types() else {
            return;
        };
        let copies = types.copy_map(
            &from.attachments.cells(),
            &AttachmentHolder::Entity(to),
            |entry| entry.copy_on_death(),
        );
        store(&to.attachments, copies);
    }

    fn sync_player(&self, player: &Arc<Player>) {
        let entity = player.get_entity();
        initial_sync(
            &AttachmentHolder::Entity(entity),
            &entity.attachments,
            player,
        );
    }

    fn sync_level(&self, world: &World, player: &Arc<Player>) {
        initial_sync(
            &AttachmentHolder::Level(world),
            &world.attachments.level,
            player,
        );
    }

    fn sync_entity(&self, entity: &Entity, player: &Arc<Player>) {
        initial_sync(
            &AttachmentHolder::Entity(entity),
            &entity.attachments,
            player,
        );
    }

    fn sync_chunks(&self, world: &World, chunks: &[Vector2<i32>], player: &Arc<Player>) {
        if !sync::has_channel(player) {
            return;
        }
        for chunk_pos in chunks {
            if let Some(chunk) = world.level.read_chunk_sync(chunk_pos, Arc::clone) {
                sync_chunk(world, &chunk, player);
            }
        }
    }

    fn flush_block_entities(&self, world: &World) {
        let pending = world.attachments.take_block_entity_sync();
        if pending.is_empty() {
            return;
        }
        let Some(types) = types() else {
            return;
        };
        for (pos, index) in pending {
            let chunk_pos = pos.chunk_position();
            let Some(block_entity) = world
                .block_entities
                .get(&chunk_pos)
                .and_then(|entities| entities.get(&pos).cloned())
            else {
                continue;
            };
            let (Some(entry), Some(slot)) = (
                types.get(index),
                world.attachments.block_entity(&block_entity),
            ) else {
                continue;
            };
            let players = sync::chunk_recipients(world, chunk_pos);
            sync::sync_update(
                &AttachmentHolder::BlockEntity(block_entity.as_ref()),
                &slot,
                entry,
                &players,
            );
        }
    }
}

impl ChunkAttachmentCodec for Hooks {
    fn write(&self, chunk: &ChunkData) -> Option<NbtCompound> {
        write_slot(&chunk.attachments)
    }

    fn read(&self, chunk: &ChunkData, nbt: &NbtCompound) {
        read_into(&chunk.attachments, &AttachmentHolder::Chunk(chunk), nbt);
    }
}

/// `AttachmentSync.onChunkSent`: the chunk, then each block entity of the chunk.
fn sync_chunk(world: &World, chunk: &ChunkData, player: &Arc<Player>) {
    initial_sync(&AttachmentHolder::Chunk(chunk), &chunk.attachments, player);
    let chunk_pos = Vector2::new(chunk.x, chunk.z);
    wake_block_entities_with_attachments(world, chunk);
    for (pos, slot) in world.attachments.block_entities_in(&chunk_pos) {
        let block_entity = world
            .block_entities
            .get(&chunk_pos)
            .and_then(|entities| entities.get(&pos).cloned());
        if let Some(block_entity) = block_entity
            && world
                .attachments
                .block_entity(&block_entity)
                .is_some_and(|live| Arc::ptr_eq(&live, &slot))
        {
            initial_sync(
                &AttachmentHolder::BlockEntity(block_entity.as_ref()),
                &slot,
                player,
            );
        }
    }
}

/// Wakes the block entities that sleep in the chunk data with saved attachments, so their
/// synced values reach the client as every block entity of a `LevelChunk` does on `NeoForge`.
/// Pumpkin wakes a block entity on first use; the other sleeping entries cost one key lookup.
fn wake_block_entities_with_attachments(world: &World, chunk: &ChunkData) {
    let sleeping: Vec<_> = chunk
        .pending_block_entities
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|(_, nbt)| nbt.get_compound(ATTACHMENTS_NBT_KEY).is_some())
        .map(|(pos, _)| *pos)
        .collect();
    let chunk_pos = Vector2::new(chunk.x, chunk.z);
    for pos in sleeping {
        let live = world
            .block_entities
            .get(&chunk_pos)
            .is_some_and(|entities| entities.contains_key(&pos));
        if !live {
            let _ = world.get_block_entity(&pos);
        }
    }
}
