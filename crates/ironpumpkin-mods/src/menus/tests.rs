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
    layout::{VanillaLayout, vanilla_layout},
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

/// The counts that the vanilla 26.3 menu classes build on the client.
#[test]
fn the_layout_table_has_the_vanilla_counts() {
    let layout = |slots, data_slots| VanillaLayout { slots, data_slots };
    assert_eq!(vanilla_layout(WindowType::Generic9x3), layout(63, 0));
    assert_eq!(vanilla_layout(WindowType::Generic9x6), layout(90, 0));
    assert_eq!(vanilla_layout(WindowType::Crafter3x3), layout(46, 10));
    assert_eq!(vanilla_layout(WindowType::Furnace), layout(39, 4));
    assert_eq!(vanilla_layout(WindowType::BrewingStand), layout(41, 4));
    assert_eq!(vanilla_layout(WindowType::Enchantment), layout(38, 10));
    assert_eq!(vanilla_layout(WindowType::Beacon), layout(37, 3));
    assert_eq!(vanilla_layout(WindowType::Lectern), layout(1, 1));
    assert_eq!(vanilla_layout(WindowType::Smithing), layout(40, 1));
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
