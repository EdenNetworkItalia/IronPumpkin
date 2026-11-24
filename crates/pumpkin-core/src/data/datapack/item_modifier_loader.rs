use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

use super::load_error::{self, Entry};

pub fn load_item_modifiers_from_dir<S: std::hash::BuildHasher>(
    namespace: &str,
    pack: &str,
    dir: &Path,
    modifiers: &mut HashMap<String, Value, S>,
) -> usize {
    let mut loaded = 0;
    load_item_modifiers_recursive(namespace, pack, dir, dir, modifiers, &mut loaded);
    loaded
}

fn load_item_modifiers_recursive<S: std::hash::BuildHasher>(
    namespace: &str,
    pack: &str,
    base_dir: &Path,
    current_dir: &Path,
    modifiers: &mut HashMap<String, Value, S>,
    loaded: &mut usize,
) {
    let Ok(entries) = fs::read_dir(current_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_item_modifiers_recursive(namespace, pack, base_dir, &path, modifiers, loaded);
        } else if let Some(modifier_id) = load_error::entry_id(namespace, base_dir, &path) {
            let file = Entry {
                registry: load_error::ITEM_MODIFIER,
                id: &modifier_id,
                pack,
            };
            if let Some((_, val)) = file.read_json(&path) {
                if let Err(cause) = load_error::type_of(&val) {
                    file.skip(cause);
                    continue;
                }
                modifiers.insert(modifier_id, val);
                *loaded += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_load_item_modifiers() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mod_dir = temp.path().join("item_modifier");
        fs::create_dir_all(&mod_dir).expect("create dir");
        fs::write(
            mod_dir.join("test_mod.json"),
            r#"{"type": "minecraft:set_count", "count": 5}"#,
        )
        .expect("write");

        let mut map = HashMap::new();
        let loaded = load_item_modifiers_from_dir("custom", "file/test", &mod_dir, &mut map);
        assert_eq!(loaded, 1);
        assert!(map.contains_key("custom:test_mod"));
    }
}
