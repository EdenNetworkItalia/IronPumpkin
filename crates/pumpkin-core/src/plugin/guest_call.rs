//! The Wasm guest call that runs on the current thread.
//!
//! A Wasm plugin store runs its guest calls one at a time across all stores: a guest call holds
//! the root permit. A host import that fires an event with a blocking fire cannot reach a Wasm
//! handler, because that handler waits for the same permit. The host of the Wasm plugins marks the
//! thread with [`enter`] while a store runs guest code, so that the server code can see the guest
//! call and avoid the wait:
//!
//! - The outbound packet path defers the send through [`GuestCall::defer`] when a Wasm handler of
//!   `PacketSentEvent` listens.
//! - A host import that returns a value from an event runs in [`native_handlers_only`], and its
//!   blocking fires skip the Wasm handlers.

use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

/// The guest call of a Wasm plugin store, as [`enter`] marks it on the thread.
pub trait GuestCall: Send + Sync {
    /// Runs `run` after the guest call returns, after the work that the store deferred before it.
    /// `label` names the work in the log.
    fn defer(&self, label: &str, run: Box<dyn FnOnce() + Send>);
}

thread_local! {
    static CURRENT: RefCell<Option<Arc<dyn GuestCall>>> = const { RefCell::new(None) };
    static NATIVE_ONLY: Cell<bool> = const { Cell::new(false) };
}

/// Restores the mark that the thread had before, also when the marked code panics.
enum Restore {
    /// The guest call and the native-only flag of [`enter`].
    Call {
        previous: Option<Arc<dyn GuestCall>>,
        native_only: bool,
    },
    /// The native-only flag of [`native_handlers_only`].
    NativeOnly(bool),
}

impl Drop for Restore {
    fn drop(&mut self) {
        match self {
            Self::Call {
                previous,
                native_only,
            } => {
                CURRENT.set(previous.take());
                NATIVE_ONLY.set(*native_only);
            }
            Self::NativeOnly(native_only) => NATIVE_ONLY.set(*native_only),
        }
    }
}

/// Runs `run` with the thread marked as inside `call`.
pub fn enter<R>(call: &Arc<dyn GuestCall>, run: impl FnOnce() -> R) -> R {
    let _restore = Restore::Call {
        previous: CURRENT.replace(Some(Arc::clone(call))),
        native_only: NATIVE_ONLY.replace(false),
    };
    run()
}

/// The guest call of the current thread, if a store runs guest code on it.
#[must_use]
pub fn current() -> Option<Arc<dyn GuestCall>> {
    CURRENT.with_borrow(Clone::clone)
}

/// Runs `run`, whose blocking fires inside a guest call reach the native handlers only.
///
/// A Wasm handler would wait for the root permit that the guest call holds. Outside a guest call,
/// `run` fires to every handler.
pub fn native_handlers_only<R>(run: impl FnOnce() -> R) -> R {
    let _restore = Restore::NativeOnly(NATIVE_ONLY.replace(true));
    run()
}

/// Whether a blocking fire on this thread skips the Wasm handlers.
pub(crate) fn skips_wasm_handlers() -> bool {
    NATIVE_ONLY.get() && CURRENT.with_borrow(Option::is_some)
}

/// A future that [`enter`]s its guest call on each poll.
pub struct InGuestCall<F> {
    call: Arc<dyn GuestCall>,
    future: Pin<Box<F>>,
}

impl<F: Future> InGuestCall<F> {
    pub fn new(call: Arc<dyn GuestCall>, future: F) -> Self {
        Self {
            call,
            future: Box::pin(future),
        }
    }
}

impl<F: Future> Future for InGuestCall<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        enter(&this.call, || this.future.as_mut().poll(cx))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<String>>);

    impl GuestCall for Recorder {
        fn defer(&self, label: &str, _run: Box<dyn FnOnce() + Send>) {
            self.0.lock().unwrap().push(label.to_owned());
        }
    }

    #[test]
    fn the_mark_lives_only_inside_enter() {
        let call: Arc<dyn GuestCall> = Arc::new(Recorder::default());
        assert!(current().is_none());
        enter(&call, || {
            current().unwrap().defer("inside", Box::new(|| {}));
            assert!(!skips_wasm_handlers());
            native_handlers_only(|| assert!(skips_wasm_handlers()));
            assert!(!skips_wasm_handlers());
        });
        assert!(current().is_none());
        native_handlers_only(|| assert!(!skips_wasm_handlers()));
    }

    #[test]
    fn a_panic_inside_enter_clears_the_mark() {
        let call: Arc<dyn GuestCall> = Arc::new(Recorder::default());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            enter(&call, || native_handlers_only(|| panic!("guest")));
        }));
        assert!(result.is_err());
        assert!(current().is_none());
        assert!(!NATIVE_ONLY.get());
    }
}
