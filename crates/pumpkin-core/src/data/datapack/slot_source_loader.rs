use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

use super::load_error::{self, Entry};

pub fn load_slot_sources_from_dir<S: std::hash::BuildHasher>(
    namespace: &str,
    pack: &str,
    dir: &Path,
    slot_sources: &mut HashMap<String, Value, S>,
) -> usize {
    let mut loaded = 0;
    load_slot_sources_recursive(namespace, pack, dir, dir, slot_sources, &mut loaded);
    loaded
}

fn load_slot_sources_recursive<S: std::hash::BuildHasher>(
    namespace: &str,
    pack: &str,
    base_dir: &Path,
    current_dir: &Path,
    slot_sources: &mut HashMap<String, Value, S>,
    loaded: &mut usize,
) {
    let Ok(entries) = fs::read_dir(current_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_slot_sources_recursive(namespace, pack, base_dir, &path, slot_sources, loaded);
        } else if let Some(slot_source_id) = load_error::entry_id(namespace, base_dir, &path) {
            let file = Entry {
                registry: load_error::SLOT_SOURCE,
                id: &slot_source_id,
                pack,
            };
            if let Some((_, val)) = file.read_json(&path) {
                if let Err(cause) = load_error::type_of(&val) {
                    file.skip(cause);
                    continue;
                }
                slot_sources.insert(slot_source_id, val);
                *loaded += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_load_slot_sources() {
        let temp = tempfile::tempdir().expect("tempdir");
        let slot_dir = temp.path().join("slot_source");
        fs::create_dir_all(&slot_dir).expect("create dir");
        fs::write(
            slot_dir.join("test_slot.json"),
            r#"{"type": "minecraft:slot_range", "slot_range": "armor.*"}"#,
        )
        .expect("write");

        let mut map = HashMap::new();
        let loaded = load_slot_sources_from_dir("custom", "file/test", &slot_dir, &mut map);
        assert_eq!(loaded, 1);
        assert!(map.contains_key("custom:test_slot"));
    }
}
