//! The storage of `NeoForge` data attachments on a holder, and the seam through which chunk saving
//! reaches the attachment types of native mods (`ironpumpkin_mods::attachments`).
//!
//! A holder keeps an [`Attachments`] slot. The slot holds no map until the first attachment is
//! stored, so a holder without attachments allocates nothing. The values are type-erased here:
//! the attachment types, which know the value types, live in `ironpumpkin-mods`.

use std::{
    any::Any,
    sync::{Mutex, MutexGuard, OnceLock, PoisonError},
};

use pumpkin_nbt::compound::NbtCompound;

use crate::chunk::ChunkData;

/// The NBT key of the attachments of a holder.
///
/// `NeoForge`: `AttachmentHolder.ATTACHMENTS_NBT_KEY`.
pub const ATTACHMENTS_NBT_KEY: &str = "neoforge:attachments";

/// The value of one attachment.
pub type AttachmentValue = Box<dyn Any + Send + Sync>;

/// The attachments of one holder, by the index of their attachment type.
#[derive(Default)]
pub struct AttachmentMap {
    entries: Vec<(u32, AttachmentValue)>,
}

impl AttachmentMap {
    /// The value of the type `index`.
    #[must_use]
    pub fn get(&self, index: u32) -> Option<&AttachmentValue> {
        self.entries
            .iter()
            .find_map(|(i, value)| (*i == index).then_some(value))
    }

    /// The value of the type `index`, for a change in place.
    pub fn get_mut(&mut self, index: u32) -> Option<&mut AttachmentValue> {
        self.entries
            .iter_mut()
            .find_map(|(i, value)| (*i == index).then_some(value))
    }

    /// Stores `value` for the type `index` and returns the previous value.
    pub fn insert(&mut self, index: u32, value: AttachmentValue) -> Option<AttachmentValue> {
        if let Some(slot) = self.get_mut(index) {
            return Some(std::mem::replace(slot, value));
        }
        self.entries.push((index, value));
        None
    }

    /// Removes the value of the type `index` and returns it.
    pub fn remove(&mut self, index: u32) -> Option<AttachmentValue> {
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

    /// The type indices and values.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &AttachmentValue)> {
        self.entries.iter().map(|(index, value)| (*index, value))
    }
}

/// The attachment slot of a holder: no map until the first attachment.
#[derive(Default)]
pub struct Attachments {
    map: Mutex<Option<Box<AttachmentMap>>>,
}

impl Attachments {
    /// The map of the holder, `None` before the first attachment. A caller that stores a value
    /// creates the map with `get_or_insert_with(Box::default)`.
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
    }

    #[test]
    fn map_replaces_and_removes_by_index() {
        let mut map = AttachmentMap::default();
        assert!(map.insert(3, Box::new(1i32)).is_none());
        let previous = map.insert(3, Box::new(2i32)).unwrap();
        assert_eq!(previous.downcast_ref::<i32>(), Some(&1));
        assert!(map.contains(3));
        assert!(!map.contains(4));
        let removed = map.remove(3).unwrap();
        assert_eq!(removed.downcast_ref::<i32>(), Some(&2));
        assert!(map.is_empty());
        assert!(map.remove(3).is_none());
    }
}
