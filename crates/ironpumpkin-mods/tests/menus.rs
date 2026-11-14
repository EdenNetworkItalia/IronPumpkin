//! The scenarios of the spec `neoforge-menus` (`openspec/changes/neoforge-shaped-plugin-api`) on a
//! real server: menus of vanilla menu types over the four slot sources, the menu callbacks, the
//! data slots and the close from a callback.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

use std::{
    marker::PhantomData,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use ironpumpkin_mods::{
    event::{BoxFuture, EventHandler, EventPriority},
    menus::{
        ContainerData, ContainerInput, MenuAccess, MenuDefinition, MenuError, MenuLogic,
        SimpleContainerData, SlotDefinition, SlotSource, VanillaMenuLogic, get_open_menu,
        open_menu,
    },
    pumpkin_core::{
        entity::player::Player,
        net::java::neoforge::NegotiatedState,
        plugin::api::events::{Payload, inventory::inventory_open::InventoryOpenEvent},
        server::Server,
    },
    pumpkin_data::{
        item::Item, item_stack::ItemStack, packet::CURRENT_MC_VERSION, screen::WindowType,
    },
    pumpkin_util::text::TextComponent,
    transfer::{
        EnergyHandler, ItemResource, ItemStacksResourceHandler, ResourceHandler,
        SimpleEnergyHandler, Transaction, TransactionContext,
    },
};
use pumpkin_inventory::{Inventory, SimpleInventory};
use pumpkin_protocol::{
    codec::{item_stack_seralizer::OptionalItemStackHash, var_int::VarInt},
    java::{
        client::play::{COpenScreen, CSetContainerProperty},
        packet_decoder::TCPNetworkDecoder,
        server::play::{SClickSlot, SContainerButtonClick, SlotActionType},
    },
    packet::MultiVersionJavaPacket,
    ser::NetworkReadExt,
};

type Log = Arc<Mutex<Vec<String>>>;

fn push(log: &Log, entry: impl Into<String>) {
    log.lock().unwrap().push(entry.into());
}

fn entries(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

fn title() -> TextComponent {
    TextComponent::text("Machine")
}

fn stack(count: u8, item: &'static Item) -> ItemStack {
    ItemStack::new(count, item)
}

/// Runs `action` on another thread and waits for it: a callback that deadlocks fails the test
/// instead of hanging it.
fn finish_within(player: &Arc<Player>, action: impl FnOnce(&Arc<Player>) + Send + 'static) {
    let (done, finished) = mpsc::channel();
    let player = player.clone();
    std::thread::spawn(move || {
        action(&player);
        let _ = done.send(());
    });
    finished
        .recv_timeout(support::SLOW_RUNNER)
        .unwrap_or_else(|_| panic!("the action did not finish: a callback deadlocked or panicked"));
}

fn click(player: &Arc<Player>, sync_id: u8, slot: i16, button: i8, mode: SlotActionType) {
    let packet = SClickSlot {
        sync_id: VarInt(i32::from(sync_id)),
        revision: VarInt(0),
        slot,
        button,
        mode,
        length_of_array: VarInt(0),
        array_of_changed_slots: Vec::new(),
        carried_item: OptionalItemStackHash(None),
    };
    finish_within(player, move |player| {
        player.on_slot_click(packet, support::server());
    });
}

fn press_button(player: &Arc<Player>, sync_id: u8, button: i32) {
    let packet = SContainerButtonClick {
        window_id: VarInt(i32::from(sync_id)),
        button_id: VarInt(button),
    };
    finish_within(player, move |player| {
        player.on_container_button_click(&packet);
    });
}

fn cursor(player: &Player) -> ItemStack {
    get_open_menu(player).unwrap().with(|menu| menu.carried())
}

/// A blocking handler that runs a closure on the event.
struct Blocking<E, F>(F, PhantomData<fn() -> E>);

impl<E, F> EventHandler<E> for Blocking<E, F>
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&mut E) + Send + Sync,
{
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut E,
    ) -> BoxFuture<'a, ()> {
        (self.0)(event);
        Box::pin(async {})
    }
}

/// A furnace-shaped machine: input 0, fuel 1, output 2 in the menu container, then the player
/// inventory, and the four furnace data slots.
#[derive(Default)]
struct Machine {
    log: Log,
    filter: Mutex<Option<ItemStack>>,
}

const OUTPUT: usize = 2;
const FILTER: usize = 1;

impl MenuLogic for Machine {
    fn clicked(
        &self,
        menu: &mut MenuAccess<'_>,
        slot: i32,
        _button: i32,
        input: ContainerInput,
        _player: &Player,
    ) -> bool {
        if usize::try_from(slot) != Ok(FILTER) || input != ContainerInput::Pickup {
            return false;
        }
        *self.filter.lock().unwrap() = Some(menu.carried().copy_with_count(1));
        push(&self.log, "ghost");
        true
    }

    fn quick_move_stack(
        &self,
        menu: &mut MenuAccess<'_>,
        _player: &Player,
        slot: usize,
    ) -> ItemStack {
        let mut stack = menu.get_item(slot);
        if slot < 3 || stack.is_empty() {
            return ItemStack::EMPTY.clone();
        }
        let clicked = stack.clone();
        if !menu.move_item_stack_to(&mut stack, 0, 1, false) {
            return ItemStack::EMPTY.clone();
        }
        menu.set_item(slot, stack);
        clicked
    }

    fn click_button(&self, menu: &mut MenuAccess<'_>, _player: &Player, button: i32) -> bool {
        push(&self.log, format!("button {button}"));
        menu.set_data(0, button);
        if button == 9 {
            menu.close();
        }
        true
    }

    fn slots_changed(&self, menu: &mut MenuAccess<'_>) {
        push(
            &self.log,
            format!("changed {:?}", menu.get_item(0).item_count),
        );
    }

    fn slot_may_place(&self, slot: usize, _stack: &ItemStack) -> bool {
        slot != OUTPUT
    }

    fn slot_on_take(&self, _player: &Player, slot: usize, stack: &ItemStack) {
        push(&self.log, format!("take {slot} {}", stack.item_count));
    }

    fn removed(&self, menu: &mut MenuAccess<'_>, player: &Player) {
        push(&self.log, "removed");
        menu.clear_container(player);
    }
}

fn machine_definition(data: Arc<dyn ContainerData>) -> MenuDefinition {
    MenuDefinition::new(WindowType::Furnace, title())
        .with_container(3)
        .slot(SlotDefinition::new(SlotSource::MenuContainer, 0))
        .slot(SlotDefinition::new(SlotSource::MenuContainer, 1))
        .slot(
            SlotDefinition::new(SlotSource::MenuContainer, OUTPUT)
                .with_place_filter()
                .with_take_callback(),
        )
        .player_inventory()
        .data(data)
}

fn open_machine(player: &Arc<Player>) -> (Arc<Machine>, Arc<SimpleContainerData>, u8) {
    let machine = Arc::new(Machine::default());
    let data = Arc::new(SimpleContainerData::new(4));
    let menu = open_menu(player, machine_definition(data.clone()), machine.clone()).unwrap();
    (machine, data, menu.sync_id())
}

fn slot_id(slot: usize) -> i16 {
    i16::try_from(slot).unwrap()
}

/// The menu slot of the hotbar slot `hotbar` in a menu with `container` slots before the player
/// inventory.
fn hotbar_slot(container: usize, hotbar: usize) -> i16 {
    i16::try_from(container + 27 + hotbar).unwrap()
}

/// Scenario "A vanilla menu type": a `generic_9x3` menu over a container shows a chest screen and
/// `get_open_menu` returns it.
#[test]
fn a_vanilla_menu_type_opens_a_chest_screen() {
    let _serial = support::serial();
    let (player, peer) = support::player_with_peer("ChestViewer", NegotiatedState::default());
    let mut peer = TCPNetworkDecoder::new(tokio::io::BufReader::new(peer));
    let chest: Arc<dyn Inventory> = Arc::new(SimpleInventory::new(27));
    let definition = MenuDefinition::vanilla(
        WindowType::Generic9x3,
        title(),
        SlotSource::Container(chest),
    )
    .unwrap();
    let menu = open_menu(&player, definition, Arc::new(VanillaMenuLogic)).unwrap();

    let (sync_id, window_type) = support::RUNTIME.block_on(async {
        loop {
            let packet = tokio::time::timeout(Duration::from_secs(10), peer.get_raw_packet())
                .await
                .unwrap()
                .unwrap();
            if packet.id == COpenScreen::to_id(CURRENT_MC_VERSION) {
                let mut body = &packet.payload[..];
                break (body.get_var_int().unwrap().0, body.get_var_int().unwrap().0);
            }
        }
    });
    assert_eq!(sync_id, i32::from(menu.sync_id()));
    assert_eq!(window_type, WindowType::Generic9x3 as i32);
    let open = get_open_menu(&player).unwrap();
    assert_eq!(open.sync_id(), menu.sync_id());
    assert!(open.logic_as::<VanillaMenuLogic>().is_some());
    assert_eq!(open.with(|menu| menu.slot_count()), 63);
}

/// A definition that does not match the vanilla layout of its type fails before anything opens.
#[test]
fn a_layout_mismatch_fails_and_opens_nothing() {
    let _serial = support::serial();
    let player = support::player("LayoutChecker");
    let chest: Arc<dyn Inventory> = Arc::new(SimpleInventory::new(27));
    let mut definition = MenuDefinition::new(WindowType::Generic9x1, title());
    for index in 0..27 {
        definition = definition.slot(SlotDefinition::new(
            SlotSource::Container(chest.clone()),
            index,
        ));
    }
    let error = open_menu(
        &player,
        definition.player_inventory(),
        Arc::new(VanillaMenuLogic),
    )
    .unwrap_err();
    assert_eq!(
        error,
        MenuError::Layout {
            window_type: WindowType::Generic9x1,
            expected: 45,
            slots: 63,
            expected_data: 0,
            data_slots: 0,
        }
    );
    let error = open_menu(
        &player,
        machine_definition(Arc::new(SimpleContainerData::new(3))),
        Arc::new(Machine::default()),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        MenuError::Layout {
            expected_data: 4,
            data_slots: 3,
            ..
        }
    ));
    assert!(get_open_menu(&player).is_none());
}

/// A cancel of `InventoryOpenEvent` (`PlayerContainerEvent.Open`) stops the open.
#[test]
fn a_cancelled_open_event_stops_the_open() {
    let _serial = support::serial();
    let player = support::player("OpenCancelled");
    support::server()
        .plugin_manager
        .register_with::<InventoryOpenEvent, _>(
            Arc::new(Blocking(
                |event: &mut InventoryOpenEvent| {
                    if event.player.gameprofile.name == "OpenCancelled" {
                        event.cancelled = true;
                    }
                },
                PhantomData,
            )),
            EventPriority::Normal,
            true,
            false,
        );
    let error = open_menu(
        &player,
        machine_definition(Arc::new(SimpleContainerData::new(4))),
        Arc::new(Machine::default()),
    )
    .unwrap_err();
    assert_eq!(error, MenuError::Cancelled);
    assert!(get_open_menu(&player).is_none());
}

/// Scenario "Shift-click into the machine": the mod's `quick_move_stack` moves the stack into the
/// input slot, and `slots_changed` runs once for the change of the menu container.
#[test]
fn shift_click_moves_the_stack_into_the_machine() {
    let _serial = support::serial();
    let player = support::player("ShiftClicker");
    player.inventory.set_stack(0, stack(40, &Item::COBBLESTONE));
    let (machine, _, sync_id) = open_machine(&player);
    click(
        &player,
        sync_id,
        hotbar_slot(3, 0),
        0,
        SlotActionType::QuickMove,
    );
    let menu = get_open_menu(&player).unwrap();
    assert_eq!(menu.with(|menu| menu.get_item(0)).item_count, 40);
    assert!(player.inventory.get_stack(0).is_empty());
    assert_eq!(entries(&machine.log), ["changed 40"]);
}

/// Scenario "Ghost slot": `clicked` returns true after it stores the filter, so the cursor keeps
/// its item and the slot stays empty.
#[test]
fn a_ghost_slot_keeps_the_cursor() {
    let _serial = support::serial();
    let player = support::player("GhostClicker");
    let (machine, _, sync_id) = open_machine(&player);
    let menu = get_open_menu(&player).unwrap();
    menu.with(|menu| menu.set_carried(stack(5, &Item::DIAMOND)));
    click(&player, sync_id, slot_id(FILTER), 0, SlotActionType::Pickup);
    assert_eq!(cursor(&player).item_count, 5);
    assert!(menu.with(|menu| menu.get_item(FILTER)).is_empty());
    assert_eq!(
        machine.filter.lock().unwrap().as_ref().unwrap().item.id,
        Item::DIAMOND.id
    );
    assert_eq!(entries(&machine.log), ["ghost"]);
}

/// Scenario "Output slot refuses items": the place filter keeps the item on the cursor, and a
/// take from the slot calls `slot_on_take`.
#[test]
fn the_output_slot_refuses_items_and_reports_takes() {
    let _serial = support::serial();
    let player = support::player("OutputClicker");
    let (machine, _, sync_id) = open_machine(&player);
    let menu = get_open_menu(&player).unwrap();
    menu.with(|menu| menu.set_carried(stack(3, &Item::IRON_INGOT)));
    click(&player, sync_id, slot_id(OUTPUT), 0, SlotActionType::Pickup);
    assert_eq!(cursor(&player).item_count, 3);
    assert!(menu.with(|menu| menu.get_item(OUTPUT)).is_empty());

    menu.with(|menu| {
        menu.set_carried(ItemStack::EMPTY.clone());
        menu.set_item(OUTPUT, stack(2, &Item::IRON_INGOT));
    });
    click(&player, sync_id, slot_id(OUTPUT), 0, SlotActionType::Pickup);
    assert_eq!(cursor(&player).item_count, 2);
    assert!(entries(&machine.log).contains(&"take 2 2".to_owned()));
}

/// A resource handler that records its extracts, and can extract at most `max_extract` per call
/// or refuse every insert.
struct Recording {
    inner: ItemStacksResourceHandler,
    log: Log,
    max_extract: u32,
    refuse_insert: bool,
}

impl Recording {
    fn new(inner: ItemStacksResourceHandler, log: &Log) -> Self {
        Self {
            inner,
            log: log.clone(),
            max_extract: u32::MAX,
            refuse_insert: false,
        }
    }

    /// Fills `index` outside a menu, past the refusals.
    fn fill(&self, index: usize, item: &'static Item, amount: u32) {
        let mut transaction = Transaction::open_root().unwrap();
        self.inner.insert_at(
            index,
            &ItemResource::of_item(item),
            amount,
            &mut transaction,
        );
        transaction.commit();
    }
}

impl ResourceHandler<ItemResource> for Recording {
    fn size(&self) -> usize {
        self.inner.size()
    }

    fn get_resource(&self, index: usize) -> ItemResource {
        self.inner.get_resource(index)
    }

    fn get_amount_as_long(&self, index: usize) -> u64 {
        self.inner.get_amount_as_long(index)
    }

    fn get_capacity_as_long(&self, index: usize, resource: &ItemResource) -> u64 {
        self.inner.get_capacity_as_long(index, resource)
    }

    fn is_valid(&self, index: usize, resource: &ItemResource) -> bool {
        self.inner.is_valid(index, resource)
    }

    fn insert_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32 {
        if self.refuse_insert {
            return 0;
        }
        let inserted = self.inner.insert_at(index, resource, amount, transaction);
        push(&self.log, format!("insert {index} {inserted}"));
        inserted
    }

    fn extract_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32 {
        let extracted =
            self.inner
                .extract_at(index, resource, amount.min(self.max_extract), transaction);
        push(&self.log, format!("extract {index} {extracted}"));
        extracted
    }
}

/// Scenario "A handler slot": a take extracts from the handler in a transaction that commits, and
/// a place inserts into it. The vanilla `generic_9x1` shift-click moves the rest into the player
/// hotbar, from its end.
#[test]
fn a_handler_slot_extracts_and_inserts_in_transactions() {
    let _serial = support::serial();
    let player = support::player("HandlerClicker");
    let log: Log = Arc::default();
    let commits = log.clone();
    let inner = ItemStacksResourceHandler::new(9, 64)
        .with_on_contents_changed(move |index, _| push(&commits, format!("commit {index}")));
    let handler = Arc::new(Recording::new(inner, &log));
    let mut transaction = Transaction::open_root().unwrap();
    handler.insert_at(
        0,
        &ItemResource::of_item(&Item::REDSTONE),
        20,
        &mut transaction,
    );
    transaction.commit();
    log.lock().unwrap().clear();
    let handler_dyn: Arc<dyn ResourceHandler<ItemResource>> = handler.clone();
    let definition = MenuDefinition::vanilla(
        WindowType::Generic9x1,
        title(),
        SlotSource::Handler(handler_dyn),
    )
    .unwrap();
    let menu = open_menu(&player, definition, Arc::new(VanillaMenuLogic)).unwrap();

    // A right click takes half: `mayPickup` simulates one extract and rolls back, then the take
    // extracts 10 and commits.
    click(&player, menu.sync_id(), 0, 1, SlotActionType::Pickup);
    assert_eq!(cursor(&player).item_count, 10);
    assert_eq!(handler.get_amount_as_long(0), 10);
    assert!(entries(&log).contains(&"extract 0 10".to_owned()));
    assert!(entries(&log).contains(&"commit 0".to_owned()));

    log.lock().unwrap().clear();
    click(&player, menu.sync_id(), 1, 0, SlotActionType::Pickup);
    assert!(cursor(&player).is_empty());
    assert_eq!(handler.get_amount_as_long(1), 10);
    assert!(entries(&log).contains(&"insert 1 10".to_owned()));

    click(&player, menu.sync_id(), 0, 0, SlotActionType::QuickMove);
    assert_eq!(handler.get_amount_as_long(0), 0);
    assert_eq!(player.inventory.get_stack(8).item_count, 10);
}

/// The vanilla shulker box menu refuses a shulker box on a shift-click.
#[test]
fn a_shulker_box_menu_refuses_shulker_boxes() {
    let _serial = support::serial();
    let player = support::player("ShulkerClicker");
    player.inventory.set_stack(0, stack(1, &Item::SHULKER_BOX));
    player.inventory.set_stack(1, stack(1, &Item::STONE));
    let definition =
        MenuDefinition::vanilla(WindowType::ShulkerBox, title(), SlotSource::MenuContainer)
            .unwrap();
    let menu = open_menu(&player, definition, Arc::new(VanillaMenuLogic)).unwrap();
    click(
        &player,
        menu.sync_id(),
        hotbar_slot(27, 0),
        0,
        SlotActionType::QuickMove,
    );
    click(
        &player,
        menu.sync_id(),
        hotbar_slot(27, 1),
        0,
        SlotActionType::QuickMove,
    );
    assert_eq!(player.inventory.get_stack(0).item.id, Item::SHULKER_BOX.id);
    assert!(player.inventory.get_stack(1).is_empty());
    assert_eq!(menu.with(|menu| menu.get_item(0)).item.id, Item::STONE.id);
}

/// Data slots reach the client after the broadcast, 16 bits per value as in vanilla.
#[test]
fn data_slots_reach_the_client_as_16_bits() {
    let _serial = support::serial();
    let (player, peer) = support::player_with_peer("DataViewer", NegotiatedState::default());
    let mut peer = TCPNetworkDecoder::new(tokio::io::BufReader::new(peer));
    // The server binds the sync handler at the join, which the support player skips.
    player
        .screen_handler_sync_handler
        .store_player(player.clone());
    let (_, data, sync_id) = open_machine(&player);
    data.set(3, 70_000);
    get_open_menu(&player).unwrap().with(|menu| {
        menu.broadcast_changes();
    });
    let (window, property, value) = support::RUNTIME.block_on(async {
        loop {
            let packet = tokio::time::timeout(Duration::from_secs(10), peer.get_raw_packet())
                .await
                .unwrap()
                .unwrap();
            if packet.id == CSetContainerProperty::to_id(CURRENT_MC_VERSION) {
                let mut body = &packet.payload[..];
                let window = body.get_var_int().unwrap().0;
                let property = body.get_i16_be().unwrap();
                let value = body.get_i16_be().unwrap();
                if property == 3 && value != 0 {
                    break (window, property, value);
                }
            }
        }
    });
    assert_eq!((window, property), (i32::from(sync_id), 3));
    // 70000 truncated to 16 bits.
    assert_eq!(value, 4464);
}

/// A button runs `click_button`, and a close inside it closes the menu when the button packet
/// returns, after `removed` gave the container back to the player. The cleared container calls
/// `slots_changed`, as a vanilla container that notifies its menu does.
#[test]
fn a_close_inside_a_button_closes_at_once() {
    let _serial = support::serial();
    let player = support::player("ButtonPresser");
    let (machine, data, sync_id) = open_machine(&player);
    get_open_menu(&player)
        .unwrap()
        .with(|menu| menu.set_item(0, stack(7, &Item::COAL)));
    press_button(&player, sync_id, 2);
    assert_eq!(data.get(0), 2);
    assert!(get_open_menu(&player).is_some());
    press_button(&player, sync_id, 9);
    assert!(get_open_menu(&player).is_none());
    let log = entries(&machine.log);
    assert_eq!(
        log,
        ["changed 7", "button 2", "button 9", "removed", "changed 0"]
    );
    assert!(
        (0..36).any(|slot| player.inventory.get_stack(slot).item.id == Item::COAL.id),
        "clear_container gives the stack back"
    );
}

/// A close inside `clicked` closes the menu when the click returns.
#[test]
fn a_close_inside_a_click_closes_at_once() {
    struct Closer;
    impl MenuLogic for Closer {
        fn clicked(
            &self,
            menu: &mut MenuAccess<'_>,
            _slot: i32,
            _button: i32,
            _input: ContainerInput,
            _player: &Player,
        ) -> bool {
            menu.close();
            true
        }
    }
    let _serial = support::serial();
    let player = support::player("ClickCloser");
    let definition =
        MenuDefinition::vanilla(WindowType::Hopper, title(), SlotSource::MenuContainer).unwrap();
    let menu = open_menu(&player, definition, Arc::new(Closer)).unwrap();
    click(&player, menu.sync_id(), 0, 0, SlotActionType::Pickup);
    assert!(get_open_menu(&player).is_none());
    assert!(!menu.is_open());
}

/// The items of `item` that the player inventory holds.
fn player_count(player: &Player, item: &Item) -> u32 {
    (0..36)
        .map(|slot| player.inventory.get_stack(slot))
        .filter(|stack| stack.item.id == item.id)
        .map(|stack| u32::from(stack.item_count))
        .sum()
}

fn handler_menu(player: &Arc<Player>, handler: &Arc<Recording>) -> u8 {
    let handler: Arc<dyn ResourceHandler<ItemResource>> = handler.clone();
    let definition = MenuDefinition::vanilla(
        WindowType::Generic9x1,
        title(),
        SlotSource::Handler(handler),
    )
    .unwrap();
    open_menu(player, definition, Arc::new(VanillaMenuLogic))
        .unwrap()
        .sync_id()
}

/// A handler that extracts at most 16 per call refuses the shift-click of its 64 items: the
/// click rolls back, and the player gets nothing.
#[test]
fn a_capped_extract_rolls_the_shift_click_back() {
    let _serial = support::serial();
    let player = support::player("CappedShifter");
    let log: Log = Arc::default();
    let mut handler = Recording::new(ItemStacksResourceHandler::new(9, 64), &log);
    handler.max_extract = 16;
    handler.fill(0, &Item::REDSTONE, 64);
    let handler = Arc::new(handler);
    let sync_id = handler_menu(&player, &handler);
    click(&player, sync_id, 0, 0, SlotActionType::QuickMove);
    assert_eq!(handler.get_amount_as_long(0), 64);
    assert_eq!(player_count(&player, &Item::REDSTONE), 0);
    assert!(cursor(&player).is_empty());
}

/// A handler that refuses every insert keeps the 5 diamonds of a place on the cursor.
#[test]
fn a_refused_place_keeps_the_cursor() {
    let _serial = support::serial();
    let player = support::player("RefusedPlacer");
    let log: Log = Arc::default();
    let mut handler = Recording::new(ItemStacksResourceHandler::new(9, 64), &log);
    handler.refuse_insert = true;
    let handler = Arc::new(handler);
    let sync_id = handler_menu(&player, &handler);
    get_open_menu(&player)
        .unwrap()
        .with(|menu| menu.set_carried(stack(5, &Item::DIAMOND)));
    click(&player, sync_id, 0, 0, SlotActionType::Pickup);
    assert_eq!(cursor(&player).item_count, 5);
    assert_eq!(handler.get_amount_as_long(0), 0);
}

/// A swap of 1 diamond onto 10 redstone in a handler that refuses inserts rolls back both sides.
#[test]
fn a_refused_swap_keeps_both_stacks() {
    let _serial = support::serial();
    let player = support::player("RefusedSwapper");
    let log: Log = Arc::default();
    let mut handler = Recording::new(ItemStacksResourceHandler::new(9, 64), &log);
    handler.refuse_insert = true;
    handler.fill(0, &Item::REDSTONE, 10);
    let handler = Arc::new(handler);
    let sync_id = handler_menu(&player, &handler);
    get_open_menu(&player)
        .unwrap()
        .with(|menu| menu.set_carried(stack(1, &Item::DIAMOND)));
    click(&player, sync_id, 0, 0, SlotActionType::Pickup);
    let carried = cursor(&player);
    assert_eq!((carried.item.id, carried.item_count), (Item::DIAMOND.id, 1));
    assert_eq!(handler.get_amount_as_long(0), 10);
    assert_eq!(
        handler.get_resource(0),
        ItemResource::of_item(&Item::REDSTONE)
    );
}

/// A root of another thread owns index 1: a place there inserts nothing, so the click rolls
/// back.
#[test]
fn a_place_into_an_index_of_another_root_keeps_the_cursor() {
    let _serial = support::serial();
    let player = support::player("ContendedPlacer");
    let log: Log = Arc::default();
    let handler = Arc::new(Recording::new(ItemStacksResourceHandler::new(9, 64), &log));
    let sync_id = handler_menu(&player, &handler);
    let (claimed, wait_claim) = mpsc::channel();
    let (release, wait_release) = mpsc::channel::<()>();
    let other = handler.clone();
    let holder = std::thread::spawn(move || {
        let mut transaction = Transaction::open_root().unwrap();
        other
            .inner
            .insert_at(1, &ItemResource::of_item(&Item::STONE), 1, &mut transaction);
        claimed.send(()).unwrap();
        wait_release.recv().unwrap();
    });
    wait_claim.recv().unwrap();
    get_open_menu(&player)
        .unwrap()
        .with(|menu| menu.set_carried(stack(5, &Item::DIAMOND)));
    click(&player, sync_id, 1, 0, SlotActionType::Pickup);
    release.send(()).unwrap();
    holder.join().unwrap();
    assert_eq!(cursor(&player).item_count, 5);
    assert_eq!(handler.get_amount_as_long(1), 0);
}

/// A callback runs inside the root of the click: its own root does not open, and it moves
/// resources through `MenuAccess::transaction`, which commits with the click.
#[test]
fn a_callback_moves_resources_in_the_click_root() {
    struct OwnTransfer {
        handler: Arc<Recording>,
        log: Log,
    }
    impl MenuLogic for OwnTransfer {
        fn quick_move_stack(
            &self,
            menu: &mut MenuAccess<'_>,
            _player: &Player,
            _slot: usize,
        ) -> ItemStack {
            push(
                &self.log,
                format!("own root {}", Transaction::open_root().is_ok()),
            );
            let redstone = ItemResource::of_item(&Item::REDSTONE);
            let moved = menu
                .transaction(|root| {
                    let mut transaction = Transaction::open(Some(root)).unwrap();
                    let moved = self.handler.extract_at(0, &redstone, 4, &mut transaction);
                    transaction.commit();
                    moved
                })
                .unwrap();
            let hotbar = menu.slot_count() - 9;
            menu.set_item(hotbar, redstone.to_stack(u8::try_from(moved).unwrap()));
            ItemStack::EMPTY.clone()
        }
    }
    let _serial = support::serial();
    let player = support::player("OwnTransfer");
    let log: Log = Arc::default();
    let handler = Arc::new(Recording::new(ItemStacksResourceHandler::new(9, 64), &log));
    handler.fill(0, &Item::REDSTONE, 20);
    let handler_dyn: Arc<dyn ResourceHandler<ItemResource>> = handler.clone();
    let definition = MenuDefinition::vanilla(
        WindowType::Generic9x1,
        title(),
        SlotSource::Handler(handler_dyn),
    )
    .unwrap();
    let logic = Arc::new(OwnTransfer {
        handler: handler.clone(),
        log: log.clone(),
    });
    let menu = open_menu(&player, definition, logic).unwrap();
    click(&player, menu.sync_id(), 0, 0, SlotActionType::QuickMove);
    assert!(entries(&log).contains(&"own root false".to_owned()));
    assert_eq!(handler.get_amount_as_long(0), 16);
    assert_eq!(player.inventory.get_stack(0).item_count, 4);
}

/// One index of cobblestone past the 255 that a stack shows: its amount lives in an energy
/// store, which is transactional.
struct Hoard(SimpleEnergyHandler);

impl ResourceHandler<ItemResource> for Hoard {
    fn size(&self) -> usize {
        1
    }

    fn get_resource(&self, _index: usize) -> ItemResource {
        if self.0.get_amount_as_long() == 0 {
            ItemResource::empty()
        } else {
            ItemResource::of_item(&Item::COBBLESTONE)
        }
    }

    fn get_amount_as_long(&self, _index: usize) -> u64 {
        self.0.get_amount_as_long()
    }

    fn get_capacity_as_long(&self, _index: usize, _resource: &ItemResource) -> u64 {
        self.0.get_capacity_as_long()
    }

    fn is_valid(&self, _index: usize, resource: &ItemResource) -> bool {
        *resource == ItemResource::of_item(&Item::COBBLESTONE)
    }

    fn insert_at(
        &self,
        _index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32 {
        if self.is_valid(0, resource) {
            self.0.insert(amount, transaction)
        } else {
            0
        }
    }

    fn extract_at(
        &self,
        _index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &mut TransactionContext,
    ) -> u32 {
        if self.is_valid(0, resource) {
            self.0.extract(amount, transaction)
        } else {
            0
        }
    }
}

/// A number key swap of an index with 300 items moves the 255 that the slot shows and keeps the
/// rest in the handler.
#[test]
fn a_swap_from_a_large_index_keeps_the_rest() {
    let _serial = support::serial();
    let player = support::player("HoardSwapper");
    let hoard = Arc::new(Hoard(SimpleEnergyHandler::new(1000, u32::MAX, u32::MAX)));
    hoard.0.set(300);
    let source: Arc<dyn ResourceHandler<ItemResource>> = hoard.clone();
    let mut definition = MenuDefinition::new(WindowType::Hopper, title())
        .with_container(4)
        .slot(SlotDefinition::new(SlotSource::Handler(source), 0));
    for index in 0..4 {
        definition = definition.slot(SlotDefinition::new(SlotSource::MenuContainer, index));
    }
    let menu = open_menu(
        &player,
        definition.player_inventory(),
        Arc::new(VanillaMenuLogic),
    )
    .unwrap();
    click(&player, menu.sync_id(), 0, 0, SlotActionType::Swap);
    let moved = player_count(&player, &Item::COBBLESTONE);
    assert!(moved > 0, "the swap moved nothing");
    assert_eq!(u64::from(moved) + hoard.get_amount_as_long(0), 300);
}
