use std::sync::Arc;

use pumpkin_core::plugin::guest_call::{GuestCall, InGuestCall};
use pumpkin_plugin_runtime::{RuntimeSpawner, SpawnError, SpawnFuture, StoreExecutor};
use wasmtime::Store;

use super::{deferred::DeferredQueue, state::PluginHostState};

pub use pumpkin_plugin_runtime::{LegacySyncReentry, StoreMode};

pub type LegacyStore = StoreExecutor<PluginHostState, LegacySyncReentry>;

pub struct TokioSpawner {
    runtime: tokio::runtime::Handle,
}

impl TokioSpawner {
    #[must_use]
    pub const fn new(runtime: tokio::runtime::Handle) -> Self {
        Self { runtime }
    }
}

impl RuntimeSpawner for TokioSpawner {
    fn spawn(&self, task: SpawnFuture) -> Result<(), SpawnError> {
        drop(self.runtime.spawn(task));
        Ok(())
    }

    fn spawn_blocking(&self, task: Box<dyn FnOnce() + Send + 'static>) -> Result<(), SpawnError> {
        drop(self.runtime.spawn_blocking(task));
        Ok(())
    }
}

/// The guest calls of one store: the work that they defer goes to the queue of the store.
struct StoreGuestCall {
    queue: DeferredQueue,
    runtime: tokio::runtime::Handle,
}

impl GuestCall for StoreGuestCall {
    fn defer(&self, label: &str, run: Box<dyn FnOnce() + Send>) {
        self.queue.push(&self.runtime, label, run);
    }
}

/// Spawns the store driver so that each poll marks the thread with the guest call of the store.
/// The driver runs the guest code and the synchronous imports, while the guest call holds the
/// root permit.
struct GuestCallSpawner {
    inner: Arc<dyn RuntimeSpawner>,
    call: Arc<dyn GuestCall>,
}

impl RuntimeSpawner for GuestCallSpawner {
    fn spawn(&self, task: SpawnFuture) -> Result<(), SpawnError> {
        self.inner
            .spawn(Box::pin(InGuestCall::new(Arc::clone(&self.call), task)))
    }

    fn spawn_blocking(&self, task: Box<dyn FnOnce() + Send + 'static>) -> Result<(), SpawnError> {
        self.inner.spawn_blocking(task)
    }
}

/// Starts the driver of `store`.
///
/// A [`StoreMode::Concurrent`] store (v0.2) marks its driver thread with
/// [`pumpkin_core::plugin::guest_call`]. A sequential store (v0.1) runs the synchronous work of
/// its imports through `pump_blocking`, which re-enters the store, so it needs no mark.
pub async fn start_legacy_store(
    store: Store<PluginHostState>,
    policy: LegacySyncReentry,
    spawner: Arc<dyn RuntimeSpawner>,
    mode: StoreMode,
) -> wasmtime::Result<LegacyStore> {
    let spawner: Arc<dyn RuntimeSpawner> = match mode {
        StoreMode::Concurrent => Arc::new(GuestCallSpawner {
            inner: spawner,
            call: Arc::new(StoreGuestCall {
                queue: store.data().deferred.clone(),
                runtime: tokio::runtime::Handle::current(),
            }),
        }),
        StoreMode::Sequential => spawner,
    };
    StoreExecutor::start(store, policy, spawner, mode).await
}
