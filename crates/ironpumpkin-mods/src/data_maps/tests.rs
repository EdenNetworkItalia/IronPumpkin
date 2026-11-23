#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::{PoisonError, RwLock},
};

use pumpkin_codecs::{Decode, Encode, json_ops::JsonOps};
use pumpkin_protocol::java::neoforge::{RegistryDataMapSyncPayload, decode_exact};
use pumpkin_util::identifier::Identifier;
use serde_json::{Value, json};

use super::{
    builtin::{
        AcceptableVillagerDistance, BlockTransformData, Oxidizable, Transformable,
        TransformableRemover, VillagerCompostable,
    },
    loader::{self, Pack},
    registries::Lookup,
    sync, *,
};
use crate::{LoadedMod, NativeMod};

fn id(text: &'static str) -> Identifier {
    Identifier::parse_static(text)
}

fn target() -> &'static RwLock<Registered> {
    Box::leak(Box::default())
}

fn read(target: &RwLock<Registered>) -> std::sync::RwLockReadGuard<'_, Registered> {
    target.read().unwrap_or_else(PoisonError::into_inner)
}

/// A mod whose `register_data_map_types` runs `register`.
struct RegisterMod {
    register: fn(&mut RegisterDataMapTypesEvent),
}

impl NativeMod for RegisterMod {
    fn id(&self) -> &'static str {
        "mapmod"
    }

    fn display_name(&self) -> &'static str {
        "Map Mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, _cx: &mut crate::ModInit) {}

    fn register_data_map_types(&self, event: &mut RegisterDataMapTypesEvent) {
        (self.register)(event);
    }
}

fn post_mod(
    target: &'static RwLock<Registered>,
    native_mod: &'static RegisterMod,
) -> Result<(), String> {
    post_with(
        &[LoadedMod {
            native_mod,
            mod_id: "mapmod",
        }],
        target,
        false,
    )
}

fn int_map(path: &'static str, registry: &'static str) -> DataMapType<i32> {
    DataMapType::builder(id(path), id(registry)).build()
}

#[test]
fn register_rejects_a_duplicate_id() {
    static MOD: RegisterMod = RegisterMod {
        register: |event| {
            event
                .register(&int_map("mapmod:burn", "minecraft:item"))
                .unwrap();
            let error = event
                .register(&int_map("mapmod:burn", "minecraft:item"))
                .unwrap_err();
            assert_eq!(
                error.to_string(),
                "Tried to register data map type with ID mapmod:burn to registry minecraft:item twice"
            );
            // The same id on another registry is another type.
            event
                .register(&int_map("mapmod:burn", "minecraft:block"))
                .unwrap();
        },
    };
    let target = target();
    let error = post_mod(target, &MOD).unwrap_err();
    assert!(
        error.starts_with("the RegisterDataMapTypesEvent of mod mapmod: Tried to register"),
        "{error}"
    );
    assert_eq!(read(target).by_registry.len(), 2);
}

#[test]
fn register_rejects_registries_that_cannot_carry_the_map() {
    static MOD: RegisterMod = RegisterMod {
        register: |event| {
            let synced = |registry| {
                DataMapType::<i32>::builder(id("mapmod:weight"), id(registry))
                    .synced(false)
                    .build()
            };
            let message = |registry| event_error(&synced(registry));
            assert_eq!(
                message("minecraft:worldgen/structure"),
                "Cannot register synced data map mapmod:weight for world datapack registry minecraft:worldgen/structure that is not synced!"
            );
            assert_eq!(
                message("minecraft:loot_table"),
                "Cannot register synced data map mapmod:weight for reloadable datapack registry minecraft:loot_table, reloadable registries do not support sync"
            );
            assert_eq!(
                message("mapmod:widget"),
                "Cannot register data map mapmod:weight for registry mapmod:widget: IronPumpkin does not know the entries of this registry"
            );
            // A synced map on a synced data pack registry and on a built-in registry.
            event.register(&synced("minecraft:worldgen/biome")).unwrap();
            event
                .register(&synced("minecraft:block_transformer"))
                .unwrap();
            event.register(&synced("minecraft:fluid")).unwrap();
            // A map that is not synced still needs a registry with known entries.
            let error = event.register(&int_map("mapmod:other", "minecraft:loot_table"));
            assert!(matches!(
                error,
                Err(DataMapError::UnsupportedRegistry { .. })
            ));
        },
    };
    fn event_error(data_map: &DataMapType<i32>) -> String {
        let mut event = RegisterDataMapTypesEvent::new(target());
        event.register(data_map).unwrap_err().to_string()
    }
    let target = target();
    assert!(post_mod(target, &MOD).is_err());
    assert_eq!(read(target).by_registry.len(), 3);
}

#[test]
fn registration_closes_after_the_event() {
    let target = target();
    post_with(&[], target, false).unwrap();
    let mut event = RegisterDataMapTypesEvent::new(target);
    assert_eq!(
        event.register(&int_map("mapmod:late", "minecraft:item")),
        Err(DataMapError::Closed(id("mapmod:late")))
    );
}

#[test]
fn builtin_types_match_the_known_data_maps_of_neoforge() {
    let target = target();
    post_with(&[], target, true).unwrap();
    let ours = sync::known_data_maps(&read(target));
    // The list that `NeoForge` 26.3.0.64-beta sends, captured in run (b).
    let neoforge = pumpkin_core::net::java::configuration_payloads::known_registry_data_maps();
    let as_sets = |payload: &pumpkin_protocol::java::neoforge::KnownRegistryDataMapsPayload| {
        payload
            .data_maps
            .iter()
            .map(|(registry, maps)| {
                let maps: BTreeSet<_> = maps
                    .iter()
                    .map(|map| (map.id.clone(), map.mandatory))
                    .collect();
                (registry.clone(), maps)
            })
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(as_sets(&ours), as_sets(&neoforge));
    let registered = read(target);
    let count: usize = registered.by_registry.values().map(Vec::len).sum();
    assert_eq!(count, 11);
    // `villager_compostables` is not synced, so it is not listed.
    assert!(
        registered.by_registry[&id("minecraft:item")]
            .iter()
            .any(|info| info.id == id("neoforge:villager_compostables") && info.sync.is_none())
    );
    assert!(sync::mandatory_data_maps(&registered).is_empty());
}

/// A registry with fixed entries and tags.
#[derive(Default)]
struct FakeLookup {
    entries: BTreeSet<(Identifier, Identifier)>,
    tags: BTreeMap<(Identifier, Identifier), Vec<Identifier>>,
}

impl FakeLookup {
    fn items(names: &[&'static str], tags: &[(&'static str, &[&'static str])]) -> Self {
        let item = id("minecraft:item");
        Self {
            entries: names.iter().map(|name| (item.clone(), id(name))).collect(),
            tags: tags
                .iter()
                .map(|(tag, values)| {
                    (
                        (item.clone(), id(tag)),
                        values.iter().map(|value| id(value)).collect(),
                    )
                })
                .collect(),
        }
    }
}

impl Lookup for FakeLookup {
    fn contains(&self, registry: &Identifier, entry: &Identifier) -> bool {
        self.entries.contains(&(registry.clone(), entry.clone()))
    }

    fn tag(&self, registry: &Identifier, tag: &Identifier) -> Vec<Identifier> {
        self.tags
            .get(&(registry.clone(), tag.clone()))
            .cloned()
            .unwrap_or_default()
    }
}

fn lookup() -> FakeLookup {
    FakeLookup::items(
        &[
            "minecraft:stick",
            "minecraft:coal",
            "minecraft:oak_planks",
            "minecraft:birch_planks",
        ],
        &[(
            "minecraft:planks",
            &["minecraft:oak_planks", "minecraft:birch_planks"],
        )],
    )
}

fn write_file(root: &Path, relative: &str, json: &Value) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, json.to_string()).unwrap();
}

fn registered_with(types: &[&dyn AnyDataMapType]) -> Registered {
    let mut registered = Registered::default();
    for data_map in types {
        let info = data_map.type_info();
        registered
            .by_registry
            .entry(info.registry.clone())
            .or_default()
            .push(info.clone());
    }
    registered
}

fn packs(dir: &Path, names: &[&str]) -> Vec<Pack> {
    names
        .iter()
        .map(|name| Pack {
            id: format!("file/{name}"),
            root: dir.join(name),
        })
        .collect()
}

fn values<T: DataMapValue + Clone>(data_map: &DataMapType<T>) -> BTreeMap<String, T> {
    data_map
        .data_map()
        .into_iter()
        .map(|(entry, value)| (entry.to_string(), (*value).clone()))
        .collect()
}

fn list_map() -> AdvancedDataMapType<Vec<i32>, Without> {
    AdvancedDataMapType::builder(id("mapmod:lists"), id("minecraft:item"))
        .merger(ListMerger)
        .remover::<Without>()
        .build()
}

/// Removes one number from a list; an empty list loses its value.
#[derive(Debug, Clone)]
struct Without(i32);

impl Encode for Without {
    fn encode<O: pumpkin_codecs::DynamicOps>(
        &self,
        ops: &'static O,
        prefix: O::Value,
    ) -> pumpkin_codecs::DataResult<O::Value> {
        self.0.encode(ops, prefix)
    }
}

impl Decode for Without {
    fn decode<O: pumpkin_codecs::DynamicOps>(
        input: O::Value,
        ops: &'static O,
    ) -> pumpkin_codecs::DataResult<(Self, O::Value)> {
        i32::decode(input, ops).map(|(value, rest)| (Self(value), rest))
    }
}

impl DataMapValueRemover<Vec<i32>> for Without {
    fn remove(
        &self,
        value: &Vec<i32>,
        _: &Identifier,
        _: &DataMapSource,
        _: &Identifier,
    ) -> Option<Vec<i32>> {
        let kept: Vec<i32> = value.iter().copied().filter(|n| *n != self.0).collect();
        (!kept.is_empty()).then_some(kept)
    }
}

#[test]
fn loader_applies_files_in_pack_order_with_merger_replace_and_removals() {
    let dir = tempfile::tempdir().unwrap();
    let lists = list_map();
    let file = "data/mapmod/data_maps/item/lists.json";
    write_file(
        &dir.path().join("low"),
        file,
        &json!({
            "values": {
                "minecraft:stick": [1],
                "#minecraft:planks": [2],
                "minecraft:coal": [3],
                "minecraft:unknown": [9]
            }
        }),
    );
    write_file(
        &dir.path().join("high"),
        file,
        &json!({
            "values": {
                // Merged with the list merger.
                "minecraft:stick": [4],
                // `replace` on the entry skips the merger.
                "minecraft:oak_planks": { "value": [5], "replace": true }
            },
            "remove": [
                { "key": "minecraft:birch_planks" },
                { "key": "minecraft:stick", "remover": 1 }
            ]
        }),
    );
    let registered = registered_with(&[&*lists]);
    let updated = loader::load(&registered, &packs(dir.path(), &["low", "high"]), &lookup());
    assert_eq!(updated, [id("minecraft:item")]);
    assert_eq!(
        values(&lists),
        BTreeMap::from([
            ("minecraft:coal".to_owned(), vec![3]),
            ("minecraft:oak_planks".to_owned(), vec![5]),
            ("minecraft:stick".to_owned(), vec![4]),
        ])
    );
    assert_eq!(*lists.get_data(&id("minecraft:stick")).unwrap(), [4]);
    assert!(lists.get_data(&id("minecraft:birch_planks")).is_none());
    assert!(lists.get_data(&id("minecraft:unknown")).is_none());
}

#[test]
fn loader_reads_the_three_removal_forms_of_an_advanced_type() {
    let dir = tempfile::tempdir().unwrap();
    let lists = list_map();
    let file = "data/mapmod/data_maps/item/lists.json";
    let base = json!({ "values": { "minecraft:stick": [1, 2], "minecraft:coal": [1, 2], "#minecraft:planks": [1, 2] } });
    write_file(&dir.path().join("base"), file, &base);
    write_file(
        &dir.path().join("keys"),
        file,
        &json!({ "values": {}, "remove": ["#minecraft:planks"] }),
    );
    write_file(
        &dir.path().join("map"),
        file,
        &json!({ "values": {}, "remove": { "minecraft:stick": 1, "minecraft:coal": 2 } }),
    );
    let registered = registered_with(&[&*lists]);
    loader::load(
        &registered,
        &packs(dir.path(), &["base", "keys", "map"]),
        &lookup(),
    );
    assert_eq!(
        values(&lists),
        BTreeMap::from([
            ("minecraft:coal".to_owned(), vec![1]),
            ("minecraft:stick".to_owned(), vec![2]),
        ])
    );
}

#[test]
fn a_plain_type_only_reads_removal_keys() {
    let dir = tempfile::tempdir().unwrap();
    let plain = int_map("mapmod:burn", "minecraft:item");
    write_file(
        &dir.path().join("a"),
        "data/mapmod/data_maps/item/burn.json",
        &json!({ "values": { "minecraft:stick": 100 }, "remove": { "minecraft:stick": 1 } }),
    );
    write_file(
        &dir.path().join("b"),
        "data/mapmod/data_maps/item/burn.json",
        &json!({ "values": { "minecraft:coal": { "value": 1600 } }, "remove": ["minecraft:stick"] }),
    );
    let registered = registered_with(&[&plain]);
    loader::load(&registered, &packs(dir.path(), &["a", "b"]), &lookup());
    // The first file has a map in `remove` and does not load.
    assert_eq!(
        values(&plain),
        BTreeMap::from([("minecraft:coal".to_owned(), 1600)])
    );
}

#[test]
fn file_replace_drops_the_values_of_lower_packs() {
    let dir = tempfile::tempdir().unwrap();
    let plain = int_map("mapmod:burn", "minecraft:item");
    let file = "data/mapmod/data_maps/item/burn.json";
    write_file(
        &dir.path().join("a"),
        file,
        &json!({ "values": { "minecraft:stick": 100, "minecraft:coal": 1 } }),
    );
    write_file(
        &dir.path().join("b"),
        file,
        &json!({ "replace": true, "values": { "minecraft:coal": 2 } }),
    );
    let registered = registered_with(&[&plain]);
    loader::load(&registered, &packs(dir.path(), &["a", "b"]), &lookup());
    assert_eq!(
        values(&plain),
        BTreeMap::from([("minecraft:coal".to_owned(), 2)])
    );
}

#[test]
fn a_type_without_tag_support_drops_tag_values_but_not_tag_removals() {
    let dir = tempfile::tempdir().unwrap();
    let single: AdvancedDataMapType<i32> =
        AdvancedDataMapType::builder(id("mapmod:single"), id("minecraft:item"))
            .disable_tag_support("one block only")
            .build();
    let file = "data/mapmod/data_maps/item/single.json";
    write_file(
        &dir.path().join("a"),
        file,
        &json!({ "values": { "minecraft:stick": 1, "minecraft:oak_planks": 2, "#minecraft:planks": 3 } }),
    );
    write_file(
        &dir.path().join("b"),
        file,
        &json!({ "values": {}, "remove": ["#minecraft:planks"] }),
    );
    let registered = registered_with(&[&*single]);
    loader::load(&registered, &packs(dir.path(), &["a"]), &lookup());
    assert_eq!(
        values(&single),
        BTreeMap::from([
            ("minecraft:oak_planks".to_owned(), 2),
            ("minecraft:stick".to_owned(), 1)
        ])
    );
    loader::load(&registered, &packs(dir.path(), &["a", "b"]), &lookup());
    assert_eq!(
        values(&single),
        BTreeMap::from([("minecraft:stick".to_owned(), 1)])
    );
    assert_eq!(single.no_tags_reason(), Some("one block only"));
}

#[test]
#[should_panic(expected = "must not be blank")]
fn a_blank_no_tags_reason_panics() {
    let _ = AdvancedDataMapType::<i32>::builder(id("mapmod:x"), id("minecraft:item"))
        .disable_tag_support(" ");
}

#[test]
fn reload_keeps_registries_without_files_and_clears_types_without_files() {
    let dir = tempfile::tempdir().unwrap();
    let burn = int_map("mapmod:burn", "minecraft:item");
    let heat = int_map("mapmod:heat", "minecraft:item");
    let hardness = int_map("mapmod:hardness", "minecraft:block");
    write_file(
        &dir.path().join("a"),
        "data/mapmod/data_maps/item/burn.json",
        &json!({ "values": { "minecraft:stick": 1 } }),
    );
    write_file(
        &dir.path().join("a"),
        "data/mapmod/data_maps/item/heat.json",
        &json!({ "values": { "minecraft:stick": 2 } }),
    );
    write_file(
        &dir.path().join("a"),
        "data/mapmod/data_maps/block/hardness.json",
        &json!({ "values": {} }),
    );
    // A file for a map nobody registered, and one that does not parse.
    write_file(
        &dir.path().join("b"),
        "data/mapmod/data_maps/item/missing.json",
        &json!({ "values": {} }),
    );
    write_file(
        &dir.path().join("b"),
        "data/mapmod/data_maps/item/burn.json",
        &json!({ "values": 3 }),
    );
    let mut registered = registered_with(&[&burn, &heat, &hardness]);
    registered
        .by_registry
        .insert(id("minecraft:fluid"), Vec::new());
    let mut lookup = lookup();
    lookup
        .entries
        .insert((id("minecraft:block"), id("minecraft:stone")));
    let updated = loader::load(&registered, &packs(dir.path(), &["a", "b"]), &lookup);
    assert_eq!(updated, [id("minecraft:block"), id("minecraft:item")]);
    assert_eq!(*burn.get_data(&id("minecraft:stick")).unwrap(), 1);

    // The next load has only the burn file: the item registry clears `heat`, and the block
    // registry keeps its values, as `DataMapLoader.apply` only visits registries with a file.
    hardness.info.set_values(Values::from([(
        id("minecraft:stone"),
        Arc::new(5) as Stored,
    )]));
    fs::remove_file(dir.path().join("a/data/mapmod/data_maps/item/heat.json")).unwrap();
    fs::remove_file(
        dir.path()
            .join("a/data/mapmod/data_maps/block/hardness.json"),
    )
    .unwrap();
    let updated = loader::load(&registered, &packs(dir.path(), &["a"]), &lookup);
    assert_eq!(updated, [id("minecraft:item")]);
    assert!(heat.get_data(&id("minecraft:stick")).is_none());
    assert_eq!(*hardness.get_data(&id("minecraft:stone")).unwrap(), 5);
}

#[test]
fn loader_reads_nested_paths_modded_registries_and_skips_conditional_values() {
    let dir = tempfile::tempdir().unwrap();
    let nested = int_map("othermod:fuel/burn", "minecraft:item");
    write_file(
        &dir.path().join("a"),
        "data/othermod/data_maps/item/fuel/burn.json",
        &json!({ "values": {
            "minecraft:stick": 1,
            "minecraft:coal": { "neoforge:conditions": [{ "type": "neoforge:true" }], "value": 2 }
        } }),
    );
    let registered = registered_with(&[&nested]);
    loader::load(&registered, &packs(dir.path(), &["a"]), &lookup());
    assert_eq!(
        values(&nested),
        BTreeMap::from([("minecraft:stick".to_owned(), 1)])
    );
    assert_eq!(
        registries::folder_location(&id("mymod:widget")),
        "mymod/widget"
    );
    assert_eq!(
        registries::folder_location(&id("minecraft:worldgen/biome")),
        "worldgen/biome"
    );
}

#[test]
fn sync_sends_the_synced_maps_the_client_knows() {
    let synced: DataMapType<i32> = DataMapType::builder(id("mapmod:burn"), id("minecraft:item"))
        .synced(true)
        .build();
    let empty: DataMapType<i32> = DataMapType::builder(id("mapmod:empty"), id("minecraft:item"))
        .synced(false)
        .build();
    let hidden = int_map("mapmod:hidden", "minecraft:item");
    let other: DataMapType<i32> =
        DataMapType::builder(id("mapmod:hardness"), id("minecraft:block"))
            .synced(false)
            .build();
    synced.info.set_values(Values::from([
        (id("minecraft:stick"), Arc::new(100) as Stored),
        (id("minecraft:coal"), Arc::new(1600) as Stored),
    ]));
    hidden.info.set_values(Values::from([(
        id("minecraft:stick"),
        Arc::new(1) as Stored,
    )]));
    let registered = registered_with(&[&synced, &empty, &hidden, &other]);

    assert_eq!(
        sync::mandatory_data_maps(&registered),
        ["mapmod:burn (minecraft:item)"]
    );
    let known = sync::known_data_maps(&registered);
    let item: Vec<_> = known.data_maps[&id("minecraft:item")]
        .iter()
        .map(|map| (map.id.to_string(), map.mandatory))
        .collect();
    assert_eq!(
        item,
        [
            ("mapmod:burn".to_owned(), true),
            ("mapmod:empty".to_owned(), false)
        ]
    );

    let client = BTreeMap::from([
        (
            id("minecraft:item"),
            BTreeSet::from([
                id("mapmod:burn"),
                id("mapmod:empty"),
                id("mapmod:hidden"),
                id("mapmod:unknown"),
            ]),
        ),
        (id("minecraft:block"), BTreeSet::new()),
    ]);
    let payloads = sync::sync_payloads(&registered, &client);
    assert_eq!(payloads.len(), 1);
    let payload = decode_exact(&payloads[0], RegistryDataMapSyncPayload::read).unwrap();
    assert_eq!(payload.registry, id("minecraft:item"));
    let maps: Vec<(String, Vec<(String, String)>)> = payload
        .data_maps
        .iter()
        .map(|(map, values)| {
            (
                map.to_string(),
                values
                    .iter()
                    .map(|(entry, json)| (entry.to_string(), json.clone()))
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        maps,
        [
            (
                "mapmod:burn".to_owned(),
                vec![
                    ("minecraft:coal".to_owned(), "1600".to_owned()),
                    ("minecraft:stick".to_owned(), "100".to_owned()),
                ]
            ),
            ("mapmod:empty".to_owned(), Vec::new()),
        ]
    );
    // The bytes of `RegistryDataMapSyncPayload.STREAM_CODEC`: registry key, map size, then per
    // map its id, its size and per entry the key and the JSON as `writeUtf`.
    let mut expected = vec![0x0e];
    expected.extend_from_slice(b"minecraft:item");
    expected.push(0x02);
    expected.push(0x0b);
    expected.extend_from_slice(b"mapmod:burn");
    expected.push(0x02);
    expected.push(0x0e);
    expected.extend_from_slice(b"minecraft:coal");
    expected.push(0x04);
    expected.extend_from_slice(b"1600");
    expected.push(0x0f);
    expected.extend_from_slice(b"minecraft:stick");
    expected.push(0x03);
    expected.extend_from_slice(b"100");
    expected.push(0x0c);
    expected.extend_from_slice(b"mapmod:empty");
    expected.push(0x00);
    assert_eq!(payloads[0], expected);

    // A client that knows no map gets nothing.
    assert!(sync::sync_payloads(&registered, &BTreeMap::new()).is_empty());
}

fn parse<T: Decode>(json: Value) -> Result<T, String> {
    data_result(T::parse(json, &JsonOps))
}

fn encode<T: Encode>(value: &T) -> Value {
    data_result(value.encode_start(&JsonOps)).unwrap()
}

#[test]
fn builtin_values_read_both_forms_and_write_the_network_form() {
    let distance: AcceptableVillagerDistance =
        parse(json!({ "acceptable_villager_distance": 2.5 })).unwrap();
    assert_eq!(distance, parse(json!(2.5)).unwrap());
    // `DISTANCE_CODEC`, the network codec, is the bare float.
    assert_eq!(encode(&distance), json!(2.5));

    let compostable: VillagerCompostable = parse(json!({})).unwrap();
    assert!(!compostable.can_villager_compost);
    assert!(
        parse::<VillagerCompostable>(json!(true))
            .unwrap()
            .can_villager_compost
    );

    let frequency = parse::<builtin::VibrationFrequency>(json!({ "frequency": 16 }));
    assert_eq!(frequency.unwrap_err(), "Value 16 outside of range [1:15]");
    assert_eq!(
        encode(&parse::<builtin::VibrationFrequency>(json!(3)).unwrap()),
        json!(3)
    );

    let oxidizable: Oxidizable =
        parse(json!({ "next_oxidation_stage": "minecraft:exposed_copper" })).unwrap();
    assert!(oxidizable.generate_block_transform);
    // The default of an optional field is left out, as `optionalFieldOf(name, true)` writes it.
    assert_eq!(
        encode(&oxidizable),
        json!({ "next_oxidation_stage": "minecraft:exposed_copper" })
    );
    assert!(
        parse::<Oxidizable>(json!({ "next_oxidation_stage": "minecraft:no_such_block" })).is_err()
    );
}

#[test]
fn transformables_read_both_forms_and_their_remover_drops_one_transformer() {
    let data = json!({ "block_state_provider": { "type": "minecraft:simple_state_provider" } });
    let short: Transformable =
        parse(json!({ "transformer": "minecraft:axe", "transform_data": data })).unwrap();
    let long: Transformable =
        parse(json!({ "transformers": { "minecraft:axe": data, "minecraft:hoe": data } })).unwrap();
    assert_eq!(
        encode(&short),
        json!({ "transformer": "minecraft:axe", "transform_data": data })
    );
    assert_eq!(long.transformers.len(), 2);
    assert_eq!(encode(&long)["transformers"].as_object().unwrap().len(), 2);
    assert!(parse::<BlockTransformData>(json!({})).is_err());

    let remover: TransformableRemover = parse(json!("minecraft:axe")).unwrap();
    let source = DataMapSource::Entry(id("minecraft:oak_log"));
    let registry = id("minecraft:block");
    let entry = id("minecraft:oak_log");
    let kept = remover.remove(&long, &registry, &source, &entry).unwrap();
    assert_eq!(
        kept.transformers.keys().cloned().collect::<Vec<_>>(),
        [id("minecraft:hoe")]
    );
    assert!(remover.remove(&short, &registry, &source, &entry).is_none());
    let hoe: TransformableRemover = parse(json!("minecraft:shovel")).unwrap();
    assert_eq!(
        hoe.remove(&short, &registry, &source, &entry),
        Some(short.clone())
    );

    // The wrapped map merger of `TRANSFORMABLES` joins the transformers of both values.
    let hoe_only: Transformable =
        parse(json!({ "transformer": "minecraft:hoe", "transform_data": data })).unwrap();
    let merged = builtin::TRANSFORMABLES.info.ops.merge(
        &registry,
        &source,
        &(Arc::new(short) as Stored),
        &source,
        Arc::new(hoe_only) as Stored,
    );
    assert_eq!(
        downcast::<Transformable>(&merged)
            .unwrap()
            .transformers
            .len(),
        2
    );
}
