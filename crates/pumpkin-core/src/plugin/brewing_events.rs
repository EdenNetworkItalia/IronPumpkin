//! The `IronPumpkin` fire sites of `PotionBrewEvent.Pre` ([`BrewEvent`]) and
//! `PotionBrewEvent.Post` ([`PotionBrewEventPost`]).
//!
//! Both events carry a copy of the stand inventory. The stand takes the stacks a handler changed
//! only when its inventory is the same as when the event was created, so a change that a player
//! made while the event fired is never lost.

use std::sync::{Arc, PoisonError, RwLockWriteGuard, atomic::Ordering};

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

fn lock(stand: &BrewingStandBlockEntity) -> RwLockWriteGuard<'_, Stacks> {
    stand.items.write().unwrap_or_else(PoisonError::into_inner)
}

/// Writes `changed` into `items` when a handler changed the stacks of `before` and `items` still
/// holds `before`. Returns whether it wrote.
fn take_changed(items: &mut Stacks, before: &Stacks, changed: Stacks) -> bool {
    let take = same_stacks(&items[..], before) && !same_stacks(&changed, before);
    if take {
        *items = changed;
    }
    take
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
    let mut items = lock(stand);
    if take_changed(&mut items, &before, event.stacks) {
        stand.mark_dirty();
    }
    fire_brew_post(stand, world, items, false);
    true
}

/// Fires [`PotionBrewEventPost`] on the stacks of `items`, the locked stand inventory, after the
/// stand brewed (`EventHooks.onPotionBrewed`). The lock is released while the handlers run. Then
/// it consumes one ingredient when `consume` is set, as `doBrew` shrinks the ingredient after the
/// event.
pub fn fire_brew_post<'a>(
    stand: &'a BrewingStandBlockEntity,
    world: &Arc<World>,
    mut items: RwLockWriteGuard<'a, Stacks>,
    consume: bool,
) {
    if let Some(server) = world.server.upgrade()
        && server.plugin_manager.has_handlers::<PotionBrewEventPost>()
    {
        let before = items.clone();
        drop(items);
        let mut event = PotionBrewEventPost::new(stand.position, before.clone());
        server.plugin_manager.fire_blocking(&server, &mut event);
        items = lock(stand);
        if take_changed(&mut items, &before, event.stacks) {
            stand.mark_dirty();
        }
    }
    if consume {
        items[3].decrement(1);
    }
}
