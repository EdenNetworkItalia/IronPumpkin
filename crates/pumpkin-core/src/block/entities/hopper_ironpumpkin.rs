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

/// `ContainerOrHandler`: what a hopper, a dropper or a crafter finds at a position.
pub enum ContainerOrHandler {
    Container(Arc<dyn Inventory>),
    Handler(Box<dyn ItemTarget>),
}

impl HopperBlockEntity {
    /// `HopperBlockEntity.getContainerOrHandlerAt`: the block container, else the item
    /// capability of the block looked up from `side`, else a container entity around `x`, `y`,
    /// `z`. A container minecart inside a block with an item capability loses to the block.
    pub(crate) fn container_or_handler_at(
        world: &Arc<World>,
        pos: &BlockPos,
        side: BlockDirection,
        state: (&'static Block, &'static BlockState),
        x: f64,
        y: f64,
        z: f64,
    ) -> Option<ContainerOrHandler> {
        if let Some(container) = Self::get_block_container(world, pos, state) {
            return Some(ContainerOrHandler::Container(container));
        }
        if let Some(handler) = Self::capability_target(world, pos, side, Some(state)) {
            return Some(ContainerOrHandler::Handler(handler));
        }
        Self::get_entity_container(world, x, y, z).map(ContainerOrHandler::Container)
    }

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

    /// `VanillaInventoryCodeHooks.insertHook`: pushes one item into the item capability
    /// `target` of the block at `target_pos`.
    pub(super) fn insert_into_handler(
        &self,
        world: &Arc<World>,
        target_pos: &BlockPos,
        target: &dyn ItemTarget,
    ) -> bool {
        if target.is_full() {
            return false;
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
                return true;
            }
            if let Some(leftover) = Self::put_back(self, slot, one_item) {
                let pos = self.position.to_centered_f64();
                world.scatter_stack(pos.x, pos.y, pos.z, leftover);
            }
        }
        false
    }

    /// `VanillaInventoryCodeHooks.extractHook`: pulls one item from the item capability
    /// `source` of the block above.
    pub(super) fn extract_from_handler(&self, source: &dyn ItemTarget) -> bool {
        source.extract_one(&mut |item| Self::add_one_item(self, self, item))
    }
}
