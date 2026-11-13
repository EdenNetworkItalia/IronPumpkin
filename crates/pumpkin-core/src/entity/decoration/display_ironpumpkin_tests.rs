use super::{BLOCK_STATE, read_block_state, write_block_state};
use pumpkin_data::BlockStateId;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_world::block::state_nbt::block_state_to_nbt;

#[test]
fn an_unreadable_state_becomes_air() {
    let air = Some(i32::from(BlockStateId::AIR.as_u16()));
    let mut nbt = NbtCompound::new();
    nbt.put_int(BLOCK_STATE, i32::MAX);
    assert_eq!(read_block_state(&nbt), air);

    let mut nbt = NbtCompound::new();
    nbt.put_string(BLOCK_STATE, "minecraft:not_a_block".to_string());
    assert_eq!(read_block_state(&nbt), air);

    assert_eq!(read_block_state(&NbtCompound::new()), None);
}

#[test]
fn an_invalid_state_id_is_written_as_air() {
    let mut nbt = NbtCompound::new();
    write_block_state(&mut nbt, -5);
    let mut air = NbtCompound::new();
    air.put_compound(BLOCK_STATE, block_state_to_nbt(BlockStateId::AIR));
    assert_eq!(nbt, air);
}
