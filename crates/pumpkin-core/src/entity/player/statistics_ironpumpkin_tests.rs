use super::*;

fn stone_stats() -> Statistics {
    let mut stats = Statistics::default();
    stats.set(
        StatisticCategory::Mined,
        i32::from(Block::STONE.id.as_u16()),
        5,
    );
    stats.set(StatisticCategory::Used, i32::from(Item::STONE.id), 2);
    stats.set(StatisticCategory::Killed, i32::from(EntityType::PIG.id), 3);
    stats.set(
        StatisticCategory::Custom,
        CustomStatistic::PlayTime as i32,
        120,
    );
    stats
}

fn named_stone_stats() -> NbtCompound {
    let mut nbt = NbtCompound::new();
    let mut statistics = NbtCompound::new();
    for (category, name, value) in [
        ("minecraft:mined", "minecraft:stone", 5),
        ("minecraft:used", "minecraft:stone", 2),
        ("minecraft:killed", "minecraft:pig", 3),
        ("minecraft:custom", "minecraft:play_time", 120),
    ] {
        let mut entries = NbtCompound::new();
        entries.put_int(name, value);
        statistics.put_compound(category, entries);
    }
    nbt.put_compound("Statistics", statistics);
    nbt
}

#[test]
fn keys_statistics_by_name_like_vanilla() {
    let mut nbt = NbtCompound::new();
    stone_stats().write_nbt(&mut nbt);
    assert_eq!(nbt, named_stone_stats());
}

#[test]
fn named_statistics_round_trip() {
    let mut read = Statistics::default();
    read.read_nbt(&named_stone_stats());
    assert_eq!(read.stats, stone_stats().stats);
}

#[test]
fn migrates_numeric_keys_and_drops_the_ones_nothing_resolves() {
    let mut statistics = NbtCompound::new();
    for (key, value) in [
        (format!("0:{}", Block::STONE.id.as_u16()), 5),
        (format!("2:{}", Item::STONE.id), 2),
        (format!("6:{}", EntityType::PIG.id), 3),
        (format!("8:{}", CustomStatistic::PlayTime as i32), 120),
        ("8:9999".to_string(), 7),
        ("99:1".to_string(), 7),
        ("garbage".to_string(), 7),
    ] {
        statistics.put_int(&key, value);
    }

    let (stats, mut unresolved) = parse_statistics(&statistics);
    assert_eq!(stats, stone_stats().stats);
    unresolved.sort();
    assert_eq!(unresolved, ["8:9999", "99:1", "garbage"]);

    let mut nbt = NbtCompound::new();
    nbt.put_compound("Statistics", statistics);
    let mut migrated = Statistics::default();
    migrated.read_nbt(&nbt);
    let mut rewritten = NbtCompound::new();
    migrated.write_nbt(&mut rewritten);
    assert_eq!(rewritten, named_stone_stats());
}

#[test]
fn mined_statistics_are_keyed_by_block_id_not_state_id() {
    for block in [&Block::OAK_STAIRS, &Block::REPEATER, &Block::HOPPER] {
        let name = namespaced(block.name);
        let block_id = i32::from(block.id.as_u16());
        let state_id = i32::from(block.default_state.id.as_u16());
        assert_ne!(block_id, state_id, "{name}");
        assert_ne!(
            stat_name(StatisticCategory::Mined, state_id).as_deref(),
            Some(name.as_str())
        );

        let mut stats = Statistics::default();
        stats.increment(StatisticCategory::Mined, block_id, 1);
        let mut nbt = NbtCompound::new();
        stats.write_nbt(&mut nbt);
        let mined = nbt
            .get_compound("Statistics")
            .and_then(|statistics| statistics.get_compound("minecraft:mined"))
            .unwrap();
        assert_eq!(mined.get_int(&name), Some(1));
    }
}

#[test]
fn drops_names_that_no_registry_resolves() {
    let mut entries = NbtCompound::new();
    entries.put_int("minecraft:stone", 1);
    entries.put_int("mymod:not_a_block", 1);
    let mut statistics = NbtCompound::new();
    statistics.put_compound("minecraft:mined", entries);
    statistics.put_compound("mymod:not_a_category", NbtCompound::new());

    let (stats, mut unresolved) = parse_statistics(&statistics);
    assert_eq!(
        stats.get(&(
            StatisticCategory::Mined as i32,
            i32::from(Block::STONE.id.as_u16())
        )),
        Some(&1)
    );
    assert_eq!(stats.len(), 1);
    unresolved.sort();
    assert_eq!(
        unresolved,
        ["minecraft:mined.mymod:not_a_block", "mymod:not_a_category"]
    );
}

#[test]
fn registry_names_match_the_extracted_data() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../../../assets/stats.json")).unwrap();
    let mut custom_count = 0;
    for (category_name, category) in data.as_object().unwrap() {
        let id = i32::try_from(category["id"].as_i64().unwrap()).unwrap();
        let category_variant = StatisticCategory::from_i32(id).unwrap();
        assert_eq!(
            &namespaced(&registry_name(&category_variant)),
            category_name
        );
        assert_eq!(
            StatisticCategory::from_registry_key(category_name),
            Some(category_variant)
        );
        for (name, entry) in category["entries"].as_object().unwrap() {
            let stat = i32::try_from(entry["id"].as_i64().unwrap()).unwrap();
            assert_eq!(stat_name(category_variant, stat).as_deref(), Some(&**name));
            assert_eq!(stat_id(category_variant, name), Some(stat));
            custom_count += 1;
        }
    }
    assert_eq!(CUSTOM_STATISTIC_IDS.len(), custom_count);
}

#[test]
fn custom_content_stats_are_not_sent() {
    let cases = [
        (
            StatisticCategory::Mined,
            BlockId::COUNT,
            Block::STONE.id.as_u16(),
        ),
        (StatisticCategory::Crafted, Item::COUNT, Item::DIAMOND.id),
        (StatisticCategory::PickedUp, Item::COUNT, Item::DIAMOND.id),
        (
            StatisticCategory::Killed,
            EntityType::COUNT,
            EntityType::PIG.id,
        ),
        (
            StatisticCategory::KilledBy,
            EntityType::COUNT,
            EntityType::PIG.id,
        ),
    ];
    for (category, generated, vanilla) in cases {
        let category = category as i32;
        assert!(is_sent_to_client(category, i32::from(vanilla)));
        assert!(!is_sent_to_client(category, i32::from(generated)));
    }
    let custom = StatisticCategory::Custom as i32;
    assert!(is_sent_to_client(custom, CustomStatistic::Jump as i32));
}
