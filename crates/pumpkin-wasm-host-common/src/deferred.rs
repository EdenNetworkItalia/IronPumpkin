//! An ordered queue of host work that a synchronous import of a Wasm plugin defers until the guest
//! call can release the store.
//!
//! A synchronous v0.2 import runs while the guest call holds the root permit of its store. Work that
//! fires an event can reach a Wasm handler, and that handler needs the permit, so the import cannot
//! run the work itself. The import pushes the work here instead. One blocking task drains the
//! queue in push order, so two sounds, or a sound and a stop of that sound, keep the order of the
//! calls. A store has one queue and at most one drain task.
//!
//! The depth of work counts the chain of causes across all stores. Work that a guest call defers
//! has the depth of that call. A root guest call that deferred work causes, on the drain thread,
//! has the depth of that work plus one: the root admission reads it from this thread
//! ([`pumpkin_plugin_runtime::DEFERRED_DEPTH_SOURCE`]). A root call that no deferred work causes
//! has depth 0, and a nested call keeps the depth of its root. Thus two plugins that answer each
//! other's work stop at the limit, and unrelated work does not add depth.

use std::{
    cell::Cell,
    collections::VecDeque,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, PoisonError},
};

use tracing::error;

/// The depth at which the queue drops work: a handler that defers work that fires the same event
/// again would loop forever. It is the limit of `MAX_SYNC_REENTRY_DEPTH` of the v0.1 host.
pub const MAX_DEFERRED_DEPTH: usize = 64;

struct Task {
    label: String,
    depth: usize,
    run: Box<dyn FnOnce() + Send>,
}

#[derive(Default)]
struct Inner {
    tasks: VecDeque<Task>,
    draining: bool,
}

thread_local! {
    /// The depth of the deferred work that runs on this thread, while a drain task runs it.
    static RUNNING_DEPTH: Cell<Option<usize>> = const { Cell::new(None) };
}

/// The depth of a root guest call that this thread admits: one more than the deferred work that
/// runs on this thread, or 0 when no deferred work runs on it.
fn admitted_depth() -> usize {
    RUNNING_DEPTH.get().map_or(0, |depth| depth + 1)
}

/// See the [module documentation](self).
#[derive(Clone, Default)]
pub struct DeferredQueue {
    inner: Arc<Mutex<Inner>>,
}

impl DeferredQueue {
    /// Queues `run`, which the drain task calls after the earlier work. `label` names the work
    /// in the log when it panics or when the depth limit drops it.
    pub fn push(
        &self,
        runtime: &tokio::runtime::Handle,
        label: impl Into<String>,
        run: impl FnOnce() + Send + 'static,
    ) {
        let _ = pumpkin_plugin_runtime::DEFERRED_DEPTH_SOURCE.set(admitted_depth);
        let label = label.into();
        let depth = pumpkin_plugin_runtime::root_deferred_depth();
        if depth >= MAX_DEFERRED_DEPTH {
            error!(
                "Dropped the deferred host work `{label}`: it exceeds the depth of {MAX_DEFERRED_DEPTH}"
            );
            return;
        }
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.tasks.push_back(Task {
            label,
            depth,
            run: Box::new(run),
        });
        if !inner.draining {
            inner.draining = true;
            let queue = self.clone();
            runtime.spawn_blocking(move || queue.drain());
        }
    }

    fn drain(&self) {
        loop {
            let task = {
                let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
                let Some(task) = inner.tasks.pop_front() else {
                    inner.draining = false;
                    return;
                };
                task
            };
            RUNNING_DEPTH.set(Some(task.depth));
            // A panic of the work must not stop the drain: the later work still runs.
            let outcome = catch_unwind(AssertUnwindSafe(task.run));
            RUNNING_DEPTH.set(None);
            if let Err(payload) = outcome {
                let message = payload
                    .downcast_ref::<&str>()
                    .map(|message| (*message).to_owned())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_default();
                error!(
                    "The deferred host work `{}` panicked: {message}",
                    task.label
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_root_call_has_the_depth_of_the_work_that_runs_on_its_thread_plus_one() {
        assert_eq!(admitted_depth(), 0);
        RUNNING_DEPTH.set(Some(3));
        assert_eq!(admitted_depth(), 4);
        RUNNING_DEPTH.set(None);
        assert_eq!(admitted_depth(), 0);
    }
}
