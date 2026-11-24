//! The `IronPumpkin` fire sites of the item, enchantment and slot click events of `NeoForge`:
//! `ItemAttributeModifierEvent`, `GetEnchantmentLevelEvent`, `EnchantedBlockLootEvent`,
//! `EnchantedEntityLootEvent` and `ItemStackedOnOtherEvent`.
//!
//! Each function checks `has_handlers` before it builds an event, so a fire site with no handler
//! allocates nothing and returns the vanilla value.
//!
//! An item stack has no server, so the enchantment level and attribute modifier reads
//! ([`ItemStack::get_enchantment_level`], [`ItemStack::get_all_enchantments`],
//! [`ItemStack::get_attribute_modifiers`]) call a process-wide hook. The hook is set when the
//! first handler of its event registers. Before that, a read costs one atomic load. The hook
//! fires the event on every server of the process that has a handler of it.

use std::borrow::Cow;
use std::cell::Cell;
use std::sync::{Arc, PoisonError, RwLock, Weak};

use pumpkin_data::data_component_impl::AttributeModifiersImpl;
use pumpkin_data::item_stack::{
    EnchantmentLevels, ItemStack, set_attribute_modifiers_hook, set_enchantment_level_hook,
};
use pumpkin_data::{Enchantment, damage::DamageType};
use pumpkin_inventory::screen_handler::ScreenHandler;
use pumpkin_util::math::position::BlockPos;

use crate::entity::{EntityBase, player::Player};
use crate::plugin::api::events::{
    Payload,
    enchantment::{EnchantedBlockLootEvent, EnchantedEntityLootEvent, GetEnchantmentLevelEvent},
    inventory::{ClickAction, ItemAttributeModifierEvent, ItemStackedOnOtherEvent},
    living::DamageSource,
};
use crate::server::Server;
use crate::world::loot::LootContextParameters;

/// The highest level that an item can hold (`ItemEnchantments.Mutable.set`).
const MAX_LEVEL: i32 = 255;

/// The servers of this process.
static SERVERS: RwLock<Vec<Weak<Server>>> = RwLock::new(Vec::new());

thread_local! {
    /// Set while a handler of the hooked events runs on this thread: a read inside a handler
    /// returns the levels and the modifiers before the event, so a handler cannot call itself.
    static IN_HANDLER: Cell<bool> = const { Cell::new(false) };
}

struct HandlerScope;

impl HandlerScope {
    fn enter() -> Option<Self> {
        IN_HANDLER.with(|flag| if flag.replace(true) { None } else { Some(Self) })
    }
}

impl Drop for HandlerScope {
    fn drop(&mut self) {
        IN_HANDLER.with(|flag| flag.set(false));
    }
}

/// Records `server`, so the hooks of the item stack reads can fire events on it. Runs in each
/// [`Server::new`].
pub fn track(server: &Arc<Server>) {
    let mut servers = SERVERS.write().unwrap_or_else(PoisonError::into_inner);
    servers.retain(|tracked| tracked.strong_count() > 0);
    servers.push(Arc::downgrade(server));
}

/// Sets the hook of the item stack reads for the first handler of `event_name`.
pub fn note_registration(event_name: &str) {
    if event_name == GetEnchantmentLevelEvent::get_name_static() {
        set_enchantment_level_hook(enchantment_level_hook);
    } else if event_name == ItemAttributeModifierEvent::get_name_static() {
        set_attribute_modifiers_hook(attribute_modifiers_hook);
    }
}

/// The servers that have a handler of `E`.
fn listening<E: Payload + 'static>() -> Vec<Arc<Server>> {
    SERVERS
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter_map(Weak::upgrade)
        .filter(|server| server.plugin_manager.has_handlers::<E>())
        .collect()
}

/// The hook of [`ItemStack::get_enchantment_level`] and [`ItemStack::get_all_enchantments`]:
/// fires [`GetEnchantmentLevelEvent`] and returns the levels that the handlers left.
fn enchantment_level_hook(
    stack: &ItemStack,
    target: Option<&'static Enchantment>,
    mut levels: EnchantmentLevels,
) -> EnchantmentLevels {
    let servers = listening::<GetEnchantmentLevelEvent>();
    if servers.is_empty() {
        return levels;
    }
    let Some(_scope) = HandlerScope::enter() else {
        return levels;
    };
    for server in servers {
        let mut event = GetEnchantmentLevelEvent::new(stack.clone(), levels, target);
        server.plugin_manager.fire_blocking(&server, &mut event);
        levels = event.enchantments;
    }
    normalize(levels)
}

/// Applies the rules of `ItemEnchantments.Mutable.set` to levels that a handler wrote: one entry
/// for each enchantment (the last level wins), none with a level of 0 or less, none above 255.
fn normalize(levels: EnchantmentLevels) -> EnchantmentLevels {
    let mut normalized: EnchantmentLevels = Vec::with_capacity(levels.len());
    for (enchantment, level) in levels {
        match normalized.iter_mut().find(|(enc, _)| *enc == enchantment) {
            Some(entry) => entry.1 = level,
            None => normalized.push((enchantment, level)),
        }
    }
    normalized.retain_mut(|(_, level)| {
        *level = (*level).min(MAX_LEVEL);
        *level > 0
    });
    normalized
}

/// The hook of [`ItemStack::get_attribute_modifiers`]: fires [`ItemAttributeModifierEvent`] and
/// returns the modifiers that the handlers left, or `None` when no handler changed them.
fn attribute_modifiers_hook(
    stack: &ItemStack,
    default: &AttributeModifiersImpl,
) -> Option<AttributeModifiersImpl> {
    let servers = listening::<ItemAttributeModifierEvent>();
    if servers.is_empty() {
        return None;
    }
    let _scope = HandlerScope::enter()?;
    let mut modifiers = default.attribute_modifiers.to_vec();
    for server in servers {
        let mut event = ItemAttributeModifierEvent::new(stack.clone(), modifiers);
        server.plugin_manager.fire_blocking(&server, &mut event);
        modifiers = event.modifiers;
    }
    let changed = modifiers.as_slice() != &*default.attribute_modifiers;
    changed.then_some(AttributeModifiersImpl {
        attribute_modifiers: Cow::Owned(modifiers),
    })
}

/// What a click does after [`ItemStackedOnOtherEvent`].
pub enum StackedOnOther {
    /// No handler listens, or none cancelled: the click runs as in vanilla.
    Continue,
    /// A handler cancelled with the result `false`: the click runs without the item overrides.
    SkipOverrides,
    /// A handler cancelled with the result `true`: the click ends.
    Handled,
}

/// The stacks that [`stacked_on_other`] gives to its event, and what the event changed.
pub struct StackedOnOtherChange {
    pub outcome: StackedOnOther,
    carried: Option<ItemStack>,
    stacked_on: Option<ItemStack>,
}

impl StackedOnOtherChange {
    const fn unchanged(outcome: StackedOnOther) -> Self {
        Self {
            outcome,
            carried: None,
            stacked_on: None,
        }
    }

    /// Writes the stacks that a handler changed into `handler`, when its cursor and the slot
    /// still hold the stacks that `stacked_on_other` got.
    pub fn apply(
        &self,
        handler: &dyn ScreenHandler,
        slot: usize,
        carried: &ItemStack,
        stacked_on: &ItemStack,
    ) {
        let behaviour = handler.get_behaviour();
        if let Some(new_carried) = &self.carried {
            let mut cursor = behaviour
                .cursor_stack
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if cursor.are_equal(carried) {
                *cursor = new_carried.clone();
            }
        }
        if let (Some(new_stacked), Some(slot)) = (&self.stacked_on, behaviour.slots.get(slot))
            && slot.get_cloned_stack().are_equal(stacked_on)
        {
            slot.set_stack(new_stacked.clone());
            slot.mark_dirty();
        }
    }
}

/// Fires [`ItemStackedOnOtherEvent`] for a pickup click on `slot` of the screen handler of
/// `player` (`CommonHooks.onItemStackedOn`). `carried` and `stacked_on` are the stacks of the
/// cursor and of the slot before the click. The caller must not hold the lock of the screen
/// handler.
pub fn stacked_on_other(
    player: &Arc<Player>,
    slot: usize,
    click_action: ClickAction,
    carried: &ItemStack,
    stacked_on: &ItemStack,
) -> StackedOnOtherChange {
    let unchanged = StackedOnOtherChange::unchanged(StackedOnOther::Continue);
    let Some(server) = player.world().server.upgrade() else {
        return unchanged;
    };
    if !server
        .plugin_manager
        .has_handlers::<ItemStackedOnOtherEvent>()
    {
        return unchanged;
    }
    let mut event = ItemStackedOnOtherEvent::new(
        player.clone(),
        carried.clone(),
        stacked_on.clone(),
        slot,
        click_action,
    );
    server.plugin_manager.fire_blocking(&server, &mut event);
    let outcome = match (event.cancelled, event.cancellation_result) {
        (false, _) => StackedOnOther::Continue,
        (true, false) => StackedOnOther::SkipOverrides,
        (true, true) => StackedOnOther::Handled,
    };
    StackedOnOtherChange {
        outcome,
        carried: (!event.carried_item.are_equal(carried)).then_some(event.carried_item),
        stacked_on: (!event.stacked_on_item.are_equal(stacked_on)).then_some(event.stacked_on_item),
    }
}

/// Fires [`EnchantedBlockLootEvent`] for the loot of a block (`EventHooks.getBlockLootEnchantmentLevel`)
/// and returns the level that the loot function or condition reads. `level` is the level of
/// `tool`. The event needs the block state and the origin of the loot context, as in `NeoForge`.
pub fn block_loot_enchantment_level(
    params: &LootContextParameters<'_>,
    tool: &ItemStack,
    enchantment: &'static Enchantment,
    level: i32,
) -> i32 {
    let (Some(world), Some(state), Some(origin)) =
        (params.world, params.block_state, params.position)
    else {
        return level;
    };
    let Some(server) = world.server.upgrade() else {
        return level;
    };
    if !server
        .plugin_manager
        .has_handlers::<EnchantedBlockLootEvent>()
    {
        return level;
    }
    let mut event = EnchantedBlockLootEvent::new(
        server.get_world_from_dimension(&world.dimension),
        BlockPos::floored(origin.x, origin.y, origin.z),
        state,
        tool.clone(),
        enchantment,
        level,
    );
    server.plugin_manager.fire_blocking(&server, &mut event);
    keep_if_not_negative(event.enchantment_level, level)
}

/// Fires [`EnchantedEntityLootEvent`] for the loot of an entity
/// (`EventHooks.getEntityLootEnchantmentLevel`) and returns the level that the loot function or
/// condition reads. The event needs a living entity and a damage type in the loot context, as
/// `NeoForge` needs `THIS_ENTITY` and `DAMAGE_SOURCE`.
pub fn entity_loot_enchantment_level(
    params: &LootContextParameters<'_>,
    enchantment: &'static Enchantment,
    level: i32,
) -> i32 {
    let (Some(this), Some(damage_type)) = (params.this_entity, params.damage_type) else {
        return level;
    };
    if this.get_living_entity().is_none() {
        return level;
    }
    let world = this.get_entity().world.load();
    let Some(server) = world.server.upgrade() else {
        return level;
    };
    if !server
        .plugin_manager
        .has_handlers::<EnchantedEntityLootEvent>()
    {
        return level;
    }
    let Some(entity) = crate::plugin::entity_events::live(&world, this.get_entity()) else {
        return level;
    };
    let mut event = EnchantedEntityLootEvent::new(
        entity,
        damage_source(&world, damage_type, params),
        enchantment,
        level,
    );
    server.plugin_manager.fire_blocking(&server, &mut event);
    keep_if_not_negative(event.enchantment_level, level)
}

fn damage_source(
    world: &crate::world::World,
    damage_type: DamageType,
    params: &LootContextParameters<'_>,
) -> DamageSource {
    let find = |entity: Option<&dyn EntityBase>| {
        entity.and_then(|entity| crate::plugin::entity_events::live(world, entity.get_entity()))
    };
    DamageSource {
        damage_type,
        direct_entity: find(params.direct_killer_entity),
        causing_entity: find(params.killer_entity),
        source_position: None,
    }
}

/// `setEnchantmentLevel` throws for a negative level: the host keeps the level before the event.
const fn keep_if_not_negative(new: i32, before: i32) -> i32 {
    if new < 0 { before } else { new }
}
