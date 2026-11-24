//! The server, data pack and command events of `NeoForge` 26.3 reach v0.2 plugins (spec
//! `neoforge-event-bus`, requirement "Wasm plugins see the same events", #178): a v0.2 handler
//! gets each event with its fields, writes back the settable fields and cancels it, and the
//! host keeps the fields that `NeoForge` cannot set.
#![expect(clippy::panic, reason = "a test fails on the first unexpected record")]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

mod guest;

use std::sync::{Arc, Mutex, PoisonError};

use guest::{fire, plugin, resource};
use pumpkin_core::{
    command::CommandSender,
    plugin::{
        player::PermissionsChangedEvent,
        server::{
            CommandEvent, GameShuttingDownEvent, LootTableLoadEvent, OnDatapackSyncEvent,
            TagsUpdatedEventServerDataLoad,
        },
    },
};
use pumpkin_util::{PermissionLvl, loot_table::DynamicLootTable};
use pumpkin_wasm_host_v0_2::pumpkin::plugin::{event::Event, permission::PermissionLevel};

/// A handler reads the sender, the command and the exception, replaces the command and cancels
/// with a message. The host keeps the sender.
#[test]
fn a_plugin_replaces_the_command_and_cancels_with_an_exception() {
    let player = support::player("WasmCommandPlayer");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let manager = plugin::<CommandEvent, _>(move |event, state| {
        let Event::CommandEvent(data) = event else {
            panic!("expected command");
        };
        let sender = data.player.as_ref().map(|player| resource(state, player));
        record.lock().unwrap_or_else(PoisonError::into_inner).push((
            sender.map(|player| player.gameprofile.name.clone()),
            data.sender_name.clone(),
            data.command.clone(),
            data.exception.clone(),
        ));
        data.command = "list".to_owned();
        data.exception = Some("refused".to_owned());
        data.cancelled = true;
    });
    let source = CommandSender::Player(player).into_source(support::server());

    let event = fire(&manager, CommandEvent::new(source, "say hi".to_owned()));

    assert!(event.cancelled);
    assert_eq!(event.command, "list");
    assert_eq!(event.exception.as_deref(), Some("refused"));
    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        [(
            Some("WasmCommandPlayer".to_owned()),
            "WasmCommandPlayer".to_owned(),
            "say hi".to_owned(),
            None
        )]
    );
}

/// The console has no player resource.
#[test]
fn the_console_sender_of_a_command_has_no_player() {
    let manager = plugin::<CommandEvent, _>(|event, _state| {
        let Event::CommandEvent(data) = event else {
            panic!("expected command");
        };
        assert!(data.player.is_none());
        assert_eq!(data.sender_name, "Server");
    });
    let source = CommandSender::Console.into_source(support::server());

    let event = fire(&manager, CommandEvent::new(source, "seed".to_owned()));

    assert!(!event.cancelled);
    assert_eq!(event.command, "seed");
}

#[test]
fn a_plugin_reads_the_levels_of_a_permission_change_and_cancels_it() {
    let player = support::player("WasmPermissionPlayer");
    let manager = plugin::<PermissionsChangedEvent, _>(|event, state| {
        let Event::PermissionsChangedEvent(data) = event else {
            panic!("expected permissions changed");
        };
        assert_eq!(
            resource(state, &data.player).gameprofile.name,
            "WasmPermissionPlayer"
        );
        assert!(matches!(data.old_level, PermissionLevel::Zero));
        assert!(matches!(data.new_level, PermissionLevel::Four));
        data.new_level = PermissionLevel::One;
        data.cancelled = true;
    });

    let event = fire(
        &manager,
        PermissionsChangedEvent::new(player, PermissionLvl::Four, PermissionLvl::Zero),
    );

    assert!(event.cancelled);
    assert_eq!(event.new_level, PermissionLvl::Four);
    assert_eq!(event.old_level, PermissionLvl::Zero);
}

#[test]
fn a_plugin_receives_the_player_of_a_datapack_sync() {
    let player = support::player("WasmSyncPlayer");
    let names = Arc::new(Mutex::new(Vec::new()));
    let record = names.clone();
    let manager = plugin::<OnDatapackSyncEvent, _>(move |event, state| {
        let Event::OnDatapackSyncEvent(data) = event else {
            panic!("expected datapack sync");
        };
        record.lock().unwrap_or_else(PoisonError::into_inner).push(
            data.player
                .as_ref()
                .map(|player| resource(state, player).gameprofile.name.clone()),
        );
    });

    fire(&manager, OnDatapackSyncEvent::new(Some(player)));
    fire(&manager, OnDatapackSyncEvent::new(None));

    assert_eq!(
        *names.lock().unwrap_or_else(PoisonError::into_inner),
        [Some("WasmSyncPlayer".to_owned()), None]
    );
}

#[test]
fn a_plugin_cancels_a_loot_table_load_and_the_host_keeps_the_table() {
    let manager = plugin::<LootTableLoadEvent, _>(|event, _state| {
        let Event::LootTableLoadEvent(data) = event else {
            panic!("expected loot table load");
        };
        assert_eq!(data.name, "events:gems");
        data.cancelled = true;
    });
    let table = Arc::new(DynamicLootTable::default());

    let event = fire(
        &manager,
        LootTableLoadEvent::new("events:gems".to_owned(), table.clone()),
    );

    assert!(event.cancelled);
    assert!(Arc::ptr_eq(&event.table, &table));
}

#[test]
fn a_plugin_receives_the_tags_update_and_the_shutdown() {
    let tags = plugin::<TagsUpdatedEventServerDataLoad, _>(|event, _state| {
        assert!(matches!(event, Event::TagsUpdatedEventServerDataLoad));
    });
    let shutdown = plugin::<GameShuttingDownEvent, _>(|event, _state| {
        assert!(matches!(event, Event::GameShuttingDownEvent));
    });

    fire(&tags, TagsUpdatedEventServerDataLoad);
    fire(&shutdown, GameShuttingDownEvent);
}
