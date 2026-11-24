//! The v0.2 conversions of the `NeoForge` 26.3 item, enchantment and brewing events.
//!
//! The events are `ItemAttributeModifierEvent`, `ItemStackedOnOtherEvent`,
//! `GetEnchantmentLevelEvent`, `EnchantedBlockLootEvent`, `EnchantedEntityLootEvent` and
//! `PlayerBrewedPotionEvent`.
//!
//! A blocking handler writes back only the settable fields; the other fields keep the native
//! values, as `NeoForge` has no setter for them.

use std::{borrow::Cow, sync::Arc};

use pumpkin_core::plugin::{
    Context, EventPriority,
    enchantment::{EnchantedBlockLootEvent, EnchantedEntityLootEvent, GetEnchantmentLevelEvent},
    inventory::{
        ClickAction, ItemAttributeModifierEvent, ItemStackedOnOtherEvent, PlayerBrewedPotionEvent,
    },
};
use pumpkin_data::data_component_impl::Modifier;
use pumpkin_wasm_host_common::state::PluginHostState;

use crate::{
    enchantment::{to_data_slot, to_wit_slot},
    events::{
        ToFromWasmEvent, WasmPluginEventHandler,
        neoforge::{add, add_item_stack, discard, take_item_stack},
        neoforge_living::{discard_damage_source, to_wasm_damage_source},
        to_wasm_block_position,
    },
    item_stack::{
        from_wit_enchantment, from_wit_item_operation, to_wit_attribute, to_wit_enchantment,
        to_wit_item_operation,
    },
    living_entity::from_wit_attribute,
    pumpkin::plugin::{
        attributes::AttributeModifier as WitAttributeModifier,
        event::{Event, EventType},
        item_stack::{EnchantmentValue, ItemAttributeModifier as WitItemAttributeModifier},
        neoforge_events::{
            ClickAction as WitClickAction, EnchantedBlockLootEventData,
            EnchantedEntityLootEventData, GetEnchantmentLevelEventData,
            ItemAttributeModifierEventData, ItemStackedOnOtherEventData,
            PlayerBrewedPotionEventData,
        },
    },
};

fn to_wit_modifier(modifier: &Modifier) -> WitItemAttributeModifier {
    WitItemAttributeModifier {
        attribute: to_wit_attribute(modifier.r#type),
        modifier: WitAttributeModifier {
            id: modifier.id.to_string(),
            amount: modifier.amount,
            operation: to_wit_item_operation(modifier.operation),
        },
        slot: to_wit_slot(&modifier.slot),
    }
}

/// The modifier that a handler returned. It keeps the display of the default modifier with the
/// same attribute and id.
fn from_wit_modifier(modifier: WitItemAttributeModifier, defaults: &[Modifier]) -> Modifier {
    let attribute = from_wit_attribute(modifier.attribute);
    let display = defaults
        .iter()
        .find(|default| default.r#type == attribute && default.id == modifier.modifier.id)
        .map(|default| default.display.clone())
        .unwrap_or_default();
    Modifier {
        r#type: attribute,
        id: Cow::Owned(modifier.modifier.id),
        amount: modifier.modifier.amount,
        operation: from_wit_item_operation(modifier.modifier.operation),
        slot: to_data_slot(modifier.slot),
        display,
    }
}

impl ToFromWasmEvent for ItemAttributeModifierEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::ItemAttributeModifierEvent(ItemAttributeModifierEventData {
            item: add_item_stack(state, &self.item_stack),
            default_modifiers: self.default_modifiers.iter().map(to_wit_modifier).collect(),
            modifiers: self.modifiers.iter().map(to_wit_modifier).collect(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        let Event::ItemAttributeModifierEvent(data) = event else {
            cleanup_neoforge_item_event(&event, state);
            return;
        };
        discard(state, &data.item);
        self.modifiers = data
            .modifiers
            .into_iter()
            .map(|modifier| from_wit_modifier(modifier, &self.default_modifiers))
            .collect();
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct ItemAttributeModifierEvent from WASM")
    }
}

impl ToFromWasmEvent for ItemStackedOnOtherEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::ItemStackedOnOtherEvent(ItemStackedOnOtherEventData {
            player: add(state, self.player.clone()),
            carried_item: add_item_stack(state, &self.carried_item),
            stacked_on_item: add_item_stack(state, &self.stacked_on_item),
            slot: u32::try_from(self.slot).unwrap_or(u32::MAX),
            click_action: match self.click_action {
                ClickAction::Primary => WitClickAction::Primary,
                ClickAction::Secondary => WitClickAction::Secondary,
            },
            cancelled: self.cancelled,
            cancellation_result: self.cancellation_result,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        let Event::ItemStackedOnOtherEvent(data) = event else {
            cleanup_neoforge_item_event(&event, state);
            return;
        };
        discard(state, &data.player);
        if let Some(carried) = take_item_stack(state, data.carried_item) {
            self.carried_item = carried;
        }
        if let Some(stacked_on) = take_item_stack(state, data.stacked_on_item) {
            self.stacked_on_item = stacked_on;
        }
        self.cancelled = data.cancelled;
        self.cancellation_result = data.cancellation_result;
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct ItemStackedOnOtherEvent from WASM")
    }
}

impl ToFromWasmEvent for GetEnchantmentLevelEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::GetEnchantmentLevelEvent(GetEnchantmentLevelEventData {
            item: add_item_stack(state, &self.stack),
            enchantments: self
                .enchantments
                .iter()
                .map(|(enchantment, level)| EnchantmentValue {
                    enchantment: to_wit_enchantment(enchantment),
                    level: u32::try_from(*level).unwrap_or(0),
                })
                .collect(),
            target_enchant: self.target_enchant.map(to_wit_enchantment),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_item_event(&event, state);
        if let Event::GetEnchantmentLevelEvent(data) = event {
            self.enchantments = data
                .enchantments
                .into_iter()
                .map(|value| {
                    (
                        from_wit_enchantment(value.enchantment),
                        i32::try_from(value.level).unwrap_or(i32::MAX),
                    )
                })
                .collect();
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct GetEnchantmentLevelEvent from WASM")
    }
}

impl ToFromWasmEvent for EnchantedBlockLootEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EnchantedBlockLootEvent(EnchantedBlockLootEventData {
            target_world: add(state, self.world.clone()),
            block_pos: to_wasm_block_position(self.pos),
            state_id: self.state.id.as_u16(),
            tool: add_item_stack(state, &self.tool),
            enchantment: to_wit_enchantment(self.enchantment),
            enchantment_level: self.enchantment_level,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_item_event(&event, state);
        if let Event::EnchantedBlockLootEvent(data) = event
            && data.enchantment_level >= 0
        {
            self.enchantment_level = data.enchantment_level;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct EnchantedBlockLootEvent from WASM")
    }
}

impl ToFromWasmEvent for EnchantedEntityLootEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EnchantedEntityLootEvent(EnchantedEntityLootEventData {
            entity: add(state, self.entity.clone()),
            damage_source: to_wasm_damage_source(state, &self.damage_source),
            enchantment: to_wit_enchantment(self.enchantment),
            enchantment_level: self.enchantment_level,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_item_event(&event, state);
        if let Event::EnchantedEntityLootEvent(data) = event
            && data.enchantment_level >= 0
        {
            self.enchantment_level = data.enchantment_level;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct EnchantedEntityLootEvent from WASM")
    }
}

impl ToFromWasmEvent for PlayerBrewedPotionEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerBrewedPotionEvent(PlayerBrewedPotionEventData {
            player: add(state, self.player.clone()),
            stack: add_item_stack(state, &self.stack),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_item_event(&event, state);
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct PlayerBrewedPotionEvent from WASM")
    }
}

/// Releases the resources of a record that the host built for a handler.
pub(super) fn cleanup_neoforge_item_event(event: &Event, state: &mut PluginHostState) {
    match event {
        Event::ItemAttributeModifierEvent(data) => discard(state, &data.item),
        Event::ItemStackedOnOtherEvent(data) => {
            discard(state, &data.player);
            discard(state, &data.carried_item);
            discard(state, &data.stacked_on_item);
        }
        Event::GetEnchantmentLevelEvent(data) => discard(state, &data.item),
        Event::EnchantedBlockLootEvent(data) => {
            discard(state, &data.target_world);
            discard(state, &data.tool);
        }
        Event::EnchantedEntityLootEvent(data) => {
            discard(state, &data.entity);
            discard_damage_source(state, &data.damage_source);
        }
        Event::PlayerBrewedPotionEvent(data) => {
            discard(state, &data.player);
            discard(state, &data.stack);
        }
        _ => {}
    }
}

/// Registers the handler of a Wasm plugin for an item, enchantment, loot level, slot stacking or
/// brewing event.
pub fn register_neoforge_item_event(
    ctx: &Context,
    handler: &Arc<WasmPluginEventHandler>,
    priority: EventPriority,
    blocking: bool,
    event_type: EventType,
) {
    fn on<E: pumpkin_core::plugin::Payload + ToFromWasmEvent + Clone + 'static>(
        ctx: &Context,
        handler: &Arc<WasmPluginEventHandler>,
        priority: EventPriority,
        blocking: bool,
    ) {
        ctx.register_event::<E, _>(Arc::clone(handler), priority, blocking);
    }

    match event_type {
        EventType::ItemAttributeModifierEvent => {
            on::<ItemAttributeModifierEvent>(ctx, handler, priority, blocking);
        }
        EventType::ItemStackedOnOtherEvent => {
            on::<ItemStackedOnOtherEvent>(ctx, handler, priority, blocking);
        }
        EventType::GetEnchantmentLevelEvent => {
            on::<GetEnchantmentLevelEvent>(ctx, handler, priority, blocking);
        }
        EventType::EnchantedBlockLootEvent => {
            on::<EnchantedBlockLootEvent>(ctx, handler, priority, blocking);
        }
        EventType::EnchantedEntityLootEvent => {
            on::<EnchantedEntityLootEvent>(ctx, handler, priority, blocking);
        }
        EventType::PlayerBrewedPotionEvent => {
            on::<PlayerBrewedPotionEvent>(ctx, handler, priority, blocking);
        }
        _ => tracing::error!(
            "non-item NeoForge event should not be routed to register_neoforge_item_event"
        ),
    }
}
