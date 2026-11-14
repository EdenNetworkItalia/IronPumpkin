//! The hopper side of `VanillaInventoryCodeHooks`: a hopper pushes into and pulls from the item
//! capability of a block without a vanilla inventory, with the same `take_one` and `put_back`
//! as the vanilla path, so a parallel write to a hopper slot is never overwritten.

use std::sync::{Arc, atomic::Ordering};

use pumpkin_data::{Block, BlockDirection, BlockState};
use pumpkin_inventory::Inventory;
use pumpkin_util::math::position::BlockPos;

use super::{HopperBlockEntity, to_offset};
use crate::{
    block::entities::BlockEntity,
    plugin::{
        api::events::inventory::inventory_move_item::InventoryMoveItemEvent,
        capabilities::{hopper_direction, item_target},
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

    /// `VanillaInventoryCodeHooks.insertHook`: pushes one item into the item capability of the
    /// block the hopper faces. `None` when that block has none.
    pub(super) fn insert_into_capability(
        &self,
        world: &Arc<World>,
        block_entity: Option<&Arc<dyn BlockEntity>>,
    ) -> Option<bool> {
        let target_pos = self.position.offset(to_offset(&self.facing));
        let target = item_target(
            world,
            &target_pos,
            hopper_direction(self.facing).opposite(),
            block_entity,
            None,
        )?;
        if target.is_full() {
            return Some(false);
        }
        for slot in 0..Self::INVENTORY_SIZE {
            let item = self.get_stack(slot);
            if item.is_empty() {
                continue;
            }
            let mut move_event = InventoryMoveItemEvent::new(
                self.position,
                target_pos,
                item.item.registry_key.to_string(),
                1,
            );
            if let Some(server) = world.server.upgrade() {
                server
                    .plugin_manager
                    .fire_blocking(&server, &mut move_event);
            }
            if move_event.cancelled {
                continue;
            }
            let Some(extraction) = self.take_one(slot) else {
                continue;
            };
            if target.insert(&extraction.one_item) == 1 {
                return Some(true);
            }
            if let Some(leftover) = self.put_back(slot, extraction) {
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
        block_entity: Option<&Arc<dyn BlockEntity>>,
        state: (&'static Block, &'static BlockState),
    ) -> Option<bool> {
        let source = item_target(
            world,
            source,
            BlockDirection::Down,
            block_entity,
            Some(state),
        )?;
        Some(source.extract_one(&mut |item| Self::add_one_item(self, self, item)))
    }
}
