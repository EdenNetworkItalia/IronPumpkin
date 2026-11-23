//! `DataMapLoader`: reads the data map files of the enabled data packs and builds the values.

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use pumpkin_util::identifier::Identifier;
use serde_json::Value;
use tracing::{error, warn};

use super::{
    DataMapSource, ErasedRemover, Registered, Stored, TypeInfo, Values, registries::Lookup,
    registries::folder_location,
};

/// `DataMapFile`: one file of one pack.
pub(super) struct DataMapFile {
    replace: bool,
    /// The targets and entries, in file order. `None` is an entry that its conditions dropped.
    values: Vec<(DataMapSource, Option<Entry>)>,
    removals: Vec<Removal>,
}

/// `DataMapEntry`.
struct Entry {
    value: Stored,
    replace: bool,
}

/// `DataMapEntry.Removal`.
struct Removal {
    key: DataMapSource,
    remover: Option<Box<dyn ErasedRemover>>,
}

/// `ExtraCodecs.TAG_OR_ELEMENT_ID`.
fn parse_source(text: &str) -> Result<DataMapSource, String> {
    let (tag, id) = text
        .strip_prefix('#')
        .map_or((false, text), |rest| (true, rest));
    let id = Identifier::parse(id)
        .map_err(|error| format!("Not a valid resource location: {text} {error}"))?;
    Ok(if tag {
        DataMapSource::Tag(id)
    } else {
        DataMapSource::Entry(id)
    })
}

/// The key that `ConditionalOps` reads the conditions of an entry from.
const CONDITIONS_KEY: &str = "neoforge:conditions";

/// `DataMapEntry.codec`: `{"value": .., "replace": ..}`, or the value alone.
fn parse_entry(info: &TypeInfo, json: Value) -> Result<Option<Entry>, String> {
    if let Value::Object(object) = &json {
        if object.contains_key(CONDITIONS_KEY) {
            // IronPumpkin evaluates no `NeoForge` load conditions yet: a conditional entry
            // never loads.
            warn!(
                "Ignored a conditional value in data map {}: load conditions are not supported",
                info.id
            );
            return Ok(None);
        }
        if let Some(value) = object.get("value") {
            let replace = match object.get("replace") {
                None => Some(false),
                Some(Value::Bool(replace)) => Some(*replace),
                Some(_) => None,
            };
            if let Some(replace) = replace
                && let Ok(value) = info.ops.decode(value.clone())
            {
                return Ok(Some(Entry { value, replace }));
            }
        }
    }
    info.ops.decode(json).map(|value| {
        Some(Entry {
            value,
            replace: false,
        })
    })
}

fn parse_removal_key(json: &Value) -> Result<DataMapSource, String> {
    json.as_str()
        .ok_or_else(|| format!("Not a string: {json}"))
        .and_then(parse_source)
}

/// The `remove` field: a list of keys, and for an advanced type also a list of
/// `{"key": .., "remover": ..}` objects or a map from key to remover.
fn parse_removals(info: &TypeInfo, json: &Value) -> Result<Vec<Removal>, String> {
    let plain = |json: &Value| -> Result<Vec<Removal>, String> {
        json.as_array()
            .ok_or_else(|| format!("Not a list: {json}"))?
            .iter()
            .map(|key| parse_removal_key(key).map(|key| Removal { key, remover: None }))
            .collect()
    };
    if !info.advanced {
        return plain(json);
    }
    let objects = |json: &Value| -> Result<Vec<Removal>, String> {
        json.as_array()
            .ok_or_else(|| format!("Not a list: {json}"))?
            .iter()
            .map(|removal| {
                let object = removal
                    .as_object()
                    .ok_or_else(|| format!("Not a map: {removal}"))?;
                let key = parse_removal_key(
                    object
                        .get("key")
                        .ok_or_else(|| "No key key in map".to_owned())?,
                )?;
                let remover = object
                    .get("remover")
                    .map(|remover| info.ops.decode_remover(remover.clone()))
                    .transpose()?;
                Ok(Removal { key, remover })
            })
            .collect()
    };
    objects(json).or_else(|_| plain(json)).or_else(|first| {
        let Value::Object(map) = json else {
            return Err(first);
        };
        map.iter()
            .map(|(key, remover)| {
                Ok(Removal {
                    key: parse_source(key)?,
                    remover: Some(info.ops.decode_remover(remover.clone())?),
                })
            })
            .collect()
    })
}

/// `DataMapFile.codec`.
pub(super) fn parse_file(info: &TypeInfo, json: Value) -> Result<DataMapFile, String> {
    let Value::Object(mut object) = json else {
        return Err(format!("Not a map: {json}"));
    };
    let replace = match object.remove("replace") {
        None => false,
        Some(Value::Bool(replace)) => replace,
        Some(other) => return Err(format!("Not a boolean: {other}")),
    };
    let Some(Value::Object(values)) = object.remove("values") else {
        return Err("No key values in map".to_owned());
    };
    let values = values
        .into_iter()
        .map(|(key, entry)| Ok((parse_source(&key)?, parse_entry(info, entry)?)))
        .collect::<Result<Vec<_>, String>>()?;
    let removals = object
        .remove("remove")
        .map_or_else(|| Ok(Vec::new()), |remove| parse_removals(info, &remove))?;
    Ok(DataMapFile {
        replace,
        values,
        removals,
    })
}

/// `DataMapLoader.checkTagUsage`: drops the tag targets of the values when the type forbids tags.
fn check_tag_usage(info: &TypeInfo, mut file: DataMapFile, pack: &str) -> DataMapFile {
    let Some(reason) = &info.no_tags_reason else {
        return file;
    };
    if !file
        .values
        .iter()
        .any(|(key, _)| matches!(key, DataMapSource::Tag(_)))
    {
        return file;
    }
    let mut tags = String::new();
    file.values.retain(|(key, _)| {
        if let DataMapSource::Tag(tag) = key {
            tags.push_str("\n\t- ");
            tags.push_str(&tag.to_string());
            false
        } else {
            true
        }
    });
    error!(
        "Found attachment to tags in {} datamap file from pack {pack} but this DataMapType does not support attaching values via tags. The values have been ignored: {reason}.{tags}",
        info.id
    );
    file
}

/// `DataMapLoader.resolve`.
fn resolve(
    lookup: &dyn Lookup,
    registry: &Identifier,
    source: &DataMapSource,
    required: bool,
) -> Vec<Identifier> {
    match source {
        DataMapSource::Tag(tag) => lookup.tag(registry, tag),
        DataMapSource::Entry(entry) => {
            if lookup.contains(registry, entry) {
                vec![entry.clone()]
            } else {
                if required {
                    error!(
                        "Object with ID {entry} specified in data map for registry {registry} doesn't exist"
                    );
                }
                Vec::new()
            }
        }
    }
}

/// `DataMapLoader.buildDataMap`: applies the files in pack order.
pub(super) fn build(info: &TypeInfo, files: Vec<DataMapFile>, lookup: &dyn Lookup) -> Values {
    let registry = &info.registry;
    let mut result: HashMap<Identifier, (Stored, DataMapSource)> = HashMap::new();
    for file in files {
        if file.replace {
            result.clear();
        }
        for (source, entry) in file.values {
            let Some(entry) = entry else {
                continue;
            };
            for key in resolve(lookup, registry, &source, true) {
                let value = match result.remove(&key) {
                    Some((old, old_source)) if !entry.replace => {
                        info.ops
                            .merge(registry, &old_source, &old, &source, entry.value.clone())
                    }
                    _ => entry.value.clone(),
                };
                result.insert(key, (value, source.clone()));
            }
        }
        for removal in file.removals {
            for key in resolve(lookup, registry, &removal.key, false) {
                let Some(remover) = &removal.remover else {
                    result.remove(&key);
                    continue;
                };
                let Some((old, old_source)) = result.remove(&key) else {
                    continue;
                };
                if let Some(kept) = remover.remove(&old, registry, &old_source, &key) {
                    result.insert(key, (kept, old_source));
                }
            }
        }
    }
    result
        .into_iter()
        .map(|(key, (value, _))| (key, value))
        .collect()
}

/// The data map files of one registry folder in one pack, by map id: `<namespace>:<path>`.
fn pack_files(root: &Path, folder: &str) -> BTreeMap<Identifier, PathBuf> {
    let mut files = BTreeMap::new();
    let Ok(namespaces) = fs::read_dir(root.join("data")) else {
        return files;
    };
    for namespace in namespaces.flatten() {
        let namespace_name = namespace.file_name().to_string_lossy().into_owned();
        let dir = namespace.path().join("data_maps").join(folder);
        collect_json(&dir, &dir, &namespace_name, &mut files);
    }
    files
}

fn collect_json(
    base: &Path,
    dir: &Path,
    namespace: &str,
    files: &mut BTreeMap<Identifier, PathBuf>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_json(base, &path, namespace, files);
        } else if path.extension().is_some_and(|ext| ext == "json")
            && let Ok(relative) = path.with_extension("").strip_prefix(base)
        {
            let map = relative.to_string_lossy().replace('\\', "/");
            if let Ok(id) = Identifier::parse(&format!("{namespace}:{map}")) {
                files.insert(id, path);
            }
        }
    }
}

/// A data pack, as the loader reads it.
pub(super) struct Pack {
    pub(super) id: String,
    pub(super) root: PathBuf,
}

/// `DataMapLoader.load` and `apply`: reads the files of `packs` (lowest priority first) for
/// every registry with data map types, and replaces the values of every registry that had a
/// file. Returns those registries.
pub(super) fn load(
    registered: &Registered,
    packs: &[Pack],
    lookup: &dyn Lookup,
) -> Vec<Identifier> {
    let mut updated = Vec::new();
    for (registry, types) in &registered.by_registry {
        let folder = folder_location(registry);
        let mut by_type: Vec<(Arc<TypeInfo>, Vec<DataMapFile>)> = Vec::new();
        // `DataMapLoader.load` keeps a registry once it has a file of a known type, even one
        // that does not parse.
        let mut had_file = false;
        for pack in packs {
            for (map, path) in pack_files(&pack.root, &folder) {
                let Some(info) = types.iter().find(|info| info.id == map) else {
                    warn!(
                        "Found data map file for non-existent data map type '{map}' on registry '{registry}'."
                    );
                    continue;
                };
                had_file = true;
                let file = fs::read_to_string(&path)
                    .map_err(|error| error.to_string())
                    .and_then(|text| serde_json::from_str(&text).map_err(|error| error.to_string()))
                    .and_then(|json| parse_file(info, json));
                let file = match file {
                    Ok(file) => check_tag_usage(info, file, &pack.id),
                    Err(message) => {
                        error!(
                            "Could not read data map of type {map} for registry {registry}: {message}"
                        );
                        continue;
                    }
                };
                match by_type
                    .iter_mut()
                    .find(|(known, _)| Arc::ptr_eq(known, info))
                {
                    Some((_, files)) => files.push(file),
                    None => by_type.push((info.clone(), vec![file])),
                }
            }
        }
        // A registry with no file keeps its values, as `DataMapLoader.apply` only visits the
        // registries that had a file.
        if !had_file {
            continue;
        }
        for info in types {
            let files = by_type
                .iter()
                .position(|(known, _)| Arc::ptr_eq(known, info))
                .map(|index| by_type.swap_remove(index).1)
                .unwrap_or_default();
            info.set_values(build(info, files, lookup));
        }
        updated.push(registry.clone());
    }
    updated
}
