pub use pumpkin_data::statistic::{CustomStatistic, StatisticCategory};
use pumpkin_data::{Block, BlockId, entity::EntityType, item::Item};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use rustc_hash::FxHashMap;
use std::fmt::Debug;
use std::sync::LazyLock;
use tracing::{debug, warn};

const STATISTICS: &str = "Statistics";

/// Custom statistic registry name -> id. The ids are 0..=77, so a scan of 0..=255 finds them all.
static CUSTOM_STATISTIC_IDS: LazyLock<FxHashMap<String, i32>> = LazyLock::new(|| {
    (0..=i32::from(u8::MAX))
        .filter_map(|id| {
            let stat = CustomStatistic::from_i32(id)?;
            Some((namespaced(&registry_name(&stat)), id))
        })
        .collect()
});

#[derive(Default)]
pub struct Statistics {
    /// (Category ID, Statistic ID) -> Value
    pub stats: FxHashMap<(i32, i32), i32>,
}

impl Statistics {
    pub fn increment(&mut self, category: StatisticCategory, stat: i32, amount: i32) {
        let entry = self.stats.entry((category as i32, stat)).or_insert(0);
        *entry = entry.saturating_add(amount);
    }

    pub fn increment_custom(&mut self, stat: CustomStatistic, amount: i32) {
        self.increment(StatisticCategory::Custom, stat as i32, amount);
    }

    pub fn set(&mut self, category: StatisticCategory, stat: i32, value: i32) {
        self.stats.insert((category as i32, stat), value);
    }

    #[must_use]
    pub fn get(&self, category: StatisticCategory, stat: i32) -> i32 {
        *self.stats.get(&(category as i32, stat)).unwrap_or(&0)
    }

    /// Writes `Statistics.<minecraft:category>.<minecraft:name>`, the layout of the `stats` object
    /// in vanilla's stats file. Numeric ids change when content is added, names do not.
    pub fn write_nbt(&self, nbt: &mut NbtCompound) {
        let mut categories: FxHashMap<String, NbtCompound> = FxHashMap::default();
        for ((category_id, stat), value) in &self.stats {
            let Some(category) = StatisticCategory::from_i32(*category_id) else {
                debug!("Not saving statistic {category_id}:{stat}, no category has this id");
                continue;
            };
            let Some(name) = stat_name(category, *stat) else {
                debug!("Not saving statistic {category_id}:{stat}, no registry names it");
                continue;
            };
            categories
                .entry(namespaced(&registry_name(&category)))
                .or_default()
                .put_int(&name, *value);
        }
        let mut stats_compound = NbtCompound::new();
        for (category, entries) in categories {
            stats_compound.put_compound(&category, entries);
        }
        nbt.put_compound(STATISTICS, stats_compound);
    }

    /// Reads the layout of [`Self::write_nbt`] and the older `<category id>:<statistic id>` int
    /// keys. A key that no registry resolves is dropped, with one warning for all of them.
    pub fn read_nbt(&mut self, nbt: &NbtCompound) {
        let Some(stats_compound) = nbt.get_compound(STATISTICS) else {
            return;
        };
        let (stats, unresolved) = parse_statistics(stats_compound);
        self.stats.extend(stats);
        if let Some(first) = unresolved.first() {
            warn!(
                "Dropped {} statistics that no registry resolves, the first is {first}",
                unresolved.len()
            );
        }
    }
}

fn parse_statistics(stats_compound: &NbtCompound) -> (FxHashMap<(i32, i32), i32>, Vec<String>) {
    let mut stats = FxHashMap::default();
    let mut unresolved = Vec::new();
    for (key, tag) in &stats_compound.child_tags {
        match tag {
            NbtTag::Compound(entries) => {
                let Some(category) = StatisticCategory::from_registry_key(key) else {
                    unresolved.push(key.to_string());
                    continue;
                };
                for (name, tag) in &entries.child_tags {
                    match (tag, stat_id(category, name)) {
                        (NbtTag::Int(value), Some(stat)) => {
                            stats.insert((category as i32, stat), *value);
                        }
                        _ => unresolved.push(format!("{key}.{name}")),
                    }
                }
            }
            NbtTag::Int(value) => match numeric_key(key) {
                Some(key) => {
                    stats.insert(key, *value);
                }
                None => unresolved.push(key.to_string()),
            },
            _ => unresolved.push(key.to_string()),
        }
    }
    (stats, unresolved)
}

/// Parses the key that older Pumpkin versions wrote, `<category id>:<statistic id>`. The ids must
/// still name something.
fn numeric_key(key: &str) -> Option<(i32, i32)> {
    let (category, stat) = key.split_once(':')?;
    let category = StatisticCategory::from_i32(category.parse().ok()?)?;
    let stat = stat.parse().ok()?;
    stat_name(category, stat)?;
    Some((category as i32, stat))
}

/// The namespaced registry name of the thing a statistic counts: a block, an item, an entity type
/// or a custom statistic.
fn stat_name(category: StatisticCategory, id: i32) -> Option<String> {
    let raw_id = u16::try_from(id).ok()?;
    match category {
        StatisticCategory::Mined => Some(namespaced(Block::from_id(BlockId::new(raw_id)?).name)),
        StatisticCategory::Crafted
        | StatisticCategory::Used
        | StatisticCategory::Broken
        | StatisticCategory::PickedUp
        | StatisticCategory::Dropped => Some(namespaced(Item::from_id(raw_id)?.registry_key)),
        StatisticCategory::Killed | StatisticCategory::KilledBy => {
            Some(namespaced(EntityType::from_raw(raw_id)?.resource_name))
        }
        StatisticCategory::Custom => {
            Some(namespaced(&registry_name(&CustomStatistic::from_i32(id)?)))
        }
    }
}

/// The reverse of [`stat_name`].
fn stat_id(category: StatisticCategory, name: &str) -> Option<i32> {
    match category {
        StatisticCategory::Mined => Some(i32::from(Block::from_name(name)?.id.as_u16())),
        StatisticCategory::Crafted
        | StatisticCategory::Used
        | StatisticCategory::Broken
        | StatisticCategory::PickedUp
        | StatisticCategory::Dropped => Some(i32::from(Item::from_registry_key(name)?.id)),
        StatisticCategory::Killed | StatisticCategory::KilledBy => {
            Some(i32::from(EntityType::from_name(name)?.id))
        }
        StatisticCategory::Custom => CUSTOM_STATISTIC_IDS.get(name).copied(),
    }
}

/// The generated enums have no name accessor: `PickedUp` is `picked_up` in the registry.
/// `registry_names_match_the_extracted_data` checks this against `assets/stats.json`.
fn registry_name(variant: &impl Debug) -> String {
    let variant = format!("{variant:?}");
    let mut name = String::with_capacity(variant.len() + 4);
    for (index, c) in variant.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if index > 0 {
                name.push('_');
            }
            name.push(c.to_ascii_lowercase());
        } else {
            name.push(c);
        }
    }
    name
}

fn namespaced(name: &str) -> String {
    if name.contains(':') {
        name.to_string()
    } else {
        format!("minecraft:{name}")
    }
}

#[cfg(test)]
mod tests {
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
}
