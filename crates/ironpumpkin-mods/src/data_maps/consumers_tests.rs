#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

//! The built-in data maps from the data packs to the vanilla consumers of the server.

use std::{
    fs,
    sync::{Mutex, PoisonError},
};

use pumpkin_core::plugin::data_maps::builtin::{self as consumer, BuiltinDataMaps, set_builtin};
use pumpkin_data::{
    Block, BlockDirection, BlockId,
    data_component_impl::BlockTransformerImpl,
    dynamic::ContentKind,
    entity::EntityType,
    game_event::GameEvent,
    item::Item,
    sound::Sound,
    villager::{VillagerProfession, VillagerType},
};
use pumpkin_util::identifier::Identifier;
use serde_json::{Value, json};

use super::{
    builtin::{OXIDIZABLES, WAXABLES},
    consumers::{collect, default_pack},
    loader::{self, Pack},
    registries::Lookup,
    *,
};

/// The built-in types hold their values in statics, and the server tables are a static too.
pub(super) static SERIAL: Mutex<()> = Mutex::new(());

struct Everything;

impl Lookup for Everything {
    fn contains(&self, registry: &Identifier, entry: &Identifier) -> bool {
        let name = entry.to_string();
        match registry.path() {
            "block" => crate::registry::contains(ContentKind::Block, &name),
            "item" => crate::registry::contains(ContentKind::Item, &name),
            "entity_type" => crate::registry::contains(ContentKind::EntityType, &name),
            "game_event" => pumpkin_data::game_event::GameEvent::from_name(&name).is_some(),
            "villager_profession" => VillagerProfession::from_name(&name).is_some(),
            "worldgen/biome" => pumpkin_data::chunk::Biome::from_name(entry.path()).is_some(),
            "block_transformer" => {
                pumpkin_data::block_transformer::get_block_transformer(&name).is_some()
            }
            _ => false,
        }
    }

    fn tag(&self, registry: &Identifier, tag: &Identifier) -> Vec<Identifier> {
        let kind = match registry.path() {
            "block" => ContentKind::Block,
            "item" => ContentKind::Item,
            "entity_type" => ContentKind::EntityType,
            _ => return Vec::new(),
        };
        crate::registry::tag_entries(kind, &tag.to_string()).unwrap_or_default()
    }
}

fn write(root: &std::path::Path, folder: &str, map: &str, json: &Value) {
    let path = root
        .join("data/neoforge/data_maps")
        .join(folder)
        .join(format!("{map}.json"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, json.to_string()).unwrap();
}

/// Loads the built-in types from the defaults of the server and a data pack with `files`
/// (`folder`, `map`, file), and returns what the server consumers get.
fn load(files: &[(&str, &str, Value)]) -> BuiltinDataMaps {
    let dir = tempfile::tempdir().unwrap();
    for (folder, map, json) in files {
        write(dir.path(), folder, map, json);
    }
    let target: &'static RwLock<Registered> = Box::leak(Box::default());
    post_with(&[], target, true).unwrap();
    let registered = target.read().unwrap_or_else(PoisonError::into_inner);
    let packs = [
        default_pack(),
        Pack {
            id: "file/test".to_owned(),
            root: Some(dir.path().to_owned()),
            memory: Vec::new(),
        },
    ];
    loader::load(&registered, &packs, &Everything);
    collect()
}

/// The built-in types hold no value before a load: other tests of the module read them empty.
fn clear_builtin_values() {
    let target: &'static RwLock<Registered> = Box::leak(Box::default());
    post_with(&[], target, true).unwrap();
    for info in target.read().unwrap().by_registry.values().flatten() {
        info.set_values(HashMap::new());
    }
}

/// Loads the files and installs the result for the consumers of the server until the end of `check`.
fn with_pack(files: &[(&str, &str, Value)], check: impl FnOnce(&BuiltinDataMaps)) {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let maps = load(files);
    set_builtin(maps.clone());
    check(&maps);
    set_builtin(BuiltinDataMaps::vanilla());
    clear_builtin_values();
}

/// The bytes of the sync of the defaults to a `NeoForge` client that names every built-in map:
/// the sum that the play boot test of `ironpumpkin-test-mod` reads on the wire.
#[test]
fn the_sync_of_the_defaults_has_the_size_of_the_play_boot_test() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    load(&[]);
    let target: &'static RwLock<Registered> = Box::leak(Box::default());
    post_with(&[], target, true).unwrap();
    let registered = target.read().unwrap();
    let client = registered
        .by_registry
        .iter()
        .map(|(registry, types)| {
            (
                registry.clone(),
                types.iter().map(|info| info.id.clone()).collect(),
            )
        })
        .collect();
    let payloads = sync::sync_payloads(&registered, &client);
    assert_eq!(payloads.len(), 6);
    let bytes: usize = payloads.iter().map(Vec::len).sum();
    drop(registered);
    clear_builtin_values();
    assert_eq!(bytes, 19152);
}

fn id(text: &'static str) -> Identifier {
    Identifier::parse_static(text)
}

#[test]
fn defaults_give_exposed_copper_for_copper_block() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    load(&[]);
    let oxidizable = OXIDIZABLES.get_data(&id("minecraft:copper_block")).unwrap();
    assert_eq!(
        oxidizable.next_oxidation_stage,
        id("minecraft:exposed_copper")
    );
    assert!(!oxidizable.generate_block_transform);
    let waxable = WAXABLES.get_data(&id("minecraft:copper_block")).unwrap();
    assert_eq!(waxed(&waxable), "minecraft:waxed_copper_block");
    assert_eq!(OXIDIZABLES.data_map().len(), 45);
    assert_eq!(WAXABLES.data_map().len(), 60);
}

fn waxed(waxable: &builtin::Waxable) -> String {
    waxable.waxed.to_string()
}

#[test]
fn the_defaults_load_to_the_values_of_vanilla() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let loaded = load(&[]);
    let vanilla = BuiltinDataMaps::vanilla();
    assert_eq!(
        loaded.acceptable_villager_distances,
        vanilla.acceptable_villager_distances
    );
    assert_eq!(loaded.monster_room_mobs, vanilla.monster_room_mobs);
    assert_eq!(loaded.oxidizables, vanilla.oxidizables);
    assert_eq!(loaded.waxables, vanilla.waxables);
    assert_eq!(loaded.parrot_imitations, vanilla.parrot_imitations);
    assert_eq!(loaded.raid_hero_gifts, vanilla.raid_hero_gifts);
    assert_eq!(loaded.vibration_frequencies, vanilla.vibration_frequencies);
    assert_eq!(loaded.villager_compostables, vanilla.villager_compostables);
    assert_eq!(loaded.villager_types, vanilla.villager_types);
    assert!(loaded.transform_appenders.is_empty() && loaded.transformables.is_empty());
}

#[test]
fn a_pack_changes_the_villager_distance() {
    with_pack(
        &[(
            "entity_type",
            "acceptable_villager_distances",
            json!({
                "values": { "minecraft:chicken": { "acceptable_villager_distance": 3.0 } },
                "remove": ["minecraft:pillager"],
            }),
        )],
        |_| {
            assert_eq!(
                consumer::acceptable_villager_distance(&EntityType::CHICKEN),
                Some(3.0)
            );
            assert_eq!(
                consumer::acceptable_villager_distance(&EntityType::PILLAGER),
                None
            );
            assert_eq!(
                consumer::acceptable_villager_distance(&EntityType::ZOMBIE),
                Some(8.0)
            );
        },
    );
}

#[test]
fn a_pack_changes_the_oxidation_of_a_block() {
    with_pack(
        &[(
            "block",
            "oxidizables",
            json!({
                "values": { "minecraft:stone": { "next_oxidation_stage": "minecraft:cobblestone" } },
                "remove": ["minecraft:copper_block"],
            }),
        )],
        |maps| {
            assert_eq!(
                consumer::next_oxidized_stage(&Block::STONE),
                Some(BlockId::COBBLESTONE)
            );
            assert_eq!(consumer::next_oxidized_stage(&Block::COPPER_BLOCK), None);
            // `generate_block_transform` is on by default: the axe scrapes the cobblestone.
            assert!(maps.oxidizables[&BlockId::STONE].generate_block_transform);
            let axe = BlockTransformerImpl {
                transformer: "minecraft:axe".into(),
            };
            let result = consumer::block_transformer(&axe)
                .unwrap()
                .transform(
                    &Block::COBBLESTONE,
                    Block::COBBLESTONE.default_state.id,
                    BlockDirection::Up,
                    &|_, _, _| &Block::COBBLESTONE,
                )
                .unwrap();
            assert_eq!(result.target_block.id, BlockId::STONE);
            assert_eq!(result.entry.sound, Some(Sound::ItemAxeScrape));
        },
    );
}

#[test]
fn a_pack_changes_the_wax_of_a_block() {
    with_pack(
        &[(
            "block",
            "waxables",
            json!({
                "values": {
                    "minecraft:stone": { "waxed": "minecraft:cobblestone", "generate_block_transform": false }
                },
            }),
        )],
        |_| {
            assert_eq!(
                consumer::waxed_block(&Block::STONE),
                Some(BlockId::COBBLESTONE)
            );
            assert_eq!(
                consumer::waxed_block(&Block::COPPER_BLOCK),
                Some(BlockId::WAXED_COPPER_BLOCK)
            );
        },
    );
}

#[test]
fn a_replacing_file_hides_the_defaults() {
    with_pack(
        &[(
            "block",
            "oxidizables",
            json!({ "replace": true, "values": {} }),
        )],
        |_| {
            assert_eq!(consumer::next_oxidized_stage(&Block::COPPER_BLOCK), None);
            assert_eq!(
                consumer::previous_oxidized_stage(&Block::EXPOSED_COPPER),
                None
            );
        },
    );
}

#[test]
fn a_pack_changes_the_monster_room_mobs() {
    with_pack(
        &[(
            "entity_type",
            "monster_room_mobs",
            json!({
                "values": { "minecraft:creeper": { "weight": 50 } },
                "remove": ["minecraft:skeleton"],
            }),
        )],
        |maps| {
            assert_eq!(
                maps.monster_room_mobs,
                vec![
                    ("minecraft:creeper".to_owned(), 50),
                    ("minecraft:spider".to_owned(), 100),
                    ("minecraft:zombie".to_owned(), 200),
                ]
            );
        },
    );
}

#[test]
fn a_pack_changes_the_parrot_imitations() {
    with_pack(
        &[(
            "entity_type",
            "parrot_imitations",
            json!({
                "values": { "minecraft:chicken": { "sound": "minecraft:entity.parrot.imitate.blaze" } },
            }),
        )],
        |_| {
            assert_eq!(
                consumer::parrot_imitation(&EntityType::CHICKEN),
                Sound::EntityParrotImitateBlaze
            );
            assert!(consumer::has_parrot_imitation(&EntityType::CHICKEN));
            assert!(consumer::has_parrot_imitation(&EntityType::CREEPER));
        },
    );
}

#[test]
fn a_pack_changes_the_hero_gifts() {
    with_pack(
        &[(
            "villager_profession",
            "raid_hero_gifts",
            json!({
                "values": { "minecraft:farmer": { "loot_table": "mymod:gifts/farmer" } },
            }),
        )],
        |_| {
            assert_eq!(
                consumer::raid_hero_gift(VillagerProfession::Farmer).as_deref(),
                Some("mymod:gifts/farmer")
            );
            assert_eq!(
                consumer::raid_hero_gift(VillagerProfession::Butcher).as_deref(),
                Some("minecraft:gameplay/hero_of_the_village/butcher_gift")
            );
        },
    );
}

#[test]
fn a_pack_changes_the_vibration_frequencies() {
    with_pack(
        &[(
            "game_event",
            "vibration_frequencies",
            json!({
                "values": { "minecraft:shriek": { "frequency": 5 } },
                "remove": ["minecraft:block_place"],
            }),
        )],
        |_| {
            assert_eq!(consumer::game_event_frequency(GameEvent::Shriek), 5);
            assert_eq!(consumer::game_event_frequency(GameEvent::BlockPlace), 0);
            assert_eq!(consumer::game_event_frequency(GameEvent::BlockDestroy), 12);
        },
    );
}

#[test]
fn a_pack_changes_what_villagers_compost() {
    with_pack(
        &[(
            "item",
            "villager_compostables",
            json!({
                "values": {
                    "minecraft:beetroot": true,
                    "minecraft:wheat_seeds": { "can_villager_compost": false },
                },
            }),
        )],
        |_| {
            assert!(consumer::villager_can_compost(&Item::BEETROOT));
            assert!(!consumer::villager_can_compost(&Item::WHEAT_SEEDS));
            assert!(consumer::villager_can_compost(&Item::BEETROOT_SEEDS));
        },
    );
}

#[test]
fn a_pack_changes_the_villager_type_of_a_biome() {
    with_pack(
        &[(
            "worldgen/biome",
            "villager_types",
            json!({
                "values": { "minecraft:plains": { "villager_type": "minecraft:swamp" } },
                "remove": ["minecraft:desert"],
            }),
        )],
        |_| {
            assert_eq!(
                consumer::villager_type_of_biome("plains"),
                Some(VillagerType::Swamp)
            );
            assert_eq!(consumer::villager_type_of_biome("desert"), None);
            assert_eq!(
                consumer::villager_type_of_biome("taiga"),
                Some(VillagerType::Taiga)
            );
        },
    );
}

fn dirt_to_stone() -> Value {
    json!({
        "block_state_provider": {
            "type": "minecraft:rule_based",
            "rules": [{
                "if_true": { "type": "minecraft:matching_blocks", "blocks": "minecraft:dirt" },
                "then": { "id": "minecraft:stone" },
            }],
        },
        "sound": "minecraft:item.axe.scrape",
        "particle": "scrape",
        "disallowed_faces": ["down"],
    })
}

fn axe_transform(block: &'static Block, face: BlockDirection) -> Option<BlockId> {
    let axe = BlockTransformerImpl {
        transformer: "axe".into(),
    };
    consumer::block_transformer(&axe)?
        .transform(block, block.default_state.id, face, &|_, _, _| block)
        .map(|result| result.target_block.id)
}

#[test]
fn a_pack_appends_a_transform_to_a_block_transformer() {
    with_pack(
        &[(
            "block_transformer",
            "block_transform_appenders",
            json!({ "values": { "minecraft:axe": [dirt_to_stone()] } }),
        )],
        |maps| {
            assert_eq!(maps.transform_appenders["minecraft:axe"].len(), 1);
            assert_eq!(
                axe_transform(&Block::DIRT, BlockDirection::Up),
                Some(BlockId::STONE)
            );
            assert_eq!(axe_transform(&Block::DIRT, BlockDirection::Down), None);
            assert_eq!(
                axe_transform(&Block::OAK_LOG, BlockDirection::Up),
                Some(BlockId::STRIPPED_OAK_LOG)
            );
        },
    );
}

#[test]
fn a_pack_gives_a_block_a_transform_and_drops_what_the_entry_cannot_hold() {
    with_pack(
        &[(
            "block",
            "transformables",
            json!({
                "values": {
                    "minecraft:dirt": {
                        "transformer": "minecraft:axe",
                        "transform_data": dirt_to_stone(),
                    },
                    "minecraft:gravel": {
                        "transformer": "minecraft:hoe",
                        "transform_data": {
                            "block_state_provider": {
                                "type": "minecraft:rule_based",
                                "rules": [{
                                    "if_true": { "type": "minecraft:true" },
                                    "then": { "id": "minecraft:stone" },
                                }],
                            },
                        },
                    },
                },
            }),
        )],
        |maps| {
            assert_eq!(maps.transformables["minecraft:axe"].len(), 1);
            assert!(!maps.transformables.contains_key("minecraft:hoe"));
            assert_eq!(
                axe_transform(&Block::DIRT, BlockDirection::Up),
                Some(BlockId::STONE)
            );
        },
    );
}
