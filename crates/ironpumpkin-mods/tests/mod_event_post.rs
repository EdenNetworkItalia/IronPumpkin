//! `IEventBus#post` and `ModList#get` on a real server (`openspec/changes/neoforge-shaped-plugin-api`,
//! spec `neoforge-event-bus`): the mod `poster` defines its own event type and posts it with
//! `PluginManager::fire_blocking`, the mod `listener` handles it, and `ModList::get` lists both
//! mods after `init_mods`. This file is its own test binary: the mod list is set once per process.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

use std::{
    any::Any,
    sync::{Arc, Mutex, PoisonError},
};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    event::{BoxFuture, EventHandler, EventPriority, Payload},
    mod_list::ModList,
    pumpkin_core::plugin::startup::STARTUP_HOOK,
    register_mod,
    server::Server,
};

static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn log(entry: String) {
    LOG.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(entry);
}

/// An event type of the mod `poster`. A mod implements `Payload` for its own events.
struct PosterEvent {
    text: String,
    replies: Vec<String>,
}

impl Payload for PosterEvent {
    fn get_name_static() -> &'static str {
        "PosterEvent"
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
}

struct Poster;

impl NativeMod for Poster {
    fn id(&self) -> &'static str {
        "poster"
    }

    fn display_name(&self) -> &'static str {
        "Poster"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        // The poster handles its own event too, at a later priority than the listener.
        cx.register_event::<PosterEvent, _>(
            Arc::new(Reply("poster")),
            EventPriority::Low,
            true,
            false,
        );
    }
}

struct Listener;

impl NativeMod for Listener {
    fn id(&self) -> &'static str {
        "listener"
    }

    fn display_name(&self) -> &'static str {
        "Listener"
    }

    fn version(&self) -> &'static str {
        "2.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        cx.register_event::<PosterEvent, _>(
            Arc::new(Reply("listener")),
            EventPriority::High,
            true,
            false,
        );
    }
}

register_mod!(Poster);
register_mod!(Listener);

struct Reply(&'static str);

impl EventHandler<PosterEvent> for Reply {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut PosterEvent,
    ) -> BoxFuture<'a, ()> {
        log(format!("{} handles {}", self.0, event.text));
        event.replies.push(self.0.to_owned());
        Box::pin(async {})
    }
}

/// Sets the startup hook before the first use of the shared server; a second call changes nothing.
fn boot() {
    let _ = STARTUP_HOOK.set(ironpumpkin_mods::init_mods);
}

#[test]
fn a_mod_posts_its_own_event_and_another_mod_handles_it() {
    boot();
    let server = support::server();
    let mut event = PosterEvent {
        text: "ping".to_owned(),
        replies: Vec::new(),
    };

    server.plugin_manager.fire_blocking(server, &mut event);

    // The handlers of both mods run on the one bus, by priority.
    assert_eq!(event.replies, ["listener", "poster"]);
    assert_eq!(
        *LOG.lock().unwrap(),
        ["listener handles ping", "poster handles ping"]
    );
}

#[test]
fn mod_list_get_lists_the_mods_that_init_mods_loaded() {
    boot();
    let _ = support::server();
    let list = ModList::get();
    assert!(list.is_loaded("poster") && list.is_loaded("listener"));
    assert!(list.is_loaded("minecraft") && list.is_loaded("neoforge"));
    assert_eq!(list.size(), 4);
    let ids: Vec<&str> = list
        .get_sorted_mods()
        .iter()
        .map(|info| info.mod_id.as_str())
        .collect();
    assert!(ids.contains(&"poster") && ids.contains(&"listener"));
}
