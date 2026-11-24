use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

use super::load_error::{self, Entry};

pub fn load_predicates_from_dir<S: std::hash::BuildHasher>(
    namespace: &str,
    pack: &str,
    dir: &Path,
    predicates: &mut HashMap<String, Value, S>,
) -> usize {
    let mut loaded = 0;
    load_predicates_recursive(namespace, pack, dir, dir, predicates, &mut loaded);
    loaded
}

fn load_predicates_recursive<S: std::hash::BuildHasher>(
    namespace: &str,
    pack: &str,
    base_dir: &Path,
    current_dir: &Path,
    predicates: &mut HashMap<String, Value, S>,
    loaded: &mut usize,
) {
    let Ok(entries) = fs::read_dir(current_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_predicates_recursive(namespace, pack, base_dir, &path, predicates, loaded);
        } else if let Some(predicate_id) = load_error::entry_id(namespace, base_dir, &path) {
            let file = Entry {
                registry: load_error::PREDICATE,
                id: &predicate_id,
                pack,
            };
            if let Some((_, val)) = file.read_json(&path) {
                if let Err(cause) = load_error::type_of(&val) {
                    file.skip(cause);
                    continue;
                }
                predicates.insert(predicate_id, val);
                *loaded += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_load_predicates() {
        let temp = tempfile::tempdir().expect("tempdir");
        let pred_dir = temp.path().join("predicate");
        fs::create_dir_all(&pred_dir).expect("create dir");
        fs::write(
            pred_dir.join("test_pred.json"),
            r#"{"type": "minecraft:match_tool"}"#,
        )
        .expect("write");

        let mut map = HashMap::new();
        let loaded = load_predicates_from_dir("custom", "file/test", &pred_dir, &mut map);
        assert_eq!(loaded, 1);
        assert!(map.contains_key("custom:test_pred"));
    }
}
