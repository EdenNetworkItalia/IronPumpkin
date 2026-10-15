//! The `IronPumpkin` events of the anvil, the grindstone and the enchanting table.
//!
//! The events fire after the player released the lock of its screen handler, so a handler can
//! use the screen handler (close it, change the ender chest). The result changes only when the
//! inputs are the same after the event.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use pumpkin_data::world::WorldEvent;
use pumpkin_inventory::{
    anvil::{AnvilInputs, AnvilScreenHandler, AnvilUpdate},
    enchanting::enchanting_screen_handler::{
        EnchantRequest, EnchantingRows, EnchantingTableScreenHandler,
    },
    grindstone_screen_handler::{
        GrindstoneInputs, GrindstonePlace, GrindstoneScreenHandler, GrindstoneTake,
    },
    screen_handler::{ScreenHandler, offer_or_drop_stack},
};
use pumpkin_util::math::position::BlockPos;

use crate::entity::{experience_orb::ExperienceOrbEntity, player::Player};
use crate::plugin::{
    Payload,
    enchantment::{EnchantItemEvent, PlayerEnchantItemEvent, PrepareItemEnchantEvent},
    inventory::{
        grindstone_take::GrindstoneEventOnTakeItem, prepare_anvil::PrepareAnvilEvent,
        prepare_grindstone::PrepareGrindstoneEvent,
    },
};
use crate::server::Server;

type Guard<'a> = MutexGuard<'a, dyn ScreenHandler + 'static>;
type Handler = Mutex<dyn ScreenHandler + 'static>;

fn lock(handler: &Handler) -> Guard<'_> {
    handler.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The server of `player` when a handler of `E` listens.
fn listening<E: Payload + 'static>(player: &Player) -> Option<Arc<Server>> {
    let server = player.world().server.upgrade()?;
    server.plugin_manager.has_handlers::<E>().then_some(server)
}

fn as_table<'a>(guard: &'a mut Guard<'_>) -> Option<&'a mut EnchantingTableScreenHandler> {
    guard
        .as_any_mut()
        .downcast_mut::<EnchantingTableScreenHandler>()
}

/// Whether a handler listens to the grindstone take event
/// (`InventoryPlayer::listens_to_grindstone_take`).
pub fn listens_to_grindstone_take(player: &Player) -> bool {
    listening::<GrindstoneEventOnTakeItem>(player).is_some()
}

/// Fires the anvil, grindstone or enchanting table events that the last action on `handler` left
/// pending. It releases `guard` while the handlers run and returns the guard it locks again.
pub fn fire_pending<'a>(
    player: &Arc<Player>,
    handler: &'a Handler,
    mut guard: Guard<'a>,
) -> Guard<'a> {
    if let Some(anvil) = guard.as_any_mut().downcast_mut::<AnvilScreenHandler>() {
        if anvil.take_update_event()
            && let Some(inputs) = anvil.update_inputs()
        {
            return fire_anvil(player, handler, guard, &inputs);
        }
    } else if let Some(grindstone) = guard.as_any_mut().downcast_mut::<GrindstoneScreenHandler>() {
        if let Some(take) = grindstone.take_take_event() {
            guard = fire_grindstone_take(player, handler, guard, take);
        }
        if let Some(grindstone) = guard.as_any_mut().downcast_mut::<GrindstoneScreenHandler>()
            && grindstone.take_update_event()
        {
            let inputs = grindstone.update_inputs();
            return fire_grindstone(player, handler, guard, &inputs);
        }
    } else if let Some(table) = as_table(&mut guard)
        && table.take_prepare_event()
    {
        return fire_prepare_item_enchant(player, handler, guard);
    }
    guard
}

/// Fires the anvil update event for `inputs` with the screen handler unlocked
/// (`CommonHooks.onAnvilUpdate`), and applies its result.
pub fn fire_anvil<'a>(
    player: &Arc<Player>,
    handler: &'a Handler,
    guard: Guard<'a>,
    inputs: &AnvilInputs,
) -> Guard<'a> {
    let Some(server) = player.world().server.upgrade() else {
        return guard;
    };
    if !server.plugin_manager.has_handlers::<PrepareAnvilEvent>() {
        return guard;
    }
    drop(guard);
    let mut event = PrepareAnvilEvent::new(
        player.clone(),
        inputs.name.clone().unwrap_or_default(),
        inputs.left.clone(),
        inputs.right.clone(),
        inputs.output.clone(),
        inputs.xp_cost.max(0).cast_unsigned(),
        inputs.material_cost,
    );
    server.plugin_manager.fire_blocking(&server, &mut event);
    let update = if event.cancelled {
        AnvilUpdate::Cancelled
    } else {
        AnvilUpdate::Result {
            output: event.output,
            xp_cost: i32::try_from(event.repair_cost).unwrap_or(i32::MAX),
            material_cost: event.material_cost,
        }
    };
    let mut guard = lock(handler);
    if let Some(anvil) = guard.as_any_mut().downcast_mut::<AnvilScreenHandler>() {
        anvil.apply_update_event(inputs, update);
    }
    guard
}

/// Fires the grindstone place event for `inputs` with the screen handler unlocked
/// (`CommonHooks.onGrindstoneChange`), and applies its result.
fn fire_grindstone<'a>(
    player: &Arc<Player>,
    handler: &'a Handler,
    guard: Guard<'a>,
    inputs: &GrindstoneInputs,
) -> Guard<'a> {
    let Some(server) = player.world().server.upgrade() else {
        return guard;
    };
    if !server
        .plugin_manager
        .has_handlers::<PrepareGrindstoneEvent>()
    {
        return guard;
    }
    drop(guard);
    let result_item =
        (!inputs.output.is_empty()).then(|| inputs.output.item.registry_key.to_string());
    let mut event = PrepareGrindstoneEvent::new(
        player.clone(),
        result_item,
        inputs.top.clone(),
        inputs.bottom.clone(),
    );
    server.plugin_manager.fire_blocking(&server, &mut event);
    let place = if event.cancelled {
        Some(GrindstonePlace::Cancelled)
    } else if event.output.is_empty() {
        None
    } else {
        Some(GrindstonePlace::Output {
            output: event.output,
            xp: event.xp,
        })
    };
    let mut guard = lock(handler);
    if let Some(place) = place
        && let Some(grindstone) = guard.as_any_mut().downcast_mut::<GrindstoneScreenHandler>()
    {
        grindstone.apply_update_event(inputs, place);
    }
    guard
}

/// Fires the grindstone take event for `take` with the screen handler unlocked
/// (`CommonHooks.onGrindstoneTake`), and gives the experience and the inputs it decides.
fn fire_grindstone_take<'a>(
    player: &Arc<Player>,
    handler: &'a Handler,
    guard: Guard<'a>,
    take: GrindstoneTake,
) -> Guard<'a> {
    let Some(server) = player.world().server.upgrade() else {
        return guard;
    };
    drop(guard);
    let pos = player.open_container_pos.load();
    let mut event = GrindstoneEventOnTakeItem::new(
        player.clone(),
        pos.unwrap_or(BlockPos::ZERO),
        take.top,
        take.bottom,
        take.xp,
    );
    server.plugin_manager.fire_blocking(&server, &mut event);
    let cancelled = event.cancelled;
    let (top, bottom) = if cancelled {
        (event.top, event.bottom)
    } else {
        // As `use_grindstone`: no orbs and no sound without a known grindstone position.
        if let Some(pos) = pos {
            let world = player.world();
            if event.xp > 0 {
                ExperienceOrbEntity::spawn(&world, pos.to_centered_f64(), event.xp.cast_unsigned());
            }
            world.sync_world_event(WorldEvent::SoundGrindstoneUsed, pos, 0);
        }
        (event.new_top, event.new_bottom)
    };
    // The close path empties the grindstone before it replaces `current_screen_handler`, so the
    // open check is the closed flag of the grindstone, read under its lock.
    let mut guard = lock(handler);
    let applied = guard
        .as_any_mut()
        .downcast_mut::<GrindstoneScreenHandler>()
        .is_some_and(|grindstone| {
            if cancelled {
                // `NeoForge` leaves the result empty after a cancelled take.
                grindstone.take_update_event();
            }
            grindstone.apply_take_event(top.clone(), bottom.clone(), !cancelled)
        });
    if !applied {
        // The grindstone is closed or has new inputs: the player keeps the stacks.
        for stack in [top, bottom] {
            if !stack.is_empty() {
                offer_or_drop_stack(player.as_ref(), stack);
            }
        }
    }
    guard
}

/// Fires [`PrepareItemEnchantEvent`] (`EnchantmentLevelSetEvent`) for the rows of the table with
/// the screen handler unlocked, and applies the rows it decides. When the rows changed while the
/// event fired, it fires once more for the new rows.
fn fire_prepare_item_enchant<'a>(
    player: &Arc<Player>,
    handler: &'a Handler,
    mut guard: Guard<'a>,
) -> Guard<'a> {
    let Some(server) = listening::<PrepareItemEnchantEvent>(player) else {
        return guard;
    };
    for fire in 0..2 {
        let Some(table) = as_table(&mut guard) else {
            break;
        };
        if fire > 0 && !table.take_prepare_event() {
            break;
        }
        let rows = table.prepare_rows();
        let bookshelf_count = table.bookshelf_count;
        drop(guard);
        let mut event = PrepareItemEnchantEvent::new(
            player.clone(),
            rows.item.clone(),
            rows.level_requirements,
            rows.enchantment_id,
            rows.enchantment_level,
            bookshelf_count,
            player.open_container_pos.load().unwrap_or(BlockPos::ZERO),
        );
        server.plugin_manager.fire_blocking(&server, &mut event);
        let result = (!event.cancelled).then(|| EnchantingRows {
            item: event.item,
            level_requirements: event.level_requirements,
            enchantment_id: event.enchantment_id,
            enchantment_level: event.enchantment_level,
        });
        guard = lock(handler);
        if let Some(table) = as_table(&mut guard) {
            table.apply_prepare_event(&rows, result);
        }
    }
    guard
}

/// Whether `current` is the click of `request` on an unchanged table.
fn same_request(current: &EnchantRequest, request: &EnchantRequest) -> bool {
    current.level_requirement == request.level_requirement
        && current.seed == request.seed
        && current.item.are_equal(&request.item)
        && current.lapis.are_equal(&request.lapis)
}

/// Handles a click on button `id` of the open screen (`Player::on_container_button_click`).
///
/// An enchanting table fires [`EnchantItemEvent`], [`PlayerEnchantItemEvent`] and the
/// enchantment level event with the screen handler unlocked. The click goes on after
/// [`EnchantItemEvent`] only when the table did not change, and the table takes the item of
/// [`PlayerEnchantItemEvent`] only when slot 0 still holds the enchanted item.
pub fn on_button_click<'a>(
    player: &Arc<Player>,
    handler: &'a Handler,
    mut guard: Guard<'a>,
    id: i32,
) -> Guard<'a> {
    let Some(table) = as_table(&mut guard) else {
        guard.on_button_click(player.as_ref(), id);
        return guard;
    };
    let Some(mut request) = table.enchant_request(player.as_ref(), id) else {
        return guard;
    };
    if let Some(server) = listening::<EnchantItemEvent>(player) {
        drop(guard);
        let mut event = EnchantItemEvent::new(
            player.clone(),
            request.item.clone(),
            id,
            request.level_requirement,
            request.enchantments.clone(),
        );
        server.plugin_manager.fire_blocking(&server, &mut event);
        guard = lock(handler);
        let unchanged = as_table(&mut guard)
            .and_then(|table| table.enchant_request(player.as_ref(), id))
            .is_some_and(|current| same_request(&current, &request));
        if !unchanged || event.cancelled || event.enchantments_to_add.is_empty() {
            return guard;
        }
        request.enchantments = event.enchantments_to_add;
    }
    let Some(table) = as_table(&mut guard) else {
        return guard;
    };
    let enchanted = table.enchant_item(player.as_ref(), &request);
    // `NeoForge` fires `onPlayerEnchantItem` on the item in the table, before the lapis is
    // consumed.
    if let Some(server) = listening::<PlayerEnchantItemEvent>(player) {
        drop(guard);
        let mut event = PlayerEnchantItemEvent::new(
            player.clone(),
            enchanted.clone(),
            request.enchantments.clone(),
        );
        server.plugin_manager.fire_blocking(&server, &mut event);
        guard = lock(handler);
        if let Some(table) = as_table(&mut guard)
            && table.inventory.get_stack(0).are_equal(&enchanted)
        {
            table.inventory.set_stack(0, event.enchanted_item);
        }
    }
    if let Some(table) = as_table(&mut guard) {
        table.finish_enchant(player.as_ref(), &request);
    }
    fire_pending(player, handler, guard)
}
