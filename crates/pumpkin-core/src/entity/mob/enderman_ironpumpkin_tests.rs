use super::carried_block_from_nbt;
use pumpkin_data::BlockStateId;
use pumpkin_nbt::tag::NbtTag;

#[test]
fn air_is_not_a_carried_block() {
    let air = NbtTag::String("minecraft:air".into());
    assert_eq!(carried_block_from_nbt(&air), None);
    let tag = NbtTag::Int(i32::from(BlockStateId::AIR.as_u16()));
    assert_eq!(carried_block_from_nbt(&tag), None);
}
