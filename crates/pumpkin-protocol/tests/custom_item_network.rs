//! A custom item reaches vanilla clients as its display item. The content registry is
//! process-wide and freezes once, so this file is its own test binary.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails on the first missing value"
)]

use std::borrow::Cow;
use std::io::Cursor;
use std::sync::Once;

use pumpkin_data::dynamic::{self, ItemDefinition};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_protocol::codec::item_stack_seralizer::{
    ItemStackSerializer, ItemStackTemplateSerializer, OptionalItemStackHash,
};
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::version::JavaMinecraftVersion;

fn ruby() -> &'static Item {
    static FREEZE: Once = Once::new();
    FREEZE.call_once(|| {
        dynamic::register_item(ItemDefinition {
            name: "test:ruby".to_string(),
            display: &Item::DIAMOND,
            block: None,
        })
        .expect("register test item");
        dynamic::freeze().expect("freeze registry");
    });
    Item::from_registry_key("test:ruby").expect("custom item resolves after the freeze")
}

fn stack(item: &'static Item) -> ItemStack {
    let mut stack = ItemStack::new(3, item);
    stack.set_custom_data("test", "charge", NbtTag::Int(7));
    stack
}

/// Every item stack encoding a packet can use, concatenated.
fn encodings(stack: ItemStack) -> Vec<u8> {
    let version = JavaMinecraftVersion::V_26_3;
    let serializer = ItemStackSerializer(Cow::Owned(stack.clone()));
    let mut out = Vec::new();
    serializer.write(&mut out).unwrap();
    serializer
        .write_length_prefixed_with_version(&mut out, &version)
        .unwrap();
    serializer
        .write_item_cost_with_version(&mut out, &version)
        .unwrap();
    serializer
        .write_template_with_version(&mut out, &version)
        .unwrap();
    ItemStackTemplateSerializer::from(stack)
        .write(&mut out)
        .unwrap();
    out
}

fn var_int(value: i32) -> Vec<u8> {
    let mut out = Vec::new();
    VarInt(value).encode(&mut out).unwrap();
    out
}

#[test]
fn custom_item_stack_encodes_as_its_display_item() {
    let ruby = ruby();
    assert_eq!(encodings(stack(ruby)), encodings(stack(&Item::DIAMOND)));
}

#[test]
fn vanilla_item_stack_bytes_are_unchanged() {
    ruby();
    let mut out = Vec::new();
    ItemStackSerializer::from(ItemStack::new(3, &Item::DIAMOND))
        .write(&mut out)
        .unwrap();
    // count, item id, components to add, components to remove
    let expected = [
        var_int(3),
        var_int(i32::from(Item::DIAMOND.id)),
        var_int(0),
        var_int(0),
    ]
    .concat();
    assert_eq!(out, expected);
}

#[test]
fn client_hash_of_display_item_matches_custom_stack() {
    let ruby = ruby();
    let hash_of = |item_id: u16| {
        let bytes = [
            vec![1],
            var_int(i32::from(item_id)),
            var_int(3),
            var_int(0),
            var_int(0),
        ]
        .concat();
        OptionalItemStackHash::read(&mut Cursor::new(bytes)).unwrap()
    };
    let custom = ItemStack::new(3, ruby);
    assert!(hash_of(Item::DIAMOND.id).hash_equals(&custom));
    assert!(!hash_of(ruby.id).hash_equals(&custom));
    assert!(hash_of(Item::DIAMOND.id).hash_equals(&ItemStack::new(3, &Item::DIAMOND)));
}
