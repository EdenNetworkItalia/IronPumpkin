//! The `IronPumpkin` events of the anvil, the grindstone and the enchanting table.
//!
//! The anvil and grindstone events fire after the player released the lock of its screen
//! handler, so a handler can use the screen handler (close it, change the ender chest). The
//! result changes only when the inputs are the same after the event.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use pumpkin_data::{Enchantment, item_stack::ItemStack};
use pumpkin_inventory::{
    anvil::{AnvilInputs, AnvilScreenHandler, AnvilUpdate},
    grindstone_screen_handler::{GrindstoneInputs, GrindstonePlace, GrindstoneScreenHandler},
    screen_handler::ScreenHandler,
};

use crate::entity::player::Player;
use crate::plugin::{
    enchantment::PlayerEnchantItemEvent,
    inventory::{prepare_anvil::PrepareAnvilEvent, prepare_grindstone::PrepareGrindstoneEvent},
};

type Guard<'a> = MutexGuard<'a, dyn ScreenHandler + 'static>;
type Handler = Mutex<dyn ScreenHandler + 'static>;

fn lock(handler: &Handler) -> Guard<'_> {
    handler.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Fires the anvil or grindstone event that the last action on `handler` left pending. It
/// releases `guard` while the handlers run and returns the guard it locks again.
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
    } else if let Some(grindstone) = guard.as_any_mut().downcast_mut::<GrindstoneScreenHandler>()
        && grindstone.take_update_event()
    {
        let inputs = grindstone.update_inputs();
        return fire_grindstone(player, handler, guard, &inputs);
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

/// Fires [`PlayerEnchantItemEvent`] for the item an enchanting table enchanted, and takes the
/// item back as the handlers left it.
pub fn fire_player_enchant_item(
    player: &Player,
    enchanted_item: &mut ItemStack,
    enchantments: &[(&'static Enchantment, i32)],
) {
    let world = player.world();
    let Some(server) = world.server.upgrade() else {
        return;
    };
    if !server
        .plugin_manager
        .has_handlers::<PlayerEnchantItemEvent>()
    {
        return;
    }
    let Some(player) = world.get_player_by_uuid(player.gameprofile.id) else {
        return;
    };
    let item = std::mem::replace(enchanted_item, ItemStack::EMPTY.clone());
    let mut event = PlayerEnchantItemEvent::new(player, item, enchantments.to_vec());
    server.plugin_manager.fire_blocking(&server, &mut event);
    *enchanted_item = event.enchanted_item;
}
