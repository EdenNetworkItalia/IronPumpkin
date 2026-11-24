//! A data component type with the stream codec of its Java half
//! (`DataComponentTypeBuilder::stream_codec`): the network carries the bytes of that codec, the
//! stack keeps the codec form, and a type without the call keeps the NBT form. The registries
//! freeze once per process, so this file is its own test binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::{
    io::{self, Cursor, Read},
    sync::Once,
};

use ironpumpkin_mods::{
    ModInit,
    data_components::{DataComponentType, DataComponentTypeBuilder},
    pumpkin_data::{
        data_component::DataComponent, dynamic, dynamic::ContentIds, item::Item,
        item_stack::ItemStack, packet::CURRENT_MC_VERSION,
    },
};
use pumpkin_protocol::{
    EncodingKey, VarInt, codec::item_stack_seralizer::ItemStackSerializer, ser::NetworkWriteExt,
};

/// The type with a stream codec: an int as four little-endian bytes, as a mod with
/// `ByteBufCodecs.INT` would not write it (the NBT form is big-endian with a tag).
const PACKED: DataComponentType<i32> = DataComponentType::new("rawmod:packed");
const PLAIN: DataComponentType<i32> = DataComponentType::new("rawmod:plain");

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "the signature that `stream_codec` takes"
)]
fn encode(value: &i32) -> Vec<u8> {
    value.to_le_bytes().to_vec()
}

fn decode(read: &mut dyn Read) -> io::Result<i32> {
    let mut bytes = [0u8; 4];
    read.read_exact(&mut bytes)?;
    Ok(i32::from_le_bytes(bytes))
}

fn frozen() {
    static FREEZE: Once = Once::new();
    FREEZE.call_once(|| {
        let mut init = ModInit::new("rawmod");
        init.register_data_component_type(
            DataComponentTypeBuilder::<i32>::new(PACKED.name()).stream_codec(encode, decode),
        )
        .unwrap();
        init.register_data_component_type(DataComponentTypeBuilder::<i32>::new(PLAIN.name()))
            .unwrap();
        dynamic::freeze().unwrap();
    });
}

fn var_int(value: i32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.write_var_int(&VarInt(value)).unwrap();
    bytes
}

const fn real() -> EncodingKey {
    EncodingKey::new(CURRENT_MC_VERSION, ContentIds::Real)
}

#[test]
fn the_network_carries_the_bytes_of_the_stream_codec_and_a_read_gives_the_value_back() {
    frozen();
    // Names sort as `packed`, `plain`, so the ids follow the vanilla components.
    let mut stack = ItemStack::new(1, &Item::STICK);
    PACKED.set(&mut stack, &9).unwrap();
    // The stack keeps the codec form: a get does not use the stream codec.
    assert_eq!(PACKED.get(&stack), Ok(Some(9)));

    let mut bytes = Vec::new();
    ItemStackSerializer::from(stack.clone())
        .write_with_version(&mut bytes, &real())
        .unwrap();
    let mut value = var_int(i32::from(DataComponent::COUNT));
    value.extend([9, 0, 0, 0]);
    assert!(bytes.ends_with(&value), "{bytes:?}");

    let read = ItemStackSerializer::read(&mut Cursor::new(&bytes[..]))
        .unwrap()
        .to_stack();
    assert_eq!(PACKED.get(&read), Ok(Some(9)));
}

#[test]
fn a_length_prefixed_stack_carries_the_bytes_with_a_length_prefix_and_reads_them_back() {
    frozen();
    let mut stack = ItemStack::new(1, &Item::STICK);
    PACKED.set(&mut stack, &9).unwrap();
    let mut bytes = Vec::new();
    ItemStackSerializer::from(stack)
        .write_length_prefixed_with_version(&mut bytes, &real())
        .unwrap();
    let mut value = var_int(i32::from(DataComponent::COUNT));
    value.extend(var_int(4));
    value.extend([9, 0, 0, 0]);
    assert!(bytes.ends_with(&value), "{bytes:?}");

    let read = ItemStackSerializer::read_length_prefixed_optional(&mut Cursor::new(&bytes[..]))
        .unwrap()
        .to_stack();
    assert_eq!(PACKED.get(&read), Ok(Some(9)));
}

#[test]
fn a_truncated_value_is_a_read_error() {
    frozen();
    let mut stack = ItemStack::new(1, &Item::STICK);
    PACKED.set(&mut stack, &9).unwrap();
    let mut bytes = Vec::new();
    ItemStackSerializer::from(stack)
        .write_with_version(&mut bytes, &real())
        .unwrap();
    bytes.truncate(bytes.len() - 2);
    assert!(ItemStackSerializer::read(&mut Cursor::new(&bytes[..])).is_err());
}

#[test]
fn a_type_without_a_stream_codec_keeps_the_nbt_form() {
    frozen();
    let mut stack = ItemStack::new(1, &Item::STICK);
    PLAIN.set(&mut stack, &9).unwrap();
    let mut bytes = Vec::new();
    ItemStackSerializer::from(stack)
        .write_with_version(&mut bytes, &real())
        .unwrap();
    // Id, then the NBT int: tag 3 and the big-endian value.
    let mut value = var_int(i32::from(DataComponent::COUNT) + 1);
    value.extend([3, 0, 0, 0, 9]);
    assert!(bytes.ends_with(&value), "{bytes:?}");
}
