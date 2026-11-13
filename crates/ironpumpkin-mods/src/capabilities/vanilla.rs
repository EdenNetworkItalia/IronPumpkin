//! The host part of the `RegisterCapabilitiesEvent` (`CapabilityHooks`) and the engine hooks
//! of hoppers and droppers (`VanillaInventoryCodeHooks`).

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use pumpkin_core::{
    block::entities::BlockEntity, plugin::capabilities::CapabilityHooks, world::World,
};
use pumpkin_data::{
    BlockDirection, BlockState,
    block_properties::{ChestLikeProperties, ChestType},
    item_stack::ItemStack,
};
use pumpkin_inventory::Inventory;
use pumpkin_util::math::{position::BlockPos, vector2::Vector2};

use super::{RegisterCapabilitiesEvent, energy, fluid, invalidate_capabilities, item};
use crate::{
    event::EventPriority,
    transfer::{
        CombinedResourceHandler, ItemResource, Resource as _, ResourceHandler, Transaction,
        insert_item_return_remaining,
        inventory::{ContainerKind, InventoryWrapper, SidedInventoryWrapper},
        is_full,
    },
};

type ItemHandler = dyn ResourceHandler<ItemResource>;

/// The chests of `registerVanillaProviders`: `Blocks.CHEST`, `Blocks.TRAPPED_CHEST` and the
/// copper chests.
const CHESTS: [&str; 10] = [
    "minecraft:chest",
    "minecraft:trapped_chest",
    "minecraft:copper_chest",
    "minecraft:exposed_copper_chest",
    "minecraft:weathered_copper_chest",
    "minecraft:oxidized_copper_chest",
    "minecraft:waxed_copper_chest",
    "minecraft:waxed_exposed_copper_chest",
    "minecraft:waxed_weathered_copper_chest",
    "minecraft:waxed_oxidized_copper_chest",
];

/// The `WorldlyContainer` block entity types of `registerVanillaProviders`.
const SIDED: [(&str, ContainerKind); 5] = [
    ("minecraft:blast_furnace", ContainerKind::Furnace),
    ("minecraft:brewing_stand", ContainerKind::BrewingStand),
    ("minecraft:furnace", ContainerKind::Furnace),
    ("minecraft:smoker", ContainerKind::Furnace),
    ("minecraft:shulker_box", ContainerKind::ShulkerBox),
];

/// The other container block entity types of `registerVanillaProviders`. Pumpkin's decorated
/// pot has no inventory yet, so its provider finds none.
const UNSIDED: [&str; 9] = [
    "minecraft:barrel",
    "minecraft:chiseled_bookshelf",
    "minecraft:dispenser",
    "minecraft:dropper",
    "minecraft:hopper",
    "minecraft:jukebox",
    "minecraft:crafter",
    "minecraft:decorated_pot",
    "minecraft:shelf",
];

static HOST_CLAIMED: AtomicBool = AtomicBool::new(false);

/// Whether this call runs the host part: once per process, as `CapabilityHooks.init`.
pub(super) fn claim_host() -> bool {
    !HOST_CLAIMED.swap(true, Ordering::AcqRel)
}

/// The listeners of the `neoforge` mod, which comes first in each phase.
pub(super) fn register(event: &mut RegisterCapabilitiesEvent) {
    match event.phase() {
        EventPriority::High => mark_proxyable_capabilities(event),
        EventPriority::Normal => register_vanilla_providers(event),
        _ => {}
    }
}

/// `CapabilityHooks.markProxyableCapabilities`.
fn mark_proxyable_capabilities(event: &mut RegisterCapabilitiesEvent) {
    let _ = event.set_proxyable(&energy::BLOCK);
    let _ = event.set_proxyable(&fluid::BLOCK);
    let _ = event.set_proxyable(&item::BLOCK);
}

/// `CapabilityHooks.registerVanillaProviders`, for the containers that Pumpkin has. The
/// composter, chest boats, minecarts and the shulker box and bundle items have no provider:
/// see the capability tables of the design.
fn register_vanilla_providers(event: &mut RegisterCapabilitiesEvent) {
    for chest in CHESTS {
        if crate::registration::block(chest).is_ok() {
            let _ = event.register_block(&item::BLOCK, chest_handler, &[chest]);
        }
    }
    for (block_entity_type, kind) in SIDED {
        let _ = event.register_block_entity(
            &item::BLOCK,
            block_entity_type,
            move |block_entity, side| {
                let inventory = block_entity.clone().get_inventory()?;
                let handler: Arc<ItemHandler> =
                    Arc::new(SidedInventoryWrapper::new(inventory, kind, side));
                Some(handler)
            },
        );
    }
    for block_entity_type in UNSIDED {
        let _ = event.register_block_entity(&item::BLOCK, block_entity_type, |block_entity, _| {
            let handler: Arc<ItemHandler> =
                InventoryWrapper::of(block_entity.clone().get_inventory()?);
            Some(handler)
        });
    }
    let _ = event.register_entity(&item::ENTITY, "minecraft:player", |entity, ()| {
        let inventory: Arc<dyn Inventory> = entity.get_player()?.inventory().clone();
        let handler: Arc<ItemHandler> = InventoryWrapper::of(inventory);
        Some(handler)
    });
}

/// `CHEST_COMBINER_HANDLER` over `ChestBlock.combine`: both halves of a double chest, the
/// first half first, whether a block or a cat blocks the chest or not.
fn chest_handler(
    world: &Arc<World>,
    pos: &BlockPos,
    state: &'static BlockState,
    block_entity: Option<&Arc<dyn BlockEntity>>,
    _side: Option<BlockDirection>,
) -> Option<Arc<ItemHandler>> {
    let this: Arc<ItemHandler> = InventoryWrapper::of(block_entity?.clone().get_inventory()?);
    let props = ChestLikeProperties::from_state_id(state.id);
    let towards = match props.r#type {
        ChestType::Single => return Some(this),
        ChestType::Left => props.facing.rotate_clockwise(),
        ChestType::Right => props.facing.rotate_counter_clockwise(),
    };
    let other_pos = pos.offset(towards.to_offset());
    let (other_block, other_state) = world.get_block_and_state(&other_pos);
    let other_props = ChestLikeProperties::from_state_id(other_state.id);
    if other_block.id != world.get_block(pos).id
        || other_props.facing != props.facing
        || other_props.r#type
            != match props.r#type {
                ChestType::Left => ChestType::Right,
                _ => ChestType::Left,
            }
    {
        return Some(this);
    }
    let Some(other) = world
        .get_block_entity(&other_pos)
        .and_then(BlockEntity::get_inventory)
    else {
        return Some(this);
    };
    let other: Arc<ItemHandler> = InventoryWrapper::of(other);
    // Vanilla: `chestType == ChestType.RIGHT ? FIRST : SECOND`.
    let halves = if props.r#type == ChestType::Right {
        vec![this, other]
    } else {
        vec![other, this]
    };
    Some(Arc::new(CombinedResourceHandler::new(halves)))
}

struct Hooks;

static HOOKS: Hooks = Hooks;

pub(super) fn install_hooks() {
    let _ = pumpkin_core::plugin::capabilities::CAPABILITY_HOOKS.set(&HOOKS);
}

impl CapabilityHooks for Hooks {
    fn invalidate_block(&self, world: &World, pos: &BlockPos) {
        invalidate_capabilities(world, pos);
    }

    fn invalidate_chunk(&self, world: &World, chunk: Vector2<i32>) {
        super::invalidate_capabilities_in_chunk(world, chunk);
    }

    /// `VanillaInventoryCodeHooks.insertHook`.
    fn hopper_insert(
        &self,
        world: &Arc<World>,
        hopper: &dyn Inventory,
        target: &BlockPos,
        side: BlockDirection,
    ) -> Option<bool> {
        let handler = item::BLOCK.get_capability(world, target, Some(side))?;
        if is_full(handler.as_ref()) {
            return Some(false);
        }
        let Ok(mut transaction) = Transaction::open_root() else {
            return Some(false);
        };
        for slot in 0..hopper.size() {
            let original = hopper.get_stack(slot);
            if original.is_empty() {
                continue;
            }
            let one = hopper.remove_stack_specific(slot, 1);
            if handler.insert(&ItemResource::of(&one), 1, &mut transaction) == 1 {
                transaction.commit();
                return Some(true);
            }
            // The item left the hopper but did not fit, so it goes back.
            hopper.set_stack(slot, original);
        }
        Some(false)
    }

    /// `VanillaInventoryCodeHooks.extractHook`.
    fn hopper_extract(
        &self,
        world: &Arc<World>,
        hopper: &dyn Inventory,
        source: &BlockPos,
    ) -> Option<bool> {
        let handler = item::BLOCK.get_capability(world, source, Some(BlockDirection::Down))?;
        for index in 0..handler.size() {
            let resource = handler.get_resource(index);
            if resource.is_empty() {
                continue;
            }
            let Ok(mut transaction) = Transaction::open_root() else {
                return Some(false);
            };
            if handler.extract_at(index, &resource, 1, &mut transaction) == 0 {
                continue;
            }
            let extracted = resource.to_stack(1);
            for slot in 0..hopper.size() {
                let mut destination = hopper.get_stack(slot);
                let fits = destination.is_empty()
                    || (destination.item_count < destination.get_max_stack_size()
                        && destination.item_count < hopper.get_max_count_per_stack()
                        && resource.matches(&destination));
                if hopper.is_valid_slot_for(slot, &extracted) && fits {
                    if destination.is_empty() {
                        hopper.set_stack(slot, extracted);
                    } else {
                        destination.increment(1);
                        hopper.set_stack(slot, destination);
                    }
                    hopper.mark_dirty();
                    transaction.commit();
                    return Some(true);
                }
            }
        }
        Some(false)
    }

    /// `DropperBlock.dispenseFrom` with `ItemUtil.insertItemReturnRemaining`.
    fn insert_stack(
        &self,
        world: &Arc<World>,
        target: &BlockPos,
        side: BlockDirection,
        stack: &ItemStack,
    ) -> Option<u8> {
        let handler = item::BLOCK.get_capability(world, target, Some(side))?;
        let remaining = insert_item_return_remaining(handler.as_ref(), stack, false, None)
            .unwrap_or_else(|_| stack.clone());
        let left = if remaining.is_empty() {
            0
        } else {
            remaining.item_count
        };
        Some(stack.item_count.saturating_sub(left))
    }
}
