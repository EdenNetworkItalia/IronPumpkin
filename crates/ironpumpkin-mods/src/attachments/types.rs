//! The attachment types behind the typed handles: the registry, the NBT form of a holder's
//! attachments and the copy rules.

use std::{
    any::Any,
    sync::{Mutex, OnceLock, PoisonError},
};

use pumpkin_core::{entity::player::Player, plugin::attachments::AttachmentMap};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::identifier::Identifier;
use rustc_hash::FxHashMap;
use tracing::error;

use super::{
    AttachmentCopyHandler, AttachmentHolder, AttachmentSerializer, AttachmentSyncHandler,
    AttachmentValue,
};

/// The default value or the default constructor of a type.
pub(super) type DefaultFn<T> = Box<dyn Fn(&AttachmentHolder<'_>) -> T + Send + Sync>;

/// One attachment type with its value type.
pub(super) struct TypedEntry<T> {
    pub(super) index: u32,
    pub(super) name: String,
    pub(super) synced_id: Option<u32>,
    pub(super) copy_on_death: bool,
    pub(super) default: DefaultFn<T>,
    pub(super) serializer: Option<Box<dyn AttachmentSerializer<T>>>,
    pub(super) copy_handler: Option<Box<dyn AttachmentCopyHandler<T>>>,
    pub(super) sync: Option<Box<dyn AttachmentSyncHandler<T>>>,
}

/// An attachment type without its value type, for the steps that visit every attachment of a
/// holder.
pub trait ErasedType: Send + Sync {
    fn index(&self) -> u32;
    fn name(&self) -> &str;
    fn synced_id(&self) -> Option<u32>;
    fn copy_on_death(&self) -> bool;
    fn has_serializer(&self) -> bool;
    /// The serialized value, `Ok(None)` when the serializer skips it or the type has none.
    fn write(&self, value: &dyn Any) -> Result<Option<NbtCompound>, String>;
    /// The value read from `nbt`. Only a type with a serializer is read.
    fn read(
        &self,
        holder: &AttachmentHolder<'_>,
        nbt: &NbtCompound,
    ) -> Result<AttachmentValue, String>;
    /// The copy for `holder`, `Ok(None)` when the copy handler drops the value.
    fn copy(
        &self,
        value: &dyn Any,
        holder: &AttachmentHolder<'_>,
    ) -> Result<Option<AttachmentValue>, String>;
    fn send_to_player(&self, holder: &AttachmentHolder<'_>, to: &Player) -> bool;
    /// Writes the sync bytes of `value`. A type without a sync handler writes nothing.
    fn write_sync(&self, buf: &mut Vec<u8>, value: &dyn Any, initial_sync: bool);
}

fn wrong_type(name: &str) -> String {
    format!("the value of attachment type {name} has another type")
}

impl<T: Send + Sync + 'static> ErasedType for TypedEntry<T> {
    fn index(&self) -> u32 {
        self.index
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn synced_id(&self) -> Option<u32> {
        self.synced_id
    }

    fn copy_on_death(&self) -> bool {
        self.copy_on_death
    }

    fn has_serializer(&self) -> bool {
        self.serializer.is_some()
    }

    fn write(&self, value: &dyn Any) -> Result<Option<NbtCompound>, String> {
        let Some(serializer) = &self.serializer else {
            return Ok(None);
        };
        let value = value
            .downcast_ref::<T>()
            .ok_or_else(|| wrong_type(&self.name))?;
        let mut nbt = NbtCompound::new();
        Ok(serializer.write(value, &mut nbt).then_some(nbt))
    }

    fn read(
        &self,
        holder: &AttachmentHolder<'_>,
        nbt: &NbtCompound,
    ) -> Result<AttachmentValue, String> {
        let serializer = self
            .serializer
            .as_ref()
            .ok_or_else(|| format!("attachment type {} has no serializer", self.name))?;
        Ok(Box::new(serializer.read(holder, nbt)?))
    }

    fn copy(
        &self,
        value: &dyn Any,
        holder: &AttachmentHolder<'_>,
    ) -> Result<Option<AttachmentValue>, String> {
        let value = value
            .downcast_ref::<T>()
            .ok_or_else(|| wrong_type(&self.name))?;
        if let Some(handler) = &self.copy_handler {
            return Ok(handler
                .copy(value, holder)
                .map(|copy| Box::new(copy) as AttachmentValue));
        }
        // `AttachmentType.defaultCopyHandler`: a write and a read through the serializer.
        let Some(serializer) = &self.serializer else {
            return Err("Cannot copy non-serializable attachments".to_owned());
        };
        let mut nbt = NbtCompound::new();
        if !serializer.write(value, &mut nbt) {
            return Ok(None);
        }
        Ok(Some(Box::new(serializer.read(holder, &nbt)?)))
    }

    fn send_to_player(&self, holder: &AttachmentHolder<'_>, to: &Player) -> bool {
        self.sync
            .as_ref()
            .is_some_and(|sync| sync.send_to_player(holder, to))
    }

    fn write_sync(&self, buf: &mut Vec<u8>, value: &dyn Any, initial_sync: bool) {
        if let Some(sync) = &self.sync
            && let Some(value) = value.downcast_ref::<T>()
        {
            sync.write(buf, value, initial_sync);
        }
    }
}

/// The frozen attachment types, by index and by name.
pub(super) struct Types {
    list: Vec<&'static dyn ErasedType>,
    by_name: FxHashMap<String, u32>,
}

impl Types {
    pub(super) fn new(list: Vec<&'static dyn ErasedType>) -> Self {
        let by_name = list
            .iter()
            .map(|entry| (entry.name().to_owned(), entry.index()))
            .collect();
        Self { list, by_name }
    }

    pub(super) fn get(&self, index: u32) -> Option<&'static dyn ErasedType> {
        self.list.get(index as usize).copied()
    }

    fn by_name(&self, name: &str) -> Option<&'static dyn ErasedType> {
        self.get(*self.by_name.get(name)?)
    }

    /// The names of the types with a sync handler, in the order of their synced ids.
    pub(super) fn synced_names(&self) -> Vec<Identifier> {
        let mut synced: Vec<_> = self
            .list
            .iter()
            .filter_map(|entry| Some((entry.synced_id()?, entry.name())))
            .collect();
        synced.sort_unstable_by_key(|(id, _)| *id);
        synced
            .into_iter()
            .filter_map(|(_, name)| Identifier::parse(name).ok())
            .collect()
    }

    /// `AttachmentHolder#serializeAttachments`: every attachment with a serializer, by type
    /// name, `None` when nothing is written. A failed write is logged and skipped.
    pub(super) fn write_map(&self, map: &AttachmentMap) -> Option<NbtCompound> {
        let mut out = NbtCompound::new();
        for (index, value) in map.iter() {
            let Some(entry) = self.get(index) else {
                continue;
            };
            match entry.write(value.as_ref()) {
                Ok(Some(nbt)) => out.put_compound(entry.name(), nbt),
                Ok(None) => {}
                Err(message) => error!(
                    "Failed to serialize data attachment {}. Skipping. {message}",
                    entry.name()
                ),
            }
        }
        (!out.is_empty()).then_some(out)
    }

    /// `AttachmentHolder#deserializeAttachments`: the values of the known types with a
    /// serializer. An invalid key, an unknown type, a type without a serializer and a failed
    /// read are logged and skipped. The caller stores the values after the reads, so a
    /// serializer can read the holder.
    pub(super) fn read_map(
        &self,
        holder: &AttachmentHolder<'_>,
        nbt: &NbtCompound,
    ) -> Vec<(u32, AttachmentValue)> {
        let mut values = Vec::new();
        for (key, tag) in &nbt.child_tags {
            if Identifier::parse(key).is_err() {
                error!("Encountered invalid data attachment key {key}. Skipping.");
                continue;
            }
            let Some(entry) = self.by_name(key).filter(|entry| entry.has_serializer()) else {
                error!("Encountered unknown or non-serializable data attachment {key}. Skipping.");
                continue;
            };
            // `ValueInput#rawChildOrEmpty`: a value that is not a compound reads as empty.
            let empty = NbtCompound::new();
            let child = match tag {
                NbtTag::Compound(child) => child,
                _ => &empty,
            };
            match entry.read(holder, child) {
                Ok(value) => values.push((entry.index(), value)),
                Err(message) => {
                    error!("Failed to deserialize data attachment {key}. Skipping. {message}");
                }
            }
        }
        values
    }

    /// `AttachmentInternals.copyAttachments`: the copies of the attachments of `from` with a
    /// serializer that `filter` accepts, for `to`. A copy handler that returns `None` drops the
    /// value, and a failed copy is logged and dropped.
    pub(super) fn copy_map(
        &self,
        from: &AttachmentMap,
        to: &AttachmentHolder<'_>,
        filter: impl Fn(&dyn ErasedType) -> bool,
    ) -> Vec<(u32, AttachmentValue)> {
        let mut copies = Vec::new();
        for (index, value) in from.iter() {
            let Some(entry) = self.get(index) else {
                continue;
            };
            if !entry.has_serializer() || !filter(entry) {
                continue;
            }
            match entry.copy(value.as_ref(), to) {
                Ok(Some(copy)) => copies.push((index, copy)),
                Ok(None) => {}
                Err(message) => error!(
                    "Failed to copy data attachment {}. Dropping it. {message}",
                    entry.name()
                ),
            }
        }
        copies
    }
}

/// The registrations before the freeze.
#[derive(Default)]
pub(super) struct Pending {
    pub(super) types: Vec<&'static dyn ErasedType>,
    pub(super) synced: u32,
    pub(super) error: Option<String>,
    pub(super) frozen: bool,
}

pub(super) static PENDING: Mutex<Pending> = Mutex::new(Pending {
    types: Vec::new(),
    synced: 0,
    error: None,
    frozen: false,
});

/// Set at the freeze when a mod registered a type.
pub(super) static TYPES: OnceLock<Types> = OnceLock::new();

pub(super) fn pending() -> std::sync::MutexGuard<'static, Pending> {
    PENDING.lock().unwrap_or_else(PoisonError::into_inner)
}
