//! Container menus (`AbstractContainerMenu`, `MenuProvider`, `IPlayerExtension#openMenu`).
//!
//! A mod describes a menu with a [`MenuDefinition`]: a vanilla menu type, a title, the slots over
//! their [`SlotSource`], an optional container of the menu and the data slots. Its
//! [`MenuLogic`] has the overridable methods of `AbstractContainerMenu` and `Slot`. [`open_menu`]
//! turns both into a Pumpkin screen handler and opens it for the player.
//!
//! The vanilla client builds the menu of a vanilla type itself, so a definition must have the
//! slot and data slot counts of that type ([`layout::vanilla_layout`]). Mod menu types need the
//! `neoforge:advanced_open_screen` payload and wait for M5.
//!
//! The callbacks run while the host holds the screen handler lock. Inside a callback, act on the
//! menu through its [`MenuAccess`]. [`Menu::with`], [`get_open_menu`] and [`open_menu`] lock the
//! screen handler, so a callback that calls them deadlocks.
//! The `on_contents_changed` callback of a resource handler behind a [`SlotSource::Handler`] runs
//! when the root transaction of the click commits, still under that lock: it must not call them
//! either.
//!
//! A click, a button, a close and a [`Menu::with`] call each run in one root transaction. A
//! handler slot writes in a nested transaction of it, and a handler that moves less than a write
//! needs rolls the whole action back ([`MenuAccess::transaction`]).

mod handler;
pub mod layout;
mod scope;
#[cfg(test)]
mod tests;

use std::{
    any::Any,
    fmt,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicI32, Ordering},
    },
};

use pumpkin_core::{entity::player::Player, plugin::Deferral};
use pumpkin_data::{Block, item_stack::ItemStack, screen::WindowType};
use pumpkin_inventory::{
    Inventory,
    player::player_inventory::PlayerInventory,
    screen_handler::{InventoryPlayer, ScreenHandlerFactory, SharedScreenHandler},
};
use pumpkin_util::{math::position::BlockPos, text::TextComponent};

use crate::transfer::{ItemResource, ResourceHandler};

use handler::ModMenu;
pub use handler::{MenuAccess, MenuContainer};

/// What a slot reads and writes.
#[derive(Clone)]
pub enum SlotSource {
    /// The inventory of the player that opens the menu: 0 to 8 is the hotbar, 9 to 35 the main
    /// inventory.
    PlayerInventory,
    /// The container of the menu, from [`MenuDefinition::with_container`].
    MenuContainer,
    /// A container, such as the inventory of a container block entity.
    Container(Arc<dyn Inventory>),
    /// A resource handler of a mod (`ResourceHandlerSlot`). A take extracts and a place inserts,
    /// each in a root transaction that commits at once.
    Handler(Arc<dyn ResourceHandler<ItemResource>>),
}

impl fmt::Debug for SlotSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PlayerInventory => "PlayerInventory",
            Self::MenuContainer => "MenuContainer",
            Self::Container(_) => "Container",
            Self::Handler(_) => "Handler",
        })
    }
}

/// One slot of a menu: an index into its source, and the `Slot` methods that the mod overrides.
#[derive(Debug, Clone)]
pub struct SlotDefinition {
    source: SlotSource,
    index: usize,
    place_filter: bool,
    take_callback: bool,
    refuses_shulker_boxes: bool,
}

impl SlotDefinition {
    /// The slot at `index` of `source`.
    #[must_use]
    pub const fn new(source: SlotSource, index: usize) -> Self {
        Self {
            source,
            index,
            place_filter: false,
            take_callback: false,
            refuses_shulker_boxes: false,
        }
    }

    /// The slot overrides `mayPlace`: the host asks [`MenuLogic::slot_may_place`] before an item
    /// enters it.
    #[must_use]
    pub const fn with_place_filter(mut self) -> Self {
        self.place_filter = true;
        self
    }

    /// The slot overrides `onTake`: the host calls [`MenuLogic::slot_on_take`] after the player
    /// takes an item from it.
    #[must_use]
    pub const fn with_take_callback(mut self) -> Self {
        self.take_callback = true;
        self
    }
}

/// The data slots of a menu (`ContainerData`). The host sends a changed value at the next
/// broadcast, as a 16-bit value like vanilla.
pub trait ContainerData: Send + Sync {
    /// `get`.
    fn get(&self, index: usize) -> i32;
    /// `set`.
    fn set(&self, index: usize, value: i32);
    /// `getCount`.
    fn count(&self) -> usize;
}

/// `SimpleContainerData`: data slots that hold their values.
pub struct SimpleContainerData(Box<[AtomicI32]>);

impl SimpleContainerData {
    /// `count` data slots at 0.
    #[must_use]
    pub fn new(count: usize) -> Self {
        Self((0..count).map(|_| AtomicI32::new(0)).collect())
    }
}

impl ContainerData for SimpleContainerData {
    fn get(&self, index: usize) -> i32 {
        self.0
            .get(index)
            .map_or(0, |value| value.load(Ordering::Relaxed))
    }

    fn set(&self, index: usize, value: i32) {
        if let Some(slot) = self.0.get(index) {
            slot.store(value, Ordering::Relaxed);
        }
    }

    fn count(&self) -> usize {
        self.0.len()
    }
}

/// The vanilla input of a click (`ContainerInput`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerInput {
    /// `PICKUP`: a left or right click.
    Pickup,
    /// `QUICK_MOVE`: a shift-click.
    QuickMove,
    /// `SWAP`: a number key or the offhand key; the button is the hotbar slot, 40 the offhand.
    Swap,
    /// `CLONE`: a middle click in creative mode.
    Clone,
    /// `THROW`: the drop key; button 1 throws the stack.
    Throw,
    /// `QUICK_CRAFT`: a drag.
    QuickCraft,
    /// `PICKUP_ALL`: a double click.
    PickupAll,
}

/// The overridable methods of `AbstractContainerMenu` and of its slots. Every method has the
/// vanilla default.
///
/// The methods take `&self`, because a slot callback runs inside a click or a shift-click of the
/// same menu: keep mutable state behind a lock or an atomic, and release it before a
/// [`MenuAccess`] call.
pub trait MenuLogic: Any + Send + Sync {
    /// `clicked`, before the vanilla click handling. Return `true` to skip the vanilla handling,
    /// as an override that does not call `super.clicked` does (a ghost slot).
    fn clicked(
        &self,
        _menu: &mut MenuAccess<'_>,
        _slot: i32,
        _button: i32,
        _input: ContainerInput,
        _player: &Player,
    ) -> bool {
        false
    }

    /// `quickMoveStack`: moves the stack of `slot` on a shift-click and returns the moved stack,
    /// or an empty stack when nothing moved. The default is the vanilla shift-click of a
    /// definition from [`MenuDefinition::vanilla`], and nothing for the others.
    fn quick_move_stack(
        &self,
        menu: &mut MenuAccess<'_>,
        player: &Player,
        slot: usize,
    ) -> ItemStack {
        menu.quick_move_storage(player, slot)
    }

    /// `stillValid`, every tick: `false` closes the menu. See [`still_valid_at`].
    fn still_valid(&self, _player: &Player) -> bool {
        true
    }

    /// `clickMenuButton`: `true` when the button did something, so the host broadcasts the
    /// changes.
    fn click_button(&self, _menu: &mut MenuAccess<'_>, _player: &Player, _button: i32) -> bool {
        false
    }

    /// `slotsChanged`: the container of the menu changed. The host calls it after the click,
    /// the button, the close or the [`Menu::with`] call that changed it.
    fn slots_changed(&self, _menu: &mut MenuAccess<'_>) {}

    /// `canTakeItemForPickAll`: whether a double click with `carried` on the cursor collects from
    /// `slot`. Vanilla menus return `false` for their result slot.
    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, _slot: usize) -> bool {
        true
    }

    /// `Slot#mayPlace` of a slot [`with_place_filter`](SlotDefinition::with_place_filter).
    fn slot_may_place(&self, _slot: usize, _stack: &ItemStack) -> bool {
        true
    }

    /// `Slot#onTake` of a slot [`with_take_callback`](SlotDefinition::with_take_callback).
    fn slot_on_take(&self, _player: &Player, _slot: usize, _stack: &ItemStack) {}

    /// `removed`, when the menu closes, after the host returned the carried stack to the player.
    /// See [`MenuAccess::clear_container`].
    fn removed(&self, _menu: &mut MenuAccess<'_>, _player: &Player) {}
}

/// The vanilla menu logic, for a definition from [`MenuDefinition::vanilla`].
pub struct VanillaMenuLogic;

impl MenuLogic for VanillaMenuLogic {}

/// A menu to open: what `MenuProvider#createMenu` builds, and the display name.
pub struct MenuDefinition {
    window_type: WindowType,
    title: TextComponent,
    slots: Vec<SlotDefinition>,
    container_size: usize,
    data: Option<Arc<dyn ContainerData>>,
    storage: Option<Storage>,
}

/// The vanilla storage menu that a definition copies.
#[derive(Clone)]
struct Storage {
    size: usize,
    /// `DispenserMenu` returns nothing when the shift-click moved nothing and calls `onTake`.
    dispenser: bool,
    /// The container of a definition from [`MenuDefinition::vanilla`] gets `startOpen` and
    /// `stopOpen`.
    container: Option<Arc<dyn Inventory>>,
}

impl MenuDefinition {
    /// A menu of the vanilla type `window_type` with no slots.
    #[must_use]
    pub const fn new(window_type: WindowType, title: TextComponent) -> Self {
        Self {
            window_type,
            title,
            slots: Vec::new(),
            container_size: 0,
            data: None,
            storage: None,
        }
    }

    /// The vanilla storage menu of `window_type` over `source`, as `ChestMenu`, `DispenserMenu`,
    /// `HopperMenu` or `ShulkerBoxMenu` build it: the container slots, the player inventory and
    /// the vanilla shift-click. A shulker box slot refuses shulker boxes. A
    /// [`SlotSource::MenuContainer`] source gets a container of the right size.
    ///
    /// # Errors
    ///
    /// [`MenuError::NotStorageType`] when `window_type` has its own menu logic, and
    /// [`MenuError::PlayerInventorySource`] for the player inventory as the source.
    pub fn vanilla(
        window_type: WindowType,
        title: TextComponent,
        source: SlotSource,
    ) -> Result<Self, MenuError> {
        let size =
            layout::storage_size(window_type).ok_or(MenuError::NotStorageType(window_type))?;
        if matches!(source, SlotSource::PlayerInventory) {
            return Err(MenuError::PlayerInventorySource);
        }
        let mut definition = Self::new(window_type, title);
        if matches!(source, SlotSource::MenuContainer) {
            definition = definition.with_container(size);
        }
        let shulker = window_type == WindowType::ShulkerBox;
        for index in 0..size {
            let mut slot = SlotDefinition::new(source.clone(), index);
            slot.refuses_shulker_boxes = shulker;
            definition = definition.slot(slot);
        }
        definition.storage = Some(Storage {
            size,
            dispenser: window_type == WindowType::Generic3x3,
            container: match source {
                SlotSource::Container(container) => Some(container),
                _ => None,
            },
        });
        Ok(definition.player_inventory())
    }

    /// Gives the menu its own container of `size` slots (a `SimpleContainer` of the menu), for
    /// [`SlotSource::MenuContainer`].
    #[must_use]
    pub const fn with_container(mut self, size: usize) -> Self {
        self.container_size = size;
        self
    }

    /// Adds a slot (`addSlot`).
    #[must_use]
    pub fn slot(mut self, slot: SlotDefinition) -> Self {
        self.slots.push(slot);
        self
    }

    /// Adds the 36 slots of the player inventory in the vanilla order
    /// (`addStandardInventorySlots`): the main inventory, then the hotbar.
    #[must_use]
    pub fn player_inventory(mut self) -> Self {
        for index in (9..36).chain(0..9) {
            self.slots
                .push(SlotDefinition::new(SlotSource::PlayerInventory, index));
        }
        self
    }

    /// Sets the data slots (`addDataSlots`).
    #[must_use]
    pub fn data(mut self, data: Arc<dyn ContainerData>) -> Self {
        self.data = Some(data);
        self
    }

    fn check(&self) -> Result<(), MenuError> {
        let expected = layout::vanilla_layout(self.window_type);
        let data_slots = self.data.as_ref().map_or(0, |data| data.count());
        if self.slots.len() != expected.slots || data_slots != expected.data_slots {
            return Err(MenuError::Layout {
                window_type: self.window_type,
                expected: expected.slots,
                slots: self.slots.len(),
                expected_data: expected.data_slots,
                data_slots,
            });
        }
        for (slot, definition) in self.slots.iter().enumerate() {
            let size = match &definition.source {
                SlotSource::PlayerInventory => layout::PLAYER_INVENTORY_SLOTS,
                SlotSource::MenuContainer => self.container_size,
                SlotSource::Container(container) => container.size(),
                SlotSource::Handler(handler) => handler.size(),
            };
            if definition.index >= size {
                return Err(MenuError::SlotOutOfRange {
                    slot,
                    index: definition.index,
                    size,
                });
            }
        }
        Ok(())
    }
}

/// Why [`open_menu`] or [`MenuDefinition::vanilla`] failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuError {
    /// The definition does not have the slot and data slot counts that the vanilla client builds
    /// for its menu type.
    Layout {
        /// The menu type.
        window_type: WindowType,
        /// The slot count of the vanilla client.
        expected: usize,
        /// The slot count of the definition.
        slots: usize,
        /// The data slot count of the vanilla client.
        expected_data: usize,
        /// The data slot count of the definition.
        data_slots: usize,
    },
    /// A slot index past the end of its source.
    SlotOutOfRange {
        /// The menu slot.
        slot: usize,
        /// The index into the source.
        index: usize,
        /// The size of the source.
        size: usize,
    },
    /// [`MenuDefinition::vanilla`] for a menu type that has its own menu logic.
    NotStorageType(WindowType),
    /// [`MenuDefinition::vanilla`] over the player inventory.
    PlayerInventorySource,
    /// A handler of `InventoryOpenEvent` (`PlayerContainerEvent.Open`) cancelled the open.
    Cancelled,
}

impl fmt::Display for MenuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Layout {
                window_type,
                expected,
                slots,
                expected_data,
                data_slots,
            } => write!(
                f,
                "a {window_type:?} menu has {expected} slots and {expected_data} data slots on \
                 the vanilla client, the definition has {slots} slots and {data_slots} data slots"
            ),
            Self::SlotOutOfRange { slot, index, size } => write!(
                f,
                "slot {slot} reads index {index} of a source with {size} slots"
            ),
            Self::NotStorageType(window_type) => {
                write!(f, "{window_type:?} is not a storage menu type")
            }
            Self::PlayerInventorySource => f.write_str(
                "a vanilla storage menu cannot show the player inventory as its container",
            ),
            Self::Cancelled => f.write_str("the open was cancelled"),
        }
    }
}

impl std::error::Error for MenuError {}

/// An open menu.
#[derive(Clone)]
pub struct Menu {
    handler: Arc<Mutex<ModMenu>>,
    logic: Arc<dyn MenuLogic>,
    sync_id: u8,
}

impl fmt::Debug for Menu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Menu")
            .field("sync_id", &self.sync_id)
            .finish_non_exhaustive()
    }
}

impl Menu {
    /// The container id (`containerId`).
    #[must_use]
    pub const fn sync_id(&self) -> u8 {
        self.sync_id
    }

    /// The menu logic.
    #[must_use]
    pub const fn logic(&self) -> &Arc<dyn MenuLogic> {
        &self.logic
    }

    /// The menu logic as its own type.
    #[must_use]
    pub fn logic_as<T: MenuLogic>(&self) -> Option<Arc<T>> {
        let logic: Arc<dyn Any + Send + Sync> = self.logic.clone();
        logic.downcast().ok()
    }

    /// Runs `action` on the menu from outside a callback, for example from the tick of a block
    /// entity, and calls [`MenuLogic::slots_changed`] when its container changed. A
    /// [`MenuAccess::close`] inside `action` closes the menu after it.
    pub fn with<R>(&self, action: impl FnOnce(&mut MenuAccess<'_>) -> R) -> R {
        let _deferral = Deferral::start();
        let mut menu = self.handler.lock().unwrap_or_else(PoisonError::into_inner);
        menu.run_action(|menu| action(&mut MenuAccess::new(menu)))
    }

    /// Whether the player still has this menu open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        let Some(player) = self
            .handler
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .player()
        else {
            return false;
        };
        std::ptr::addr_eq(
            Arc::as_ptr(&current_handler(&player)),
            Arc::as_ptr(&self.handler),
        )
    }
}

fn current_handler(player: &Player) -> SharedScreenHandler {
    player
        .current_screen_handler
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// The menu that `player` has open, when a mod opened it (`Player#containerMenu`).
#[must_use]
pub fn get_open_menu(player: &Player) -> Option<Menu> {
    let current = current_handler(player);
    let current = current.lock().unwrap_or_else(PoisonError::into_inner);
    current.as_any().downcast_ref::<ModMenu>()?.handle()
}

/// Opens the menu of `definition` for `player` (`IPlayerExtension#openMenu`): closes the open
/// menu, fires `InventoryOpenEvent`, sends the menu and returns its handle.
///
/// # Errors
///
/// [`MenuError::Layout`] or [`MenuError::SlotOutOfRange`] for a definition that the vanilla client
/// cannot show, [`MenuError::Cancelled`] when a handler cancelled the open.
pub fn open_menu(
    player: &Arc<Player>,
    definition: MenuDefinition,
    logic: Arc<dyn MenuLogic>,
) -> Result<Menu, MenuError> {
    definition.check()?;
    let factory = Factory {
        title: definition.title.clone(),
        pending: Mutex::new(Some((definition, logic.clone()))),
        player: Arc::downgrade(player),
        created: Mutex::new(None),
    };
    player
        .open_handled_screen(&factory, None)
        .ok_or(MenuError::Cancelled)?;
    let handler = factory
        .created
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .ok_or(MenuError::Cancelled)?;
    let sync_id = handler
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .sync_id_value();
    Ok(Menu {
        handler,
        logic,
        sync_id,
    })
}

/// The vanilla `stillValid(ContainerLevelAccess, Player, Block)`: the block at `pos` is still
/// `block` and the player is within its block interaction range plus 4.
#[must_use]
pub fn still_valid_at(player: &Player, pos: &BlockPos, block: &Block) -> bool {
    player.world().get_block(pos).id == block.id && player.can_interact_with_block_at(pos, 4.0)
}

type PendingMenu = (MenuDefinition, Arc<dyn MenuLogic>);

/// The `MenuProvider` that [`open_menu`] passes to Pumpkin.
struct Factory {
    title: TextComponent,
    pending: Mutex<Option<PendingMenu>>,
    player: std::sync::Weak<Player>,
    created: Mutex<Option<Arc<Mutex<ModMenu>>>>,
}

impl ScreenHandlerFactory for Factory {
    fn create_screen_handler(
        &self,
        sync_id: u8,
        player_inventory: &Arc<PlayerInventory>,
        player: &dyn InventoryPlayer,
    ) -> Option<SharedScreenHandler> {
        let (definition, logic) = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()?;
        let menu = ModMenu::create(
            sync_id,
            definition,
            &logic,
            player_inventory,
            self.player.clone(),
            player.is_spectator(),
        );
        *self.created.lock().unwrap_or_else(PoisonError::into_inner) = Some(menu.clone());
        Some(menu)
    }

    fn get_display_name(&self) -> TextComponent {
        self.title.clone()
    }
}

fn as_player(player: &dyn InventoryPlayer) -> Option<&Player> {
    player.as_any().downcast_ref::<Player>()
}
