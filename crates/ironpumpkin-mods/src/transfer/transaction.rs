//! Transactions of the `NeoForge` transfer API (`net.neoforged.neoforge.transfer.transaction`).
//!
//! A transaction groups changes to resource handlers. The changes take effect when the root
//! transaction commits and roll back when a transaction closes without a commit. Each thread has
//! its own stack of transactions, as `TransactionManager` keeps one per thread: one root at a
//! time, and a nested transaction opens on the innermost open one.
//!
//! The borrow checker enforces what `NeoForge` checks at run time. [`Transaction`] is `!Send`, so
//! it stays on the thread that opened it and cannot live across an `.await` in a `Send` future,
//! such as a Pumpkin event handler. [`Transaction::open`] takes the parent mutably, so the parent
//! is unusable while a nested transaction is open, and a handler gets the
//! [`TransactionContext`] only for the length of its call, so it never sees a closed
//! transaction.

use std::{
    cell::RefCell,
    collections::VecDeque,
    fmt,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex, PoisonError, Weak},
};

use tracing::error;

/// The open transaction that a handler gets in `insert` and `extract`: `TransactionContext`.
/// Only [`Transaction`] creates one.
pub struct TransactionContext {
    depth: usize,
    // `TransactionManager` is per thread.
    _not_send: PhantomData<*const ()>,
}

impl TransactionContext {
    /// The depth of the transaction: 0 for the root.
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.depth
    }
}

impl fmt::Debug for TransactionContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransactionContext")
            .field("depth", &self.depth)
            .finish()
    }
}

/// `Transaction`: an open transaction. [`commit`](Self::commit) keeps its changes; a drop
/// without a commit rolls them back, as `close` does.
#[must_use = "a transaction rolls back when it is dropped without a commit"]
pub struct Transaction<'parent> {
    context: TransactionContext,
    _parent: PhantomData<&'parent mut TransactionContext>,
}

/// Why a transaction cannot open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionError {
    /// [`Transaction::open_root`] while a root transaction is open on this thread.
    RootAlreadyOpen,
}

impl fmt::Display for TransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RootAlreadyOpen => {
                f.write_str("a root transaction is already open on this thread")
            }
        }
    }
}

impl std::error::Error for TransactionError {}

/// `Transaction.Lifecycle`: the state of the transactions of this thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    /// No transaction is open.
    None,
    /// A transaction is open, and nested ones can open.
    Open,
    /// A transaction is closing: no transaction can open.
    Closing,
    /// The root transaction closed and the root commit callbacks run.
    RootClosing,
}

impl Transaction<'static> {
    /// `Transaction.openRoot`.
    ///
    /// # Errors
    ///
    /// [`TransactionError::RootAlreadyOpen`] when this thread has an open transaction.
    pub fn open_root() -> Result<Self, TransactionError> {
        open_root_depth().map(Self::new)
    }
}

impl<'parent> Transaction<'parent> {
    const fn new(depth: usize) -> Self {
        Self {
            context: TransactionContext {
                depth,
                _not_send: PhantomData,
            },
            _parent: PhantomData,
        }
    }

    /// `Transaction.open(parent)`: a nested transaction of `parent`. Without a parent it is
    /// [`open_root`](Transaction::open_root).
    ///
    /// # Errors
    ///
    /// [`TransactionError::RootAlreadyOpen`] without a parent while this thread has an open
    /// transaction.
    ///
    /// # Panics
    ///
    /// When `parent` is not the innermost open transaction, which only a leaked transaction
    /// (`mem::forget`) causes.
    pub fn open(parent: Option<&'parent mut TransactionContext>) -> Result<Self, TransactionError> {
        let Some(parent) = parent else {
            return open_root_depth().map(Self::new);
        };
        let depth = with_manager(|manager| {
            assert_eq!(
                manager.current,
                Some(parent.depth),
                "a nested transaction opens on the innermost open transaction"
            );
            manager.push()
        });
        Ok(Self::new(depth))
    }

    /// Commits the transaction: its changes join the parent, or take effect for a root.
    pub fn commit(self) {
        let depth = self.context.depth;
        std::mem::forget(self);
        close(depth, false);
    }

    /// `Transaction.getLifecycle` of this thread.
    #[must_use]
    pub fn lifecycle() -> Lifecycle {
        with_manager(|manager| match manager.current {
            None if manager.processing_root_commit => Lifecycle::RootClosing,
            None => Lifecycle::None,
            Some(depth) if manager.frames[depth].open => Lifecycle::Open,
            Some(_) => Lifecycle::Closing,
        })
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        close(self.context.depth, true);
    }
}

impl Deref for Transaction<'_> {
    type Target = TransactionContext;

    fn deref(&self) -> &TransactionContext {
        &self.context
    }
}

impl DerefMut for Transaction<'_> {
    fn deref_mut(&mut self) -> &mut TransactionContext {
        &mut self.context
    }
}

impl fmt::Debug for Transaction<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transaction")
            .field("depth", &self.context.depth)
            .finish()
    }
}

/// What a closing transaction calls on each journal that joined it.
trait JournalClose: Send + Sync {
    fn on_close(&self, depth: usize, aborted: bool);
    fn call_on_root_commit(&self);
}

#[derive(Default)]
struct Frame {
    open: bool,
    journals: Vec<Arc<dyn JournalClose>>,
}

#[derive(Default)]
struct Manager {
    /// The depth of the innermost open transaction.
    current: Option<usize>,
    frames: Vec<Frame>,
    root_commit_queue: VecDeque<Arc<dyn JournalClose>>,
    processing_root_commit: bool,
}

impl Manager {
    fn push(&mut self) -> usize {
        let depth = self.current.map_or(0, |depth| depth + 1);
        if self.frames.len() == depth {
            self.frames.push(Frame::default());
        }
        self.frames[depth].open = true;
        self.current = Some(depth);
        depth
    }
}

thread_local! {
    static MANAGER: RefCell<Manager> = RefCell::default();
}

fn open_root_depth() -> Result<usize, TransactionError> {
    with_manager(|manager| {
        if manager.current.is_some() {
            return Err(TransactionError::RootAlreadyOpen);
        }
        Ok(manager.push())
    })
}

/// Runs `f` on the manager of this thread. The journal callbacks run outside, so a callback
/// that opens a transaction does not borrow the manager twice.
fn with_manager<R>(f: impl FnOnce(&mut Manager) -> R) -> R {
    MANAGER.with(|manager| f(&mut manager.borrow_mut()))
}

/// `Transaction.close(wasAborted)`: closes the journals of the transaction at `depth`, and after
/// the root runs the root commit callbacks.
fn close(depth: usize, aborted: bool) {
    let journals = with_manager(|manager| {
        if manager.current != Some(depth) || !manager.frames[depth].open {
            return None;
        }
        manager.frames[depth].open = false;
        Some(std::mem::take(&mut manager.frames[depth].journals))
    });
    let Some(journals) = journals else {
        error!("transaction of depth {depth} closed out of order; it was leaked or closed twice");
        return;
    };
    // The depth goes down even when a journal callback panics, as `close` does on an exception.
    let _restore = CloseGuard(depth);
    for journal in journals {
        journal.on_close(depth, aborted);
    }
}

struct CloseGuard(usize);

impl Drop for CloseGuard {
    fn drop(&mut self) {
        let depth = self.0;
        let panicking = std::thread::panicking();
        let root = with_manager(|manager| {
            manager.current = depth.checked_sub(1);
            if depth == 0 && panicking {
                manager.root_commit_queue.clear();
            }
            depth == 0
                && !panicking
                && !std::mem::replace(&mut manager.processing_root_commit, true)
        });
        if !root {
            return;
        }
        while let Some(journal) = with_manager(|manager| manager.root_commit_queue.pop_front()) {
            journal.call_on_root_commit();
        }
        with_manager(|manager| manager.processing_root_commit = false);
    }
}

/// The state of a [`SnapshotJournal`] that `SnapshotJournal` subclasses implement:
/// `createSnapshot`, `revertToSnapshot`, `releaseSnapshot` and `onRootCommit`.
pub trait Journal: Send + Sync + 'static {
    /// The saved state.
    type Snapshot: Send + 'static;

    /// Saves the current state.
    fn create_snapshot(&self) -> Self::Snapshot;

    /// Rolls back to `snapshot`.
    fn revert_to_snapshot(&self, snapshot: Self::Snapshot);

    /// Drops a snapshot that no transaction needs.
    fn release_snapshot(&self, _snapshot: Self::Snapshot) {}

    /// Runs after the root transaction committed, with the state before the transaction, such
    /// as to mark a block entity dirty. It may open a new transaction.
    fn on_root_commit(&self, _original_state: Self::Snapshot) {}
}

struct JournalState<S> {
    /// One snapshot per depth; `None` where this journal has none.
    snapshots: Vec<Option<S>>,
    snapshot_count: usize,
    original_state: Option<S>,
}

/// `SnapshotJournal`: makes the state of `J` transactional. A handler calls
/// [`update_snapshots`](Self::update_snapshots) before each change, and the journal rolls the
/// change back when the transaction aborts.
///
/// A journal joins the transactions of one thread at a time, as in `NeoForge`.
pub struct SnapshotJournal<J: Journal> {
    journal: J,
    state: Mutex<JournalState<J::Snapshot>>,
    this: Weak<Self>,
}

impl<J: Journal> SnapshotJournal<J> {
    /// A journal over `journal`.
    pub fn new(journal: J) -> Arc<Self> {
        Arc::new_cyclic(|this| Self {
            journal,
            state: Mutex::new(JournalState {
                snapshots: Vec::new(),
                snapshot_count: 0,
                original_state: None,
            }),
            this: this.clone(),
        })
    }

    /// `updateSnapshots`: saves the state for `transaction` unless it already has a snapshot.
    /// Call it before each change.
    pub fn update_snapshots(&self, transaction: &TransactionContext) {
        let depth = transaction.depth;
        {
            let mut state = self.lock();
            if state.snapshots.len() <= depth {
                state.snapshots.resize_with(depth + 1, || None);
            }
            if state.snapshots[depth].is_some() {
                return;
            }
        }
        let snapshot = self.journal.create_snapshot();
        let mut state = self.lock();
        state.snapshot_count += 1;
        state.snapshots[depth] = Some(snapshot);
        drop(state);
        self.join(depth);
    }

    /// `isInTransaction`: whether a transaction holds a snapshot of this journal.
    pub fn is_in_transaction(&self) -> bool {
        self.lock().snapshot_count > 0
    }

    fn join(&self, depth: usize) {
        let Some(this) = self.this.upgrade() else {
            return;
        };
        with_manager(|manager| manager.frames[depth].journals.push(this));
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, JournalState<J::Snapshot>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<J: Journal> Deref for SnapshotJournal<J> {
    type Target = J;

    fn deref(&self) -> &J {
        &self.journal
    }
}

impl<J: Journal> JournalClose for SnapshotJournal<J> {
    fn on_close(&self, depth: usize, aborted: bool) {
        enum Step<S> {
            Revert(S),
            Release(S),
            RootCommit,
            MoveUp,
        }
        let step = {
            let mut state = self.lock();
            let Some(snapshot) = state.snapshots.get_mut(depth).and_then(Option::take) else {
                return;
            };
            state.snapshots.truncate(depth);
            if aborted {
                state.snapshot_count -= 1;
                Step::Revert(snapshot)
            } else if depth == 0 {
                state.snapshot_count = 0;
                if state.original_state.is_none() {
                    state.original_state = Some(snapshot);
                    Step::RootCommit
                } else {
                    // A root commit callback changed the journal again: the scheduled callback
                    // covers this change.
                    Step::Release(snapshot)
                }
            } else if state.snapshots[depth - 1].is_none() {
                state.snapshots[depth - 1] = Some(snapshot);
                Step::MoveUp
            } else {
                state.snapshot_count -= 1;
                Step::Release(snapshot)
            }
        };
        match step {
            Step::Revert(snapshot) => {
                self.journal.revert_to_snapshot(snapshot);
            }
            Step::Release(snapshot) => self.journal.release_snapshot(snapshot),
            Step::RootCommit => {
                if let Some(this) = self.this.upgrade() {
                    with_manager(|manager| manager.root_commit_queue.push_back(this));
                }
            }
            Step::MoveUp => self.join(depth - 1),
        }
    }

    fn call_on_root_commit(&self) {
        let original = self.lock().original_state.take();
        if let Some(original) = original {
            self.journal.on_root_commit(original);
        }
    }
}

/// `RootCommitJournal`: runs a callback once after each root commit of a transaction that it
/// joined, with no state to roll back.
pub struct RootCommitJournal {
    journal: Arc<SnapshotJournal<RootCommit>>,
}

/// The callback of a [`RootCommitJournal`].
pub struct RootCommit(Box<dyn Fn() + Send + Sync>);

impl Journal for RootCommit {
    type Snapshot = ();

    fn create_snapshot(&self) {}

    fn revert_to_snapshot(&self, (): ()) {}

    fn on_root_commit(&self, (): ()) {
        (self.0)();
    }
}

impl RootCommitJournal {
    /// A journal that runs `root_commit_callback` after each root commit.
    pub fn new(root_commit_callback: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            journal: SnapshotJournal::new(RootCommit(Box::new(root_commit_callback))),
        }
    }

    /// Joins `transaction`, so the callback runs when its root commits.
    pub fn update_snapshots(&self, transaction: &TransactionContext) {
        self.journal.update_snapshots(transaction);
    }
}

impl fmt::Debug for RootCommitJournal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RootCommitJournal")
    }
}

#[cfg(test)]
#[expect(
    clippy::semicolon_outside_block,
    reason = "a block scope drops its transaction without a commit"
)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct Cell(Mutex<i32>);

    impl Journal for Cell {
        type Snapshot = i32;

        fn create_snapshot(&self) -> i32 {
            *self.0.lock().unwrap_or_else(PoisonError::into_inner)
        }

        fn revert_to_snapshot(&self, snapshot: i32) {
            *self.0.lock().unwrap_or_else(PoisonError::into_inner) = snapshot;
        }
    }

    fn set(journal: &SnapshotJournal<Cell>, value: i32, transaction: &TransactionContext) {
        journal.update_snapshots(transaction);
        *journal.0.lock().unwrap_or_else(PoisonError::into_inner) = value;
    }

    fn get(journal: &SnapshotJournal<Cell>) -> i32 {
        *journal.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[test]
    fn a_dropped_root_rolls_back() -> Result<(), TransactionError> {
        let journal = SnapshotJournal::new(Cell(Mutex::new(1)));
        {
            let transaction = Transaction::open_root()?;
            set(&journal, 2, &transaction);
            assert_eq!(get(&journal), 2);
            assert!(journal.is_in_transaction());
        }
        assert_eq!(get(&journal), 1);
        assert!(!journal.is_in_transaction());
        assert_eq!(Transaction::lifecycle(), Lifecycle::None);
        Ok(())
    }

    #[test]
    fn a_committed_nested_transaction_rolls_back_with_its_root() -> Result<(), TransactionError> {
        let journal = SnapshotJournal::new(Cell(Mutex::new(1)));
        {
            let mut root = Transaction::open_root()?;
            set(&journal, 2, &root);
            let nested = Transaction::open(Some(&mut root))?;
            assert_eq!(nested.depth(), 1);
            set(&journal, 3, &nested);
            nested.commit();
            assert_eq!(get(&journal), 3);
        }
        assert_eq!(get(&journal), 1);
        Ok(())
    }

    #[test]
    fn an_aborted_nested_transaction_keeps_the_root_changes() -> Result<(), TransactionError> {
        let journal = SnapshotJournal::new(Cell(Mutex::new(1)));
        let mut root = Transaction::open_root()?;
        set(&journal, 2, &root);
        {
            let nested = Transaction::open(Some(&mut root))?;
            set(&journal, 3, &nested);
        }
        assert_eq!(get(&journal), 2);
        root.commit();
        assert_eq!(get(&journal), 2);
        Ok(())
    }

    #[test]
    fn a_snapshot_taken_only_in_a_nested_transaction_moves_up() -> Result<(), TransactionError> {
        let journal = SnapshotJournal::new(Cell(Mutex::new(1)));
        {
            let mut root = Transaction::open_root()?;
            let nested = Transaction::open(Some(&mut root))?;
            set(&journal, 5, &nested);
            nested.commit();
        }
        assert_eq!(get(&journal), 1);
        Ok(())
    }

    #[test]
    fn a_second_root_on_the_same_thread_fails() -> Result<(), TransactionError> {
        let _root = Transaction::open_root()?;
        assert_eq!(
            Transaction::open_root().err(),
            Some(TransactionError::RootAlreadyOpen)
        );
        assert_eq!(
            Transaction::open(None).err(),
            Some(TransactionError::RootAlreadyOpen)
        );
        assert_eq!(Transaction::lifecycle(), Lifecycle::Open);
        Ok(())
    }

    #[test]
    fn the_root_commit_callback_runs_once_per_root_commit() -> Result<(), TransactionError> {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let journal = RootCommitJournal::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        let mut root = Transaction::open_root()?;
        journal.update_snapshots(&root);
        let nested = Transaction::open(Some(&mut root))?;
        journal.update_snapshots(&nested);
        nested.commit();
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        root.commit();
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        {
            let aborted = Transaction::open_root()?;
            journal.update_snapshots(&aborted);
        }
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        Ok(())
    }

    #[test]
    fn a_root_commit_callback_can_open_a_transaction() -> Result<(), TransactionError> {
        let opened = Arc::new(AtomicUsize::new(0));
        let counter = opened.clone();
        let journal = RootCommitJournal::new(move || {
            assert_eq!(Transaction::lifecycle(), Lifecycle::RootClosing);
            if Transaction::open_root().is_ok() {
                counter.fetch_add(1, Ordering::Relaxed);
            }
        });
        let root = Transaction::open_root()?;
        journal.update_snapshots(&root);
        root.commit();
        assert_eq!(opened.load(Ordering::Relaxed), 1);
        assert_eq!(Transaction::lifecycle(), Lifecycle::None);
        Ok(())
    }
}
