//! `DataMapLoader`: reads the data map files of the enabled data packs and builds the values.

use std::{
    collections::{BTreeMap, HashMap},
    fmt, fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use pumpkin_util::identifier::Identifier;
use serde::{Deserialize, Deserializer, de};
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

/// A JSON object with its keys in file order, as Gson reads it: a key given twice keeps its
/// first place and its last value. `serde_json::Map` sorts its keys unless a crate of the build
/// enables `preserve_order`.
struct OrderedObject(Vec<(String, Value)>);

impl<'de> Deserialize<'de> for OrderedObject {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = OrderedObject;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON object")
            }

            fn visit_map<A: de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<OrderedObject, A::Error> {
                let mut entries: Vec<(String, Value)> = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    match entries.iter_mut().find(|(known, _)| *known == key) {
                        Some((_, old)) => *old = value,
                        None => entries.push((key, value)),
                    }
                }
                Ok(OrderedObject(entries))
            }
        }

        deserializer.deserialize_map(Visitor)
    }
}

/// The fields of `DataMapFile.codec`; other keys are ignored, as `RecordCodecBuilder` does.
#[derive(Deserialize)]
struct RawFile {
    replace: Option<Value>,
    values: Option<OrderedObject>,
    remove: Option<RawRemove>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawRemove {
    List(Vec<Value>),
    Map(OrderedObject),
}

/// `ExtraCodecs.strictUnboundedMap`: the keys of an object parsed in file order. Two keys that
/// parse to the same target fail, as `ImmutableMap.Builder.build` throws.
fn strict_keys(object: OrderedObject) -> Result<Vec<(DataMapSource, Value)>, String> {
    let mut entries: Vec<(DataMapSource, Value)> = Vec::with_capacity(object.0.len());
    for (key, value) in object.0 {
        let source = parse_source(&key)?;
        if entries.iter().any(|(known, _)| *known == source) {
            return Err(format!("Multiple entries with same key: {source}"));
        }
        entries.push((source, value));
    }
    Ok(entries)
}

/// The `remove` field: a list of keys, and for an advanced type also a list of
/// `{"key": .., "remover": ..}` objects or a map from key to remover.
fn parse_removals(info: &TypeInfo, remove: RawRemove) -> Result<Vec<Removal>, String> {
    let plain = |list: &[Value]| -> Result<Vec<Removal>, String> {
        list.iter()
            .map(|key| parse_removal_key(key).map(|key| Removal { key, remover: None }))
            .collect()
    };
    let objects = |list: &[Value]| -> Result<Vec<Removal>, String> {
        list.iter()
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
    match remove {
        RawRemove::List(list) if !info.advanced => plain(&list),
        RawRemove::List(list) => objects(&list).or_else(|_| plain(&list)),
        RawRemove::Map(map) if info.advanced => strict_keys(map)?
            .into_iter()
            .map(|(key, remover)| {
                Ok(Removal {
                    key,
                    remover: Some(info.ops.decode_remover(remover)?),
                })
            })
            .collect(),
        RawRemove::Map(_) => {
            Err("Not a list: the remove field of a data map without a remover".to_owned())
        }
    }
}

/// `FriendlyByteBuf.writeUtf(String)`: at most 32767 UTF-16 units.
pub(super) const MAX_SYNCED_JSON_UNITS: usize = 32767;

/// Whether the value of a synced map fits one `writeUtf` of `RegistryDataMapSyncPayload`. A
/// value that does not fit is dropped here, so the sync of its registry never meets it.
fn fits_sync(info: &TypeInfo, key: &DataMapSource, entry: &Entry) -> bool {
    if info.sync.is_none() {
        return true;
    }
    let units = info
        .ops
        .encode(&entry.value)
        .map_or(0, |json| json.to_string().encode_utf16().count());
    if units <= MAX_SYNCED_JSON_UNITS {
        return true;
    }
    error!(
        "Ignored the value of synced data map {} for {key}: its JSON has {units} characters, more than the {MAX_SYNCED_JSON_UNITS} that the sync payload carries",
        info.id
    );
    false
}

/// `DataMapFile.codec`, read from the text of a file.
pub(super) fn parse_file(info: &TypeInfo, text: &str) -> Result<DataMapFile, String> {
    let raw: RawFile = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let replace = match raw.replace {
        None => false,
        Some(Value::Bool(replace)) => replace,
        Some(other) => return Err(format!("Not a boolean: {other}")),
    };
    let values = strict_keys(raw.values.ok_or("No key values in map")?)?
        .into_iter()
        .map(|(key, entry)| {
            let entry = parse_entry(info, entry)?.filter(|entry| fits_sync(info, &key, entry));
            Ok((key, entry))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let removals = raw
        .remove
        .map_or_else(|| Ok(Vec::new()), |remove| parse_removals(info, remove))?;
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

/// Where the text of a data map file comes from.
enum FileSource {
    Path(PathBuf),
    Text(String),
}

impl FileSource {
    fn read(&self) -> Result<String, String> {
        match self {
            Self::Path(path) => fs::read_to_string(path).map_err(|error| error.to_string()),
            Self::Text(text) => Ok(text.clone()),
        }
    }
}

/// The data map files of one registry folder in one pack, by map id: `<namespace>:<path>`.
fn pack_files(pack: &Pack, folder: &str) -> BTreeMap<Identifier, FileSource> {
    let mut files: BTreeMap<Identifier, FileSource> = pack
        .memory
        .iter()
        .filter(|file| file.folder == folder)
        .map(|file| (file.map.clone(), FileSource::Text(file.text.clone())))
        .collect();
    let Some(root) = &pack.root else {
        return files;
    };
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
    files: &mut BTreeMap<Identifier, FileSource>,
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
                files.insert(id, FileSource::Path(path));
            }
        }
    }
}

/// A data map file that the server holds in memory.
pub(super) struct MemoryFile {
    /// The registry folder (`block`, `worldgen/biome`).
    pub(super) folder: String,
    /// The map id (`neoforge:oxidizables`).
    pub(super) map: Identifier,
    pub(super) text: String,
}

/// A data pack, as the loader reads it: the files of a directory, and the files in memory.
pub(super) struct Pack {
    pub(super) id: String,
    /// The directory of the pack, or `None` for a pack that has only memory files.
    pub(super) root: Option<PathBuf>,
    pub(super) memory: Vec<MemoryFile>,
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
            for (map, source) in pack_files(pack, &folder) {
                let Some(info) = types.iter().find(|info| info.id == map) else {
                    warn!(
                        "Found data map file for non-existent data map type '{map}' on registry '{registry}'."
                    );
                    continue;
                };
                had_file = true;
                let file = source.read().and_then(|text| parse_file(info, &text));
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
