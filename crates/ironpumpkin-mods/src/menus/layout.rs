//! The slot and data slot counts that the vanilla 26.3 client builds for each menu type.
//!
//! The client creates the menu of a vanilla type itself (`MenuType#create`), so it knows only the
//! slots and data slots of the vanilla class. A container content packet with more slots throws
//! in `AbstractContainerMenu.initializeContents`, and a data value for a missing data slot throws
//! in `setData`.

use pumpkin_data::screen::WindowType;

/// The player inventory part of a vanilla menu: 27 main slots, then the 9 hotbar slots.
pub const PLAYER_INVENTORY_SLOTS: usize = 36;

/// The layout of a vanilla menu type on the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaLayout {
    /// All the slots of the menu, the player inventory included.
    pub slots: usize,
    /// The data slots (`addDataSlot` and `addDataSlots`).
    pub data_slots: usize,
}

const fn with_player(container_slots: usize, data_slots: usize) -> VanillaLayout {
    VanillaLayout {
        slots: container_slots + PLAYER_INVENTORY_SLOTS,
        data_slots,
    }
}

/// The client layout of `window_type`, from the vanilla 26.3 menu classes.
#[must_use]
pub const fn vanilla_layout(window_type: WindowType) -> VanillaLayout {
    match window_type {
        WindowType::Generic9x1 | WindowType::Generic3x3 => with_player(9, 0),
        WindowType::Generic9x2 => with_player(18, 0),
        WindowType::Generic9x3 | WindowType::ShulkerBox => with_player(27, 0),
        WindowType::Generic9x4 => with_player(36, 0),
        WindowType::Generic9x5 => with_player(45, 0),
        WindowType::Generic9x6 => with_player(54, 0),
        // `CrafterMenu`: 9 crafter slots, the player inventory, then the result slot.
        WindowType::Crafter3x3 => with_player(10, 10),
        WindowType::Anvil => with_player(3, 1),
        WindowType::Beacon => with_player(1, 3),
        WindowType::BlastFurnace | WindowType::Furnace | WindowType::Smoker => with_player(3, 4),
        WindowType::BrewingStand => with_player(5, 4),
        WindowType::Crafting => with_player(10, 0),
        WindowType::Enchantment => with_player(2, 10),
        WindowType::Grindstone | WindowType::Merchant | WindowType::CartographyTable => {
            with_player(3, 0)
        }
        WindowType::Hopper => with_player(5, 0),
        // `LecternMenu` has no player inventory slots.
        WindowType::Lectern => VanillaLayout {
            slots: 1,
            data_slots: 1,
        },
        WindowType::Loom | WindowType::Smithing => with_player(4, 1),
        WindowType::Stonecutter => with_player(2, 1),
    }
}

/// The container size of a storage-only menu type (`ChestMenu`, `DispenserMenu`, `HopperMenu`,
/// `ShulkerBoxMenu`), or `None` for a type with its own menu logic.
#[must_use]
pub const fn storage_size(window_type: WindowType) -> Option<usize> {
    match window_type {
        WindowType::Generic9x1 | WindowType::Generic3x3 => Some(9),
        WindowType::Generic9x2 => Some(18),
        WindowType::Generic9x3 | WindowType::ShulkerBox => Some(27),
        WindowType::Generic9x4 => Some(36),
        WindowType::Generic9x5 => Some(45),
        WindowType::Generic9x6 => Some(54),
        WindowType::Hopper => Some(5),
        _ => None,
    }
}
