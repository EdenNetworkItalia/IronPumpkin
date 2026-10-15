//! The `NeoForge` event bus as Pumpkin events (`openspec/changes/neoforge-shaped-plugin-api`,
//! spec `neoforge-event-bus`): the dispatch order, receive-canceled, the parent events, the
//! guardrail of no event without a handler, and one test per event row of the first slice of the
//! catalogue, against a server of the support module.
//!
//! Scenarios of `specs/neoforge-event-bus/spec.md` and the tests that cover them:
//!
//! - [x] Handler on a parent: `handler_on_a_parent` (with test events: no event of this slice has
//!   a concrete parent)
//! - [ ] Lifecycle method of two mods: task 2.2
//! - [ ] Game event reaches every mod: task 2.2
//! - [ ] Too deep a nesting in a Wasm plugin: task 2.18
//! - [x] Fire from a native handler: `fire_from_a_native_handler`
//! - [x] Receive canceled: `receive_canceled`
//! - [x] Priority order: `priority_order`
//! - [x] Blocking before non-blocking: `blocking_before_non_blocking`
//! - [x] World call from a handler: `world_call_from_a_handler`
//! - [x] Change a settable field: `change_a_settable_field_anvil_update_event` (the slice has no
//!   `LivingHealEvent`; the anvil output is its settable field)
//! - [x] Cancel a block break: `cancel_server_chat_event` (the slice has no `BlockBreakEvent`; the
//!   chat cancel is its cancel test)
//! - [ ] Deny a despawn: no event of this slice has a result
//! - [ ] Late call: task 2.2
//! - [ ] Player login: task 2.12
//! - [x] Missing field added to a Pumpkin event: the `one_event_per_fire_site_*` tests check the
//!   added fields of each mapped event
//! - [ ] A mod cancels the break of another mod's block: task 2.12
//! - [x] One test per event family (server, tick and root package events): write-back
//!   `change_a_settable_field_anvil_update_event`, cancel `cancel_server_chat_event`
//! - [ ] A Wasm plugin cancels the break of a native mod's block: task 2.18
//! - [ ] A native handler cancels, a Wasm handler observes: task 2.18
//! - [ ] A Wasm handler cancels, a native handler observes: task 2.18
//! - [ ] One event pipeline scenarios: task 2.4
//!
//! `server_without_listeners_builds_no_event` covers the scenario "Server without listeners" of
//! `specs/neoforge-native-channel/spec.md`. The boot test `neoforge_lifecycle_events` of
//! `ironpumpkin-test-mod` covers the rows of the server lifecycle and the server tick.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::{
    any::Any,
    marker::PhantomData,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use pumpkin_core::{
    block::entities::{BlockEntity, brewing_stand::BrewingStandBlockEntity},
    command::argument_builder::command,
    entity::player::Player,
    net::ClientPlatform,
    plugin::{
        BoxFuture, Cancellable, EventHandler, EventPriority, ParentEventHandler, Payload,
        PluginManager, PluginMetadata,
        block::brewing_start::BrewingStartEvent,
        enchantment::{PlayerEnchantItemEvent, PrepareItemEnchantEvent},
        inventory::{
            brew::BrewEvent, prepare_anvil::PrepareAnvilEvent,
            prepare_grindstone::PrepareGrindstoneEvent,
        },
        player::{
            player_chat::PlayerChatEvent, player_statistic_increment::PlayerStatisticIncrementEvent,
        },
        startup::create_context,
        world::{
            generic_game::GenericGameEvent,
            level_tick::{LevelTickEventPost, LevelTickEventPre},
        },
    },
    server::Server,
};
use pumpkin_data::{
    Enchantment,
    data_component_impl::PotionContentsImpl,
    item::Item,
    item_stack::ItemStack,
    potion::Potion,
    statistic::{CustomStatistic, StatisticCategory},
};
use pumpkin_inventory::{
    SimpleInventory, anvil::AnvilScreenHandler,
    enchanting::enchanting_screen_handler::EnchantingTableScreenHandler,
    grindstone_screen_handler::GrindstoneScreenHandler, inventory::Inventory,
    screen_handler::ScreenHandler,
};
use pumpkin_protocol::{codec::var_int::VarInt, java::server::play::SChatMessage};
use pumpkin_util::{GameMode, math::position::BlockPos, math::vector3::Vector3};

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

fn blocking<E, F: Fn(&mut E) + Send + Sync>(handler: F) -> Arc<Blocking<E, F>> {
    Arc::new(Blocking(handler, PhantomData))
}

/// A non-blocking handler that runs a closure on the event.
struct Observer<E, F>(F, PhantomData<fn() -> E>);

impl<E, F> EventHandler<E> for Observer<E, F>
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&E) + Send + Sync,
{
    fn handle<'a>(&'a self, _server: &'a Arc<Server>, event: &'a E) -> BoxFuture<'a, ()> {
        (self.0)(event);
        Box::pin(async {})
    }
}

fn observer<E, F: Fn(&E) + Send + Sync>(handler: F) -> Arc<Observer<E, F>> {
    Arc::new(Observer(handler, PhantomData))
}

/// A cancellable event that only these tests fire.
#[derive(Default)]
struct BusTestEvent {
    cancelled: bool,
}

impl Payload for BusTestEvent {
    fn get_name_static() -> &'static str {
        "NeoForgeBusTestEvent"
    }

    fn get_name(&self) -> &'static str {
        Self::get_name_static()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl Cancellable for BusTestEvent {
    fn cancelled(&self) -> bool {
        self.cancelled
    }

    fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// Registers a blocking handler on `manager` that logs `name`.
fn logging(manager: &PluginManager, log: &Log, name: &'static str, priority: EventPriority) {
    let log = log.clone();
    manager.register_with::<BusTestEvent, _>(
        blocking(move |_: &mut BusTestEvent| push(&log, name)),
        priority,
        true,
        false,
    );
}

#[test]
fn priority_order() {
    let manager = PluginManager::new(Vec::new());
    let log = log();
    logging(&manager, &log, "low", EventPriority::Low);
    logging(&manager, &log, "high", EventPriority::High);
    logging(&manager, &log, "low-second", EventPriority::Low);

    manager.fire_blocking(support::server(), &mut BusTestEvent::default());

    assert_eq!(entries(&log), ["high", "low", "low-second"]);
}

#[test]
fn blocking_before_non_blocking() {
    let manager = PluginManager::new(Vec::new());
    let log = log();
    let observed = log.clone();
    manager.register_with::<BusTestEvent, _>(
        observer(move |_: &BusTestEvent| push(&observed, "non-blocking highest")),
        EventPriority::Highest,
        false,
        false,
    );
    logging(&manager, &log, "blocking lowest", EventPriority::Lowest);

    manager.fire_blocking(support::server(), &mut BusTestEvent::default());

    assert_eq!(entries(&log), ["blocking lowest", "non-blocking highest"]);
}

#[test]
fn receive_canceled() {
    let manager = PluginManager::new(Vec::new());
    let log = log();
    let canceller = log.clone();
    manager.register_with::<BusTestEvent, _>(
        blocking(move |event: &mut BusTestEvent| {
            push(&canceller, "cancel");
            event.set_cancelled(true);
        }),
        EventPriority::High,
        true,
        false,
    );
    logging(
        &manager,
        &log,
        "without receive-canceled",
        EventPriority::Normal,
    );
    let receiver = log.clone();
    manager.register_with::<BusTestEvent, _>(
        blocking(move |event: &mut BusTestEvent| {
            push(
                &receiver,
                format!("with receive-canceled {}", event.cancelled),
            );
        }),
        EventPriority::Normal,
        true,
        true,
    );

    let mut event = BusTestEvent::default();
    manager.fire_blocking(support::server(), &mut event);

    assert!(event.cancelled);
    assert_eq!(entries(&log), ["cancel", "with receive-canceled true"]);
}

/// A concrete parent event and a subclass, as `EntityTeleportEvent` and `EnderPearl`.
struct ParentTestEvent;

struct ChildTestEvent {
    value: i32,
}

macro_rules! test_payload {
    ($ty:ty, $name:literal, $parents:expr) => {
        impl Payload for $ty {
            fn get_name_static() -> &'static str {
                $name
            }

            fn get_name(&self) -> &'static str {
                $name
            }

            fn as_any(&self) -> &dyn Any {
                self
            }

            fn as_any_mut(&mut self) -> &mut dyn Any {
                self
            }

            fn get_parent_names_static() -> &'static [&'static str] {
                $parents
            }
        }
    };
}

test_payload!(ParentTestEvent, "NeoForgeBusTestParent", &[]);
test_payload!(
    ChildTestEvent,
    "NeoForgeBusTestChild",
    &["NeoForgeBusTestParent"]
);

struct ParentProbe(Log);

impl ParentEventHandler for ParentProbe {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut (dyn Payload + Send + Sync),
    ) -> BoxFuture<'a, ()> {
        let name = event.get_name();
        if let Some(child) = <dyn Payload>::downcast_mut::<ChildTestEvent>(event) {
            child.value = 7;
        }
        push(&self.0, format!("parent handler got {name}"));
        Box::pin(async {})
    }
}

#[test]
fn handler_on_a_parent() {
    let manager = PluginManager::new(Vec::new());
    let log = log();
    manager.register_parent::<ParentTestEvent, _>(
        Arc::new(ParentProbe(log.clone())),
        EventPriority::High,
        true,
        false,
    );
    assert!(manager.has_handlers::<ChildTestEvent>());

    let child_log = log.clone();
    manager.register_with::<ChildTestEvent, _>(
        blocking(move |event: &mut ChildTestEvent| {
            push(&child_log, format!("child handler saw {}", event.value));
        }),
        EventPriority::Low,
        true,
        false,
    );

    let mut child = ChildTestEvent { value: 0 };
    manager.fire_blocking(support::server(), &mut child);
    manager.fire_blocking(support::server(), &mut ParentTestEvent);

    assert_eq!(child.value, 7);
    assert_eq!(
        entries(&log),
        [
            "parent handler got NeoForgeBusTestChild",
            "child handler saw 7",
            "parent handler got NeoForgeBusTestParent",
        ]
    );
}

#[test]
fn derived_events_report_cancel() {
    let mut event = BrewEvent::new(
        BlockPos::new(0, 0, 0),
        0,
        std::array::from_fn(|_| ItemStack::EMPTY.clone()),
    );
    assert!(!event.is_cancelled());
    event.set_cancelled(true);
    assert!(event.is_cancelled());
}

#[test]
fn server_without_listeners_builds_no_event() {
    let manager = PluginManager::new(Vec::new());
    let built = AtomicUsize::new(0);
    let world = support::world();

    assert!(!manager.has_handlers::<LevelTickEventPre>());
    let fired = manager.fire_blocking_with(support::server(), || {
        built.fetch_add(1, Ordering::Relaxed);
        LevelTickEventPre::new(world.clone())
    });

    assert!(fired.is_none());
    assert_eq!(built.load(Ordering::Relaxed), 0);
    assert!(!manager.has_handlers::<ChildTestEvent>());
}

#[test]
fn fire_from_a_native_handler() {
    let manager = Arc::new(PluginManager::new(Vec::new()));
    let config = Arc::new(Mutex::new(3));
    let log = log();
    let inner_log = log.clone();
    manager.register_with::<ChildTestEvent, _>(
        blocking(move |event: &mut ChildTestEvent| {
            push(&inner_log, format!("inner {}", event.value));
        }),
        EventPriority::Normal,
        true,
        false,
    );
    let outer_log = log.clone();
    let outer_manager = manager.clone();
    let outer_config = config;
    manager.register_with::<BusTestEvent, _>(
        blocking(move |_: &mut BusTestEvent| {
            // The lock of the mod's own state ends before the nested fire.
            let value = *outer_config.lock().unwrap();
            outer_manager.fire_blocking(support::server(), &mut ChildTestEvent { value });
            push(
                &outer_log,
                format!("outer read {}", *outer_config.lock().unwrap()),
            );
        }),
        EventPriority::Normal,
        true,
        false,
    );

    manager.fire_blocking(support::server(), &mut BusTestEvent::default());

    assert_eq!(entries(&log), ["inner 3", "outer read 3"]);
}

#[test]
fn world_call_from_a_handler() {
    let manager = PluginManager::new(Vec::new());
    let log = log();
    let world_log = log.clone();
    manager.register_with::<BusTestEvent, _>(
        blocking(move |_: &mut BusTestEvent| {
            let world = support::world();
            let state = futures::executor::block_on(tokio::task::unconstrained(
                world.get_block_state_async(&BlockPos::new(0, -64, 0)),
            ));
            push(
                &world_log,
                format!(
                    "bedrock {}",
                    state.id == pumpkin_data::Block::BEDROCK.default_state.id
                ),
            );
        }),
        EventPriority::High,
        true,
        false,
    );
    logging(&manager, &log, "next handler", EventPriority::Low);

    manager.fire_blocking(support::server(), &mut BusTestEvent::default());

    assert_eq!(entries(&log), ["bedrock true", "next handler"]);
}

/// The anvil of `player`: a damaged sword on the left and a diamond on the right.
fn anvil(player: &Player) -> (AnvilScreenHandler, Arc<SimpleInventory>) {
    let inventory = Arc::new(SimpleInventory::new(3));
    let handler = AnvilScreenHandler::new(1, &player.inventory, inventory.clone());
    let mut sword = ItemStack::new(1, &Item::DIAMOND_SWORD);
    sword.set_damage(1000);
    inventory.set_stack(0, sword);
    inventory.set_stack(1, ItemStack::new(3, &Item::DIAMOND));
    (handler, inventory)
}

#[test]
fn change_a_settable_field_anvil_update_event() {
    let _serial = support::serial();
    let player = support::player("AnvilUser");
    let cancel = Arc::new(AtomicBool::new(false));
    let seen = log();
    let (cancel_flag, seen_by_handler) = (cancel.clone(), seen.clone());
    support::server()
        .plugin_manager
        .register_with::<PrepareAnvilEvent, _>(
            blocking(move |event: &mut PrepareAnvilEvent| {
                if event.player.gameprofile.name != "AnvilUser" {
                    return;
                }
                push(
                    &seen_by_handler,
                    format!(
                        "{} {} {} {} {}",
                        event.left.item.registry_key,
                        event.right.item.registry_key,
                        event.vanilla_output.item.registry_key,
                        event.vanilla_repair_cost,
                        event.vanilla_material_cost,
                    ),
                );
                if cancel_flag.load(Ordering::Relaxed) {
                    event.set_cancelled(true);
                    return;
                }
                event.output = ItemStack::new(1, &Item::NETHER_STAR);
                event.repair_cost = 7;
                event.material_cost = 1;
            }),
            EventPriority::Normal,
            true,
            false,
        );

    let (mut handler, inventory) = anvil(&player);
    handler.update_result(player.as_ref());
    assert_eq!(inventory.get_stack(2).item, &Item::NETHER_STAR);
    assert_eq!(handler.get_cost(), 7);
    assert_eq!(handler.repair_item_count_cost.load(Ordering::Relaxed), 1);
    // Vanilla repairs the sword with 3 diamonds: 3 levels, 3 diamonds.
    assert_eq!(entries(&seen), ["diamond_sword diamond diamond_sword 3 3"]);

    cancel.store(true, Ordering::Relaxed);
    let (mut handler, inventory) = anvil(&player);
    handler.update_result(player.as_ref());
    assert!(inventory.get_stack(2).is_empty());
    assert_eq!(handler.get_cost(), 0);
    assert_eq!(handler.repair_item_count_cost.load(Ordering::Relaxed), 0);
}

/// Sends `message` as a chat message of `player` and returns how many bytes the server queued
/// for the player.
fn chat(player: &Arc<Player>, message: &str) -> usize {
    let ClientPlatform::Java(client) = player.client.as_ref() else {
        panic!("the support joins Java players");
    };
    let before = client.pending_bytes.load(Ordering::Relaxed);
    let packet = SChatMessage {
        message,
        timestamp: 0,
        salt: 0,
        signature: None,
        message_count: VarInt(0),
        acknowledged: &[0; 3],
        checksum: 0,
    };
    support::RUNTIME.block_on(client.handle_chat_message(support::server(), player, packet));
    client.pending_bytes.load(Ordering::Relaxed) - before
}

#[test]
fn cancel_server_chat_event() {
    let _serial = support::serial();
    let player = support::player("Chatter");
    let seen = log();
    let seen_by_handler = seen.clone();
    let manager = &support::server().plugin_manager;
    manager.register_with::<PlayerChatEvent, _>(
        blocking(move |event: &mut PlayerChatEvent| {
            if event.username() != "Chatter" {
                return;
            }
            if event.raw_text == "cancel me" {
                event.set_cancelled(true);
            } else {
                event.message = "rewritten".to_owned();
            }
        }),
        EventPriority::High,
        true,
        false,
    );
    manager.register_with::<PlayerChatEvent, _>(
        blocking(move |event: &mut PlayerChatEvent| {
            if event.username() == "Chatter" {
                push(
                    &seen_by_handler,
                    format!("{} -> {}", event.raw_text, event.message),
                );
            }
        }),
        EventPriority::Low,
        true,
        false,
    );

    assert!(chat(&player, "hello") > 0);
    assert_eq!(chat(&player, "cancel me"), 0);
    assert_eq!(entries(&seen), ["hello -> rewritten"]);
}

/// The grindstone of `player` with an enchanted sword in the top slot.
fn grindstone(player: &Player) -> (GrindstoneScreenHandler, Arc<SimpleInventory>) {
    let inventory = Arc::new(SimpleInventory::new(3));
    let handler = GrindstoneScreenHandler::new(1, &player.inventory, inventory.clone());
    let mut sword = ItemStack::new(1, &Item::IRON_SWORD);
    sword.add_enchantment(&Enchantment::SHARPNESS, 2);
    inventory.set_stack(0, sword);
    (handler, inventory)
}

#[test]
fn one_event_per_fire_site_grindstone_event_on_place_item() {
    let _serial = support::serial();
    let player = support::player("Grinder");
    let mode = Arc::new(AtomicUsize::new(0));
    let seen = log();
    let (handler_mode, seen_by_handler) = (mode.clone(), seen.clone());
    support::server()
        .plugin_manager
        .register_with::<PrepareGrindstoneEvent, _>(
            blocking(move |event: &mut PrepareGrindstoneEvent| {
                if event.player.gameprofile.name != "Grinder" {
                    return;
                }
                push(
                    &seen_by_handler,
                    format!(
                        "{} {} {:?} {}",
                        event.top.item.registry_key,
                        event.bottom.is_empty(),
                        event.result_item,
                        event.xp
                    ),
                );
                match handler_mode.load(Ordering::Relaxed) {
                    1 => {
                        event.output = ItemStack::new(1, &Item::STICK);
                        event.xp = 5;
                    }
                    2 => event.set_cancelled(true),
                    _ => {}
                }
            }),
            EventPriority::Normal,
            true,
            false,
        );

    // No output: the vanilla result, an unenchanted sword, and the vanilla experience.
    let (mut handler, inventory) = grindstone(&player);
    handler.update_result(player.as_ref());
    let result = inventory.get_stack(2);
    assert_eq!(result.item, &Item::IRON_SWORD);
    assert!(!result.has_enchantments());
    assert_eq!(handler.xp.load(Ordering::Relaxed), -1);

    mode.store(1, Ordering::Relaxed);
    let (mut handler, inventory) = grindstone(&player);
    handler.update_result(player.as_ref());
    assert_eq!(inventory.get_stack(2).item, &Item::STICK);
    assert_eq!(handler.xp.load(Ordering::Relaxed), 5);

    mode.store(2, Ordering::Relaxed);
    let (mut handler, inventory) = grindstone(&player);
    handler.update_result(player.as_ref());
    assert!(inventory.get_stack(2).is_empty());

    assert_eq!(
        entries(&seen),
        [r#"iron_sword true Some("iron_sword") -1"#; 3]
    );
}

#[test]
fn one_event_per_fire_site_stat_award_event() {
    let _serial = support::serial();
    let player = support::player("Jumper");
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_flag = cancel.clone();
    support::server()
        .plugin_manager
        .register_with::<PlayerStatisticIncrementEvent, _>(
            blocking(move |event: &mut PlayerStatisticIncrementEvent| {
                if event.player.gameprofile.name != "Jumper" {
                    return;
                }
                if cancel_flag.load(Ordering::Relaxed) {
                    event.set_cancelled(true);
                    return;
                }
                assert_eq!(event.category, StatisticCategory::Custom);
                assert_eq!(event.stat, CustomStatistic::Jump as i32);
                event.stat = CustomStatistic::EnchantItem as i32;
                event.amount = 5;
            }),
            EventPriority::Normal,
            true,
            false,
        );

    player.increment_stat(StatisticCategory::Custom, CustomStatistic::Jump as i32, 1);
    assert_eq!(
        player.get_stat(StatisticCategory::Custom, CustomStatistic::Jump as i32),
        0
    );
    let enchanted = CustomStatistic::EnchantItem as i32;
    assert_eq!(player.get_stat(StatisticCategory::Custom, enchanted), 5);

    cancel.store(true, Ordering::Relaxed);
    player.increment_stat(StatisticCategory::Custom, enchanted, 1);
    assert_eq!(player.get_stat(StatisticCategory::Custom, enchanted), 5);
}

#[test]
fn one_event_per_fire_site_vanilla_game_event() {
    let _serial = support::serial();
    let seen = log();
    let seen_by_handler = seen.clone();
    support::server()
        .plugin_manager
        .register_with::<GenericGameEvent, _>(
            blocking(move |event: &mut GenericGameEvent| {
                push(
                    &seen_by_handler,
                    format!("{} {:?}", event.event_key, event.position),
                );
            }),
            EventPriority::Normal,
            true,
            false,
        );

    support::world().emit_game_event("block_activate", Vector3::new(0.5, 64.5, 0.5));

    assert_eq!(
        entries(&seen),
        ["block_activate Vector3 { x: 0.5, y: 64.5, z: 0.5 }"]
    );
}

fn water_bottle() -> ItemStack {
    let mut bottle = ItemStack::new(1, &Item::POTION);
    bottle.set_data_component(PotionContentsImpl {
        potion_id: Some(i32::from(Potion::WATER.id)),
        custom_color: None,
        custom_effects: Vec::new(),
        custom_name: None,
    });
    bottle
}

/// Ticks a brewing stand with water, nether wart and blaze powder until it brews. A handler of
/// `BrewingStartEvent` shortens the brew to one tick.
fn brew(position: BlockPos) -> [ItemStack; 5] {
    let world = support::world();
    let stand = BrewingStandBlockEntity::new(position);
    // The stand has no block in the world: with the bottles it already shows, it does not
    // update the block state.
    *stand.last_potion_count.lock().unwrap() = Some([true, true, false]);
    *stand.items.write().unwrap() = [
        water_bottle(),
        water_bottle(),
        ItemStack::EMPTY.clone(),
        ItemStack::new(1, &Item::NETHER_WART),
        ItemStack::new(1, &Item::BLAZE_POWDER),
    ];
    stand.tick(&world);
    stand.tick(&world);
    stand.items.read().unwrap().clone()
}

#[test]
fn one_event_per_fire_site_potion_brew_event_pre() {
    let _serial = support::serial();
    let manager = &support::server().plugin_manager;
    manager.register_with::<BrewingStartEvent, _>(
        blocking(|event: &mut BrewingStartEvent| event.brewing_time = 1),
        EventPriority::Normal,
        true,
        false,
    );
    let cancelled_at = BlockPos::new(8, 64, 8);
    manager.register_with::<BrewEvent, _>(
        blocking(move |event: &mut BrewEvent| {
            assert_eq!(event.stacks[3].item, &Item::NETHER_WART);
            event.stacks[2] = ItemStack::new(1, &Item::DIAMOND);
            if event.block_pos == cancelled_at {
                event.set_cancelled(true);
            }
        }),
        EventPriority::Normal,
        true,
        false,
    );

    // Not cancelled: vanilla brews and ignores the stacks of the handler.
    let items = brew(BlockPos::new(4, 64, 4));
    let awkward = Some(i32::from(Potion::AWKWARD.id));
    let potion = |stack: &ItemStack| {
        stack
            .get_data_component::<PotionContentsImpl>()
            .and_then(|contents| contents.potion_id)
    };
    assert_eq!(potion(&items[0]), awkward);
    assert!(items[2].is_empty());
    assert!(items[3].is_empty());

    // Cancelled: no brew, and the stand takes the stacks of the handler.
    let items = brew(cancelled_at);
    assert_eq!(items[2].item, &Item::DIAMOND);
    assert_eq!(potion(&items[0]), Some(i32::from(Potion::WATER.id)));
    assert_eq!(items[3].item, &Item::NETHER_WART);
}

#[test]
fn one_event_per_fire_site_enchantment_events() {
    let _serial = support::serial();
    let player = support::player("Enchanter");
    player.gamemode.store(GameMode::Creative);
    let original = Arc::new(Mutex::new([0; 3]));
    let applied = log();
    let manager = &support::server().plugin_manager;
    let original_by_handler = original.clone();
    manager.register_with::<PrepareItemEnchantEvent, _>(
        blocking(move |event: &mut PrepareItemEnchantEvent| {
            if event.player.gameprofile.name != "Enchanter" {
                return;
            }
            assert_eq!(event.level_requirements, event.original_level_requirements);
            assert_eq!(event.bookshelf_count, 15);
            *original_by_handler.lock().unwrap() = event.original_level_requirements;
            event.level_requirements[0] = 1;
        }),
        EventPriority::Normal,
        true,
        false,
    );
    let applied_by_handler = applied.clone();
    manager.register_with::<PlayerEnchantItemEvent, _>(
        blocking(move |event: &mut PlayerEnchantItemEvent| {
            if event.player.gameprofile.name != "Enchanter" {
                return;
            }
            for &(enchantment, level) in &event.enchantments {
                assert_eq!(
                    event.enchanted_item.get_enchantment_level(enchantment),
                    level
                );
            }
            push(
                &applied_by_handler,
                format!(
                    "{} {}",
                    event.enchanted_item.item.registry_key,
                    !event.enchantments.is_empty()
                ),
            );
        }),
        EventPriority::Normal,
        true,
        false,
    );

    let inventory: Arc<dyn Inventory> = Arc::new(SimpleInventory::new(2));
    inventory.set_stack(0, ItemStack::new(1, &Item::DIAMOND_SWORD));
    inventory.set_stack(1, ItemStack::new(3, &Item::LAPIS_LAZULI));
    let mut table = EnchantingTableScreenHandler::new(1, &player.inventory, &inventory, 42, 15);
    table.update_enchantments(player.as_ref());

    let vanilla = *original.lock().unwrap();
    assert!(vanilla[2] > 1, "{vanilla:?}");
    assert_eq!(table.level_requirements, [1, vanilla[1], vanilla[2]]);

    assert!(table.on_button_click(player.as_ref(), 2));
    assert!(inventory.get_stack(0).has_enchantments());
    assert_eq!(entries(&applied), ["diamond_sword true"]);
}

#[test]
fn one_event_per_fire_site_level_tick_events() {
    let _serial = support::serial();
    let seen = log();
    let manager = &support::server().plugin_manager;
    let pre = seen.clone();
    manager.register_with::<LevelTickEventPre, _>(
        blocking(move |event: &mut LevelTickEventPre| {
            push(
                &pre,
                format!("pre {}", event.world.dimension.minecraft_name),
            );
        }),
        EventPriority::Normal,
        true,
        false,
    );
    let post = seen.clone();
    manager.register_with::<LevelTickEventPost, _>(
        blocking(move |event: &mut LevelTickEventPost| {
            push(
                &post,
                format!("post {}", event.world.dimension.minecraft_name),
            );
        }),
        EventPriority::Normal,
        true,
        false,
    );

    support::server().tick_worlds();

    assert_eq!(
        entries(&seen),
        ["pre minecraft:overworld", "post minecraft:overworld"]
    );
}

#[test]
fn one_event_per_fire_site_register_commands_event() {
    let _serial = support::serial();
    let metadata = PluginMetadata {
        name: "bus-test-mod".to_owned(),
        version: "1.0.0".to_owned(),
        authors: Vec::new(),
        description: "Event bus test mod".to_owned(),
        dependencies: Vec::new(),
        permissions: Vec::new(),
    };
    let context = create_context(support::server(), metadata);
    assert!(
        !support::server()
            .command_dispatcher
            .load()
            .has_command("bustest")
    );

    context.register_command(
        command("bustest", "A command of the event bus test."),
        "bustest",
    );

    assert!(
        support::server()
            .command_dispatcher
            .load()
            .has_command("bustest")
    );
}
