//! The v0.2 conversions of the `NeoForge` 26.3 server, data pack and command events of the root
//! `event` package, and of `PermissionsChangedEvent`.
//!
//! A blocking handler writes back only the settable fields; the other fields keep the native
//! values, as `NeoForge` has no setter for them. `AddServerReloadListenersEvent` has no record:
//! its only use is to add a reload listener, which is host code.

use std::sync::Arc;

use pumpkin_core::plugin::{
    Context, EventPriority,
    player::PermissionsChangedEvent,
    server::{
        CommandEvent, GameShuttingDownEvent, LootTableLoadEvent, OnDatapackSyncEvent,
        TagsUpdatedEventServerDataLoad,
    },
};
use pumpkin_wasm_host_common::state::PluginHostState;

use crate::{
    events::{
        ToFromWasmEvent, WasmPluginEventHandler,
        neoforge::{add, discard},
    },
    player::to_wit_permission_level,
    pumpkin::plugin::{
        event::{Event, EventType},
        neoforge_events::{
            CommandEventData, LootTableLoadEventData, OnDatapackSyncEventData,
            PermissionsChangedEventData,
        },
    },
};

macro_rules! server_event {
    ($($event:tt)*) => {
        neoforge_event!(cleanup_neoforge_server_event; $($event)*);
    };
}

server_event!(
    CommandEvent,
    CommandEvent,
    |this, state| CommandEventData {
        player: this
            .source
            .output
            .as_player()
            .map(|player| add(state, player)),
        sender_name: this.source.name.clone(),
        command: this.command.clone(),
        exception: this.exception.clone(),
        cancelled: this.cancelled,
    },
    |target, data| {
        target.command = data.command;
        target.exception = data.exception;
        target.cancelled = data.cancelled;
    }
);

server_event!(
    PermissionsChangedEvent,
    PermissionsChangedEvent,
    |this, state| PermissionsChangedEventData {
        player: add(state, this.player.clone()),
        new_level: to_wit_permission_level(this.new_level),
        old_level: to_wit_permission_level(this.old_level),
        cancelled: this.cancelled,
    },
    |target, data| {
        target.cancelled = data.cancelled;
    }
);

server_event!(
    OnDatapackSyncEvent,
    OnDatapackSyncEvent,
    |this, state| OnDatapackSyncEventData {
        player: this.player.clone().map(|player| add(state, player)),
    },
    |target, data| {}
);

server_event!(
    LootTableLoadEvent,
    LootTableLoadEvent,
    |this, _state| LootTableLoadEventData {
        name: this.name.clone(),
        cancelled: this.cancelled,
    },
    |target, data| {
        target.cancelled = data.cancelled;
    }
);

impl ToFromWasmEvent for TagsUpdatedEventServerDataLoad {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::TagsUpdatedEventServerDataLoad
    }

    fn apply_wasm_event(&mut self, _event: Event, _state: &mut PluginHostState) {}

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct TagsUpdatedEventServerDataLoad from WASM")
    }
}

impl ToFromWasmEvent for GameShuttingDownEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::GameShuttingDownEvent
    }

    fn apply_wasm_event(&mut self, _event: Event, _state: &mut PluginHostState) {}

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct GameShuttingDownEvent from WASM")
    }
}

/// Releases the resources of a record that the host built for a handler.
pub(super) fn cleanup_neoforge_server_event(event: &Event, state: &mut PluginHostState) {
    match event {
        Event::CommandEvent(CommandEventData { player, .. })
        | Event::OnDatapackSyncEvent(OnDatapackSyncEventData { player }) => {
            if let Some(player) = player {
                discard(state, player);
            }
        }
        Event::PermissionsChangedEvent(PermissionsChangedEventData { player, .. }) => {
            discard(state, player);
        }
        _ => {}
    }
}

/// Registers the handler of a Wasm plugin for a server, data pack or command event.
pub fn register_neoforge_server_event(
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
        EventType::CommandEvent => on::<CommandEvent>(ctx, handler, priority, blocking),
        EventType::PermissionsChangedEvent => {
            on::<PermissionsChangedEvent>(ctx, handler, priority, blocking);
        }
        EventType::TagsUpdatedEventServerDataLoad => {
            on::<TagsUpdatedEventServerDataLoad>(ctx, handler, priority, blocking);
        }
        EventType::OnDatapackSyncEvent => {
            on::<OnDatapackSyncEvent>(ctx, handler, priority, blocking);
        }
        EventType::LootTableLoadEvent => {
            on::<LootTableLoadEvent>(ctx, handler, priority, blocking);
        }
        EventType::GameShuttingDownEvent => {
            on::<GameShuttingDownEvent>(ctx, handler, priority, blocking);
        }
        _ => tracing::error!(
            "non-server NeoForge event should not be routed to register_neoforge_server_event"
        ),
    }
}
