//! Clicks of `ScreenHandler::internal_on_slot_click` against vanilla
//! `AbstractContainerMenu#doClick` (26.3).
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails on the first missing value"
)]

use std::{
    any::Any,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

use pumpkin_data::{item::Item, item_stack::ItemStack, screen::WindowType};
use pumpkin_inventory::{
    entity_equipment::EntityEquipment,
    inventory::{Inventory, SimpleInventory},
    player::player_inventory::PlayerInventory,
    screen_handler::{InventoryPlayer, ScreenHandler, ScreenHandlerBehaviour},
    slot::Slot,
};
use pumpkin_protocol::java::{
    client::play::{
        CSetContainerContent, CSetContainerProperty, CSetContainerSlot, CSetCursorItem,
        CSetPlayerInventory, CSetSelectedSlot,
    },
    server::play::SlotActionType,
};

struct TestPlayer {
    inventory: Arc<PlayerInventory>,
    drops: Mutex<Vec<ItemStack>>,
}

impl TestPlayer {
    fn new() -> Self {
        Self {
            inventory: Arc::new(PlayerInventory::new(
                Arc::new(Mutex::new(EntityEquipment::new())),
                Arc::new(rustc_hash::FxHashMap::default()),
            )),
            drops: Mutex::new(Vec::new()),
        }
    }
}

impl InventoryPlayer for TestPlayer {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn drop_item(&self, item: ItemStack, _retain_ownership: bool) {
        self.drops.lock().unwrap().push(item);
    }

    fn get_inventory(&self) -> Arc<PlayerInventory> {
        self.inventory.clone()
    }

    fn has_infinite_materials(&self) -> bool {
        false
    }

    fn is_creative(&self) -> bool {
        false
    }

    fn experience_level(&self) -> i32 {
        0
    }

    fn add_experience_levels(&self, _levels: i32) {}

    fn enchantment_seed(&self) -> i32 {
        0
    }

    fn set_enchantment_seed(&self, _seed: i32) {}

    fn enqueue_inventory_packet(
        &self,
        _packet: &CSetContainerContent,
        _window_type: Option<WindowType>,
    ) {
    }

    fn enqueue_slot_packet(
        &self,
        _packet: &CSetContainerSlot,
        _window_type: Option<WindowType>,
        _total_slots: usize,
    ) {
    }

    fn enqueue_cursor_packet(&self, _packet: &CSetCursorItem) {}

    fn enqueue_property_packet(
        &self,
        _packet: &CSetContainerProperty,
        _window_type: Option<WindowType>,
    ) {
    }

    fn enqueue_slot_set_packet(&self, _packet: &CSetPlayerInventory) {}

    fn enqueue_set_held_item_packet(&self, _packet: &CSetSelectedSlot) {}

    fn enqueue_equipment_change(
        &self,
        _slot: &pumpkin_data::data_component_impl::EquipmentSlot,
        _stack: &ItemStack,
    ) {
    }

    fn award_experience(&self, _amount: i32) {}

    fn increment_stat(
        &self,
        _category: pumpkin_data::statistic::StatisticCategory,
        _stat_id: i32,
        _amount: i32,
    ) {
    }

    fn play_block_sound(&self, _sound: pumpkin_data::sound::Sound, _pitch: f32) {}
}

/// A slot that counts `onTake` and can refuse every take (`mayPickup` false).
struct TestSlot {
    inventory: Arc<dyn Inventory>,
    index: usize,
    id: AtomicU8,
    refuses_take: AtomicBool,
    takes: AtomicUsize,
}

impl TestSlot {
    fn new(inventory: Arc<dyn Inventory>, index: usize) -> Arc<Self> {
        Arc::new(Self {
            inventory,
            index,
            id: AtomicU8::new(0),
            refuses_take: AtomicBool::new(false),
            takes: AtomicUsize::new(0),
        })
    }
}

impl Slot for TestSlot {
    fn get_inventory(&self) -> Arc<dyn Inventory> {
        self.inventory.clone()
    }

    fn get_index(&self) -> usize {
        self.index
    }

    fn set_id(&self, id: usize) {
        self.id.store(u8::try_from(id).unwrap(), Ordering::Relaxed);
    }

    fn on_take_item(&self, _player: &dyn InventoryPlayer, _stack: &ItemStack) {
        self.takes.fetch_add(1, Ordering::Relaxed);
        self.mark_dirty();
    }

    fn mark_dirty(&self) {
        self.inventory.mark_dirty();
    }

    fn can_take_items(&self, _player: &dyn InventoryPlayer) -> bool {
        !self.refuses_take.load(Ordering::Relaxed)
    }
}

struct TestHandler {
    behaviour: ScreenHandlerBehaviour,
    refused_pick_all: Option<usize>,
}

impl TestHandler {
    fn new(slots: &[Arc<TestSlot>]) -> Self {
        let mut handler = Self {
            behaviour: ScreenHandlerBehaviour::new(1, Some(WindowType::Generic9x1)),
            refused_pick_all: None,
        };
        for slot in slots {
            handler.add_slot(slot.clone());
        }
        handler
    }
}

impl ScreenHandler for TestHandler {
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

    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, slot_index: usize) -> bool {
        self.refused_pick_all != Some(slot_index)
    }

    fn quick_move(&mut self, _player: &dyn InventoryPlayer, _slot_index: i32) -> ItemStack {
        ItemStack::EMPTY.clone()
    }
}

fn slots(count: usize) -> (Arc<SimpleInventory>, Vec<Arc<TestSlot>>) {
    let inventory = Arc::new(SimpleInventory::new(count));
    let slots = (0..count)
        .map(|index| TestSlot::new(inventory.clone(), index))
        .collect();
    (inventory, slots)
}

/// A throw-all from a slot that refuses the take ends: vanilla stops when `safeTake` returns an
/// empty stack.
#[test]
fn throw_all_from_a_slot_that_refuses_the_take_ends() {
    let (inventory, slots) = slots(1);
    inventory.set_stack(0, ItemStack::new(10, &Item::STONE));
    slots[0].refuses_take.store(true, Ordering::Relaxed);
    let (done, finished) = mpsc::channel();
    std::thread::spawn(move || {
        let player = TestPlayer::new();
        let mut handler = TestHandler::new(&slots);
        handler.internal_on_slot_click(0, 1, SlotActionType::Throw, &player);
        let _ = done.send(player.drops.lock().unwrap().len());
    });
    let drops = finished
        .recv_timeout(Duration::from_secs(10))
        .expect("the throw-all loop did not end");
    assert_eq!(drops, 0);
    assert_eq!(inventory.get_stack(0).item_count, 10);
}

/// One drop key press takes one item and runs `onTake` once, in `safeTake`.
#[test]
fn a_single_throw_runs_on_take_once() {
    let (inventory, slots) = slots(1);
    inventory.set_stack(0, ItemStack::new(10, &Item::STONE));
    let player = TestPlayer::new();
    let mut handler = TestHandler::new(&slots);
    handler.internal_on_slot_click(0, 0, SlotActionType::Throw, &player);
    assert_eq!(slots[0].takes.load(Ordering::Relaxed), 1);
    assert_eq!(inventory.get_stack(0).item_count, 9);
    assert_eq!(player.drops.lock().unwrap()[0].item_count, 1);
}

fn pick_all(
    stacks: &[ItemStack],
    carried: ItemStack,
    clicked: i32,
    button: i32,
    refused: Option<usize>,
) -> (Vec<u8>, u8) {
    let (inventory, slots) = slots(stacks.len());
    for (index, stack) in stacks.iter().enumerate() {
        inventory.set_stack(index, stack.clone());
    }
    let player = TestPlayer::new();
    let mut handler = TestHandler::new(&slots);
    handler.refused_pick_all = refused;
    *handler.behaviour.cursor_stack.lock().unwrap() = carried;
    handler.internal_on_slot_click(clicked, button, SlotActionType::PickupAll, &player);
    let counts = (0..stacks.len())
        .map(|index| inventory.get_stack(index).item_count)
        .collect();
    let cursor = handler.behaviour.cursor_stack.lock().unwrap().item_count;
    (counts, cursor)
}

fn stone(count: u8) -> ItemStack {
    ItemStack::new(count, &Item::STONE)
}

fn pearls(count: u8) -> ItemStack {
    ItemStack::new(count, &Item::ENDER_PEARL)
}

/// The first pass skips full stacks, so a double click empties the partial stacks first.
#[test]
fn pick_all_takes_partial_stacks_before_full_ones() {
    let stacks = [ItemStack::EMPTY.clone(), stone(64), stone(10)];
    assert_eq!(
        pick_all(&stacks, stone(1), 0, 0, None),
        (vec![0, 11, 0], 64)
    );
}

/// Button 1 collects from the end.
#[test]
fn pick_all_with_button_1_collects_from_the_end() {
    let stacks = [ItemStack::EMPTY.clone(), pearls(5), pearls(5)];
    assert_eq!(
        pick_all(&stacks, pearls(10), 0, 1, None),
        (vec![0, 4, 0], 16)
    );
    assert_eq!(
        pick_all(&stacks, pearls(10), 0, 0, None),
        (vec![0, 0, 4], 16)
    );
}

/// A double click on a slot with an item that the player can take collects nothing.
#[test]
fn pick_all_on_a_takeable_stack_collects_nothing() {
    let stacks = [stone(3), stone(10)];
    assert_eq!(pick_all(&stacks, stone(1), 0, 0, None), (vec![3, 10], 1));
}

/// `canTakeItemForPickAll` excludes a slot, as vanilla menus exclude their result slot.
#[test]
fn pick_all_skips_a_slot_that_the_menu_excludes() {
    let stacks = [ItemStack::EMPTY.clone(), stone(10), stone(10)];
    assert_eq!(
        pick_all(&stacks, stone(1), 0, 0, Some(1)),
        (vec![0, 10, 0], 11)
    );
}
