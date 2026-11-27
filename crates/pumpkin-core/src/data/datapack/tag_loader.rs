use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DynamicTagEntry {
    pub id: String,
    pub values: Vec<String>,
    /// A pack replaced the tag: its built-in values no longer apply.
    pub replace: bool,
    /// The values that no pack lists as required (`"required": false`): the tag still builds
    /// when one of them names nothing.
    pub optional: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct RawTagFile {
    #[serde(default)]
    replace: bool,
    #[serde(default)]
    values: Vec<RawTagValue>,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum RawTagValue {
    String(String),
    Object {
        id: String,
        #[serde(default = "required_by_default")]
        required: bool,
    },
}

const fn required_by_default() -> bool {
    true
}

pub fn load_tags_from_dir<S1: std::hash::BuildHasher + Default, S2: std::hash::BuildHasher>(
    namespace: &str,
    tags_dir: &Path,
    tags: &mut HashMap<String, HashMap<String, DynamicTagEntry, S1>, S2>,
) -> usize {
    let mut total_loaded = 0;
    let Ok(entries) = fs::read_dir(tags_dir) else {
        return 0;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let cat_name = entry.file_name().to_string_lossy().to_string();
        if cat_name == "worldgen" {
            // Worldgen tags are nested, e.g. tags/worldgen/biome
            if let Ok(wg_entries) = fs::read_dir(&path) {
                for wg_entry in wg_entries.flatten() {
                    let wg_path = wg_entry.path();
                    if wg_path.is_dir() {
                        let sub_cat =
                            format!("worldgen/{}", wg_entry.file_name().to_string_lossy());
                        let category_map = tags.entry(sub_cat).or_default();
                        let before = category_map.len();
                        load_tag_category_recursive(namespace, &wg_path, &wg_path, category_map);
                        total_loaded += category_map.len() - before;
                    }
                }
            }
        } else {
            let category_map = tags.entry(cat_name).or_default();
            let before = category_map.len();
            load_tag_category_recursive(namespace, &path, &path, category_map);
            total_loaded += category_map.len() - before;
        }
    }

    total_loaded
}

fn load_tag_category_recursive<S: std::hash::BuildHasher>(
    namespace: &str,
    base_dir: &Path,
    current_dir: &Path,
    category_map: &mut HashMap<String, DynamicTagEntry, S>,
) {
    let Ok(entries) = fs::read_dir(current_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_tag_category_recursive(namespace, base_dir, &path, category_map);
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            && let Ok(rel_path) = path.strip_prefix(base_dir)
        {
            let mut stem_path = rel_path.to_string_lossy().to_string();
            if let Some(stem) = stem_path.strip_suffix(".json") {
                stem_path = stem.to_string();
            }
            let stem_path = stem_path.replace('\\', "/");
            let tag_id = format!("{namespace}:{stem_path}");

            if let Ok(content) = fs::read_to_string(&path)
                && let Ok(raw) = serde_json::from_str::<RawTagFile>(&content)
            {
                let mut string_values = Vec::with_capacity(raw.values.len());
                let mut optional = Vec::new();
                for val in raw.values {
                    match val {
                        RawTagValue::String(s) => string_values.push(s),
                        RawTagValue::Object { id, required } => {
                            if !required {
                                optional.push(id.clone());
                            }
                            string_values.push(id);
                        }
                    }
                }

                if raw.replace {
                    category_map.insert(
                        tag_id.clone(),
                        DynamicTagEntry {
                            id: tag_id,
                            values: string_values,
                            replace: true,
                            optional,
                        },
                    );
                } else {
                    let entry =
                        category_map
                            .entry(tag_id.clone())
                            .or_insert_with(|| DynamicTagEntry {
                                id: tag_id,
                                values: Vec::new(),
                                replace: false,
                                optional: Vec::new(),
                            });
                    for val in string_values {
                        if !entry.values.contains(&val) {
                            entry.values.push(val.clone());
                            if optional.contains(&val) {
                                entry.optional.push(val);
                            }
                        } else if !optional.contains(&val) {
                            // A pack that requires the value makes it required.
                            entry.optional.retain(|v| *v != val);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_load_tags() {
        let temp = tempfile::tempdir().expect("tempdir");
        let tags_dir = temp.path().join("tags");
        let block_tags = tags_dir.join("block");
        fs::create_dir_all(&block_tags).expect("create dir");
        fs::write(
            block_tags.join("custom_tag.json"),
            r#"{"replace": false, "values": ["minecraft:stone", {"id": "minecraft:dirt"}]}"#,
        )
        .expect("write");

        let mut map: HashMap<String, HashMap<String, DynamicTagEntry>> = HashMap::new();
        let loaded = load_tags_from_dir("custom", &tags_dir, &mut map);
        assert_eq!(loaded, 1);
        let block_map = map.get("block").expect("block tags");
        let entry = block_map.get("custom:custom_tag").expect("custom_tag");
        assert_eq!(entry.values, ["minecraft:stone", "minecraft:dirt"]);
        assert!(!entry.replace);
    }

    #[test]
    fn a_replacing_pack_marks_the_tag_and_later_packs_append() {
        let temp = tempfile::tempdir().expect("tempdir");
        let write = |pack: &str, json: &str| {
            let dir = temp.path().join(pack).join("tags/item");
            fs::create_dir_all(&dir).expect("create dir");
            fs::write(dir.join("planks.json"), json).expect("write");
            temp.path().join(pack).join("tags")
        };
        let low = write("low", r#"{"values": ["minecraft:oak_planks"]}"#);
        let mid = write(
            "mid",
            r#"{"replace": true, "values": ["minecraft:birch_planks"]}"#,
        );
        let high = write("high", r#"{"values": ["minecraft:spruce_planks"]}"#);
        let mut map: HashMap<String, HashMap<String, DynamicTagEntry>> = HashMap::new();
        load_tags_from_dir("minecraft", &low, &mut map);
        assert!(!map["item"]["minecraft:planks"].replace);
        load_tags_from_dir("minecraft", &mid, &mut map);
        load_tags_from_dir("minecraft", &high, &mut map);
        let entry = &map["item"]["minecraft:planks"];
        assert!(entry.replace);
        assert_eq!(
            entry.values,
            ["minecraft:birch_planks", "minecraft:spruce_planks"]
        );
    }
}
