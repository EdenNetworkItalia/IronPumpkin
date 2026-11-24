//! The item, enchantment, loot level, slot stacking and brewing events of `NeoForge` 26.3 reach
//! v0.2 plugins (spec `neoforge-event-bus`, requirement "Wasm plugins see the same events",
//! #180): a v0.2 handler gets each event with its fields and writes back the settable fields,
//! and the host keeps the fields that `NeoForge` cannot set.
#![expect(
    clippy::panic,
    clippy::expect_used,
    reason = "a test fails on the first unexpected record"
)]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

mod guest;

use std::{
    borrow::Cow,
    sync::{Arc, Mutex, PoisonError},
};

use guest::{fire, plugin, resource};
use pumpkin_core::{
    entity::EntityBase,
    plugin::{
        enchantment::{
            EnchantedBlockLootEvent, EnchantedEntityLootEvent, GetEnchantmentLevelEvent,
        },
        inventory::{
            ClickAction, ItemAttributeModifierEvent, ItemStackedOnOtherEvent,
            PlayerBrewedPotionEvent,
        },
        living::DamageSource,
    },
};
use pumpkin_data::{
    AttributeModifierSlot, Block, Enchantment,
    attributes::Attributes,
    damage::DamageType,
    data_component_impl::{Modifier, ModifierDisplay, Operation},
    item::Item,
    item_stack::ItemStack,
};
use pumpkin_util::math::position::BlockPos;
use pumpkin_wasm_host_v0_2::pumpkin::plugin::{
    attributes::{Attribute, AttributeModifier, ModifierOperation},
    enchantments::{AttributeModifierSlot as WitSlot, Enchantment as WitEnchantment},
    event::Event,
    item_stack::{EnchantmentValue, ItemAttributeModifier},
    neoforge_events::ClickAction as WitClickAction,
};

fn item_id(
    state: &pumpkin_wasm_host_common::state::PluginHostState,
    res: &wasmtime::component::Resource<
        pumpkin_wasm_host_v0_2::pumpkin::plugin::item_stack::ItemStack,
    >,
) -> u16 {
    resource(state, res)
        .try_lock()
        .map(|item| item.item.id)
        .expect("unlocked stack")
}

const fn modifier(attribute: &'static Attributes, id: &'static str, amount: f64) -> Modifier {
    Modifier {
        r#type: attribute,
        id: Cow::Borrowed(id),
        amount,
        operation: Operation::AddValue,
        slot: AttributeModifierSlot::MainHand,
        display: ModifierDisplay::Hidden,
    }
}

/// A handler reads the default and the current modifiers, removes one and adds one. The host
/// keeps the display of a modifier that stays and gives the new one the default display.
#[test]
fn a_plugin_replaces_the_attribute_modifiers_of_an_item() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let manager = plugin::<ItemAttributeModifierEvent, _>(move |event, state| {
        let Event::ItemAttributeModifierEvent(data) = event else {
            panic!("expected item attribute modifiers");
        };
        record.lock().unwrap_or_else(PoisonError::into_inner).push((
            item_id(state, &data.item),
            data.default_modifiers.len(),
            data.modifiers.len(),
        ));
        data.modifiers
            .retain(|entry| entry.modifier.id != "test:speed");
        data.modifiers.push(ItemAttributeModifier {
            attribute: Attribute::Luck,
            modifier: AttributeModifier {
                id: "test:luck".to_owned(),
                amount: 2.5,
                operation: ModifierOperation::MultiplyBase,
            },
            slot: WitSlot::OffHand,
        });
    });
    let defaults = vec![
        modifier(&Attributes::ATTACK_DAMAGE, "test:damage", 4.0),
        modifier(&Attributes::ATTACK_SPEED, "test:speed", -2.0),
    ];

    let event = fire(
        &manager,
        ItemAttributeModifierEvent::new(ItemStack::new(1, &Item::STICK), defaults.clone()),
    );

    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        [(Item::STICK.id, 2, 2)]
    );
    assert_eq!(event.default_modifiers, defaults);
    assert_eq!(event.modifiers.len(), 2);
    assert_eq!(event.modifiers[0], defaults[0]);
    let luck = &event.modifiers[1];
    assert_eq!(luck.r#type, &Attributes::LUCK);
    assert_eq!(luck.id, "test:luck");
    assert_eq!(luck.amount, 2.5);
    assert_eq!(luck.operation, Operation::AddMultipliedBase);
    assert_eq!(luck.slot, AttributeModifierSlot::OffHand);
    assert_eq!(luck.display, ModifierDisplay::Default);
}

/// A handler swaps the carried stack, cancels with the result `false` and reads the slot and the
/// click action. The host keeps the player and the stack in the slot.
#[test]
fn a_plugin_changes_the_stacks_of_a_click_and_cancels_with_a_result() {
    let player = support::player("WasmStackedPlayer");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let manager = plugin::<ItemStackedOnOtherEvent, _>(move |event, state| {
        let Event::ItemStackedOnOtherEvent(data) = event else {
            panic!("expected item stacked on other");
        };
        record.lock().unwrap_or_else(PoisonError::into_inner).push((
            resource(state, &data.player).gameprofile.name.clone(),
            item_id(state, &data.carried_item),
            item_id(state, &data.stacked_on_item),
            data.slot,
            matches!(data.click_action, WitClickAction::Secondary),
            data.cancelled,
            data.cancellation_result,
        ));
        guest_replace_stack(state, &mut data.carried_item, &Item::DIAMOND);
        data.cancelled = true;
        data.cancellation_result = false;
    });

    let event = fire(
        &manager,
        ItemStackedOnOtherEvent::new(
            player,
            ItemStack::new(1, &Item::APPLE),
            ItemStack::new(3, &Item::STICK),
            12,
            ClickAction::Secondary,
        ),
    );

    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        [(
            "WasmStackedPlayer".to_owned(),
            Item::APPLE.id,
            Item::STICK.id,
            12,
            true,
            false,
            true
        )]
    );
    assert!(event.cancelled);
    assert!(!event.cancellation_result);
    assert_eq!(event.carried_item.item.id, Item::DIAMOND.id);
    assert_eq!(event.stacked_on_item.item.id, Item::STICK.id);
    assert_eq!(event.stacked_on_item.item_count, 3);
    assert_eq!(event.slot, 12);
}

/// Drops the guest handle `res` and puts a new stack of `item` in its place.
fn guest_replace_stack(
    state: &mut pumpkin_wasm_host_common::state::PluginHostState,
    res: &mut wasmtime::component::Resource<
        pumpkin_wasm_host_v0_2::pumpkin::plugin::item_stack::ItemStack,
    >,
    item: &'static Item,
) {
    guest::drop_resource(state, res);
    *res = state
        .add(Arc::new(tokio::sync::Mutex::new(ItemStack::new(1, item))))
        .expect("new stack");
}

/// A handler reads the levels and the target of a lookup and writes the levels back.
#[test]
fn a_plugin_changes_the_enchantment_levels_of_a_lookup() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let manager = plugin::<GetEnchantmentLevelEvent, _>(move |event, state| {
        let Event::GetEnchantmentLevelEvent(data) = event else {
            panic!("expected enchantment level lookup");
        };
        record.lock().unwrap_or_else(PoisonError::into_inner).push((
            item_id(state, &data.item),
            data.enchantments.len(),
            data.target_enchant
                .is_some_and(|e| matches!(e, WitEnchantment::Fortune)),
        ));
        data.enchantments = vec![EnchantmentValue {
            enchantment: WitEnchantment::Fortune,
            level: 5,
        }];
    });

    let event = fire(
        &manager,
        GetEnchantmentLevelEvent::new(
            ItemStack::new(1, &Item::STICK),
            vec![(&Enchantment::SHARPNESS, 1)],
            Some(&Enchantment::FORTUNE),
        ),
    );

    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        [(Item::STICK.id, 1, true)]
    );
    assert_eq!(
        event
            .enchantments
            .iter()
            .map(|(enchantment, level)| (enchantment.name, *level))
            .collect::<Vec<_>>(),
        [(Enchantment::FORTUNE.name, 5)]
    );
    assert_eq!(
        event.target_enchant.map(|enchantment| enchantment.name),
        Some(Enchantment::FORTUNE.name)
    );
}

/// A handler reads the block loot context and sets the level; a negative level changes nothing.
#[test]
fn a_plugin_changes_the_level_of_a_block_loot_and_the_host_ignores_a_negative_level() {
    let manager = plugin::<EnchantedBlockLootEvent, _>(|event, state| {
        let Event::EnchantedBlockLootEvent(data) = event else {
            panic!("expected enchanted block loot");
        };
        assert_eq!(
            (data.block_pos.x, data.block_pos.y, data.block_pos.z),
            (4, 70, -2)
        );
        assert_eq!(item_id(state, &data.tool), Item::STICK.id);
        assert!(matches!(data.enchantment, WitEnchantment::Fortune));
        data.enchantment_level = if data.enchantment_level == 1 { 4 } else { -3 };
    });
    let make = |level| {
        EnchantedBlockLootEvent::new(
            support::world(),
            BlockPos::new(4, 70, -2),
            Block::STONE.default_state,
            ItemStack::new(1, &Item::STICK),
            &Enchantment::FORTUNE,
            level,
        )
    };

    assert_eq!(fire(&manager, make(1)).enchantment_level, 4);
    assert_eq!(fire(&manager, make(2)).enchantment_level, 2);
}

/// A handler reads the entity and the damage source and sets the level.
#[test]
fn a_plugin_changes_the_level_of_an_entity_loot() {
    let victim = support::player("WasmLootVictim");
    let victim_id = victim.entity_id();
    let manager = plugin::<EnchantedEntityLootEvent, _>(move |event, state| {
        let Event::EnchantedEntityLootEvent(data) = event else {
            panic!("expected enchanted entity loot");
        };
        assert_eq!(
            resource(state, &data.entity).get_entity().entity_id,
            victim_id
        );
        assert!(data.damage_source.direct_entity.is_none());
        assert!(data.damage_source.causing_entity.is_none());
        assert!(matches!(data.enchantment, WitEnchantment::Looting));
        data.enchantment_level = 7;
    });
    let event = fire(
        &manager,
        EnchantedEntityLootEvent::new(
            victim as Arc<dyn EntityBase>,
            DamageSource {
                damage_type: DamageType::PLAYER_ATTACK,
                direct_entity: None,
                causing_entity: None,
                source_position: None,
            },
            &Enchantment::LOOTING,
            1,
        ),
    );

    assert_eq!(event.enchantment_level, 7);
}

/// A handler reads the player and the potion.
#[test]
fn a_plugin_receives_the_player_and_the_potion_of_a_brew_take() {
    let player = support::player("WasmBrewedPlayer");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let manager = plugin::<PlayerBrewedPotionEvent, _>(move |event, state| {
        let Event::PlayerBrewedPotionEvent(data) = event else {
            panic!("expected brewed potion");
        };
        record.lock().unwrap_or_else(PoisonError::into_inner).push((
            resource(state, &data.player).gameprofile.name.clone(),
            item_id(state, &data.stack),
        ));
    });

    fire(
        &manager,
        PlayerBrewedPotionEvent::new(player, ItemStack::new(1, &Item::POTION)),
    );

    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        [("WasmBrewedPlayer".to_owned(), Item::POTION.id)]
    );
}
