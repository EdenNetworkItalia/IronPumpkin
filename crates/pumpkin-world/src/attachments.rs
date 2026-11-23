//! The storage of `NeoForge` data attachments on a holder, and the seam through which chunk saving
//! reaches the attachment types of native mods (`ironpumpkin_mods::attachments`).
//!
//! A holder keeps an [`Attachments`] slot. The slot holds no map until the first attachment is
//! stored, so a holder without attachments allocates nothing. The values are type-erased here:
//! the attachment types, which know the value types, live in `ironpumpkin-mods`.

use std::{
    any::Any,
    sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError},
};

use pumpkin_nbt::compound::NbtCompound;

use crate::chunk::ChunkData;

/// The NBT key of the attachments of a holder.
///
/// `NeoForge`: `AttachmentHolder.ATTACHMENTS_NBT_KEY`.
pub const ATTACHMENTS_NBT_KEY: &str = "neoforge:attachments";

/// The value of one attachment.
pub type AttachmentValue = Box<dyn Any + Send + Sync>;

/// One attachment with its own lock, so a mod can change one type of a holder while it reads
/// another.
pub type AttachmentCell = Arc<Mutex<AttachmentValue>>;

/// A new cell for `value`.
#[must_use]
pub fn new_cell(value: AttachmentValue) -> AttachmentCell {
    Arc::new(Mutex::new(value))
}

/// The attachments of one holder, by the index of their attachment type.
#[derive(Default)]
pub struct AttachmentMap {
    entries: Vec<(u32, AttachmentCell)>,
}

impl AttachmentMap {
    /// The cell of the type `index`.
    #[must_use]
    pub fn get(&self, index: u32) -> Option<&AttachmentCell> {
        self.entries
            .iter()
            .find_map(|(i, cell)| (*i == index).then_some(cell))
    }

    /// Stores `value` for the type `index` in a new cell and returns the cell.
    ///
    /// # Errors
    ///
    /// The cell that the type already has, and `value`.
    pub fn insert_new(
        &mut self,
        index: u32,
        value: AttachmentValue,
    ) -> Result<AttachmentCell, (AttachmentCell, AttachmentValue)> {
        if let Some(cell) = self.get(index) {
            return Err((cell.clone(), value));
        }
        let cell = new_cell(value);
        self.entries.push((index, cell.clone()));
        Ok(cell)
    }

    /// Stores `cell` for the type `index` and returns the previous cell.
    pub fn insert(&mut self, index: u32, cell: AttachmentCell) -> Option<AttachmentCell> {
        if let Some(slot) = self
            .entries
            .iter_mut()
            .find_map(|(i, slot)| (*i == index).then_some(slot))
        {
            return Some(std::mem::replace(slot, cell));
        }
        self.entries.push((index, cell));
        None
    }

    /// Removes the cell of the type `index` and returns it.
    pub fn remove(&mut self, index: u32) -> Option<AttachmentCell> {
        let position = self.entries.iter().position(|(i, _)| *i == index)?;
        Some(self.entries.swap_remove(position).1)
    }

    /// Whether the map has a value of the type `index`.
    #[must_use]
    pub fn contains(&self, index: u32) -> bool {
        self.get(index).is_some()
    }

    /// Whether the map has no value.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The type indices and cells.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &AttachmentCell)> {
        self.entries.iter().map(|(index, cell)| (*index, cell))
    }

    /// A copy of the type indices and cells, to read the values without the lock of the map.
    #[must_use]
    pub fn cells(&self) -> Vec<(u32, AttachmentCell)> {
        self.entries.clone()
    }
}

/// The attachment slot of a holder: no map until the first attachment.
#[derive(Default)]
pub struct Attachments {
    map: Mutex<Option<Box<AttachmentMap>>>,
    /// Held while a sync payload of the holder is encoded and sent, so the payloads leave in
    /// the order of their snapshots.
    sync: Mutex<()>,
}

impl Attachments {
    /// The map of the holder, `None` before the first attachment. A caller that stores a value
    /// creates the map with `get_or_insert_with(Box::default)`. A caller must not lock a cell
    /// while it holds this lock.
    pub fn lock(&self) -> MutexGuard<'_, Option<Box<AttachmentMap>>> {
        self.map.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether the holder has at least one attachment.
    ///
    /// `NeoForge`: `IAttachmentHolder#hasAttachments`.
    #[must_use]
    pub fn has_attachments(&self) -> bool {
        self.lock().as_ref().is_some_and(|map| !map.is_empty())
    }

    /// The cell of the type `index`.
    #[must_use]
    pub fn cell(&self, index: u32) -> Option<AttachmentCell> {
        self.lock().as_ref()?.get(index).cloned()
    }

    /// A copy of the type indices and cells, empty without attachments.
    #[must_use]
    pub fn cells(&self) -> Vec<(u32, AttachmentCell)> {
        self.lock()
            .as_ref()
            .map_or_else(Vec::new, |map| map.cells())
    }

    /// A slot that shares the cells of this one: a proto chunk that becomes a full chunk, or
    /// the reverse, keeps its attachments.
    #[must_use]
    pub fn share(&self) -> Self {
        let cells = self.cells();
        if cells.is_empty() {
            return Self::default();
        }
        let mut map = AttachmentMap::default();
        for (index, cell) in cells {
            map.insert(index, cell);
        }
        Self {
            map: Mutex::new(Some(Box::new(map))),
            sync: Mutex::new(()),
        }
    }

    /// The lock that orders the sync payloads of the holder. A caller encodes and sends a
    /// payload while it holds the guard, and holds no lock of a value while it sends.
    pub fn sync_lock(&self) -> MutexGuard<'_, ()> {
        self.sync.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Takes the map out of the slot.
    pub fn take(&self) -> Option<Box<AttachmentMap>> {
        self.lock().take()
    }

    /// Replaces the map of the slot.
    pub fn replace(&self, map: Option<Box<AttachmentMap>>) {
        *self.lock() = map;
    }
}

/// The attachment codec of chunks, implemented by `ironpumpkin-mods`.
pub trait ChunkAttachmentCodec: Send + Sync {
    /// The serialized attachments of `chunk`, `None` when no attachment is saved.
    ///
    /// `NeoForge`: `ChunkAccess#writeAttachmentsToNBT`.
    fn write(&self, chunk: &ChunkData) -> Option<NbtCompound>;

    /// Reads the attachments that `nbt` holds into `chunk`.
    ///
    /// `NeoForge`: `ChunkAccess#readAttachmentsFromNBT`.
    fn read(&self, chunk: &ChunkData, nbt: &NbtCompound);
}

/// The codec of chunk attachments, set once by `ironpumpkin-mods`.
///
/// It is set at the freeze of the attachment types, when a mod registered one. Without it,
/// chunks save no attachment and drop the saved ones on load, as `NeoForge` drops the
/// attachments of unknown types.
pub static CHUNK_ATTACHMENT_CODEC: OnceLock<&'static dyn ChunkAttachmentCodec> = OnceLock::new();

/// Logs once that the Pump chunk format drops the attachments of a chunk.
pub(crate) fn warn_pump_drop(pos: (i32, i32)) {
    static WARNED: std::sync::Once = std::sync::Once::new();
    WARNED.call_once(|| {
        tracing::warn!(
            "The Pump chunk format does not save NeoForge attachments: the attachments of chunk \
             {pos:?} and of later chunks are dropped. Use the Anvil or the Linear format."
        );
    });
}

/// Whether the NBT bytes of a chunk hold the attachment key, for the converter, which runs
/// without the attachment codec.
pub(crate) fn bytes_hold_attachments(nbt: &[u8]) -> bool {
    nbt.windows(ATTACHMENTS_NBT_KEY.len())
        .any(|window| window == ATTACHMENTS_NBT_KEY.as_bytes())
}

/// Writes the attachments of `chunk` into its NBT root.
pub(crate) fn write_chunk(chunk: &ChunkData, root: &mut NbtCompound) {
    if let Some(codec) = CHUNK_ATTACHMENT_CODEC.get()
        && let Some(nbt) = codec.write(chunk)
    {
        root.put_compound(ATTACHMENTS_NBT_KEY, nbt);
    }
}

/// Reads the attachments of `chunk` from its NBT root.
pub(crate) fn read_chunk(chunk: &ChunkData, root: &NbtCompound) {
    if let Some(codec) = CHUNK_ATTACHMENT_CODEC.get()
        && let Some(nbt) = root.get_compound(ATTACHMENTS_NBT_KEY)
    {
        codec.read(chunk, nbt);
    }
}

#[cfg(test)]
mod tests {
    use super::{AttachmentMap, Attachments};

    #[test]
    fn empty_slot_holds_no_map() {
        let slot = Attachments::default();
        assert!(slot.lock().is_none());
        assert!(!slot.has_attachments());
        assert!(slot.cells().is_empty());
    }

    #[test]
    fn map_keeps_one_cell_per_index() {
        let mut map = AttachmentMap::default();
        assert!(map.insert_new(3, Box::new(1i32)).is_ok());
        let (cell, value) = map.insert_new(3, Box::new(2i32)).unwrap_err();
        assert_eq!(value.downcast_ref::<i32>(), Some(&2));
        assert_eq!(cell.lock().unwrap().downcast_ref::<i32>(), Some(&1));
        assert!(map.contains(3));
        assert!(!map.contains(4));
        let removed = map.remove(3).unwrap();
        assert_eq!(removed.lock().unwrap().downcast_ref::<i32>(), Some(&1));
        assert!(map.is_empty());
        assert!(map.remove(3).is_none());
    }
}
