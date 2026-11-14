//! The Pumpkin screen handler of a mod menu, its slots and its container.

use std::{
    any::Any,
    collections::HashMap,
    sync::{
        Arc, Mutex, PoisonError, Weak,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
};

use pumpkin_core::{entity::player::Player, plugin::defer_if_active};
use pumpkin_data::{
    item_stack::ItemStack,
    screen::WindowType,
    tag::{self, Taggable},
};
use pumpkin_inventory::{
    Clearable, Inventory, PropertyDelegate, SimpleInventory,
    player::player_inventory::PlayerInventory,
    screen_handler::{
        InventoryPlayer, ScreenHandler, ScreenHandlerBehaviour, ScreenHandlerListener,
        ScreenProperty,
    },
    slot::Slot,
};
use pumpkin_protocol::java::server::play::SlotActionType;
use tracing::warn;

use super::scope;
use super::{
    ContainerData, ContainerInput, Menu, MenuDefinition, MenuLogic, SlotSource, Storage, as_player,
};
use crate::transfer::{ItemResource, Resource, ResourceHandler, Transaction, TransactionContext};

/// The container of a menu (`SimpleContainer`): a change calls [`MenuLogic::slots_changed`].
pub struct MenuContainer {
    items: SimpleInventory,
    changed: AtomicBool,
}

impl MenuContainer {
    fn new(size: usize) -> Self {
        Self {
            items: SimpleInventory::new(size),
            changed: AtomicBool::new(false),
        }
    }

    fn changed(&self) {
        self.changed.store(true, Ordering::Relaxed);
    }
}

impl Clearable for MenuContainer {
    fn clear(&self) {
        self.items.clear();
        self.changed();
    }
}

impl Inventory for MenuContainer {
    fn size(&self) -> usize {
        self.items.size()
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn get_stack(&self, slot: usize) -> ItemStack {
        self.items.get_stack(slot)
    }

    fn remove_stack(&self, slot: usize) -> ItemStack {
        self.changed();
        self.items.remove_stack(slot)
    }

    fn remove_stack_specific(&self, slot: usize, amount: u8) -> ItemStack {
        self.changed();
        self.items.remove_stack_specific(slot, amount)
    }

    fn set_stack(&self, slot: usize, stack: ItemStack) {
        self.items.set_stack(slot, stack);
        self.changed();
    }

    fn mark_dirty(&self) {
        self.changed();
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A resource handler seen as a Pumpkin inventory, so that a slot over it works with the Pumpkin
/// click code. Every write is a nested transaction of the action root ([`scope`]) that commits
/// only when it moved everything; outside an action it is its own root.
pub(super) struct HandlerInventory(pub(super) Arc<dyn ResourceHandler<ItemResource>>);

impl HandlerInventory {
    fn amount(&self, slot: usize) -> u8 {
        saturate_u8(self.0.get_amount_as_int(slot))
    }
}

impl Clearable for HandlerInventory {
    fn clear(&self) {
        for slot in 0..self.0.size() {
            self.set_stack(slot, ItemStack::EMPTY.clone());
        }
    }
}

impl Inventory for HandlerInventory {
    fn size(&self) -> usize {
        self.0.size()
    }

    fn is_empty(&self) -> bool {
        (0..self.0.size()).all(|slot| self.0.get_amount_as_long(slot) == 0)
    }

    fn get_stack(&self, slot: usize) -> ItemStack {
        self.0.get_resource(slot).to_stack(self.amount(slot))
    }

    fn remove_stack(&self, slot: usize) -> ItemStack {
        self.remove_stack_specific(slot, u8::MAX)
    }

    fn remove_stack_specific(&self, slot: usize, amount: u8) -> ItemStack {
        let resource = self.0.get_resource(slot);
        if resource.is_empty() || amount == 0 {
            return ItemStack::EMPTY.clone();
        }
        let extracted = transact(|transaction| {
            Some(
                self.0
                    .extract_at(slot, &resource, u32::from(amount), transaction),
            )
        });
        resource.to_stack(saturate_u8(extracted.unwrap_or(0)))
    }

    fn set_stack(&self, slot: usize, stack: ItemStack) {
        let old = self.0.get_resource(slot);
        // The amount that `get_stack` showed: the click code computed its moves from it.
        let old_amount = u32::from(self.amount(slot));
        let new = ItemResource::of(&stack);
        let new_amount = if new.is_empty() {
            0
        } else {
            u32::from(stack.item_count)
        };
        let moved = transact(|transaction| {
            let moved = if !old.is_empty() && old == new {
                if new_amount >= old_amount {
                    let diff = new_amount - old_amount;
                    self.0.insert_at(slot, &new, diff, transaction) == diff
                } else {
                    let diff = old_amount - new_amount;
                    self.0.extract_at(slot, &old, diff, transaction) == diff
                }
            } else {
                (old.is_empty()
                    || self.0.extract_at(slot, &old, old_amount, transaction) == old_amount)
                    && (new_amount == 0
                        || self.0.insert_at(slot, &new, new_amount, transaction) == new_amount)
            };
            moved.then_some(())
        });
        if moved.is_none() {
            warn!(
                "a menu slot over a resource handler could not set index {slot} to {} x{}: \
                 the handler refused part of the change, so the menu action rolls back",
                stack.item.registry_key, stack.item_count
            );
        }
    }

    fn is_valid_slot_for(&self, slot: usize, stack: &ItemStack) -> bool {
        self.0.is_valid(slot, &ItemResource::of(stack))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Runs `write` in a nested transaction of the action root, or in its own root outside an
/// action. `None` means refused: nothing changed.
fn transact<R>(write: impl FnOnce(&mut TransactionContext) -> Option<R>) -> Option<R> {
    let mut write = Some(write);
    match scope::write(|transaction| write.take().and_then(|write| write(transaction))) {
        scope::Write::Done(result) => Some(result),
        scope::Write::Refused => None,
        scope::Write::NoScope => {
            let write = write.take()?;
            let Ok(mut transaction) = Transaction::open_root() else {
                warn!(
                    "a menu slot over a resource handler cannot write: a root transaction is open"
                );
                return None;
            };
            let result = write(&mut transaction)?;
            transaction.commit();
            Some(result)
        }
    }
}

/// A slot of a mod menu.
struct MenuSlot {
    inventory: Arc<dyn Inventory>,
    handler: Option<Arc<dyn ResourceHandler<ItemResource>>>,
    index: usize,
    position: usize,
    id: AtomicU8,
    place_filter: bool,
    take_callback: bool,
    refuses_shulker_boxes: bool,
    logic: Arc<dyn MenuLogic>,
}

fn saturate_u8(value: u32) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

impl Slot for MenuSlot {
    fn get_inventory(&self) -> Arc<dyn Inventory> {
        self.inventory.clone()
    }

    fn get_index(&self) -> usize {
        self.index
    }

    fn set_id(&self, id: usize) {
        self.id
            .store(u8::try_from(id).unwrap_or(u8::MAX), Ordering::Relaxed);
    }

    fn on_take_item(&self, player: &dyn InventoryPlayer, stack: &ItemStack) {
        self.mark_dirty();
        if self.take_callback
            && let Some(player) = as_player(player)
        {
            self.logic.slot_on_take(player, self.position, stack);
        }
    }

    fn can_insert(&self, stack: &ItemStack) -> bool {
        if self.refuses_shulker_boxes && stack.item.has_tag(&tag::Item::MINECRAFT_SHULKER_BOXES) {
            return false;
        }
        if let Some(handler) = &self.handler
            && (stack.is_empty() || !handler.is_valid(self.index, &ItemResource::of(stack)))
        {
            return false;
        }
        !self.place_filter || self.logic.slot_may_place(self.position, stack)
    }

    fn mark_dirty(&self) {
        self.inventory.mark_dirty();
    }

    fn get_max_item_count(&self) -> u8 {
        self.handler.as_ref().map_or_else(
            || self.inventory.get_max_count_per_stack(),
            |handler| saturate_u8(handler.get_capacity_as_int(self.index, &ItemResource::empty())),
        )
    }

    fn get_max_item_count_for_stack(&self, stack: &ItemStack) -> u8 {
        self.handler.as_ref().map_or_else(
            || self.get_max_item_count().min(stack.get_max_stack_size()),
            |handler| {
                saturate_u8(handler.get_capacity_as_int(self.index, &ItemResource::of(stack)))
            },
        )
    }

    /// `ResourceHandlerSlot#mayPickup`: a handler slot allows a take when one item extracts in a
    /// transaction that rolls back.
    fn can_take_items(&self, _player: &dyn InventoryPlayer) -> bool {
        let Some(handler) = &self.handler else {
            return true;
        };
        let resource = handler.get_resource(self.index);
        if resource.is_empty() {
            return false;
        }
        let take_one = |transaction: &mut TransactionContext| {
            handler.extract_at(self.index, &resource, 1, transaction) == 1
        };
        match scope::simulate(take_one) {
            scope::Write::Done(takes) => takes,
            scope::Write::Refused => false,
            scope::Write::NoScope => {
                Transaction::open_root().is_ok_and(|mut transaction| take_one(&mut transaction))
            }
        }
    }
}

/// The data slots of a menu as a Pumpkin property delegate.
struct DataDelegate(Arc<dyn ContainerData>);

impl PropertyDelegate for DataDelegate {
    fn get_property(&self, index: i32) -> i32 {
        usize::try_from(index).map_or(0, |index| self.0.get(index))
    }

    fn set_property(&self, index: i32, value: i32) {
        if let Ok(index) = usize::try_from(index) {
            self.0.set(index, value);
        }
    }

    fn get_properties_size(&self) -> i32 {
        i32::try_from(self.0.count()).unwrap_or(i32::MAX)
    }
}

/// Sends a changed data slot, as the Pumpkin furnace and brewing stand screens do. It works around
/// an upstream Pumpkin bug in `pumpkin-inventory/src/screen_handler.rs`:
/// `send_content_updates` runs `update_tracked_properties`, which stores the new value, before
/// `check_property_updates` compares against it, so the sync handler never gets a change. The
/// compare also writes with `Vec::insert` instead of an assignment, so the tracked values shift.
struct DataSlotSync;

impl ScreenHandlerListener for DataSlotSync {
    fn on_property_update(
        &self,
        screen_handler: &ScreenHandlerBehaviour,
        property: u8,
        value: i32,
    ) {
        if let Some(sync_handler) = &screen_handler.sync_handler {
            sync_handler.update_property(screen_handler, i32::from(property), value);
        }
    }
}

/// The stacks that a refused menu action restores.
struct Snapshot {
    cursor: ItemStack,
    slots: Vec<(usize, ItemStack)>,
}

/// The Pumpkin screen handler of a mod menu.
pub(super) struct ModMenu {
    behaviour: ScreenHandlerBehaviour,
    logic: Arc<dyn MenuLogic>,
    this: Weak<Mutex<Self>>,
    player: Weak<Player>,
    container: Option<Arc<MenuContainer>>,
    data: Option<Arc<dyn ContainerData>>,
    storage: Option<Storage>,
    /// The container that got `startOpen`, for `stopOpen` on close.
    opened: Option<Arc<dyn Inventory>>,
    /// The positions of the slots over a resource handler.
    handler_slots: Vec<usize>,
    close_requested: bool,
    closing: bool,
}

impl ModMenu {
    pub(super) fn create(
        sync_id: u8,
        definition: MenuDefinition,
        logic: &Arc<dyn MenuLogic>,
        player_inventory: &Arc<PlayerInventory>,
        player: Weak<Player>,
        spectator: bool,
    ) -> Arc<Mutex<Self>> {
        let container = (definition.container_size > 0)
            .then(|| Arc::new(MenuContainer::new(definition.container_size)));
        let player_inventory: Arc<dyn Inventory> = player_inventory.clone();
        let mut handler_inventories: HashMap<*const (), Arc<dyn Inventory>> = HashMap::new();
        let opened = definition
            .storage
            .as_ref()
            .and_then(|storage| storage.container.clone())
            .filter(|_| !spectator);
        if let Some(container) = &opened {
            container.on_open();
        }
        Arc::new_cyclic(|this| {
            let mut menu = Self {
                behaviour: ScreenHandlerBehaviour::new(sync_id, Some(definition.window_type)),
                logic: logic.clone(),
                this: this.clone(),
                player,
                container: container.clone(),
                data: definition.data.clone(),
                storage: definition.storage.clone(),
                opened,
                handler_slots: Vec::new(),
                close_requested: false,
                closing: false,
            };
            for (position, slot) in definition.slots.into_iter().enumerate() {
                let (inventory, handler): (Arc<dyn Inventory>, _) = match slot.source {
                    SlotSource::PlayerInventory => (player_inventory.clone(), None),
                    // `check` rejects a container slot of a menu without a container.
                    SlotSource::MenuContainer => match &container {
                        Some(container) => (container.clone(), None),
                        None => continue,
                    },
                    SlotSource::Container(inventory) => (inventory, None),
                    SlotSource::Handler(handler) => {
                        let inventory = handler_inventories
                            .entry(Arc::as_ptr(&handler).cast::<()>())
                            .or_insert_with(|| Arc::new(HandlerInventory(handler.clone())))
                            .clone();
                        menu.handler_slots.push(position);
                        (inventory, Some(handler))
                    }
                };
                menu.add_slot(Arc::new(MenuSlot {
                    inventory,
                    handler,
                    index: slot.index,
                    position,
                    id: AtomicU8::new(0),
                    place_filter: slot.place_filter,
                    take_callback: slot.take_callback,
                    refuses_shulker_boxes: slot.refuses_shulker_boxes,
                    logic: logic.clone(),
                }));
            }
            if let Some(data) = &definition.data {
                let delegate: Arc<dyn PropertyDelegate> = Arc::new(DataDelegate(data.clone()));
                for index in 0..data.count() {
                    menu.add_property(ScreenProperty::new(
                        delegate.clone(),
                        u8::try_from(index).unwrap_or(u8::MAX),
                    ));
                }
                menu.add_listener(Arc::new(DataSlotSync));
            }
            Mutex::new(menu)
        })
    }

    pub(super) fn handle(&self) -> Option<Menu> {
        Some(Menu {
            handler: self.this.upgrade()?,
            logic: self.logic.clone(),
            sync_id: self.behaviour.sync_id,
        })
    }

    pub(super) const fn sync_id_value(&self) -> u8 {
        self.behaviour.sync_id
    }

    pub(super) fn player(&self) -> Option<Arc<Player>> {
        self.player.upgrade()
    }

    /// Runs `action` as one menu action ([`scope`]): when a handler refuses a write, the root
    /// rolls back, the cursor and the other slots get their stacks from before the action, and
    /// the client gets the whole menu again. Then calls `slotsChanged` when the container of the
    /// menu changed.
    pub(super) fn run_action<R>(&mut self, action: impl FnOnce(&mut Self) -> R) -> R {
        let guard = scope::enter();
        let snapshot = guard.owns().then(|| self.snapshot());
        let result = action(self);
        if !scope::refused() {
            self.notify_slots_changed();
        }
        if guard.finish()
            && let Some(snapshot) = snapshot
        {
            self.restore(snapshot);
        }
        result
    }

    /// The cursor and the stacks of the slots that are not over a resource handler: the root
    /// transaction rolls back the others.
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            cursor: self
                .behaviour
                .cursor_stack
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
            slots: self
                .behaviour
                .slots
                .iter()
                .enumerate()
                .filter(|(position, _)| !self.handler_slots.contains(position))
                .map(|(position, slot)| (position, slot.get_cloned_stack()))
                .collect(),
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        for (position, stack) in snapshot.slots {
            let slot = &self.behaviour.slots[position];
            slot.get_inventory().set_stack(slot.get_index(), stack);
        }
        *self
            .behaviour
            .cursor_stack
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = snapshot.cursor;
        if let Some(container) = &self.container {
            container.changed.store(false, Ordering::Relaxed);
        }
        if !self.closing {
            self.sync_state();
        }
    }

    /// Calls `slotsChanged` once when the container of the menu changed. The writes of the
    /// callback itself do not call it again.
    pub(super) fn notify_slots_changed(&mut self) {
        let Some(container) = self.container.clone() else {
            return;
        };
        if container.changed.swap(false, Ordering::Relaxed) {
            let logic = self.logic.clone();
            logic.slots_changed(&mut MenuAccess::new(self));
            container.changed.store(false, Ordering::Relaxed);
        }
    }
}

impl ScreenHandler for ModMenu {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn get_behaviour(&self) -> &ScreenHandlerBehaviour {
        &self.behaviour
    }

    fn get_behaviour_mut(&mut self) -> &mut ScreenHandlerBehaviour {
        &mut self.behaviour
    }

    fn can_use(&self, player: &dyn InventoryPlayer) -> bool {
        !self.close_requested
            && as_player(player).is_none_or(|player| self.logic.still_valid(player))
    }

    fn on_closed(&mut self, player: &dyn InventoryPlayer) {
        // The tick close and a deferred close can both reach a menu.
        if self.closing {
            return;
        }
        self.closing = true;
        self.default_on_closed(player);
        if let Some(player) = as_player(player) {
            let logic = self.logic.clone();
            self.run_action(|menu| logic.removed(&mut MenuAccess::new(menu), player));
        }
        if let Some(container) = self.opened.take()
            && !player.is_spectator()
        {
            container.on_close();
        }
    }

    fn quick_move(&mut self, player: &dyn InventoryPlayer, slot_index: i32) -> ItemStack {
        let (Some(player), Ok(slot)) = (as_player(player), usize::try_from(slot_index)) else {
            return ItemStack::EMPTY.clone();
        };
        // A refused write ends the shift-click loop of the click code.
        if slot >= self.behaviour.slots.len() || scope::refused() {
            return ItemStack::EMPTY.clone();
        }
        let logic = self.logic.clone();
        logic.quick_move_stack(&mut MenuAccess::new(self), player, slot)
    }

    fn can_take_item_for_pick_all(&self, carried: &ItemStack, slot_index: usize) -> bool {
        self.logic.can_take_item_for_pick_all(carried, slot_index)
    }

    fn on_button_click(&mut self, player: &dyn InventoryPlayer, button_id: i32) -> bool {
        let Some(player) = as_player(player) else {
            return false;
        };
        let logic = self.logic.clone();
        let clicked = self.run_action(|menu| {
            logic.click_button(&mut MenuAccess::new(menu), player, button_id) && !scope::refused()
        });
        if clicked {
            self.send_content_updates();
        }
        clicked
    }

    fn on_slot_click(
        &mut self,
        slot_index: i32,
        button: i32,
        action_type: SlotActionType,
        player: &dyn InventoryPlayer,
    ) {
        self.run_action(|menu| {
            if let Some(mc_player) = as_player(player) {
                let logic = menu.logic.clone();
                let input = container_input(&action_type);
                if logic.clicked(
                    &mut MenuAccess::new(menu),
                    slot_index,
                    button,
                    input,
                    mc_player,
                ) {
                    return;
                }
            }
            menu.internal_on_slot_click(slot_index, button, action_type, player);
        });
    }
}

const fn container_input(action: &SlotActionType) -> ContainerInput {
    match action {
        SlotActionType::Pickup => ContainerInput::Pickup,
        SlotActionType::QuickMove => ContainerInput::QuickMove,
        SlotActionType::Swap => ContainerInput::Swap,
        SlotActionType::Clone => ContainerInput::Clone,
        SlotActionType::Throw => ContainerInput::Throw,
        SlotActionType::QuickCraft => ContainerInput::QuickCraft,
        SlotActionType::PickupAll => ContainerInput::PickupAll,
    }
}

/// The menu inside a callback or a [`Menu::with`] call: the protected methods of
/// `AbstractContainerMenu`.
pub struct MenuAccess<'a> {
    menu: &'a mut ModMenu,
}

impl<'a> MenuAccess<'a> {
    pub(super) const fn new(menu: &'a mut ModMenu) -> Self {
        Self { menu }
    }

    /// The container id (`containerId`).
    #[must_use]
    pub const fn sync_id(&self) -> u8 {
        self.menu.behaviour.sync_id
    }

    /// The menu type.
    #[must_use]
    pub const fn window_type(&self) -> Option<WindowType> {
        self.menu.behaviour.window_type
    }

    /// The number of slots.
    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.menu.behaviour.slots.len()
    }

    /// The stack in `slot` (`getSlot(slot).getItem()`), empty for a slot that does not exist.
    #[must_use]
    pub fn get_item(&self, slot: usize) -> ItemStack {
        self.menu
            .behaviour
            .slots
            .get(slot)
            .map_or_else(|| ItemStack::EMPTY.clone(), |slot| slot.get_stack())
    }

    /// Writes `stack` into `slot` (`getSlot(slot).set(stack)`).
    pub fn set_item(&mut self, slot: usize, stack: ItemStack) {
        if let Some(slot) = self.menu.behaviour.slots.get(slot).cloned() {
            slot.set_stack(stack);
        }
    }

    /// `moveItemStackTo`: moves `stack` into the slots from `start` to `end` (exclusive), first
    /// onto equal stacks, then into empty slots that accept it, from the end when `backwards`.
    /// Returns whether anything moved. `stack` keeps the rest.
    pub fn move_item_stack_to(
        &mut self,
        stack: &mut ItemStack,
        start: usize,
        end: usize,
        backwards: bool,
    ) -> bool {
        let end = end.min(self.slot_count());
        if start >= end {
            return false;
        }
        let (Ok(start), Ok(end)) = (i32::try_from(start), i32::try_from(end)) else {
            return false;
        };
        self.menu.insert_item(stack, start, end, backwards)
    }

    /// The stack on the cursor (`getCarried`).
    #[must_use]
    pub fn carried(&self) -> ItemStack {
        self.menu
            .behaviour
            .cursor_stack
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Sets the stack on the cursor (`setCarried`).
    pub fn set_carried(&mut self, stack: ItemStack) {
        *self
            .menu
            .behaviour
            .cursor_stack
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = stack;
    }

    /// The value of data slot `index`, 0 for a data slot that does not exist.
    #[must_use]
    pub fn data(&self, index: usize) -> i32 {
        self.menu.data.as_ref().map_or(0, |data| data.get(index))
    }

    /// Sets data slot `index` (`setData`). The client gets it at the next broadcast.
    pub fn set_data(&mut self, index: usize, value: i32) {
        if let Some(data) = &self.menu.data {
            data.set(index, value);
        }
    }

    /// The container of the menu, from [`MenuDefinition::with_container`].
    #[must_use]
    pub const fn container(&self) -> Option<&Arc<MenuContainer>> {
        self.menu.container.as_ref()
    }

    /// `clearContainer` for the container of the menu: gives every stack back to the player or
    /// drops it. Call it in [`MenuLogic::removed`] for a container that keeps no items.
    pub fn clear_container(&mut self, player: &Player) {
        if let Some(container) = self.menu.container.clone() {
            self.menu.drop_inventory(player, container);
        }
    }

    /// The vanilla shift-click of a definition from [`MenuDefinition::vanilla`]
    /// (`ChestMenu#quickMoveStack` and the others): from the container to the player inventory,
    /// from the end, or from the player inventory into the container. Moves nothing for another
    /// definition.
    pub fn quick_move_storage(&mut self, player: &Player, slot: usize) -> ItemStack {
        let Some(storage) = self.menu.storage.as_ref() else {
            return ItemStack::EMPTY.clone();
        };
        let (size, dispenser) = (storage.size, storage.dispenser);
        let Some(source) = self.menu.behaviour.slots.get(slot).cloned() else {
            return ItemStack::EMPTY.clone();
        };
        if !source.has_stack() {
            return ItemStack::EMPTY.clone();
        }
        let mut stack = source.get_stack();
        let clicked = stack.clone();
        let moved = if slot < size {
            self.move_item_stack_to(&mut stack, size, self.slot_count(), true)
        } else {
            self.move_item_stack_to(&mut stack, 0, size, false)
        };
        if !moved {
            return ItemStack::EMPTY.clone();
        }
        source.set_stack(if stack.is_empty() {
            ItemStack::EMPTY.clone()
        } else {
            stack.clone()
        });
        if dispenser {
            if stack.item_count == clicked.item_count {
                return ItemStack::EMPTY.clone();
            }
            source.on_take_item(player, &stack);
        }
        clicked
    }

    /// Runs `action` with the root transaction of this click, button, close or [`Menu::with`]
    /// call, for a callback that moves resources of a handler itself: open a nested
    /// transaction on it with `Transaction::open(Some(root))`. The root commits with the action,
    /// or rolls back when a slot write of the action was refused. `Transaction::open_root` fails
    /// inside a callback, because this root is open. Returns `None` when the thread had a root
    /// transaction before the action.
    ///
    /// [`Menu::with`]: super::Menu::with
    pub fn transaction<R>(
        &mut self,
        action: impl FnOnce(&mut TransactionContext) -> R,
    ) -> Option<R> {
        scope::with_root(action)
    }

    /// `broadcastChanges`: sends the changed slots, the cursor and the data slots.
    pub fn broadcast_changes(&mut self) {
        self.menu.send_content_updates();
    }

    /// `Player#closeContainer`. Inside a click, a button or a [`Menu::with`] call the menu closes
    /// when the host released it, before the next packet; elsewhere, for example in
    /// [`MenuLogic::slots_changed`] after a write from another menu, at the next tick.
    ///
    /// [`Menu::with`]: super::Menu::with
    pub fn close(&mut self) {
        if self.menu.closing || self.menu.close_requested {
            return;
        }
        self.menu.close_requested = true;
        let (player, sync_id) = (self.menu.player.clone(), self.sync_id());
        defer_if_active(move || {
            let Some(player) = player.upgrade() else {
                return;
            };
            let current = player
                .current_screen_handler
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            let open = current
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .sync_id();
            if open == sync_id {
                player.close_handled_screen();
            }
        });
    }
}
