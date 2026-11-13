use super::*;
use pumpkin_data::item::Item;

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
        sherds: [
            Some(Cow::Borrowed("minecraft:archer_pottery_sherd")),
            None,
            Some(Cow::Borrowed("minecraft:brick")),
            Some(Cow::Borrowed("minecraft:miner_pottery_sherd")),
        ],
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
        decoded.sherds,
        [
            Some(Cow::Borrowed("minecraft:archer_pottery_sherd")),
            None,
            None,
            Some(Cow::Borrowed("minecraft:miner_pottery_sherd")),
        ]
    );

    // `ItemStackTemplate` rejects an empty item.
    let air = pot_bytes([Some(&Item::AIR), None, None, None]);
    assert!(PotDecorationsImpl::deserialize(&mut air.as_slice()).is_err());
}
