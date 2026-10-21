//! A v0.2 plugin handler without a Wasm guest: the host lowers the event to its WIT record as
//! `handle-event` does, a closure reads and edits the record as the guest would, and the host
//! applies the returned record. The guest drops a resource with `drop_resource`, as a guest that
//! drops an owned handle.
#![allow(dead_code, reason = "each test binary uses a part of the helper")]
#![expect(
    clippy::expect_used,
    reason = "a test fails on the first missing value"
)]

use std::{
    marker::PhantomData,
    sync::{Arc, Mutex, PoisonError},
};

use pumpkin_core::{
    plugin::{BoxFuture, EventHandler, EventPriority, Payload, PluginManager},
    server::Server,
};
use pumpkin_wasm_host_common::state::{FromResource, PluginHostState};
use pumpkin_wasm_host_v0_2::{
    events::{ToFromWasmEvent, cleanup_event},
    pumpkin::plugin::event::Event,
};
use wasmtime::component::Resource;

pub struct Guest<E, F> {
    state: Mutex<PluginHostState>,
    edit: F,
    _event: PhantomData<fn() -> E>,
}

impl<E, F> Guest<E, F> {
    pub fn new(guest: F) -> Self {
        Self {
            state: Mutex::new(PluginHostState::new()),
            edit: guest,
            _event: PhantomData,
        }
    }
}

impl<E, F> EventHandler<E> for Guest<E, F>
where
    E: Payload + ToFromWasmEvent + Send + Sync + 'static,
    F: Fn(&mut Event, &mut PluginHostState) + Send + Sync,
{
    fn handle<'a>(&'a self, _server: &'a Arc<Server>, event: &'a E) -> BoxFuture<'a, ()> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let mut wasm = event.to_wasm_event(&mut state);
        (self.edit)(&mut wasm, &mut state);
        cleanup_event(&wasm, &mut state);
        Box::pin(async {})
    }

    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut E,
    ) -> BoxFuture<'a, ()> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let mut wasm = event.to_wasm_event(&mut state);
        (self.edit)(&mut wasm, &mut state);
        event.apply_wasm_event(wasm, &mut state);
        Box::pin(async {})
    }
}

/// A plugin manager with one blocking v0.2 handler for `E`, registered as
/// `Context::register_event` registers it: it also receives cancelled events.
pub fn plugin<E, F>(guest: F) -> PluginManager
where
    E: Payload + ToFromWasmEvent + Send + Sync + 'static,
    F: Fn(&mut Event, &mut PluginHostState) + Send + Sync + 'static,
{
    let manager = PluginManager::new(Vec::new());
    manager.register_with::<E, _>(
        Arc::new(Guest::new(guest)),
        EventPriority::Normal,
        true,
        true,
    );
    manager
}

/// Fires `event` through `manager` and returns it with the changes of the handlers.
pub fn fire<E: Payload + Send + Sync + 'static>(manager: &PluginManager, event: E) -> E {
    manager
        .fire_blocking_with(crate::support::server(), || event)
        .expect("the plugin listens to the event")
}

/// The value of a resource of the record, as the host method of the resource reads it.
pub fn resource<T: FromResource>(state: &PluginHostState, res: &Resource<T>) -> T::Internal
where
    T::Internal: Clone,
{
    state.get(res).expect("live resource").clone()
}

pub fn drop_resource<T: FromResource>(state: &mut PluginHostState, res: &Resource<T>) {
    state.discard_to_be_removed(res);
}
