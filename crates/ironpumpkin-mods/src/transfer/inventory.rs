//! Item handlers over Pumpkin inventories: `VanillaContainerWrapper` and
//! `WorldlyContainerWrapper`.
//!
//! A handler writes to the inventory at once and rolls the slot back when the transaction
//! aborts. Worlds tick in parallel, so another thread can write the slot in between: a rollback
//! restores a slot only while it still holds what the transaction wrote, and otherwise keeps the
//! other write and logs a warning. `NeoForge` restores the slot without the check.
//!
//! The face rules of furnaces, brewing stands and shulker boxes (`WorldlyContainer`) are a
//! table here, because `pumpkin-inventory` has no face slots.

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex, PoisonError, Weak},
};

use pumpkin_data::{
    BlockDirection, data_component_impl::CookingFuelImpl, item::Item, item_stack::ItemStack,
};
use pumpkin_inventory::Inventory;
use tracing::warn;

use super::{
    ItemResource, Journal, Resource, ResourceHandler, SnapshotJournal, TransactionContext,
};

/// The vanilla container rules that a wrapper applies beyond `Inventory::is_valid_slot_for`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContainerKind {
    Plain,
    /// `AbstractFurnaceBlockEntity`: furnace, blast furnace and smoker.
    Furnace,
    BrewingStand,
    ShulkerBox,
}

const FURNACE_INPUT: usize = 0;
const FURNACE_FUEL: usize = 1;
const FURNACE_RESULT: usize = 2;
const BREWING_INGREDIENT: usize = 3;
const BREWING_FUEL: usize = 4;

impl ContainerKind {
    /// `getSlotsForFace`, `None` for every slot.
    const fn slots_for_face(self, side: BlockDirection) -> Option<&'static [usize]> {
        match (self, side) {
            (Self::Furnace, BlockDirection::Up) => Some(&[FURNACE_INPUT]),
            (Self::Furnace, BlockDirection::Down) => Some(&[FURNACE_RESULT, FURNACE_FUEL]),
            (Self::Furnace, _) => Some(&[FURNACE_FUEL]),
            (Self::BrewingStand, BlockDirection::Up) => Some(&[BREWING_INGREDIENT]),
            (Self::BrewingStand, BlockDirection::Down) => Some(&[0, 1, 2, BREWING_INGREDIENT]),
            (Self::BrewingStand, _) => Some(&[0, 1, 2, BREWING_FUEL]),
            (Self::Plain | Self::ShulkerBox, _) => None,
        }
    }

    /// `canPlaceItem`.
    fn can_place(self, inventory: &dyn Inventory, slot: usize, stack: &ItemStack) -> bool {
        if self == Self::Furnace {
            return match slot {
                FURNACE_RESULT => false,
                FURNACE_FUEL => {
                    stack.get_data_component::<CookingFuelImpl>().is_some()
                        || (stack.item.id == Item::BUCKET.id
                            && inventory.get_stack(FURNACE_FUEL).item.id != Item::BUCKET.id)
                }
                _ => true,
            };
        }
        inventory.is_valid_slot_for(slot, stack)
    }

    /// `canTakeItemThroughFace`.
    fn can_take_through_face(self, slot: usize, stack: &ItemStack, side: BlockDirection) -> bool {
        match self {
            Self::Furnace if side == BlockDirection::Down && slot == FURNACE_FUEL => {
                stack.item.id == Item::WATER_BUCKET.id || stack.item.id == Item::BUCKET.id
            }
            Self::BrewingStand if slot == BREWING_INGREDIENT => {
                stack.item.id == Item::GLASS_BOTTLE.id
            }
            _ => true,
        }
    }

    /// The capacity limits of `VanillaContainerWrapper.SlotWrapper.getCapacity`.
    const fn capacity_limit(self, slot: usize, resource: &ItemResource) -> Option<u32> {
        match self {
            Self::Furnace if slot == FURNACE_FUEL && resource.is(&Item::BUCKET) => Some(1),
            Self::BrewingStand if slot < BREWING_INGREDIENT => Some(1),
            _ => None,
        }
    }
}

/// One slot of an inventory as a journal. `written` is what the open transactions last wrote.
struct Slot {
    inventory: Arc<dyn Inventory>,
    index: usize,
    written: Mutex<Option<ItemStack>>,
}

impl Slot {
    fn write(&self, stack: ItemStack) {
        self.inventory.set_stack(self.index, stack.clone());
        *self.written.lock().unwrap_or_else(PoisonError::into_inner) = Some(stack);
    }
}

impl Journal for Slot {
    type Snapshot = ItemStack;

    fn create_snapshot(&self) -> ItemStack {
        self.inventory.get_stack(self.index)
    }

    fn revert_to_snapshot(&self, snapshot: ItemStack) {
        let current = self.inventory.get_stack(self.index);
        let mut written = self.written.lock().unwrap_or_else(PoisonError::into_inner);
        let ours = written
            .as_ref()
            .is_none_or(|written| same_stack(written, &current));
        if ours {
            self.inventory.set_stack(self.index, snapshot.clone());
            *written = Some(snapshot);
        } else {
            warn!(
                "transaction rollback keeps slot {} of an inventory: another write changed it during the transaction",
                self.index
            );
        }
    }

    fn on_root_commit(&self, _original_state: ItemStack) {
        *self.written.lock().unwrap_or_else(PoisonError::into_inner) = None;
        self.inventory.mark_dirty();
    }
}

fn same_stack(a: &ItemStack, b: &ItemStack) -> bool {
    if a.is_empty() || b.is_empty() {
        return a.is_empty() && b.is_empty();
    }
    a.item_count == b.item_count && a.are_items_and_components_equal(b)
}

/// `VanillaContainerWrapper`: an item handler over every slot of a Pumpkin inventory.
///
/// One
/// wrapper exists per inventory while a handle to it lives, as `VanillaContainerWrapper.of`
/// caches it, so the transactions of all lookups share its slot journals.
pub struct InventoryWrapper {
    inventory: Arc<dyn Inventory>,
    kind: ContainerKind,
    slots: Vec<Arc<SnapshotJournal<Slot>>>,
}

type WrapperCache = Mutex<HashMap<usize, Weak<InventoryWrapper>>>;

static WRAPPERS: LazyLock<WrapperCache> = LazyLock::new(WrapperCache::default);

impl InventoryWrapper {
    /// `VanillaContainerWrapper.of`: the wrapper of `inventory`.
    #[must_use]
    pub fn of(inventory: Arc<dyn Inventory>) -> Arc<Self> {
        Self::of_kind(inventory, ContainerKind::Plain)
    }

    pub(crate) fn of_kind(inventory: Arc<dyn Inventory>, kind: ContainerKind) -> Arc<Self> {
        let key = Arc::as_ptr(&inventory).cast::<()>() as usize;
        let mut wrappers = WRAPPERS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(wrapper) = wrappers.get(&key).and_then(Weak::upgrade) {
            return wrapper;
        }
        wrappers.retain(|_, wrapper| wrapper.strong_count() > 0);
        let slots = (0..inventory.size())
            .map(|index| {
                SnapshotJournal::new(Slot {
                    inventory: inventory.clone(),
                    index,
                    written: Mutex::new(None),
                })
            })
            .collect();
        let wrapper = Arc::new(Self {
            inventory,
            kind,
            slots,
        });
        wrappers.insert(key, Arc::downgrade(&wrapper));
        wrapper
    }

    /// The wrapped inventory.
    #[must_use]
    pub fn inventory(&self) -> &Arc<dyn Inventory> {
        &self.inventory
    }
}

impl ResourceHandler<ItemResource> for InventoryWrapper {
    fn size(&self) -> usize {
        self.slots.len()
    }

    fn get_resource(&self, index: usize) -> ItemResource {
        if index >= self.slots.len() {
            return ItemResource::empty();
        }
        ItemResource::of(&self.inventory.get_stack(index))
    }

    fn get_amount_as_long(&self, index: usize) -> u64 {
        if index >= self.slots.len() {
            return 0;
        }
        let stack = self.inventory.get_stack(index);
        if stack.is_empty() {
            0
        } else {
            u64::from(stack.item_count)
        }
    }

    fn get_capacity_as_long(&self, index: usize, resource: &ItemResource) -> u64 {
        let container = u32::from(self.inventory.get_max_count_per_stack());
        let capacity = resource
            .max_stack_size()
            .filter(|_| !resource.is_empty())
            .map_or(container, |max| max.min(container));
        u64::from(
            self.kind
                .capacity_limit(index, resource)
                .map_or(capacity, |limit| limit.min(capacity)),
        )
    }

    fn is_valid(&self, index: usize, resource: &ItemResource) -> bool {
        index < self.slots.len()
            && self
                .kind
                .can_place(self.inventory.as_ref(), index, resource.as_stack())
    }

    fn insert_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32 {
        if resource.is_empty() || amount == 0 || !self.is_valid(index, resource) {
            return 0;
        }
        let current = self.inventory.get_stack(index);
        if !current.is_empty() && !resource.matches(&current) {
            return 0;
        }
        let count = if current.is_empty() {
            0
        } else {
            u32::from(current.item_count)
        };
        let capacity = self.get_capacity_as_int(index, resource);
        let inserted = amount.min(capacity.saturating_sub(count));
        let Ok(new_count) = u8::try_from(count + inserted) else {
            return 0;
        };
        if inserted > 0 {
            let slot = &self.slots[index];
            slot.update_snapshots(transaction);
            slot.write(resource.to_stack(new_count));
        }
        inserted
    }

    fn extract_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32 {
        if resource.is_empty() || amount == 0 || index >= self.slots.len() {
            return 0;
        }
        let current = self.inventory.get_stack(index);
        if current.is_empty() || !resource.matches(&current) {
            return 0;
        }
        let extracted = amount.min(u32::from(current.item_count));
        let Ok(left) = u8::try_from(u32::from(current.item_count) - extracted) else {
            return 0;
        };
        if extracted > 0 {
            let slot = &self.slots[index];
            slot.update_snapshots(transaction);
            slot.write(current.copy_with_count(left));
        }
        extracted
    }
}

/// `WorldlyContainerWrapper`: the slots of a sided inventory that one face exposes, with the
/// face rules of the inventory. Without a side it exposes every slot.
pub struct SidedInventoryWrapper {
    wrapper: Arc<InventoryWrapper>,
    side: Option<BlockDirection>,
}

impl SidedInventoryWrapper {
    pub(crate) fn new(
        inventory: Arc<dyn Inventory>,
        kind: ContainerKind,
        side: Option<BlockDirection>,
    ) -> Self {
        Self {
            wrapper: InventoryWrapper::of_kind(inventory, kind),
            side,
        }
    }

    fn convert(&self, index: usize) -> Option<usize> {
        self.side
            .and_then(|side| self.wrapper.kind.slots_for_face(side))
            .map_or_else(
                || (index < self.wrapper.size()).then_some(index),
                |slots| slots.get(index).copied(),
            )
    }
}

impl ResourceHandler<ItemResource> for SidedInventoryWrapper {
    fn size(&self) -> usize {
        self.side
            .and_then(|side| self.wrapper.kind.slots_for_face(side))
            .map_or_else(|| self.wrapper.size(), <[usize]>::len)
    }

    fn get_resource(&self, index: usize) -> ItemResource {
        self.convert(index)
            .map_or_else(ItemResource::empty, |slot| self.wrapper.get_resource(slot))
    }

    fn get_amount_as_long(&self, index: usize) -> u64 {
        self.convert(index)
            .map_or(0, |slot| self.wrapper.get_amount_as_long(slot))
    }

    fn get_capacity_as_long(&self, index: usize, resource: &ItemResource) -> u64 {
        self.convert(index)
            .map_or(0, |slot| self.wrapper.get_capacity_as_long(slot, resource))
    }

    fn is_valid(&self, index: usize, resource: &ItemResource) -> bool {
        self.convert(index)
            .is_some_and(|slot| self.wrapper.is_valid(slot, resource))
    }

    fn insert_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32 {
        // `canPlaceItemThroughFace` is `canPlaceItem`, which `is_valid` checks.
        self.convert(index).map_or(0, |slot| {
            self.wrapper.insert_at(slot, resource, amount, transaction)
        })
    }

    fn extract_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32 {
        let Some(slot) = self.convert(index) else {
            return 0;
        };
        if let Some(side) = self.side
            && !self
                .wrapper
                .kind
                .can_take_through_face(slot, resource.as_stack(), side)
        {
            return 0;
        }
        self.wrapper.extract_at(slot, resource, amount, transaction)
    }
}
