//! The server lifecycle events of `NeoForge` that Pumpkin has no event for.
//!
//! `ServerStartedEvent` is [`ServerLoadEvent`](super::server_load::ServerLoadEvent). The server is the `server`
//! argument of the handler, as `ServerLifecycleEvent#getServer`.

use pumpkin_macros::Event;

/// Fires after the mod lifecycle and before the worlds load, as the first event of the server.
///
/// `NeoForge`: `net.neoforged.neoforge.event.server.ServerAboutToStartEvent`, fired by
/// `ServerLifecycleHooks.handleServerAboutToStart` in `DedicatedServer.initServer` before
/// `loadLevel`, after the synced configs load. The Wasm plugins load later, so only native mods
/// receive it. The datapack recipes and tags are not loaded yet: Pumpkin loads them after the
/// worlds, while `NeoForge` has them in the `WorldStem` before this event.
#[derive(Event, Clone, Copy, Default)]
pub struct ServerAboutToStartEvent;

/// Fires after the worlds load and before the first tick and the first connection.
///
/// `NeoForge`: `net.neoforged.neoforge.event.server.ServerStartingEvent`, fired by
/// `ServerLifecycleHooks.handleServerStarting` at the end of `DedicatedServer.initServer`.
#[derive(Event, Clone, Copy, Default)]
pub struct ServerStartingEvent;

/// Fires when the server stops accepting connections, before it saves the players and the
/// worlds.
///
/// `NeoForge`: `net.neoforged.neoforge.event.server.ServerStoppingEvent`, fired by
/// `ServerLifecycleHooks.handleServerStopping` at the head of `MinecraftServer.stopServer`.
#[derive(Event, Clone, Copy, Default)]
pub struct ServerStoppingEvent;

/// Fires after the server saved the worlds, as the last event of the server.
///
/// `NeoForge`: `net.neoforged.neoforge.event.server.ServerStoppedEvent`, fired by
/// `ServerLifecycleHooks.handleServerStopped` after `MinecraftServer.stopServer`. The Wasm
/// plugins are unloaded before it, so only native mods receive it.
#[derive(Event, Clone, Copy, Default)]
pub struct ServerStoppedEvent;
