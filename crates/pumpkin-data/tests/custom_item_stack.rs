//! Item stacks that hold a custom item. The content registry is process-wide and freezes once, so
//! this file is its own test binary.
#![expect(
    clippy::expect_used,
    reason = "a test fails on the first missing value"
)]

use std::io::Cursor;
use std::sync::Once;

use pumpkin_data::data_component_impl::IDSetContent;
use pumpkin_data::dynamic::{self, ContentIds, ContentKind, ItemDefinition};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_nbt::Nbt;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::deserializer::NbtReadHelperJava;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::text::hover::HoverEvent;

const RUBY: &str = "test:ruby";
/// Listed in the world's content manifest, but no mod registers it.
const GHOST: &str = "test:ghost";

fn ruby() -> &'static Item {
    static FREEZE: Once = Once::new();
    FREEZE.call_once(|| {
        dynamic::register_item(ItemDefinition {
            name: RUBY.to_string(),
            display: &Item::DIAMOND,
            block: None,
            tags: Vec::new(),
        })
        .expect("register test item");
        dynamic::register_placeholder_item(ItemDefinition {
            name: GHOST.to_string(),
            display: &Item::EMERALD,
            block: None,
            tags: Vec::new(),
        })
        .expect("register placeholder item");
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
    assert!(read.item.id >= Item::COUNT);
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

/// A name in neither the registry nor the content manifest is dropped, as in vanilla.
#[test]
fn unknown_namespaced_item_is_dropped() {
    ruby();
    let mut compound = NbtCompound::new();
    compound.put_string("id", "test:missing".to_string());
    compound.put_int("count", 1);
    assert!(ItemStack::read_item_stack(&compound).is_none());
}

/// A stack of a placeholder item keeps its name and components through a save.
#[test]
fn placeholder_item_stack_round_trips() {
    ruby();
    let ghost = Item::from_registry_key(GHOST).expect("placeholder resolves after the freeze");
    assert!(dynamic::is_placeholder(ContentKind::Item, GHOST));
    assert!(!dynamic::is_placeholder(ContentKind::Item, RUBY));
    assert_eq!(
        ghost.to_java_network_id(pumpkin_data::dynamic::ContentIds::Display),
        Item::EMERALD.id
    );

    let mut stack = ItemStack::new(3, ghost);
    stack.set_custom_data("test", "charge", NbtTag::Int(7));
    let compound = nbt_round_trip(&stack);
    assert_eq!(compound.get_string("id"), Some(GHOST));
    let read = ItemStack::read_item_stack(&compound).expect("placeholder stack reads back");
    assert_eq!(read.item, ghost);
    assert_eq!(read.item_count, 3);
    assert_eq!(read.get_custom_data("test", "charge"), Some(NbtTag::Int(7)));
}

#[test]
fn network_id_of_custom_item_is_its_display_item() {
    let ruby = ruby();
    assert_eq!(
        ruby.to_java_network_id(pumpkin_data::dynamic::ContentIds::Display),
        Item::DIAMOND.id
    );
    assert_eq!(
        Item::DIAMOND.to_java_network_id(pumpkin_data::dynamic::ContentIds::Display),
        Item::DIAMOND.id
    );
    assert_eq!(
        Item::AIR.to_java_network_id(pumpkin_data::dynamic::ContentIds::Display),
        Item::AIR.id
    );
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
    assert_eq!(
        IDSetContent::registry_id(ruby, pumpkin_data::dynamic::ContentIds::Display),
        Item::DIAMOND.id
    );
    assert_eq!(
        IDSetContent::registry_id(&Item::STONE, pumpkin_data::dynamic::ContentIds::Display),
        Item::STONE.id
    );
}

#[test]
fn custom_stack_is_displayed_as_its_display_item_stack() {
    let ruby = ruby();
    let mut custom = ItemStack::new(3, ruby);
    custom.set_custom_data("test", "charge", NbtTag::Int(1));

    let mut shown = ItemStack::new(3, &Item::DIAMOND);
    shown.set_custom_data("test", "charge", NbtTag::Int(1));
    assert!(custom.is_displayed_as(&shown, ContentIds::Display));
    // A `Real` client knows the custom item and sends it, never its display item.
    assert!(!custom.is_displayed_as(&shown, ContentIds::Real));

    shown.item_count = 2;
    assert!(!custom.is_displayed_as(&shown, ContentIds::Display));
    shown.item_count = 3;
    shown.set_custom_data("test", "charge", NbtTag::Int(2));
    assert!(!custom.is_displayed_as(&shown, ContentIds::Display));
    assert!(!custom.is_displayed_as(&ItemStack::new(3, &Item::EMERALD), ContentIds::Display));

    // A vanilla stack is never replaced, even by an equal one.
    let vanilla = ItemStack::new(3, &Item::DIAMOND);
    for ids in [ContentIds::Display, ContentIds::Real] {
        assert!(!vanilla.is_displayed_as(&ItemStack::new(3, &Item::DIAMOND), ids));
    }
}

#[test]
fn placeholder_stack_is_displayed_as_its_display_item_in_both_modes() {
    ruby();
    let ghost = Item::from_registry_key(GHOST).expect("placeholder resolves after the freeze");
    let placeholder = ItemStack::new(3, ghost);
    for ids in [ContentIds::Display, ContentIds::Real] {
        assert!(placeholder.is_displayed_as(&ItemStack::new(3, &Item::EMERALD), ids));
    }
}
