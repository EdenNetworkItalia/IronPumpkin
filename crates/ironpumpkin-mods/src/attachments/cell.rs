//! The lock of one attachment, and the guard of mod callbacks against panics.

use std::{
    cell::RefCell,
    ops::{Deref, DerefMut},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{MutexGuard, PoisonError},
};

use pumpkin_core::plugin::attachments::{AttachmentCell, AttachmentValue};

thread_local! {
    /// The cells that this thread holds, so a re-entry panics instead of a deadlock.
    static HELD: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// The value of one attachment, locked.
pub(super) struct CellGuard<'a> {
    guard: MutexGuard<'a, AttachmentValue>,
    key: usize,
}

fn key(cell: &AttachmentCell) -> usize {
    std::sync::Arc::as_ptr(cell).cast::<()>() as usize
}

/// Locks `cell`, the value of the attachment type `name`.
///
/// # Panics
///
/// When this thread holds the cell already: `with_data` that reads or writes its own type,
/// for example. A mutex would wait for ever.
pub(super) fn lock<'a>(cell: &'a AttachmentCell, name: &str) -> CellGuard<'a> {
    let key = key(cell);
    // A re-entry is a bug of the mod, and a hang hides it.
    assert!(
        !HELD.with(|held| held.borrow().contains(&key)),
        "attachment type {name} is used inside `with_data` of the same type on the same \
         holder; a value can be read or written only after `with_data` returns"
    );
    let guard = cell.lock().unwrap_or_else(PoisonError::into_inner);
    HELD.with(|held| held.borrow_mut().push(key));
    CellGuard { guard, key }
}

impl Drop for CellGuard<'_> {
    fn drop(&mut self) {
        let key = self.key;
        HELD.with(|held| {
            let mut held = held.borrow_mut();
            if let Some(position) = held.iter().rposition(|k| *k == key) {
                held.swap_remove(position);
            }
        });
    }
}

impl Deref for CellGuard<'_> {
    type Target = AttachmentValue;

    fn deref(&self) -> &AttachmentValue {
        &self.guard
    }
}

impl DerefMut for CellGuard<'_> {
    fn deref_mut(&mut self) -> &mut AttachmentValue {
        &mut self.guard
    }
}

/// Runs a mod callback, with a panic turned into the error that `NeoForge` logs when the
/// callback throws.
pub(super) fn guarded<R>(f: impl FnOnce() -> R) -> Result<R, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|payload| {
        payload
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "the callback panicked".to_owned())
    })
}
