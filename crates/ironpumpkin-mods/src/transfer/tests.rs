//! Cross-thread, rollback and panic cases of the transfer API, and the vanilla container rules of
//! the inventory wrappers.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]
#![expect(
    clippy::semicolon_outside_block,
    reason = "a block scope drops its transaction without a commit"
)]

use std::{
    any::Any,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Barrier, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

use pumpkin_core::block::entities::{
    brewing_stand::BrewingStandBlockEntity, chiseled_bookshelf::ChiseledBookshelfBlockEntity,
    furnace::FurnaceBlockEntity, furnace_like_block_entity::CookingBlockEntityBase,
    hopper::HopperBlockEntity, jukebox::JukeboxBlockEntity,
};
use pumpkin_data::{block_properties::FacingHopper, item::Item, item_stack::ItemStack};
use pumpkin_inventory::{Clearable, Inventory};
use pumpkin_util::math::position::BlockPos;

use super::{
    ItemResource, ItemStacksResourceHandler, Journal, Lifecycle, ResourceHandler,
    RootCommitJournal, SnapshotJournal, Transaction, insert_item_return_remaining,
    inventory::{ContainerKind, InventoryWrapper, SidedInventoryWrapper},
};

/// An inventory with no rules, like a chest.
struct Inv(Mutex<Vec<ItemStack>>);

impl Inv {
    fn new(size: usize) -> Arc<Self> {
        Arc::new(Self(Mutex::new(vec![ItemStack::EMPTY.clone(); size])))
    }
}

impl Clearable for Inv {
    fn clear(&self) {
        for stack in self.0.lock().unwrap().iter_mut() {
            *stack = ItemStack::EMPTY.clone();
        }
    }
}

impl Inventory for Inv {
    fn size(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    fn is_empty(&self) -> bool {
        self.0.lock().unwrap().iter().all(ItemStack::is_empty)
    }

    fn get_stack(&self, slot: usize) -> ItemStack {
        self.0.lock().unwrap()[slot].clone()
    }

    fn remove_stack(&self, slot: usize) -> ItemStack {
        std::mem::replace(&mut self.0.lock().unwrap()[slot], ItemStack::EMPTY.clone())
    }

    fn remove_stack_specific(&self, slot: usize, amount: u8) -> ItemStack {
        self.0.lock().unwrap()[slot].split(amount)
    }

    fn set_stack(&self, slot: usize, stack: ItemStack) {
        self.0.lock().unwrap()[slot] = stack;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn count(stack: &ItemStack) -> u32 {
    if stack.is_empty() {
        0
    } else {
        u32::from(stack.item_count)
    }
}

fn stone() -> ItemResource {
    ItemResource::of_item(&Item::STONE)
}

/// Thread B holds a transaction on index 0 while thread A commits an insert into index 1; B's
/// abort must not touch index 1.
#[test]
fn an_abort_on_one_thread_keeps_a_commit_of_another_thread() {
    let handler = Arc::new(ItemStacksResourceHandler::new(2, 64));
    let mut transaction = Transaction::open_root().unwrap();
    assert_eq!(handler.insert_at(0, &stone(), 10, &mut transaction), 10);
    transaction.commit();
    let barrier = Arc::new(Barrier::new(2));
    let (shared, wait) = (handler.clone(), barrier.clone());
    let b = thread::spawn(move || {
        let mut transaction = Transaction::open_root().unwrap();
        assert_eq!(shared.extract_at(0, &stone(), 1, &mut transaction), 1);
        wait.wait();
        wait.wait();
        drop(transaction);
    });
    barrier.wait();
    let mut transaction = Transaction::open_root().unwrap();
    assert_eq!(handler.insert_at(1, &stone(), 5, &mut transaction), 5);
    transaction.commit();
    barrier.wait();
    b.join().unwrap();
    assert_eq!(
        handler.get_amount_as_long(0) + handler.get_amount_as_long(1),
        15
    );
}

/// A journal that a transaction of thread A touched belongs to A until A's root closes: thread B
/// transfers nothing meanwhile, and its abort changes nothing.
#[test]
fn a_journal_owned_by_another_thread_transfers_nothing() {
    let handler = Arc::new(ItemStacksResourceHandler::new(1, 64));
    let mut transaction = Transaction::open_root().unwrap();
    handler.insert_at(0, &stone(), 10, &mut transaction);
    transaction.commit();
    let barrier = Arc::new(Barrier::new(2));
    let (shared, wait) = (handler.clone(), barrier.clone());
    let a = thread::spawn(move || {
        let mut transaction = Transaction::open_root().unwrap();
        assert_eq!(shared.extract_at(0, &stone(), 1, &mut transaction), 1);
        wait.wait();
        wait.wait();
        transaction.commit();
    });
    barrier.wait();
    {
        let mut transaction = Transaction::open_root().unwrap();
        assert_eq!(handler.extract_at(0, &stone(), 4, &mut transaction), 0);
        assert_eq!(handler.insert_at(0, &stone(), 4, &mut transaction), 0);
    }
    barrier.wait();
    a.join().unwrap();
    assert_eq!(handler.get_amount_as_long(0), 9);
    // A's root closed, so B's next transaction transfers.
    let mut transaction = Transaction::open_root().unwrap();
    assert_eq!(handler.extract_at(0, &stone(), 4, &mut transaction), 4);
    transaction.commit();
    assert_eq!(handler.get_amount_as_long(0), 5);
}

/// The slot journal of a Pumpkin inventory belongs to the root of thread A: thread B extracts
/// nothing, and A's abort conserves the items.
#[test]
fn an_inventory_slot_owned_by_another_thread_transfers_nothing() {
    let inv = Inv::new(1);
    inv.set_stack(0, ItemStack::new(5, &Item::STONE));
    let wrapper = InventoryWrapper::of(inv.clone());
    let barrier = Arc::new(Barrier::new(2));
    let (shared, wait) = (wrapper.clone(), barrier.clone());
    let a = thread::spawn(move || {
        let mut transaction = Transaction::open_root().unwrap();
        assert_eq!(shared.insert_at(0, &stone(), 1, &mut transaction), 1);
        wait.wait();
        wait.wait();
        drop(transaction);
    });
    barrier.wait();
    let mut transaction = Transaction::open_root().unwrap();
    let extracted = wrapper.extract_at(0, &stone(), 3, &mut transaction);
    transaction.commit();
    barrier.wait();
    a.join().unwrap();
    assert_eq!(count(&inv.get_stack(0)) + extracted, 5);
}

/// An aborted extract after a write outside the transaction gives back what it took.
#[test]
fn an_aborted_extract_after_another_write_gives_back_its_items() {
    let inv = Inv::new(1);
    inv.set_stack(0, ItemStack::new(5, &Item::STONE));
    let wrapper = InventoryWrapper::of(inv.clone());
    {
        let mut transaction = Transaction::open_root().unwrap();
        assert_eq!(wrapper.extract_at(0, &stone(), 2, &mut transaction), 2);
        let mut stack = inv.get_stack(0);
        stack.item_count += 1;
        inv.set_stack(0, stack);
    }
    assert_eq!(count(&inv.get_stack(0)), 6);
}

/// An aborted insert after a take outside the transaction takes back only what it inserted.
#[test]
fn an_aborted_insert_after_another_take_takes_back_its_items() {
    let inv = Inv::new(1);
    inv.set_stack(0, ItemStack::new(5, &Item::STONE));
    let wrapper = InventoryWrapper::of(inv.clone());
    {
        let mut transaction = Transaction::open_root().unwrap();
        assert_eq!(wrapper.insert_at(0, &stone(), 2, &mut transaction), 2);
        let mut stack = inv.get_stack(0);
        stack.item_count -= 1;
        inv.set_stack(0, stack);
    }
    assert_eq!(count(&inv.get_stack(0)), 4);
}

/// Two writes of a transaction with a write of another writer between them: the abort undoes
/// only the writes of the transaction.
#[test]
fn an_abort_undoes_only_its_own_writes_around_another_write() {
    let inv = Inv::new(1);
    inv.set_stack(0, ItemStack::new(10, &Item::STONE));
    let wrapper = InventoryWrapper::of(inv.clone());
    {
        let mut transaction = Transaction::open_root().unwrap();
        assert_eq!(wrapper.extract_at(0, &stone(), 2, &mut transaction), 2);
        let mut stack = inv.get_stack(0);
        stack.item_count += 3;
        inv.set_stack(0, stack);
        assert_eq!(wrapper.extract_at(0, &stone(), 4, &mut transaction), 4);
    }
    assert_eq!(count(&inv.get_stack(0)), 13);
}

/// An abort after another writer replaced the item keeps that item and touches nothing.
#[test]
fn an_abort_after_the_item_changed_keeps_the_other_item() {
    let inv = Inv::new(1);
    inv.set_stack(0, ItemStack::new(5, &Item::STONE));
    let wrapper = InventoryWrapper::of(inv.clone());
    {
        let mut transaction = Transaction::open_root().unwrap();
        wrapper.extract_at(0, &stone(), 2, &mut transaction);
        inv.set_stack(0, ItemStack::new(1, &Item::EMERALD));
    }
    let stack = inv.get_stack(0);
    assert_eq!((stack.item.id, stack.item_count), (Item::EMERALD.id, 1));
}

#[test]
fn a_panic_in_a_root_commit_callback_leaves_the_thread_usable() {
    let panicking = RootCommitJournal::new(|| panic!("mod callback panics"));
    let result = catch_unwind(AssertUnwindSafe(|| {
        let transaction = Transaction::open_root().unwrap();
        assert!(panicking.update_snapshots(&transaction));
        transaction.commit();
    }));
    assert!(result.is_err());
    assert_eq!(Transaction::lifecycle(), Lifecycle::None);
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let healthy = RootCommitJournal::new(move || {
        counter.fetch_add(1, Ordering::Relaxed);
    });
    let transaction = Transaction::open_root().unwrap();
    assert!(healthy.update_snapshots(&transaction));
    transaction.commit();
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn a_panic_in_the_root_commit_queue_still_runs_the_other_callbacks() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let second = Arc::new(RootCommitJournal::new(move || {
        counter.fetch_add(1, Ordering::Relaxed);
    }));
    let first = RootCommitJournal::new(|| panic!("first callback panics"));
    let result = catch_unwind(AssertUnwindSafe(|| {
        let transaction = Transaction::open_root().unwrap();
        assert!(first.update_snapshots(&transaction));
        assert!(second.update_snapshots(&transaction));
        transaction.commit();
    }));
    assert!(result.is_err());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let again = second.clone();
    thread::spawn(move || {
        let transaction = Transaction::open_root().unwrap();
        assert!(again.update_snapshots(&transaction));
        transaction.commit();
    })
    .join()
    .unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

struct PanicRevert;

impl Journal for PanicRevert {
    type Snapshot = ();

    fn create_snapshot(&self) {}

    fn revert_to_snapshot(&self, (): ()) {
        panic!("revert panics");
    }
}

struct Cell(Mutex<i32>);

impl Journal for Cell {
    type Snapshot = i32;

    fn create_snapshot(&self) -> i32 {
        *self.0.lock().unwrap()
    }

    fn revert_to_snapshot(&self, snapshot: i32) {
        *self.0.lock().unwrap() = snapshot;
    }
}

#[test]
fn a_panic_in_one_revert_still_reverts_the_other_journals() {
    let bad = SnapshotJournal::new(PanicRevert);
    let good = SnapshotJournal::new(Cell(Mutex::new(1)));
    let result = catch_unwind(AssertUnwindSafe(|| {
        let transaction = Transaction::open_root().unwrap();
        assert!(bad.update_snapshots(&transaction));
        assert!(good.update_snapshots(&transaction));
        *good.0.lock().unwrap() = 2;
    }));
    assert!(result.is_err());
    assert_eq!(*good.0.lock().unwrap(), 1);
    {
        let transaction = Transaction::open_root().unwrap();
        assert!(good.update_snapshots(&transaction));
        *good.0.lock().unwrap() = 3;
    }
    assert_eq!(*good.0.lock().unwrap(), 1);
    assert!(!good.is_in_transaction());
}

#[test]
fn a_panic_in_a_handler_rolls_back_and_releases_the_journal() {
    let handler = Arc::new(ItemStacksResourceHandler::new(1, 64));
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut transaction = Transaction::open_root().unwrap();
        handler.insert_at(0, &stone(), 5, &mut transaction);
        panic!("handler panics");
    }));
    assert!(result.is_err());
    assert_eq!(handler.get_amount_as_long(0), 0);
    assert_eq!(Transaction::lifecycle(), Lifecycle::None);
    let other = handler.clone();
    let inserted = thread::spawn(move || {
        let mut transaction = Transaction::open_root().unwrap();
        let inserted = other.insert_at(0, &stone(), 5, &mut transaction);
        transaction.commit();
        inserted
    })
    .join()
    .unwrap();
    assert_eq!(inserted, 5);
}

#[test]
fn an_outer_abort_reverts_journals_first_touched_in_a_nested_transaction() {
    let a = SnapshotJournal::new(Cell(Mutex::new(1)));
    let b = SnapshotJournal::new(Cell(Mutex::new(10)));
    {
        let mut root = Transaction::open_root().unwrap();
        assert!(a.update_snapshots(&root));
        *a.0.lock().unwrap() = 2;
        let mut first = Transaction::open(Some(&mut root)).unwrap();
        let second = Transaction::open(Some(&mut first)).unwrap();
        assert!(b.update_snapshots(&second));
        *b.0.lock().unwrap() = 11;
        assert!(a.update_snapshots(&second));
        *a.0.lock().unwrap() = 3;
        second.commit();
        first.commit();
        assert!(b.is_in_transaction());
    }
    assert_eq!(*a.0.lock().unwrap(), 1);
    assert_eq!(*b.0.lock().unwrap(), 10);
    assert!(!a.is_in_transaction() && !b.is_in_transaction());
}

#[test]
fn a_partial_insert_respects_the_max_stack_size_and_simulate_changes_nothing() {
    let inv = Inv::new(3);
    let wrapper = InventoryWrapper::of(inv.clone());
    let pearls = ItemStack::new(40, &Item::ENDER_PEARL);
    let left = insert_item_return_remaining(wrapper.as_ref(), &pearls, false, None).unwrap();
    let counts: Vec<_> = (0..3).map(|slot| count(&inv.get_stack(slot))).collect();
    assert_eq!(counts, vec![16, 16, 8]);
    assert!(left.is_empty());
    let more = ItemStack::new(30, &Item::ENDER_PEARL);
    let left = insert_item_return_remaining(wrapper.as_ref(), &more, true, None).unwrap();
    assert_eq!(count(&left), 22);
    assert_eq!(count(&inv.get_stack(2)), 8);
}

#[test]
fn a_full_extract_leaves_an_empty_slot() {
    let inv = Inv::new(1);
    inv.set_stack(0, ItemStack::new(3, &Item::STONE));
    let wrapper = InventoryWrapper::of(inv.clone());
    let mut transaction = Transaction::open_root().unwrap();
    assert_eq!(wrapper.extract_at(0, &stone(), 3, &mut transaction), 3);
    transaction.commit();
    assert_eq!(inv.get_stack(0).item.id, Item::AIR.id);
}

#[test]
fn an_invalid_resource_has_no_capacity() {
    let handler = ItemStacksResourceHandler::new(1, 64)
        .with_validator(|_, resource| resource.is(&Item::STONE));
    assert_eq!(handler.get_capacity_as_long(0, &stone()), 64);
    assert_eq!(
        handler.get_capacity_as_long(0, &ItemResource::of_item(&Item::DIRT)),
        0
    );
}

#[test]
fn the_wrapper_cache_keeps_one_wrapper_per_inventory_and_kind() {
    let inv = Inv::new(1);
    let plain = InventoryWrapper::of(inv.clone());
    assert!(Arc::ptr_eq(&plain, &InventoryWrapper::of(inv.clone())));
    let furnace = InventoryWrapper::of_kind(inv, ContainerKind::Furnace);
    assert!(!Arc::ptr_eq(&plain, &furnace));
}

#[test]
fn a_transaction_on_the_furnace_input_resets_the_cooking_only_on_commit() {
    let furnace = Arc::new(FurnaceBlockEntity::new(BlockPos::new(0, 64, 0)));
    furnace.set_stack(0, ItemStack::new(4, &Item::RAW_IRON));
    furnace.set_cooking_time_spent(120);
    let wrapper = InventoryWrapper::of_kind(furnace.clone(), ContainerKind::Furnace);
    let iron = ItemResource::of_item(&Item::RAW_IRON);
    {
        let mut transaction = Transaction::open_root().unwrap();
        assert_eq!(wrapper.extract_at(0, &iron, 4, &mut transaction), 4);
    }
    assert_eq!(furnace.get_cooking_time_spent(), 120);
    // Taking some of the input keeps the progress, as vanilla does.
    let mut transaction = Transaction::open_root().unwrap();
    assert_eq!(wrapper.extract_at(0, &iron, 1, &mut transaction), 1);
    transaction.commit();
    assert_eq!(furnace.get_cooking_time_spent(), 120);
    // Emptying the input resets it at the commit.
    let mut transaction = Transaction::open_root().unwrap();
    assert_eq!(wrapper.extract_at(0, &iron, 3, &mut transaction), 3);
    assert_eq!(furnace.get_cooking_time_spent(), 120);
    transaction.commit();
    assert_eq!(furnace.get_cooking_time_spent(), 0);
}

#[test]
fn an_insert_into_an_empty_hopper_sets_its_cooldown_unless_aborted() {
    let hopper = Arc::new(HopperBlockEntity::new(
        BlockPos::new(0, 64, 0),
        FacingHopper::Down,
    ));
    let wrapper = InventoryWrapper::of_kind(hopper.clone(), ContainerKind::Hopper);
    {
        let mut transaction = Transaction::open_root().unwrap();
        assert_eq!(wrapper.insert_at(0, &stone(), 1, &mut transaction), 1);
        assert_eq!(hopper.cooldown(), 8);
    }
    assert_eq!(hopper.cooldown(), -1);
    let mut transaction = Transaction::open_root().unwrap();
    assert_eq!(wrapper.insert_at(0, &stone(), 1, &mut transaction), 1);
    transaction.commit();
    assert_eq!(hopper.cooldown(), 8);
}

#[test]
fn the_brewing_stand_takes_one_potion_input_per_bottle_slot_and_reagents_on_top() {
    let stand = Arc::new(BrewingStandBlockEntity::new(BlockPos::new(0, 64, 0)));
    let side = SidedInventoryWrapper::new(stand, ContainerKind::BrewingStand, None);
    let mut transaction = Transaction::open_root().unwrap();
    let bottle = ItemResource::of_item(&Item::GLASS_BOTTLE);
    assert_eq!(side.insert_at(0, &bottle, 5, &mut transaction), 1);
    assert_eq!(side.insert_at(0, &bottle, 1, &mut transaction), 0);
    // Stone is a reagent (the infested potion); dirt is not.
    let dirt = ItemResource::of_item(&Item::DIRT);
    assert_eq!(side.insert_at(3, &dirt, 1, &mut transaction), 0);
    let wart = ItemResource::of_item(&Item::NETHER_WART);
    assert_eq!(side.insert_at(3, &wart, 2, &mut transaction), 2);
    assert_eq!(side.insert_at(1, &dirt, 1, &mut transaction), 0);
    transaction.commit();
}

#[test]
fn the_jukebox_and_the_chiseled_bookshelf_hold_one_item_per_slot() {
    let jukebox = Arc::new(JukeboxBlockEntity::new(BlockPos::new(0, 64, 0)));
    let records = InventoryWrapper::of_kind(jukebox, ContainerKind::Jukebox);
    let shelf = Arc::new(ChiseledBookshelfBlockEntity::new(BlockPos::new(1, 64, 0)));
    let books = InventoryWrapper::of_kind(shelf, ContainerKind::ChiseledBookshelf);
    let mut transaction = Transaction::open_root().unwrap();
    assert_eq!(records.insert_at(0, &stone(), 1, &mut transaction), 0);
    let disc = ItemResource::of_item(&Item::MUSIC_DISC_CAT);
    assert_eq!(records.insert_at(0, &disc, 1, &mut transaction), 1);
    assert_eq!(records.insert_at(0, &disc, 1, &mut transaction), 0);
    assert_eq!(books.insert_at(0, &stone(), 1, &mut transaction), 0);
    let book = ItemResource::of_item(&Item::BOOK);
    assert_eq!(books.insert_at(0, &book, 16, &mut transaction), 1);
    assert_eq!(books.get_capacity_as_long(1, &book), 1);
    transaction.commit();
}
