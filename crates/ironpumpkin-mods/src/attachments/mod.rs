//! Data attachments: mod data stored on entities, block entities, chunks and levels, with the
//! behaviour of `NeoForge` attachment types (`net.neoforged.neoforge.attachment`).
//!
//! A mod registers an [`AttachmentType`] in `neoforge:attachment_types` with
//! [`ModInit::register_attachment_type`] and an [`AttachmentTypeBuilder`]: a default value or a
//! default constructor, an optional serializer, copy on death with an optional copy handler, and
//! an optional sync handler. The handle is typed, so a value of another type does not compile.
//! A registration error stops the server at the freeze of the attachment types, after the last
//! `RegisterEvent`.
//!
//! The holders implement [`AttachmentAccess`]: every [`Entity`] (players included, through
//! `player.get_entity()`), every [`World`] (the level), the block entity of
//! [`block_entity`] and the loaded chunk of [`chunk`].
//!
//! - Saving: a type with a serializer is saved with its holder under the `neoforge:attachments`
//!   NBT key and restored on load. A type without one does not survive a save. The level saves
//!   to `data/neoforge/data_attachments.dat` in its dimension folder, as
//!   `LevelAttachmentsSavedData` does.
//! - Copy on death: when a player respawns after death, only the types with copy on death keep
//!   their value, through their copy handler. When a player leaves the End, every type with a
//!   serializer keeps its value. A type without a serializer never survives a respawn.
//! - Sync: a type with a sync handler reaches the `NeoForge` clients that negotiated
//!   `neoforge:sync_attachments` when its value is created, set or removed and on
//!   [`AttachmentAccess::sync_data`], and every holder sends its synced values when a client
//!   starts to see it. A vanilla client gets nothing.
//!
//! A holder without attachments allocates nothing and writes no NBT key, and without a
//! registered type the engine hooks are not installed.
//!
//! The holder calls a default constructor, a serializer, a copy handler and a sync handler
//! with the lock of its attachments released where `NeoForge` order allows it, but
//! [`AttachmentAccess::with_data`] and the sync writes run under the lock: such code must not
//! read or write the attachments of the same holder.
//!
//! ```
//! use ironpumpkin_mods::{
//!     ModInit,
//!     attachments::{
//!         AttachmentAccess, AttachmentType, AttachmentTypeBuilder, NbtCompound, NbtSerializer,
//!     },
//!     entity::EntityBase,
//! };
//! # fn init(cx: &mut ModInit) -> AttachmentType<i32> {
//! let mana = cx
//!     .register_attachment_type(
//!         "examplemod:mana",
//!         AttachmentTypeBuilder::new(|| 0i32)
//!             .serialize(NbtSerializer::new(
//!                 |nbt: &NbtCompound| nbt.get_int("mana").ok_or_else(|| "no mana".to_owned()),
//!                 |mana: &i32, nbt: &mut NbtCompound| nbt.put_int("mana", *mana),
//!             ))
//!             .copy_on_death(),
//!     )
//!     .unwrap();
//! # mana
//! # }
//! # fn on_kill(player: &ironpumpkin_mods::entity::player::Player, mana: AttachmentType<i32>) {
//! let entity = player.get_entity();
//! let old = entity.get_data(mana);
//! entity.set_data(mana, old + 10);
//! # }
//! ```

mod hooks;
mod sync;
mod types;

#[cfg(test)]
mod tests;

use std::{fmt, marker::PhantomData, sync::Arc};

use pumpkin_codecs::{Decode, Encode};
pub use pumpkin_core::plugin::attachments::ATTACHMENTS_NBT_KEY;
use pumpkin_core::{
    block::entities::BlockEntity,
    entity::{Entity, player::Player},
    plugin::attachments::{AttachmentMap, AttachmentValue, Attachments},
    world::World,
};
/// The saved form of an attachment, re-exported for serializers.
pub use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::{nbt_ops::NbtOps, tag::NbtTag};
use pumpkin_util::{identifier::Identifier, math::vector2::Vector2};
use pumpkin_world::chunk::{ChunkData, io::Dirtiable};
pub use sync::SYNC_ATTACHMENTS_CHANNEL;
use types::{DefaultFn, ErasedType, TypedEntry};

use crate::ModInit;
use private::Slot;

/// The holder of an attachment, as a default constructor, a serializer, a copy handler and a
/// sync handler see it.
///
/// `NeoForge`: `IAttachmentHolder`, which is the entity, the block entity, the chunk or the
/// level itself. A chunk holder carries the chunk data only: a chunk that loads from disk has no
/// world yet.
#[derive(Clone, Copy)]
pub enum AttachmentHolder<'a> {
    /// An entity, players included.
    Entity(&'a Entity),
    /// A block entity.
    BlockEntity(&'a dyn BlockEntity),
    /// A chunk.
    Chunk(&'a ChunkData),
    /// A level.
    Level(&'a World),
}

impl fmt::Debug for AttachmentHolder<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Entity(entity) => f.debug_tuple("Entity").field(&entity.entity_id).finish(),
            Self::BlockEntity(block_entity) => f
                .debug_tuple("BlockEntity")
                .field(&block_entity.get_position())
                .finish(),
            Self::Chunk(chunk) => f.debug_tuple("Chunk").field(&(chunk.x, chunk.z)).finish(),
            Self::Level(world) => f.debug_tuple("Level").field(&world.dimension).finish(),
        }
    }
}

/// Reads and writes the saved form of an attachment.
///
/// `NeoForge`: `IAttachmentSerializer`, with the NBT compound of the attachment as `ValueInput`
/// and `ValueOutput`.
pub trait AttachmentSerializer<T>: Send + Sync + 'static {
    /// Reads a value for `holder`. An error is logged and the attachment is skipped.
    ///
    /// # Errors
    ///
    /// Why the value cannot be read.
    fn read(&self, holder: &AttachmentHolder<'_>, nbt: &NbtCompound) -> Result<T, String>;

    /// Writes `value` into `nbt`, or returns `false` to save nothing for it.
    fn write(&self, value: &T, nbt: &mut NbtCompound) -> bool;
}

/// An [`AttachmentSerializer`] from a read and a write function. The write always saves.
pub struct NbtSerializer<R, W> {
    read: R,
    write: W,
}

impl<R, W> NbtSerializer<R, W> {
    /// The serializer with these functions.
    pub const fn new(read: R, write: W) -> Self {
        Self { read, write }
    }
}

impl<T, R, W> AttachmentSerializer<T> for NbtSerializer<R, W>
where
    R: Fn(&NbtCompound) -> Result<T, String> + Send + Sync + 'static,
    W: Fn(&T, &mut NbtCompound) + Send + Sync + 'static,
{
    fn read(&self, _holder: &AttachmentHolder<'_>, nbt: &NbtCompound) -> Result<T, String> {
        (self.read)(nbt)
    }

    fn write(&self, value: &T, nbt: &mut NbtCompound) -> bool {
        (self.write)(value, nbt);
        true
    }
}

/// A value that saves itself.
///
/// `NeoForge`: `ValueIOSerializable`, for [`AttachmentTypeBuilder::serializable`].
pub trait NbtSerializable {
    /// Writes the value. `NeoForge`: `serialize(ValueOutput)`.
    fn serialize(&self, nbt: &mut NbtCompound);
    /// Reads the saved state into a default value. `NeoForge`: `deserialize(ValueInput)`.
    fn deserialize(&mut self, nbt: &NbtCompound);
}

/// Copies an attachment to the holder that replaces its holder.
///
/// `NeoForge`: `IAttachmentCopyHandler`.
pub trait AttachmentCopyHandler<T>: Send + Sync + 'static {
    /// The copy for `holder`, or `None` to drop the value.
    fn copy(&self, value: &T, holder: &AttachmentHolder<'_>) -> Option<T>;
}

impl<T, F> AttachmentCopyHandler<T> for F
where
    F: Fn(&T, &AttachmentHolder<'_>) -> Option<T> + Send + Sync + 'static,
{
    fn copy(&self, value: &T, holder: &AttachmentHolder<'_>) -> Option<T> {
        self(value, holder)
    }
}

/// Sends an attachment to clients.
///
/// `NeoForge`: `AttachmentSyncHandler`. The `read` method of `NeoForge` runs on the client and
/// has no server counterpart.
pub trait AttachmentSyncHandler<T>: Send + Sync + 'static {
    /// Whether `to` gets the value of `holder` when it changes. Default: every player.
    fn send_to_player(&self, _holder: &AttachmentHolder<'_>, _to: &Player) -> bool {
        true
    }

    /// Writes `value` as the client reads it. On the initial sync, a handler that writes
    /// nothing leaves the type out of the payload.
    fn write(&self, buf: &mut Vec<u8>, value: &T, initial_sync: bool);
}

struct CodecSync<P, E> {
    send_to_player: P,
    encode: E,
}

impl<T, P, E> AttachmentSyncHandler<T> for CodecSync<P, E>
where
    P: Fn(&AttachmentHolder<'_>, &Player) -> bool + Send + Sync + 'static,
    E: Fn(&mut Vec<u8>, &T) + Send + Sync + 'static,
{
    fn send_to_player(&self, holder: &AttachmentHolder<'_>, to: &Player) -> bool {
        (self.send_to_player)(holder, to)
    }

    fn write(&self, buf: &mut Vec<u8>, value: &T, _initial_sync: bool) {
        (self.encode)(buf, value);
    }
}

/// The builder state of an attachment type without a serializer.
#[derive(Debug)]
pub struct NotSerialized;

/// The builder state of an attachment type with a serializer: copy on death and a copy handler
/// need one, as `NeoForge` checks at `copyOnDeath` and `copyHandler`.
#[derive(Debug)]
pub struct Serialized;

/// The builder of an attachment type.
///
/// `NeoForge`: `AttachmentType.Builder`. The serializer can be set once, and copy on death and
/// the copy handler exist only after it: the checks that `NeoForge` makes at run time are
/// compile errors here.
///
/// ```compile_fail
/// use ironpumpkin_mods::attachments::AttachmentTypeBuilder;
///
/// // `copyOnDeath requires a serializer`.
/// let _ = AttachmentTypeBuilder::new(|| 0i32).copy_on_death();
/// ```
pub struct AttachmentTypeBuilder<T, S = NotSerialized> {
    default: DefaultFn<T>,
    serializer: Option<Box<dyn AttachmentSerializer<T>>>,
    copy_on_death: bool,
    copy_handler: Option<Box<dyn AttachmentCopyHandler<T>>>,
    sync: Option<Box<dyn AttachmentSyncHandler<T>>>,
    state: PhantomData<S>,
}

impl<T, S> fmt::Debug for AttachmentTypeBuilder<T, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttachmentTypeBuilder")
            .field("serializer", &self.serializer.is_some())
            .field("copy_on_death", &self.copy_on_death)
            .field("copy_handler", &self.copy_handler.is_some())
            .field("sync", &self.sync.is_some())
            .finish_non_exhaustive()
    }
}

impl<T: Send + Sync + 'static> AttachmentTypeBuilder<T, NotSerialized> {
    /// A type whose default value comes from `default`.
    ///
    /// `NeoForge`: `AttachmentType.builder(Supplier)`.
    pub fn new(default: impl Fn() -> T + Send + Sync + 'static) -> Self {
        Self::with_constructor(move |_| default())
    }

    /// A type whose default value comes from `constructor`, which gets the holder.
    ///
    /// `NeoForge`: `AttachmentType.builder(Function<IAttachmentHolder, T>)`.
    pub fn with_constructor(
        constructor: impl Fn(&AttachmentHolder<'_>) -> T + Send + Sync + 'static,
    ) -> Self {
        Self {
            default: Box::new(constructor),
            serializer: None,
            copy_on_death: false,
            copy_handler: None,
            sync: None,
            state: PhantomData,
        }
    }

    /// Saves the type with `serializer`.
    ///
    /// `NeoForge`: `AttachmentType.Builder#serialize(IAttachmentSerializer)`.
    pub fn serialize(
        self,
        serializer: impl AttachmentSerializer<T>,
    ) -> AttachmentTypeBuilder<T, Serialized> {
        AttachmentTypeBuilder {
            default: self.default,
            serializer: Some(Box::new(serializer)),
            copy_on_death: false,
            copy_handler: None,
            sync: self.sync,
            state: PhantomData,
        }
    }

    /// Saves the type with its codec, which must encode a compound, as a `MapCodec` does.
    ///
    /// `NeoForge`: `AttachmentType.Builder#serialize(MapCodec)`.
    #[must_use]
    pub fn serialize_codec(self) -> AttachmentTypeBuilder<T, Serialized>
    where
        T: Encode + Decode,
    {
        self.serialize_codec_if(|_| true)
    }

    /// Saves the type with its codec, and only the values that `should_serialize` accepts.
    ///
    /// `NeoForge`: `AttachmentType.Builder#serialize(MapCodec, Predicate)`.
    pub fn serialize_codec_if(
        self,
        should_serialize: impl Fn(&T) -> bool + Send + Sync + 'static,
    ) -> AttachmentTypeBuilder<T, Serialized>
    where
        T: Encode + Decode,
    {
        self.serialize(CodecSerializer { should_serialize })
    }
}

impl<T: NbtSerializable + Send + Sync + 'static> AttachmentTypeBuilder<T, Serialized> {
    /// A type that saves itself: a read builds the default value, then reads into it.
    ///
    /// `NeoForge`: `AttachmentType.serializable(Supplier)`.
    pub fn serializable(default: impl Fn() -> T + Send + Sync + 'static) -> Self {
        Self::serializable_with_constructor(move |_| default())
    }

    /// Like [`serializable`](Self::serializable), with a default constructor that gets the
    /// holder.
    ///
    /// `NeoForge`: `AttachmentType.serializable(Function<IAttachmentHolder, T>)`.
    pub fn serializable_with_constructor(
        constructor: impl Fn(&AttachmentHolder<'_>) -> T + Send + Sync + 'static,
    ) -> Self {
        let constructor = Arc::new(constructor);
        let read = constructor.clone();
        AttachmentTypeBuilder::with_constructor(move |holder| constructor(holder))
            .serialize(SelfSerializer { constructor: read })
    }
}

impl<T: Send + Sync + 'static> AttachmentTypeBuilder<T, Serialized> {
    /// Keeps the value on the player that respawns after death.
    ///
    /// `NeoForge`: `AttachmentType.Builder#copyOnDeath`.
    #[must_use]
    pub const fn copy_on_death(mut self) -> Self {
        self.copy_on_death = true;
        self
    }

    /// Copies the value with `handler` on respawn, instead of a write and a read through the
    /// serializer.
    ///
    /// `NeoForge`: `AttachmentType.Builder#copyHandler`.
    #[must_use]
    pub fn copy_handler(mut self, handler: impl AttachmentCopyHandler<T>) -> Self {
        self.copy_handler = Some(Box::new(handler));
        self
    }
}

impl<T: Send + Sync + 'static, S> AttachmentTypeBuilder<T, S> {
    /// Sends the type to clients with `handler`.
    ///
    /// `NeoForge`: `AttachmentType.Builder#sync(AttachmentSyncHandler)`.
    #[must_use]
    pub fn sync(mut self, handler: impl AttachmentSyncHandler<T>) -> Self {
        self.sync = Some(Box::new(handler));
        self
    }

    /// Sends the type to every client with `encode`, the encoder of its stream codec.
    ///
    /// `NeoForge`: `AttachmentType.Builder#sync(StreamCodec)`.
    #[must_use]
    pub fn sync_with(self, encode: impl Fn(&mut Vec<u8>, &T) + Send + Sync + 'static) -> Self {
        self.sync_to(|_, _| true, encode)
    }

    /// Sends the type with `encode` to the players that `send_to_player` accepts.
    ///
    /// `NeoForge`: `AttachmentType.Builder#sync(BiPredicate, StreamCodec)`.
    #[must_use]
    pub fn sync_to(
        self,
        send_to_player: impl Fn(&AttachmentHolder<'_>, &Player) -> bool + Send + Sync + 'static,
        encode: impl Fn(&mut Vec<u8>, &T) + Send + Sync + 'static,
    ) -> Self {
        self.sync(CodecSync {
            send_to_player,
            encode,
        })
    }
}

struct CodecSerializer<P> {
    should_serialize: P,
}

impl<T, P> AttachmentSerializer<T> for CodecSerializer<P>
where
    T: Encode + Decode,
    P: Fn(&T) -> bool + Send + Sync + 'static,
{
    fn read(&self, _holder: &AttachmentHolder<'_>, nbt: &NbtCompound) -> Result<T, String> {
        match T::parse(NbtTag::Compound(nbt.clone()), &NbtOps) {
            pumpkin_codecs::DataResult::Success { result, .. } => Ok(result),
            pumpkin_codecs::DataResult::Error { message, .. } => Err(message),
        }
    }

    fn write(&self, value: &T, nbt: &mut NbtCompound) -> bool {
        if !(self.should_serialize)(value) {
            return false;
        }
        match value.encode_start(&NbtOps) {
            pumpkin_codecs::DataResult::Success {
                result: NbtTag::Compound(encoded),
                ..
            } => {
                *nbt = encoded;
                true
            }
            pumpkin_codecs::DataResult::Success { .. } => {
                tracing::error!("an attachment codec must encode a compound");
                false
            }
            pumpkin_codecs::DataResult::Error { message, .. } => {
                tracing::error!("an attachment codec failed: {message}");
                false
            }
        }
    }
}

struct SelfSerializer<C> {
    constructor: Arc<C>,
}

impl<T, C> AttachmentSerializer<T> for SelfSerializer<C>
where
    T: NbtSerializable,
    C: Fn(&AttachmentHolder<'_>) -> T + Send + Sync + 'static,
{
    fn read(&self, holder: &AttachmentHolder<'_>, nbt: &NbtCompound) -> Result<T, String> {
        let mut value = (self.constructor)(holder);
        value.deserialize(nbt);
        Ok(value)
    }

    fn write(&self, value: &T, nbt: &mut NbtCompound) -> bool {
        value.serialize(nbt);
        true
    }
}

/// A registered attachment type. The handle is `Copy`; a mod keeps it, for example in a
/// `OnceLock`, and passes it to the holder calls.
///
/// `NeoForge`: `AttachmentType<T>`, an entry of `neoforge:attachment_types`. The handle fixes
/// the value type, so a value of another type does not compile:
///
/// ```compile_fail
/// use ironpumpkin_mods::{
///     attachments::{AttachmentAccess, AttachmentType},
///     entity::Entity,
/// };
///
/// fn set(entity: &Entity, mana: AttachmentType<i32>) {
///     entity.set_data(mana, "ten");
/// }
/// ```
pub struct AttachmentType<T: 'static> {
    entry: &'static TypedEntry<T>,
}

impl<T> Clone for AttachmentType<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for AttachmentType<T> {}

impl<T> PartialEq for AttachmentType<T> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.entry, other.entry)
    }
}

impl<T> Eq for AttachmentType<T> {}

impl<T> fmt::Debug for AttachmentType<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AttachmentType")
            .field(&self.entry.name)
            .finish()
    }
}

impl<T> AttachmentType<T> {
    /// The name of the type in `neoforge:attachment_types`.
    #[must_use]
    pub fn name(&self) -> &'static str {
        &self.entry.name
    }

    const fn index(self) -> u32 {
        self.entry.index
    }
}

impl<T: Send + Sync + 'static> AttachmentType<T> {
    fn erased(self) -> &'static dyn ErasedType {
        self.entry
    }
}

/// Why an attachment call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentError {
    /// The name is not a valid `namespace:path` resource location.
    InvalidName(String),
    /// The name has no namespace or uses the `minecraft` namespace.
    ReservedNamespace(String),
    /// A type with this name is already registered.
    Duplicate(String),
    /// The attachment types are frozen: a type registers in [`crate::NativeMod::init`].
    RegistryFrozen(String),
    /// `sync_data` on a type without a sync handler.
    NotSynced(String),
}

impl fmt::Display for AttachmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName(name) => write!(f, "{name} is not a valid resource location"),
            Self::ReservedNamespace(name) => {
                write!(f, "{name} has no namespace or uses the minecraft namespace")
            }
            Self::Duplicate(name) => write!(f, "{name} is already registered"),
            Self::RegistryFrozen(name) => {
                write!(
                    f,
                    "{name} registers after the freeze of the attachment types"
                )
            }
            Self::NotSynced(name) => write!(f, "attachment type {name} has no sync handler"),
        }
    }
}

impl std::error::Error for AttachmentError {}

fn validate_name(name: &str) -> Result<(), AttachmentError> {
    let Some((namespace, path)) = name.split_once(':') else {
        return Err(AttachmentError::ReservedNamespace(name.to_owned()));
    };
    if namespace.is_empty() || namespace == "minecraft" {
        return Err(AttachmentError::ReservedNamespace(name.to_owned()));
    }
    if Identifier::is_valid_namespace(namespace)
        && !path.is_empty()
        && Identifier::is_valid_path(path)
    {
        Ok(())
    } else {
        Err(AttachmentError::InvalidName(name.to_owned()))
    }
}

/// Registers a type in the pending list.
fn register<T: Send + Sync + 'static, S>(
    name: &str,
    builder: AttachmentTypeBuilder<T, S>,
) -> Result<AttachmentType<T>, AttachmentError> {
    validate_name(name)?;
    let mut pending = types::pending();
    if pending.frozen {
        return Err(AttachmentError::RegistryFrozen(name.to_owned()));
    }
    if pending.types.iter().any(|entry| entry.name() == name) {
        return Err(AttachmentError::Duplicate(name.to_owned()));
    }
    let synced_id = builder.sync.is_some().then(|| {
        let id = pending.synced;
        pending.synced += 1;
        id
    });
    let ty = leak_entry(pending.types.len() as u32, name, synced_id, builder);
    pending.types.push(ty.entry);
    Ok(ty)
}

/// The type `index` of a registry, with the parts of `builder`. Each type lives as long as the
/// process, as a registry entry does.
fn leak_entry<T: Send + Sync + 'static, S>(
    index: u32,
    name: &str,
    synced_id: Option<u32>,
    builder: AttachmentTypeBuilder<T, S>,
) -> AttachmentType<T> {
    let entry: &'static TypedEntry<T> = Box::leak(Box::new(TypedEntry {
        index,
        name: name.to_owned(),
        synced_id,
        copy_on_death: builder.copy_on_death,
        default: builder.default,
        serializer: builder.serializer,
        copy_handler: builder.copy_handler,
        sync: builder.sync,
    }));
    AttachmentType { entry }
}

impl ModInit {
    /// Registers an attachment type in `neoforge:attachment_types`.
    ///
    /// `NeoForge`: a `DeferredRegister` of `NeoForgeRegistries.ATTACHMENT_TYPES`. The first
    /// error of the process stops the server at the freeze of the attachment types, as a failed
    /// registration does on `NeoForge`.
    ///
    /// # Errors
    ///
    /// [`AttachmentError::InvalidName`], [`AttachmentError::ReservedNamespace`],
    /// [`AttachmentError::Duplicate`] and [`AttachmentError::RegistryFrozen`].
    pub fn register_attachment_type<T: Send + Sync + 'static, S>(
        &mut self,
        name: &str,
        builder: AttachmentTypeBuilder<T, S>,
    ) -> Result<AttachmentType<T>, AttachmentError> {
        let result = register(name, builder);
        if let Err(error) = &result {
            types::pending().error.get_or_insert_with(|| {
                format!(
                    "native mod \"{}\" cannot register the attachment type \"{name}\": {error}",
                    self.id
                )
            });
        }
        result
    }
}

/// Freezes the attachment types after the last `RegisterEvent` and installs the engine hooks
/// when a mod registered one.
///
/// # Errors
///
/// The first registration error of the process.
#[cfg_attr(
    test,
    expect(
        dead_code,
        reason = "the lifecycle unit tests leave the process registry open"
    )
)]
pub(crate) fn freeze() -> Result<(), String> {
    let list = {
        let mut pending = types::pending();
        if pending.frozen {
            return Ok(());
        }
        pending.frozen = true;
        if let Some(error) = pending.error.take() {
            return Err(error);
        }
        std::mem::take(&mut pending.types)
    };
    if list.is_empty() {
        return Ok(());
    }
    let types = types::Types::new(list);
    let _ = pumpkin_core::plugin::attachments::SYNCED_ATTACHMENT_TYPES.set(types.synced_names());
    if types::TYPES.set(types).is_ok() {
        hooks::install();
    }
    Ok(())
}

mod private {
    use std::{ops::Deref, sync::Arc};

    use super::{AttachmentHolder, Attachments, ErasedType};

    /// The attachments of a holder, borrowed or shared.
    pub enum Slot<'a> {
        Borrowed(&'a Attachments),
        Shared(Arc<Attachments>),
    }

    impl Deref for Slot<'_> {
        type Target = Attachments;

        fn deref(&self) -> &Attachments {
            match self {
                Self::Borrowed(slot) => slot,
                Self::Shared(slot) => slot,
            }
        }
    }

    /// The engine side of a holder.
    pub trait Holder {
        /// The attachment slot, `None` when the holder has none.
        fn existing_slot(&self) -> Option<Slot<'_>>;
        /// The attachment slot, created when the holder has none.
        fn slot(&self) -> Slot<'_>;
        /// The holder as the mod callbacks see it.
        fn view(&self) -> AttachmentHolder<'_>;
        /// `setChanged` of a block entity, `markUnsaved` of a chunk.
        fn changed(&self) {}
        /// `syncData` of the holder for a type with a sync handler.
        fn sync(&self, entry: &'static dyn ErasedType);
    }
}

fn downcast<T: 'static>(map: &AttachmentMap, index: u32) -> Option<&T> {
    map.get(index)?.downcast_ref::<T>()
}

fn take_value<T: 'static>(value: Option<AttachmentValue>) -> Option<T> {
    value?.downcast::<T>().ok().map(|value| *value)
}

/// The attachment calls of a holder.
///
/// `NeoForge`: `IAttachmentHolder`.
pub trait AttachmentAccess: private::Holder {
    /// Whether the holder has at least one attachment.
    ///
    /// `NeoForge`: `IAttachmentHolder#hasAttachments`.
    fn has_attachments(&self) -> bool {
        self.existing_slot()
            .is_some_and(|slot| slot.has_attachments())
    }

    /// Whether the holder has a value of `ty`.
    ///
    /// `NeoForge`: `IAttachmentHolder#hasData`.
    fn has_data<T: Send + Sync + 'static>(&self, ty: AttachmentType<T>) -> bool {
        self.existing_slot().is_some_and(|slot| {
            slot.lock()
                .as_ref()
                .is_some_and(|map| map.contains(ty.index()))
        })
    }

    /// The value of `ty`, created from the default and stored when the holder has none. A
    /// created value is synced.
    ///
    /// `NeoForge`: `IAttachmentHolder#getData`, which returns the stored object itself: a value
    /// that a mod changes in place is an `Arc` with interior mutability, or goes through
    /// [`with_data`](Self::with_data).
    fn get_data<T: Clone + Send + Sync + 'static>(&self, ty: AttachmentType<T>) -> T {
        self.with_data(ty, |value| value.clone())
    }

    /// Runs `f` on the stored value of `ty`, created from the default when the holder has
    /// none. A change in place is not synced: call [`sync_data`](Self::sync_data) after it,
    /// as on `NeoForge`. `f` runs under the lock of the holder's attachments.
    fn with_data<T: Send + Sync + 'static, R>(
        &self,
        ty: AttachmentType<T>,
        f: impl FnOnce(&mut T) -> R,
    ) -> R {
        let index = ty.index();
        let mut created = None;
        let mut synced = false;
        loop {
            let slot = self.slot();
            let mut guard = slot.lock();
            let map = guard.get_or_insert_with(Box::default);
            if let Some(value) = created.take()
                && downcast::<T>(map, index).is_none()
            {
                map.insert(index, Box::new(value));
                synced = true;
            }
            if let Some(value) = map
                .get_mut(index)
                .and_then(|value| value.downcast_mut::<T>())
            {
                let result = f(value);
                drop(guard);
                if synced {
                    self.sync(ty.erased());
                }
                return result;
            }
            drop(guard);
            // `getData`: the default comes from the holder with the lock released.
            created = Some((ty.entry.default)(&self.view()));
        }
    }

    /// The value of `ty`, or `None` when the holder has none.
    ///
    /// `NeoForge`: `IAttachmentHolder#getExistingData`.
    fn get_existing_data<T: Clone + Send + Sync + 'static>(
        &self,
        ty: AttachmentType<T>,
    ) -> Option<T> {
        let slot = self.existing_slot()?;
        let guard = slot.lock();
        downcast::<T>(guard.as_ref()?, ty.index()).cloned()
    }

    /// Stores `value` for `ty`, syncs it and returns the previous value.
    ///
    /// `NeoForge`: `IAttachmentHolder#setData`.
    fn set_data<T: Send + Sync + 'static>(&self, ty: AttachmentType<T>, value: T) -> Option<T> {
        self.changed();
        let previous = self
            .slot()
            .lock()
            .get_or_insert_with(Box::default)
            .insert(ty.index(), Box::new(value));
        self.sync(ty.erased());
        take_value(previous)
    }

    /// Removes the value of `ty`, syncs the removal and returns the value.
    ///
    /// `NeoForge`: `IAttachmentHolder#removeData`.
    fn remove_data<T: Send + Sync + 'static>(&self, ty: AttachmentType<T>) -> Option<T> {
        self.changed();
        let slot = self.existing_slot()?;
        let previous = {
            let mut guard = slot.lock();
            guard.as_mut()?.remove(ty.index())
        };
        drop(slot);
        self.sync(ty.erased());
        take_value(previous)
    }

    /// Sends the current value of `ty`, or its absence, to the clients that see the holder.
    ///
    /// `NeoForge`: `IAttachmentHolder#syncData`, which does nothing for a type without a sync
    /// handler.
    ///
    /// # Errors
    ///
    /// [`AttachmentError::NotSynced`] when `ty` has no sync handler.
    fn sync_data<T: Send + Sync + 'static>(
        &self,
        ty: AttachmentType<T>,
    ) -> Result<(), AttachmentError> {
        if ty.entry.sync.is_none() {
            return Err(AttachmentError::NotSynced(ty.name().to_owned()));
        }
        self.sync(ty.erased());
        Ok(())
    }
}

impl private::Holder for Entity {
    fn existing_slot(&self) -> Option<Slot<'_>> {
        Some(self.slot())
    }

    fn slot(&self) -> Slot<'_> {
        Slot::Borrowed(&self.attachments)
    }

    fn view(&self) -> AttachmentHolder<'_> {
        AttachmentHolder::Entity(self)
    }

    fn sync(&self, entry: &'static dyn ErasedType) {
        if entry.synced_id().is_none() {
            return;
        }
        let world = self.world.load_full();
        let players = sync::entity_recipients(&world, self);
        let guard = self.attachments.lock();
        sync::sync_update(&self.view(), guard.as_deref(), entry, &players);
    }
}

impl AttachmentAccess for Entity {}

impl private::Holder for World {
    fn existing_slot(&self) -> Option<Slot<'_>> {
        Some(self.slot())
    }

    fn slot(&self) -> Slot<'_> {
        Slot::Borrowed(&self.attachments.level)
    }

    fn view(&self) -> AttachmentHolder<'_> {
        AttachmentHolder::Level(self)
    }

    fn sync(&self, entry: &'static dyn ErasedType) {
        if entry.synced_id().is_none() {
            return;
        }
        let players = sync::level_recipients(self);
        let guard = self.attachments.level.lock();
        sync::sync_update(&self.view(), guard.as_deref(), entry, &players);
    }
}

impl AttachmentAccess for World {}

/// The attachments of a block entity of a world.
///
/// `NeoForge`: `BlockEntity` as an `IAttachmentHolder`. A change marks the chunk unsaved
/// (`setChanged`), and a sync goes to the players that see the chunk at the start of the next
/// world tick (`ChunkHolder#broadcastChanges`).
pub struct BlockEntityAttachments<'a> {
    world: &'a World,
    block_entity: &'a Arc<dyn BlockEntity>,
}

impl fmt::Debug for BlockEntityAttachments<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("BlockEntityAttachments")
            .field(&self.block_entity.get_position())
            .finish()
    }
}

/// The attachment holder of `block_entity`, which is in `world`.
#[must_use]
pub const fn block_entity<'a>(
    world: &'a World,
    block_entity: &'a Arc<dyn BlockEntity>,
) -> BlockEntityAttachments<'a> {
    BlockEntityAttachments {
        world,
        block_entity,
    }
}

impl private::Holder for BlockEntityAttachments<'_> {
    fn existing_slot(&self) -> Option<Slot<'_>> {
        self.world
            .attachments
            .block_entity(self.block_entity)
            .map(Slot::Shared)
    }

    fn slot(&self) -> Slot<'_> {
        Slot::Shared(
            self.world
                .attachments
                .block_entity_or_insert(self.block_entity),
        )
    }

    fn view(&self) -> AttachmentHolder<'_> {
        AttachmentHolder::BlockEntity(self.block_entity.as_ref())
    }

    fn changed(&self) {
        let chunk = self.block_entity.get_position().chunk_position();
        self.world.level.read_chunk_sync(&chunk, |chunk| {
            chunk.mark_dirty(true);
        });
    }

    fn sync(&self, entry: &'static dyn ErasedType) {
        if entry.synced_id().is_some() {
            self.world
                .attachments
                .queue_block_entity_sync(self.block_entity.get_position(), entry.index());
        }
    }
}

impl AttachmentAccess for BlockEntityAttachments<'_> {}

/// The attachments of a loaded chunk of a world.
///
/// `NeoForge`: `LevelChunk` as an `IAttachmentHolder`. A change marks the chunk unsaved
/// (`markUnsaved`), and a sync goes to the players that got the chunk.
pub struct ChunkAttachments<'a> {
    world: &'a World,
    chunk: Arc<ChunkData>,
}

impl fmt::Debug for ChunkAttachments<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ChunkAttachments")
            .field(&(self.chunk.x, self.chunk.z))
            .finish()
    }
}

/// The attachment holder of the chunk at `pos` (chunk coordinates) of `world`, `None` when the
/// chunk is not loaded.
#[must_use]
pub fn chunk(world: &World, pos: Vector2<i32>) -> Option<ChunkAttachments<'_>> {
    let chunk = world.level.read_chunk_sync(&pos, Arc::clone)?;
    Some(ChunkAttachments { world, chunk })
}

impl private::Holder for ChunkAttachments<'_> {
    fn existing_slot(&self) -> Option<Slot<'_>> {
        Some(self.slot())
    }

    fn slot(&self) -> Slot<'_> {
        Slot::Borrowed(&self.chunk.attachments)
    }

    fn view(&self) -> AttachmentHolder<'_> {
        AttachmentHolder::Chunk(&self.chunk)
    }

    fn changed(&self) {
        self.chunk.mark_dirty(true);
    }

    fn sync(&self, entry: &'static dyn ErasedType) {
        if entry.synced_id().is_none() {
            return;
        }
        let players = sync::chunk_recipients(self.world, Vector2::new(self.chunk.x, self.chunk.z));
        let guard = self.chunk.attachments.lock();
        sync::sync_update(&self.view(), guard.as_deref(), entry, &players);
    }
}

impl AttachmentAccess for ChunkAttachments<'_> {}
