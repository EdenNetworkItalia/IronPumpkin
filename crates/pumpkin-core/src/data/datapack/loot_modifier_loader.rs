//! The files of the global loot modifiers (`LootModifierManager`), decoded by
//! [`crate::world::loot::modifiers`].

use std::collections::HashMap;
use std::fs;
use std::hash::BuildHasher;
use std::path::Path;

use pumpkin_util::identifier::Identifier;
use serde_json::Value;
use tracing::warn;

use crate::world::loot::modifiers::{LOOT_MODIFIER_TYPES, LootModifiers};

/// The modifier files of the packs so far by id, `<namespace>:<path>`: the JSON or why it does
/// not parse.
pub type LootModifierFiles = HashMap<String, Result<Value, String>>;

/// Reads `data/<namespace>/loot_modifiers/**/*.json` into `files`. A later pack replaces a file
/// with the same id, broken or not, as `SimpleJsonResourceReloadListener` reads only the top
/// pack's file.
pub fn load_loot_modifiers_from_dir(namespace: &str, dir: &Path, files: &mut LootModifierFiles) {
    load_recursive(namespace, dir, dir, files);
}

fn load_recursive(
    namespace: &str,
    base_dir: &Path,
    current_dir: &Path,
    files: &mut LootModifierFiles,
) {
    let Ok(entries) = fs::read_dir(current_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_recursive(namespace, base_dir, &path, files);
        } else if path.extension().is_some_and(|ext| ext == "json")
            && let Ok(rel_path) = path.strip_prefix(base_dir)
        {
            let rel_path = rel_path.to_string_lossy().replace('\\', "/");
            let Some(stem) = rel_path.strip_suffix(".json") else {
                continue;
            };
            // `PathPackResources` skips a file whose name is not a resource location.
            if !Identifier::is_valid_namespace(namespace) || !Identifier::is_valid_path(stem) {
                warn!("Invalid path in pack: {namespace}:{stem}, ignoring");
                continue;
            }
            let file = fs::read_to_string(&path)
                .map_err(|error| error.to_string())
                .and_then(|text| serde_json::from_str(&text).map_err(|error| error.to_string()));
            files.insert(format!("{namespace}:{stem}"), file);
        }
    }
}

/// Decodes the modifier files of a data load with the registered modifier types. `predicates`
/// are the predicates of the same load.
#[must_use]
pub fn decode_loot_modifiers<S: BuildHasher>(
    files: LootModifierFiles,
    predicates: &HashMap<String, Value, S>,
) -> LootModifiers {
    LOOT_MODIFIER_TYPES.load(files, &|id| {
        predicates
            .get(id)
            .cloned()
            .or_else(|| pumpkin_data::loot_table::get_loot_predicate(id).cloned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_files_and_keeps_parse_errors() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path().join("loot_modifiers");
        fs::create_dir_all(dir.join("chests")).expect("create dir");
        fs::write(
            dir.join("chests/extra.json"),
            r#"{"type": "neoforge:add_table"}"#,
        )
        .expect("write");
        fs::write(dir.join("broken.json"), "{").expect("write");
        fs::write(dir.join("notes.txt"), "{}").expect("write");
        fs::write(dir.join("Upper Case.json"), "{}").expect("write");

        let mut files = HashMap::new();
        load_loot_modifiers_from_dir("examplemod", &dir, &mut files);
        assert_eq!(files.len(), 2);
        assert!(files["examplemod:chests/extra"].is_ok());
        assert!(files["examplemod:broken"].is_err());
    }
}
