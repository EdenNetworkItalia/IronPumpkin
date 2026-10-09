//! The startup content phase.
//!
//! Native mods register their blocks, items and entity types before the first world loads.
//! [`run`] then reads the world's content manifest, registers a placeholder for each listed name
//! that no mod registered, freezes the registry and writes the manifest back. The manifest holds
//! names only: ids are allocated again at every start.

pub mod behaviour;

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use pumpkin_data::dynamic::{
    self, BlockDefinition, BlockPropertyDefinition, ContentKind, ContentTables,
    EntityTypeDefinition, ItemDefinition, RegistryError,
};
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::{Block, BlockId, BlockStateId};
use pumpkin_nbt::compound::NbtCompound;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// The manifest format this build reads and writes.
const FORMAT: u32 = 1;

/// The path of the content manifest of the world in `world_path`.
#[must_use]
pub fn manifest_path(world_path: &Path) -> PathBuf {
    world_path.join("ironpumpkin").join("content_registry.json")
}

#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error("cannot read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error(
        "cannot parse {}: {message}. Fix the file or delete it while the server is stopped",
        path.display()
    )]
    Parse { path: PathBuf, message: String },
    #[error(
        "cannot restore the {kind} \"{name}\" from {}: {source}. Fix the entry or delete it while the server is stopped",
        path.display()
    )]
    Entry {
        path: PathBuf,
        kind: ContentKind,
        name: String,
        source: RegistryError,
    },
    #[error("cannot freeze the content registry: {0}")]
    Freeze(RegistryError),
    #[error("cannot write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    format: u32,
    #[serde(default)]
    blocks: BTreeMap<String, BlockEntry>,
    #[serde(default)]
    items: BTreeMap<String, DisplayEntry>,
    #[serde(default)]
    entity_types: BTreeMap<String, DisplayEntry>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct BlockEntry {
    /// Property names in name order, values in state layout order.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    properties: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    default: BTreeMap<String, String>,
    /// A vanilla block state, for example `minecraft:redstone_lamp[lit=false]`.
    display: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct DisplayEntry {
    display: String,
}

/// Runs the content phase for the world in `world_path` and returns the names that got a
/// placeholder. Runs once, before the first world loads.
///
/// With no custom content it writes nothing, so a vanilla server leaves the world unchanged. An
/// entry that cannot be restored stops the run before the freeze and leaves the manifest as it
/// is, because rewriting it would lose that content.
pub fn run(world_path: &Path) -> Result<Vec<(ContentKind, String)>, ContentError> {
    let path = manifest_path(world_path);
    let manifest = read_manifest(&path)?;
    let placeholders = register_placeholders(&path, &manifest)?;
    let tables = dynamic::freeze().map_err(ContentError::Freeze)?;
    if !(tables.blocks().is_empty()
        && tables.items().is_empty()
        && tables.entity_types().is_empty())
    {
        let current = manifest_of(tables);
        warn_changed(ContentKind::Block, &manifest.blocks, &current.blocks);
        warn_changed(ContentKind::Item, &manifest.items, &current.items);
        warn_changed(
            ContentKind::EntityType,
            &manifest.entity_types,
            &current.entity_types,
        );
        write_manifest(&path, &current)?;
    }
    Ok(placeholders)
}

/// Warns once per name that a mod registers differently from the manifest.
fn warn_changed<T: PartialEq>(
    kind: ContentKind,
    saved: &BTreeMap<String, T>,
    current: &BTreeMap<String, T>,
) {
    for (name, entry) in saved {
        if current.get(name).is_some_and(|current| current != entry)
            && !dynamic::is_placeholder(kind, name)
        {
            warn!(
                "[ironpumpkin] the {kind} \"{name}\" in the content manifest differs from what its mod registers: the mod's version wins and the manifest entry is rewritten"
            );
        }
    }
}

fn read_manifest(path: &Path) -> Result<Manifest, ContentError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Manifest::default()),
        Err(source) => {
            return Err(ContentError::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let parse = |message: String| ContentError::Parse {
        path: path.to_path_buf(),
        message,
    };
    let manifest: Manifest = serde_json::from_str(&text).map_err(|err| parse(err.to_string()))?;
    if manifest.format != FORMAT {
        return Err(parse(format!(
            "unknown format {}, this server reads format {FORMAT}",
            manifest.format
        )));
    }
    Ok(manifest)
}

/// Registers a placeholder for each manifest entry that no mod registered.
fn register_placeholders(
    path: &Path,
    manifest: &Manifest,
) -> Result<Vec<(ContentKind, String)>, ContentError> {
    let mut placeholders = Vec::new();
    let mut restore = |kind: ContentKind,
                       name: &String,
                       register: &dyn Fn() -> Result<(), RegistryError>|
     -> Result<(), ContentError> {
        if dynamic::is_registered(kind, name) {
            return Ok(());
        }
        register().map_err(|source| ContentError::Entry {
            path: path.to_path_buf(),
            kind,
            name: name.clone(),
            source,
        })?;
        warn!(
            "[ironpumpkin] {kind} \"{name}\" is in the content manifest but no mod registers it: it loads as a placeholder"
        );
        placeholders.push((kind, name.clone()));
        Ok(())
    };
    for (name, entry) in &manifest.blocks {
        restore(ContentKind::Block, name, &|| {
            dynamic::register_placeholder_block(block_definition(name, entry)?)
        })?;
    }
    for (name, entry) in &manifest.items {
        restore(ContentKind::Item, name, &|| {
            let display =
                item_display(&entry.display).ok_or_else(|| not_vanilla(ContentKind::Item, name))?;
            dynamic::register_placeholder_item(ItemDefinition {
                name: name.clone(),
                display,
                block: None,
                tags: Vec::new(),
            })
        })?;
    }
    for (name, entry) in &manifest.entity_types {
        restore(ContentKind::EntityType, name, &|| {
            let display = entity_type_display(&entry.display)
                .ok_or_else(|| not_vanilla(ContentKind::EntityType, name))?;
            dynamic::register_placeholder_entity_type(EntityTypeDefinition {
                name: name.clone(),
                display,
                dimensions: None,
                eye_height: None,
                tags: Vec::new(),
            })
        })?;
    }
    Ok(placeholders)
}

fn not_vanilla(kind: ContentKind, name: &str) -> RegistryError {
    RegistryError::DisplayNotVanilla {
        kind,
        name: name.to_string(),
    }
}

fn block_definition(name: &str, entry: &BlockEntry) -> Result<BlockDefinition, RegistryError> {
    let display =
        block_display(&entry.display).ok_or_else(|| not_vanilla(ContentKind::Block, name))?;
    let properties = entry
        .properties
        .iter()
        .map(|(property, values)| BlockPropertyDefinition {
            name: property.clone(),
            values: values.clone(),
            // A missing default fails the registry's check, like any value not in `values`.
            default: entry.default.get(property).cloned().unwrap_or_default(),
        })
        .collect();
    Ok(BlockDefinition {
        name: name.to_string(),
        display,
        properties,
        tags: Vec::new(),
    })
}

/// Parses `minecraft:name[key=value,...]` into a vanilla block state.
fn block_display(display: &str) -> Option<BlockStateId> {
    let (name, properties) = match display.split_once('[') {
        Some((name, rest)) => (name, rest.strip_suffix(']')?),
        None => (display, ""),
    };
    let block = Block::from_name(name).filter(|block| block.id.as_u16() < BlockId::COUNT)?;
    let props: Vec<(&str, &str)> = properties
        .split(',')
        .filter(|pair| !pair.is_empty())
        .map(|pair| pair.split_once('='))
        .collect::<Option<_>>()?;
    if props.is_empty() {
        return Some(block.default_state.id);
    }
    // `Block::from_properties` keeps the default for a value the block does not have, and the file
    // is edited by hand: search the states, so a display that names no state is rejected.
    block.states.iter().map(|state| state.id).find(|&id| {
        block.properties(id).is_some_and(|properties| {
            let values = properties.to_props();
            props
                .iter()
                .all(|pair| values.iter().any(|value| value == pair))
        })
    })
}

fn item_display(display: &str) -> Option<&'static Item> {
    Item::from_registry_key(display).filter(|item| item.id < Item::COUNT)
}

fn entity_type_display(display: &str) -> Option<&'static EntityType> {
    EntityType::from_name(display.strip_prefix("minecraft:").unwrap_or(display))
        .filter(|entity_type| entity_type.id < EntityType::COUNT)
}

fn state_string(id: BlockStateId) -> String {
    let block = id.to_block();
    let mut text = dynamic::namespaced_name(block.name).into_owned();
    if let Some(properties) = block.properties(id) {
        let pairs: Vec<String> = properties
            .to_props()
            .into_iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect();
        text.push('[');
        text.push_str(&pairs.join(","));
        text.push(']');
    }
    text
}

fn manifest_of(tables: &'static ContentTables) -> Manifest {
    let blocks = tables
        .blocks()
        .iter()
        .zip(tables.block_properties())
        .map(|(block, properties)| {
            let default = block
                .properties(block.default_state.id)
                .map(|props| {
                    props
                        .to_props()
                        .into_iter()
                        .map(|(key, value)| (key.to_string(), value.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let entry = BlockEntry {
                properties: properties
                    .iter()
                    .map(|property| {
                        let values = property.values.iter().map(|v| (*v).to_string()).collect();
                        (property.name.to_string(), values)
                    })
                    .collect(),
                default,
                display: state_string(block.default_state.id.display_state()),
            };
            (block.name.to_string(), entry)
        })
        .collect();
    let items = tables
        .items()
        .iter()
        .map(|item| {
            let display = Item::from_id(item.to_java_network_id()).unwrap_or(&Item::AIR);
            let display = dynamic::namespaced_name(display.registry_key).into_owned();
            (item.registry_key.to_string(), DisplayEntry { display })
        })
        .collect();
    let entity_types = tables
        .entity_types()
        .iter()
        .map(|entity_type| {
            let display = dynamic::namespaced_name(entity_type.display_type().resource_name);
            let display = display.into_owned();
            (
                entity_type.resource_name.to_string(),
                DisplayEntry { display },
            )
        })
        .collect();
    Manifest {
        format: FORMAT,
        blocks,
        items,
        entity_types,
    }
}

/// Writes a temporary file and renames it, so a crash never leaves a truncated manifest.
fn write_manifest(path: &Path, manifest: &Manifest) -> Result<(), ContentError> {
    let error = |source| ContentError::Write {
        path: path.to_path_buf(),
        source,
    };
    let text = serde_json::to_string_pretty(manifest)
        .map_err(io::Error::other)
        .map_err(error)?
        + "\n";
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(error)?;
    }
    let temporary = path.with_extension("json.tmp");
    match std::fs::remove_file(&temporary) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(error(err)),
        _ => {}
    }
    let mut file = std::fs::File::create(&temporary).map_err(error)?;
    file.write_all(text.as_bytes()).map_err(error)?;
    // The rename must not land before the data, or a crash leaves an empty manifest.
    file.sync_all().map_err(error)?;
    drop(file);
    std::fs::rename(&temporary, path).map_err(error)
}

/// Whether a saved entity record has a placeholder type. Such a record never spawns: it stays in
/// its chunk's saved data and is written back unchanged.
#[must_use]
pub fn is_placeholder_record(record: &NbtCompound) -> bool {
    record
        .get_string("id")
        // Custom names always carry their own namespace, so vanilla records skip the lookup.
        .filter(|id| id.contains(':') && !id.starts_with("minecraft:"))
        .and_then(EntityType::from_name)
        .is_some_and(EntityType::is_placeholder)
}

/// Takes the records to spawn out of an entity chunk's saved data when the chunk goes live. The
/// records of placeholder types stay.
pub fn take_spawnable_records(data: &mut Vec<NbtCompound>) -> Vec<NbtCompound> {
    let (placeholders, spawnable) = std::mem::take(data)
        .into_iter()
        .partition(is_placeholder_record);
    *data = placeholders;
    spawnable
}

/// Rebuilds the saved data of a live entity chunk from the records of its live entities. The
/// records of placeholder types stay.
pub fn rebuild_live_records(data: &mut Vec<NbtCompound>, fresh: Vec<NbtCompound>) {
    data.retain(is_placeholder_record);
    data.extend(fresh);
}
