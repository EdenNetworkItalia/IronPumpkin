use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::block::entities::hopper::{ContainerOrHandler, HopperBlockEntity};
use crate::entity::{Entity, player::Player};
use pumpkin_data::{BlockDirection, translation};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::text::TextComponent;

use super::container::{self, MinecartInventory};

#[derive(Clone)]
pub(super) struct HopperMinecart {
    enabled: Arc<AtomicBool>,
    inventory: Arc<MinecartInventory>,
}

impl HopperMinecart {
    pub(super) fn new() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
            inventory: Arc::new(MinecartInventory::new(5)),
        }
    }

    pub(super) const fn inventory(&self) -> &Arc<MinecartInventory> {
        &self.inventory
    }

    pub(super) fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub(super) fn tick(&self, entity: &Entity) {
        if !self.enabled.load(Ordering::Relaxed) || !entity.is_alive() {
            return;
        }

        let world = entity.world.load();
        let pos = entity.pos.load();
        let source_pos = BlockPos::floored(pos.x, pos.y + 1.5, pos.z);
        let inventory = &self.inventory;
        let cart_box = entity.bounding_box.load().expand(0.25, 0.0, 0.25);

        let source_state = world.get_block_and_state(&source_pos);
        match HopperBlockEntity::container_or_handler_at(
            &world,
            &source_pos,
            BlockDirection::Down,
            source_state,
            pos.x,
            pos.y + 1.5,
            pos.z,
        ) {
            Some(ContainerOrHandler::Container(source)) => {
                match HopperBlockEntity::try_take_from_source(inventory.as_ref(), source.as_ref()) {
                    Ok(true) => return,
                    Ok(false) => {}
                    Err(leftover) => {
                        // A concurrent insert filled the slot, drop the item, never void it.
                        let drop_pos = source_pos.to_centered_f64();
                        world.scatter_stack(drop_pos.x, drop_pos.y, drop_pos.z, leftover);
                    }
                }
            }
            Some(ContainerOrHandler::Handler(source)) => {
                if HopperBlockEntity::extract_from_handler(source.as_ref(), inventory.as_ref()) {
                    return;
                }
            }
            None => {
                // `Hopper.SUCK_AABB` around the cart; never blocked, the cart isn't grid aligned.
                let suction_box = BoundingBox::new(
                    Vector3::new(pos.x - 0.5, pos.y + 0.6875, pos.z - 0.5),
                    Vector3::new(pos.x + 0.5, pos.y + 2.0, pos.z + 0.5),
                );
                if Self::pick_up_item_internal(&world, inventory, &suction_box) {
                    return;
                }
            }
        }
        // Vanilla `MinecartHopper.suckInItems` falls back to its own box either way.
        Self::pick_up_item_internal(&world, inventory, &cart_box);
    }

    fn pick_up_item_internal(
        world: &Arc<crate::world::World>,
        inventory: &Arc<MinecartInventory>,
        search_box: &BoundingBox,
    ) -> bool {
        for entity in world.get_entities_at_box(search_box) {
            if !entity.get_entity().is_alive() {
                continue;
            }
            let Some(item) = entity.get_item_entity() else {
                continue;
            };
            if HopperBlockEntity::pickup_item_entity(inventory.as_ref(), item) {
                return true;
            }
        }
        false
    }

    pub(super) fn interact(
        &self,
        entity: &crate::entity::Entity,
        player: &Arc<Player>,
        minecart: &dyn crate::entity::EntityBase,
    ) -> bool {
        container::open(
            entity,
            player,
            &self.inventory,
            TextComponent::translate_cross(
                translation::java::ENTITY_MINECRAFT_HOPPER_MINECART,
                translation::bedrock::ENTITY_HOPPER_MINECART_NAME,
                [],
            ),
            true,
            Some(minecart),
        )
    }

    pub(super) fn write_nbt(&self, nbt: &mut NbtCompound) {
        self.inventory.write_nbt(nbt);
        nbt.put_bool("Enabled", self.enabled.load(Ordering::Relaxed));
    }

    pub(super) fn read_nbt(&self, nbt: &NbtCompound) {
        self.inventory.read_nbt(nbt);
        self.enabled
            .store(nbt.get_bool("Enabled").unwrap_or(true), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::HopperMinecart;
    use pumpkin_inventory::Inventory;

    #[test]
    fn hopper_minecart_inventory_has_five_slots() {
        assert_eq!(HopperMinecart::new().inventory.size(), 5);
    }
}
