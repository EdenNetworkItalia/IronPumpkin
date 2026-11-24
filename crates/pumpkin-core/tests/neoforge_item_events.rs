//! The `NeoForge` item, enchantment, loot level, slot stacking and brewing events (issue #180):
//! `ItemAttributeModifierEvent`, `GetEnchantmentLevelEvent`, `EnchantedBlockLootEvent`,
//! `EnchantedEntityLootEvent`, `ItemStackedOnOtherEvent` and `PlayerBrewedPotionEvent`, against
//! a server of the support module.
//!
//! The handlers stay registered on the shared server, so each handler filters the events that
//! its test owns by an item that no other test uses, or by the player name.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::{
    borrow::Cow,
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use pumpkin_core::{
    data::datapack::loot_table_loader::parse_loot_table,
    entity::{EntityBase, player::Player, r#type::from_type},
    plugin::{
        BoxFuture, EventHandler, EventPriority, Payload,
        enchantment::{
            EnchantedBlockLootEvent, EnchantedEntityLootEvent, GetEnchantmentLevelEvent,
        },
        inventory::{ItemAttributeModifierEvent, ItemStackedOnOtherEvent, PlayerBrewedPotionEvent},
    },
    server::Server,
    world::loot::{LootContextParameters, generate_dynamic_loot_with_context},
};
use pumpkin_data::{
    AttributeModifierSlot, Block, Enchantment,
    attributes::Attributes,
    damage::DamageType,
    data_component_impl::{EnchantmentsImpl, Modifier, ModifierDisplay, Operation},
    entity::EntityType,
    item::Item,
    item_stack::ItemStack,
};
use pumpkin_inventory::{
    SimpleInventory, brewing::brewing_screen_handler::BrewingScreenHandler, inventory::Inventory,
    window_property::PropertyDelegate,
};
use pumpkin_protocol::{
    codec::{item_stack_seralizer::OptionalItemStackHash, var_int::VarInt},
    java::server::play::{SClickSlot, SlotActionType},
};
use pumpkin_util::{
    Hand,
    math::{position::BlockPos, vector3::Vector3},
};

type Log = Arc<Mutex<Vec<String>>>;

fn log() -> Log {
    Arc::new(Mutex::new(Vec::new()))
}

fn push(log: &Log, entry: impl Into<String>) {
    log.lock().unwrap().push(entry.into());
}

fn entries(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

/// A blocking handler that runs a closure on the event.
struct Blocking<E, F>(F, PhantomData<fn() -> E>);

impl<E, F> EventHandler<E> for Blocking<E, F>
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&mut E) + Send + Sync,
{
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut E,
    ) -> BoxFuture<'a, ()> {
        (self.0)(event);
        Box::pin(async {})
    }
}

/// Registers a blocking handler for `E` with priority `Normal`.
fn on<E, F>(handler: F)
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&mut E) + Send + Sync + 'static,
{
    support::server().plugin_manager.register_with::<E, _>(
        Arc::new(Blocking(handler, PhantomData)),
        EventPriority::Normal,
        true,
        false,
    );
}

const fn modifier(attribute: &'static Attributes, id: &'static str, amount: f64) -> Modifier {
    Modifier {
        r#type: attribute,
        id: Cow::Borrowed(id),
        amount,
        operation: Operation::AddValue,
        slot: AttributeModifierSlot::MainHand,
        display: ModifierDisplay::Default,
    }
}

/// The modifiers that a read of `stack` sees, as attribute name, id and amount.
fn modifiers(stack: &ItemStack) -> Vec<(&'static str, String, f64)> {
    stack
        .get_attribute_modifiers()
        .map(|component| {
            component
                .attribute_modifiers
                .iter()
                .map(|m| (m.r#type.name, m.id.to_string(), m.amount))
                .collect()
        })
        .unwrap_or_default()
}

/// `ItemAttributeModifierEvent` fires on a read of the attribute modifiers of a stack. A handler
/// adds a modifier (a second add with the same attribute and id fails), replaces one at its
/// position and removes the modifiers of an attribute. The stack keeps its component, and a stack
/// of another item is not touched.
#[test]
fn item_attribute_modifier_event_adds_replaces_and_removes() {
    let _serial = support::serial();
    let sword = ItemStack::new(1, &Item::GOLDEN_SWORD);
    let default = modifiers(&sword);
    assert!(
        default
            .iter()
            .any(|(attribute, _, _)| *attribute == Attributes::ATTACK_SPEED.name),
        "{default:?}"
    );
    let results = log();
    let seen = results.clone();
    on::<ItemAttributeModifierEvent, _>(move |event| {
        if event.item_stack.item.id != Item::GOLDEN_SWORD.id {
            return;
        }
        push(&seen, format!("default {}", event.default_modifiers.len()));
        let damage = event
            .default_modifiers
            .iter()
            .find(|m| m.r#type == &Attributes::ATTACK_DAMAGE)
            .unwrap()
            .clone();
        let replaced = Modifier {
            amount: 99.0,
            ..damage
        };
        event.replace_modifier(replaced);
        push(
            &seen,
            format!(
                "add {}",
                event.add_modifier(modifier(&Attributes::LUCK, "test:luck", 3.0))
            ),
        );
        push(
            &seen,
            format!(
                "add again {}",
                event.add_modifier(modifier(&Attributes::LUCK, "test:luck", 4.0))
            ),
        );
        push(
            &seen,
            format!(
                "remove {}",
                event.remove_all_modifiers_for(&Attributes::ATTACK_SPEED)
            ),
        );
    });

    let changed = modifiers(&sword);

    assert_eq!(
        entries(&results),
        [
            format!("default {}", default.len()),
            "add true".to_owned(),
            "add again false".to_owned(),
            "remove true".to_owned(),
        ]
    );
    assert_eq!(
        changed
            .iter()
            .map(|(attribute, id, amount)| (*attribute, id.as_str(), *amount))
            .collect::<Vec<_>>(),
        [
            (
                Attributes::ATTACK_DAMAGE.name,
                "minecraft:base_attack_damage",
                99.0
            ),
            (Attributes::LUCK.name, "test:luck", 3.0),
        ]
    );
    assert!(
        sword
            .get_data_component::<pumpkin_data::data_component_impl::AttributeModifiersImpl>()
            .is_some_and(|component| component.attribute_modifiers.len() == default.len())
    );
    assert_eq!(
        modifiers(&ItemStack::new(1, &Item::IRON_SWORD)).len(),
        default.len()
    );
}

/// `GetEnchantmentLevelEvent` fires on a lookup of one enchantment and on a lookup of all of
/// them. A handler changes the levels for that lookup: the component keeps its levels, and the
/// lookup inside a handler returns the levels before the event instead of calling the handler
/// again.
#[test]
fn get_enchantment_level_event_changes_the_levels_of_a_lookup() {
    let _serial = support::serial();
    let seen = log();
    let record = seen.clone();
    on::<GetEnchantmentLevelEvent, _>(move |event| {
        if event.stack.item.id != Item::WOODEN_HOE.id {
            return;
        }
        push(
            &record,
            format!(
                "target {:?}, inner {}",
                event.target_enchant.map(|e| e.name),
                event.stack.get_enchantment_level(&Enchantment::FORTUNE)
            ),
        );
        if event.is_targetting(&Enchantment::FORTUNE) {
            let level = event.level(&Enchantment::FORTUNE);
            event.set_level(&Enchantment::FORTUNE, level + 2);
        }
        if event.is_targetting(&Enchantment::MENDING) {
            event.set_level(&Enchantment::MENDING, 300);
        }
    });
    let mut hoe = ItemStack::new(1, &Item::WOODEN_HOE);

    assert_eq!(hoe.get_enchantment_level(&Enchantment::FORTUNE), 2);
    assert_eq!(hoe.get_enchantment_level(&Enchantment::EFFICIENCY), 0);
    assert_eq!(hoe.get_tag_enchantment_level(&Enchantment::FORTUNE), 0);
    hoe.add_enchantment(&Enchantment::FORTUNE, 1);
    assert_eq!(hoe.get_enchantment_level(&Enchantment::FORTUNE), 3);
    assert_eq!(hoe.get_tag_enchantment_level(&Enchantment::FORTUNE), 1);

    let all = hoe.get_all_enchantments().unwrap();
    let levels: Vec<_> = all.enchantment.iter().map(|(e, l)| (e.name, *l)).collect();
    assert_eq!(
        levels,
        [
            (Enchantment::FORTUNE.name, 3),
            (Enchantment::MENDING.name, 255)
        ]
    );
    assert_eq!(
        hoe.get_data_component::<EnchantmentsImpl>()
            .unwrap()
            .enchantment
            .len(),
        1
    );
    assert_eq!(
        ItemStack::new(1, &Item::STONE_HOE).get_enchantment_level(&Enchantment::FORTUNE),
        0
    );
    let seen = entries(&seen);
    assert_eq!(seen[0], "target Some(\"minecraft:fortune\"), inner 0");
    assert!(
        seen.iter()
            .any(|entry| entry.starts_with("target None, inner")),
        "{seen:?}"
    );
}

const FORTUNE_TABLE: &str = r#"{"type":"minecraft:block","pools":[{"rolls":1,"conditions":[{"condition":"minecraft:table_bonus","enchantment":"minecraft:fortune","chances":[0.0,1.0]}],"entries":[{"type":"minecraft:item","name":"minecraft:diamond"}]}]}"#;

/// `EnchantedBlockLootEvent` fires when the loot of a block reads an enchantment level of the
/// tool, with the tool, the origin and the state of the context. A handler changes the level that
/// the loot condition reads.
#[test]
fn enchanted_block_loot_event_changes_the_level_of_a_block_loot() {
    let _serial = support::serial();
    let table = parse_loot_table(FORTUNE_TABLE).unwrap();
    let world = support::world();
    let tool = ItemStack::new(1, &Item::GOLDEN_AXE);
    let stone = Block::STONE.default_state;
    let params = LootContextParameters {
        world: Some(&world),
        block_state: Some(stone),
        position: Some(Vector3::new(10.5, 64.2, -3.5)),
        tool: Some(tool),
        ..Default::default()
    };
    assert!(
        generate_dynamic_loot_with_context(&table, 1, &params).is_empty(),
        "no handler: the level is 0 and the chance is 0"
    );
    let seen = log();
    let record = seen.clone();
    on::<EnchantedBlockLootEvent, _>(move |event| {
        if event.tool.item.id != Item::GOLDEN_AXE.id {
            return;
        }
        push(
            &record,
            format!(
                "{} {:?} {} {}",
                event.enchantment.name,
                event.pos,
                event.state.id.as_u16() == stone.id.as_u16(),
                event.enchantment_level
            ),
        );
        event.enchantment_level = 1;
    });

    let drops = generate_dynamic_loot_with_context(&table, 1, &params);

    assert_eq!(drops.len(), 1);
    assert_eq!(
        entries(&seen),
        [format!(
            "minecraft:fortune {:?} true 0",
            BlockPos::new(10, 64, -4)
        )]
    );
}

const LOOTING_TABLE: &str = r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"minecraft:diamond","modifier":[{"type":"minecraft:set_count","count":0},{"type":"minecraft:enchanted_count_increase","enchantment":"minecraft:looting","count":1.0}]}]}]}"#;

/// `EnchantedEntityLootEvent` fires when the loot of a killed entity reads the looting level,
/// with the entity and the damage source of the context. A handler changes the level that the
/// loot function reads; a negative level is ignored.
#[test]
fn enchanted_entity_loot_event_changes_the_level_of_an_entity_loot() {
    let _serial = support::serial();
    let victim = support::player("LootVictim");
    let table = parse_loot_table(LOOTING_TABLE).unwrap();
    let params = LootContextParameters {
        this_entity: Some(victim.as_ref() as &dyn EntityBase),
        damage_type: Some(DamageType::PLAYER_ATTACK),
        ..Default::default()
    };
    let count = |params: &LootContextParameters<'_>| {
        generate_dynamic_loot_with_context(&table, 1, params)
            .first()
            .map_or(0, |stack| stack.item_count)
    };
    assert_eq!(count(&params), 0, "no handler: looting 0 adds nothing");
    let seen = log();
    let record = seen.clone();
    on::<EnchantedEntityLootEvent, _>(move |event| {
        if event.entity.get_entity().entity_id != victim_id(&event.entity) {
            return;
        }
        push(
            &record,
            format!(
                "{} {} {}",
                event.enchantment.name,
                event.damage_source.damage_type.message_id,
                event.enchantment_level
            ),
        );
        event.enchantment_level = if event.enchantment_level == 0 { 3 } else { -1 };
    });

    assert_eq!(count(&params), 3);
    assert_eq!(
        entries(&seen),
        [format!(
            "minecraft:looting {} 0",
            DamageType::PLAYER_ATTACK.message_id
        )]
    );
    // The loot context of a second roll reads 0 again, and the handler's -1 changes nothing.
    assert_eq!(count(&params), 3);
}

fn victim_id(entity: &Arc<dyn EntityBase>) -> i32 {
    entity.get_entity().entity_id
}

/// Sends a click packet for the screen with `sync_id` of `player`.
fn send(player: &Arc<Player>, sync_id: i32, slot: i16, button: i8, mode: SlotActionType) {
    let packet = SClickSlot {
        sync_id: VarInt(sync_id),
        revision: VarInt(0),
        slot,
        button,
        mode,
        length_of_array: VarInt(0),
        array_of_changed_slots: Vec::new(),
        carried_item: OptionalItemStackHash(None),
    };
    player.on_slot_click(packet, support::server());
}

/// Left-clicks `slot` of the player inventory screen (sync id 0).
fn click(player: &Arc<Player>, slot: i16) {
    send(
        player,
        0,
        slot,
        SClickSlot::BUTTON_LEFT,
        SlotActionType::Pickup,
    );
}

const SLOT: usize = 36;

/// Puts `slot_stack` in the slot 36 and `carried` on the cursor of the inventory screen.
fn set_up(player: &Arc<Player>, slot_stack: ItemStack, carried: ItemStack) {
    let handler = player.current_screen_handler.lock().unwrap().clone();
    let handler = handler.lock().unwrap();
    let behaviour = handler.get_behaviour();
    behaviour.slots[SLOT].set_stack(slot_stack);
    *behaviour.cursor_stack.lock().unwrap() = carried;
}

/// The stack in the slot 36 and the stack on the cursor.
fn read(player: &Arc<Player>) -> (ItemStack, ItemStack) {
    let handler = player.current_screen_handler.lock().unwrap().clone();
    let handler = handler.lock().unwrap();
    let behaviour = handler.get_behaviour();
    (
        behaviour.slots[SLOT].get_cloned_stack(),
        behaviour.cursor_stack.lock().unwrap().clone(),
    )
}

/// `ItemStackedOnOtherEvent` fires on a pickup click, with the two stacks and the slot, before the
/// click runs. A cancel with the result `true` ends the click; a handler that changes the carried
/// stack changes the cursor; a cancel with `false` runs the click without the bundle logic.
#[test]
fn item_stacked_on_other_event_cancels_the_click_and_changes_the_stacks() {
    let _serial = support::serial();
    let player = support::player("StackedOnPlayer");
    on::<ItemStackedOnOtherEvent, _>(|event| {
        if event.player.gameprofile.name != "StackedOnPlayer" {
            return;
        }
        match event.carried_item.item.registry_key {
            "apple" => event.cancelled = true,
            "carrot" => {
                event.carried_item = ItemStack::new(7, &Item::CARROT);
            }
            "bundle" => event.cancel_with_result(false),
            _ => {}
        }
    });
    let stick = || ItemStack::new(1, &Item::STICK);

    // No handler touches a bread click: the stacks swap.
    set_up(&player, stick(), ItemStack::new(1, &Item::BREAD));
    click(&player, SLOT as i16);
    let (slot, cursor) = read(&player);
    assert_eq!(
        (slot.item.id, cursor.item.id),
        (Item::BREAD.id, Item::STICK.id)
    );

    // A cancel with the result `true`: nothing swaps.
    set_up(&player, stick(), ItemStack::new(1, &Item::APPLE));
    click(&player, SLOT as i16);
    let (slot, cursor) = read(&player);
    assert_eq!(
        (slot.item.id, cursor.item.id),
        (Item::STICK.id, Item::APPLE.id)
    );

    // A changed carried stack goes to the cursor before the click swaps it into the slot.
    set_up(&player, stick(), ItemStack::new(1, &Item::CARROT));
    click(&player, SLOT as i16);
    let (slot, cursor) = read(&player);
    assert_eq!((slot.item.id, slot.item_count), (Item::CARROT.id, 7));
    assert_eq!(cursor.item.id, Item::STICK.id);

    // A cancel with the result `false` skips the bundle logic: a bundle on the cursor swaps with
    // the stick instead of taking it in.
    let mut bundle = ItemStack::new(1, &Item::BUNDLE);
    bundle.set_data_component(pumpkin_data::data_component_impl::BundleContentsImpl {
        items: Vec::new(),
        selected_item:
            pumpkin_data::data_component_impl::BundleContentsImpl::NO_SELECTED_ITEM_INDEX,
    });
    set_up(&player, stick(), bundle);
    click(&player, SLOT as i16);
    let (slot, cursor) = read(&player);
    assert_eq!(
        (slot.item.id, cursor.item.id),
        (Item::BUNDLE.id, Item::STICK.id)
    );
}

/// `PlayerBrewedPotionEvent` fires after the click that takes a brewed potion out of a brewing
/// stand, with the player and the potion.
#[test]
fn player_brewed_potion_event_fires_when_a_potion_is_taken() {
    let _serial = support::serial();
    let player = support::player("BrewedPlayer");
    let seen = log();
    let record = seen.clone();
    on::<PlayerBrewedPotionEvent, _>(move |event| {
        if event.player.gameprofile.name == "BrewedPlayer" {
            push(&record, event.stack.item.registry_key);
        }
    });
    let inventory = Arc::new(SimpleInventory::new(5));
    inventory.set_stack(0, ItemStack::new(1, &Item::POTION));
    let delegate: Arc<dyn PropertyDelegate> = Arc::new(NoProperties);
    let handler = Arc::new(Mutex::new(BrewingScreenHandler::new(
        1,
        &player.inventory,
        inventory.clone(),
        &delegate,
    )));
    *player.current_screen_handler.lock().unwrap() = handler;

    let packet = SClickSlot {
        sync_id: VarInt(1),
        revision: VarInt(0),
        slot: 0,
        button: SClickSlot::BUTTON_LEFT,
        mode: SlotActionType::Pickup,
        length_of_array: VarInt(0),
        array_of_changed_slots: Vec::new(),
        carried_item: OptionalItemStackHash(None),
    };
    player.on_slot_click(packet, support::server());

    assert_eq!(entries(&seen), ["potion"]);
    assert!(inventory.get_stack(0).is_empty());
}

/// Vanilla `BrewingStandMenu.quickMoveStack` calls `slot.onTake`, so a shift-click of a potion
/// fires `PlayerBrewedPotionEvent` too.
#[test]
fn player_brewed_potion_event_fires_for_a_shift_click() {
    let _serial = support::serial();
    let player = support::player("QuickBrewedPlayer");
    let seen = log();
    let record = seen.clone();
    on::<PlayerBrewedPotionEvent, _>(move |event| {
        if event.player.gameprofile.name == "QuickBrewedPlayer" {
            push(&record, event.stack.item.registry_key);
        }
    });
    let inventory = Arc::new(SimpleInventory::new(5));
    inventory.set_stack(0, ItemStack::new(1, &Item::POTION));
    let delegate: Arc<dyn PropertyDelegate> = Arc::new(NoProperties);
    let handler = Arc::new(Mutex::new(BrewingScreenHandler::new(
        1,
        &player.inventory,
        inventory.clone(),
        &delegate,
    )));
    *player.current_screen_handler.lock().unwrap() = handler;

    send(
        &player,
        1,
        0,
        SClickSlot::BUTTON_LEFT,
        SlotActionType::QuickMove,
    );

    assert!(
        inventory.get_stack(0).is_empty(),
        "the potion moved to the inventory"
    );
    assert_eq!(entries(&seen), ["potion"]);
}

/// A brewing stand screen with no property to sync.
struct NoProperties;

impl PropertyDelegate for NoProperties {
    fn get_property(&self, _index: i32) -> i32 {
        0
    }

    fn set_property(&self, _index: i32, _value: i32) {}

    fn get_properties_size(&self) -> i32 {
        0
    }
}

/// Vanilla turns a drag over one slot into `doClick(slot, PICKUP)`, which reaches
/// `tryItemClickBehaviourOverride`: `ItemStackedOnOtherEvent` fires for it too, a cancel with the
/// result `true` ends the click, and the drag ends.
#[test]
fn item_stacked_on_other_event_fires_for_a_drag_over_one_slot() {
    let _serial = support::serial();
    let player = support::player("DragStackedPlayer");
    let seen = log();
    let record = seen.clone();
    on::<ItemStackedOnOtherEvent, _>(move |event| {
        if event.player.gameprofile.name == "DragStackedPlayer" {
            push(
                &record,
                format!("{} {}", event.carried_item.item.registry_key, event.slot),
            );
            event.cancelled = true;
        }
    });
    set_up(
        &player,
        ItemStack::EMPTY.clone(),
        ItemStack::new(4, &Item::BREAD),
    );

    send(&player, 0, -999, 0, SlotActionType::QuickCraft);
    send(&player, 0, SLOT as i16, 1, SlotActionType::QuickCraft);
    send(&player, 0, -999, 2, SlotActionType::QuickCraft);

    assert_eq!(entries(&seen), ["bread 36"]);
    let (slot, cursor) = read(&player);
    assert!(
        slot.is_empty(),
        "the cancelled click put nothing in the slot"
    );
    assert_eq!(cursor.item_count, 4);
    let handler = player.current_screen_handler.lock().unwrap().clone();
    assert!(
        handler
            .lock()
            .unwrap()
            .get_behaviour()
            .drag_slots
            .is_empty()
    );
}

const SMELT_TABLE: &str = r##"{"pools":[{"entries":[{"type":"minecraft:item","name":"minecraft:beef","modifier":[{"type":"minecraft:furnace_smelt","conditions":[{"condition":"minecraft:entity_properties","entity":"direct_attacker","predicate":{"equipment":{"mainhand":{"predicates":{"minecraft:enchantments":[{"enchantments":"#minecraft:smelts_loot"}]}}}}}]}]}]}]}"##;

const SILK_TABLE: &str = r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"minecraft:diamond","conditions":[{"condition":"minecraft:match_tool","predicate":{"items":"minecraft:golden_pickaxe","predicates":{"minecraft:enchantments":[{"enchantments":"minecraft:silk_touch","levels":{"min":1}}]}}}]}]}]}"#;

/// NeoForge reads `getAllEnchantments` in the `smelts_loot` weapon check and in the
/// `minecraft:enchantments` item predicate: a handler that adds an enchantment to a stack changes
/// what the loot sees.
#[test]
fn loot_checks_read_the_enchantments_that_a_handler_adds() {
    let _serial = support::serial();
    let smelt = parse_loot_table(SMELT_TABLE).unwrap();
    let silk = parse_loot_table(SILK_TABLE).unwrap();
    let sword = ItemStack::new(1, &Item::GOLDEN_SWORD);
    let pickaxe = ItemStack::new(1, &Item::GOLDEN_PICKAXE);
    let kill = LootContextParameters {
        direct_attacker_weapon: Some(sword),
        ..Default::default()
    };
    let mine = LootContextParameters {
        tool: Some(pickaxe),
        ..Default::default()
    };
    let item_names = |table, params: &LootContextParameters<'_>| {
        generate_dynamic_loot_with_context(table, 1, params)
            .iter()
            .map(|stack| stack.item.registry_key)
            .collect::<Vec<_>>()
    };
    assert_eq!(item_names(&smelt, &kill), ["beef"]);
    assert!(item_names(&silk, &mine).is_empty());
    on::<GetEnchantmentLevelEvent, _>(|event| match event.stack.item.registry_key {
        "golden_sword" => event.set_level(&Enchantment::FIRE_ASPECT, 1),
        "golden_pickaxe" => event.set_level(&Enchantment::SILK_TOUCH, 1),
        _ => {}
    });

    assert_eq!(item_names(&smelt, &kill), ["cooked_beef"]);
    assert_eq!(item_names(&silk, &mine), ["diamond"]);
}

/// `processEquipmentDropChance` asks `getEntityLootEnchantmentLevel` for the looting level of the
/// attacker: a kill of a villager (its loot table has no looting) fires `EnchantedEntityLootEvent`
/// for the equipment drops.
#[test]
fn enchanted_entity_loot_event_fires_for_the_equipment_drops() {
    let _serial = support::serial();
    let killer = support::player("EquipmentLooter");
    let mut sword = ItemStack::new(1, &Item::GOLDEN_SWORD);
    sword.add_enchantment(&Enchantment::LOOTING, 2);
    killer.inventory().set_stack_in_hand(Hand::Right, sword);
    let world = support::world();
    let villager: Arc<dyn EntityBase> = from_type(
        &EntityType::VILLAGER,
        Vector3::new(40.5, 120.0, 40.5),
        &world,
        uuid::Uuid::new_v4(),
    );
    world.spawn_entity(villager.clone());
    let villager_id = villager.get_entity().entity_id;
    let seen = log();
    let record = seen.clone();
    on::<EnchantedEntityLootEvent, _>(move |event| {
        if event.entity.get_entity().entity_id == villager_id {
            push(
                &record,
                format!("{} {}", event.enchantment.name, event.enchantment_level),
            );
        }
    });

    villager.get_living_entity().unwrap().damage_with_context(
        villager.as_ref(),
        1000.0,
        DamageType::PLAYER_ATTACK,
        None,
        Some(killer.as_ref()),
        Some(killer.as_ref()),
    );

    assert_eq!(entries(&seen), ["minecraft:looting 2"]);
}
