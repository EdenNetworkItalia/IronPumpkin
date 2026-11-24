//! An ordered queue of host work that a synchronous import of a Wasm plugin defers until the guest
//! call can release the store.
//!
//! A synchronous v0.2 import runs while the guest call holds the root permit of its store. Work that
//! fires an event can reach a Wasm handler, and that handler needs the permit, so the import cannot
//! run the work itself. The import pushes the work here instead. One blocking task drains the
//! queue in push order, so two sounds, or a sound and a stop of that sound, keep the order of the
//! calls. A store has one queue and at most one drain task.

use std::{
    collections::VecDeque,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicUsize, Ordering},
    },
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

/// See the [module documentation](self).
#[derive(Clone, Default)]
pub struct DeferredQueue {
    inner: Arc<Mutex<Inner>>,
    /// The depth of the work that the drain task runs now, plus one: the depth of work that this
    /// work defers. Zero while the drain task is idle.
    next_depth: Arc<AtomicUsize>,
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
        let label = label.into();
        let depth = self.next_depth.load(Ordering::Acquire);
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
                    self.next_depth.store(0, Ordering::Release);
                    return;
                };
                task
            };
            self.next_depth.store(task.depth + 1, Ordering::Release);
            // A panic of the work must not stop the drain: the later work still runs.
            if let Err(payload) = catch_unwind(AssertUnwindSafe(task.run)) {
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
