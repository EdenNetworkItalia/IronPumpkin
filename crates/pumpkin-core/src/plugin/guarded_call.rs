//! A scope for a call of mod code whose panic the server survives, as `NeoForge` logs the
//! exception of such a call and goes on.
//!
//! The panic hook of the server binary stops the server on a panic. A panic inside
//! [`guarded_call`] only unwinds to the call: the hook reads [`is_guarded`], logs the panic and
//! leaves the server running.

use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    thread::Result,
};

thread_local! {
    /// How many guarded calls the current thread is inside.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Whether the current thread runs inside a [`guarded_call`].
#[must_use]
pub fn is_guarded() -> bool {
    DEPTH.with(|depth| depth.get() > 0)
}

/// Runs `f` and returns its panic as an `Err`, with [`is_guarded`] true while `f` runs.
///
/// # Errors
///
/// The payload of the panic of `f`.
pub fn guarded_call<R>(f: impl FnOnce() -> R) -> Result<R> {
    struct Scope;
    impl Drop for Scope {
        fn drop(&mut self) {
            DEPTH.with(|depth| depth.set(depth.get() - 1));
        }
    }
    DEPTH.with(|depth| depth.set(depth.get() + 1));
    let _scope = Scope;
    catch_unwind(AssertUnwindSafe(f))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_is_set_inside_the_call_and_after_a_panic_it_is_clear_again() {
        assert!(!is_guarded());
        assert!(guarded_call(is_guarded).unwrap());
        // Nested calls keep the flag until the outer call ends.
        assert!(
            guarded_call(|| {
                guarded_call(|| ()).unwrap();
                is_guarded()
            })
            .unwrap()
        );
        let panicked = guarded_call(|| {
            assert!(is_guarded());
            panic!("a mod callback");
        });
        assert!(panicked.is_err());
        assert!(!is_guarded());
    }

    #[test]
    fn the_flag_belongs_to_one_thread() {
        guarded_call(|| {
            assert!(!std::thread::spawn(is_guarded).join().unwrap());
        })
        .unwrap();
    }
}
