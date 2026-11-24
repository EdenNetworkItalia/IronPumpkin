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
use std::cell::{Cell, RefCell};
use std::sync::{Arc, PoisonError, Weak};

use arc_swap::ArcSwapOption;

use pumpkin_data::data_component_impl::AttributeModifiersImpl;
use pumpkin_data::item_stack::{
    AttributeModifiersHook, EnchantmentLevelHook, EnchantmentLevels, ItemStack,
    set_attribute_modifiers_hook, set_enchantment_level_hook,
};
use pumpkin_data::{Enchantment, damage::DamageType};
use pumpkin_inventory::screen_handler::ScreenHandler;
use pumpkin_protocol::java::server::play::SlotActionType;
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
static SERVERS: ArcSwapOption<Vec<Weak<Server>>> = ArcSwapOption::const_empty();

thread_local! {
    /// Set while a handler of the hooked events runs on this thread: a read inside a handler
    /// returns the levels and the modifiers before the event, so a handler cannot call itself.
    static IN_HANDLER: Cell<bool> = const { Cell::new(false) };
}

thread_local! {
    /// The answer of `ArmorSlot.mayPickup` for the stack of an armor slot click, read before the
    /// click takes the lock of the screen handler.
    static ARMOR_ANSWER: RefCell<Option<(ItemStack, bool)>> = const { RefCell::new(None) };
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

/// Keeps the pre-read answer of [`preread_armor_change`] until it drops.
pub struct ArmorAnswer;

impl Drop for ArmorAnswer {
    fn drop(&mut self) {
        ARMOR_ANSWER.with_borrow_mut(|answer| *answer = None);
    }
}

/// Reads `has_prevent_armor_change` of `stack` while no lock is held, and keeps the answer for
/// [`armor_change_answer`] on this thread. `ArmorSlot.mayPickup` asks it inside the click, under
/// the lock of the screen handler, and the read fires `GetEnchantmentLevelEvent`. Returns `None`
/// when no handler listens: the read then costs nothing.
#[must_use]
pub fn preread_armor_change(stack: &ItemStack) -> Option<ArmorAnswer> {
    if stack.is_empty() || !any_listening::<GetEnchantmentLevelEvent>() {
        return None;
    }
    let answer = stack.has_prevent_armor_change();
    ARMOR_ANSWER.with_borrow_mut(|kept| *kept = Some((stack.clone(), answer)));
    Some(ArmorAnswer)
}

/// The pre-read answer for `stack`, when [`preread_armor_change`] kept one for it.
#[must_use]
pub fn armor_change_answer(stack: &ItemStack) -> Option<bool> {
    ARMOR_ANSWER.with_borrow(|kept| {
        kept.as_ref()
            .filter(|(read, _)| read.are_equal(stack))
            .map(|(_, answer)| *answer)
    })
}

/// Whether a handler of `ItemAttributeModifierEvent` listens now.
#[must_use]
pub fn listens_to_attribute_modifiers() -> bool {
    any_listening::<ItemAttributeModifierEvent>()
}

/// Whether the event named `event_name` fires from the reads of an item stack.
#[must_use]
pub fn is_hooked_event(event_name: &str) -> bool {
    event_name == GetEnchantmentLevelEvent::get_name_static()
        || event_name == ItemAttributeModifierEvent::get_name_static()
}

/// Runs `operation` with the item stack reads of this thread firing no event.
///
/// The Wasm host runs the host work of a handler of a hooked event this way: the handler follows
/// the event chain onto a pool thread, where the thread-local guard of the fire is not set.
pub fn without_item_hooks<R>(operation: impl FnOnce() -> R) -> R {
    let _scope = HandlerScope::enter();
    operation()
}

/// Records `server`, so the hooks of the item stack reads can fire events on it. Runs in each
/// [`Server::new`].
pub fn track(server: &Arc<Server>) {
    SERVERS.rcu(|servers| {
        let mut servers: Vec<_> = servers
            .as_deref()
            .map(|servers| {
                servers
                    .iter()
                    .filter(|tracked| tracked.strong_count() > 0)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        servers.push(Arc::downgrade(server));
        Some(Arc::new(servers))
    });
}

/// Sets the hook of the item stack reads for the first handler of `event_name`.
pub fn note_registration(event_name: &str) {
    if event_name == GetEnchantmentLevelEvent::get_name_static() {
        set_enchantment_level_hook(EnchantmentLevelHook {
            active: any_listening::<GetEnchantmentLevelEvent>,
            apply: enchantment_level_hook,
        });
    } else if event_name == ItemAttributeModifierEvent::get_name_static() {
        set_attribute_modifiers_hook(AttributeModifiersHook {
            active: any_listening::<ItemAttributeModifierEvent>,
            apply: attribute_modifiers_hook,
        });
    }
}

/// Whether a server has a handler of `E` and this thread runs no handler of a hooked event.
/// Each read of an item stack calls it once the hook is set: it allocates nothing and takes no
/// lock.
fn any_listening<E: Payload + 'static>() -> bool {
    !IN_HANDLER.with(Cell::get)
        && SERVERS.load().as_deref().is_some_and(|servers| {
            servers
                .iter()
                .filter_map(Weak::upgrade)
                .any(|server| server.plugin_manager.has_handlers::<E>())
        })
}

/// The servers that have a handler of `E`.
fn listening<E: Payload + 'static>() -> Vec<Arc<Server>> {
    SERVERS
        .load()
        .as_deref()
        .map(|servers| {
            servers
                .iter()
                .filter_map(Weak::upgrade)
                .filter(|server| server.plugin_manager.has_handlers::<E>())
                .collect()
        })
        .unwrap_or_default()
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

/// The click that fires [`ItemStackedOnOtherEvent`]: the slot, the click action and the stacks of
/// the slot and of the cursor before the click.
pub struct ClickTarget {
    pub index: usize,
    action: ClickAction,
    clicked: ItemStack,
    cursor: ItemStack,
    /// The click is the pickup that ends a drag over one slot.
    pub from_drag: bool,
}

impl ClickTarget {
    /// The target of a click packet on `handler`, or `None` when vanilla fires no event for it:
    /// a pickup click with a left or right button (`doClick(.., PICKUP)`), and the end of a drag
    /// over one slot, which vanilla turns into a pickup click of that slot.
    #[must_use]
    pub fn of(
        handler: &dyn ScreenHandler,
        slot: i16,
        button: i8,
        mode: &SlotActionType,
    ) -> Option<Self> {
        let behaviour = handler.get_behaviour();
        let (index, button, from_drag) = match mode {
            SlotActionType::Pickup => (usize::try_from(slot).ok()?, button, false),
            SlotActionType::QuickCraft if button & 3 == 2 && behaviour.drag_slots.len() == 1 => {
                (behaviour.drag_slots[0] as usize, (button >> 2) & 3, true)
            }
            _ => return None,
        };
        let action = match button {
            0 => ClickAction::Primary,
            1 => ClickAction::Secondary,
            _ => return None,
        };
        Some(Self {
            index,
            action,
            clicked: behaviour.slots.get(index)?.get_cloned_stack(),
            cursor: behaviour
                .cursor_stack
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
            from_drag,
        })
    }
}

/// What [`stacked_on_other`] decided, and the stacks that its event changed.
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
    /// still hold the stacks that `target` got. It writes both changes or none: a change to the
    /// carried stack and one to the slot belong to one click.
    pub fn apply(&self, handler: &dyn ScreenHandler, target: &ClickTarget) {
        let behaviour = handler.get_behaviour();
        let mut cursor = behaviour
            .cursor_stack
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let slot = behaviour.slots.get(target.index);
        let cursor_is_unchanged = self.carried.is_none() || cursor.are_equal(&target.cursor);
        let slot_is_unchanged = self.stacked_on.is_none()
            || slot.is_some_and(|slot| slot.get_cloned_stack().are_equal(&target.clicked));
        if !cursor_is_unchanged || !slot_is_unchanged {
            return;
        }
        if let Some(new_carried) = &self.carried {
            *cursor = new_carried.clone();
        }
        if let (Some(new_stacked), Some(slot)) = (&self.stacked_on, slot) {
            slot.set_stack(new_stacked.clone());
            slot.mark_dirty();
        }
    }
}

/// Fires [`ItemStackedOnOtherEvent`] for the click `target` of the screen handler of `player`
/// (`CommonHooks.onItemStackedOn`). The caller must not hold the lock of the screen handler.
pub fn stacked_on_other(player: &Arc<Player>, target: &ClickTarget) -> StackedOnOtherChange {
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
        target.cursor.clone(),
        target.clicked.clone(),
        target.index,
        target.action,
    );
    server.plugin_manager.fire_blocking(&server, &mut event);
    let outcome = match (event.cancelled, event.cancellation_result) {
        (false, _) => StackedOnOther::Continue,
        (true, false) => StackedOnOther::SkipOverrides,
        (true, true) => StackedOnOther::Handled,
    };
    StackedOnOtherChange {
        outcome,
        carried: (!event.carried_item.are_equal(&target.cursor)).then_some(event.carried_item),
        stacked_on: (!event.stacked_on_item.are_equal(&target.clicked))
            .then_some(event.stacked_on_item),
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
