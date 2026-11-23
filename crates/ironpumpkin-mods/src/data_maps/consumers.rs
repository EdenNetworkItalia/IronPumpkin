//! The vanilla consumers of the built-in data maps.
//!
//! After each load, [`publish`] gives the values of the `NeoForge` built-in maps to the server,
//! in the form that the vanilla behaviours read (`pumpkin_core::plugin::data_maps::builtin`). The
//! loader reads the values of vanilla first: the server gives them as a pack in memory, so they
//! have the lowest priority.

use std::path::PathBuf;

use pumpkin_core::plugin::data_maps::builtin::{
    BlockStage, BuiltinDataMaps, default_files, set_builtin,
};
use pumpkin_data::{
    BlockId,
    chunk::Biome,
    dynamic::ContentKind,
    game_event::GameEvent,
    sound::Sound,
    villager::{VillagerProfession, VillagerType},
};
use pumpkin_util::identifier::Identifier;
use tracing::warn;

use super::{
    DataMapType,
    builtin::{
        ACCEPTABLE_VILLAGER_DISTANCES, BLOCK_TRANSFORM_APPENDERS, MONSTER_ROOM_MOBS, OXIDIZABLES,
        PARROT_IMITATIONS, RAID_HERO_GIFTS, TRANSFORMABLES, VIBRATION_FREQUENCIES,
        VILLAGER_COMPOSTABLES, VILLAGER_TYPES, WAXABLES,
    },
    loader::{MemoryFile, Pack},
    transform_entry,
};

/// The pack with the values of vanilla: the files that `NeoForge` ships in
/// `data/neoforge/data_maps`.
pub(super) fn default_pack() -> Pack {
    Pack {
        id: "ironpumpkin/builtin".to_owned(),
        root: None::<PathBuf>,
        memory: default_files()
            .into_iter()
            .filter_map(|file| {
                Some(MemoryFile {
                    folder: file.folder.to_owned(),
                    map: Identifier::parse(file.map).ok()?,
                    text: file.text,
                })
            })
            .collect(),
    }
}

fn content_id(kind: ContentKind, entry: &Identifier) -> Option<u16> {
    crate::registry::id_of(kind, &entry.to_string())
}

/// The values of a map in the order of their entry ids, so a load gives the same result each time.
fn sorted<T: super::DataMapValue>(map: &DataMapType<T>) -> Vec<(Identifier, T)> {
    let mut values: Vec<(Identifier, T)> = map
        .data_map()
        .iter()
        .map(|(entry, value)| (entry.clone(), value.clone()))
        .collect();
    values.sort_by(|a, b| a.0.cmp(&b.0));
    values
}

/// The values of the built-in data maps, as the server consumers read them.
pub(super) fn collect() -> BuiltinDataMaps {
    let mut maps = BuiltinDataMaps::default();

    for (entry, value) in sorted(&ACCEPTABLE_VILLAGER_DISTANCES) {
        if let Some(id) = content_id(ContentKind::EntityType, &entry) {
            maps.acceptable_villager_distances
                .insert(id, value.distance);
        }
    }
    for (entry, value) in sorted(&MONSTER_ROOM_MOBS) {
        maps.monster_room_mobs
            .push((entry.to_string(), value.weight));
    }
    for (entry, value) in sorted(&OXIDIZABLES) {
        if let Some((block, target)) = block_pair(&entry, &value.next_oxidation_stage) {
            maps.oxidizables.insert(
                block,
                BlockStage {
                    target,
                    generate_block_transform: value.generate_block_transform,
                },
            );
        }
    }
    for (entry, value) in sorted(&WAXABLES) {
        if let Some((block, target)) = block_pair(&entry, &value.waxed) {
            maps.waxables.insert(
                block,
                BlockStage {
                    target,
                    generate_block_transform: value.generate_block_transform,
                },
            );
        }
    }
    for (entry, value) in sorted(&PARROT_IMITATIONS) {
        let Some(id) = content_id(ContentKind::EntityType, &entry) else {
            continue;
        };
        if value.sound.is_vanilla()
            && let Some(sound) = Sound::from_name(value.sound.path())
        {
            maps.parrot_imitations.insert(id, sound);
        } else {
            warn!(
                "Ignored the parrot imitation of {entry}: IronPumpkin plays only the sounds of vanilla ({})",
                value.sound
            );
        }
    }
    for (entry, value) in sorted(&RAID_HERO_GIFTS) {
        if let Some(profession) = VillagerProfession::from_name(&entry.to_string()) {
            maps.raid_hero_gifts
                .insert(profession, value.loot_table.to_string());
        }
    }
    for (entry, value) in sorted(&VIBRATION_FREQUENCIES) {
        if let Some(event) = GameEvent::from_name(&entry.to_string()) {
            maps.vibration_frequencies.insert(event, value.frequency);
        }
    }
    for (entry, value) in sorted(&VILLAGER_COMPOSTABLES) {
        if value.can_villager_compost
            && let Some(id) = content_id(ContentKind::Item, &entry)
        {
            maps.villager_compostables.insert(id);
        }
    }
    for (entry, value) in sorted(&VILLAGER_TYPES) {
        if entry.is_vanilla()
            && Biome::from_name(entry.path()).is_some()
            && let Some(villager_type) = VillagerType::from_name(&value.villager_type.to_string())
        {
            maps.villager_types
                .insert(entry.path().to_owned(), villager_type);
        }
    }
    for (transformer, value) in sorted(&BLOCK_TRANSFORM_APPENDERS) {
        let entries = value
            .entries
            .iter()
            .filter_map(|data| {
                transform_entry::compile(
                    data,
                    &format!("{} for {transformer}", BLOCK_TRANSFORM_APPENDERS.id()),
                )
            })
            .collect::<Vec<_>>();
        if !entries.is_empty() {
            maps.transform_appenders
                .entry(transformer.to_string())
                .or_default()
                .extend(entries);
        }
    }
    for (block, value) in sorted(&TRANSFORMABLES) {
        for (transformer, data) in &value.transformers {
            if let Some(entry) =
                transform_entry::compile(data, &format!("{} for {block}", TRANSFORMABLES.id()))
            {
                maps.transformables
                    .entry(transformer.to_string())
                    .or_default()
                    .push(entry);
            }
        }
    }
    maps
}

/// The block ids of an entry and the block that it leads to.
fn block_pair(entry: &Identifier, target: &Identifier) -> Option<(BlockId, BlockId)> {
    let block = content_id(ContentKind::Block, entry).and_then(BlockId::new)?;
    let target = content_id(ContentKind::Block, target).and_then(BlockId::new)?;
    Some((block, target))
}

/// Gives the values of the built-in maps to the server.
pub(super) fn publish() {
    set_builtin(collect());
}
