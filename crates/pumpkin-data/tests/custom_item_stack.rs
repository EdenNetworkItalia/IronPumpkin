//! Item stacks that hold a custom item. The content registry is process-wide and freezes once, so
//! this file is its own test binary.
#![expect(
    clippy::expect_used,
    reason = "a test fails on the first missing value"
)]

use std::io::Cursor;
use std::sync::Once;

use pumpkin_data::data_component_impl::IDSetContent;
use pumpkin_data::dynamic::{self, ItemDefinition};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_nbt::Nbt;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::deserializer::NbtReadHelperJava;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::text::hover::HoverEvent;

const RUBY: &str = "test:ruby";

fn ruby() -> &'static Item {
    static FREEZE: Once = Once::new();
    FREEZE.call_once(|| {
        dynamic::register_item(ItemDefinition {
            name: RUBY.to_string(),
            display: &Item::DIAMOND,
            block: None,
        })
        .expect("register test item");
        dynamic::freeze().expect("freeze registry");
    });
    Item::from_registry_key(RUBY).expect("custom item resolves after the freeze")
}

/// Writes the stack, encodes the compound to NBT bytes and decodes it again.
fn nbt_round_trip(stack: &ItemStack) -> NbtCompound {
    let mut compound = NbtCompound::new();
    stack.write_item_stack(&mut compound);
    let bytes = Nbt::new(String::new(), compound).write();
    Nbt::read(&mut NbtReadHelperJava::new(Cursor::new(bytes.as_ref())))
        .expect("written item stack re-reads")
        .root_tag
}

#[test]
fn custom_item_stack_round_trips_by_namespaced_name() {
    let ruby = ruby();
    let mut stack = ItemStack::new(5, ruby);
    stack.set_custom_data("test", "charge", NbtTag::Int(3));

    let compound = nbt_round_trip(&stack);
    assert_eq!(compound.get_string("id"), Some(RUBY));
    assert_eq!(compound.get_int("count"), Some(5));

    let read = ItemStack::read_item_stack(&compound).expect("custom stack reads back");
    assert_eq!(read.item, ruby);
    assert_eq!(read.item.id, Item::COUNT);
    assert_eq!(read.item_count, 5);
    assert_eq!(read.get_custom_data("test", "charge"), Some(NbtTag::Int(3)));
}

#[test]
fn vanilla_item_stack_nbt_is_unchanged() {
    ruby();
    let compound = nbt_round_trip(&ItemStack::new(2, &Item::DIAMOND));
    assert_eq!(compound.get_string("id"), Some("minecraft:diamond"));
    let read = ItemStack::read_item_stack(&compound).expect("vanilla stack reads back");
    assert_eq!(read.item, &Item::DIAMOND);

    // Bare names, as older saves may hold, still resolve to vanilla items.
    let mut bare = NbtCompound::new();
    bare.put_string("id", "diamond".to_string());
    bare.put_int("count", 1);
    assert_eq!(
        ItemStack::read_item_stack(&bare).map(|stack| stack.item),
        Some(&Item::DIAMOND)
    );
}

#[test]
fn unknown_namespaced_item_is_dropped() {
    ruby();
    let mut compound = NbtCompound::new();
    compound.put_string("id", "test:missing".to_string());
    compound.put_int("count", 1);
    assert!(ItemStack::read_item_stack(&compound).is_none());
}

#[test]
fn network_id_of_custom_item_is_its_display_item() {
    let ruby = ruby();
    assert_eq!(ruby.to_java_network_id(), Item::DIAMOND.id);
    assert_eq!(Item::DIAMOND.to_java_network_id(), Item::DIAMOND.id);
    assert_eq!(Item::AIR.to_java_network_id(), Item::AIR.id);
}

#[test]
fn hover_event_and_id_sets_name_the_display_item() {
    let ruby = ruby();
    let diamond = HoverEvent::ShowItem {
        id: "diamond".into(),
        count: Some(2),
    };
    assert_eq!(ruby.show_item_hover(Some(2)), diamond);
    assert_eq!(Item::DIAMOND.show_item_hover(Some(2)), diamond);
    assert_eq!(IDSetContent::registry_id(ruby), Item::DIAMOND.id);
    assert_eq!(IDSetContent::registry_id(&Item::STONE), Item::STONE.id);
}
