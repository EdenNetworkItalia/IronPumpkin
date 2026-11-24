//! The report of a data pack file that does not decode.
//!
//! Vanilla `RegistryDataLoader` collects these errors and then refuses the whole load. `IronPumpkin`
//! keeps the Pumpkin rule: the loader skips the file, logs one line, and loads the other files.
//! The line has the message of `RegistryLoadTask.PendingRegistration.loadFromResource` (`Failed to
//! parse <id> from pack <pack>`) and the cause that the decoder found. The recipe manager logs a
//! recipe that fails after the parse with the same first words (`server/recipe.rs`).

use std::fmt::Display;
use std::path::Path;

use pumpkin_util::identifier::Identifier;
use serde_json::Value;
use tracing::{debug, warn};

/// The registry ids that this module reports, as `Registries` names them.
pub(super) const LOOT_TABLE: &str = "minecraft:loot_table";
pub(super) const PREDICATE: &str = "minecraft:predicate";
pub(super) const ITEM_MODIFIER: &str = "minecraft:item_modifier";
pub(super) const ADVANCEMENT: &str = "minecraft:advancement";
pub(super) const RECIPE: &str = "minecraft:recipe";
pub(super) const SLOT_SOURCE: &str = "minecraft:slot_source";
pub(super) const CONTEXT_INT_PROVIDER: &str = "minecraft:context_int_provider";
pub(super) const CONTEXT_FLOAT_PROVIDER: &str = "minecraft:context_float_provider";
pub(super) const VILLAGER_TRADE: &str = "minecraft:villager_trade";
pub(super) const TRADE_SET: &str = "minecraft:trade_set";
pub(super) const VILLAGER_TRADE_TAG: &str = "minecraft:tags/villager_trade";

/// The key of a NeoForge load condition list in the root object of a data file.
const CONDITIONS_KEY: &str = "neoforge:conditions";

/// The data pack file that one loader reads, for the log line of a skipped file.
#[derive(Clone, Copy)]
pub(super) struct Entry<'a> {
    pub registry: &'static str,
    pub id: &'a str,
    pub pack: &'a str,
}

impl Entry<'_> {
    /// Logs that the loader skips the file.
    pub fn skip(&self, cause: impl Display) {
        warn!(
            "Failed to parse {} from pack {}: {cause} (skipped {} entry)",
            self.id, self.pack, self.registry
        );
    }

    /// The text of the file, or `None` after a log line when the file is not readable text.
    pub fn read(&self, path: &Path) -> Option<String> {
        match std::fs::read_to_string(path) {
            Ok(text) => Some(text),
            Err(error) => {
                self.skip(error);
                None
            }
        }
    }

    /// The text and the JSON value of the file `path`. `None` after a log line when the file is not
    /// readable JSON, and after a debug line for a removal stub.
    pub fn read_json(&self, path: &Path) -> Option<(String, Value)> {
        let text = self.read(path)?;
        let value = match serde_json::from_str::<Value>(&text) {
            Ok(value) => value,
            Err(error) => {
                self.skip(error);
                return None;
            }
        };
        // NeoForge skips an entry whose conditions fail, with a debug line. The server does not
        // evaluate conditions yet (task 6.9 of `modded-gameplay-parity`). A file that holds only
        // the conditions is a removal stub: it cannot decode, so the loader skips it quietly.
        if value
            .as_object()
            .is_some_and(|map| map.len() == 1 && map.contains_key(CONDITIONS_KEY))
        {
            debug!(
                "Skipping loading registry entry {}/{} as its conditions were not met",
                self.registry, self.id
            );
            return None;
        }
        Some((text, value))
    }
}

/// The `type` of a typed JSON object, as vanilla `Registry.byNameCodec().dispatch` reads it: the
/// value is an object, it has the key, and the key holds a valid identifier.
pub(super) fn type_of(value: &Value) -> Result<&str, String> {
    let map = root_object(value)?;
    match map.get("type") {
        None => Err(format!("No key type in MapLike[{value}]")),
        Some(Value::String(name)) => identifier_error(name).map_or(Ok(name), Err),
        Some(other) => Err(format!("Not a string: {other}")),
    }
}

/// The root object of a data file, or the vanilla message for a value that is not an object.
pub(super) fn root_object(value: &Value) -> Result<&serde_json::Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("Not a map: {value}"))
}

/// The error of `Identifier.read` for `text`, or `None` when `text` is a valid identifier.
pub(super) fn identifier_error(text: &str) -> Option<String> {
    let (namespace, path) = text.split_once(':').unwrap_or(("minecraft", text));
    let namespace = if namespace.is_empty() {
        "minecraft"
    } else {
        namespace
    };
    let reason = if !Identifier::is_valid_namespace(namespace) {
        format!("Non [a-z0-9_.-] character in namespace of identifier: {namespace}:{path}")
    } else if !Identifier::is_valid_path(path) {
        format!("Non [a-z0-9/._-] character in path of location: {namespace}:{path}")
    } else {
        return None;
    };
    Some(format!("Not a valid resource location: {text} {reason}"))
}

/// The id of the file `path` below `base_dir`, in `namespace`: the path without `.json`, with `/`
/// as the separator. `None` for a file that is not a JSON file below `base_dir`.
pub(super) fn entry_id(namespace: &str, base_dir: &Path, path: &Path) -> Option<String> {
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        return None;
    }
    let relative = path.strip_prefix(base_dir).ok()?.to_string_lossy();
    let stem = relative.strip_suffix(".json").unwrap_or(&relative);
    Some(format!("{namespace}:{}", stem.replace('\\', "/")))
}
