//! The root transaction of a menu action: a click, a button, a close or a [`Menu::with`] call.
//!
//! The Pumpkin click code writes a handler slot through `Inventory::set_stack`, with no
//! transaction argument, and it moves the other side (the cursor, the player inventory) before
//! or after that write. So the action keeps one root transaction on its thread, every handler
//! write is a nested transaction under it, and a refused write marks the whole action refused:
//! the owner then rolls the root back and restores the other slots.
//!
//! [`Menu::with`]: super::Menu::with

use std::cell::RefCell;

use tracing::warn;

use crate::transfer::{Transaction, TransactionContext};

struct Scope {
    /// `None` when the thread already had a root transaction outside the menu: then no handler
    /// write can happen in this action.
    root: Option<Transaction<'static>>,
    refused: bool,
}

thread_local! {
    static SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) };
}

/// The scope of one action. Only the outermost guard of a thread owns the root.
pub(super) struct Guard {
    owner: bool,
}

/// Opens the scope of an action, or joins the scope that is already open on this thread.
pub(super) fn enter() -> Guard {
    SCOPE.with_borrow_mut(|scope| {
        if scope.is_some() {
            return Guard { owner: false };
        }
        let root = Transaction::open_root()
            .inspect_err(|_| {
                warn!(
                    "a menu action runs inside a root transaction of this thread: its handler \
                     slots refuse every write"
                );
            })
            .ok();
        *scope = Some(Scope {
            root,
            refused: false,
        });
        Guard { owner: true }
    })
}

impl Guard {
    pub(super) const fn owns(&self) -> bool {
        self.owner
    }

    /// Ends the action: commits the root, or rolls it back when a write was refused. Returns
    /// whether the action was refused. A guard that does not own the root returns `false`.
    pub(super) fn finish(mut self) -> bool {
        if !self.owner {
            return false;
        }
        self.owner = false;
        let Some(scope) = SCOPE.with_borrow_mut(Option::take) else {
            return false;
        };
        match scope.root {
            Some(root) if !scope.refused => {
                root.commit();
                false
            }
            _ => scope.refused,
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        // A callback panicked: the root rolls back as it drops.
        if self.owner {
            drop(SCOPE.with_borrow_mut(Option::take));
        }
    }
}

/// Whether a write of the current action was refused.
pub(super) fn refused() -> bool {
    SCOPE.with_borrow(|scope| scope.as_ref().is_some_and(|scope| scope.refused))
}

/// What [`write`] did.
pub(super) enum Write<R> {
    /// No action is open on this thread.
    NoScope,
    /// The write ran in a nested transaction of the action root.
    Done(R),
    /// The write was refused, now or earlier in the action.
    Refused,
}

/// Runs `write` in a nested transaction of the action root. The nested transaction commits when
/// `write` returns `Some`; on `None` it rolls back and the action is refused.
pub(super) fn write<R>(write: impl FnOnce(&mut TransactionContext) -> Option<R>) -> Write<R> {
    SCOPE.with(|cell| {
        // A handler that writes a menu slot from inside its own `insert_at` re-enters here.
        let Ok(mut scope) = cell.try_borrow_mut() else {
            return Write::Refused;
        };
        let Some(scope) = scope.as_mut() else {
            return Write::NoScope;
        };
        if scope.refused {
            return Write::Refused;
        }
        let result = scope.root.as_mut().and_then(|root| {
            let mut nested = Transaction::open(Some(root)).ok()?;
            let result = write(&mut nested)?;
            nested.commit();
            Some(result)
        });
        result.map_or_else(
            || {
                scope.refused = true;
                Write::Refused
            },
            Write::Done,
        )
    })
}

/// Runs `simulate` in a nested transaction of the action root that always rolls back.
pub(super) fn simulate<R>(simulate: impl FnOnce(&mut TransactionContext) -> R) -> Write<R> {
    SCOPE.with(|cell| {
        let Ok(mut scope) = cell.try_borrow_mut() else {
            return Write::Refused;
        };
        let Some(scope) = scope.as_mut() else {
            return Write::NoScope;
        };
        let Some(root) = scope.root.as_mut() else {
            return Write::Refused;
        };
        Transaction::open(Some(root)).map_or(Write::Refused, |mut nested| {
            Write::Done(simulate(&mut nested))
        })
    })
}

/// Runs `action` with the root transaction of the current action, for a callback that moves
/// resources itself. Returns `None` outside an action or inside another handler call.
pub(super) fn with_root<R>(action: impl FnOnce(&mut TransactionContext) -> R) -> Option<R> {
    SCOPE.with(|cell| {
        let Ok(mut scope) = cell.try_borrow_mut() else {
            return None;
        };
        let root = scope.as_mut()?.root.as_mut()?;
        Some(action(root))
    })
}
