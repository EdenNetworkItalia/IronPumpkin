//! Registry data maps: values that data packs attach to the entries of a registry.
//!
//! `NeoForge`: `net.neoforged.neoforge.registries.datamaps`. A mod builds a [`DataMapType`] (or an
//! [`AdvancedDataMapType`] with a merger, a remover or no tag support) for one registry and
//! registers it in [`NativeMod::register_data_map_types`](crate::NativeMod). Data packs then give
//! the values in `data/<namespace>/data_maps/<registry>/<path>.json`, where the map id is
//! `<namespace>:<path>` and `<registry>` is the registry path, prefixed with its namespace outside
//! `minecraft` (`item`, `worldgen/biome`, `mymod/widget`). The server reads the files of every
//! enabled data pack at startup and on `/reload`, then fires
//! [`DataMapsUpdatedEvent`] for every registry that had a data map file.
//! [`DataMapType::get_data`] reads the value of an entry.
//!
//! A file has the form of `DataMapFile`:
//!
//! ```json
//! {
//!   "replace": false,
//!   "values": {
//!     "minecraft:stick": { "burn_time": 100 },
//!     "#minecraft:planks": { "value": { "burn_time": 300 }, "replace": true }
//!   },
//!   "remove": ["minecraft:bamboo"]
//! }
//! ```
//!
//! The value type `T` is the codec of the map: it implements the `Encode` and `Decode` traits of
//! `pumpkin_codecs`. `Decode` reads the values of the files. `Encode` writes the value that a
//! synced map sends to a `NeoForge` client, so it writes the form of the network codec of the
//! map. A vanilla client gets nothing.
//!
//! ```
//! use ironpumpkin_mods::{
//!     ModInit, NativeMod,
//!     data_maps::{DataMapType, RegisterDataMapTypesEvent},
//!     identifier::Identifier,
//! };
//! use std::sync::LazyLock;
//!
//! static BURN_TIME: LazyLock<DataMapType<i32>> = LazyLock::new(|| {
//!     DataMapType::builder(
//!         Identifier::parse_static("examplemod:burn_time"),
//!         Identifier::vanilla_static("item"),
//!     )
//!     .synced(false)
//!     .build()
//! });
//!
//! struct FuelMod;
//!
//! impl NativeMod for FuelMod {
//!     fn id(&self) -> &'static str {
//!         "examplemod"
//!     }
//!
//!     fn display_name(&self) -> &'static str {
//!         "Example Mod"
//!     }
//!
//!     fn version(&self) -> &'static str {
//!         "1.0.0"
//!     }
//!
//!     fn init(&self, _cx: &mut ModInit) {}
//!
//!     fn register_data_map_types(&self, event: &mut RegisterDataMapTypesEvent) {
//!         let _ = event.register(&*BURN_TIME);
//!     }
//! }
//!
//! fn burn_time(item: &Identifier) -> i32 {
//!     BURN_TIME.get_data(item).map_or(0, |time| *time)
//! }
//! # let _ = burn_time;
//! ```

pub mod builtin;
mod loader;
mod registries;
#[cfg(test)]
mod review_tests;
mod sync;
#[cfg(test)]
mod tests;

use arc_swap::ArcSwap;
use std::{
    any::Any,
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    hash::Hash,
    marker::PhantomData,
    ops::Deref,
    sync::{Arc, PoisonError, RwLock},
};

use pumpkin_codecs::{DataResult, Decode, Encode, json_ops::JsonOps};
pub use pumpkin_core::plugin::data_maps::{DataMapsUpdatedEvent, UpdateCause};
use pumpkin_util::identifier::Identifier;

use crate::LoadedMod;

/// A value type of a data map: its codec. One value that targets a tag is shared by the entries
/// of the tag, so a merger gets a clone.
pub trait DataMapValue: Encode + Decode + Clone + Send + Sync + 'static {}

impl<T: Encode + Decode + Clone + Send + Sync + 'static> DataMapValue for T {}

/// The target of a value or a removal in a data map file: a tag or one entry.
///
/// `NeoForge`: `Either<TagKey<R>, ResourceKey<R>>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DataMapSource {
    /// `#namespace:path`.
    Tag(Identifier),
    /// `namespace:path`.
    Entry(Identifier),
}

impl fmt::Display for DataMapSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tag(tag) => write!(f, "#{tag}"),
            Self::Entry(entry) => entry.fmt(f),
        }
    }
}

/// Merges two values that data packs attach to the same entry without `replace`.
///
/// `NeoForge`: `DataMapValueMerger`. `first` is the value that the entry has, from
/// `first_source`; `second` is the new value, from `second_source`.
pub trait DataMapValueMerger<T>: Send + Sync + 'static {
    fn merge(
        &self,
        registry: &Identifier,
        first_source: &DataMapSource,
        first: &T,
        second_source: &DataMapSource,
        second: T,
    ) -> T;
}

/// `DataMapValueMerger.defaultMerger`: the new value wins.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultMerger;

impl<T> DataMapValueMerger<T> for DefaultMerger {
    fn merge(&self, _: &Identifier, _: &DataMapSource, _: &T, _: &DataMapSource, second: T) -> T {
        second
    }
}

/// `DataMapValueMerger.listMerger`: the values of the first list, then those of the second.
#[derive(Debug, Clone, Copy, Default)]
pub struct ListMerger;

impl<E: Clone + Send + Sync + 'static> DataMapValueMerger<Vec<E>> for ListMerger {
    fn merge(
        &self,
        _: &Identifier,
        _: &DataMapSource,
        first: &Vec<E>,
        _: &DataMapSource,
        second: Vec<E>,
    ) -> Vec<E> {
        let mut merged = first.clone();
        merged.extend(second);
        merged
    }
}

/// `DataMapValueMerger.setMerger`: the union of both sets.
#[derive(Debug, Clone, Copy, Default)]
pub struct SetMerger;

impl<E: Clone + Eq + Hash + Send + Sync + 'static> DataMapValueMerger<HashSet<E>> for SetMerger {
    fn merge(
        &self,
        _: &Identifier,
        _: &DataMapSource,
        first: &HashSet<E>,
        _: &DataMapSource,
        second: HashSet<E>,
    ) -> HashSet<E> {
        let mut merged = first.clone();
        merged.extend(second);
        merged
    }
}

/// `DataMapValueMerger.mapMerger`: the entries of both maps; a key of the second map wins.
#[derive(Debug, Clone, Copy, Default)]
pub struct MapMerger;

impl<K, V> DataMapValueMerger<HashMap<K, V>> for MapMerger
where
    K: Clone + Eq + Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn merge(
        &self,
        _: &Identifier,
        _: &DataMapSource,
        first: &HashMap<K, V>,
        _: &DataMapSource,
        second: HashMap<K, V>,
    ) -> HashMap<K, V> {
        let mut merged = first.clone();
        merged.extend(second);
        merged
    }
}

impl<K, V> DataMapValueMerger<BTreeMap<K, V>> for MapMerger
where
    K: Clone + Ord + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn merge(
        &self,
        _: &Identifier,
        _: &DataMapSource,
        first: &BTreeMap<K, V>,
        _: &DataMapSource,
        second: BTreeMap<K, V>,
    ) -> BTreeMap<K, V> {
        let mut merged = first.clone();
        merged.extend(second);
        merged
    }
}

/// `DataMapValueMerger.wrappedListMerger`, `wrappedSetMerger` and `wrappedMapMerger`: merges
/// the collection that a value wraps with `merger`, then wraps the result.
pub struct WrappedMerger<T, C, M> {
    unwrap: fn(&T) -> &C,
    wrap: fn(C) -> T,
    merger: M,
}

impl<T, C, M> WrappedMerger<T, C, M> {
    #[must_use]
    pub const fn new(unwrap: fn(&T) -> &C, wrap: fn(C) -> T, merger: M) -> Self {
        Self {
            unwrap,
            wrap,
            merger,
        }
    }
}

impl<T, C, M> DataMapValueMerger<T> for WrappedMerger<T, C, M>
where
    T: Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
    M: DataMapValueMerger<C>,
{
    fn merge(
        &self,
        registry: &Identifier,
        first_source: &DataMapSource,
        first: &T,
        second_source: &DataMapSource,
        second: T,
    ) -> T {
        let merged = self.merger.merge(
            registry,
            first_source,
            (self.unwrap)(first),
            second_source,
            (self.unwrap)(&second).clone(),
        );
        (self.wrap)(merged)
    }
}

/// Removes a value, or a part of it, from an entry.
///
/// `NeoForge`: `DataMapValueRemover`. `None` removes the whole value; `Some` keeps the returned
/// value. The remover type is the codec of the objects in the `remove` map of a file.
pub trait DataMapValueRemover<T>: Send + Sync + 'static {
    fn remove(
        &self,
        value: &T,
        registry: &Identifier,
        source: &DataMapSource,
        entry: &Identifier,
    ) -> Option<T>;
}

/// `DataMapValueRemover.Default`: removes the whole value. Its codec reads any map, as
/// `MapCodec.unitCodec` used as a `Codec`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DefaultRemover;

impl<T> DataMapValueRemover<T> for DefaultRemover {
    fn remove(&self, _: &T, _: &Identifier, _: &DataMapSource, _: &Identifier) -> Option<T> {
        None
    }
}

impl Encode for DefaultRemover {
    fn encode<O: pumpkin_codecs::DynamicOps>(
        &self,
        ops: &'static O,
        _prefix: O::Value,
    ) -> DataResult<O::Value> {
        DataResult::new_success(ops.empty_map())
    }
}

impl Decode for DefaultRemover {
    fn decode<O: pumpkin_codecs::DynamicOps>(
        input: O::Value,
        ops: &'static O,
    ) -> DataResult<(Self, O::Value)> {
        ops.get_map(&input).map(|_| (Self, ops.empty()))
    }
}

/// A value in the store, erased to its type: `Arc<T>`.
type Stored = Arc<dyn Any + Send + Sync>;

/// The values of one data map, per entry.
type Values = HashMap<Identifier, Stored>;

/// A remover of a file, erased to the value type.
trait ErasedRemover {
    fn remove(
        &self,
        value: &Stored,
        registry: &Identifier,
        source: &DataMapSource,
        entry: &Identifier,
    ) -> Option<Stored>;
}

/// The operations of a data map type on erased values.
trait ErasedOps: Send + Sync {
    fn decode(&self, json: serde_json::Value) -> Result<Stored, String>;
    fn encode(&self, value: &Stored) -> Result<serde_json::Value, String>;
    fn merge(
        &self,
        registry: &Identifier,
        first_source: &DataMapSource,
        first: &Stored,
        second_source: &DataMapSource,
        second: Stored,
    ) -> Stored;
    fn decode_remover(&self, json: serde_json::Value) -> Result<Box<dyn ErasedRemover>, String>;
}

fn data_result<R>(result: DataResult<R>) -> Result<R, String> {
    match result {
        DataResult::Success { result, .. } => Ok(result),
        DataResult::Error { message, .. } => Err(message),
    }
}

/// A store holds only values of its own type, so the cast fails only on a foreign value.
fn downcast<T: Send + Sync + 'static>(value: &Stored) -> Option<&T> {
    value.downcast_ref::<T>()
}

struct TypedOps<T, R> {
    merger: Box<dyn DataMapValueMerger<T>>,
    _remover: PhantomData<fn() -> R>,
}

struct TypedRemover<T, R> {
    remover: R,
    _value: PhantomData<fn() -> T>,
}

impl<T: DataMapValue, R: DataMapValueRemover<T>> ErasedRemover for TypedRemover<T, R> {
    fn remove(
        &self,
        value: &Stored,
        registry: &Identifier,
        source: &DataMapSource,
        entry: &Identifier,
    ) -> Option<Stored> {
        let Some(typed) = downcast::<T>(value) else {
            return Some(value.clone());
        };
        self.remover
            .remove(typed, registry, source, entry)
            .map(|kept| Arc::new(kept) as Stored)
    }
}

impl<T, R> ErasedOps for TypedOps<T, R>
where
    T: DataMapValue,
    R: DataMapValueRemover<T> + Decode,
{
    fn decode(&self, json: serde_json::Value) -> Result<Stored, String> {
        data_result(T::parse(json, &JsonOps)).map(|value| Arc::new(value) as Stored)
    }

    fn encode(&self, value: &Stored) -> Result<serde_json::Value, String> {
        let value = downcast::<T>(value).ok_or("the value has another type")?;
        data_result(value.encode_start(&JsonOps))
    }

    fn merge(
        &self,
        registry: &Identifier,
        first_source: &DataMapSource,
        first: &Stored,
        second_source: &DataMapSource,
        second: Stored,
    ) -> Stored {
        let second = match second.downcast::<T>() {
            Ok(second) => second,
            Err(foreign) => return foreign,
        };
        let Some(first) = downcast::<T>(first) else {
            return second;
        };
        Arc::new(self.merger.merge(
            registry,
            first_source,
            first,
            second_source,
            Arc::unwrap_or_clone(second),
        ))
    }

    fn decode_remover(&self, json: serde_json::Value) -> Result<Box<dyn ErasedRemover>, String> {
        data_result(R::parse(json, &JsonOps)).map(|remover| {
            Box::new(TypedRemover::<T, R> {
                remover,
                _value: PhantomData,
            }) as Box<dyn ErasedRemover>
        })
    }
}

/// What a data map type is, shared by its handles and the registered types.
#[doc(hidden)]
pub struct TypeInfo {
    id: Identifier,
    registry: Identifier,
    /// `Some(mandatory)` when the map is synced.
    sync: Option<bool>,
    /// `AdvancedDataMapType`: the `remove` list of a file also takes remover objects.
    advanced: bool,
    no_tags_reason: Option<String>,
    ops: Box<dyn ErasedOps>,
    values: ArcSwap<Values>,
}

impl TypeInfo {
    fn values(&self) -> Arc<Values> {
        self.values.load_full()
    }

    fn set_values(&self, values: Values) {
        self.values.store(Arc::new(values));
    }
}

/// A data map type: values of type `T` attached to the entries of one registry.
///
/// `NeoForge`: `DataMapType<R, T>`, built with [`DataMapType::builder`]. A handle is cheap to
/// clone; every clone reads the same values.
pub struct DataMapType<T> {
    info: Arc<TypeInfo>,
    _value: PhantomData<fn() -> T>,
}

impl<T> Clone for DataMapType<T> {
    fn clone(&self) -> Self {
        Self {
            info: self.info.clone(),
            _value: PhantomData,
        }
    }
}

impl<T> fmt::Debug for DataMapType<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataMapType")
            .field("id", &self.info.id)
            .field("registry", &self.info.registry)
            .field("sync", &self.info.sync)
            .finish_non_exhaustive()
    }
}

impl<T: DataMapValue> DataMapType<T> {
    /// `DataMapType.builder(id, registry, codec)`: a type with id `id` for the registry
    /// `registry` (`minecraft:item`), not synced. The codec is `T`.
    #[must_use]
    pub const fn builder(id: Identifier, registry: Identifier) -> DataMapTypeBuilder<T> {
        DataMapTypeBuilder {
            id,
            registry,
            sync: None,
            _value: PhantomData,
        }
    }

    /// `DataMapType.getData` through `Registry.getData(type, key)`: the value of `entry`, or
    /// `None`.
    #[must_use]
    pub fn get_data(&self, entry: &Identifier) -> Option<Arc<T>> {
        self.info
            .values
            .load()
            .get(entry)
            .and_then(|value| value.clone().downcast::<T>().ok())
    }

    /// Runs `f` on the value of `entry`, or returns `None`. It takes no reference count, so a
    /// read on a hot path writes no shared memory.
    pub fn with_data<R>(&self, entry: &Identifier, f: impl FnOnce(&T) -> R) -> Option<R> {
        let values = self.info.values.load();
        values
            .get(entry)
            .and_then(|value| downcast::<T>(value))
            .map(f)
    }

    /// `Registry.getDataMap(type)`: every entry with a value, as loaded now. A later load does
    /// not change the view.
    #[must_use]
    pub fn data_map(&self) -> DataMapView<T> {
        DataMapView {
            values: self.info.values(),
            _value: PhantomData,
        }
    }
}

/// The values of a data map at one load: [`DataMapType::data_map`].
pub struct DataMapView<T> {
    values: Arc<Values>,
    _value: PhantomData<fn() -> T>,
}

impl<T: DataMapValue> DataMapView<T> {
    /// The value of `entry`, or `None`.
    #[must_use]
    pub fn get(&self, entry: &Identifier) -> Option<&T> {
        self.values
            .get(entry)
            .and_then(|value| downcast::<T>(value))
    }

    /// Every entry and its value, in no fixed order.
    pub fn iter(&self) -> impl Iterator<Item = (&Identifier, &T)> {
        self.values
            .iter()
            .filter_map(|(entry, value)| downcast::<T>(value).map(|value| (entry, value)))
    }

    /// The number of entries with a value.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether no entry has a value.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl<T> fmt::Debug for DataMapView<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataMapView")
            .field("len", &self.values.len())
            .finish_non_exhaustive()
    }
}

impl<T> DataMapType<T> {
    /// `id()`.
    #[must_use]
    pub fn id(&self) -> &Identifier {
        &self.info.id
    }

    /// `registryKey()`.
    #[must_use]
    pub fn registry_key(&self) -> &Identifier {
        &self.info.registry
    }

    /// Whether the map is synced: `networkCodec() != null`.
    #[must_use]
    pub fn is_synced(&self) -> bool {
        self.info.sync.is_some()
    }

    /// `mandatorySync()`: a client without the map cannot join.
    #[must_use]
    pub fn mandatory_sync(&self) -> bool {
        self.info.sync == Some(true)
    }
}

/// `DataMapType.Builder`.
pub struct DataMapTypeBuilder<T> {
    id: Identifier,
    registry: Identifier,
    sync: Option<bool>,
    _value: PhantomData<fn() -> T>,
}

impl<T: DataMapValue> DataMapTypeBuilder<T> {
    /// `synced(networkCodec, mandatory)`: the map is sent to `NeoForge` clients that know it, in
    /// the form that `T` encodes. A `mandatory` map refuses a client without it.
    #[must_use]
    pub const fn synced(mut self, mandatory: bool) -> Self {
        self.sync = Some(mandatory);
        self
    }

    /// `build()`.
    #[must_use]
    pub fn build(self) -> DataMapType<T> {
        build_type::<T, DefaultRemover>(
            self.id,
            self.registry,
            self.sync,
            false,
            None,
            Box::new(DefaultMerger),
        )
    }
}

fn build_type<T, R>(
    id: Identifier,
    registry: Identifier,
    sync: Option<bool>,
    advanced: bool,
    no_tags_reason: Option<String>,
    merger: Box<dyn DataMapValueMerger<T>>,
) -> DataMapType<T>
where
    T: DataMapValue,
    R: DataMapValueRemover<T> + Decode,
{
    DataMapType {
        info: Arc::new(TypeInfo {
            id,
            registry,
            sync,
            advanced,
            no_tags_reason,
            ops: Box::new(TypedOps::<T, R> {
                merger,
                _remover: PhantomData,
            }),
            values: ArcSwap::from_pointee(HashMap::new()),
        }),
        _value: PhantomData,
    }
}

/// A data map type with a merger, a remover type `R` and optional tag support.
///
/// `NeoForge`: `AdvancedDataMapType<R, T, VR>`, built with [`AdvancedDataMapType::builder`]. It
/// dereferences to its [`DataMapType`].
pub struct AdvancedDataMapType<T, R = DefaultRemover> {
    base: DataMapType<T>,
    _remover: PhantomData<fn() -> R>,
}

impl<T, R> Clone for AdvancedDataMapType<T, R> {
    fn clone(&self) -> Self {
        Self {
            base: self.base.clone(),
            _remover: PhantomData,
        }
    }
}

impl<T, R> fmt::Debug for AdvancedDataMapType<T, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AdvancedDataMapType")
            .field(&self.base)
            .finish()
    }
}

impl<T, R> Deref for AdvancedDataMapType<T, R> {
    type Target = DataMapType<T>;

    fn deref(&self) -> &DataMapType<T> {
        &self.base
    }
}

impl<T: DataMapValue> AdvancedDataMapType<T> {
    /// `AdvancedDataMapType.builder(id, registry, codec)`: the default merger (the new value
    /// wins), the default remover (removes the whole value) and tag support.
    #[must_use]
    pub fn builder(id: Identifier, registry: Identifier) -> AdvancedDataMapTypeBuilder<T> {
        AdvancedDataMapTypeBuilder {
            id,
            registry,
            sync: None,
            merger: Box::new(DefaultMerger),
            no_tags_reason: None,
            _remover: PhantomData,
        }
    }
}

impl<T, R> AdvancedDataMapType<T, R> {
    /// `noTagsReason()`: why the files of the map cannot target tags, or `None`.
    #[must_use]
    pub fn no_tags_reason(&self) -> Option<&str> {
        self.base.info.no_tags_reason.as_deref()
    }
}

/// `AdvancedDataMapType.Builder`.
pub struct AdvancedDataMapTypeBuilder<T, R = DefaultRemover> {
    id: Identifier,
    registry: Identifier,
    sync: Option<bool>,
    merger: Box<dyn DataMapValueMerger<T>>,
    no_tags_reason: Option<String>,
    _remover: PhantomData<fn() -> R>,
}

impl<T, R> AdvancedDataMapTypeBuilder<T, R>
where
    T: DataMapValue,
    R: DataMapValueRemover<T> + Decode,
{
    /// `synced(networkCodec, mandatory)`; see [`DataMapTypeBuilder::synced`].
    #[must_use]
    pub const fn synced(mut self, mandatory: bool) -> Self {
        self.sync = Some(mandatory);
        self
    }

    /// `merger(merger)`: merges two values for one entry when the newer has no `replace`.
    #[must_use]
    pub fn merger(mut self, merger: impl DataMapValueMerger<T>) -> Self {
        self.merger = Box::new(merger);
        self
    }

    /// `remover(codec)`: the type of the objects in the `remove` map of a file.
    #[must_use]
    pub fn remover<R2: DataMapValueRemover<T> + Decode>(self) -> AdvancedDataMapTypeBuilder<T, R2> {
        AdvancedDataMapTypeBuilder {
            id: self.id,
            registry: self.registry,
            sync: self.sync,
            merger: self.merger,
            no_tags_reason: self.no_tags_reason,
            _remover: PhantomData,
        }
    }

    /// `disableTagSupport(reason)`: the values of a file must target single entries. The
    /// loader drops the tag targets of a file and logs `reason`.
    ///
    /// # Panics
    ///
    /// When `reason` is blank, or tag support is disabled already, as `NeoForge` throws.
    #[must_use]
    pub fn disable_tag_support(mut self, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        assert!(
            !reason.trim().is_empty(),
            "Reason for disabling tag support must not be blank"
        );
        assert!(
            self.no_tags_reason.is_none(),
            "Tag support has already been disabled for this data map type"
        );
        self.no_tags_reason = Some(reason);
        self
    }

    /// `build()`.
    #[must_use]
    pub fn build(self) -> AdvancedDataMapType<T, R> {
        AdvancedDataMapType {
            base: build_type::<T, R>(
                self.id,
                self.registry,
                self.sync,
                true,
                self.no_tags_reason,
                self.merger,
            ),
            _remover: PhantomData,
        }
    }
}

/// Why a data map type could not be registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataMapError {
    /// A type with this id is registered for the registry already.
    Duplicate {
        id: Identifier,
        registry: Identifier,
    },
    /// The registry cannot carry this data map.
    UnsupportedRegistry {
        id: Identifier,
        registry: Identifier,
        reason: String,
    },
    /// The registration ran already.
    Closed(Identifier),
}

impl fmt::Display for DataMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate { id, registry } => write!(
                f,
                "Tried to register data map type with ID {id} to registry {registry} twice"
            ),
            Self::UnsupportedRegistry { reason, .. } => f.write_str(reason),
            Self::Closed(id) => write!(
                f,
                "cannot register data map type {id}: the RegisterDataMapTypesEvent has run"
            ),
        }
    }
}

impl std::error::Error for DataMapError {}

/// The registered data map types, per registry, in registration order.
#[derive(Default)]
struct Registered {
    closed: bool,
    by_registry: BTreeMap<Identifier, Vec<Arc<TypeInfo>>>,
}

static REGISTERED: RwLock<Registered> = RwLock::new(Registered {
    closed: false,
    by_registry: BTreeMap::new(),
});

fn registered() -> std::sync::RwLockReadGuard<'static, Registered> {
    REGISTERED.read().unwrap_or_else(PoisonError::into_inner)
}

/// `RegisterDataMapTypesEvent`: registers data map types.
///
/// The host passes it to [`NativeMod::register_data_map_types`](crate::NativeMod) of every mod
/// in load order, after `RegisterCapabilitiesEvent` and before `ExtendPoiTypesEvent`, as
/// `RegistrationEvents.init` posts it. A failed call returns the error and keeps it, so the
/// server does not start.
pub struct RegisterDataMapTypesEvent {
    target: &'static RwLock<Registered>,
    error: Option<DataMapError>,
}

impl fmt::Debug for RegisterDataMapTypesEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisterDataMapTypesEvent")
            .finish_non_exhaustive()
    }
}

/// A handle that the event can register: a [`DataMapType`] or an [`AdvancedDataMapType`].
pub trait AnyDataMapType: sealed::Sealed {
    #[doc(hidden)]
    fn type_info(&self) -> &Arc<TypeInfo>;
}

mod sealed {
    pub trait Sealed {}
}

impl<T> sealed::Sealed for DataMapType<T> {}
impl<T, R> sealed::Sealed for AdvancedDataMapType<T, R> {}

impl<T> AnyDataMapType for DataMapType<T> {
    fn type_info(&self) -> &Arc<TypeInfo> {
        &self.info
    }
}

impl<T, R> AnyDataMapType for AdvancedDataMapType<T, R> {
    fn type_info(&self) -> &Arc<TypeInfo> {
        &self.base.info
    }
}

impl RegisterDataMapTypesEvent {
    const fn new(target: &'static RwLock<Registered>) -> Self {
        Self {
            target,
            error: None,
        }
    }

    /// `register(type)`.
    ///
    /// # Errors
    ///
    /// [`DataMapError::Duplicate`] when the registry has a type with this id, and
    /// [`DataMapError::UnsupportedRegistry`] when the registry cannot carry the map: a synced map
    /// on a world registry that is not synced or on a reloadable registry, as in `NeoForge`, or a
    /// registry whose entries the server does not know.
    pub fn register(&mut self, data_map: &impl AnyDataMapType) -> Result<(), DataMapError> {
        let result = self.try_register(data_map.type_info());
        if let Err(error) = &result {
            self.error.get_or_insert_with(|| error.clone());
        }
        result
    }

    fn try_register(&self, info: &Arc<TypeInfo>) -> Result<(), DataMapError> {
        let mut target = self.target.write().unwrap_or_else(PoisonError::into_inner);
        if target.closed {
            return Err(DataMapError::Closed(info.id.clone()));
        }
        registries::check_registry(&info.id, &info.registry, info.sync.is_some())?;
        let types = target.by_registry.entry(info.registry.clone()).or_default();
        if types.iter().any(|known| known.id == info.id) {
            return Err(DataMapError::Duplicate {
                id: info.id.clone(),
                registry: info.registry.clone(),
            });
        }
        types.push(info.clone());
        Ok(())
    }
}

/// Posts the `RegisterDataMapTypesEvent` to the host, which registers the `NeoForge` built-in
/// types, and to every mod in load order, then closes the registration and installs the data map
/// host of the server. Returns the first error after the last mod.
pub(crate) fn post(mods: &[LoadedMod<'_>]) -> Result<(), String> {
    // The lifecycle tests post once per test in one process: each gets a registration of its
    // own.
    #[cfg(test)]
    let target: &'static RwLock<Registered> = Box::leak(Box::default());
    #[cfg(not(test))]
    let target = &REGISTERED;
    let result = post_with(mods, target, true);
    sync::install_host();
    result
}

/// [`post`] into `target`, with the built-in types when `builtin` is set.
fn post_with(
    mods: &[LoadedMod<'_>],
    target: &'static RwLock<Registered>,
    builtin: bool,
) -> Result<(), String> {
    let mut first_error = None;
    if builtin {
        let mut event = RegisterDataMapTypesEvent::new(target);
        builtin::register(&mut event);
        if let Some(error) = event.error {
            first_error.get_or_insert_with(|| {
                format!("the RegisterDataMapTypesEvent of the host: {error}")
            });
        }
    }
    for loaded in mods {
        let mut event = RegisterDataMapTypesEvent::new(target);
        loaded.native_mod.register_data_map_types(&mut event);
        if let Some(error) = event.error {
            first_error.get_or_insert_with(|| {
                format!(
                    "the RegisterDataMapTypesEvent of mod {}: {error}",
                    loaded.mod_id
                )
            });
        }
    }
    target
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .closed = true;
    first_error.map_or(Ok(()), Err)
}
