use super::*;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;

/// Appends one side of vanilla `PotDecorations.STREAM_CODEC`: `ByteBufCodecs.optional` of
/// `ItemStackTemplate.STREAM_CODEC`, which is the item holder id, the count and the patch.
fn push_pot_side(bytes: &mut Vec<u8>, item: Option<&Item>) {
    let Some(item) = item else {
        bytes.push(0);
        return;
    };
    bytes.push(1);
    bytes.write_var_int(&VarInt(i32::from(item.id))).unwrap();
    bytes.extend_from_slice(&[1, 0, 0]); // count 1, no added or removed components
}

fn pot_bytes(sides: [Option<&Item>; 4]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for side in sides {
        push_pot_side(&mut bytes, side);
    }
    bytes
}

#[test]
fn pot_decorations_are_written_as_four_optional_templates() {
    let decorations = PotDecorationsImpl {
        back: Some(ItemStack::new(1, &Item::ARCHER_POTTERY_SHERD)),
        left: None,
        right: Some(ItemStack::new(1, &Item::BRICK)),
        front: Some(ItemStack::new(1, &Item::MINER_POTTERY_SHERD)),
    };
    let mut encoded = Vec::new();
    decorations.serialize(&mut encoded).unwrap();
    assert_eq!(
        encoded,
        pot_bytes([
            Some(&Item::ARCHER_POTTERY_SHERD),
            None,
            Some(&Item::BRICK),
            Some(&Item::MINER_POTTERY_SHERD),
        ])
    );
}

#[test]
fn pot_decorations_are_read_from_four_optional_templates() {
    let bytes = pot_bytes([
        Some(&Item::ARCHER_POTTERY_SHERD),
        None,
        None,
        Some(&Item::MINER_POTTERY_SHERD),
    ]);
    let mut slice = bytes.as_slice();
    let decoded = PotDecorationsImpl::deserialize(&mut slice).unwrap();
    assert!(slice.is_empty());
    assert_eq!(
        [&decoded.back, &decoded.left, &decoded.right, &decoded.front]
            .map(|side| side.as_ref().map(|stack| stack.item.id)),
        [
            Some(Item::ARCHER_POTTERY_SHERD.id),
            None,
            None,
            Some(Item::MINER_POTTERY_SHERD.id),
        ]
    );

    // `ItemStackTemplate` rejects an empty item.
    let air = pot_bytes([Some(&Item::AIR), None, None, None]);
    assert!(PotDecorationsImpl::deserialize(&mut air.as_slice()).is_err());
}

/// `BundleContents.STREAM_CODEC` with one `ItemStackTemplate.STREAM_CODEC` entry. Its patch is
/// `DataComponentPatch.STREAM_CODEC`: added count, removed count, then each added type id and
/// its value without a length prefix, then each removed type id.
#[test]
fn nested_template_patch_has_no_length_prefix() {
    let mut bytes = Vec::new();
    bytes.write_var_int(&VarInt(1)).unwrap(); // one template
    bytes
        .write_var_int(&VarInt(i32::from(Item::IRON_SWORD.id)))
        .unwrap();
    bytes.write_var_int(&VarInt(2)).unwrap(); // count
    bytes.write_var_int(&VarInt(1)).unwrap(); // added
    bytes.write_var_int(&VarInt(1)).unwrap(); // removed
    bytes
        .write_var_int(&VarInt(i32::from(DataComponent::Damage.to_id())))
        .unwrap();
    bytes.write_var_int(&VarInt(300)).unwrap(); // `ByteBufCodecs.VAR_INT`
    bytes
        .write_var_int(&VarInt(i32::from(DataComponent::Unbreakable.to_id())))
        .unwrap();

    let mut slice = bytes.as_slice();
    let bundle = BundleContentsImpl::deserialize(&mut slice).unwrap();
    assert!(slice.is_empty());
    let [stack] = bundle.items.as_slice() else {
        panic!("one template expected");
    };
    assert_eq!(stack.item.id, Item::IRON_SWORD.id);
    assert_eq!(stack.item_count, 2);
    assert_eq!(
        stack.get_data_component::<DamageImpl>().map(|d| d.damage),
        Some(300)
    );
    assert!(
        stack
            .patch
            .iter()
            .any(|(id, data)| *id == DataComponent::Unbreakable && data.is_none())
    );

    let mut encoded = Vec::new();
    bundle.serialize(&mut encoded).unwrap();
    assert_eq!(encoded, bytes);
}
