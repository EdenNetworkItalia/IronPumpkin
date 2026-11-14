//! The `NeoForge` transfer API (`net.neoforged.neoforge.transfer`).
//!
//! Resources, resource and energy handlers, item access and transactions, over Pumpkin item
//! stacks, fluids and inventories. The `capabilities` module exposes the handlers on blocks,
//! entities and items.
//!
//! Every `insert` and `extract` takes an open [`TransactionContext`]: see [`transaction`].
//! Amounts are `u32`, so the negative amounts that `TransferPreconditions` rejects do not exist,
//! and an empty resource inserts and extracts nothing where `NeoForge` throws.

pub mod inventory;
#[cfg(test)]
mod tests;
pub mod transaction;

use std::{
    fmt,
    sync::{Arc, Mutex, PoisonError},
};

use pumpkin_data::{fluid::Fluid, item::Item, item_stack::ItemStack};

pub use transaction::{
    Journal, Lifecycle, RootCommitJournal, SnapshotJournal, Transaction, TransactionContext,
    TransactionError,
};

/// `FluidType.BUCKET_VOLUME`: the amount of fluid in a bucket, in millibuckets.
pub const BUCKET_VOLUME: u32 = 1000;

/// `Resource`: an immutable kind of thing a handler stores, without an amount.
pub trait Resource: Clone + PartialEq + Send + Sync + 'static {
    /// The empty resource, `EMPTY`.
    fn empty() -> Self;

    /// Whether this is the empty resource, which an empty index holds.
    fn is_empty(&self) -> bool;

    /// The largest amount that one index holds by default, such as the max stack size of an
    /// item. `None` for no limit.
    fn max_stack_size(&self) -> Option<u32> {
        None
    }
}

/// `ItemResource`: an item and its data components, without a count.
#[derive(Clone)]
pub struct ItemResource(ItemStack);

impl ItemResource {
    /// `ItemResource.EMPTY`.
    #[must_use]
    pub fn empty() -> Self {
        Self(ItemStack::EMPTY.clone())
    }

    /// `ItemResource.of(ItemStack)`: the item and components of `stack`.
    #[must_use]
    pub fn of(stack: &ItemStack) -> Self {
        if stack.is_empty() {
            Self::empty()
        } else {
            Self(stack.copy_with_count(1))
        }
    }

    /// `ItemResource.of(ItemLike)`: `item` with its default components.
    #[must_use]
    pub fn of_item(item: &'static Item) -> Self {
        Self::of(&ItemStack::new(1, item))
    }

    /// `getItem`.
    #[must_use]
    pub const fn item(&self) -> &'static Item {
        self.0.item
    }

    /// `is(Item)`.
    #[must_use]
    pub const fn is(&self, item: &Item) -> bool {
        self.0.item.id == item.id
    }

    /// `matches(ItemStack)`: whether `stack` has this item and these components.
    #[must_use]
    pub fn matches(&self, stack: &ItemStack) -> bool {
        if self.is_empty() || stack.is_empty() {
            return self.is_empty() && stack.is_empty();
        }
        self.0.are_items_and_components_equal(stack)
    }

    /// `toStack(count)`.
    #[must_use]
    pub fn to_stack(&self, count: u8) -> ItemStack {
        if self.is_empty() || count == 0 {
            ItemStack::EMPTY.clone()
        } else {
            self.0.copy_with_count(count)
        }
    }

    /// A stack of one of this resource, for checks that take a stack.
    #[must_use]
    pub const fn as_stack(&self) -> &ItemStack {
        &self.0
    }
}

impl Resource for ItemResource {
    fn empty() -> Self {
        Self::empty()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn max_stack_size(&self) -> Option<u32> {
        Some(u32::from(self.0.get_max_stack_size()))
    }
}

impl PartialEq for ItemResource {
    fn eq(&self, other: &Self) -> bool {
        self.matches(&other.0)
    }
}

impl fmt::Debug for ItemResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ItemResource({})", self.0.item.registry_key)
    }
}

/// `FluidResource`: a fluid. Pumpkin fluids have no data components.
#[derive(Clone, Copy)]
pub struct FluidResource(&'static Fluid);

impl FluidResource {
    /// `FluidResource.EMPTY`.
    pub const EMPTY: Self = Self(&Fluid::EMPTY);

    /// `FluidResource.of(Fluid)`.
    #[must_use]
    pub const fn of(fluid: &'static Fluid) -> Self {
        Self(fluid)
    }

    /// `getFluid`.
    #[must_use]
    pub const fn fluid(&self) -> &'static Fluid {
        self.0
    }
}

impl Resource for FluidResource {
    fn empty() -> Self {
        Self::EMPTY
    }

    fn is_empty(&self) -> bool {
        self.0.id == Fluid::EMPTY.id
    }
}

impl PartialEq for FluidResource {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}

impl fmt::Debug for FluidResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FluidResource({})", self.0.name)
    }
}

fn saturate(amount: u64) -> u32 {
    u32::try_from(amount).unwrap_or(u32::MAX)
}

/// `ResourceHandler<T>`: indexed storage of a resource, such as the slots of an inventory or the
/// tanks of a machine.
///
/// `NeoForge` overloads `insert` and `extract` with and without an index;
/// here the indexed forms are [`insert_at`](Self::insert_at) and
/// [`extract_at`](Self::extract_at).
pub trait ResourceHandler<T: Resource>: Send + Sync {
    /// `size`: the number of indices.
    fn size(&self) -> usize;

    /// `getResource`: the resource at `index`, empty when the index holds nothing.
    fn get_resource(&self, index: usize) -> T;

    /// `getAmountAsLong`.
    fn get_amount_as_long(&self, index: usize) -> u64;

    /// `getAmountAsInt`: the amount, saturated to `u32`.
    fn get_amount_as_int(&self, index: usize) -> u32 {
        saturate(self.get_amount_as_long(index))
    }

    /// `getCapacityAsLong`: the largest amount of `resource` that `index` holds.
    fn get_capacity_as_long(&self, index: usize, resource: &T) -> u64;

    /// `getCapacityAsInt`: the capacity, saturated to `u32`.
    fn get_capacity_as_int(&self, index: usize, resource: &T) -> u32 {
        saturate(self.get_capacity_as_long(index, resource))
    }

    /// `isValid`: whether `index` may hold `resource`, ignoring its current content.
    fn is_valid(&self, index: usize, resource: &T) -> bool;

    /// `insert(index, resource, amount, transaction)`: inserts up to `amount` of `resource` at
    /// `index` and returns the inserted amount.
    fn insert_at(
        &self,
        index: usize,
        resource: &T,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32;

    /// `extract(index, resource, amount, transaction)`: extracts up to `amount` of `resource`
    /// from `index` and returns the extracted amount.
    fn extract_at(
        &self,
        index: usize,
        resource: &T,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32;

    /// `insert(resource, amount, transaction)`: inserts into the indices in order.
    fn insert(&self, resource: &T, amount: u32, transaction: &mut TransactionContext) -> u32 {
        if resource.is_empty() {
            return 0;
        }
        let mut inserted = 0;
        for index in 0..self.size() {
            if inserted == amount {
                break;
            }
            inserted += self.insert_at(index, resource, amount - inserted, transaction);
        }
        inserted
    }

    /// `extract(resource, amount, transaction)`: extracts from the indices in order.
    fn extract(&self, resource: &T, amount: u32, transaction: &mut TransactionContext) -> u32 {
        if resource.is_empty() {
            return 0;
        }
        let mut extracted = 0;
        for index in 0..self.size() {
            if extracted == amount {
                break;
            }
            extracted += self.extract_at(index, resource, amount - extracted, transaction);
        }
        extracted
    }
}

/// `ResourceHandlerUtil.isFull`: whether no index takes more of its resource.
pub fn is_full<T: Resource>(handler: &dyn ResourceHandler<T>) -> bool {
    (0..handler.size()).all(|index| {
        handler.get_amount_as_long(index)
            >= handler.get_capacity_as_long(index, &handler.get_resource(index))
    })
}

/// `ItemUtil.insertItemReturnRemaining`: inserts `stack` in a transaction nested in
/// `transaction` (a root without one), commits it unless `simulate`, and returns what did not
/// fit.
///
/// # Errors
///
/// [`TransactionError::RootAlreadyOpen`] without `transaction` while this thread has an open
/// transaction.
pub fn insert_item_return_remaining(
    handler: &dyn ResourceHandler<ItemResource>,
    stack: &ItemStack,
    simulate: bool,
    transaction: Option<&mut TransactionContext>,
) -> Result<ItemStack, TransactionError> {
    if stack.is_empty() {
        return Ok(ItemStack::EMPTY.clone());
    }
    let mut nested = Transaction::open(transaction)?;
    let inserted = handler.insert(
        &ItemResource::of(stack),
        u32::from(stack.item_count),
        &mut nested,
    );
    if !simulate {
        nested.commit();
    }
    let leftover = u32::from(stack.item_count).saturating_sub(inserted);
    Ok(u8::try_from(leftover).map_or_else(
        |_| stack.clone(),
        |leftover| {
            if leftover == 0 {
                ItemStack::EMPTY.clone()
            } else {
                stack.copy_with_count(leftover)
            }
        },
    ))
}

/// `EnergyHandler`: a store of energy units.
pub trait EnergyHandler: Send + Sync {
    /// `getAmountAsLong`.
    fn get_amount_as_long(&self) -> u64;

    /// `getAmountAsInt`: the amount, saturated to `u32`.
    fn get_amount_as_int(&self) -> u32 {
        saturate(self.get_amount_as_long())
    }

    /// `getCapacityAsLong`.
    fn get_capacity_as_long(&self) -> u64;

    /// `getCapacityAsInt`: the capacity, saturated to `u32`.
    fn get_capacity_as_int(&self) -> u32 {
        saturate(self.get_capacity_as_long())
    }

    /// `insert`: inserts up to `amount` and returns the inserted amount.
    fn insert(&self, amount: u32, transaction: &mut TransactionContext) -> u32;

    /// `extract`: extracts up to `amount` and returns the extracted amount.
    fn extract(&self, amount: u32, transaction: &mut TransactionContext) -> u32;
}

/// `ResourceStack<T>`: a resource and an amount.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceStack<T> {
    /// The resource.
    pub resource: T,
    /// The amount.
    pub amount: u32,
}

/// The journal of one index of a [`ResourceStacksResourceHandler`].
struct IndexStack<T: Resource> {
    index: usize,
    stack: Mutex<ResourceStack<T>>,
    on_contents_changed: Arc<ContentsChanged<T>>,
}

/// `onContentsChanged` of a [`ResourceStacksResourceHandler`].
type ContentsChanged<T> = dyn Fn(usize, &ResourceStack<T>) + Send + Sync;

/// `isValid` of a [`ResourceStacksResourceHandler`].
type Validator<T> = dyn Fn(usize, &T) -> bool + Send + Sync;

impl<T: Resource> IndexStack<T> {
    fn get(&self) -> ResourceStack<T> {
        self.stack
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set(&self, stack: ResourceStack<T>) {
        *self.stack.lock().unwrap_or_else(PoisonError::into_inner) = stack;
    }
}

impl<T: Resource> Journal for IndexStack<T> {
    type Snapshot = ResourceStack<T>;

    fn create_snapshot(&self) -> ResourceStack<T> {
        self.get()
    }

    fn revert_to_snapshot(&self, snapshot: ResourceStack<T>) {
        self.set(snapshot);
    }

    fn on_root_commit(&self, original_state: ResourceStack<T>) {
        (self.on_contents_changed)(self.index, &original_state);
    }
}

/// `ResourceStacksResourceHandler<T>` (`ItemStacksResourceHandler` and
/// `FluidStacksResourceHandler` for items and fluids).
///
/// A transactional handler that owns its stacks, for the storage of a mod block or entity. Each
/// index has its own journal, so transactions of two threads on two indices do not wait on each
/// other.
pub struct ResourceStacksResourceHandler<T: Resource> {
    stacks: Vec<Arc<SnapshotJournal<IndexStack<T>>>>,
    capacity: u32,
    valid: Box<Validator<T>>,
}

/// `ItemStacksResourceHandler`.
pub type ItemStacksResourceHandler = ResourceStacksResourceHandler<ItemResource>;
/// `FluidStacksResourceHandler`.
pub type FluidStacksResourceHandler = ResourceStacksResourceHandler<FluidResource>;

fn index_stacks<T: Resource>(
    stacks: Vec<ResourceStack<T>>,
    on_contents_changed: &Arc<ContentsChanged<T>>,
) -> Vec<Arc<SnapshotJournal<IndexStack<T>>>> {
    stacks
        .into_iter()
        .enumerate()
        .map(|(index, stack)| {
            SnapshotJournal::new(IndexStack {
                index,
                stack: Mutex::new(stack),
                on_contents_changed: on_contents_changed.clone(),
            })
        })
        .collect()
}

impl<T: Resource> ResourceStacksResourceHandler<T> {
    /// `size` empty indices that each hold up to `capacity`, and up to the max stack size of an
    /// item.
    #[must_use]
    pub fn new(size: usize, capacity: u32) -> Self {
        let empty = ResourceStack {
            resource: T::empty(),
            amount: 0,
        };
        let on_contents_changed: Arc<ContentsChanged<T>> = Arc::new(|_, _| {});
        Self {
            stacks: index_stacks(vec![empty; size], &on_contents_changed),
            capacity,
            valid: Box::new(|_, _| true),
        }
    }

    /// Only resources that `valid` accepts at an index go in, as `isValid` overrides do.
    #[must_use]
    pub fn with_validator(
        mut self,
        valid: impl Fn(usize, &T) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.valid = Box::new(valid);
        self
    }

    /// `onContentsChanged(index, previousContents)`: runs after each root commit that touched an
    /// index, with the stack of the index before the transaction, such as to mark a block dirty.
    #[must_use]
    pub fn with_on_contents_changed(
        self,
        on_contents_changed: impl Fn(usize, &ResourceStack<T>) + Send + Sync + 'static,
    ) -> Self {
        let on_contents_changed: Arc<ContentsChanged<T>> = Arc::new(on_contents_changed);
        Self {
            stacks: index_stacks(self.snapshot(), &on_contents_changed),
            capacity: self.capacity,
            valid: self.valid,
        }
    }

    /// A copy of the stacks.
    #[must_use]
    pub fn snapshot(&self) -> Vec<ResourceStack<T>> {
        self.stacks.iter().map(|stack| stack.get()).collect()
    }

    /// Sets the stacks outside a transaction, such as when loading from NBT. The length stays.
    pub fn set_stacks(&self, stacks: impl IntoIterator<Item = ResourceStack<T>>) {
        for (slot, stack) in self.stacks.iter().zip(stacks) {
            slot.set(stack);
        }
    }
}

impl<T: Resource> ResourceHandler<T> for ResourceStacksResourceHandler<T> {
    fn size(&self) -> usize {
        self.stacks.len()
    }

    fn get_resource(&self, index: usize) -> T {
        self.stacks
            .get(index)
            .map_or_else(T::empty, |stack| stack.get().resource)
    }

    fn get_amount_as_long(&self, index: usize) -> u64 {
        self.stacks
            .get(index)
            .map_or(0, |stack| u64::from(stack.get().amount))
    }

    fn get_capacity_as_long(&self, index: usize, resource: &T) -> u64 {
        if !resource.is_empty() && !self.is_valid(index, resource) {
            return 0;
        }
        u64::from(
            resource
                .max_stack_size()
                .map_or(self.capacity, |max| max.min(self.capacity)),
        )
    }

    fn is_valid(&self, index: usize, resource: &T) -> bool {
        index < self.stacks.len() && (self.valid)(index, resource)
    }

    fn insert_at(
        &self,
        index: usize,
        resource: &T,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32 {
        if resource.is_empty() || amount == 0 || !self.is_valid(index, resource) {
            return 0;
        }
        let journal = &self.stacks[index];
        if !journal.claim(transaction) {
            return 0;
        }
        let stack = journal.get();
        if stack.amount > 0 && stack.resource != *resource {
            return 0;
        }
        let capacity = self.get_capacity_as_int(index, resource);
        let inserted = amount.min(capacity.saturating_sub(stack.amount));
        if inserted == 0 || !journal.update_snapshots(transaction) {
            return 0;
        }
        journal.set(ResourceStack {
            resource: resource.clone(),
            amount: stack.amount + inserted,
        });
        inserted
    }

    fn extract_at(
        &self,
        index: usize,
        resource: &T,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32 {
        if resource.is_empty() || amount == 0 || index >= self.stacks.len() {
            return 0;
        }
        let journal = &self.stacks[index];
        if !journal.claim(transaction) {
            return 0;
        }
        let stack = journal.get();
        if stack.amount == 0 || stack.resource != *resource {
            return 0;
        }
        let extracted = amount.min(stack.amount);
        if !journal.update_snapshots(transaction) {
            return 0;
        }
        let left = stack.amount - extracted;
        journal.set(ResourceStack {
            resource: if left == 0 {
                T::empty()
            } else {
                stack.resource
            },
            amount: left,
        });
        extracted
    }
}

/// `CombinedResourceHandler<T>`: the indices of several handlers in a row, such as the two
/// halves of a double chest.
pub struct CombinedResourceHandler<T: Resource> {
    handlers: Vec<Arc<dyn ResourceHandler<T>>>,
}

impl<T: Resource> CombinedResourceHandler<T> {
    /// The indices of `handlers`, in order.
    #[must_use]
    pub const fn new(handlers: Vec<Arc<dyn ResourceHandler<T>>>) -> Self {
        Self { handlers }
    }

    fn locate(&self, mut index: usize) -> Option<(&dyn ResourceHandler<T>, usize)> {
        for handler in &self.handlers {
            let size = handler.size();
            if index < size {
                return Some((handler.as_ref(), index));
            }
            index -= size;
        }
        None
    }
}

impl<T: Resource> ResourceHandler<T> for CombinedResourceHandler<T> {
    fn size(&self) -> usize {
        self.handlers.iter().map(|handler| handler.size()).sum()
    }

    fn get_resource(&self, index: usize) -> T {
        self.locate(index)
            .map_or_else(T::empty, |(handler, index)| handler.get_resource(index))
    }

    fn get_amount_as_long(&self, index: usize) -> u64 {
        self.locate(index)
            .map_or(0, |(handler, index)| handler.get_amount_as_long(index))
    }

    fn get_capacity_as_long(&self, index: usize, resource: &T) -> u64 {
        self.locate(index).map_or(0, |(handler, index)| {
            handler.get_capacity_as_long(index, resource)
        })
    }

    fn is_valid(&self, index: usize, resource: &T) -> bool {
        self.locate(index)
            .is_some_and(|(handler, index)| handler.is_valid(index, resource))
    }

    fn insert_at(
        &self,
        index: usize,
        resource: &T,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32 {
        self.locate(index).map_or(0, |(handler, index)| {
            handler.insert_at(index, resource, amount, transaction)
        })
    }

    fn extract_at(
        &self,
        index: usize,
        resource: &T,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32 {
        self.locate(index).map_or(0, |(handler, index)| {
            handler.extract_at(index, resource, amount, transaction)
        })
    }
}

struct Energy {
    amount: Mutex<u64>,
}

impl Journal for Energy {
    type Snapshot = u64;

    fn create_snapshot(&self) -> u64 {
        *self.amount.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn revert_to_snapshot(&self, snapshot: u64) {
        *self.amount.lock().unwrap_or_else(PoisonError::into_inner) = snapshot;
    }
}

/// `SimpleEnergyHandler`: a transactional energy store with a capacity and transfer limits.
pub struct SimpleEnergyHandler {
    journal: Arc<SnapshotJournal<Energy>>,
    capacity: u64,
    max_insert: u32,
    max_extract: u32,
}

impl SimpleEnergyHandler {
    /// An empty store of `capacity` that moves up to `max_insert` in and `max_extract` out per
    /// call.
    #[must_use]
    pub fn new(capacity: u64, max_insert: u32, max_extract: u32) -> Self {
        Self {
            journal: SnapshotJournal::new(Energy {
                amount: Mutex::new(0),
            }),
            capacity,
            max_insert,
            max_extract,
        }
    }

    /// `set`: sets the amount outside a transaction, such as when loading from NBT.
    pub fn set(&self, amount: u64) {
        *self
            .journal
            .amount
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = amount.min(self.capacity);
    }

    fn change(&self, amount: u64, transaction: &TransactionContext) -> bool {
        if !self.journal.update_snapshots(transaction) {
            return false;
        }
        *self
            .journal
            .amount
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = amount;
        true
    }
}

impl EnergyHandler for SimpleEnergyHandler {
    fn get_amount_as_long(&self) -> u64 {
        self.journal.create_snapshot()
    }

    fn get_capacity_as_long(&self) -> u64 {
        self.capacity
    }

    fn insert(&self, amount: u32, transaction: &mut TransactionContext) -> u32 {
        if !self.journal.claim(transaction) {
            return 0;
        }
        let current = self.get_amount_as_long();
        let inserted = u64::from(amount.min(self.max_insert)).min(self.capacity - current);
        if inserted == 0 || !self.change(current + inserted, transaction) {
            return 0;
        }
        saturate(inserted)
    }

    fn extract(&self, amount: u32, transaction: &mut TransactionContext) -> u32 {
        if !self.journal.claim(transaction) {
            return 0;
        }
        let current = self.get_amount_as_long();
        let extracted = u64::from(amount.min(self.max_extract)).min(current);
        if extracted == 0 || !self.change(current - extracted, transaction) {
            return 0;
        }
        saturate(extracted)
    }
}

/// `ItemAccess`: the stack that an item capability reads and changes, such as a stack in a
/// player hand. It is the context of the item capabilities in
/// `capabilities`.
pub trait ItemAccess: Send + Sync {
    /// `getResource`.
    fn get_resource(&self) -> ItemResource;

    /// `getAmount`.
    fn get_amount(&self) -> u32;

    /// `insert`: adds up to `amount` of `resource` to the accessed stack.
    fn insert(
        &self,
        resource: &ItemResource,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32;

    /// `extract`: removes up to `amount` of `resource` from the accessed stack.
    fn extract(
        &self,
        resource: &ItemResource,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32;
}

struct AccessedStack(Mutex<ItemStack>);

impl Journal for AccessedStack {
    type Snapshot = ItemStack;

    fn create_snapshot(&self) -> ItemStack {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn revert_to_snapshot(&self, snapshot: ItemStack) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = snapshot;
    }
}

/// `ItemAccess.forStack`: an access over a stack that it owns. [`stack`](Self::stack) reads the
/// result.
pub struct StackItemAccess {
    journal: Arc<SnapshotJournal<AccessedStack>>,
}

impl StackItemAccess {
    /// An access over `stack`.
    #[must_use]
    pub fn new(stack: ItemStack) -> Arc<Self> {
        Arc::new(Self {
            journal: SnapshotJournal::new(AccessedStack(Mutex::new(stack))),
        })
    }

    /// The accessed stack.
    #[must_use]
    pub fn stack(&self) -> ItemStack {
        self.journal.create_snapshot()
    }

    fn set(&self, stack: ItemStack, transaction: &TransactionContext) -> bool {
        if !self.journal.update_snapshots(transaction) {
            return false;
        }
        *self
            .journal
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = stack;
        true
    }
}

impl ItemAccess for StackItemAccess {
    fn get_resource(&self) -> ItemResource {
        ItemResource::of(&self.stack())
    }

    fn get_amount(&self) -> u32 {
        u32::from(self.stack().item_count)
    }

    fn insert(
        &self,
        resource: &ItemResource,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32 {
        if !self.journal.claim(transaction) {
            return 0;
        }
        let stack = self.stack();
        if resource.is_empty() || (!stack.is_empty() && !resource.matches(&stack)) {
            return 0;
        }
        let max = resource.as_stack().get_max_stack_size();
        let current = if stack.is_empty() {
            0
        } else {
            stack.item_count
        };
        let inserted = u8::try_from(amount)
            .unwrap_or(u8::MAX)
            .min(max.saturating_sub(current));
        if inserted == 0 || !self.set(resource.to_stack(current + inserted), transaction) {
            return 0;
        }
        u32::from(inserted)
    }

    fn extract(
        &self,
        resource: &ItemResource,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32 {
        if !self.journal.claim(transaction) {
            return 0;
        }
        let stack = self.stack();
        if resource.is_empty() || stack.is_empty() || !resource.matches(&stack) {
            return 0;
        }
        let extracted = u8::try_from(amount)
            .unwrap_or(u8::MAX)
            .min(stack.item_count);
        if extracted == 0 || !self.set(resource.to_stack(stack.item_count - extracted), transaction)
        {
            return 0;
        }
        u32::from(extracted)
    }
}
