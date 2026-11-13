use super::*;

fn sherd_ids(sherds: &NbtTag) -> [Option<&str>; 4] {
    let NbtTag::Compound(map) = sherds else {
        panic!("sherds must be written as a compound");
    };
    ["back", "left", "right", "front"].map(|side| {
        map.get_compound(side)
            .and_then(|side| side.get_string("id"))
    })
}

#[test]
fn sherds_are_written_as_the_four_side_map_and_read_from_both_shapes() {
    // 26.3 `DecoratedPotBlockEntity` stores `sherds` with `PotDecorations.CODEC`.
    let expected = [
        Some("minecraft:archer_pottery_sherd"),
        Some("minecraft:brick"),
        Some("minecraft:prize_pottery_sherd"),
        Some("minecraft:miner_pottery_sherd"),
    ];
    let ids = expected.map(|id| NbtTag::String(id.unwrap().into()));

    let mut as_list = NbtCompound::new();
    as_list.put_list("sherds", ids.to_vec());
    let pot = DecoratedPotBlockEntity::from_nbt(&as_list, BlockPos::new(0, 0, 0));

    let mut written = NbtCompound::new();
    pot.write_nbt(&mut written);
    assert_eq!(sherd_ids(written.get("sherds").unwrap()), expected);
    assert_eq!(
        sherd_ids(pot.chunk_data_nbt().unwrap().get("sherds").unwrap()),
        expected
    );

    let reloaded = DecoratedPotBlockEntity::from_nbt(&written, BlockPos::new(0, 0, 0));
    let mut rewritten = NbtCompound::new();
    reloaded.write_nbt(&mut rewritten);
    assert_eq!(sherd_ids(rewritten.get("sherds").unwrap()), expected);
}

#[test]
fn a_missing_side_stays_empty_and_no_sherds_write_nothing() {
    // `PotDecorations.CODEC` reads every side with `optionalFieldOf`.
    let mut front = NbtCompound::new();
    front.put_string("id", "minecraft:heart_pottery_sherd".to_string());
    let mut map = NbtCompound::new();
    map.put_compound("front", front);
    let mut nbt = NbtCompound::new();
    nbt.put_compound("sherds", map);
    let pot = DecoratedPotBlockEntity::from_nbt(&nbt, BlockPos::new(0, 0, 0));

    let mut written = NbtCompound::new();
    pot.write_nbt(&mut written);
    assert_eq!(
        sherd_ids(written.get("sherds").unwrap()),
        [None, None, None, Some("minecraft:heart_pottery_sherd")]
    );

    // `DecoratedPotBlockEntity.saveAdditional` skips `PotDecorations.EMPTY`.
    let plain = DecoratedPotBlockEntity::from_nbt(&NbtCompound::new(), BlockPos::new(0, 0, 0));
    let mut written = NbtCompound::new();
    plain.write_nbt(&mut written);
    assert!(written.get("sherds").is_none());
}
