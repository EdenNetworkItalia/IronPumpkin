//! The `IronPumpkin` fire sites of `PotionBrewEvent.Pre` ([`BrewEvent`]) and
//! `PotionBrewEvent.Post` ([`PotionBrewEventPost`]).
//!
//! Both events carry a copy of the stand inventory. The stand takes the stacks a handler changed
//! only when its inventory is the same as when the event was created, so a change that a player
//! made while the event fired is never lost.

use std::sync::{Arc, PoisonError, atomic::Ordering};

use pumpkin_data::item_stack::ItemStack;
use pumpkin_inventory::Inventory;

use crate::block::entities::brewing_stand::BrewingStandBlockEntity;
use crate::plugin::inventory::{brew::BrewEvent, potion_brew_post::PotionBrewEventPost};
use crate::world::World;

type Stacks = [ItemStack; BrewingStandBlockEntity::INVENTORY_SIZE];

fn snapshot(stand: &BrewingStandBlockEntity) -> Stacks {
    stand
        .items
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

fn same_stacks(a: &[ItemStack], b: &[ItemStack]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.are_equal(b))
}

/// Writes `changed` into the stand when a handler changed the stacks of `before` and the stand
/// still holds `before`.
fn take_changed(stand: &BrewingStandBlockEntity, before: &Stacks, changed: Stacks) {
    let mut items = stand.items.write().unwrap_or_else(PoisonError::into_inner);
    if same_stacks(&items[..], before) && !same_stacks(&changed, before) {
        *items = changed;
        drop(items);
        stand.mark_dirty();
    }
}

/// Fires [`BrewEvent`] before the stand brews (`EventHooks.onPotionAttemptBrew`). Returns `true`
/// when a handler cancelled the brew: the stand then takes the changed stacks, and
/// [`PotionBrewEventPost`] fires.
pub fn fire_brew_pre(stand: &BrewingStandBlockEntity, world: &Arc<World>) -> bool {
    let Some(server) = world.server.upgrade() else {
        return false;
    };
    if !server.plugin_manager.has_handlers::<BrewEvent>() {
        return false;
    }
    let before = snapshot(stand);
    let mut event = BrewEvent::new(
        stand.position,
        stand.fuel.load(Ordering::Relaxed) as u8,
        before.clone(),
    );
    server.plugin_manager.fire_blocking(&server, &mut event);
    if !event.cancelled {
        return false;
    }
    take_changed(stand, &before, event.stacks);
    fire_brew_post(stand, world);
    true
}

/// Fires [`PotionBrewEventPost`] after the stand brewed (`EventHooks.onPotionBrewed`).
pub fn fire_brew_post(stand: &BrewingStandBlockEntity, world: &Arc<World>) {
    let Some(server) = world.server.upgrade() else {
        return;
    };
    if !server.plugin_manager.has_handlers::<PotionBrewEventPost>() {
        return;
    }
    let before = snapshot(stand);
    let mut event = PotionBrewEventPost::new(stand.position, before.clone());
    server.plugin_manager.fire_blocking(&server, &mut event);
    take_changed(stand, &before, event.stacks);
}
