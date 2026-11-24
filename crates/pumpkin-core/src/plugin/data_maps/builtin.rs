//! The `NeoForge` built-in data maps, in the form that the vanilla consumers read.
//!
//! `NeoForge`: `net.neoforged.neoforge.registries.datamaps.builtin.NeoForgeDataMaps`. The loader of
//! `ironpumpkin-mods` reads the data map files and installs the result with [`set_builtin`]. Until
//! the first load the maps hold the values of vanilla ([`BuiltinDataMaps::vanilla`]), so a server
//! without a data map host behaves as vanilla. The values of vanilla also come to the loader as
//! its first, lowest priority file ([`default_files`]): they are the `data/neoforge/data_maps`
//! files of `NeoForge`, built from the vanilla code and data of this crate.
//!
//! A consumer reads a map where `NeoForge` reads it. A map that `NeoForge` reads with a vanilla
//! fallback (the villager distances, the hero gifts, the villager types, the parrots) keeps the
//! vanilla table in its consumer, and the other consumers read only the map.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::{Arc, LazyLock, Mutex, PoisonError},
};

use arc_swap::{ArcSwap, Guard};
use pumpkin_data::{
    Block, BlockId,
    block_transformer::{
        BlockPredicate, BlockTransformer, BlockTransformerEntry, BlockTransformerRule,
        BlockTransformerStateProvider, get_block_transformer, get_next_weathering, get_waxed,
    },
    chunk::Biome,
    data_component_impl::BlockTransformerImpl,
    entity::EntityType,
    game_event::GameEvent,
    item::Item,
    sound::Sound,
    tag,
    villager::{VillagerProfession, VillagerType},
    world::WorldEvent,
};
use serde_json::{Map, Value, json};

use crate::entity::{
    ai::brain::{
        behavior::{interact::give_gift_to_hero, work::work_at_composter},
        sensing::villager_hostiles::ACCEPTABLE_DISTANCE_FROM_HOSTILES,
    },
    passive::{parrot, villager::breeding},
};

/// The path of the key of `BlockTransformers.AXE` (`minecraft:axe`): the transformer that the
/// generated scrape and wax off transforms join.
const AXE: &str = "axe";

/// `Oxidizable` and `Waxable`: the block that follows, and whether the axe gets a transform that
/// undoes the step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockStage {
    pub target: BlockId,
    pub generate_block_transform: bool,
}

/// The values of the eleven `NeoForge` built-in data maps.
#[derive(Debug, Clone, Default)]
pub struct BuiltinDataMaps {
    /// `ACCEPTABLE_VILLAGER_DISTANCES`, by entity type id.
    pub acceptable_villager_distances: HashMap<u16, f32>,
    /// `MONSTER_ROOM_MOBS`: the entity type name and its weight.
    pub monster_room_mobs: Vec<(String, i32)>,
    /// `OXIDIZABLES`.
    pub oxidizables: BTreeMap<BlockId, BlockStage>,
    /// `WAXABLES`.
    pub waxables: BTreeMap<BlockId, BlockStage>,
    /// `PARROT_IMITATIONS`, by entity type id.
    pub parrot_imitations: HashMap<u16, Sound>,
    /// `RAID_HERO_GIFTS`: the loot table name.
    pub raid_hero_gifts: HashMap<VillagerProfession, String>,
    /// `VIBRATION_FREQUENCIES`.
    pub vibration_frequencies: HashMap<GameEvent, i32>,
    /// `VILLAGER_COMPOSTABLES`: the ids of the items with `can_villager_compost`.
    pub villager_compostables: HashSet<u16>,
    /// `VILLAGER_TYPES`, by the registry id of the biome (`plains`).
    pub villager_types: HashMap<String, VillagerType>,
    /// `BLOCK_TRANSFORM_APPENDERS`: the transforms that each block transformer gets, by its key
    /// (`minecraft:axe`).
    pub transform_appenders: BTreeMap<String, Vec<&'static BlockTransformerEntry>>,
    /// `TRANSFORMABLES`: the transforms of the blocks, by the key of the block transformer.
    pub transformables: BTreeMap<String, Vec<&'static BlockTransformerEntry>>,
    derived: Derived,
}

/// What [`BuiltinDataMaps::seal`] computes from the maps.
#[derive(Debug, Clone, Default)]
struct Derived {
    /// `DataMapHooks.INVERSE_OXIDIZABLES_DATAMAP`.
    inverse_oxidizables: HashMap<BlockId, BlockId>,
    /// The block transformers that data maps extend, by the path of the key (`axe`). A
    /// transformer without an addition is the static one of `pumpkin-data`.
    transformers: HashMap<String, &'static BlockTransformer>,
}

impl BuiltinDataMaps {
    /// The values of vanilla, which are the `NeoForge` defaults. They come from the tables of the
    /// vanilla consumers and from the generated data of `pumpkin-data`.
    #[must_use]
    pub fn vanilla() -> Self {
        let mut maps = Self::default();
        for (entity_type, distance) in ACCEPTABLE_DISTANCE_FROM_HOSTILES {
            maps.acceptable_villager_distances
                .insert(entity_type.id, *distance);
        }
        maps.monster_room_mobs =
            pumpkin_world::generation::feature::features::monster_room::vanilla_monster_room_mobs();
        for index in 0..BlockId::COUNT {
            let Some(id) = BlockId::new(index) else {
                continue;
            };
            let stage = |target: BlockId| BlockStage {
                target,
                generate_block_transform: false,
            };
            if let Some(next) = get_next_weathering(id) {
                maps.oxidizables.insert(id, stage(next));
            }
            if let Some(waxed) = get_waxed(id) {
                maps.waxables.insert(id, stage(waxed));
            }
        }
        for (entity_type, sound) in parrot::IMITATIONS {
            maps.parrot_imitations.insert(entity_type.id, *sound);
        }
        for id in 0.. {
            let Some(profession) = VillagerProfession::from_i32(id) else {
                break;
            };
            if matches!(
                profession,
                VillagerProfession::None | VillagerProfession::Nitwit
            ) {
                continue;
            }
            maps.raid_hero_gifts.insert(
                profession,
                give_gift_to_hero::vanilla_loot_table(profession).to_owned(),
            );
        }
        for name in tag::GameEvent::MINECRAFT_VIBRATIONS.0 {
            if let Some(event) = GameEvent::from_name(name)
                && event.vibration_frequency() > 0
            {
                maps.vibration_frequencies
                    .insert(event, i32::from(event.vibration_frequency()));
            }
        }
        for item in work_at_composter::COMPOSTABLE_ITEMS {
            maps.villager_compostables.insert(item.id);
        }
        for id in 0..=u8::MAX {
            let Some(biome) = Biome::from_id(id) else {
                break;
            };
            let villager_type = breeding::vanilla_villager_type_for_biome(biome.registry_id);
            if villager_type != VillagerType::Plains {
                maps.villager_types
                    .insert(biome.registry_id.to_owned(), villager_type);
            }
        }
        maps.seal()
    }

    /// Computes the inverse maps and the block transformers that the maps extend. Call it after
    /// the last change to a field.
    #[must_use]
    pub fn seal(mut self) -> Self {
        let mut derived = Derived::default();
        for (block, stage) in &self.oxidizables {
            derived.inverse_oxidizables.insert(stage.target, *block);
        }

        let generated = self.generated_axe_entries();
        let appenders = by_transformer(&self.transform_appenders);
        let transformables = by_transformer(&self.transformables);
        let mut keys: BTreeSet<&str> = appenders
            .keys()
            .chain(transformables.keys())
            .map(String::as_str)
            .collect();
        if !generated.is_empty() {
            keys.insert(AXE);
        }
        for key in keys {
            let Some(base) = get_block_transformer(key) else {
                continue;
            };
            // `Iterables.concat(toolTransformers, appenders, DATAMAP_BLOCK_TRANSFORMERS)`.
            let mut added: Vec<&'static BlockTransformerEntry> =
                appenders.get(key).cloned().unwrap_or_default();
            if key == AXE {
                added.extend(generated.iter().copied());
            }
            added.extend(transformables.get(key).cloned().unwrap_or_default());
            derived
                .transformers
                .insert(key.to_owned(), extended_transformer(key, base, &added));
        }
        self.derived = derived;
        self
    }

    /// The transforms that `DataMapHooks.onDataMapsUpdated` builds for an oxidizable or a waxable
    /// with `generate_block_transform`.
    fn generated_axe_entries(&self) -> Vec<&'static BlockTransformerEntry> {
        let mut entries = Vec::new();
        for (block, stage) in &self.oxidizables {
            if stage.generate_block_transform {
                entries.push(generated_entry(*block, stage.target, false));
            }
        }
        for (block, stage) in &self.waxables {
            if stage.generate_block_transform {
                entries.push(generated_entry(*block, stage.target, true));
            }
        }
        entries
    }
}

/// The transforms of a map by the path of the transformer: `axe` and `minecraft:axe` name the
/// same transformer, and their transforms join in key order.
fn by_transformer(
    map: &BTreeMap<String, Vec<&'static BlockTransformerEntry>>,
) -> BTreeMap<String, Vec<&'static BlockTransformerEntry>> {
    let mut merged: BTreeMap<String, Vec<&'static BlockTransformerEntry>> = BTreeMap::new();
    for (key, entries) in map {
        merged
            .entry(transformer_path(key).to_owned())
            .or_default()
            .extend(entries);
    }
    merged
}

/// Leaks a value for the life of the server, once for each `key`: a load with the same data
/// finds the value of the load before.
fn interned<K: std::hash::Hash + Eq, V: Copy>(
    table: &LazyLock<Mutex<HashMap<K, V>>>,
    key: K,
    make: impl FnOnce() -> V,
) -> V {
    *table
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(key)
        .or_insert_with(make)
}

type Interned<K, V> = LazyLock<Mutex<HashMap<K, V>>>;

/// The generated entries by block, target and kind (`false` scrapes, `true` waxes off).
static GENERATED: Interned<(BlockId, BlockId, bool), &'static BlockTransformerEntry> =
    LazyLock::new(Mutex::default);

/// The extended transformers by key and the addresses of their added entries.
static TRANSFORMERS: Interned<(String, Vec<usize>), &'static BlockTransformer> =
    LazyLock::new(Mutex::default);

/// The transform that `DataMapHooks.onDataMapsUpdated` builds for a step from `block` to `target`:
/// the axe turns `target` into `block`.
fn generated_entry(
    block: BlockId,
    target: BlockId,
    wax_off: bool,
) -> &'static BlockTransformerEntry {
    interned(&GENERATED, (block, target, wax_off), || {
        let rules: &'static [BlockTransformerRule] = Box::leak(Box::new([BlockTransformerRule {
            predicate: BlockPredicate::MatchingBlocks {
                blocks: Box::leak(Box::new([target])),
                offset: (0, 0, 0),
            },
            provider: BlockTransformerStateProvider::SimpleState(block),
        }]));
        let (sound, particle) = if wax_off {
            (Sound::ItemAxeWaxOff, WorldEvent::ParticlesWaxOff)
        } else {
            (Sound::ItemAxeScrape, WorldEvent::ParticlesScrape)
        };
        Box::leak(Box::new(BlockTransformerEntry {
            rules,
            disallowed_faces: &[],
            item_damage_per_use: 1,
            sound: Some(sound),
            particle: Some(particle),
            loot: None,
            drop_strategy: None,
            transform_type: None,
            update_from_neighbors: true,
        }))
    })
}

/// The transformer `base` with the `added` entries after its own.
fn extended_transformer(
    key: &str,
    base: &BlockTransformer,
    added: &[&'static BlockTransformerEntry],
) -> &'static BlockTransformer {
    let addresses = added
        .iter()
        .map(|entry| std::ptr::from_ref::<BlockTransformerEntry>(entry) as usize)
        .collect();
    interned(&TRANSFORMERS, (key.to_owned(), addresses), || {
        let mut entries: Vec<BlockTransformerEntry> = base.entries.to_vec();
        entries.extend(added.iter().map(|entry| **entry));
        let entries: &'static [BlockTransformerEntry] = Box::leak(entries.into_boxed_slice());
        Box::leak(Box::new(BlockTransformer { entries }))
    })
}

/// The path of a transformer key: `axe` for `axe` and `minecraft:axe`.
fn transformer_path(key: &str) -> &str {
    key.strip_prefix("minecraft:").unwrap_or(key)
}

static BUILTIN: LazyLock<ArcSwap<BuiltinDataMaps>> =
    LazyLock::new(|| ArcSwap::from_pointee(BuiltinDataMaps::vanilla()));

/// The values that the consumers read now.
#[must_use]
pub fn builtin() -> Guard<Arc<BuiltinDataMaps>> {
    BUILTIN.load()
}

/// Installs the values of a data map load, as the `DataMapsUpdatedEvent` handlers of `NeoForge`
/// (`DataMapHooks`, `MonsterRoomHooks`) take them.
pub fn set_builtin(maps: BuiltinDataMaps) {
    pumpkin_world::generation::feature::features::monster_room::set_monster_room_mobs(
        maps.monster_room_mobs.clone(),
    );
    BUILTIN.store(Arc::new(maps.seal()));
}

/// `VillagerHostilesSensor`: `mob.getData(ACCEPTABLE_VILLAGER_DISTANCES)`.
#[must_use]
pub fn acceptable_villager_distance(entity_type: &EntityType) -> Option<f32> {
    builtin()
        .acceptable_villager_distances
        .get(&entity_type.id)
        .copied()
}

/// `DataMapHooks.getNextOxidizedStage`.
#[must_use]
pub fn next_oxidized_stage(block: &Block) -> Option<BlockId> {
    builtin()
        .oxidizables
        .get(&block.id)
        .map(|stage| stage.target)
}

/// `DataMapHooks.getPreviousOxidizedStage`.
#[must_use]
pub fn previous_oxidized_stage(block: &Block) -> Option<BlockId> {
    builtin()
        .derived
        .inverse_oxidizables
        .get(&block.id)
        .copied()
}

/// The most stages that [`first_oxidized_stage`] walks back: a data pack can make a cycle.
const MAX_OXIDATION_STEPS: usize = 64;

/// `WeatheringCopper.getFirst`: the block that the previous stages lead back to, or the block.
///
/// The walk stops after [`MAX_OXIDATION_STEPS`] steps, so a cyclic map cannot hang the tick.
#[must_use]
pub fn first_oxidized_stage(block: &Block) -> BlockId {
    let maps = builtin();
    let mut first = block.id;
    for _ in 0..MAX_OXIDATION_STEPS {
        match maps.derived.inverse_oxidizables.get(&first) {
            Some(previous) => first = *previous,
            None => break,
        }
    }
    first
}

/// `DataMapHooks.getBlockWaxed`.
#[must_use]
pub fn waxed_block(block: &Block) -> Option<BlockId> {
    builtin().waxables.get(&block.id).map(|stage| stage.target)
}

/// `Parrot.getImitatedSound`: the sound of the data map, or the ambient sound of a parrot.
#[must_use]
pub fn parrot_imitation(entity_type: &EntityType) -> Sound {
    builtin()
        .parrot_imitations
        .get(&entity_type.id)
        .copied()
        .unwrap_or(Sound::EntityParrotAmbient)
}

/// Whether the `parrot_imitations` data map names the entity type.
#[must_use]
pub fn has_parrot_imitation(entity_type: &EntityType) -> bool {
    builtin().parrot_imitations.contains_key(&entity_type.id)
}

/// The entity types that a parrot imitates: those of the data map and those of the vanilla table,
/// ordered by id. `Parrot.getAmbient` picks one of them, and `NOT_PARROT_PREDICATE` accepts them.
#[must_use]
pub fn parrot_imitated_types() -> Vec<&'static EntityType> {
    let mut ids: BTreeSet<u16> = builtin().parrot_imitations.keys().copied().collect();
    ids.extend(
        parrot::IMITATIONS
            .iter()
            .map(|(entity_type, _)| entity_type.id),
    );
    ids.into_iter().filter_map(EntityType::from_raw).collect()
}

/// `GiveGiftToHero.getLootTableToThrow`: the loot table of the data map for the profession.
#[must_use]
pub fn raid_hero_gift(profession: VillagerProfession) -> Option<String> {
    builtin().raid_hero_gifts.get(&profession).cloned()
}

/// `VibrationSystem.getGameEventFrequency`: 0 for an event without a value.
#[must_use]
pub fn game_event_frequency(event: GameEvent) -> i32 {
    builtin()
        .vibration_frequencies
        .get(&event)
        .copied()
        .unwrap_or(0)
}

/// `WorkAtComposter`: `itemStack.getData(VILLAGER_COMPOSTABLES).canVillagerCompost()`.
#[must_use]
pub fn villager_can_compost(item: &Item) -> bool {
    builtin().villager_compostables.contains(&item.id)
}

/// `VillagerType.byBiome`: the type of the data map for the biome.
#[must_use]
pub fn villager_type_of_biome(registry_id: &str) -> Option<VillagerType> {
    builtin().villager_types.get(registry_id).copied()
}

/// The block transformer of an item: the static one of `pumpkin-data`, extended by the data maps
/// (`DataMapHooks.appendDatamapTransformers`).
#[must_use]
pub fn block_transformer(component: &BlockTransformerImpl) -> Option<&'static BlockTransformer> {
    let maps = builtin();
    if !maps.derived.transformers.is_empty()
        && let Some(extended) = maps
            .derived
            .transformers
            .get(transformer_path(&component.transformer))
    {
        return Some(extended);
    }
    component.get_transformer()
}

/// A data map file that the server gives to the loader before the files of the data packs.
#[derive(Debug, Clone)]
pub struct DefaultFile {
    /// The registry folder: `block`, `worldgen/biome`.
    pub folder: &'static str,
    /// The map id: `neoforge:oxidizables`.
    pub map: &'static str,
    /// The JSON text of the file.
    pub text: String,
}

fn namespaced(name: &str) -> String {
    format!("minecraft:{name}")
}

fn block_name(id: BlockId) -> String {
    namespaced(id.to_block().name)
}

fn file(folder: &'static str, map: &'static str, values: &Map<String, Value>) -> DefaultFile {
    DefaultFile {
        folder,
        map,
        text: json!({ "values": values }).to_string(),
    }
}

/// The entries of a map as a JSON object in the order of their ids.
fn by_id(mut entries: Vec<(String, Value)>) -> Map<String, Value> {
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.into_iter().collect()
}

fn entity_name(id: u16) -> Option<String> {
    EntityType::from_raw(id).map(|entity_type| namespaced(entity_type.resource_name))
}

/// `OXIDIZABLES` and `WAXABLES`: the block that follows is in the field `field`.
fn stages_json(stages: &BTreeMap<BlockId, BlockStage>, field: &str) -> Map<String, Value> {
    stages
        .iter()
        .map(|(block, stage)| {
            (
                block_name(*block),
                json!({
                    field: block_name(stage.target),
                    "generate_block_transform": stage.generate_block_transform,
                }),
            )
        })
        .collect()
}

/// The files of the entity type maps.
fn entity_type_files(vanilla: &BuiltinDataMaps) -> [DefaultFile; 3] {
    let distances = vanilla
        .acceptable_villager_distances
        .iter()
        .filter_map(|(id, distance)| {
            Some((
                entity_name(*id)?,
                json!({ "acceptable_villager_distance": distance }),
            ))
        })
        .collect();
    let mobs = vanilla
        .monster_room_mobs
        .iter()
        .map(|(name, weight)| (name.clone(), json!({ "weight": weight })))
        .collect();
    let imitations = vanilla
        .parrot_imitations
        .iter()
        .filter_map(|(id, sound)| {
            Some((
                entity_name(*id)?,
                json!({ "sound": namespaced(sound.to_name()) }),
            ))
        })
        .collect();
    [
        file(
            "entity_type",
            "neoforge:acceptable_villager_distances",
            &by_id(distances),
        ),
        file("entity_type", "neoforge:monster_room_mobs", &by_id(mobs)),
        file(
            "entity_type",
            "neoforge:parrot_imitations",
            &by_id(imitations),
        ),
    ]
}

/// The files of the maps of the other registries.
fn other_files(vanilla: &BuiltinDataMaps) -> [DefaultFile; 4] {
    let gifts = vanilla
        .raid_hero_gifts
        .iter()
        .map(|(profession, table)| {
            (
                namespaced(profession.to_name()),
                json!({ "loot_table": table }),
            )
        })
        .collect();
    let frequencies = vanilla
        .vibration_frequencies
        .iter()
        .map(|(event, frequency)| (namespaced(event.name()), json!({ "frequency": frequency })))
        .collect();
    let compostables = vanilla
        .villager_compostables
        .iter()
        .filter_map(|id| {
            Some((
                namespaced(Item::from_id(*id)?.registry_key),
                json!({ "can_villager_compost": true }),
            ))
        })
        .collect();
    let types = vanilla
        .villager_types
        .iter()
        .map(|(biome, villager_type)| {
            (
                namespaced(biome),
                json!({ "villager_type": namespaced(villager_type.to_name()) }),
            )
        })
        .collect();
    [
        file(
            "game_event",
            "neoforge:vibration_frequencies",
            &by_id(frequencies),
        ),
        file(
            "item",
            "neoforge:villager_compostables",
            &by_id(compostables),
        ),
        file(
            "villager_profession",
            "neoforge:raid_hero_gifts",
            &by_id(gifts),
        ),
        file("worldgen/biome", "neoforge:villager_types", &by_id(types)),
    ]
}

/// The `NeoForge` files of `data/neoforge/data_maps`, built from [`BuiltinDataMaps::vanilla`].
/// Their order is the order of the files of a data pack.
#[must_use]
pub fn default_files() -> Vec<DefaultFile> {
    let vanilla = BuiltinDataMaps::vanilla();
    let mut files = vec![
        file(
            "block",
            "neoforge:oxidizables",
            &stages_json(&vanilla.oxidizables, "next_oxidation_stage"),
        ),
        file(
            "block",
            "neoforge:waxables",
            &stages_json(&vanilla.waxables, "waxed"),
        ),
    ];
    files.extend(entity_type_files(&vanilla));
    files.extend(other_files(&vanilla));
    files
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Mutex, MutexGuard, PoisonError};

    use super::{BuiltinDataMaps, set_builtin};

    /// The data maps are a process-wide static: a test that reads or sets them holds this guard,
    /// and the maps return to vanilla when it drops.
    pub struct Installed(
        #[expect(dead_code, reason = "the guard serializes the tests")] MutexGuard<'static, ()>,
    );

    static LOCK: Mutex<()> = Mutex::new(());

    /// Takes the lock and installs `maps` until the guard drops.
    pub fn install(maps: BuiltinDataMaps) -> Installed {
        let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        set_builtin(maps);
        Installed(guard)
    }

    impl Installed {
        /// Installs other maps under the same lock.
        #[expect(clippy::unused_self, reason = "the guard proves that the lock is held")]
        pub fn replace(&self, maps: BuiltinDataMaps) {
            set_builtin(maps);
        }
    }

    impl Drop for Installed {
        fn drop(&mut self) {
            set_builtin(BuiltinDataMaps::vanilla());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use pumpkin_data::BlockDirection;

    use super::{test_support::install, *};

    /// A transform that turns `from` into `to`, the way a mod gives it.
    fn transform(from: BlockId, to: BlockId) -> &'static BlockTransformerEntry {
        let rules: &'static [BlockTransformerRule] = Box::leak(Box::new([BlockTransformerRule {
            predicate: BlockPredicate::MatchingBlocks {
                blocks: Box::leak(Box::new([from])),
                offset: (0, 0, 0),
            },
            provider: BlockTransformerStateProvider::SimpleState(to),
        }]));
        Box::leak(Box::new(BlockTransformerEntry {
            rules,
            disallowed_faces: &[],
            item_damage_per_use: 1,
            sound: Some(Sound::ItemAxeScrape),
            particle: None,
            loot: None,
            drop_strategy: None,
            transform_type: None,
            update_from_neighbors: true,
        }))
    }

    fn axe_result(
        block: &'static Block,
    ) -> Option<pumpkin_data::block_transformer::TransformResult> {
        let component = BlockTransformerImpl {
            transformer: Cow::Borrowed("minecraft:axe"),
        };
        let transformer = block_transformer(&component)?;
        transformer.transform(
            block,
            block.default_state.id,
            BlockDirection::Up,
            &|_, _, _| block,
        )
    }

    #[test]
    fn a_data_map_oxidizable_changes_the_next_and_previous_stage() {
        let installed = install(BuiltinDataMaps::vanilla());
        assert_eq!(next_oxidized_stage(&Block::STONE), None);
        let mut maps = BuiltinDataMaps::vanilla();
        maps.oxidizables.insert(
            BlockId::STONE,
            BlockStage {
                target: BlockId::COBBLESTONE,
                generate_block_transform: false,
            },
        );
        installed.replace(maps);
        assert_eq!(
            next_oxidized_stage(&Block::STONE),
            Some(BlockId::COBBLESTONE)
        );
        assert_eq!(
            previous_oxidized_stage(&Block::COBBLESTONE),
            Some(BlockId::STONE)
        );
        assert_eq!(
            crate::block::blocks::weathering_copper::get_next(&Block::STONE),
            Some(&Block::COBBLESTONE)
        );
        // `generate_block_transform` is off: the axe has no new transform.
        assert!(axe_result(&Block::COBBLESTONE).is_none());
    }

    #[test]
    fn an_oxidizable_with_generate_block_transform_gives_the_axe_a_scrape() {
        let mut maps = BuiltinDataMaps::vanilla();
        maps.oxidizables.insert(
            BlockId::STONE,
            BlockStage {
                target: BlockId::COBBLESTONE,
                generate_block_transform: true,
            },
        );
        let _installed = install(maps);
        let result = axe_result(&Block::COBBLESTONE).expect("the scrape transform");
        assert_eq!(result.target_block.id, BlockId::STONE);
        assert_eq!(result.entry.sound, Some(Sound::ItemAxeScrape));
        assert_eq!(result.entry.particle, Some(WorldEvent::ParticlesScrape));
    }

    #[test]
    fn a_waxable_with_generate_block_transform_gives_the_axe_a_wax_off() {
        let installed = install(BuiltinDataMaps::vanilla());
        assert_eq!(waxed_block(&Block::STONE), None);
        let mut maps = BuiltinDataMaps::vanilla();
        maps.waxables.insert(
            BlockId::STONE,
            BlockStage {
                target: BlockId::COBBLESTONE,
                generate_block_transform: true,
            },
        );
        installed.replace(maps);
        assert_eq!(waxed_block(&Block::STONE), Some(BlockId::COBBLESTONE));
        let result = axe_result(&Block::COBBLESTONE).expect("the wax off transform");
        assert_eq!(result.target_block.id, BlockId::STONE);
        assert_eq!(result.entry.sound, Some(Sound::ItemAxeWaxOff));
        assert_eq!(result.entry.particle, Some(WorldEvent::ParticlesWaxOff));
    }

    #[test]
    fn appenders_and_transformables_extend_the_block_transformer_after_the_vanilla_entries() {
        let mut maps = BuiltinDataMaps::vanilla();
        maps.transform_appenders.insert(
            "minecraft:axe".to_owned(),
            vec![transform(BlockId::DIRT, BlockId::STONE)],
        );
        maps.transformables.insert(
            "minecraft:axe".to_owned(),
            vec![transform(BlockId::GRAVEL, BlockId::SAND)],
        );
        let _installed = install(maps);
        let appended = axe_result(&Block::DIRT).expect("the appended transform");
        assert_eq!(appended.target_block.id, BlockId::STONE);
        let transformable = axe_result(&Block::GRAVEL).expect("the transformable");
        assert_eq!(transformable.target_block.id, BlockId::SAND);
        // The vanilla entries stay first.
        let log = axe_result(&Block::OAK_LOG).expect("the vanilla strip");
        assert_eq!(log.target_block.id, BlockId::STRIPPED_OAK_LOG);
    }

    #[test]
    fn a_data_map_frequency_sets_the_vibration_and_a_missing_event_has_none() {
        let installed = install(BuiltinDataMaps::vanilla());
        assert_eq!(game_event_frequency(GameEvent::BlockPlace), 13);
        assert_eq!(game_event_frequency(GameEvent::Shriek), 0);
        let mut maps = BuiltinDataMaps::vanilla();
        maps.vibration_frequencies.insert(GameEvent::Shriek, 5);
        maps.vibration_frequencies.remove(&GameEvent::BlockPlace);
        installed.replace(maps);
        assert_eq!(game_event_frequency(GameEvent::Shriek), 5);
        // `NeoForge` has no fallback for a missing event.
        assert_eq!(game_event_frequency(GameEvent::BlockPlace), 0);
    }

    #[test]
    fn a_data_map_item_is_compostable_by_villagers_and_vanilla_has_no_fallback() {
        let installed = install(BuiltinDataMaps::vanilla());
        assert!(villager_can_compost(&Item::WHEAT_SEEDS));
        assert!(!villager_can_compost(&Item::BEETROOT));
        let mut maps = BuiltinDataMaps::vanilla();
        maps.villager_compostables.insert(Item::BEETROOT.id);
        maps.villager_compostables.remove(&Item::WHEAT_SEEDS.id);
        installed.replace(maps);
        assert!(villager_can_compost(&Item::BEETROOT));
        assert!(!villager_can_compost(&Item::WHEAT_SEEDS));
    }

    #[test]
    fn oxidizables_give_the_next_stage_of_copper() {
        let _installed = install(BuiltinDataMaps::vanilla());
        let maps = BuiltinDataMaps::vanilla();
        assert_eq!(
            maps.oxidizables
                .get(&BlockId::COPPER_BLOCK)
                .map(|s| s.target),
            Some(BlockId::EXPOSED_COPPER)
        );
        assert_eq!(
            next_oxidized_stage(&Block::COPPER_BLOCK),
            Some(BlockId::EXPOSED_COPPER)
        );
        assert_eq!(
            previous_oxidized_stage(&Block::EXPOSED_COPPER),
            Some(BlockId::COPPER_BLOCK)
        );
        assert_eq!(
            waxed_block(&Block::COPPER_BLOCK),
            Some(BlockId::WAXED_COPPER_BLOCK)
        );
    }

    #[test]
    fn the_default_files_hold_the_vanilla_values_of_neoforge() {
        let files = default_files();
        let find = |map: &str| {
            let file = files.iter().find(|file| file.map == map).expect("a file");
            serde_json::from_str::<Value>(&file.text).expect("JSON")["values"].clone()
        };
        assert_eq!(
            find("neoforge:oxidizables")["minecraft:copper_block"],
            json!({
                "next_oxidation_stage": "minecraft:exposed_copper",
                "generate_block_transform": false
            })
        );
        assert_eq!(
            find("neoforge:oxidizables").as_object().map(Map::len),
            Some(45)
        );
        assert_eq!(
            find("neoforge:waxables").as_object().map(Map::len),
            Some(60)
        );
        assert_eq!(
            find("neoforge:acceptable_villager_distances")["minecraft:pillager"],
            json!({ "acceptable_villager_distance": 15.0 })
        );
        assert_eq!(
            find("neoforge:monster_room_mobs"),
            json!({
                "minecraft:skeleton": { "weight": 100 },
                "minecraft:spider": { "weight": 100 },
                "minecraft:zombie": { "weight": 200 },
            })
        );
        assert_eq!(
            find("neoforge:parrot_imitations").as_object().map(Map::len),
            Some(42)
        );
        assert_eq!(
            find("neoforge:vibration_frequencies")
                .as_object()
                .map(Map::len),
            Some(56)
        );
        assert_eq!(
            find("neoforge:villager_compostables"),
            json!({
                "minecraft:beetroot_seeds": { "can_villager_compost": true },
                "minecraft:wheat_seeds": { "can_villager_compost": true },
            })
        );
        assert_eq!(
            find("neoforge:raid_hero_gifts").as_object().map(Map::len),
            Some(13)
        );
        assert_eq!(
            find("neoforge:villager_types").as_object().map(Map::len),
            Some(29)
        );
    }

    fn weathering_state(
        block: &Block,
    ) -> Option<crate::block::blocks::weathering_copper::WeatherState> {
        crate::block::blocks::weathering_copper::get_weather_state(block)
    }

    fn stage(target: BlockId) -> BlockStage {
        BlockStage {
            target,
            generate_block_transform: false,
        }
    }

    #[test]
    fn a_cyclic_oxidizables_map_cannot_hang_the_walks() {
        use crate::block::blocks::weathering_copper::{WeatherState, get_first, get_next};
        use crate::entity::lightning::get_first_stage_copper;

        let mut maps = BuiltinDataMaps::vanilla();
        maps.oxidizables
            .insert(BlockId::OXIDIZED_COPPER, stage(BlockId::COPPER_BLOCK));
        let _installed = install(maps);
        // The age belongs to the class: it follows the vanilla chain and ends.
        assert_eq!(
            weathering_state(&Block::OXIDIZED_COPPER),
            Some(WeatherState::Oxidized)
        );
        // `getFirst` walks the map: the walk has a cap.
        assert!(get_first(&Block::OXIDIZED_COPPER).is_some());
        assert!(get_first_stage_copper(BlockId::OXIDIZED_COPPER).is_some());
        assert!(first_oxidized_stage(&Block::EXPOSED_COPPER).as_u16() < BlockId::COUNT);
        assert_eq!(
            get_next(&Block::OXIDIZED_COPPER),
            Some(&Block::COPPER_BLOCK)
        );
    }

    #[test]
    fn a_non_copper_entry_does_not_make_a_block_weathering_copper() {
        use crate::block::blocks::weathering_copper::{get_first, get_next, get_weather_state};
        use crate::entity::lightning::{get_first_stage_copper, is_weathering_copper};

        let mut maps = BuiltinDataMaps::vanilla();
        maps.oxidizables
            .insert(BlockId::STONE, stage(BlockId::COBBLESTONE));
        let _installed = install(maps);
        // `get_next` reads the map, and the class gates read the vanilla copper family.
        assert_eq!(get_next(&Block::STONE), Some(&Block::COBBLESTONE));
        assert!(!is_weathering_copper(BlockId::STONE));
        assert!(!is_weathering_copper(BlockId::COBBLESTONE));
        assert_eq!(get_weather_state(&Block::STONE), None);
        assert_eq!(get_weather_state(&Block::COBBLESTONE), None);
        assert!(get_first(&Block::COBBLESTONE).is_none());
        assert_eq!(get_first_stage_copper(BlockId::COBBLESTONE), None);
    }

    #[test]
    fn a_pack_that_removes_the_first_link_keeps_the_age_of_the_next_block() {
        use crate::block::blocks::weathering_copper::WeatherState;

        let mut maps = BuiltinDataMaps::vanilla();
        maps.oxidizables.remove(&BlockId::COPPER_BLOCK);
        let _installed = install(maps);
        assert_eq!(
            weathering_state(&Block::EXPOSED_COPPER),
            Some(WeatherState::Exposed)
        );
        assert_eq!(
            weathering_state(&Block::COPPER_BLOCK),
            Some(WeatherState::Unaffected)
        );
        // The inverse map lost the link, so `getFirst` stops at the exposed block.
        assert_eq!(
            first_oxidized_stage(&Block::EXPOSED_COPPER),
            BlockId::EXPOSED_COPPER
        );
    }

    #[test]
    fn a_load_with_the_same_data_finds_the_transformers_of_the_load_before() {
        let mut maps = BuiltinDataMaps::vanilla();
        maps.transform_appenders.insert(
            "minecraft:axe".to_owned(),
            vec![transform(BlockId::DIRT, BlockId::STONE)],
        );
        maps.oxidizables.insert(
            BlockId::STONE,
            BlockStage {
                target: BlockId::COBBLESTONE,
                generate_block_transform: true,
            },
        );
        let installed = install(maps.clone());
        let axe = BlockTransformerImpl {
            transformer: Cow::Borrowed("axe"),
        };
        let first = block_transformer(&axe).expect("the extended transformer");
        installed.replace(maps);
        let second = block_transformer(&axe).expect("the extended transformer");
        assert!(std::ptr::eq(first, second));
        assert!(std::ptr::eq(
            generated_entry(BlockId::STONE, BlockId::COBBLESTONE, false),
            generated_entry(BlockId::STONE, BlockId::COBBLESTONE, false),
        ));
    }

    #[test]
    fn axe_and_minecraft_axe_name_the_same_transformer() {
        let mut maps = BuiltinDataMaps::vanilla();
        maps.transform_appenders.insert(
            "axe".to_owned(),
            vec![transform(BlockId::DIRT, BlockId::STONE)],
        );
        maps.transform_appenders.insert(
            "minecraft:axe".to_owned(),
            vec![transform(BlockId::GRAVEL, BlockId::SAND)],
        );
        let _installed = install(maps);
        assert_eq!(
            axe_result(&Block::DIRT).map(|result| result.target_block.id),
            Some(BlockId::STONE)
        );
        assert_eq!(
            axe_result(&Block::GRAVEL).map(|result| result.target_block.id),
            Some(BlockId::SAND)
        );
    }
}
