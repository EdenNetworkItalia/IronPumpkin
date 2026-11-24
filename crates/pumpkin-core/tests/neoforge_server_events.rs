//! The `NeoForge` server, data pack and command events of the root `event` package (issue #178):
//! `CommandEvent`, `PermissionsChangedEvent`, `TagsUpdatedEvent.ServerDataLoad`,
//! `OnDatapackSyncEvent`, `LootTableLoadEvent` and `AddServerReloadListenersEvent`, against a
//! server of the support module.
//!
//! The handlers stay registered on the shared server, so each test filters the events that it
//! owns by the command, the player name or the loot table id.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::{
    marker::PhantomData,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use pumpkin_core::{
    command::CommandSender,
    data::datapack::{
        loot_table_loader::parse_loot_table,
        mod_packs::{ModDataPack, PackFiles},
    },
    entity::player::Player,
    net::{
        java::neoforge::NegotiatedState,
        management::{handlers::dispatch, rpc::RpcRequest},
    },
    plugin::{
        BoxFuture, EventHandler, EventPriority, ParentEventHandler, Payload,
        player::PermissionsChangedEvent,
        server::{
            AddServerReloadListenersEvent, CommandEvent, LootTableLoadEvent, OnDatapackSyncEvent,
            ReloadListener, ReloadListenerError, TagsUpdatedEvent, TagsUpdatedEventServerDataLoad,
        },
        data_maps::{DataMapHost, set_host},
        server_events,
    },
    server::{Server, recipe::RecipeManager},
    world::loot::{LootContextParameters, generate_loot_from_handle},
};
use pumpkin_data::game_rules::GameRule;
use pumpkin_protocol::java::neoforge::KnownRegistryDataMapsPayload;
use pumpkin_util::identifier::Identifier;
use std::collections::{BTreeMap, BTreeSet};
use pumpkin_util::PermissionLvl;

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

fn rule_value(rule: &GameRule) -> String {
    support::world().get_game_rule(rule).to_string()
}

/// Runs `command` as the console, without feedback.
fn run_as_console(command: &str) {
    let server = support::server();
    let source = CommandSender::Console.into_source(server).with_silent();
    server
        .command_dispatcher
        .load()
        .handle_command(&source, command);
}

/// `CommandEvent` fires for a console command, with the command and the sender, before the
/// parse. A cancel stops the command.
#[test]
fn cancel_command_event() {
    let _serial = support::serial();
    let seen = log();
    let seen_by_handler = seen.clone();
    let command = format!("gamerule {} true", GameRule::KeepInventory);
    let watched = command.clone();
    on::<CommandEvent, _>(move |event| {
        if event.command == watched {
            push(
                &seen_by_handler,
                format!("{} {}", event.source.name, event.source.output),
            );
            event.cancelled = true;
        }
    });
    support::world().set_game_rule(
        &GameRule::KeepInventory,
        pumpkin_data::game_rules::GameRuleValue::Bool(false),
    );

    run_as_console(&command);

    assert_eq!(entries(&seen), ["Server Server"]);
    assert_eq!(rule_value(&GameRule::KeepInventory), "false");
}

/// A handler replaces the parse result: it sets `command`, and the dispatcher parses and runs the
/// new command. A leading slash is not part of `command`.
#[test]
fn replace_the_command_of_a_command_event() {
    let _serial = support::serial();
    let seen = log();
    let seen_by_handler = seen.clone();
    let rule = GameRule::RandomTickSpeed;
    let old_command = format!("gamerule {rule} 11");
    let new_command = format!("gamerule {rule} 12");
    let (watched, replacement) = (old_command.clone(), new_command);
    on::<CommandEvent, _>(move |event| {
        if event.command == watched {
            push(&seen_by_handler, event.command.clone());
            event.command.clone_from(&replacement);
        }
    });

    run_as_console(&format!("/{old_command}"));

    assert_eq!(entries(&seen), [old_command]);
    assert_eq!(rule_value(&GameRule::RandomTickSpeed), "12");
}

/// A player command reaches the event with the player as the sender. A cancel with an exception
/// stops the command and sends the message to the sender.
#[test]
fn command_event_from_a_player_with_an_exception() {
    let _serial = support::serial();
    let server = support::server();
    let player = support::player("CommandEventPlayer");
    let seen = log();
    let seen_by_handler = seen.clone();
    let command = format!("gamerule {} false", GameRule::FireDamage);
    let watched = command.clone();
    on::<CommandEvent, _>(move |event| {
        if event.command == watched {
            let name = event
                .source
                .output
                .as_player()
                .map(|player| player.gameprofile.name.clone());
            push(&seen_by_handler, format!("{name:?}"));
            event.exception = Some("blocked by a mod".to_owned());
            event.cancelled = true;
        }
    });

    let source = CommandSender::Player(player).into_source(server);
    server
        .command_dispatcher
        .load()
        .handle_command(&source, &command);

    assert_eq!(entries(&seen), ["Some(\"CommandEventPlayer\")"]);
    assert_eq!(rule_value(&GameRule::FireDamage), "true");
}

/// A command of a function is not a command that a sender performs: it fires no `CommandEvent`.
#[test]
fn a_function_line_fires_no_command_event() {
    let _serial = support::serial();
    let seen = log();
    let seen_by_handler = seen.clone();
    let command = format!("gamerule {} true", GameRule::FreezeDamage);
    let watched = command.clone();
    on::<CommandEvent, _>(move |event| {
        if event.command == watched {
            push(&seen_by_handler, event.command.clone());
        }
    });
    let server = support::server();
    let source = CommandSender::Console.into_source(server).with_silent();

    server
        .command_dispatcher
        .load()
        .handle_function_command(&source, &command);
    assert!(entries(&seen).is_empty());

    server
        .command_dispatcher
        .load()
        .handle_command(&source, &command);
    assert_eq!(entries(&seen).len(), 1);
}

/// `PlayerCommandPreprocessEvent` fires for a player command before `CommandEvent`; a handler
/// replaces or cancels the command.
#[test]
fn player_command_preprocess_event_replaces_and_cancels() {
    use pumpkin_core::plugin::player::PlayerCommandPreprocessEvent;

    let _serial = support::serial();
    let server = support::server();
    let player = support::player("PreprocessPlayer");
    on::<PlayerCommandPreprocessEvent, _>(|event| {
        if event.player.gameprofile.name == "PreprocessPlayer" {
            if event.command == "old" {
                event.command = "new".to_owned();
            } else if event.command == "stop" {
                event.cancelled = true;
            }
        }
    });

    let run = |command: &str| {
        support::RUNTIME.block_on(server_events::preprocess_command(server, &player, command))
    };

    assert_eq!(run("old").as_deref(), Some("new"));
    assert_eq!(run("stop"), None);
    assert_eq!(run("other").as_deref(), Some("other"));
}

fn op_level(server: &Server, player: &Player) -> Option<PermissionLvl> {
    server
        .data
        .operator_config
        .read()
        .unwrap()
        .get_entry(&player.gameprofile.id)
        .map(|entry| entry.level)
}

/// `PermissionsChangedEvent` fires on `/op` and `/deop` of an online player, with the old and
/// the new level. A cancel keeps the old level.
#[test]
fn permissions_changed_event_of_op_and_deop() {
    let _serial = support::serial();
    let server = support::server();
    let player = support::player("PermissionPlayer");
    let op_level_of_server = server.basic_config.op_permission_level;
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<PermissionsChangedEvent, _>(move |event| {
        if event.player.gameprofile.name == "PermissionPlayer" {
            push(
                &seen_by_handler,
                format!("{:?} -> {:?}", event.old_level, event.new_level),
            );
        }
    });

    run_as_console("op PermissionPlayer");
    assert_eq!(op_level(server, &player), Some(op_level_of_server));
    run_as_console("deop PermissionPlayer");
    assert_eq!(op_level(server, &player), None);

    assert_eq!(
        entries(&seen),
        [
            format!("{:?} -> {op_level_of_server:?}", PermissionLvl::Zero),
            format!("{op_level_of_server:?} -> {:?}", PermissionLvl::Zero),
        ]
    );
}

/// A cancel of `PermissionsChangedEvent` stops the op and the deop: the op list and the level of
/// the player stay as they are.
#[test]
fn cancel_permissions_changed_event() {
    let _serial = support::serial();
    let server = support::server();
    let player = support::player("CancelledOp");
    on::<PermissionsChangedEvent, _>(|event| {
        if event.player.gameprofile.name == "CancelledOp" {
            event.cancelled = true;
        }
    });

    let source = CommandSender::Console.into_source(server).with_silent();
    let result = server
        .command_dispatcher
        .load()
        .execute_input("op CancelledOp", &source);

    // Vanilla counts the player and sends the success message although a handler cancelled.
    assert_eq!(result.unwrap(), 1);
    assert_eq!(op_level(server, &player), None);
    assert_eq!(player.permission_lvl.load(), PermissionLvl::Zero);
}

/// `TagsUpdatedEvent.ServerDataLoad` fires after a reload of the data packs, and a handler on
/// the parent `TagsUpdatedEvent` gets it. `OnDatapackSyncEvent` fires for all players at the same
/// reload.
#[test]
fn tags_updated_and_datapack_sync_events_after_a_reload() {
    let _serial = support::serial();
    let server = support::server();
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<TagsUpdatedEventServerDataLoad, _>(move |_event| push(&seen_by_handler, "load"));
    let seen_by_parent = seen.clone();
    server
        .plugin_manager
        .register_parent::<TagsUpdatedEvent, _>(
            Arc::new(ParentProbe(seen_by_parent)),
            EventPriority::Normal,
            true,
            false,
        );
    let seen_by_sync = seen.clone();
    on::<OnDatapackSyncEvent, _>(move |event| {
        push(&seen_by_sync, format!("sync {}", event.player.is_none()));
    });

    server.reload_datapacks(server);

    assert_eq!(entries(&seen), ["load", "parent", "sync true"]);
}

struct ParentProbe(Log);

impl ParentEventHandler for ParentProbe {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut (dyn Payload + Send + Sync),
    ) -> BoxFuture<'a, ()> {
        if <dyn Payload>::downcast_ref::<TagsUpdatedEventServerDataLoad>(event).is_some() {
            push(&self.0, "parent");
        }
        Box::pin(async {})
    }
}

/// `OnDatapackSyncEvent` fires for a joining player before the data maps, with the player.
#[test]
fn datapack_sync_event_of_a_join() {
    let _serial = support::serial();
    let server = support::server();
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<OnDatapackSyncEvent, _>(move |event| {
        if let Some(player) = &event.player {
            push(
                &seen_by_handler,
                format!(
                    "{} {}",
                    player.gameprofile.name,
                    event.relevant_players(&support::server().clone()).len()
                ),
            );
        }
    });
    let (player, _peer) = support::player_with_peer("SyncJoin", NegotiatedState::default());
    let world = support::world();

    let joined = support::RUNTIME.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(120),
            world.spawn_java_player(&server.basic_config, &player, server),
        )
        .await
    });
    assert!(joined.is_ok(), "spawn_java_player timed out");
    support::RUNTIME.block_on(world.remove_player(&player, false));

    assert_eq!(entries(&seen), ["SyncJoin 1"]);
}

const POOL: &[u8] = br#"{"type":"minecraft:block","pools":[{"rolls":1,"entries":[{"type":"minecraft:item","name":"minecraft:diamond"}]}]}"#;

const LOOT_FILES: PackFiles = &[
    ("data/events/loot_table/kept.json", POOL),
    ("data/events/loot_table/cancelled.json", POOL),
    ("data/events/loot_table/replaced.json", POOL),
];

fn pools(server: &Server, id: &str) -> usize {
    match server.datapack_manager.get_loot_table(id).unwrap().table {
        pumpkin_core::world::loot::LootTableRef::Dynamic(table) => table.pools.len(),
        pumpkin_core::world::loot::LootTableRef::Static(table) => table.pools.len(),
    }
}

/// `LootTableLoadEvent` fires once for each loaded table with its id. A cancel loads an empty
/// table, a replaced table loads as the handler left it, and a table that no handler touches
/// stays.
#[test]
fn loot_table_load_event_replaces_and_cancels() {
    let _serial = support::serial();
    let server = support::server();
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<LootTableLoadEvent, _>(move |event| {
        if !event.name.starts_with("events:") {
            return;
        }
        push(&seen_by_handler, event.name.clone());
        match event.name.as_str() {
            "events:cancelled" => event.cancelled = true,
            "events:replaced" => {
                let mut table = (*event.table).clone();
                table.pools.clear();
                event.table = Arc::new(table);
            }
            _ => {}
        }
    });
    let world = tempfile::tempdir().unwrap();
    let mod_root = tempfile::tempdir().unwrap();
    let pack = ModDataPack {
        mod_id: "events".to_owned(),
        files: LOOT_FILES,
    };
    server.datapack_manager.load_all_with_mod_packs(
        world.path(),
        &[],
        &RecipeManager::default(),
        &[pack],
        mod_root.path(),
    );
    assert_eq!(pools(server, "events:kept"), 1);

    server_events::server_data_loaded(server);

    assert_eq!(
        entries(&seen),
        ["events:cancelled", "events:kept", "events:replaced"]
    );
    assert_eq!(pools(server, "events:kept"), 1);
    assert_eq!(pools(server, "events:cancelled"), 0);
    assert_eq!(pools(server, "events:replaced"), 0);
}

const DIAMOND_TABLE: &str = r#"{"type":"minecraft:block","pools":[{"rolls":1,"entries":[{"type":"minecraft:item","name":"minecraft:diamond"}]}]}"#;

/// The names of the items that a roll of the table `id` gives.
fn drops(server: &Server, id: &str) -> Vec<String> {
    let handle = server.datapack_manager.get_loot_table(id).unwrap();
    generate_loot_from_handle(&handle, 1, &LootContextParameters::default())
        .iter()
        .map(|stack| stack.item.registry_key.to_owned())
        .collect()
}

/// `LootTableLoadEvent` also fires for the vanilla tables, which are compiled in: a replaced
/// vanilla table changes the drop, a cancelled one drops nothing, and a table that no handler
/// touches stays the compiled table.
#[test]
fn loot_table_load_event_changes_a_vanilla_table() {
    let _serial = support::serial();
    let server = support::server();
    assert_eq!(drops(server, "minecraft:blocks/dirt"), ["dirt"]);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<LootTableLoadEvent, _>(move |event| match event.name.as_str() {
        "minecraft:blocks/dirt" => {
            push(&seen_by_handler, "dirt");
            event.table = Arc::new(parse_loot_table(DIAMOND_TABLE).unwrap());
        }
        "minecraft:blocks/stone" => {
            push(&seen_by_handler, "stone");
            event.cancelled = true;
        }
        "minecraft:blocks/gravel" => push(&seen_by_handler, "gravel"),
        _ => {}
    });

    server_events::server_data_loaded(server);

    assert_eq!(entries(&seen), ["dirt", "gravel", "stone"]);
    assert_eq!(drops(server, "minecraft:blocks/dirt"), ["diamond"]);
    assert!(drops(server, "minecraft:blocks/stone").is_empty());
    assert!(matches!(
        server
            .datapack_manager
            .get_loot_table("minecraft:blocks/stone")
            .unwrap()
            .table,
        pumpkin_core::world::loot::LootTableRef::Dynamic(_)
    ));
    assert!(matches!(
        server
            .datapack_manager
            .get_loot_table("minecraft:blocks/gravel")
            .unwrap()
            .table,
        pumpkin_core::world::loot::LootTableRef::Static(_)
    ));
}

struct Listener(Log, &'static str);

impl ReloadListener for Listener {
    fn reload(
        &self,
        _server: &Arc<Server>,
        packs: &[pumpkin_core::data::datapack::LoadedDatapack],
    ) {
        push(&self.0, format!("{} {}", self.1, packs.len()));
    }
}

/// `AddServerReloadListenersEvent` fires at each load: the listeners run in registration order,
/// a dependency moves a listener behind the one it needs, and a duplicate key, an unknown key
/// and a cycle are errors.
#[test]
fn add_server_reload_listeners_event_runs_the_listeners_in_order() {
    let _serial = support::serial();
    let server = support::server();
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<AddServerReloadListenersEvent, _>(move |event| {
        if event
            .add_listener(
                "events:late",
                Arc::new(Listener(seen_by_handler.clone(), "late")),
            )
            .is_err()
        {
            return;
        }
        event
            .add_listener(
                "events:early",
                Arc::new(Listener(seen_by_handler.clone(), "early")),
            )
            .unwrap();
        assert_eq!(
            event
                .add_listener(
                    "events:early",
                    Arc::new(Listener(seen_by_handler.clone(), "x"))
                )
                .unwrap_err(),
            ReloadListenerError::DuplicateKey("events:early".to_owned())
        );
        assert_eq!(
            event
                .add_dependency("events:none", "events:late")
                .unwrap_err(),
            ReloadListenerError::UnknownKey("events:none".to_owned())
        );
        event.add_dependency("events:early", "events:late").unwrap();
        assert_eq!(event.keys(), ["events:late", "events:early"]);
    });

    server_events::server_data_loaded(server);

    assert_eq!(
        entries(&seen),
        [
            format!("early {}", server.datapack_manager.get_loaded_packs().len()),
            format!("late {}", server.datapack_manager.get_loaded_packs().len()),
        ]
    );
}

#[test]
fn a_cycle_of_reload_listener_dependencies_fails_the_sort() {
    let mut event = AddServerReloadListenersEvent::default();
    let seen = log();
    event
        .add_listener("a:one", Arc::new(Listener(seen.clone(), "one")))
        .unwrap();
    event
        .add_listener("a:two", Arc::new(Listener(seen, "two")))
        .unwrap();
    event.add_dependency("a:one", "a:two").unwrap();
    event.add_dependency("a:two", "a:one").unwrap();

    assert!(matches!(event.sorted(), Err(ReloadListenerError::Cycle)));
}

/// Calls a method of the server management protocol.
fn rpc(method: &str, params: serde_json::Value) {
    let request = RpcRequest {
        jsonrpc: Some("2.0".to_owned()),
        id: Some(serde_json::json!(1)),
        method: Some(method.to_owned()),
        params: Some(params),
    };
    let response = support::RUNTIME
        .block_on(dispatch(request, support::server()))
        .unwrap();
    assert!(response.error.is_none(), "{:?}", response.error);
}

fn operator(player: &Player) -> serde_json::Value {
    serde_json::json!({
        "player": {"id": player.gameprofile.id, "name": player.gameprofile.name},
        "permissionLevel": 4
    })
}

fn is_op(server: &Server, player: &Player) -> bool {
    op_level(server, player).is_some()
}

/// The operator methods of the server management protocol fire `PermissionsChangedEvent` before
/// the op list changes, as `PlayerList.op` and `deop` do for them: a cancel keeps the op list
/// entry as it is.
#[test]
fn the_management_operator_methods_fire_permissions_changed_event() {
    let _serial = support::serial();
    let server = support::server();
    let refused = support::player("RpcRefused");
    let allowed = support::player("RpcAllowed");
    let keep_allowed = Arc::new(AtomicBool::new(false));
    let keep = keep_allowed.clone();
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<PermissionsChangedEvent, _>(move |event| {
        let name = event.player.gameprofile.name.clone();
        if name == "RpcRefused" {
            event.cancelled = true;
        } else if name == "RpcAllowed" {
            push(
                &seen_by_handler,
                format!("{:?} -> {:?}", event.old_level, event.new_level),
            );
            event.cancelled = event.new_level == PermissionLvl::Zero && keep.load(Ordering::SeqCst);
        }
    });

    rpc(
        "operators/add",
        serde_json::json!({"add": [operator(&refused), operator(&allowed)]}),
    );
    assert!(!is_op(server, &refused));
    assert!(is_op(server, &allowed));

    rpc(
        "operators/set",
        serde_json::json!({"operators": [operator(&refused)]}),
    );
    assert!(!is_op(server, &refused));
    assert!(!is_op(server, &allowed));

    rpc(
        "operators/add",
        serde_json::json!({"add": [operator(&allowed)]}),
    );
    keep_allowed.store(true, Ordering::SeqCst);
    rpc(
        "operators/remove",
        serde_json::json!({"remove": [{"id": allowed.gameprofile.id}]}),
    );
    assert!(is_op(server, &allowed), "a cancel keeps the entry");
    rpc("operators/clear", serde_json::json!([]));
    assert!(is_op(server, &allowed), "a cancel keeps the entry");
    rpc("operators/set", serde_json::json!({"operators": []}));
    assert!(is_op(server, &allowed), "a cancel keeps the entry");
    keep_allowed.store(false, Ordering::SeqCst);
    rpc("operators/clear", serde_json::json!([]));
    assert!(!is_op(server, &allowed));

    assert_eq!(
        entries(&seen),
        [
            "Zero -> Four",
            "Four -> Zero",
            "Zero -> Four",
            "Four -> Zero",
            "Four -> Zero",
            "Four -> Zero",
            "Four -> Zero",
        ]
    );
}

/// `Player::change_permission_lvl`, which the permission level call of the plugins uses, fires
/// the event with the level of the player as the old level. A cancel keeps the level.
#[test]
fn change_permission_lvl_fires_permissions_changed_event() {
    let _serial = support::serial();
    let server = support::server();
    let player = support::player("LevelChange");
    on::<PermissionsChangedEvent, _>(|event| {
        if event.player.gameprofile.name == "LevelChange" {
            event.cancelled = event.new_level == PermissionLvl::Four;
        }
    });
    let dispatcher = server.command_dispatcher.load();

    assert!(!player.change_permission_lvl(server, PermissionLvl::Four, &dispatcher));
    assert_eq!(player.permission_lvl.load(), PermissionLvl::Zero);
    assert!(player.change_permission_lvl(server, PermissionLvl::Two, &dispatcher));
    assert_eq!(player.permission_lvl.load(), PermissionLvl::Two);
    assert!(player.change_permission_lvl(server, PermissionLvl::Two, &dispatcher));
}

static ORDER: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// A data map host that records when the server loads the data maps.
struct OrderHost;

impl DataMapHost for OrderHost {
    fn load(&self, _server: &Arc<Server>) {
        ORDER.lock().unwrap().push("data maps".to_owned());
    }

    fn known_data_maps(&self) -> KnownRegistryDataMapsPayload {
        KnownRegistryDataMapsPayload::default()
    }

    fn mandatory_data_maps(&self) -> Vec<String> {
        Vec::new()
    }

    fn sync_payloads(&self, _known: &BTreeMap<Identifier, BTreeSet<Identifier>>) -> Vec<Vec<u8>> {
        Vec::new()
    }
}

static ORDER_HOST: OrderHost = OrderHost;

/// The data maps load before the tags event and the sync event, as `NeoForge` loads them in its
/// own handler of `TagsUpdatedEvent.ServerDataLoad`: a handler of either event sees the new
/// values.
#[test]
fn the_data_maps_load_before_the_tags_and_the_datapack_sync_events() {
    let _serial = support::serial();
    let server = support::server();
    assert!(set_host(&ORDER_HOST));
    on::<TagsUpdatedEventServerDataLoad, _>(|_event| ORDER.lock().unwrap().push("tags".to_owned()));
    on::<OnDatapackSyncEvent, _>(|_event| ORDER.lock().unwrap().push("sync".to_owned()));
    ORDER.lock().unwrap().clear();

    server.reload_datapacks(server);

    assert_eq!(*ORDER.lock().unwrap(), ["data maps", "tags", "sync"]);
}

/// Reads what the peer got within a short time.
fn read_available(peer: &mut tokio::net::TcpStream) -> Vec<u8> {
    use tokio::io::AsyncReadExt;
    support::RUNTIME.block_on(async {
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        while let Ok(Ok(read)) =
            tokio::time::timeout(Duration::from_millis(500), peer.read(&mut buffer)).await
        {
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
        }
        bytes
    })
}

/// An op through the management protocol sends the entity status of the new permission level to
/// the player, as `PlayerList.sendPlayerPermissionLevel` does, and a deop sends level 0.
#[test]
fn a_management_op_sends_the_permission_packet() {
    let _serial = support::serial();
    let (player, mut peer) = support::player_with_peer("RpcPacket", NegotiatedState::default());
    let status = |level: i8| {
        let mut bytes = player.entity_id().to_be_bytes().to_vec();
        bytes.push(u8::try_from(24 + level).unwrap());
        bytes
    };
    let contains = |haystack: &[u8], needle: &[u8]| {
        haystack.windows(needle.len()).any(|window| window == needle)
    };

    rpc("operators/add", serde_json::json!({"add": [operator(&player)]}));
    assert!(contains(&read_available(&mut peer), &status(4)));

    rpc(
        "operators/remove",
        serde_json::json!({"remove": [{"id": player.gameprofile.id}]}),
    );
    assert!(contains(&read_available(&mut peer), &status(0)));
}

/// `PlayerList.op` passes the operator level of the server to the event, also when the call gives
/// the entry another level: the event has the server level and the op list has the given level.
#[test]
fn a_management_op_event_has_the_operator_level_of_the_server() {
    let _serial = support::serial();
    let server = support::server();
    let player = support::player("RpcLevel");
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<PermissionsChangedEvent, _>(move |event| {
        if event.player.gameprofile.name == "RpcLevel" {
            push(
                &seen_by_handler,
                format!("{:?} -> {:?}", event.old_level, event.new_level),
            );
        }
    });
    let mut entry = operator(&player);
    entry["permissionLevel"] = serde_json::json!(2);

    rpc("operators/add", serde_json::json!({"add": [entry]}));

    assert_eq!(
        entries(&seen),
        [format!(
            "Zero -> {:?}",
            server.basic_config.op_permission_level
        )]
    );
    assert_eq!(op_level(server, &player), Some(PermissionLvl::Two));
}
