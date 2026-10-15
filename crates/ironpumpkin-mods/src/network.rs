//! Custom payload channels, payload handlers and configuration tasks, with the semantics of the
//! `NeoForge` network API.
//!
//! A mod registers its channels in a handler of [`RegisterPayloadHandlersEvent`]. The event fires
//! once at startup, after `load_complete` of every mod and before the synced configs load. Each
//! [`PayloadRegistrar`] method registers one channel with a direction, a phase, the version of the
//! registrar, its optional flag and its handler thread. The server offers every channel in the
//! `NeoForge` channel negotiation: a `NeoForge` client without a required channel, or with another
//! version of it, is disconnected with `neoforge:modded_network_setup_failed`, and a client
//! without `NeoForge` cannot join while a required channel exists.
//!
//! A payload is raw bytes. The handler of a channel gets the bytes and a [`PayloadContext`] once
//! per payload. In play it runs on the world tick of the player ([`HandlerThread::Main`]) or on
//! the connection task ([`HandlerThread::Network`]). In configuration it runs on the connection
//! task, because the player is not in a world yet. A payload on a channel that the client did
//! not negotiate, and that is not optional, disconnects the client, as on `NeoForge`. A payload on
//! a channel that no mod registered reaches the plugin events (`ConfigCustomPayloadEvent`,
//! `PlayerCustomPayloadEvent`), and no mod handler.
//!
//! [`send_to_player`] fails with [`NetworkError::ChannelNotNegotiated`] for a client that cannot
//! receive the channel. [`send_to_all_players`] and [`send_to_players_in_dimension`] skip such
//! clients.
//!
//! A configuration task ([`CustomConfigurationTask`]) is registered per connection in a handler
//! of [`RegisterConfigurationTasksEvent`]. The tasks run in registration order after the tasks of
//! `NeoForge` and before the end of the configuration, and each waits until it finishes: in
//! [`CustomConfigurationTask::run`] through [`TaskContext::finish`], or later in the handler of
//! the client's answer through [`PayloadContext::finish_current_task`]. A finish request for a
//! task that is not the current task disconnects the client.
//!
//! ```
//! use std::sync::Arc;
//!
//! use ironpumpkin_mods::{
//!     ModInit, NativeMod,
//!     event::{BoxFuture, EventHandler, EventPriority},
//!     network::{PayloadContext, RegisterPayloadHandlersEvent},
//!     server::Server,
//!     tracing::error,
//! };
//!
//! struct Payloads;
//!
//! impl EventHandler<RegisterPayloadHandlersEvent> for Payloads {
//!     fn handle<'a>(
//!         &'a self,
//!         _server: &'a Arc<Server>,
//!         event: &'a RegisterPayloadHandlersEvent,
//!     ) -> BoxFuture<'a, ()> {
//!         // The registrar must outlive the chain: each call returns a reference to it.
//!         let registrar = event.registrar("1");
//!         let registered = registrar
//!             .play_to_server("examplemod:set_level", |data, context: &mut PayloadContext<'_>| {
//!                 if let Some(player) = context.player() {
//!                     let _ = (player, data);
//!                 }
//!             })
//!             .and_then(|registrar| registrar.play_to_client("examplemod:level"));
//!         if let Err(err) = registered {
//!             error!("examplemod: {err}");
//!         }
//!         Box::pin(async {})
//!     }
//! }
//!
//! struct ExampleMod;
//!
//! impl NativeMod for ExampleMod {
//!     fn id(&self) -> &'static str {
//!         "examplemod"
//!     }
//!     fn display_name(&self) -> &'static str {
//!         "Example Mod"
//!     }
//!     fn version(&self) -> &'static str {
//!         "1.0.0"
//!     }
//!     fn init(&self, cx: &mut ModInit) {
//!         cx.register_event::<RegisterPayloadHandlersEvent, _>(
//!             Arc::new(Payloads),
//!             EventPriority::Normal,
//!             false,
//!             false,
//!         );
//!     }
//! }
//! ```

pub use pumpkin_core::plugin::api::events::network::{
    ConnectionProtocol, ConnectionType, CustomConfigurationTask, HandlerThread, NegotiatedChannels,
    NetworkError, PacketFlow, PayloadContext, PayloadHandler, PayloadRegistrar,
    RegisterConfigurationTasksEvent, RegisterPayloadHandlersEvent, RegistrarResult, TaskContext,
};
pub use pumpkin_core::plugin::network_events::{
    negotiated_channels, send_to_all_players, send_to_player, send_to_players_in_dimension,
};
