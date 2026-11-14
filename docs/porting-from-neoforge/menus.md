# Porting menus from NeoForge

In NeoForge, a mod opens a container screen with `player.openMenu(provider)`. The `MenuProvider` creates an `AbstractContainerMenu` with slots, data slots and overridden methods: `quickMoveStack`, `stillValid`, `clicked`, `clickMenuButton`, `slotsChanged`, `removed`, and `mayPlace` and `onTake` of its slots.

In IronPumpkin, the `menus` module of `ironpumpkin-mods` has the same parts. A `MenuDefinition` holds the menu type, the title, the slots, the container of the menu and the data slots. A `MenuLogic` implementation holds the overridden methods. `menus::open_menu` opens both as a Pumpkin screen handler.

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Open** | `player.openMenu(provider)` | `menus::open_menu(&player, definition, logic)`. It closes the open menu, fires `InventoryOpenEvent` (`PlayerContainerEvent.Open`) and returns a `Menu`. A handler can cancel the open: this is a Pumpkin addition, because `PlayerContainerEvent.Open` of NeoForge cannot be cancelled. |
| **Extra data** | `openMenu(provider, extraDataWriter)`, `openMenu(provider, pos)`, `IMenuProviderExtension#writeClientSideData` | Waits for M5: the client reads the extra data from `neoforge:advanced_open_screen`. |
| **Menu type** | A vanilla `MenuType`, or a mod `MenuType` with `IMenuTypeExtension.create` | A vanilla `WindowType` only. Mod menu types need `neoforge:advanced_open_screen` and wait for M5. |
| **Layout** | The client builds the slots of the vanilla menu class | The definition must have the slot and data slot counts of the vanilla type (`menus::layout::vanilla_layout`), or `open_menu` fails with `MenuError::Layout`. |
| **Storage menus** | `ChestMenu.threeRows(id, inventory, container)`, `DispenserMenu`, `HopperMenu`, `ShulkerBoxMenu` | `MenuDefinition::vanilla(WindowType::Generic9x3, title, source)` with the vanilla slots and shift-click, and `VanillaMenuLogic` |
| **Slots** | `addSlot(new Slot(container, index, x, y))`, `ResourceHandlerSlot`, `addStandardInventorySlots` | `.slot(SlotDefinition::new(source, index))` and `.player_inventory()`. The client places the slots, so there are no coordinates. |
| **Slot sources** | Any `Container`, or a `ResourceHandler` | `SlotSource::PlayerInventory`, `MenuContainer`, `Container(Arc<dyn Inventory>)` and `Handler(Arc<dyn ResourceHandler<ItemResource>>)` |
| **Slot overrides** | A `Slot` subclass with `mayPlace` or `onTake` | `.with_place_filter()` and `.with_take_callback()` on the slot; the host calls `MenuLogic::slot_may_place` and `slot_on_take` with the menu slot index. |
| **Container of the menu** | A `SimpleContainer` field, `slotsChanged`, `clearContainer` in `removed` | `.with_container(size)`, `MenuLogic::slots_changed`, `MenuAccess::clear_container` |
| **Data slots** | `addDataSlots(ContainerData)`, `SimpleContainerData` | `.data(Arc<dyn ContainerData>)`, `SimpleContainerData`. A value reaches the client at the next broadcast, as 16 bits like vanilla. |
| **Protected methods** | `moveItemStackTo`, `getCarried`, `setData`, `broadcastChanges` | `MenuAccess::move_item_stack_to`, `carried`, `set_data`, `broadcast_changes` |
| **`stillValid` helper** | `AbstractContainerMenu.stillValid(access, player, block)` | `menus::still_valid_at(&player, &pos, &block)` |
| **Open menu** | `player.containerMenu` | `menus::get_open_menu(&player)`, and `Menu::with` to act on it from a block entity tick |
| **Close** | `player.closeContainer()` | `MenuAccess::close()`. Inside a click, a button or a `Menu::with` call the menu closes when the call returns; elsewhere at the next tick. |
| **Double click** | `canTakeItemForPickAll` | `MenuLogic::can_take_item_for_pick_all` |

---

## Code Comparison: a machine with an input slot

### 1. NeoForge implementation (Java)

```java
public class MachineMenu extends AbstractContainerMenu {
    private final Container container;

    public MachineMenu(int id, Inventory inventory, Container container, ContainerData data) {
        super(MenuType.FURNACE, id);
        this.container = container;
        addSlot(new Slot(container, 0, 56, 17));
        addSlot(new Slot(container, 1, 56, 53));
        addSlot(new Slot(container, 2, 116, 35) {
            @Override
            public boolean mayPlace(ItemStack stack) {
                return false;
            }
        });
        addStandardInventorySlots(inventory, 8, 84);
        addDataSlots(data);
    }

    @Override
    public ItemStack quickMoveStack(Player player, int index) {
        Slot slot = slots.get(index);
        if (index < 3 || !slot.hasItem()) {
            return ItemStack.EMPTY;
        }
        ItemStack stack = slot.getItem();
        ItemStack clicked = stack.copy();
        if (!moveItemStackTo(stack, 0, 1, false)) {
            return ItemStack.EMPTY;
        }
        if (stack.isEmpty()) {
            slot.setByPlayer(ItemStack.EMPTY);
        } else {
            slot.setChanged();
        }
        return clicked;
    }

    @Override
    public boolean stillValid(Player player) {
        return container.stillValid(player);
    }
}

player.openMenu(new SimpleMenuProvider(
        (id, inventory, p) -> new MachineMenu(id, inventory, machine, machine.data()),
        Component.literal("Machine")));
```

### 2. IronPumpkin implementation (Rust)

```rust
use std::sync::Arc;

use ironpumpkin_mods::{
    entity::player::Player,
    menus::{
        ContainerData, MenuAccess, MenuDefinition, MenuError, MenuLogic, SlotDefinition,
        SlotSource, open_menu, still_valid_at,
    },
    pumpkin_data::{Block, item_stack::ItemStack, screen::WindowType},
    pumpkin_util::{math::position::BlockPos, text::TextComponent},
};
use pumpkin_inventory::Inventory;

struct MachineMenu {
    pos: BlockPos,
}

impl MenuLogic for MachineMenu {
    fn quick_move_stack(&self, menu: &mut MenuAccess<'_>, _player: &Player, slot: usize) -> ItemStack {
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

    fn still_valid(&self, player: &Player) -> bool {
        still_valid_at(player, &self.pos, &Block::FURNACE)
    }

    fn slot_may_place(&self, slot: usize, _stack: &ItemStack) -> bool {
        slot != 2
    }
}

fn open_machine(
    player: &Arc<Player>,
    pos: BlockPos,
    machine: Arc<dyn Inventory>,
    data: Arc<dyn ContainerData>,
) -> Result<(), MenuError> {
    let source = SlotSource::Container(machine);
    let definition = MenuDefinition::new(WindowType::Furnace, TextComponent::text("Machine"))
        .slot(SlotDefinition::new(source.clone(), 0))
        .slot(SlotDefinition::new(source.clone(), 1))
        .slot(SlotDefinition::new(source, 2).with_place_filter())
        .player_inventory()
        .data(data);
    open_menu(player, definition, Arc::new(MachineMenu { pos }))?;
    Ok(())
}
```

`pumpkin-inventory` is a dependency of the pack crate. The `Inventory` of the machine is the inventory of its block entity.

---

## Rules that differ

- The callbacks take `&self`. A slot callback runs inside a click of the same menu, so keep mutable state behind a `Mutex` or an atomic and release it before a `MenuAccess` call.
- The callbacks run while the host holds the screen handler. Inside a callback, use the `MenuAccess` argument. `Menu::with`, `get_open_menu` and `open_menu` lock the screen handler and deadlock there. The `on_contents_changed` callback of a handler behind a handler slot runs when the click commits, also under that lock, so it must not call them either.
- A click, a button, a close and a `Menu::with` call each run in one root transaction. A handler slot writes through the handler in a nested transaction of it: a take is `extract_at`, a place is `insert_at`, and a set moves only the difference. When the handler moves less than the write needs, the whole action rolls back: the root aborts, the cursor and the other slots get their stacks from before the action, and the client gets the whole menu again. NeoForge `ResourceHandlerSlot` writes through an `IndexModifier` instead.
- `Transaction::open_root` fails inside a callback, because the root of the action is open. To move resources of a handler in a callback, open a nested transaction in `MenuAccess::transaction`: it commits with the action.
- `slots_changed` runs once after the click, the button, the close or the `Menu::with` call that changed the container of the menu, not after each write. Writes inside `slots_changed` do not call it again.
- A definition from `MenuDefinition::vanilla` calls `on_open` and `on_close` of its `Container` source, as the vanilla menus call `startOpen` and `stopOpen`. A custom definition does not.
- A menu over a `Container` does not check the distance to its block. Implement `still_valid` with `still_valid_at`.
