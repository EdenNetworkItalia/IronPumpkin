//! Item handlers over Pumpkin inventories: `VanillaContainerWrapper` and
//! `WorldlyContainerWrapper`.
//!
//! A handler writes to the inventory at once and rolls its writes back when the transaction
//! aborts. Code outside the transaction, such as a Pumpkin hopper or a player click, can write
//! the slot in between, so a slot journal records the net amount that the open transactions
//! moved, not the stack it saw: a rollback adds or takes back only that amount. When another
//! writer replaced the item, the rollback keeps that item and logs a warning. `NeoForge` restores
//! the saved stack instead.
//!
//! The vanilla rules of each container (`canPlaceItem`, `getMaxStackSize`, the `WorldlyContainer`
//! faces, `onTransfer`) are a table here, because `pumpkin-inventory` has neither face slots nor
//! the vanilla placement rules of every container. A transactional write does not trigger the
//! side effects of a vanilla `setItem` early: the cooking progress of a furnace resets only when
//! the root commits, and an insert into an empty hopper sets its cooldown in the transaction.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock, Mutex, PoisonError, Weak},
};

use pumpkin_core::block::entities::{
    blasting_furnace::BlastingFurnaceBlockEntity, furnace::FurnaceBlockEntity,
    furnace_like_block_entity::CookingBlockEntityBase, hopper::HopperBlockEntity,
    smoker::SmokerBlockEntity,
};
use pumpkin_data::{
    BlockDirection,
    data_component_impl::{BrewingFuelImpl, CookingFuelImpl, JukeboxPlayableImpl},
    item::Item,
    item_stack::ItemStack,
    potion_brewing::BREWING_RECIPES,
    recipes::{CookingRecipeKind, get_cooking_recipe_with_ingredient},
    tag::{self, Taggable},
};
use pumpkin_inventory::Inventory;
use tracing::warn;

use super::{
    ItemResource, Journal, Resource, ResourceHandler, SnapshotJournal, TransactionContext,
};

/// The vanilla container rules that a wrapper applies beyond `Inventory::is_valid_slot_for`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ContainerKind {
    Plain,
    /// `AbstractFurnaceBlockEntity`: furnace, blast furnace and smoker.
    Furnace,
    BrewingStand,
    ShulkerBox,
    Hopper,
    Jukebox,
    ChiseledBookshelf,
    Crafter,
}

const FURNACE_INPUT: usize = 0;
const FURNACE_FUEL: usize = 1;
const FURNACE_RESULT: usize = 2;
const BREWING_INGREDIENT: usize = 3;
const BREWING_FUEL: usize = 4;
/// `HopperBlockEntity.MOVE_ITEM_SPEED`.
const HOPPER_COOLDOWN: i32 = 8;

/// `RecipePropertySet.BREWING_REAGENTS` and the `BREWING_INPUTS` part of
/// `PotionIngredient.isPotionInput`, from the brewing recipes.
static BREWING_REAGENTS: LazyLock<HashSet<u16>> = LazyLock::new(|| {
    BREWING_RECIPES
        .iter()
        .map(|recipe| recipe.reagent().id)
        .collect()
});
static BREWING_INPUTS: LazyLock<HashSet<u16>> = LazyLock::new(|| {
    BREWING_RECIPES
        .iter()
        .map(|recipe| recipe.from_item().id)
        .collect()
});

fn count(stack: &ItemStack) -> u32 {
    if stack.is_empty() {
        0
    } else {
        u32::from(stack.item_count)
    }
}

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
            _ => None,
        }
    }

    /// `getMaxStackSize()` of the container.
    fn max_stack_size(self, inventory: &dyn Inventory) -> u32 {
        match self {
            Self::Jukebox | Self::ChiseledBookshelf => 1,
            _ => u32::from(inventory.get_max_count_per_stack()),
        }
    }

    /// `canPlaceItem`, and `canPlaceItemThroughFace` of the sided containers.
    fn can_place(self, inventory: &dyn Inventory, slot: usize, stack: &ItemStack) -> bool {
        let current = || inventory.get_stack(slot);
        match self {
            Self::Furnace => match slot {
                FURNACE_RESULT => false,
                FURNACE_FUEL => {
                    stack.get_data_component::<CookingFuelImpl>().is_some()
                        || (stack.item.id == Item::BUCKET.id
                            && current().item.id != Item::BUCKET.id)
                }
                _ => true,
            },
            Self::BrewingStand => match slot {
                BREWING_FUEL => stack.get_data_component::<BrewingFuelImpl>().is_some(),
                BREWING_INGREDIENT => BREWING_REAGENTS.contains(&stack.item.id),
                _ => {
                    (BREWING_INPUTS.contains(&stack.item.id)
                        || stack
                            .item
                            .has_tag(&tag::Item::MINECRAFT_BREWING_POTION_INPUTS))
                        && current().is_empty()
                }
            },
            Self::Jukebox => {
                stack.get_data_component::<JukeboxPlayableImpl>().is_some() && current().is_empty()
            }
            Self::ChiseledBookshelf => {
                stack.item.has_tag(&tag::Item::MINECRAFT_BOOKSHELF_BOOKS) && current().is_empty()
            }
            Self::Crafter => {
                // `is_valid_slot_for` of the Pumpkin crafter is the disabled slot check.
                if !inventory.is_valid_slot_for(slot, stack) {
                    return false;
                }
                let base = current();
                if base.is_empty() {
                    return true;
                }
                let size = base.item_count;
                size < base.get_max_stack_size()
                    && !(slot + 1..inventory.size()).any(|other| {
                        let other_stack = inventory.get_stack(other);
                        inventory.is_valid_slot_for(other, &other_stack)
                            && (other_stack.is_empty()
                                || (other_stack.item_count < size
                                    && other_stack.are_items_and_components_equal(&base)))
                    })
            }
            Self::Plain | Self::ShulkerBox | Self::Hopper => {
                inventory.is_valid_slot_for(slot, stack)
            }
        }
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

/// Runs `$body` on the furnace, blast furnace or smoker behind `$inventory`.
macro_rules! with_furnace {
    ($inventory:expr, |$furnace:ident| $body:expr) => {{
        let any = $inventory.as_any();
        if let Some($furnace) = any.downcast_ref::<FurnaceBlockEntity>() {
            Some($body)
        } else if let Some($furnace) = any.downcast_ref::<BlastingFurnaceBlockEntity>() {
            Some($body)
        } else if let Some($furnace) = any.downcast_ref::<SmokerBlockEntity>() {
            Some($body)
        } else {
            None
        }
    }};
}

/// Writes `stack` to `slot`. The Pumpkin furnace resets its cooking progress in `set_stack` when
/// the input item changes; inside a transaction that reset waits for the root commit, as the
/// `insideTransaction` flag of `NeoForge` defers it, so the write keeps the progress.
fn write_slot(inventory: &dyn Inventory, kind: ContainerKind, slot: usize, stack: ItemStack) {
    if kind == ContainerKind::Furnace && slot == FURNACE_INPUT {
        let progress = with_furnace!(inventory, |furnace| (
            furnace.get_cooking_time_spent(),
            furnace.get_cooking_total_time()
        ));
        inventory.set_stack(slot, stack);
        if let Some((spent, total)) = progress {
            with_furnace!(inventory, |furnace| {
                furnace.set_cooking_time_spent(spent);
                furnace.set_cooking_total_time(total);
            });
        }
    } else {
        inventory.set_stack(slot, stack);
    }
}

/// What the open transactions of the owner moved in one slot: `net` items of `kind`.
#[derive(Clone, Default)]
struct Moved {
    kind: Option<ItemResource>,
    net: i64,
    /// The transactions moved two kinds of item, so a rollback cannot undo them.
    untracked: bool,
}

/// One slot of an inventory as a journal.
struct Slot {
    inventory: Arc<dyn Inventory>,
    kind: ContainerKind,
    index: usize,
    moved: Mutex<Moved>,
}

impl Slot {
    fn lock(&self) -> std::sync::MutexGuard<'_, Moved> {
        self.moved.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn record(&self, resource: &ItemResource, delta: i64) {
        let mut moved = self.lock();
        match &moved.kind {
            Some(kind) if kind == resource => moved.net += delta,
            Some(_) if moved.net != 0 => moved.untracked = true,
            _ => {
                moved.kind = Some(resource.clone());
                moved.net = delta;
            }
        }
    }
}

impl Journal for Slot {
    type Snapshot = Moved;

    fn create_snapshot(&self) -> Moved {
        self.lock().clone()
    }

    fn revert_to_snapshot(&self, snapshot: Moved) {
        let now = std::mem::replace(&mut *self.lock(), snapshot.clone());
        if now.untracked && !snapshot.untracked {
            warn!(
                "transaction rollback keeps slot {}: the transaction moved two kinds of item",
                self.index
            );
            return;
        }
        let Some(kind) = now.kind else {
            return;
        };
        let undo = if snapshot.kind.as_ref() == Some(&kind) {
            now.net - snapshot.net
        } else {
            now.net
        };
        if undo == 0 {
            return;
        }
        let current = self.inventory.get_stack(self.index);
        if !current.is_empty() && !kind.matches(&current) {
            warn!(
                "transaction rollback keeps slot {}: another write replaced its item",
                self.index
            );
            return;
        }
        let max = i64::from(
            self.kind
                .max_stack_size(self.inventory.as_ref())
                .min(kind.max_stack_size().unwrap_or(u32::MAX)),
        );
        let target = i64::from(count(&current)) - undo;
        let restored = target.clamp(0, max);
        if restored != target {
            warn!(
                "transaction rollback of slot {} moves {} of {} items: another write changed the slot",
                self.index,
                (restored - i64::from(count(&current))).abs(),
                undo.abs()
            );
        }
        let restored = u8::try_from(restored).unwrap_or(u8::MAX);
        write_slot(
            self.inventory.as_ref(),
            self.kind,
            self.index,
            kind.to_stack(restored),
        );
    }

    fn on_root_commit(&self, _original_state: Moved) {
        *self.lock() = Moved::default();
        self.inventory.mark_dirty();
    }
}

/// `cookingResetJournal` of `AbstractFurnaceBlockEntity`.
struct CookingReset {
    furnace: Arc<dyn Inventory>,
    needed: Mutex<bool>,
}

impl Journal for CookingReset {
    type Snapshot = bool;

    fn create_snapshot(&self) -> bool {
        *self.needed.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn revert_to_snapshot(&self, snapshot: bool) {
        *self.needed.lock().unwrap_or_else(PoisonError::into_inner) = snapshot;
    }

    fn on_root_commit(&self, _original_state: bool) {
        let needed =
            std::mem::take(&mut *self.needed.lock().unwrap_or_else(PoisonError::into_inner));
        if !needed {
            return;
        }
        let input = self.furnace.get_stack(FURNACE_INPUT);
        // The reset of Pumpkin's furnace `set_stack`.
        let total = if input.is_empty() {
            0
        } else {
            get_cooking_recipe_with_ingredient(input.item, CookingRecipeKind::Smelting)
                .map_or(0, |recipe| {
                    u16::try_from(recipe.cookingtime).unwrap_or(u16::MAX)
                })
        };
        with_furnace!(self.furnace.as_ref(), |furnace| {
            furnace.set_cooking_total_time(total);
            furnace.set_cooking_time_spent(0);
        });
    }
}

/// `cooldownTimeJournal` of `HopperBlockEntity`.
struct HopperCooldown {
    hopper: Arc<dyn Inventory>,
}

impl HopperCooldown {
    fn hopper(&self) -> Option<&HopperBlockEntity> {
        self.hopper.as_any().downcast_ref::<HopperBlockEntity>()
    }
}

impl Journal for HopperCooldown {
    type Snapshot = i32;

    fn create_snapshot(&self) -> i32 {
        self.hopper().map_or(0, HopperBlockEntity::cooldown)
    }

    fn revert_to_snapshot(&self, snapshot: i32) {
        if let Some(hopper) = self.hopper() {
            hopper.set_cooldown(snapshot);
        }
    }
}

/// The journals of `onTransfer`, per container kind.
enum TransferJournal {
    None,
    Furnace(Arc<SnapshotJournal<CookingReset>>),
    Hopper(Arc<SnapshotJournal<HopperCooldown>>),
}

/// `VanillaContainerWrapper`: an item handler over every slot of a Pumpkin inventory.
///
/// One wrapper exists per inventory and container kind while a handle to it lives, as
/// `VanillaContainerWrapper.of` caches it, so the transactions of all lookups share its slot
/// journals.
pub struct InventoryWrapper {
    inventory: Arc<dyn Inventory>,
    kind: ContainerKind,
    slots: Vec<Arc<SnapshotJournal<Slot>>>,
    transfer: TransferJournal,
}

type WrapperCache = Mutex<HashMap<(usize, ContainerKind), Weak<InventoryWrapper>>>;

static WRAPPERS: LazyLock<WrapperCache> = LazyLock::new(WrapperCache::default);

impl InventoryWrapper {
    /// `VanillaContainerWrapper.of`: the wrapper of `inventory`, with no vanilla rule beyond
    /// `Inventory::is_valid_slot_for`.
    #[must_use]
    pub fn of(inventory: Arc<dyn Inventory>) -> Arc<Self> {
        Self::of_kind(inventory, ContainerKind::Plain)
    }

    pub(crate) fn of_kind(inventory: Arc<dyn Inventory>, kind: ContainerKind) -> Arc<Self> {
        let key = (Arc::as_ptr(&inventory).cast::<()>() as usize, kind);
        let mut wrappers = WRAPPERS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(wrapper) = wrappers.get(&key).and_then(Weak::upgrade) {
            return wrapper;
        }
        wrappers.retain(|_, wrapper| wrapper.strong_count() > 0);
        let slots = (0..inventory.size())
            .map(|index| {
                SnapshotJournal::new(Slot {
                    inventory: inventory.clone(),
                    kind,
                    index,
                    moved: Mutex::default(),
                })
            })
            .collect();
        let transfer = match kind {
            ContainerKind::Furnace => {
                TransferJournal::Furnace(SnapshotJournal::new(CookingReset {
                    furnace: inventory.clone(),
                    needed: Mutex::new(false),
                }))
            }
            ContainerKind::Hopper => {
                TransferJournal::Hopper(SnapshotJournal::new(HopperCooldown {
                    hopper: inventory.clone(),
                }))
            }
            _ => TransferJournal::None,
        };
        let wrapper = Arc::new(Self {
            inventory,
            kind,
            slots,
            transfer,
        });
        wrappers.insert(key, Arc::downgrade(&wrapper));
        wrapper
    }

    /// The wrapped inventory.
    #[must_use]
    pub fn inventory(&self) -> &Arc<dyn Inventory> {
        &self.inventory
    }

    /// Claims the journals that a transfer at `index` can change, before reading the slot.
    fn claim(&self, index: usize, transaction: &TransactionContext) -> bool {
        if !self.slots[index].claim(transaction) {
            return false;
        }
        match &self.transfer {
            TransferJournal::Furnace(journal) if index == FURNACE_INPUT => {
                journal.claim(transaction)
            }
            TransferJournal::Hopper(journal) => journal.claim(transaction),
            _ => true,
        }
    }

    /// `Container.onTransfer`: the side effects of a transfer of `change` items at `index`.
    fn on_transfer(&self, index: usize, change: i64, transaction: &TransactionContext) {
        match &self.transfer {
            TransferJournal::Furnace(journal) if index == FURNACE_INPUT => {
                let current = i64::from(count(&self.inventory.get_stack(index)));
                if (current == 0 || current == change) && journal.update_snapshots(transaction) {
                    *journal
                        .needed
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = true;
                }
            }
            TransferJournal::Hopper(journal) if change > 0 => {
                let Some(hopper) = journal.hopper() else {
                    return;
                };
                let was_empty = i64::from(count(&self.inventory.get_stack(index))) == change
                    && (0..self.slots.len())
                        .all(|slot| slot == index || self.inventory.get_stack(slot).is_empty());
                if hopper.cooldown() <= HOPPER_COOLDOWN
                    && was_empty
                    && journal.update_snapshots(transaction)
                {
                    hopper.set_cooldown(HOPPER_COOLDOWN);
                }
            }
            _ => {}
        }
    }

    fn write(
        &self,
        index: usize,
        resource: &ItemResource,
        stack: ItemStack,
        change: i64,
        transaction: &TransactionContext,
    ) -> bool {
        let slot = &self.slots[index];
        if !slot.update_snapshots(transaction) {
            return false;
        }
        slot.record(resource, change);
        write_slot(self.inventory.as_ref(), self.kind, index, stack);
        self.on_transfer(index, change, transaction);
        true
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
        u64::from(count(&self.inventory.get_stack(index)))
    }

    fn get_capacity_as_long(&self, index: usize, resource: &ItemResource) -> u64 {
        if !resource.is_empty() && !self.is_valid(index, resource) {
            return 0;
        }
        let container = self.kind.max_stack_size(self.inventory.as_ref());
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
        if resource.is_empty() || amount == 0 || index >= self.slots.len() {
            return 0;
        }
        if !self.claim(index, transaction) || !self.is_valid(index, resource) {
            return 0;
        }
        let current = self.inventory.get_stack(index);
        if !current.is_empty() && !resource.matches(&current) {
            return 0;
        }
        let have = count(&current);
        let capacity = self.get_capacity_as_int(index, resource);
        let inserted = amount.min(capacity.saturating_sub(have));
        let Ok(new_count) = u8::try_from(have + inserted) else {
            return 0;
        };
        if inserted == 0
            || !self.write(
                index,
                resource,
                resource.to_stack(new_count),
                i64::from(inserted),
                transaction,
            )
        {
            return 0;
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
        if !self.claim(index, transaction) {
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
        if extracted == 0
            || !self.write(
                index,
                resource,
                resource.to_stack(left),
                -i64::from(extracted),
                transaction,
            )
        {
            return 0;
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
        // `canPlaceItemThroughFace` is `canPlaceItem` for every vanilla sided container, which
        // `is_valid` checks.
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
