//! `BlockCapabilityCache` and the invalidation listeners of each world
//! (`CapabilityListenerHolder`).

use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, LazyLock, Mutex, PoisonError, Weak},
};

use pumpkin_core::world::World;
use pumpkin_util::math::{position::BlockPos, vector2::Vector2};

use super::{BlockCapability, CapabilityError};

/// `ICapabilityInvalidationListener`: told that the capabilities at a position may have
/// changed. Returns `false` to stop listening.
pub trait CapabilityInvalidationListener: Send + Sync {
    /// `onInvalidate`.
    fn on_invalidate(&self) -> bool;
}

type ChunkListeners = HashMap<BlockPos, Vec<Weak<dyn CapabilityInvalidationListener>>>;
/// The listeners by world, chunk and position. The world is the address of its `World`.
type Holder = Mutex<HashMap<usize, HashMap<Vector2<i32>, ChunkListeners>>>;

static LISTENERS: LazyLock<Holder> = LazyLock::new(Holder::default);

fn world_key(world: &World) -> usize {
    std::ptr::from_ref(world) as usize
}

/// `ServerLevel.registerCapabilityListener`: calls `listener` when the capabilities at `pos`
/// may have changed. The world keeps only a weak reference: the listener stops when the caller
/// drops it.
pub fn register_capability_listener(
    world: &World,
    pos: BlockPos,
    listener: &Arc<dyn CapabilityInvalidationListener>,
) {
    let mut holder = LISTENERS.lock().unwrap_or_else(PoisonError::into_inner);
    let listeners = holder
        .entry(world_key(world))
        .or_default()
        .entry(pos.chunk_position())
        .or_default()
        .entry(pos)
        .or_default();
    let listener = Arc::downgrade(listener);
    listeners.retain(|existing| existing.strong_count() > 0);
    if !listeners
        .iter()
        .any(|existing| Weak::ptr_eq(existing, &listener))
    {
        listeners.push(listener);
    }
}

/// `ServerLevel.invalidateCapabilities(BlockPos)`: calls the listeners at `pos`.
///
/// A mod calls it
/// when the capabilities of a block without a block entity change; the host calls it when a
/// block entity is placed or removed and when a chest changes its type.
pub fn invalidate_capabilities(world: &World, pos: &BlockPos) {
    let listeners = {
        let mut holder = LISTENERS.lock().unwrap_or_else(PoisonError::into_inner);
        holder
            .get_mut(&world_key(world))
            .and_then(|chunks| chunks.get_mut(&pos.chunk_position()))
            .and_then(|positions| positions.remove(pos))
    };
    if let Some(listeners) = listeners {
        notify(world, vec![(*pos, listeners)]);
    }
}

/// `ServerLevel.invalidateCapabilities(ChunkPos)`: calls the listeners in the chunk. The host
/// calls it when the chunk loads or unloads.
pub fn invalidate_capabilities_in_chunk(world: &World, chunk: Vector2<i32>) {
    let positions = {
        let mut holder = LISTENERS.lock().unwrap_or_else(PoisonError::into_inner);
        holder
            .get_mut(&world_key(world))
            .and_then(|chunks| chunks.remove(&chunk))
    };
    if let Some(positions) = positions {
        notify(world, positions.into_iter().collect());
    }
}

/// Calls the listeners outside the lock, then keeps those that returned `true`.
fn notify(
    world: &World,
    positions: Vec<(BlockPos, Vec<Weak<dyn CapabilityInvalidationListener>>)>,
) {
    let mut kept = Vec::new();
    for (pos, listeners) in positions {
        let alive: Vec<_> = listeners
            .into_iter()
            .filter(|listener| {
                listener
                    .upgrade()
                    .is_some_and(|listener| listener.on_invalidate())
            })
            .collect();
        if !alive.is_empty() {
            kept.push((pos, alive));
        }
    }
    if kept.is_empty() {
        return;
    }
    let mut holder = LISTENERS.lock().unwrap_or_else(PoisonError::into_inner);
    let chunks = holder.entry(world_key(world)).or_default();
    for (pos, alive) in kept {
        let listeners = chunks
            .entry(pos.chunk_position())
            .or_default()
            .entry(pos)
            .or_default();
        for listener in alive {
            if !listeners
                .iter()
                .any(|existing| Weak::ptr_eq(existing, &listener))
            {
                listeners.push(listener);
            }
        }
    }
}

struct CacheState<T: ?Sized> {
    cache_valid: bool,
    cached: Option<Arc<T>>,
    can_query: bool,
}

struct Listener<T: ?Sized> {
    state: Mutex<CacheState<T>>,
    is_valid: Box<dyn Fn() -> bool + Send + Sync>,
    on_invalidate: Box<dyn Fn() + Send + Sync>,
}

impl<T: ?Sized + Send + Sync> CapabilityInvalidationListener for Listener<T> {
    fn on_invalidate(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if !state.cache_valid {
            drop(state);
            return (self.is_valid)();
        }
        state.can_query = false;
        state.cache_valid = false;
        state.cached = None;
        drop(state);
        if !(self.is_valid)() {
            return false;
        }
        (self.on_invalidate)();
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .can_query = true;
        true
    }
}

/// `BlockCapabilityCache`: a lookup of a block capability at one position that keeps its
/// result until the host invalidates it.
pub struct BlockCapabilityCache<T: ?Sized + 'static, C: 'static> {
    capability: BlockCapability<T, C>,
    world: Arc<World>,
    pos: BlockPos,
    context: C,
    listener: Arc<Listener<T>>,
}

impl<T: ?Sized + Send + Sync + 'static, C: Clone + Send + Sync + 'static>
    BlockCapabilityCache<T, C>
{
    /// `BlockCapabilityCache.create(capability, level, pos, context)`: a cache that never
    /// becomes invalid.
    #[must_use]
    pub fn create(
        capability: BlockCapability<T, C>,
        world: &Arc<World>,
        pos: BlockPos,
        context: C,
    ) -> Self {
        Self::create_with_listener(capability, world, pos, context, || true, || {})
    }

    /// `BlockCapabilityCache.create(capability, level, pos, context, isValid,
    /// invalidationListener)`: `invalidation_listener` runs when a valid cache was invalidated
    /// and `is_valid` still returns `true`. Once `is_valid` returns `false` the cache stops
    /// listening and [`get_capability`](Self::get_capability) fails.
    pub fn create_with_listener(
        capability: BlockCapability<T, C>,
        world: &Arc<World>,
        pos: BlockPos,
        context: C,
        is_valid: impl Fn() -> bool + Send + Sync + 'static,
        invalidation_listener: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let listener = Arc::new(Listener {
            state: Mutex::new(CacheState {
                cache_valid: false,
                cached: None,
                can_query: true,
            }),
            is_valid: Box::new(is_valid),
            on_invalidate: Box::new(invalidation_listener),
        });
        let dyn_listener: Arc<dyn CapabilityInvalidationListener> = listener.clone();
        register_capability_listener(world, pos, &dyn_listener);
        Self {
            capability,
            world: world.clone(),
            pos,
            context,
            listener,
        }
    }

    /// `level`.
    #[must_use]
    pub const fn world(&self) -> &Arc<World> {
        &self.world
    }

    /// `pos`.
    #[must_use]
    pub const fn pos(&self) -> BlockPos {
        self.pos
    }

    /// `context`.
    #[must_use]
    pub const fn context(&self) -> &C {
        &self.context
    }

    /// `getCapability`: the cached handler, looked up again after an invalidation. An unloaded
    /// chunk has no capability.
    ///
    /// # Errors
    ///
    /// [`CapabilityError::InvalidCache`] after `is_valid` returned `false`, or from the
    /// invalidation listener.
    pub fn get_capability(&self) -> Result<Option<Arc<T>>, CapabilityError> {
        {
            let state = self
                .listener
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if !state.can_query {
                return Err(CapabilityError::InvalidCache);
            }
            if state.cache_valid {
                return Ok(state.cached.clone());
            }
        }
        let cached = if self.world.level.is_chunk_loaded(&self.pos.chunk_position()) {
            self.capability
                .get_capability(&self.world, &self.pos, self.context.clone())
        } else {
            None
        };
        let mut state = self
            .listener
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.cached.clone_from(&cached);
        state.cache_valid = true;
        Ok(cached)
    }
}

impl<T: ?Sized + 'static, C: 'static> fmt::Debug for BlockCapabilityCache<T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockCapabilityCache")
            .field("capability", &self.capability)
            .field("pos", &self.pos)
            .finish_non_exhaustive()
    }
}
