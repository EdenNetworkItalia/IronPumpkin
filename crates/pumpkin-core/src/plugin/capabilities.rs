//! The seam through which the engine reaches the capabilities of native mods
//! (`ironpumpkin_mods::capabilities`).
//!
//! The engine calls these functions where `NeoForge` patches vanilla: block entity placement
//! and removal, chest type changes and chunk load and unload invalidate the capability caches,
//! and hoppers and droppers fall back to the item capability of a block without a vanilla
//! inventory (`VanillaInventoryCodeHooks`). Without installed hooks every function does
//! nothing.

use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};

use pumpkin_data::{BlockDirection, block_properties::FacingHopper, item_stack::ItemStack};
use pumpkin_util::math::{position::BlockPos, vector2::Vector2};

use crate::world::World;

/// The item handler of a block, as hoppers and droppers use it. Each call runs in its own root
/// transaction.
pub trait ItemTarget {
    /// `ResourceHandlerUtil.isFull`.
    fn is_full(&self) -> bool;
    /// Inserts `stack` and returns the inserted count.
    fn insert(&self, stack: &ItemStack) -> u8;
    /// `VanillaInventoryCodeHooks.extractHook`: extracts one item from the first index that
    /// gives one and that `accept` takes, and commits only when `accept` returns `true`.
    fn extract_one(&self, accept: &mut dyn FnMut(&ItemStack) -> bool) -> bool;
}

/// The capability host of `ironpumpkin-mods`.
pub trait CapabilityHooks: Send + Sync {
    /// `ServerLevel.invalidateCapabilities(BlockPos)`.
    fn invalidate_block(&self, world: &World, pos: &BlockPos);
    /// `ServerLevel.invalidateCapabilities(ChunkPos)`.
    fn invalidate_chunk(&self, world: &World, chunk: Vector2<i32>);
    /// `Capabilities.Item.BLOCK` of the block at `pos`, looked up from `side`.
    fn item_target(
        &self,
        world: &Arc<World>,
        pos: &BlockPos,
        side: BlockDirection,
    ) -> Option<Box<dyn ItemTarget>>;
}

/// Set once by `ironpumpkin-mods` before the first world loads.
pub static CAPABILITY_HOOKS: OnceLock<&'static dyn CapabilityHooks> = OnceLock::new();

/// Set when a mod registers the first capability invalidation listener, so the worlds collect
/// their chunk changes for the listeners and the engine skips the invalidation calls before.
pub static CAPABILITY_LISTENERS: AtomicBool = AtomicBool::new(false);

/// Whether a capability invalidation listener exists.
pub fn has_listeners() -> bool {
    CAPABILITY_LISTENERS.load(Ordering::Relaxed)
}

fn hooks_with_listeners() -> Option<&'static dyn CapabilityHooks> {
    if has_listeners() {
        CAPABILITY_HOOKS.get().copied()
    } else {
        None
    }
}

pub(crate) fn invalidate_block(world: &World, pos: &BlockPos) {
    if let Some(hooks) = hooks_with_listeners() {
        hooks.invalidate_block(world, pos);
    }
}

pub(crate) fn invalidate_chunk(world: &World, chunk: Vector2<i32>) {
    if let Some(hooks) = hooks_with_listeners() {
        hooks.invalidate_chunk(world, chunk);
    }
}

pub(crate) fn item_target(
    world: &Arc<World>,
    pos: &BlockPos,
    side: BlockDirection,
) -> Option<Box<dyn ItemTarget>> {
    CAPABILITY_HOOKS.get()?.item_target(world, pos, side)
}

/// The side a hopper facing `facing` pushes into.
pub(crate) const fn hopper_direction(facing: FacingHopper) -> BlockDirection {
    match facing {
        FacingHopper::Down => BlockDirection::Down,
        FacingHopper::North => BlockDirection::North,
        FacingHopper::South => BlockDirection::South,
        FacingHopper::West => BlockDirection::West,
        FacingHopper::East => BlockDirection::East,
    }
}
