//! The `NeoForge` tag ids that the `Tags` classes name exist in the generated tag maps, one test
//! row per registry.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use pumpkin_data::Block;
use pumpkin_data::item::Item;
use pumpkin_data::tag::{RegistryKey, get_tag_ids, get_tag_values};

#[test]
fn block_tags_exist() {
    let farmlands = get_tag_ids(RegistryKey::Block, "neoforge:villager_farmlands").unwrap();
    assert_eq!(farmlands, [u16::from(Block::FARMLAND.id)]);
    for tag in [
        "neoforge:enderman_place_on_blacklist",
        "neoforge:needs_wood_tool",
        "neoforge:needs_gold_tool",
        "neoforge:needs_netherite_tool",
    ] {
        assert!(get_tag_ids(RegistryKey::Block, tag).is_some(), "{tag}");
    }
}

#[test]
fn item_tags_follow_the_conventional_tags() {
    let bows = get_tag_ids(RegistryKey::Item, "neoforge:skeleton_usable_bows").unwrap();
    assert_eq!(bows, [Item::BOW.id]);
    let fuels = get_tag_ids(RegistryKey::Item, "neoforge:enchanting_fuels").unwrap();
    assert_eq!(fuels, [Item::LAPIS_LAZULI.id]);
    for tag in [
        "neoforge:piglin_usable_crossbows",
        "neoforge:pillager_usable_crossbows",
    ] {
        assert_eq!(
            get_tag_ids(RegistryKey::Item, tag).unwrap(),
            [Item::CROSSBOW.id],
            "{tag}"
        );
    }
}

#[test]
fn damage_type_tags_resolve_nested_tags() {
    let magic = get_tag_values(RegistryKey::DamageType, "neoforge:is_magic").unwrap();
    assert!(magic.contains(&"magic"));
    assert!(magic.contains(&"wither"));
    for tag in [
        "neoforge:is_poison",
        "neoforge:is_wither",
        "neoforge:is_environment",
        "neoforge:is_physical",
        "neoforge:is_technical",
        "neoforge:no_flinch",
    ] {
        assert!(
            get_tag_values(RegistryKey::DamageType, tag).is_some(),
            "{tag}"
        );
    }
}

#[test]
fn world_clock_tags_exist() {
    for tag in [
        "neoforge:ignores/pause_command",
        "neoforge:ignores/advance_time_rule",
    ] {
        assert!(get_tag_ids(RegistryKey::WorldClock, tag).is_some(), "{tag}");
    }
}

#[test]
fn biome_tag_poplar_exists() {
    let values = get_tag_values(RegistryKey::WorldgenBiome, "c:primary_wood_type/poplar").unwrap();
    assert_eq!(values, ["dappled_forest"]);
}
