#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::sync::Arc;

use pumpkin_data::{item::Item, item_stack::ItemStack, screen::WindowType};
use pumpkin_inventory::{Inventory, SimpleInventory};
use pumpkin_util::text::TextComponent;

use super::{
    ContainerData, MenuDefinition, MenuError, SimpleContainerData, SlotDefinition, SlotSource,
    handler::HandlerInventory,
    layout::{VanillaLayout, storage_size, vanilla_layout},
};
use crate::transfer::{ItemResource, ItemStacksResourceHandler, ResourceHandler, Transaction};

fn title() -> TextComponent {
    TextComponent::text("Test")
}

fn redstone_handler(amount: u32) -> Arc<ItemStacksResourceHandler> {
    let handler = Arc::new(ItemStacksResourceHandler::new(2, 64));
    let mut transaction = Transaction::open_root().unwrap();
    handler.insert_at(
        0,
        &ItemResource::of_item(&Item::REDSTONE),
        amount,
        &mut transaction,
    );
    transaction.commit();
    handler
}

/// The counts that the vanilla 26.3 menu classes build on the client, for every menu type.
#[test]
fn the_layout_table_has_the_vanilla_counts() {
    use WindowType as W;
    let table = [
        (W::Generic9x1, 45, 0, Some(9)),
        (W::Generic9x2, 54, 0, Some(18)),
        (W::Generic9x3, 63, 0, Some(27)),
        (W::Generic9x4, 72, 0, Some(36)),
        (W::Generic9x5, 81, 0, Some(45)),
        (W::Generic9x6, 90, 0, Some(54)),
        (W::Generic3x3, 45, 0, Some(9)),
        (W::Crafter3x3, 46, 10, None),
        (W::Anvil, 39, 1, None),
        (W::Beacon, 37, 3, None),
        (W::BlastFurnace, 39, 4, None),
        (W::BrewingStand, 41, 4, None),
        (W::Crafting, 46, 0, None),
        (W::Enchantment, 38, 10, None),
        (W::Furnace, 39, 4, None),
        (W::Grindstone, 39, 0, None),
        (W::Hopper, 41, 0, Some(5)),
        (W::Lectern, 1, 1, None),
        (W::Loom, 40, 1, None),
        (W::Merchant, 39, 0, None),
        (W::ShulkerBox, 63, 0, Some(27)),
        (W::Smithing, 40, 1, None),
        (W::Smoker, 39, 4, None),
        (W::CartographyTable, 39, 0, None),
        (W::Stonecutter, 38, 1, None),
    ];
    for (window_type, slots, data_slots, storage) in table {
        assert_eq!(
            vanilla_layout(window_type),
            VanillaLayout { slots, data_slots },
            "{window_type:?}"
        );
        assert_eq!(storage_size(window_type), storage, "{window_type:?}");
    }
}

#[test]
fn a_vanilla_definition_has_the_vanilla_layout() {
    for window_type in [
        WindowType::Generic9x1,
        WindowType::Generic9x2,
        WindowType::Generic9x3,
        WindowType::Generic9x4,
        WindowType::Generic9x5,
        WindowType::Generic9x6,
        WindowType::Generic3x3,
        WindowType::Hopper,
        WindowType::ShulkerBox,
    ] {
        let definition =
            MenuDefinition::vanilla(window_type, title(), SlotSource::MenuContainer).unwrap();
        assert_eq!(definition.check(), Ok(()), "{window_type:?}");
    }
    assert_eq!(
        MenuDefinition::vanilla(WindowType::Furnace, title(), SlotSource::MenuContainer).err(),
        Some(MenuError::NotStorageType(WindowType::Furnace))
    );
    assert_eq!(
        MenuDefinition::vanilla(WindowType::Hopper, title(), SlotSource::PlayerInventory).err(),
        Some(MenuError::PlayerInventorySource)
    );
}

#[test]
fn a_slot_past_its_source_fails_the_check() {
    let small: Arc<dyn Inventory> = Arc::new(SimpleInventory::new(5));
    let definition = MenuDefinition::vanilla(
        WindowType::Generic9x1,
        title(),
        SlotSource::Container(small),
    )
    .unwrap();
    assert_eq!(
        definition.check(),
        Err(MenuError::SlotOutOfRange {
            slot: 5,
            index: 5,
            size: 5
        })
    );
    let definition = MenuDefinition::new(WindowType::Lectern, title())
        .slot(SlotDefinition::new(SlotSource::MenuContainer, 0))
        .data(Arc::new(SimpleContainerData::new(1)));
    assert_eq!(
        definition.check(),
        Err(MenuError::SlotOutOfRange {
            slot: 0,
            index: 0,
            size: 0
        })
    );
}

#[test]
fn simple_container_data_holds_its_values() {
    let data = SimpleContainerData::new(2);
    data.set(1, -7);
    data.set(5, 3);
    assert_eq!(
        (data.count(), data.get(0), data.get(1), data.get(5)),
        (2, 0, -7, 0)
    );
}

/// A take from a handler slot extracts and commits; a set moves only the difference.
#[test]
fn a_handler_inventory_moves_through_transactions() {
    let handler = redstone_handler(20);
    let inventory = HandlerInventory(handler.clone());
    assert_eq!(inventory.get_stack(0).item_count, 20);
    let taken = inventory.remove_stack_specific(0, 5);
    assert_eq!((taken.item.id, taken.item_count), (Item::REDSTONE.id, 5));
    assert_eq!(handler.get_amount_as_long(0), 15);
    inventory.set_stack(0, ItemStack::new(30, &Item::REDSTONE));
    assert_eq!(handler.get_amount_as_long(0), 30);
    inventory.set_stack(0, ItemStack::new(1, &Item::GLOWSTONE_DUST));
    assert_eq!(inventory.get_stack(0).item.id, Item::GLOWSTONE_DUST.id);
    inventory.set_stack(0, ItemStack::EMPTY.clone());
    assert!(inventory.is_empty());
}

/// A set that the handler refuses in part rolls back as a whole.
#[test]
fn a_refused_set_rolls_back() {
    let handler = Arc::new(ItemStacksResourceHandler::new(1, 64).with_validator(
        |_, resource: &ItemResource| !resource.matches(&ItemStack::new(1, &Item::DIRT)),
    ));
    let mut transaction = Transaction::open_root().unwrap();
    handler.insert_at(
        0,
        &ItemResource::of_item(&Item::STONE),
        10,
        &mut transaction,
    );
    transaction.commit();
    let inventory = HandlerInventory(handler.clone());
    inventory.set_stack(0, ItemStack::new(4, &Item::DIRT));
    assert_eq!(inventory.get_stack(0).item.id, Item::STONE.id);
    assert_eq!(handler.get_amount_as_long(0), 10);
}
