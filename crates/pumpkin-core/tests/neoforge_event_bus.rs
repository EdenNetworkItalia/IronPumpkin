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
//! - [ ] Player login: `World::spawn_java_player` needs a client that takes the spawn chunks;
//!   the play state of the headless client (task 2.21) covers it
//! - [x] Missing field added to a Pumpkin event: the `one_event_per_fire_site_*` tests check the
//!   added fields of each mapped event
//! - [ ] A mod cancels the break of another mod's block: task 2.13
//! - [x] One test per event family (server, tick and root package events): write-back
//!   `change_a_settable_field_anvil_update_event`, cancel `cancel_server_chat_event`
//!   (player events: write-back `change_a_settable_field_critical_hit_event`, cancel
//!   `cancel_sweep_attack_event`, both in `neoforge_player_events.rs`)
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
        mpsc,
    },
    time::Duration,
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
        enchantment::{EnchantItemEvent, PlayerEnchantItemEvent, PrepareItemEnchantEvent},
        inventory::{
            brew::BrewEvent, grindstone_take::GrindstoneEventOnTakeItem,
            potion_brew_post::PotionBrewEventPost, prepare_anvil::PrepareAnvilEvent,
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
    entity::EntityType,
    item::Item,
    item_stack::ItemStack,
    potion::Potion,
    screen::WindowType,
    statistic::{CustomStatistic, StatisticCategory},
};
use pumpkin_inventory::{
    SimpleInventory, anvil::AnvilScreenHandler,
    enchanting::enchanting_screen_handler::EnchantingTableScreenHandler,
    grindstone_screen_handler::GrindstoneScreenHandler, inventory::Inventory,
    screen_handler::ScreenHandler,
};
use pumpkin_protocol::{
    codec::{item_stack_seralizer::OptionalItemStackHash, var_int::VarInt},
    java::server::play::{
        SChatMessage, SClickSlot, SContainerButtonClick, SRenameItem, SlotActionType,
    },
};
use pumpkin_util::{math::position::BlockPos, math::vector3::Vector3};

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

/// A cancellable event that only `register_and_register_event_receive_cancelled_events` fires
/// on the shared server.
#[derive(Default)]
struct UpstreamRegisterEvent {
    cancelled: bool,
}

impl Payload for UpstreamRegisterEvent {
    fn get_name_static() -> &'static str {
        "NeoForgeBusUpstreamRegisterEvent"
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

#[test]
fn register_and_register_event_receive_cancelled_events() {
    let server = support::server();
    let manager = &server.plugin_manager;
    let log = log();
    manager.register_with::<UpstreamRegisterEvent, _>(
        blocking(|event: &mut UpstreamRegisterEvent| event.cancelled = true),
        EventPriority::Highest,
        true,
        false,
    );
    let native = log.clone();
    manager.register::<UpstreamRegisterEvent, _>(
        blocking(move |event: &mut UpstreamRegisterEvent| {
            push(&native, format!("register {}", event.cancelled));
        }),
        EventPriority::Normal,
        true,
    );
    // The Wasm hosts register through the plugin context.
    let metadata = PluginMetadata {
        name: "bus-cancel-mod".to_owned(),
        version: "1.0.0".to_owned(),
        authors: Vec::new(),
        description: "Event bus cancel test mod".to_owned(),
        dependencies: Vec::new(),
        permissions: Vec::new(),
    };
    let wasm = log.clone();
    create_context(server, metadata).register_event::<UpstreamRegisterEvent, _>(
        blocking(move |event: &mut UpstreamRegisterEvent| {
            push(&wasm, format!("register_event {}", event.cancelled));
        }),
        EventPriority::Low,
        true,
    );

    manager.fire_blocking(server, &mut UpstreamRegisterEvent::default());

    assert_eq!(entries(&log), ["register true", "register_event true"]);
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

/// Opens `handler` as the screen of `player`, as `Player::open_handled_screen` does.
fn open<H: ScreenHandler + 'static>(player: &Player, handler: H) -> Arc<Mutex<H>> {
    let handler = Arc::new(Mutex::new(handler));
    *player.current_screen_handler.lock().unwrap() = handler.clone();
    handler
}

/// Runs `action` on another thread and waits for it: a handler that deadlocks fails the test
/// instead of hanging it.
fn finish_within(player: &Arc<Player>, action: impl FnOnce(&Arc<Player>) + Send + 'static) {
    let (done, finished) = mpsc::channel();
    let player = player.clone();
    std::thread::spawn(move || {
        action(&player);
        let _ = done.send(());
    });
    finished
        .recv_timeout(Duration::from_secs(30))
        .unwrap_or_else(|_| panic!("the action did not finish: a handler deadlocked or panicked"));
}

/// Left-clicks `slot` of the open screen (sync id 1) of `player`.
fn click(player: &Arc<Player>, slot: i16) {
    let packet = SClickSlot {
        sync_id: VarInt(1),
        revision: VarInt(0),
        slot,
        button: SClickSlot::BUTTON_LEFT,
        mode: SlotActionType::Pickup,
        length_of_array: VarInt(0),
        array_of_changed_slots: Vec::new(),
        carried_item: OptionalItemStackHash(None),
    };
    finish_within(player, move |player| {
        player.on_slot_click(packet, support::server());
    });
}

/// Opens an anvil for `player`: a damaged sword on the left and a diamond on the right.
fn anvil(player: &Player) -> (Arc<Mutex<AnvilScreenHandler>>, Arc<SimpleInventory>) {
    let inventory = Arc::new(SimpleInventory::new(3));
    let mut sword = ItemStack::new(1, &Item::DIAMOND_SWORD);
    sword.set_damage(1000);
    inventory.set_stack(0, sword);
    inventory.set_stack(1, ItemStack::new(3, &Item::DIAMOND));
    let handler = AnvilScreenHandler::new(1, &player.inventory, inventory.clone());
    (open(player, handler), inventory)
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

    // A click on the empty result slot updates the result: the player cannot pay the vanilla
    // cost, so the click takes nothing.
    let (handler, inventory) = anvil(&player);
    click(&player, 2);
    assert_eq!(inventory.get_stack(2).item, &Item::NETHER_STAR);
    let anvil_state = handler.lock().unwrap();
    assert_eq!(anvil_state.get_cost(), 7);
    assert_eq!(
        anvil_state.repair_item_count_cost.load(Ordering::Relaxed),
        1
    );
    drop(anvil_state);
    // Vanilla repairs the sword with 3 diamonds: 3 levels, 3 diamonds.
    assert_eq!(entries(&seen), ["diamond_sword diamond diamond_sword 3 3"]);

    cancel.store(true, Ordering::Relaxed);
    let (handler, inventory) = anvil(&player);
    click(&player, 2);
    assert!(inventory.get_stack(2).is_empty());
    let anvil_state = handler.lock().unwrap();
    assert_eq!(anvil_state.get_cost(), 0);
    assert_eq!(
        anvil_state.repair_item_count_cost.load(Ordering::Relaxed),
        0
    );
}

#[test]
fn anvil_update_event_handler_closes_the_screen() {
    let _serial = support::serial();
    let player = support::player("AnvilCloser");
    support::server()
        .plugin_manager
        .register_with::<PrepareAnvilEvent, _>(
            blocking(|event: &mut PrepareAnvilEvent| {
                if event.player.gameprofile.name != "AnvilCloser" {
                    return;
                }
                event.output = ItemStack::new(1, &Item::NETHER_STAR);
                // It locks the screen handler of the player: the event fires outside that lock.
                event.player.close_handled_screen();
            }),
            EventPriority::Normal,
            true,
            false,
        );

    let (_handler, inventory) = anvil(&player);
    finish_within(&player, |player| {
        player.on_rename_item(&SRenameItem { item_name: "Named" });
    });

    // The closed anvil gave the inputs back, so the output of the handler does not apply.
    assert!(inventory.get_stack(0).is_empty());
    assert_ne!(inventory.get_stack(2).item, &Item::NETHER_STAR);
    let current = player.current_screen_handler.lock().unwrap().clone();
    let inventory_screen: Arc<Mutex<dyn ScreenHandler>> = player.player_screen_handler.clone();
    assert!(Arc::ptr_eq(&current, &inventory_screen));
}

#[test]
fn update_result_without_handler_is_vanilla() {
    let _serial = support::serial();
    let player = support::player("NoHandler");

    let (updated, updated_inventory) = anvil(&player);
    let (created, created_inventory) = anvil(&player);
    let mut updated = updated.lock().unwrap();
    let mut created = created.lock().unwrap();
    updated.update_result(player.as_ref());
    // The player plays in survival: no infinite materials.
    created.create_result(false);
    assert!(
        updated_inventory
            .get_stack(2)
            .are_equal(&created_inventory.get_stack(2))
    );
    assert_eq!(updated.get_cost(), created.get_cost());
    assert_eq!(
        updated.repair_item_count_cost.load(Ordering::Relaxed),
        created.repair_item_count_cost.load(Ordering::Relaxed)
    );
    // The event waits for the player to release the screen-handler lock.
    assert!(updated.take_update_event());
    assert!(!created.take_update_event());

    let (updated, updated_inventory) = grindstone(&player);
    let (created, created_inventory) = grindstone(&player);
    let mut updated = updated.lock().unwrap();
    updated.update_result();
    created.lock().unwrap().create_result();
    assert!(
        updated_inventory
            .get_stack(2)
            .are_equal(&created_inventory.get_stack(2))
    );
    assert_eq!(updated.xp.load(Ordering::Relaxed), -1);
    assert!(updated.take_update_event());
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

/// Opens a grindstone for `player` with an enchanted sword in the top slot.
fn grindstone(player: &Player) -> (Arc<Mutex<GrindstoneScreenHandler>>, Arc<SimpleInventory>) {
    let inventory = Arc::new(SimpleInventory::new(3));
    let mut sword = ItemStack::new(1, &Item::IRON_SWORD);
    sword.add_enchantment(&Enchantment::SHARPNESS, 2);
    inventory.set_stack(0, sword);
    let handler = GrindstoneScreenHandler::new(1, &player.inventory, inventory.clone());
    (open(player, handler), inventory)
}

/// The experience orbs at `pos`.
fn orbs_at(pos: BlockPos) -> usize {
    support::world()
        .entities
        .load()
        .iter()
        .filter(|entity| {
            let entity = entity.get_entity();
            entity.entity_type.id == EntityType::EXPERIENCE_ORB.id && entity.block_pos.load() == pos
        })
        .count()
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
                if event.player.gameprofile.name != "Grinder" || event.top.is_empty() {
                    return;
                }
                // It locks the screen handler of the player: the event fires outside that lock.
                let screen = event.player.current_screen_handler.lock().unwrap().clone();
                let window = screen.lock().unwrap().window_type();
                push(
                    &seen_by_handler,
                    format!(
                        "{} {} {:?} {window:?}",
                        event.top.item.registry_key,
                        event.bottom.is_empty(),
                        event.result_item,
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

    // No output: the vanilla result, an unenchanted sword, and the vanilla experience. The
    // first click updates the result, the second takes it.
    let vanilla_at = BlockPos::new(40, 100, 40);
    player.open_container_pos.store(Some(vanilla_at));
    let (_handler, inventory) = grindstone(&player);
    click(&player, 2);
    let result = inventory.get_stack(2);
    assert_eq!(result.item, &Item::IRON_SWORD);
    assert!(!result.has_enchantments());
    click(&player, 2);
    assert!(orbs_at(vanilla_at) > 0);

    // A set output with 5 experience: orbs of 3, 1 and 1.
    mode.store(1, Ordering::Relaxed);
    let output_at = BlockPos::new(48, 100, 48);
    player.open_container_pos.store(Some(output_at));
    let (_handler, inventory) = grindstone(&player);
    click(&player, 2);
    assert_eq!(inventory.get_stack(2).item, &Item::STICK);
    click(&player, 2);
    assert_eq!(orbs_at(output_at), 3);

    mode.store(2, Ordering::Relaxed);
    let (_handler, inventory) = grindstone(&player);
    click(&player, 2);
    assert!(inventory.get_stack(2).is_empty());
    player.open_container_pos.store(None);

    assert_eq!(
        entries(&seen),
        [r#"iron_sword true Some("iron_sword") Some(Grindstone)"#; 3]
    );
}

#[test]
fn one_event_per_fire_site_stat_award_event() {
    let _serial = support::serial();
    let player = support::player("Jumper");
    let cancel = Arc::new(AtomicBool::new(false));
    let seen = log();
    let (cancel_flag, seen_by_handler) = (cancel.clone(), seen.clone());
    support::server()
        .plugin_manager
        .register_with::<PlayerStatisticIncrementEvent, _>(
            blocking(move |event: &mut PlayerStatisticIncrementEvent| {
                if event.player.gameprofile.name != "Jumper" {
                    return;
                }
                push(
                    &seen_by_handler,
                    format!("{} {} {}", event.stat, event.amount, event.value),
                );
                if cancel_flag.load(Ordering::Relaxed) {
                    event.set_cancelled(true);
                    return;
                }
                assert_eq!(event.category, StatisticCategory::Custom);
                if event.stat == CustomStatistic::Jump as i32 {
                    // The total goes to the stat the handler names.
                    event.stat = CustomStatistic::EnchantItem as i32;
                    event.value = 5;
                }
            }),
            EventPriority::Normal,
            true,
            false,
        );
    let jump = CustomStatistic::Jump as i32;
    let enchanted = CustomStatistic::EnchantItem as i32;
    let rest = CustomStatistic::TimeSinceRest as i32;

    player.increment_stat(StatisticCategory::Custom, jump, 1);
    assert_eq!(player.get_stat(StatisticCategory::Custom, jump), 0);
    assert_eq!(player.get_stat(StatisticCategory::Custom, enchanted), 5);

    // `set_stat` fires with the new total, as `StatsCounter.setValue`, and a reset to 0 too.
    player.set_stat(StatisticCategory::Custom, rest, 40);
    player.increment_stat(StatisticCategory::Custom, rest, 2);
    player.set_stat(StatisticCategory::Custom, rest, 0);
    assert_eq!(player.get_stat(StatisticCategory::Custom, rest), 0);

    cancel.store(true, Ordering::Relaxed);
    player.increment_stat(StatisticCategory::Custom, enchanted, 1);
    player.set_stat(StatisticCategory::Custom, enchanted, 0);
    assert_eq!(player.get_stat(StatisticCategory::Custom, enchanted), 5);

    assert_eq!(
        entries(&seen),
        [
            format!("{jump} 1 1"),
            format!("{rest} 40 40"),
            format!("{rest} 2 42"),
            format!("{rest} -42 0"),
            format!("{enchanted} 1 6"),
            format!("{enchanted} -5 0"),
        ]
    );
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

/// A brewing stand at `position` with water, nether wart and blaze powder.
fn brewing_stand(position: BlockPos) -> Arc<BrewingStandBlockEntity> {
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
    Arc::new(stand)
}

/// Ticks `stand` until it brews. A handler of `BrewingStartEvent` shortens the brew to one tick.
fn brew(stand: &BrewingStandBlockEntity) -> [ItemStack; 5] {
    let world = support::world();
    stand.tick(&world);
    stand.tick(&world);
    stand.items.read().unwrap().clone()
}

#[test]
fn one_event_per_fire_site_potion_brew_event_pre() {
    let _serial = support::serial();
    let manager = &support::server().plugin_manager;
    // The handlers stay on the shared server, so they act only on the stands of this test.
    let brewed = BlockPos::new(4, 64, 4);
    let cancelled = BlockPos::new(8, 64, 8);
    let cancelled_meanwhile = BlockPos::new(12, 64, 12);
    let changed_meanwhile = BlockPos::new(16, 64, 16);
    let positions = [brewed, cancelled, cancelled_meanwhile, changed_meanwhile];
    manager.register_with::<BrewingStartEvent, _>(
        blocking(move |event: &mut BrewingStartEvent| {
            if positions.contains(&event.block_pos) {
                event.brewing_time = 1;
            }
        }),
        EventPriority::Normal,
        true,
        false,
    );
    let stands = positions.map(brewing_stand);
    let players_side = stands.clone();
    manager.register_with::<BrewEvent, _>(
        blocking(move |event: &mut BrewEvent| {
            let Some(index) = positions.iter().position(|pos| *pos == event.block_pos) else {
                return;
            };
            assert_eq!(event.stacks[3].item, &Item::NETHER_WART);
            if event.block_pos != cancelled_meanwhile {
                event.stacks[2] = ItemStack::new(1, &Item::DIAMOND);
            }
            if event.block_pos == cancelled_meanwhile || event.block_pos == changed_meanwhile {
                // A player takes the ingredient while the event fires.
                players_side[index].items.write().unwrap()[3] = ItemStack::EMPTY.clone();
            }
            if event.block_pos != brewed {
                event.set_cancelled(true);
            }
        }),
        EventPriority::Normal,
        true,
        false,
    );
    let potion = |stack: &ItemStack| {
        stack
            .get_data_component::<PotionContentsImpl>()
            .and_then(|contents| contents.potion_id)
    };
    let water = Some(i32::from(Potion::WATER.id));

    // Not cancelled: vanilla brews and ignores the stacks of the handler.
    let items = brew(&stands[0]);
    assert_eq!(potion(&items[0]), Some(i32::from(Potion::AWKWARD.id)));
    assert!(items[2].is_empty());
    assert!(items[3].is_empty());

    // Cancelled: no brew, and the stand takes the stacks of the handler.
    let items = brew(&stands[1]);
    assert_eq!(items[2].item, &Item::DIAMOND);
    assert_eq!(potion(&items[0]), water);
    assert_eq!(items[3].item, &Item::NETHER_WART);

    // Cancelled with the stacks unchanged: the ingredient the player took stays taken.
    let items = brew(&stands[2]);
    assert!(items[3].is_empty());
    assert_eq!(potion(&items[0]), water);

    // Cancelled with changed stacks while the inventory changed: the player's change wins.
    let items = brew(&stands[3]);
    assert!(items[3].is_empty());
    assert!(items[2].is_empty());
    assert_eq!(potion(&items[0]), water);
}

/// The window type of the open screen of `player`. It locks the screen handler, so a handler of
/// an event that fires under that lock deadlocks here.
fn open_window(player: &Player) -> Option<WindowType> {
    let screen = player.current_screen_handler.lock().unwrap().clone();
    screen.lock().unwrap().window_type()
}

/// Opens an enchanting table for `player` with `item` on the cursor, `lapis` lapis lazuli in
/// slot 1 and 15 bookshelves.
fn enchanting_table(
    player: &Arc<Player>,
    item: ItemStack,
    lapis: u8,
) -> (Arc<Mutex<EnchantingTableScreenHandler>>, Arc<dyn Inventory>) {
    let inventory: Arc<dyn Inventory> = Arc::new(SimpleInventory::new(2));
    inventory.set_stack(1, ItemStack::new(lapis, &Item::LAPIS_LAZULI));
    let table = open(
        player,
        EnchantingTableScreenHandler::new(1, &player.inventory, &inventory, 42, 15),
    );
    *table
        .lock()
        .unwrap()
        .get_behaviour()
        .cursor_stack
        .lock()
        .unwrap() = item;
    (table, inventory)
}

/// Clicks enchantment row `row` of the open screen (sync id 1) of `player`.
fn enchant(player: &Arc<Player>, row: i32) {
    finish_within(player, move |player| {
        player.on_container_button_click(&SContainerButtonClick {
            window_id: VarInt(1),
            button_id: VarInt(row),
        });
    });
}

/// The first stack of `item` in the player inventory of `player`.
fn held(player: &Player, item: &Item) -> Option<ItemStack> {
    (0..36)
        .map(|slot| player.inventory.get_stack(slot))
        .find(|stack| stack.item == item)
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "a handler per enchanting event and the flow through the table"
)]
fn one_event_per_fire_site_enchantment_events() {
    let _serial = support::serial();
    let player = support::player("Enchanter");
    // Row 2 needs at most 31 levels with 15 bookshelves.
    player.experience_level.store(40, Ordering::Relaxed);
    let table_at = BlockPos::new(24, 64, 24);
    player.open_container_pos.store(Some(table_at));
    let original = Arc::new(Mutex::new([0; 3]));
    let seen = log();
    let manager = &support::server().plugin_manager;
    let original_by_handler = original.clone();
    manager.register_with::<PrepareItemEnchantEvent, _>(
        blocking(move |event: &mut PrepareItemEnchantEvent| {
            if event.player.gameprofile.name != "Enchanter" {
                return;
            }
            assert_eq!(
                event.level_requirements,
                event.original_level_requirements()
            );
            assert_eq!(event.bookshelf_count, 15);
            assert_eq!(event.pos, table_at);
            assert_eq!(open_window(&event.player), Some(WindowType::Enchantment));
            *original_by_handler.lock().unwrap() = event.original_level_requirements();
            event.level_requirements[0] = 1;
        }),
        EventPriority::High,
        true,
        false,
    );
    let seen_by_later = seen.clone();
    manager.register_with::<PrepareItemEnchantEvent, _>(
        blocking(move |event: &mut PrepareItemEnchantEvent| {
            if event.player.gameprofile.name == "Enchanter" {
                push(
                    &seen_by_later,
                    format!(
                        "later {} {:?}",
                        event.level_requirements[0],
                        event.original_level_requirements()
                    ),
                );
            }
        }),
        EventPriority::Low,
        true,
        false,
    );
    let seen_by_enchant = seen.clone();
    manager.register_with::<EnchantItemEvent, _>(
        blocking(move |event: &mut EnchantItemEvent| {
            if event.player.gameprofile.name != "Enchanter" {
                return;
            }
            assert_eq!(open_window(&event.player), Some(WindowType::Enchantment));
            push(
                &seen_by_enchant,
                format!(
                    "enchant {} row {}",
                    event.item.item.registry_key, event.option
                ),
            );
        }),
        EventPriority::Normal,
        true,
        false,
    );
    let applied_by_handler = seen.clone();
    manager.register_with::<PlayerEnchantItemEvent, _>(
        blocking(move |event: &mut PlayerEnchantItemEvent| {
            if event.player.gameprofile.name != "Enchanter" {
                return;
            }
            assert_eq!(open_window(&event.player), Some(WindowType::Enchantment));
            for &(enchantment, level) in &event.enchantments {
                assert_eq!(
                    event.enchanted_item.get_enchantment_level(enchantment),
                    level
                );
            }
            push(
                &applied_by_handler,
                format!(
                    "enchanted {} {}",
                    event.enchanted_item.item.registry_key,
                    !event.enchantments.is_empty()
                ),
            );
            // The event carries the item of the table: the table keeps this change.
            event
                .enchanted_item
                .add_enchantment(&Enchantment::MENDING, 1);
        }),
        EventPriority::Normal,
        true,
        false,
    );

    // Putting the sword into the table updates the rows.
    let (table, inventory) = enchanting_table(&player, ItemStack::new(1, &Item::DIAMOND_SWORD), 3);
    click(&player, 0);
    let vanilla = *original.lock().unwrap();
    assert!(vanilla[0] > 1 && vanilla[2] > 1, "{vanilla:?}");
    assert_eq!(
        table.lock().unwrap().level_requirements,
        [1, vanilla[1], vanilla[2]]
    );

    enchant(&player, 2);
    let enchanted = inventory.get_stack(0);
    assert!(enchanted.has_enchantments());
    assert_eq!(enchanted.get_enchantment_level(&Enchantment::MENDING), 1);
    // Row 2 costs 3 levels and 3 lapis lazuli.
    assert!(inventory.get_stack(1).is_empty());
    assert_eq!(player.experience_level.load(Ordering::Relaxed), 37);
    player.open_container_pos.store(None);
    assert_eq!(
        entries(&seen),
        [
            format!("later 1 {vanilla:?}"),
            "enchant diamond_sword row 2".to_owned(),
            "enchanted diamond_sword true".to_owned(),
        ]
    );
}

#[test]
fn enchanting_events_handler_closes_the_screen() {
    let _serial = support::serial();
    let manager = &support::server().plugin_manager;
    let close = |player: &Arc<Player>, name: &str| {
        if player.gameprofile.name == name {
            // It locks the screen handler of the player: the event fires outside that lock.
            player.close_handled_screen();
        }
    };
    manager.register_with::<PrepareItemEnchantEvent, _>(
        blocking(move |event: &mut PrepareItemEnchantEvent| {
            if event.player.gameprofile.name == "LevelCloser" {
                close(&event.player, "LevelCloser");
                event.level_requirements = [1; 3];
            }
        }),
        EventPriority::Normal,
        true,
        false,
    );
    manager.register_with::<EnchantItemEvent, _>(
        blocking(move |event: &mut EnchantItemEvent| close(&event.player, "EnchantCloser")),
        EventPriority::Normal,
        true,
        false,
    );
    manager.register_with::<PlayerEnchantItemEvent, _>(
        blocking(move |event: &mut PlayerEnchantItemEvent| {
            close(&event.player, "AppliedCloser");
        }),
        EventPriority::Normal,
        true,
        false,
    );
    let player_screen = |player: &Arc<Player>| {
        let current = player.current_screen_handler.lock().unwrap().clone();
        let inventory_screen: Arc<Mutex<dyn ScreenHandler>> = player.player_screen_handler.clone();
        Arc::ptr_eq(&current, &inventory_screen)
    };

    // The table closed during the level event: it gave the sword back and keeps no rows.
    let player = support::player("LevelCloser");
    let (table, inventory) = enchanting_table(&player, ItemStack::new(1, &Item::DIAMOND_SWORD), 3);
    click(&player, 0);
    assert!(inventory.get_stack(0).is_empty());
    assert_ne!(table.lock().unwrap().level_requirements, [1; 3]);
    assert!(held(&player, &Item::DIAMOND_SWORD).is_some_and(|sword| !sword.has_enchantments()));
    assert!(player_screen(&player));

    // The table closed before the enchantments apply: nothing is enchanted or paid.
    for (name, enchanted, levels) in [("EnchantCloser", false, 40), ("AppliedCloser", true, 37)] {
        let player = support::player(name);
        player.experience_level.store(40, Ordering::Relaxed);
        let (table, inventory) =
            enchanting_table(&player, ItemStack::new(1, &Item::DIAMOND_SWORD), 3);
        click(&player, 0);
        let rows = table.lock().unwrap().level_requirements;
        assert!(rows[2] > 0, "{rows:?}");
        enchant(&player, 2);
        assert!(inventory.get_stack(0).is_empty(), "{name}");
        assert!(player_screen(&player), "{name}");
        let sword = held(&player, &Item::DIAMOND_SWORD).unwrap();
        assert_eq!(sword.has_enchantments(), enchanted, "{name}");
        assert_eq!(
            player.experience_level.load(Ordering::Relaxed),
            levels,
            "{name}"
        );
    }
}

/// Opens a grindstone for `player` with an enchanted sword in the top slot and a vanilla result.
fn grindstone_with_result(
    player: &Arc<Player>,
    pos: BlockPos,
) -> (Arc<Mutex<GrindstoneScreenHandler>>, Arc<SimpleInventory>) {
    player.open_container_pos.store(Some(pos));
    let (handler, inventory) = grindstone(player);
    // A click on the empty result slot updates the result.
    click(player, 2);
    assert_eq!(inventory.get_stack(2).item, &Item::IRON_SWORD);
    (handler, inventory)
}

#[test]
fn one_event_per_fire_site_grindstone_event_on_take_item() {
    let _serial = support::serial();
    let player = support::player("Taker");
    let mode = Arc::new(AtomicUsize::new(0));
    let seen = log();
    let (handler_mode, seen_by_handler) = (mode.clone(), seen.clone());
    support::server()
        .plugin_manager
        .register_with::<GrindstoneEventOnTakeItem, _>(
            blocking(move |event: &mut GrindstoneEventOnTakeItem| {
                if event.player.gameprofile.name != "Taker" {
                    return;
                }
                push(
                    &seen_by_handler,
                    format!(
                        "{} {} {} {:?}",
                        event.top.item.registry_key,
                        event.bottom.is_empty(),
                        event.xp > 0,
                        open_window(&event.player),
                    ),
                );
                match handler_mode.load(Ordering::Relaxed) {
                    1 => {
                        event.xp = 5;
                        event.new_top = ItemStack::new(1, &Item::STICK);
                    }
                    2 => event.set_cancelled(true),
                    3 => {
                        event.new_top = ItemStack::new(1, &Item::STICK);
                        event.player.close_handled_screen();
                    }
                    _ => {}
                }
            }),
            EventPriority::Normal,
            true,
            false,
        );

    // No change: the vanilla experience, and the inputs are consumed.
    let vanilla_at = BlockPos::new(56, 100, 56);
    let (_handler, inventory) = grindstone_with_result(&player, vanilla_at);
    click(&player, 2);
    assert!(orbs_at(vanilla_at) > 0);
    assert!(inventory.get_stack(0).is_empty());

    // 5 experience (orbs of 3, 1 and 1) and a new top input.
    mode.store(1, Ordering::Relaxed);
    let changed_at = BlockPos::new(64, 100, 64);
    let (_handler, inventory) = grindstone_with_result(&player, changed_at);
    click(&player, 2);
    assert_eq!(orbs_at(changed_at), 3);
    assert_eq!(inventory.get_stack(0).item, &Item::STICK);

    // Cancelled: no experience, the inputs stay, and the result stays empty.
    mode.store(2, Ordering::Relaxed);
    let cancelled_at = BlockPos::new(72, 100, 72);
    let (handler, inventory) = grindstone_with_result(&player, cancelled_at);
    click(&player, 2);
    assert_eq!(orbs_at(cancelled_at), 0);
    assert_eq!(inventory.get_stack(0).item, &Item::IRON_SWORD);
    assert!(inventory.get_stack(0).has_enchantments());
    assert!(inventory.get_stack(2).is_empty());
    let cursor = handler
        .lock()
        .unwrap()
        .get_behaviour()
        .cursor_stack
        .lock()
        .unwrap()
        .clone();
    assert_eq!(cursor.item, &Item::IRON_SWORD);
    assert!(!cursor.has_enchantments());

    // The grindstone closed during the event: the new top input goes to the player.
    mode.store(3, Ordering::Relaxed);
    let closed_at = BlockPos::new(80, 100, 80);
    let (_handler, inventory) = grindstone_with_result(&player, closed_at);
    click(&player, 2);
    assert!(inventory.get_stack(0).is_empty());
    assert!(held(&player, &Item::STICK).is_some());
    player.open_container_pos.store(None);

    assert_eq!(entries(&seen), ["iron_sword true true Some(Grindstone)"; 4]);
}

#[test]
fn one_event_per_fire_site_potion_brew_event_post() {
    let _serial = support::serial();
    let manager = &support::server().plugin_manager;
    let brewed = BlockPos::new(32, 64, 32);
    let cancelled = BlockPos::new(36, 64, 36);
    let positions = [brewed, cancelled];
    manager.register_with::<BrewingStartEvent, _>(
        blocking(move |event: &mut BrewingStartEvent| {
            if positions.contains(&event.block_pos) {
                event.brewing_time = 1;
            }
        }),
        EventPriority::Normal,
        true,
        false,
    );
    manager.register_with::<BrewEvent, _>(
        blocking(move |event: &mut BrewEvent| {
            if event.block_pos == cancelled {
                event.stacks[2] = ItemStack::new(1, &Item::DIAMOND);
                event.set_cancelled(true);
            }
        }),
        EventPriority::Normal,
        true,
        false,
    );
    let seen = log();
    let seen_by_handler = seen.clone();
    manager.register_with::<PotionBrewEventPost, _>(
        blocking(move |event: &mut PotionBrewEventPost| {
            if !positions.contains(&event.block_pos) {
                return;
            }
            push(
                &seen_by_handler,
                format!(
                    "post {} {} ingredient {}",
                    event.stacks[0].item.registry_key,
                    event.stacks[2].item.registry_key,
                    !event.stacks[3].is_empty(),
                ),
            );
            // `NeoForge` hands out the live stacks: the stand takes this change.
            event.stacks[1] = ItemStack::new(1, &Item::GLASS_BOTTLE);
        }),
        EventPriority::Normal,
        true,
        false,
    );
    let potion = |stack: &ItemStack| {
        stack
            .get_data_component::<PotionContentsImpl>()
            .and_then(|contents| contents.potion_id)
    };

    // The vanilla brew: the event sees the brewed bottles and the consumed ingredient.
    let items = brew(&brewing_stand(brewed));
    assert_eq!(potion(&items[0]), Some(i32::from(Potion::AWKWARD.id)));
    assert_eq!(items[1].item, &Item::GLASS_BOTTLE);

    // A cancelled `PotionBrewEvent.Pre`: the event sees the stacks of the Pre handler.
    let items = brew(&brewing_stand(cancelled));
    assert_eq!(potion(&items[0]), Some(i32::from(Potion::WATER.id)));
    assert_eq!(items[1].item, &Item::GLASS_BOTTLE);
    assert_eq!(items[2].item, &Item::DIAMOND);

    assert_eq!(
        entries(&seen),
        [
            "post potion air ingredient false",
            "post potion diamond ingredient true"
        ]
    );
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
    // The tick of a frozen game.
    support::server().tick_players_and_network();

    assert_eq!(
        entries(&seen),
        ["pre minecraft:overworld", "post minecraft:overworld"].repeat(2)
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
