//! The hopper side of `VanillaInventoryCodeHooks`: a hopper pushes into and pulls from the item
//! capability of a block without a vanilla inventory, with the same `take_one` and `put_back`
//! as the vanilla path, so a parallel write to a hopper slot is never overwritten.

use std::sync::{Arc, atomic::Ordering};

use pumpkin_data::{Block, BlockDirection, BlockState};
use pumpkin_inventory::Inventory;
use pumpkin_util::math::position::BlockPos;

use super::HopperBlockEntity;
use crate::{
    plugin::{
        api::events::inventory::inventory_move_item::InventoryMoveItemEvent,
        capabilities::{CAPABILITY_HOOKS, ItemTarget, item_target},
    },
    world::World,
};

impl HopperBlockEntity {
    /// `cooldownTime`.
    #[must_use]
    pub fn cooldown(&self) -> i32 {
        self.cooldown_time.load(Ordering::Relaxed)
    }

    /// `setCooldown`.
    pub fn set_cooldown(&self, cooldown: i32) {
        self.cooldown_time.store(cooldown, Ordering::Relaxed);
    }

    /// The item capability of the block at `pos`, looked up from `side`. Reads the block entity
    /// only when a mod installed the hooks and the state has one, as the vanilla container
    /// lookup does.
    pub(crate) fn capability_target(
        world: &Arc<World>,
        pos: &BlockPos,
        side: BlockDirection,
        state: Option<(&'static Block, &'static BlockState)>,
    ) -> Option<Box<dyn ItemTarget>> {
        CAPABILITY_HOOKS.get()?;
        let state = state.unwrap_or_else(|| world.get_block_and_state(pos));
        let block_entity = if state.1.has_block_entity() {
            world.get_block_entity(pos)
        } else {
            None
        };
        item_target(world, pos, side, block_entity.as_ref(), Some(state))
    }

    /// `VanillaInventoryCodeHooks.insertHook`: pushes one item into the item capability of the
    /// block at `target_pos`, looked up from `side`. `None` when that block has none.
    pub(super) fn insert_into_capability(
        &self,
        world: &Arc<World>,
        target_pos: &BlockPos,
        side: BlockDirection,
    ) -> Option<bool> {
        let target = Self::capability_target(world, target_pos, side, None)?;
        if target.is_full() {
            return Some(false);
        }
        for slot in 0..Self::INVENTORY_SIZE {
            let item = self.get_stack(slot);
            if item.is_empty() {
                continue;
            }
            if world.cancelled_by_plugin(|| {
                InventoryMoveItemEvent::new(
                    self.position,
                    *target_pos,
                    item.item.registry_key.to_string(),
                    1,
                )
            }) {
                continue;
            }
            let Some(one_item) = Self::take_one(self, slot, &item) else {
                continue;
            };
            if target.insert(&one_item) == 1 {
                return Some(true);
            }
            if let Some(leftover) = Self::put_back(self, slot, one_item) {
                let pos = self.position.to_centered_f64();
                world.scatter_stack(pos.x, pos.y, pos.z, leftover);
            }
        }
        Some(false)
    }

    /// `VanillaInventoryCodeHooks.extractHook`: pulls one item from the item capability of the
    /// block at `source`, looked up from below. `None` when that block has none.
    pub(super) fn extract_from_capability(
        &self,
        world: &Arc<World>,
        source: &BlockPos,
        state: (&'static Block, &'static BlockState),
    ) -> Option<bool> {
        let source = Self::capability_target(world, source, BlockDirection::Down, Some(state))?;
        Some(source.extract_one(&mut |item| Self::add_one_item(self, self, item)))
    }
}
