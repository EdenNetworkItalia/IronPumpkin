//! The seam through which the engine reaches the data attachments of native mods
//! (`ironpumpkin_mods::attachments`).
//!
//! The engine calls these functions where `NeoForge` patches vanilla: entity, block entity and
//! level saving, the player clone on respawn, and the initial sync of an entity, a chunk, a
//! player and a level. Without installed hooks, which `ironpumpkin-mods` installs only when a
//! mod registers an attachment type, every function does nothing.

use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use dashmap::DashMap;
use pumpkin_data::dynamic::ContentTables;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::{
    identifier::Identifier,
    math::{position::BlockPos, vector2::Vector2},
};
pub use pumpkin_world::attachments::{
    ATTACHMENTS_NBT_KEY, AttachmentMap, AttachmentValue, Attachments,
};
use pumpkin_world::chunk::ChunkData;
use rustc_hash::FxHashMap;

use crate::{block::entities::BlockEntity, entity::Entity, entity::player::Player, world::World};

/// The attachment host of `ironpumpkin-mods`.
pub trait AttachmentHooks: Send + Sync {
    /// Writes the attachments of `entity` into its NBT.
    ///
    /// `NeoForge`: `Entity#saveWithoutId`, the `neoforge:attachments` child.
    fn write_entity(&self, entity: &Entity, nbt: &mut NbtCompound);
    /// Reads the attachments of `entity` from its NBT.
    ///
    /// `NeoForge`: `Entity#load`.
    fn read_entity(&self, entity: &Entity, nbt: &NbtCompound);
    /// Writes the attachments of `block_entity` into its NBT.
    ///
    /// `NeoForge`: `BlockEntity#saveAdditional`.
    fn write_block_entity(
        &self,
        world: &World,
        block_entity: &Arc<dyn BlockEntity>,
        nbt: &mut NbtCompound,
    );
    /// Reads the attachments of `block_entity` from its NBT.
    ///
    /// `NeoForge`: `BlockEntity#loadAdditional`.
    fn read_block_entity(
        &self,
        world: &World,
        block_entity: &Arc<dyn BlockEntity>,
        nbt: &NbtCompound,
    );
    /// Reads the saved attachments of `world`.
    ///
    /// `NeoForge`: `LevelAttachmentsSavedData.init`.
    fn load_level(&self, world: &World);
    /// Saves the attachments of `world`.
    ///
    /// `NeoForge`: the save of `LevelAttachmentsSavedData`.
    fn save_level(&self, world: &World);
    /// Applies the copy rules of `PlayerEvent.Clone` to `player`, which is both the original
    /// and the new player in Pumpkin.
    ///
    /// `NeoForge`: `AttachmentInternals.onPlayerClone`.
    fn clone_player(&self, player: &Player, was_death: bool);
    /// Sends the synced attachments of `player` to its own client.
    ///
    /// `NeoForge`: `AttachmentSync.syncInitialPlayerAttachments`.
    fn sync_player(&self, player: &Arc<Player>);
    /// Sends the synced attachments of `world` to `player`.
    ///
    /// `NeoForge`: `AttachmentSync.syncInitialLevelAttachments`.
    fn sync_level(&self, world: &World, player: &Arc<Player>);
    /// Sends the synced attachments of `entity` to `player`, which starts to track it.
    ///
    /// `NeoForge`: `AttachmentSync.syncInitialEntityAttachments`.
    fn sync_entity(&self, entity: &Entity, player: &Arc<Player>);
    /// Sends the synced attachments of `chunk` and of its block entities to `player`, which got
    /// the chunk.
    ///
    /// `NeoForge`: `AttachmentSync.onChunkSent`.
    fn sync_chunk(&self, world: &World, chunk: &ChunkData, player: &Arc<Player>);
    /// Sends the block entity attachment changes that wait for the end of the tick.
    ///
    /// `NeoForge`: `AttachmentSync.syncBlockEntityUpdates` from `ChunkHolder#broadcastChanges`.
    fn flush_block_entities(&self, world: &World);
}

/// Set once by `ironpumpkin-mods` at the freeze of the attachment types, when a mod registered
/// one.
pub static ATTACHMENT_HOOKS: OnceLock<&'static dyn AttachmentHooks> = OnceLock::new();

/// The names of the attachment types with a sync handler, in the order of their ids in
/// `neoforge:synced_attachment_types`. Set once by `ironpumpkin-mods` at the freeze.
pub static SYNCED_ATTACHMENT_TYPES: OnceLock<Vec<Identifier>> = OnceLock::new();

/// The registry snapshot of `neoforge:synced_attachment_types`, empty without mod types. The
/// content tables do not hold attachment types: the argument fits the snapshot table of the
/// registry sync.
#[must_use]
pub fn synced_types_snapshot(_tables: &ContentTables) -> Vec<(i32, Identifier)> {
    SYNCED_ATTACHMENT_TYPES
        .get()
        .map_or_else(Vec::new, |types| (0..).zip(types.iter().cloned()).collect())
}

/// The attachments of a world that do not live on an engine type: the level itself and its
/// block entities.
#[derive(Default)]
pub struct WorldAttachments {
    /// The attachments of the level.
    pub level: Attachments,
    /// The attachments of the block entities, by chunk, so an unloaded chunk drops its block
    /// entities in one step.
    block_entities: DashMap<Vector2<i32>, FxHashMap<BlockPos, BlockEntitySlot>>,
    /// The block entity attachment types to sync at the end of the tick.
    pending_block_entity_sync: Mutex<Vec<(BlockPos, u32)>>,
}

struct BlockEntitySlot {
    block_entity: Weak<dyn BlockEntity>,
    attachments: Arc<Attachments>,
}

impl WorldAttachments {
    /// The attachment slot of `block_entity`, `None` when it has none. A slot of another block
    /// entity at the same position does not count.
    #[must_use]
    pub fn block_entity(&self, block_entity: &Arc<dyn BlockEntity>) -> Option<Arc<Attachments>> {
        let pos = block_entity.get_position();
        let chunk = self.block_entities.get(&pos.chunk_position())?;
        let slot = chunk.get(&pos)?;
        Weak::ptr_eq(&slot.block_entity, &Arc::downgrade(block_entity))
            .then(|| slot.attachments.clone())
    }

    /// The attachment slot of `block_entity`, created when it has none. The slot of another
    /// block entity at the same position is dropped.
    pub fn block_entity_or_insert(&self, block_entity: &Arc<dyn BlockEntity>) -> Arc<Attachments> {
        let pos = block_entity.get_position();
        let weak = Arc::downgrade(block_entity);
        let mut chunk = self.block_entities.entry(pos.chunk_position()).or_default();
        let slot = chunk.entry(pos).or_insert_with(|| BlockEntitySlot {
            block_entity: weak.clone(),
            attachments: Arc::default(),
        });
        if !Weak::ptr_eq(&slot.block_entity, &weak) {
            *slot = BlockEntitySlot {
                block_entity: weak,
                attachments: Arc::default(),
            };
        }
        slot.attachments.clone()
    }

    /// The positions and attachment slots of the live block entities of `chunk_pos` that have
    /// a slot.
    #[must_use]
    pub fn block_entities_in(&self, chunk_pos: &Vector2<i32>) -> Vec<(BlockPos, Arc<Attachments>)> {
        self.block_entities
            .get(chunk_pos)
            .map_or_else(Vec::new, |chunk| {
                chunk
                    .iter()
                    .filter(|(_, slot)| slot.block_entity.strong_count() > 0)
                    .map(|(pos, slot)| (*pos, slot.attachments.clone()))
                    .collect()
            })
    }

    /// Drops the attachments of the block entity at `pos`.
    pub fn remove_block_entity(&self, pos: &BlockPos) {
        let chunk_pos = pos.chunk_position();
        let empty = self
            .block_entities
            .get_mut(&chunk_pos)
            .is_some_and(|mut chunk| {
                chunk.remove(pos);
                chunk.is_empty()
            });
        if empty {
            self.block_entities
                .remove_if(&chunk_pos, |_, chunk| chunk.is_empty());
        }
    }

    /// Drops the attachments of the block entities of an unloaded chunk.
    pub fn unload_chunk(&self, chunk_pos: &Vector2<i32>) {
        self.block_entities.remove(chunk_pos);
    }

    /// Queues the attachment type `index` of the block entity at `pos` for the sync at the end
    /// of the tick.
    pub fn queue_block_entity_sync(&self, pos: BlockPos, index: u32) {
        let mut pending = self
            .pending_block_entity_sync
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !pending.contains(&(pos, index)) {
            pending.push((pos, index));
        }
    }

    /// Takes the queued block entity syncs.
    pub fn take_block_entity_sync(&self) -> Vec<(BlockPos, u32)> {
        std::mem::take(
            &mut *self
                .pending_block_entity_sync
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        )
    }
}

fn hooks() -> Option<&'static dyn AttachmentHooks> {
    ATTACHMENT_HOOKS.get().copied()
}

/// See [`AttachmentHooks::write_entity`].
pub fn write_entity(entity: &Entity, nbt: &mut NbtCompound) {
    if let Some(hooks) = hooks() {
        hooks.write_entity(entity, nbt);
    }
}

/// See [`AttachmentHooks::read_entity`].
pub fn read_entity(entity: &Entity, nbt: &NbtCompound) {
    if let Some(hooks) = hooks() {
        hooks.read_entity(entity, nbt);
    }
}

/// See [`AttachmentHooks::write_block_entity`].
pub fn write_block_entity(
    world: &World,
    block_entity: &Arc<dyn BlockEntity>,
    nbt: &mut NbtCompound,
) {
    if let Some(hooks) = hooks() {
        hooks.write_block_entity(world, block_entity, nbt);
    }
}

/// See [`AttachmentHooks::read_block_entity`].
pub fn read_block_entity(world: &World, block_entity: &Arc<dyn BlockEntity>, nbt: &NbtCompound) {
    if let Some(hooks) = hooks() {
        hooks.read_block_entity(world, block_entity, nbt);
    }
}

/// See [`AttachmentHooks::load_level`].
pub fn load_level(world: &World) {
    if let Some(hooks) = hooks() {
        hooks.load_level(world);
    }
}

/// See [`AttachmentHooks::save_level`].
pub fn save_level(world: &World) {
    if let Some(hooks) = hooks() {
        hooks.save_level(world);
    }
}

/// See [`AttachmentHooks::clone_player`].
pub fn clone_player(player: &Player, was_death: bool) {
    if let Some(hooks) = hooks() {
        hooks.clone_player(player, was_death);
    }
}

/// See [`AttachmentHooks::sync_player`].
pub fn sync_player(player: &Arc<Player>) {
    if let Some(hooks) = hooks() {
        hooks.sync_player(player);
    }
}

/// See [`AttachmentHooks::sync_level`].
pub fn sync_level(world: &World, player: &Arc<Player>) {
    if let Some(hooks) = hooks() {
        hooks.sync_level(world, player);
    }
}

/// See [`AttachmentHooks::sync_entity`].
pub fn sync_entity(entity: &Entity, player: &Arc<Player>) {
    if let Some(hooks) = hooks() {
        hooks.sync_entity(entity, player);
    }
}

/// See [`AttachmentHooks::sync_chunk`], for each chunk of `chunks` that `player` just got.
pub fn sync_chunks(world: &World, player: &Player, chunks: &[Vector2<i32>]) {
    let Some(hooks) = hooks() else {
        return;
    };
    if chunks.is_empty() {
        return;
    }
    let Some(player) = world.get_player_by_uuid(player.gameprofile.id) else {
        return;
    };
    for pos in chunks {
        if let Some(chunk) = world.level.read_chunk_sync(pos, Arc::clone) {
            hooks.sync_chunk(world, &chunk, &player);
        }
    }
}

/// See [`AttachmentHooks::flush_block_entities`].
pub fn flush_block_entities(world: &World) {
    if let Some(hooks) = hooks() {
        hooks.flush_block_entities(world);
    }
}
