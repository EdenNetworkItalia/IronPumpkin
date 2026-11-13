//! The seam through which the engine reaches the capabilities of native mods
//! (`ironpumpkin_mods::capabilities`).
//!
//! The engine calls these functions where `NeoForge` patches vanilla: block entity placement
//! and removal, chest type changes and chunk load and unload invalidate the capability caches,
//! and hoppers and droppers fall back to the item capability of a block without a vanilla
//! inventory (`VanillaInventoryCodeHooks`). Without installed hooks every function does
//! nothing.

use std::sync::{Arc, OnceLock};

use pumpkin_data::{BlockDirection, item_stack::ItemStack};
use pumpkin_inventory::Inventory;
use pumpkin_util::math::{position::BlockPos, vector2::Vector2};

use crate::world::World;

/// The capability host of `ironpumpkin-mods`.
pub trait CapabilityHooks: Send + Sync {
    /// `ServerLevel.invalidateCapabilities(BlockPos)`.
    fn invalidate_block(&self, world: &World, pos: &BlockPos);
    /// `ServerLevel.invalidateCapabilities(ChunkPos)`.
    fn invalidate_chunk(&self, world: &World, chunk: Vector2<i32>);
    /// `VanillaInventoryCodeHooks.insertHook`: moves one item of `hopper` into the item
    /// capability of the block at `target`, looked up from `side`. `None` when the block has no
    /// item capability.
    fn hopper_insert(
        &self,
        world: &Arc<World>,
        hopper: &dyn Inventory,
        target: &BlockPos,
        side: BlockDirection,
    ) -> Option<bool>;
    /// `VanillaInventoryCodeHooks.extractHook`: moves one item from the item capability of the
    /// block at `source`, looked up from below, into `hopper`. `None` when the block has no item
    /// capability.
    fn hopper_extract(
        &self,
        world: &Arc<World>,
        hopper: &dyn Inventory,
        source: &BlockPos,
    ) -> Option<bool>;
    /// The dropper branch of `DropperBlock.dispenseFrom`: inserts `stack` into the item
    /// capability of the block at `target`, looked up from `side`, and returns the inserted
    /// count. `None` when the block has no item capability.
    fn insert_stack(
        &self,
        world: &Arc<World>,
        target: &BlockPos,
        side: BlockDirection,
        stack: &ItemStack,
    ) -> Option<u8>;
}

/// Set once by `ironpumpkin-mods` before the first world loads.
pub static CAPABILITY_HOOKS: OnceLock<&'static dyn CapabilityHooks> = OnceLock::new();

pub(crate) fn invalidate_block(world: &World, pos: &BlockPos) {
    if let Some(hooks) = CAPABILITY_HOOKS.get() {
        hooks.invalidate_block(world, pos);
    }
}

pub(crate) fn invalidate_chunk(world: &World, chunk: Vector2<i32>) {
    if let Some(hooks) = CAPABILITY_HOOKS.get() {
        hooks.invalidate_chunk(world, chunk);
    }
}

pub(crate) fn hopper_insert(
    world: &Arc<World>,
    hopper: &dyn Inventory,
    target: &BlockPos,
    side: BlockDirection,
) -> Option<bool> {
    CAPABILITY_HOOKS
        .get()?
        .hopper_insert(world, hopper, target, side)
}

pub(crate) fn hopper_extract(
    world: &Arc<World>,
    hopper: &dyn Inventory,
    source: &BlockPos,
) -> Option<bool> {
    CAPABILITY_HOOKS
        .get()?
        .hopper_extract(world, hopper, source)
}

pub(crate) fn insert_stack(
    world: &Arc<World>,
    target: &BlockPos,
    side: BlockDirection,
    stack: &ItemStack,
) -> Option<u8> {
    CAPABILITY_HOOKS
        .get()?
        .insert_stack(world, target, side, stack)
}
