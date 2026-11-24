//! The report of a data pack file that does not decode.
//!
//! Vanilla `RegistryDataLoader` collects these errors and then refuses the whole load. `IronPumpkin`
//! keeps the Pumpkin rule: the loader skips the file, logs one line, and loads the other files.
//! The line has the message of `RegistryLoadTask.PendingRegistration.loadFromResource` (`Failed to
//! parse <id> from pack <pack>`) and the cause that the decoder found. The recipe manager logs a
//! recipe that fails after the parse with the same first words (`server/recipe.rs`).

use std::fmt::Display;
use std::path::Path;

use serde_json::Value;
use tracing::warn;

/// The registry ids that this module reports, as `Registries` names them.
pub(super) const LOOT_TABLE: &str = "minecraft:loot_table";
pub(super) const PREDICATE: &str = "minecraft:predicate";
pub(super) const ITEM_MODIFIER: &str = "minecraft:item_modifier";
pub(super) const ADVANCEMENT: &str = "minecraft:advancement";
pub(super) const RECIPE: &str = "minecraft:recipe";

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

    /// The JSON value of `text`, or `None` after a log line when it is not JSON.
    pub fn json(&self, text: &str) -> Option<Value> {
        match serde_json::from_str(text) {
            Ok(value) => Some(value),
            Err(error) => {
                self.skip(error);
                None
            }
        }
    }
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
