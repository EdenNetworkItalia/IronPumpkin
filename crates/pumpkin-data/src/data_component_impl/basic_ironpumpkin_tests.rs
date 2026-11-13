use super::*;

fn decorations() -> PotDecorationsImpl {
    PotDecorationsImpl {
        sherds: [
            Some(Cow::Borrowed("minecraft:archer_pottery_sherd")),
            Some(Cow::Borrowed("minecraft:brick")),
            Some(Cow::Borrowed("minecraft:prize_pottery_sherd")),
            Some(Cow::Borrowed("minecraft:miner_pottery_sherd")),
        ],
    }
}

fn side_id<'a>(map: &'a NbtCompound, side: &str) -> Option<&'a str> {
    map.get_compound(side)?.get_string("id")
}

#[test]
fn pot_decorations_write_the_four_side_map() {
    // 26.3 `PotDecorations.CODEC`: a record of `back`, `left`, `right` and `front`, each an
    // item stack template with an `id`.
    let NbtTag::Compound(map) = decorations().write_data() else {
        panic!("pot decorations must be written as a compound");
    };
    assert_eq!(
        side_id(&map, "back"),
        Some("minecraft:archer_pottery_sherd")
    );
    assert_eq!(side_id(&map, "left"), Some("minecraft:brick"));
    assert_eq!(
        side_id(&map, "right"),
        Some("minecraft:prize_pottery_sherd")
    );
    assert_eq!(
        side_id(&map, "front"),
        Some("minecraft:miner_pottery_sherd")
    );
}

#[test]
fn pot_decorations_read_the_map_and_the_26_2_list() {
    let from_map = PotDecorationsImpl::read_data(&decorations().write_data()).unwrap();
    assert_eq!(from_map, decorations());

    // `PotDecorationsComponentUnflatteningFix`: list index 0 to 3 is back, left, right, front.
    let list = NbtTag::List(
        decorations()
            .ordered()
            .iter()
            .map(|id| NbtTag::String(id.to_string().into()))
            .collect(),
    );
    assert_eq!(PotDecorationsImpl::read_data(&list).unwrap(), decorations());

    // The fix pads a short list with bricks.
    let short = NbtTag::List(vec![NbtTag::String(
        "minecraft:archer_pottery_sherd".into(),
    )]);
    assert_eq!(
        PotDecorationsImpl::read_data(&short).unwrap().sherds,
        [
            Some(Cow::Borrowed("minecraft:archer_pottery_sherd")),
            Some(Cow::Borrowed("minecraft:brick")),
            Some(Cow::Borrowed("minecraft:brick")),
            Some(Cow::Borrowed("minecraft:brick")),
        ]
    );
}

#[test]
fn pot_decorations_map_reads_bare_ids_and_missing_sides() {
    // `ItemStackTemplate.CODEC` also accepts a bare item id, and every side of
    // `PotDecorations.CODEC` is optional: a missing side stays empty.
    let mut map = NbtCompound::new();
    map.put_string("left", "minecraft:archer_pottery_sherd".to_string());
    let mut front = NbtCompound::new();
    front.put_string("id", "minecraft:miner_pottery_sherd".to_string());
    map.put_compound("front", front);

    let read = PotDecorationsImpl::read_data(&NbtTag::Compound(map)).unwrap();
    assert_eq!(
        read.sherds,
        [
            None,
            Some(Cow::Borrowed("minecraft:archer_pottery_sherd")),
            None,
            Some(Cow::Borrowed("minecraft:miner_pottery_sherd")),
        ]
    );
    assert_eq!(
        read.ordered(),
        [
            "minecraft:brick",
            "minecraft:archer_pottery_sherd",
            "minecraft:brick",
            "minecraft:miner_pottery_sherd",
        ]
    );

    // Only the present sides are written back.
    let NbtTag::Compound(written) = read.write_data() else {
        panic!("pot decorations must be written as a compound");
    };
    assert_eq!(written.child_tags.len(), 2);
    assert_eq!(
        side_id(&written, "left"),
        Some("minecraft:archer_pottery_sherd")
    );
    assert_eq!(
        side_id(&written, "front"),
        Some("minecraft:miner_pottery_sherd")
    );
    assert!(
        PotDecorationsImpl::read_data(&NbtTag::Compound(NbtCompound::new()))
            .unwrap()
            .is_empty()
    );
}
