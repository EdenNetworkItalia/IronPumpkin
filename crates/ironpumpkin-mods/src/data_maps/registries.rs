//! The registries that can carry data maps, and their entries and tags on a server.

use std::collections::BTreeSet;

use pumpkin_core::server::Server;
use pumpkin_data::{
    block_transformer::get_block_transformer,
    dynamic::ContentKind,
    fluid::Fluid,
    game_event::GameEvent,
    registry::REGISTRY_V_26_3,
    tag::{RegistryKey, get_tag_values},
    villager::{VillagerProfession, VillagerType},
};
use pumpkin_util::identifier::Identifier;

use super::DataMapError;

/// `RegistryDataLoader.WORLD_REGISTRIES` without the synced ones: `DataPackRegistriesHooks`
/// refuses a synced data map on them.
const WORLD_NOT_SYNCED: &[&str] = &[
    "worldgen/carver",
    "worldgen/feature",
    "worldgen/placed_feature",
    "worldgen/structure",
    "worldgen/structure_set",
    "worldgen/processor_list",
    "worldgen/template_pool",
    "worldgen/noise_settings",
    "worldgen/noise",
    "worldgen/density_function",
    "worldgen/material_rule",
    "worldgen/material_condition",
    "worldgen/world_preset",
    "worldgen/flat_level_generator_preset",
    "trial_spawner",
    "worldgen/multi_noise_biome_source_parameter_list",
    "enchantment_provider",
    "villager_trade",
    "trade_set",
];

/// `RegistryDataLoader.RELOADABLE_REGISTRIES`: they cannot sync.
const RELOADABLE: &[&str] = &[
    "loot_table",
    "predicate",
    "context_float_provider",
    "context_int_provider",
    "item_modifier",
    "slot_source",
    "advancement",
    "recipe",
];

/// The built-in registries whose entries the server knows, besides the synced data pack
/// registries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Static {
    Content(ContentKind),
    Fluid,
    GameEvent,
    VillagerProfession,
    VillagerType,
    /// A synced data pack registry in 26.3, with the vanilla transformers built in.
    BlockTransformer,
}

fn static_registry(registry: &Identifier) -> Option<Static> {
    if !registry.is_vanilla() {
        return None;
    }
    Some(match registry.path() {
        "block" => Static::Content(ContentKind::Block),
        "item" => Static::Content(ContentKind::Item),
        "entity_type" => Static::Content(ContentKind::EntityType),
        "sound_event" => Static::Content(ContentKind::SoundEvent),
        "mob_effect" => Static::Content(ContentKind::MobEffect),
        "attribute" => Static::Content(ContentKind::Attribute),
        "data_component_type" => Static::Content(ContentKind::DataComponentType),
        "fluid" => Static::Fluid,
        "game_event" => Static::GameEvent,
        "villager_profession" => Static::VillagerProfession,
        "villager_type" => Static::VillagerType,
        "block_transformer" => Static::BlockTransformer,
        _ => return None,
    })
}

/// A synced data pack registry: its entries live in the dynamic registries of the server.
fn is_synced_data_pack_registry(registry: &Identifier) -> bool {
    registry.is_vanilla()
        && REGISTRY_V_26_3
            .iter()
            .any(|known| known.registry_id == registry.path())
}

/// The registration checks of `RegisterDataMapTypesEvent.register`, and one of `IronPumpkin`: the
/// server must know the entries of the registry.
pub(super) fn check_registry(
    id: &Identifier,
    registry: &Identifier,
    synced: bool,
) -> Result<(), DataMapError> {
    let unsupported = |reason: String| DataMapError::UnsupportedRegistry {
        id: id.clone(),
        registry: registry.clone(),
        reason,
    };
    if synced && registry.is_vanilla() {
        if WORLD_NOT_SYNCED.contains(&registry.path()) {
            return Err(unsupported(format!(
                "Cannot register synced data map {id} for world datapack registry {registry} that is not synced!"
            )));
        }
        if RELOADABLE.contains(&registry.path()) {
            return Err(unsupported(format!(
                "Cannot register synced data map {id} for reloadable datapack registry {registry}, reloadable registries do not support sync"
            )));
        }
    }
    if static_registry(registry).is_none() && !is_synced_data_pack_registry(registry) {
        return Err(unsupported(format!(
            "Cannot register data map {id} for registry {registry}: IronPumpkin does not know the entries of this registry"
        )));
    }
    Ok(())
}

/// `DataMapLoader.getFolderLocation`: the registry path, prefixed with its namespace outside
/// `minecraft`.
pub(super) fn folder_location(registry: &Identifier) -> String {
    if registry.is_vanilla() {
        registry.path().to_owned()
    } else {
        format!("{}/{}", registry.namespace(), registry.path())
    }
}

/// The entries and tags of the registries, as the loader resolves the targets of a file.
pub(super) trait Lookup {
    /// Whether the registry has the entry.
    fn contains(&self, registry: &Identifier, entry: &Identifier) -> bool;

    /// The entries of the tag, with nested tags resolved; empty when the registry has no such
    /// tag, as `Registry.getTagOrEmpty` returns.
    fn tag(&self, registry: &Identifier, tag: &Identifier) -> Vec<Identifier>;
}

/// The registries of a running server: the built-in entries and tags, those of the mods, and the
/// tags of the enabled data packs.
pub(super) struct ServerLookup<'a> {
    pub(super) server: &'a Server,
}

impl ServerLookup<'_> {
    /// The raw values of one tag: entry names and `#tag` references.
    fn tag_values(&self, registry: &Identifier, tag: &Identifier) -> Vec<String> {
        let name = tag.to_string();
        let mut values: Vec<String> = Vec::new();
        match static_registry(registry) {
            Some(Static::Content(kind)) => {
                if let Some(entries) = crate::registry::tag_entries(kind, &name) {
                    values.extend(entries.iter().map(ToString::to_string));
                }
            }
            _ => {
                if registry.is_vanilla()
                    && let Some(key) = RegistryKey::from_string(registry.path())
                    && let Some(entries) = get_tag_values(key, &name)
                {
                    values.extend(entries.iter().map(|entry| (*entry).to_owned()));
                }
            }
        }
        if let Some(entries) = self
            .server
            .datapack_manager
            .get_tag_entries(&folder_location(registry), &name)
        {
            values.extend(entries);
        }
        values
    }

    fn collect_tag(
        &self,
        registry: &Identifier,
        tag: &Identifier,
        visited: &mut BTreeSet<Identifier>,
        out: &mut Vec<Identifier>,
    ) {
        if !visited.insert(tag.clone()) {
            return;
        }
        for value in self.tag_values(registry, tag) {
            if let Some(nested) = value.strip_prefix('#') {
                if let Ok(nested) = Identifier::parse(nested) {
                    self.collect_tag(registry, &nested, visited, out);
                }
            } else if let Ok(entry) = Identifier::parse(&value)
                && !out.contains(&entry)
                && self.contains(registry, &entry)
            {
                out.push(entry);
            }
        }
    }
}

impl Lookup for ServerLookup<'_> {
    fn contains(&self, registry: &Identifier, entry: &Identifier) -> bool {
        let name = entry.to_string();
        match static_registry(registry) {
            Some(Static::Content(kind)) => crate::registry::contains(kind, &name),
            Some(Static::Fluid) => {
                entry.is_vanilla() && Fluid::from_registry_key(entry.path()).is_some()
            }
            Some(Static::GameEvent) => GameEvent::from_name(&name).is_some(),
            Some(Static::VillagerProfession) => {
                entry.is_vanilla() && VillagerProfession::from_name(&name).is_some()
            }
            Some(Static::VillagerType) => {
                entry.is_vanilla() && VillagerType::from_name(&name).is_some()
            }
            Some(Static::BlockTransformer) => {
                (entry.is_vanilla() && get_block_transformer(entry.path()).is_some())
                    || self
                        .server
                        .datapack_manager
                        .has_dynamic_registry_entry(&registry.to_string(), &name)
            }
            None => self
                .server
                .datapack_manager
                .has_dynamic_registry_entry(&registry.to_string(), &name),
        }
    }

    fn tag(&self, registry: &Identifier, tag: &Identifier) -> Vec<Identifier> {
        let mut out = Vec::new();
        self.collect_tag(registry, tag, &mut BTreeSet::new(), &mut out);
        out
    }
}
