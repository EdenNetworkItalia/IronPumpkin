//! Capabilities (`net.neoforged.neoforge.capabilities`): handlers that blocks, entities and items
//! expose to other mods, such as the item handler of a chest.
//!
//! A capability is a key with a name, a handler type and a context type, created with
//! [`BlockCapability::create`], [`EntityCapability::create`] or [`ItemCapability::create`]. The
//! handler type is a trait object, such as `dyn ResourceHandler<ItemResource>`, and a lookup
//! returns an `Arc` of it. The built-in keys are in [`item`], [`fluid`] and [`energy`]
//! (`Capabilities.Item`, `Capabilities.Fluid` and `Capabilities.Energy`).
//!
//! A mod registers providers in [`NativeMod::register_capabilities`](crate::NativeMod), the
//! `RegisterCapabilitiesEvent`. The host calls it once per [`EventPriority`] phase, from
//! `Highest` to `Lowest`, for every mod in load order, as `ModLoader.postEventWrapContainerInModOrder`
//! posts the event phase by phase. The host acts as the `neoforge` mod, first in each phase: it
//! marks the built-in block capabilities proxyable at `High` and registers the vanilla providers
//! at `Normal`. The event fires in `RegistrationEvents.init`, before `ExtendPoiTypesEvent`.
//! After the last phase the registration closes.
//!
//! [`BlockCapabilityCache`] keeps the result of a block lookup until the host invalidates it,
//! with the `NeoForge` triggers: a block entity is placed or removed, a chest changes its type, a
//! chunk loads or unloads, or a mod calls [`invalidate_capabilities`] for a block without a block
//! entity.
//!
//! `level.getCapability(cap, pos, context)` is `cap.get_capability(&world, &pos, context)`,
//! `entity.getCapability(cap, context)` is `cap.get_capability(&entity, context)` and
//! `stack.getCapability(cap, context)` is `cap.get_capability(&stack, context)`.

mod cache;
mod vanilla;

use std::{
    any::{Any, TypeId, type_name},
    collections::HashMap,
    fmt,
    marker::PhantomData,
    sync::{
        Arc, LazyLock, Mutex, OnceLock, PoisonError, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use rustc_hash::FxHashMap;

use pumpkin_core::{block::entities::BlockEntity, entity::EntityBase, world::World};
use pumpkin_data::{
    Block, BlockDirection, BlockId, BlockState,
    block_properties::BLOCK_ENTITY_TYPES,
    dynamic::{self, ContentKind},
    item::Item,
    item_stack::ItemStack,
};
use pumpkin_util::math::position::BlockPos;

pub use cache::{
    BlockCapabilityCache, CapabilityInvalidationListener, invalidate_capabilities,
    invalidate_capabilities_in_chunk, register_capability_listener,
};

use crate::{
    LoadedMod,
    event::EventPriority,
    registration::{self, RegistrationError},
    transfer::{EnergyHandler, FluidResource, ItemAccess, ItemResource, ResourceHandler},
};

/// Why a capability call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityError {
    /// `type-mismatch`: the name exists with another handler or context type.
    TypeMismatch {
        /// The capability name.
        name: String,
        /// The handler and context types of the existing capability.
        existing: String,
        /// The handler and context types of the call.
        requested: String,
    },
    /// `registration-closed`: the `RegisterCapabilitiesEvent` already fired.
    RegistrationClosed,
    /// A provider registered for no block or item.
    NoTargets,
    /// No vanilla or mod block, block entity type, entity type or item has this name.
    Unknown(RegistrationError),
    /// No vanilla block entity type has this name.
    UnknownBlockEntityType(String),
    /// `setProxyable` after `setNonProxyable`, or the reverse.
    ProxyableConflict {
        /// The capability name.
        name: String,
        /// Whether the capability was already marked proxyable.
        proxyable: bool,
    },
    /// A lookup on a cache whose `is_valid` returned false, or from its invalidation listener.
    InvalidCache,
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypeMismatch {
                name,
                existing,
                requested,
            } => write!(
                f,
                "capability {name} exists with {existing}, not {requested}"
            ),
            Self::RegistrationClosed => f.write_str("the RegisterCapabilitiesEvent already fired"),
            Self::NoTargets => f.write_str("a provider needs at least one block or item"),
            Self::Unknown(error) => error.fmt(f),
            Self::UnknownBlockEntityType(name) => write!(f, "unknown block entity type \"{name}\""),
            Self::ProxyableConflict { name, proxyable } => write!(
                f,
                "capability {name} is already {}",
                if *proxyable {
                    "proxyable"
                } else {
                    "non-proxyable"
                }
            ),
            Self::InvalidCache => f.write_str(
                "a lookup on an invalid capability cache or from its invalidation listener",
            ),
        }
    }
}

impl std::error::Error for CapabilityError {}

impl From<RegistrationError> for CapabilityError {
    fn from(error: RegistrationError) -> Self {
        Self::Unknown(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Block,
    Entity,
    Item,
}

/// A registered capability, whatever its types.
struct Entry {
    data: &'static (dyn Any + Send + Sync),
    freeze: &'static dyn Freeze,
    types: TypeId,
    type_names: String,
    proxyable: Option<&'static Mutex<Option<bool>>>,
}

type Entries = HashMap<(Kind, String), Entry>;
type Registry = Mutex<Entries>;

struct State {
    registry: Registry,
    builtins: builtins::Builtins,
}

/// `CapabilityRegistry`. The built-in capabilities enter first, so a mod that creates one of
/// their names with other types gets [`CapabilityError::TypeMismatch`].
static STATE: LazyLock<State> = LazyLock::new(|| {
    let registry = Registry::default();
    let builtins = builtins::Builtins::new(&registry);
    State { registry, builtins }
});

/// Providers in registration order: `seq` orders block and block entity providers of one
/// block together, as `NeoForge` keeps them in one list.
struct Ordered<P: ?Sized> {
    seq: u64,
    provider: Arc<P>,
}

static SEQ: AtomicU64 = AtomicU64::new(0);

fn ordered<P: ?Sized>(provider: Arc<P>) -> Ordered<P> {
    Ordered {
        seq: SEQ.fetch_add(1, Ordering::Relaxed),
        provider,
    }
}

impl<P: ?Sized> Clone for Ordered<P> {
    fn clone(&self) -> Self {
        Self {
            seq: self.seq,
            provider: self.provider.clone(),
        }
    }
}

/// `IBlockCapabilityProvider`: the world, the position, its block state, its block entity and
/// the context.
pub type BlockProvider<T, C> = dyn Fn(
        &Arc<World>,
        &BlockPos,
        &'static BlockState,
        Option<&Arc<dyn BlockEntity>>,
        C,
    ) -> Option<Arc<T>>
    + Send
    + Sync;
/// `ICapabilityProvider` of a block entity type.
pub type BlockEntityProvider<T, C> =
    dyn Fn(&Arc<dyn BlockEntity>, C) -> Option<Arc<T>> + Send + Sync;
/// `ICapabilityProvider` of an entity type.
pub type EntityProvider<T, C> = dyn Fn(&Arc<dyn EntityBase>, C) -> Option<Arc<T>> + Send + Sync;
/// `ICapabilityProvider` of an item.
pub type ItemProvider<T, C> = dyn Fn(&ItemStack, C) -> Option<Arc<T>> + Send + Sync;

type Providers<P> = FxHashMap<Box<str>, Vec<P>>;

/// Providers by block, block entity type, entity type or item name, without the `minecraft`
/// namespace, which `Block::name` and the other generated names also leave out. Registration
/// writes `building`; the end of the `RegisterCapabilitiesEvent` copies it into `frozen`, and a
/// lookup reads the frozen copy without a lock, so the parallel world ticks do not share a lock.
struct ByName<P> {
    building: RwLock<Providers<P>>,
    frozen: OnceLock<Providers<P>>,
}

impl<P> Default for ByName<P> {
    fn default() -> Self {
        Self {
            building: RwLock::default(),
            frozen: OnceLock::new(),
        }
    }
}

/// The key of a registry name in a [`ByName`].
fn key(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

impl<P: Clone> ByName<P> {
    fn push(&self, name: &str, provider: P) {
        self.building
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(key(name).into())
            .or_default()
            .push(provider);
    }

    fn contains(&self, name: &str) -> bool {
        self.building
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(key(name))
    }

    /// Whether no provider is registered, which a lookup checks before it reads the world.
    fn is_empty(&self) -> bool {
        self.frozen.get().map_or_else(
            || {
                self.building
                    .read()
                    .unwrap_or_else(PoisonError::into_inner)
                    .is_empty()
            },
            HashMap::is_empty,
        )
    }

    /// Runs `f` on the providers of `name`. Before the freeze it runs on a copy, so a provider
    /// that looks up a capability does not take the lock twice.
    fn with<R>(&self, name: &str, f: impl FnOnce(&[P]) -> R) -> R {
        if let Some(frozen) = self.frozen.get() {
            return f(frozen.get(key(name)).map_or(&[], Vec::as_slice));
        }
        let providers = self
            .building
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key(name))
            .cloned()
            .unwrap_or_default();
        f(&providers)
    }

    fn freeze(&self) {
        let providers = self
            .building
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let _ = self.frozen.set(providers);
    }
}

/// Copies the providers of a capability into its frozen maps.
trait Freeze: Send + Sync {
    fn freeze(&self);
}

struct BlockData<T: ?Sized + 'static, C: 'static> {
    name: String,
    blocks: ByName<Ordered<BlockProvider<T, C>>>,
    block_entities: ByName<Ordered<BlockEntityProvider<T, C>>>,
    proxyable: Mutex<Option<bool>>,
    /// Set at the freeze: whether a block provider is registered for a block without a block
    /// entity type, so a lookup at a position without a block entity can find a handler.
    plain: OnceLock<bool>,
}

impl<T: ?Sized + 'static, C: 'static> BlockData<T, C> {
    fn answers_without_block_entity(&self) -> bool {
        self.plain.get().copied().unwrap_or(true)
    }
}

/// What a block lookup knows of the block entity at the position.
#[derive(Clone, Copy)]
pub(crate) enum EntityHint<'a> {
    /// The lookup reads the block entity when a provider needs it.
    Unknown,
    /// The caller looked it up: `None` for no block entity.
    Known(Option<&'a Arc<dyn BlockEntity>>),
}

struct EntityData<T: ?Sized + 'static, C: 'static> {
    name: String,
    providers: ByName<Arc<EntityProvider<T, C>>>,
}

struct ItemData<T: ?Sized + 'static, C: 'static> {
    name: String,
    providers: ByName<Arc<ItemProvider<T, C>>>,
}

fn type_names<T: ?Sized, C>() -> String {
    format!(
        "handler {} and context {}",
        type_name::<T>(),
        type_name::<C>()
    )
}

/// What a capability kind stores, built from its name.
trait CapabilityData: Any + Freeze + Sized {
    const KIND: Kind;

    fn new(name: String) -> Self;

    fn proxyable(&'static self) -> Option<&'static Mutex<Option<bool>>> {
        None
    }
}

/// `CapabilityRegistry.create`: the data of `name`, created on first use.
fn intern<D: CapabilityData, T: ?Sized + 'static, C: 'static>(
    registry: &Registry,
    name: &str,
) -> Result<&'static D, CapabilityError> {
    let name = registration::namespaced(name);
    let mut entries = registry.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(entry) = entries.get(&(D::KIND, name.clone())) {
        return match entry.data.downcast_ref::<D>() {
            Some(data) if entry.types == TypeId::of::<(PhantomData<T>, C)>() => Ok(data),
            _ => Err(CapabilityError::TypeMismatch {
                name,
                existing: entry.type_names.clone(),
                requested: type_names::<T, C>(),
            }),
        };
    }
    Ok(insert::<D, T, C>(&mut entries, name))
}

fn insert<D: CapabilityData, T: ?Sized + 'static, C: 'static>(
    entries: &mut Entries,
    name: String,
) -> &'static D {
    let data: &'static D = Box::leak(Box::new(D::new(name.clone())));
    if CLOSED.load(Ordering::Acquire) {
        data.freeze();
    }
    entries.insert(
        (D::KIND, name),
        Entry {
            data,
            freeze: data,
            types: TypeId::of::<(PhantomData<T>, C)>(),
            type_names: type_names::<T, C>(),
            proxyable: data.proxyable(),
        },
    );
    data
}

impl<T: ?Sized + Send + Sync + 'static, C: Send + Sync + 'static> Freeze for BlockData<T, C> {
    fn freeze(&self) {
        self.blocks.freeze();
        self.block_entities.freeze();
        let plain = self.blocks.frozen.get().is_some_and(|providers| {
            providers.keys().any(|name| {
                Block::from_name(name)
                    .filter(|block| block.id.as_u16() < BlockId::COUNT)
                    .is_none_or(|block| block.default_state.block_entity_type == u16::MAX)
            })
        });
        let _ = self.plain.set(plain);
    }
}

impl<T: ?Sized + Send + Sync + 'static, C: Send + Sync + 'static> Freeze for EntityData<T, C> {
    fn freeze(&self) {
        self.providers.freeze();
    }
}

impl<T: ?Sized + Send + Sync + 'static, C: Send + Sync + 'static> Freeze for ItemData<T, C> {
    fn freeze(&self) {
        self.providers.freeze();
    }
}

impl<T: ?Sized + Send + Sync + 'static, C: Send + Sync + 'static> CapabilityData
    for BlockData<T, C>
{
    const KIND: Kind = Kind::Block;

    fn new(name: String) -> Self {
        Self {
            name,
            blocks: ByName::default(),
            block_entities: ByName::default(),
            proxyable: Mutex::new(None),
            plain: OnceLock::new(),
        }
    }

    fn proxyable(&'static self) -> Option<&'static Mutex<Option<bool>>> {
        Some(&self.proxyable)
    }
}

impl<T: ?Sized + Send + Sync + 'static, C: Send + Sync + 'static> CapabilityData
    for EntityData<T, C>
{
    const KIND: Kind = Kind::Entity;

    fn new(name: String) -> Self {
        Self {
            name,
            providers: ByName::default(),
        }
    }
}

impl<T: ?Sized + Send + Sync + 'static, C: Send + Sync + 'static> CapabilityData
    for ItemData<T, C>
{
    const KIND: Kind = Kind::Item;

    fn new(name: String) -> Self {
        Self {
            name,
            providers: ByName::default(),
        }
    }
}

/// `BlockCapability<T, C>`: a capability of blocks, with the handler type `T` and the context
/// type `C`, usually `Option<BlockDirection>`, the side.
pub struct BlockCapability<T: ?Sized + 'static, C: 'static> {
    data: &'static BlockData<T, C>,
}

impl<T: ?Sized + 'static, C: 'static> Clone for BlockCapability<T, C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized + 'static, C: 'static> Copy for BlockCapability<T, C> {}

impl<T: ?Sized + 'static, C: 'static> fmt::Debug for BlockCapability<T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlockCapability({})", self.data.name)
    }
}

impl<T: ?Sized + Send + Sync + 'static, C: Clone + Send + Sync + 'static> BlockCapability<T, C> {
    /// `BlockCapability.create`: the capability `name`, created on first use.
    ///
    /// # Errors
    ///
    /// [`CapabilityError::TypeMismatch`] when `name` exists with other types.
    pub fn create(name: &str) -> Result<Self, CapabilityError> {
        intern::<_, T, C>(&STATE.registry, name).map(|data| Self { data })
    }

    /// `name`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.data.name
    }

    /// `isProxyable`: whether a block may forward lookups of this capability to other blocks.
    /// Block capabilities are not proxyable by default.
    #[must_use]
    pub fn is_proxyable(&self) -> bool {
        *self
            .data
            .proxyable
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            == Some(true)
    }

    /// `getCapability` (`Level.getCapability`): the handler at `pos` for `context`, from the
    /// first provider that returns one.
    pub fn get_capability(&self, world: &Arc<World>, pos: &BlockPos, context: C) -> Option<Arc<T>> {
        self.get_capability_with(world, pos, None, context)
    }

    /// `getCapability` with the block entity that the caller already has.
    pub fn get_capability_with(
        &self,
        world: &Arc<World>,
        pos: &BlockPos,
        block_entity: Option<&Arc<dyn BlockEntity>>,
        context: C,
    ) -> Option<Arc<T>> {
        let hint = block_entity.map_or(EntityHint::Unknown, |block_entity| {
            EntityHint::Known(Some(block_entity))
        });
        self.lookup(world, pos, None, hint, context)
    }

    /// `getCapability` with what the caller already read of the position.
    pub(crate) fn lookup(
        self,
        world: &Arc<World>,
        pos: &BlockPos,
        state: Option<(&'static Block, &'static BlockState)>,
        entity: EntityHint<'_>,
        context: C,
    ) -> Option<Arc<T>> {
        let data = self.data;
        if data.blocks.is_empty() && data.block_entities.is_empty() {
            return None;
        }
        if matches!(entity, EntityHint::Known(None)) && !data.answers_without_block_entity() {
            return None;
        }
        let (block, state) = state.unwrap_or_else(|| world.get_block_and_state(pos));
        let read;
        // `BlockCapability.getCapability` reads the block entity only for a block with one.
        let block_entity = match entity {
            EntityHint::Known(block_entity) => block_entity,
            EntityHint::Unknown
                if state.block_entity_type == u16::MAX || data.block_entities.is_empty() =>
            {
                None
            }
            EntityHint::Unknown => {
                read = world.get_block_entity(pos);
                read.as_ref()
            }
        };
        data.blocks.with(block.name, |block_providers| {
            let entity_type =
                block_entity.map_or("", |block_entity| block_entity.resource_location());
            data.block_entities.with(entity_type, |entity_providers| {
                let (mut blocks, mut entities) = (
                    block_providers.iter().peekable(),
                    entity_providers.iter().peekable(),
                );
                loop {
                    let next_block = blocks.peek().map(|ordered| ordered.seq);
                    let next_entity = entities.peek().map(|ordered| ordered.seq);
                    let handler = match (next_block, next_entity) {
                        (None, None) => return None,
                        (Some(block_seq), Some(entity_seq)) if entity_seq < block_seq => {
                            entities.next().and_then(|ordered| {
                                block_entity.and_then(|be| (ordered.provider)(be, context.clone()))
                            })
                        }
                        (None, Some(_)) => entities.next().and_then(|ordered| {
                            block_entity.and_then(|be| (ordered.provider)(be, context.clone()))
                        }),
                        _ => blocks.next().and_then(|ordered| {
                            (ordered.provider)(world, pos, state, block_entity, context.clone())
                        }),
                    };
                    if handler.is_some() {
                        return handler;
                    }
                }
            })
        })
    }
}

impl<T: ?Sized + Send + Sync + 'static> BlockCapability<T, Option<BlockDirection>> {
    /// `BlockCapability.createSided`: a capability with the side as its context.
    ///
    /// # Errors
    ///
    /// [`CapabilityError::TypeMismatch`] when `name` exists with other types.
    pub fn create_sided(name: &str) -> Result<Self, CapabilityError> {
        Self::create(name)
    }
}

impl<T: ?Sized + Send + Sync + 'static> BlockCapability<T, ()> {
    /// `BlockCapability.createVoid`: a capability without a context.
    ///
    /// # Errors
    ///
    /// [`CapabilityError::TypeMismatch`] when `name` exists with other types.
    pub fn create_void(name: &str) -> Result<Self, CapabilityError> {
        Self::create(name)
    }
}

/// `BlockCapability.getAllProxyable`: the names of the proxyable block capabilities. Complete
/// only after the `High` phase of the `RegisterCapabilitiesEvent`, where mods mark them.
#[must_use]
pub fn get_all_proxyable() -> Vec<String> {
    let mut names: Vec<String> = STATE
        .registry
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|((kind, _), entry)| {
            *kind == Kind::Block
                && entry.proxyable.is_some_and(|proxyable| {
                    *proxyable.lock().unwrap_or_else(PoisonError::into_inner) == Some(true)
                })
        })
        .map(|((_, name), _)| name.clone())
        .collect();
    names.sort();
    names
}

/// `EntityCapability<T, C>`: a capability of entities.
pub struct EntityCapability<T: ?Sized + 'static, C: 'static> {
    data: &'static EntityData<T, C>,
}

impl<T: ?Sized + 'static, C: 'static> Clone for EntityCapability<T, C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized + 'static, C: 'static> Copy for EntityCapability<T, C> {}

impl<T: ?Sized + 'static, C: 'static> fmt::Debug for EntityCapability<T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EntityCapability({})", self.data.name)
    }
}

impl<T: ?Sized + Send + Sync + 'static, C: Clone + Send + Sync + 'static> EntityCapability<T, C> {
    /// `EntityCapability.create`.
    ///
    /// # Errors
    ///
    /// [`CapabilityError::TypeMismatch`] when `name` exists with other types.
    pub fn create(name: &str) -> Result<Self, CapabilityError> {
        intern::<_, T, C>(&STATE.registry, name).map(|data| Self { data })
    }

    /// `name`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.data.name
    }

    /// `getCapability` (`Entity.getCapability`).
    pub fn get_capability(&self, entity: &Arc<dyn EntityBase>, context: C) -> Option<Arc<T>> {
        self.data
            .providers
            .with(entity.get_entity().entity_type.resource_name, |providers| {
                providers
                    .iter()
                    .find_map(|provider| provider(entity, context.clone()))
            })
    }
}

impl<T: ?Sized + Send + Sync + 'static> EntityCapability<T, Option<BlockDirection>> {
    /// `EntityCapability.createSided`.
    ///
    /// # Errors
    ///
    /// [`CapabilityError::TypeMismatch`] when `name` exists with other types.
    pub fn create_sided(name: &str) -> Result<Self, CapabilityError> {
        Self::create(name)
    }
}

impl<T: ?Sized + Send + Sync + 'static> EntityCapability<T, ()> {
    /// `EntityCapability.createVoid`.
    ///
    /// # Errors
    ///
    /// [`CapabilityError::TypeMismatch`] when `name` exists with other types.
    pub fn create_void(name: &str) -> Result<Self, CapabilityError> {
        Self::create(name)
    }
}

/// `ItemCapability<T, C>`: a capability of item stacks, usually with an [`ItemAccess`]
/// context.
pub struct ItemCapability<T: ?Sized + 'static, C: 'static> {
    data: &'static ItemData<T, C>,
}

impl<T: ?Sized + 'static, C: 'static> Clone for ItemCapability<T, C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized + 'static, C: 'static> Copy for ItemCapability<T, C> {}

impl<T: ?Sized + 'static, C: 'static> fmt::Debug for ItemCapability<T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ItemCapability({})", self.data.name)
    }
}

impl<T: ?Sized + Send + Sync + 'static, C: Clone + Send + Sync + 'static> ItemCapability<T, C> {
    /// `ItemCapability.create`.
    ///
    /// # Errors
    ///
    /// [`CapabilityError::TypeMismatch`] when `name` exists with other types.
    pub fn create(name: &str) -> Result<Self, CapabilityError> {
        intern::<_, T, C>(&STATE.registry, name).map(|data| Self { data })
    }

    /// `name`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.data.name
    }

    /// `getCapability` (`ItemStack.getCapability`). An empty stack has no capability.
    pub fn get_capability(&self, stack: &ItemStack, context: C) -> Option<Arc<T>> {
        if stack.is_empty() {
            return None;
        }
        self.data
            .providers
            .with(stack.item.registry_key, |providers| {
                providers
                    .iter()
                    .find_map(|provider| provider(stack, context.clone()))
            })
    }
}

impl<T: ?Sized + Send + Sync + 'static> ItemCapability<T, Arc<dyn ItemAccess>> {
    /// `ItemAccess.getCapability`: the capability of the accessed stack, with `access` as the
    /// context.
    pub fn get_capability_from(&self, access: &Arc<dyn ItemAccess>) -> Option<Arc<T>> {
        let count = u8::try_from(access.get_amount()).unwrap_or(u8::MAX);
        self.get_capability(&access.get_resource().to_stack(count), access.clone())
    }
}

impl<T: ?Sized + Send + Sync + 'static> ItemCapability<T, ()> {
    /// `ItemCapability.createVoid`.
    ///
    /// # Errors
    ///
    /// [`CapabilityError::TypeMismatch`] when `name` exists with other types.
    pub fn create_void(name: &str) -> Result<Self, CapabilityError> {
        Self::create(name)
    }
}

/// Built-in capabilities that `NeoForge` names in `Capabilities`.
mod builtins {
    use super::{
        Arc, BlockCapability, BlockData, BlockDirection, EnergyHandler, EntityCapability,
        EntityData, Entries, FluidResource, ItemAccess, ItemCapability, ItemData, ItemResource,
        PoisonError, Registry, ResourceHandler, insert,
    };

    pub(super) type ItemHandler = dyn ResourceHandler<ItemResource>;
    pub(super) type FluidHandler = dyn ResourceHandler<FluidResource>;
    pub(super) type Side = Option<BlockDirection>;
    pub(super) type Access = Arc<dyn ItemAccess>;

    pub(super) struct Builtins {
        pub(super) item_block: BlockCapability<ItemHandler, Side>,
        pub(super) item_entity: EntityCapability<ItemHandler, ()>,
        pub(super) item_entity_automation: EntityCapability<ItemHandler, Side>,
        pub(super) item_item: ItemCapability<ItemHandler, Access>,
        pub(super) fluid_block: BlockCapability<FluidHandler, Side>,
        pub(super) fluid_entity: EntityCapability<FluidHandler, Side>,
        pub(super) fluid_item: ItemCapability<FluidHandler, Access>,
        pub(super) energy_block: BlockCapability<dyn EnergyHandler, Side>,
        pub(super) energy_entity: EntityCapability<dyn EnergyHandler, Side>,
        pub(super) energy_item: ItemCapability<dyn EnergyHandler, Access>,
    }

    fn block<T: ?Sized + Send + Sync + 'static, C: Send + Sync + 'static>(
        entries: &mut Entries,
        name: &str,
    ) -> BlockCapability<T, C> {
        BlockCapability {
            data: insert::<BlockData<T, C>, T, C>(entries, name.to_owned()),
        }
    }

    fn entity<T: ?Sized + Send + Sync + 'static, C: Send + Sync + 'static>(
        entries: &mut Entries,
        name: &str,
    ) -> EntityCapability<T, C> {
        EntityCapability {
            data: insert::<EntityData<T, C>, T, C>(entries, name.to_owned()),
        }
    }

    fn item<T: ?Sized + Send + Sync + 'static, C: Send + Sync + 'static>(
        entries: &mut Entries,
        name: &str,
    ) -> ItemCapability<T, C> {
        ItemCapability {
            data: insert::<ItemData<T, C>, T, C>(entries, name.to_owned()),
        }
    }

    impl Builtins {
        /// Fills the empty `registry`.
        pub(super) fn new(registry: &Registry) -> Self {
            let mut entries = registry.lock().unwrap_or_else(PoisonError::into_inner);
            Self {
                item_block: block(&mut entries, "neoforge:item_handler"),
                item_entity: entity(&mut entries, "neoforge:item_handler"),
                item_entity_automation: entity(&mut entries, "neoforge:item_handler_automation"),
                item_item: item(&mut entries, "neoforge:item_handler"),
                fluid_block: block(&mut entries, "neoforge:fluid_handler"),
                fluid_entity: entity(&mut entries, "neoforge:fluid_handler"),
                fluid_item: item(&mut entries, "neoforge:fluid_handler"),
                energy_block: block(&mut entries, "neoforge:energy_handler"),
                energy_entity: entity(&mut entries, "neoforge:energy_handler"),
                energy_item: item(&mut entries, "neoforge:energy_handler"),
            }
        }
    }
}

macro_rules! builtin {
    ($(#[$doc:meta])* $name:ident: $ty:ty = $field:ident) => {
        $(#[$doc])*
        pub static $name: LazyLock<$ty> = LazyLock::new(|| super::STATE.builtins.$field);
    };
}

/// `Capabilities.Item`: item handlers.
pub mod item {
    use super::{
        BlockCapability, EntityCapability, ItemCapability, LazyLock,
        builtins::{Access, ItemHandler, Side},
    };

    builtin!(
        /// `Capabilities.Item.BLOCK` (`neoforge:item_handler`).
        BLOCK: BlockCapability<ItemHandler, Side> = item_block
    );
    builtin!(
        /// `Capabilities.Item.ENTITY` (`neoforge:item_handler`): the inventory of an entity.
        ENTITY: EntityCapability<ItemHandler, ()> = item_entity
    );
    builtin!(
        /// `Capabilities.Item.ENTITY_AUTOMATION` (`neoforge:item_handler_automation`): what
        /// hoppers and droppers reach of an entity.
        ENTITY_AUTOMATION: EntityCapability<ItemHandler, Side> = item_entity_automation
    );
    builtin!(
        /// `Capabilities.Item.ITEM` (`neoforge:item_handler`): the contents of an item.
        ITEM: ItemCapability<ItemHandler, Access> = item_item
    );
}

/// `Capabilities.Fluid`: fluid handlers.
pub mod fluid {
    use super::{
        BlockCapability, EntityCapability, ItemCapability, LazyLock,
        builtins::{Access, FluidHandler, Side},
    };

    builtin!(
        /// `Capabilities.Fluid.BLOCK` (`neoforge:fluid_handler`).
        BLOCK: BlockCapability<FluidHandler, Side> = fluid_block
    );
    builtin!(
        /// `Capabilities.Fluid.ENTITY` (`neoforge:fluid_handler`).
        ENTITY: EntityCapability<FluidHandler, Side> = fluid_entity
    );
    builtin!(
        /// `Capabilities.Fluid.ITEM` (`neoforge:fluid_handler`).
        ITEM: ItemCapability<FluidHandler, Access> = fluid_item
    );
}

/// `Capabilities.Energy`: energy handlers.
pub mod energy {
    use super::{
        BlockCapability, EnergyHandler, EntityCapability, ItemCapability, LazyLock,
        builtins::{Access, Side},
    };

    builtin!(
        /// `Capabilities.Energy.BLOCK` (`neoforge:energy_handler`).
        BLOCK: BlockCapability<dyn EnergyHandler, Side> = energy_block
    );
    builtin!(
        /// `Capabilities.Energy.ENTITY` (`neoforge:energy_handler`).
        ENTITY: EntityCapability<dyn EnergyHandler, Side> = energy_entity
    );
    builtin!(
        /// `Capabilities.Energy.ITEM` (`neoforge:energy_handler`).
        ITEM: ItemCapability<dyn EnergyHandler, Access> = energy_item
    );
}

static CLOSED: AtomicBool = AtomicBool::new(false);

/// `RegisterCapabilitiesEvent`: registers capability providers.
///
/// The host passes it to
/// [`NativeMod::register_capabilities`](crate::NativeMod) once per [`phase`](Self::phase). A
/// failed call returns the error and keeps it, so the server does not start.
pub struct RegisterCapabilitiesEvent {
    phase: EventPriority,
    closed: &'static AtomicBool,
    error: Option<CapabilityError>,
}

impl fmt::Debug for RegisterCapabilitiesEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisterCapabilitiesEvent")
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

/// The phases in the order that `EventBus` runs them.
const PHASES: [EventPriority; 5] = [
    EventPriority::Highest,
    EventPriority::High,
    EventPriority::Normal,
    EventPriority::Low,
    EventPriority::Lowest,
];

impl RegisterCapabilitiesEvent {
    const fn new(phase: EventPriority, closed: &'static AtomicBool) -> Self {
        Self {
            phase,
            closed,
            error: None,
        }
    }

    /// The listener priority of this call. A mod registers most providers at
    /// `EventPriority::Normal`, a fallback at `Low` after checking `is_block_registered`, and
    /// marks capabilities proxyable at `High` or earlier.
    #[must_use]
    pub const fn phase(&self) -> &EventPriority {
        &self.phase
    }

    fn keep<R>(&mut self, result: Result<R, CapabilityError>) -> Result<R, CapabilityError> {
        if let Err(error) = &result {
            self.error.get_or_insert_with(|| error.clone());
        }
        result
    }

    fn open(&self) -> Result<(), CapabilityError> {
        if self.closed.load(Ordering::Acquire) {
            Err(CapabilityError::RegistrationClosed)
        } else {
            Ok(())
        }
    }

    /// `registerBlock`: `provider` answers lookups of `capability` on `blocks`, such as
    /// `"mymod:tank"`.
    ///
    /// # Errors
    ///
    /// [`CapabilityError::NoTargets`], [`CapabilityError::Unknown`] for an unknown block and
    /// [`CapabilityError::RegistrationClosed`].
    pub fn register_block<T, C>(
        &mut self,
        capability: &BlockCapability<T, C>,
        provider: impl Fn(
            &Arc<World>,
            &BlockPos,
            &'static BlockState,
            Option<&Arc<dyn BlockEntity>>,
            C,
        ) -> Option<Arc<T>>
        + Send
        + Sync
        + 'static,
        blocks: &[&str],
    ) -> Result<(), CapabilityError>
    where
        T: ?Sized + Send + Sync + 'static,
        C: Clone + Send + Sync + 'static,
    {
        let result = self.open().and_then(|()| {
            if blocks.is_empty() {
                return Err(CapabilityError::NoTargets);
            }
            let names = blocks
                .iter()
                .map(|block| registration::block(block).map(|(name, _)| name))
                .collect::<Result<Vec<_>, _>>()?;
            let provider: Arc<BlockProvider<T, C>> = Arc::new(provider);
            for name in names {
                capability
                    .data
                    .blocks
                    .push(&name, ordered(provider.clone()));
            }
            Ok(())
        });
        self.keep(result)
    }

    /// `registerBlockEntity`: `provider` answers lookups of `capability` on blocks whose block
    /// entity has the type `block_entity_type`, such as `"minecraft:barrel"`.
    ///
    /// # Errors
    ///
    /// [`CapabilityError::UnknownBlockEntityType`] and
    /// [`CapabilityError::RegistrationClosed`].
    pub fn register_block_entity<T, C>(
        &mut self,
        capability: &BlockCapability<T, C>,
        block_entity_type: &str,
        provider: impl Fn(&Arc<dyn BlockEntity>, C) -> Option<Arc<T>> + Send + Sync + 'static,
    ) -> Result<(), CapabilityError>
    where
        T: ?Sized + Send + Sync + 'static,
        C: Clone + Send + Sync + 'static,
    {
        let result = self.open().and_then(|()| {
            let name = registration::namespaced(block_entity_type);
            let known = name
                .strip_prefix("minecraft:")
                .is_some_and(|path| BLOCK_ENTITY_TYPES.contains(&path));
            if !known {
                return Err(CapabilityError::UnknownBlockEntityType(name));
            }
            let provider: Arc<BlockEntityProvider<T, C>> = Arc::new(provider);
            capability
                .data
                .block_entities
                .push(&name, ordered(provider));
            Ok(())
        });
        self.keep(result)
    }

    /// `isBlockRegistered`: whether `capability` has a provider for `block`, directly or
    /// through its vanilla block entity type.
    #[must_use]
    pub fn is_block_registered<T, C>(&self, capability: &BlockCapability<T, C>, block: &str) -> bool
    where
        T: ?Sized + Send + Sync + 'static,
        C: Clone + Send + Sync + 'static,
    {
        let Ok((name, vanilla)) = registration::block(block) else {
            return false;
        };
        if capability.data.blocks.contains(&name) {
            return true;
        }
        vanilla
            .and_then(|block| {
                BLOCK_ENTITY_TYPES.get(usize::from(block.default_state.block_entity_type))
            })
            .is_some_and(|path| capability.data.block_entities.contains(path))
    }

    fn set_proxyable_state<T: ?Sized, C>(
        &mut self,
        capability: BlockCapability<T, C>,
        proxyable: bool,
    ) -> Result<(), CapabilityError> {
        let result = self.open().and_then(|()| {
            let mut state = capability
                .data
                .proxyable
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            match *state {
                Some(current) if current != proxyable => Err(CapabilityError::ProxyableConflict {
                    name: capability.data.name.clone(),
                    proxyable: current,
                }),
                _ => {
                    *state = Some(proxyable);
                    Ok(())
                }
            }
        });
        self.keep(result)
    }

    /// `setProxyable`.
    ///
    /// # Errors
    ///
    /// [`CapabilityError::ProxyableConflict`] after [`set_non_proxyable`](Self::set_non_proxyable)
    /// and [`CapabilityError::RegistrationClosed`].
    pub fn set_proxyable<T: ?Sized, C>(
        &mut self,
        capability: &BlockCapability<T, C>,
    ) -> Result<(), CapabilityError> {
        self.set_proxyable_state(*capability, true)
    }

    /// `setNonProxyable`.
    ///
    /// # Errors
    ///
    /// [`CapabilityError::ProxyableConflict`] after [`set_proxyable`](Self::set_proxyable) and
    /// [`CapabilityError::RegistrationClosed`].
    pub fn set_non_proxyable<T: ?Sized, C>(
        &mut self,
        capability: &BlockCapability<T, C>,
    ) -> Result<(), CapabilityError> {
        self.set_proxyable_state(*capability, false)
    }

    /// `registerEntity`: `provider` answers lookups of `capability` on entities of
    /// `entity_type`.
    ///
    /// # Errors
    ///
    /// [`CapabilityError::Unknown`] for an unknown entity type and
    /// [`CapabilityError::RegistrationClosed`].
    pub fn register_entity<T, C>(
        &mut self,
        capability: &EntityCapability<T, C>,
        entity_type: &str,
        provider: impl Fn(&Arc<dyn EntityBase>, C) -> Option<Arc<T>> + Send + Sync + 'static,
    ) -> Result<(), CapabilityError>
    where
        T: ?Sized + Send + Sync + 'static,
        C: Clone + Send + Sync + 'static,
    {
        let result = self.open().and_then(|()| {
            let (name, _) = registration::entity_type(entity_type)?;
            let provider: Arc<EntityProvider<T, C>> = Arc::new(provider);
            capability.data.providers.push(&name, provider);
            Ok(())
        });
        self.keep(result)
    }

    /// `isEntityRegistered`.
    #[must_use]
    pub fn is_entity_registered<T, C>(
        &self,
        capability: &EntityCapability<T, C>,
        entity_type: &str,
    ) -> bool
    where
        T: ?Sized + Send + Sync + 'static,
        C: Clone + Send + Sync + 'static,
    {
        capability.data.providers.contains(entity_type)
    }

    /// `registerItem`: `provider` answers lookups of `capability` on stacks of `items`.
    ///
    /// # Errors
    ///
    /// [`CapabilityError::NoTargets`], [`CapabilityError::Unknown`] for an unknown item and
    /// [`CapabilityError::RegistrationClosed`].
    pub fn register_item<T, C>(
        &mut self,
        capability: &ItemCapability<T, C>,
        provider: impl Fn(&ItemStack, C) -> Option<Arc<T>> + Send + Sync + 'static,
        items: &[&str],
    ) -> Result<(), CapabilityError>
    where
        T: ?Sized + Send + Sync + 'static,
        C: Clone + Send + Sync + 'static,
    {
        let result = self.open().and_then(|()| {
            if items.is_empty() {
                return Err(CapabilityError::NoTargets);
            }
            let names = items
                .iter()
                .map(|item| item_name(item))
                .collect::<Result<Vec<_>, _>>()?;
            let provider: Arc<ItemProvider<T, C>> = Arc::new(provider);
            for name in names {
                capability.data.providers.push(&name, provider.clone());
            }
            Ok(())
        });
        self.keep(result)
    }

    /// `isItemRegistered`.
    #[must_use]
    pub fn is_item_registered<T, C>(&self, capability: &ItemCapability<T, C>, item: &str) -> bool
    where
        T: ?Sized + Send + Sync + 'static,
        C: Clone + Send + Sync + 'static,
    {
        capability.data.providers.contains(item)
    }
}

fn item_name(item: &str) -> Result<String, CapabilityError> {
    let name = registration::namespaced(item);
    let vanilla = name
        .strip_prefix("minecraft:")
        .and_then(Item::from_registry_key)
        .is_some();
    if vanilla || dynamic::is_registered(ContentKind::Item, &name) {
        Ok(name)
    } else {
        Err(CapabilityError::Unknown(RegistrationError::UnknownItem(
            name,
        )))
    }
}

/// Posts the `RegisterCapabilitiesEvent` phase by phase to the host and every mod in load
/// order, then closes the registration and freezes the providers. Returns the first error after
/// the last phase.
pub(crate) fn post(mods: &[LoadedMod<'_>]) -> Result<(), String> {
    static POSTING: Mutex<()> = Mutex::new(());
    let _posting = POSTING.lock().unwrap_or_else(PoisonError::into_inner);
    vanilla::install_hooks();
    let result = post_with(mods, &CLOSED, vanilla::claim_host());
    let entries = STATE
        .registry
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    for entry in entries.values() {
        entry.freeze.freeze();
    }
    result
}

/// Runs the `RegisterCapabilitiesEvent` with no mod, as a server with no capability provider
/// does: the engine hooks, the vanilla providers and the freeze. For the benches.
#[doc(hidden)]
pub fn install_for_bench() {
    let _ = post(&[]);
}

/// [`post`] without the host, the engine hooks and the freeze, for the lifecycle tests.
#[cfg(test)]
pub(crate) fn post_unfrozen(mods: &[LoadedMod<'_>]) -> Result<(), String> {
    post_with(mods, Box::leak(Box::new(AtomicBool::new(false))), false)
}

/// The phases of [`post`] with the registration flag `closed`, and with the host part when
/// `host` is set.
fn post_with(
    mods: &[LoadedMod<'_>],
    closed: &'static AtomicBool,
    host: bool,
) -> Result<(), String> {
    let mut first_error = None;
    for phase in PHASES {
        if host {
            let mut event = RegisterCapabilitiesEvent::new(phase.clone(), closed);
            vanilla::register(&mut event);
            if let Some(error) = event.error {
                first_error.get_or_insert_with(|| {
                    format!("the RegisterCapabilitiesEvent of the host: {error}")
                });
            }
        }
        for loaded in mods {
            let mut event = RegisterCapabilitiesEvent::new(phase.clone(), closed);
            loaded.native_mod.register_capabilities(&mut event);
            if let Some(error) = event.error {
                first_error.get_or_insert_with(|| {
                    format!(
                        "the RegisterCapabilitiesEvent of mod {}: {error}",
                        loaded.mod_id
                    )
                });
            }
        }
    }
    closed.store(true, Ordering::Release);
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests;
