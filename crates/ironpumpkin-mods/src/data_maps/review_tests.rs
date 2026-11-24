#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::{fs, path::Path};

use pumpkin_codecs::{Decode, json_ops::JsonOps};
use serde_json::json;

use super::{
    builtin::{BLOCK_TRANSFORM_APPENDERS, MonsterRoomMob, TRANSFORMABLES},
    loader::{self, Pack},
    registries::{Lookup, check_registry},
    *,
};

fn id(text: &'static str) -> Identifier {
    Identifier::parse_static(text)
}

struct Items;

impl Lookup for Items {
    fn contains(&self, registry: &Identifier, entry: &Identifier) -> bool {
        registry == &id("minecraft:item")
            && [
                "minecraft:stick",
                "minecraft:oak_planks",
                "minecraft:birch_planks",
            ]
            .contains(&entry.to_string().as_str())
    }

    fn tag(&self, _: &Identifier, tag: &Identifier) -> Vec<Identifier> {
        if tag == &id("minecraft:planks") {
            vec![id("minecraft:oak_planks"), id("minecraft:birch_planks")]
        } else {
            Vec::new()
        }
    }
}

/// Writes `text` verbatim, so the key order of the file is the order on disk.
fn write_raw(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn load_one<T: DataMapValue>(data_map: &DataMapType<T>, dir: &Path) {
    let mut registered = Registered::default();
    registered
        .by_registry
        .entry(data_map.info.registry.clone())
        .or_default()
        .push(data_map.info.clone());
    loader::load(
        &registered,
        &[Pack {
            id: "file/pack".to_owned(),
            root: Some(dir.join("pack")),
            memory: Vec::new(),
        }],
        &Items,
    );
}

/// `DataMapFile.values` is an `ImmutableMap` built in the order of the Gson object, so an entry
/// and a tag that share a key merge in file order.
#[test]
fn values_of_one_file_apply_in_file_order() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    let lists: AdvancedDataMapType<Vec<i32>> =
        AdvancedDataMapType::builder(id("mapmod:order"), id("minecraft:item"))
            .merger(ListMerger)
            .build();
    write_raw(
        dir,
        "pack/data/mapmod/data_maps/item/order.json",
        r##"{ "values": { "minecraft:oak_planks": [1], "#minecraft:planks": [2] } }"##,
    );
    load_one(&lists, dir);
    // NeoForge: oak_planks gets [1] first, then the tag merges [2] in: [1, 2].
    assert_eq!(
        *lists.get_data(&id("minecraft:oak_planks")).unwrap(),
        [1, 2]
    );
}

/// `ExtraCodecs.strictUnboundedMap` builds an `ImmutableMap`, whose `build()` throws on two keys
/// that parse to the same id, so `readData` drops the whole file.
#[test]
fn two_keys_for_one_entry_fail_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    let ints: DataMapType<i32> =
        DataMapType::builder(id("mapmod:dup"), id("minecraft:item")).build();
    write_raw(
        dir,
        "pack/data/mapmod/data_maps/item/dup.json",
        r#"{ "values": { "stick": 1, "minecraft:stick": 2 } }"#,
    );
    load_one(&ints, dir);
    assert!(ints.get_data(&id("minecraft:stick")).is_none());
}

/// `NeoForgeDataMaps.BLOCK_TRANSFORM_APPENDERS` does not call `disableTagSupport`; only
/// `TRANSFORMABLES` does.
#[test]
fn block_transform_appenders_accept_tags() {
    assert!(TRANSFORMABLES.no_tags_reason().is_some());
    assert_eq!(BLOCK_TRANSFORM_APPENDERS.no_tags_reason(), None);
}

/// `RecordCodecBuilder` ignores the keys it does not read, so the record form of
/// `MonsterRoomMob.CODEC` reads `{"weight": 5, "comment": ..}`.
#[test]
fn a_one_field_record_ignores_other_keys() {
    let mob = MonsterRoomMob::parse(json!({ "weight": 5, "comment": "x" }), &JsonOps);
    assert!(
        matches!(
            mob,
            pumpkin_codecs::DataResult::Success {
                result: MonsterRoomMob { weight: 5 },
                ..
            }
        ),
        "{mob:?}"
    );
}

/// `Codec.INT` reads any JSON number through `Number.intValue`.
#[test]
fn an_int_codec_reads_a_whole_float() {
    let mob = MonsterRoomMob::parse(json!(5.0), &JsonOps);
    assert!(
        matches!(
            mob,
            pumpkin_codecs::DataResult::Success {
                result: MonsterRoomMob { weight: 5 },
                ..
            }
        ),
        "{mob:?}"
    );
}

/// `NeoForge` registers a data map for any registry; `IronPumpkin` refuses the registries whose
/// entries it does not know.
#[test]
fn registries_refused_at_registration() {
    let mut refused = Vec::new();
    for registry in [
        "minecraft:potion",
        "minecraft:enchantment",
        "minecraft:damage_type",
        "minecraft:worldgen/structure",
        "minecraft:worldgen/biome",
        "minecraft:block_entity_type",
        "minecraft:particle_type",
        "minecraft:loot_table",
        "minecraft:recipe",
        "minecraft:painting_variant",
        "minecraft:instrument",
        "mymod:widget",
    ] {
        if check_registry(&id("mapmod:x"), &id(registry), false).is_err() {
            refused.push(registry);
        }
    }
    assert_eq!(
        refused,
        [
            "minecraft:potion",
            "minecraft:worldgen/structure",
            "minecraft:block_entity_type",
            "minecraft:particle_type",
            "minecraft:loot_table",
            "minecraft:recipe",
            "mymod:widget",
        ]
    );
}

/// A `MapCodec.unitCodec` used as a `Codec` reads only a map, so `"remove": {"minecraft:stick": 1}`
/// fails for a type with the default remover.
#[test]
fn the_default_remover_reads_only_a_map() {
    assert!(matches!(
        DefaultRemover::parse(json!(1), &JsonOps),
        pumpkin_codecs::DataResult::Error { .. }
    ));
}
