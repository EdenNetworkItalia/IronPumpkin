use crate::entity::player::advancement::predicate::CriterionConditions;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use super::load_error::{self, Entry};

/// A parsed dynamic advancement definition from a datapack with pre-parsed criteria.
#[derive(Clone, Debug)]
pub struct DynamicAdvancementDefinition {
    pub id: String,
    pub json: String,
    pub criteria: HashMap<String, (String, Arc<CriterionConditions>)>,
}

/// Decodes the `criteria` of an advancement as vanilla `Advancement.CODEC` does: the key is
/// required, the map is not empty (`Advancement.java:39`), and each criterion names a trigger.
fn decode_criteria(
    value: &Value,
) -> Result<HashMap<String, (String, Arc<CriterionConditions>)>, String> {
    let criteria = match value.get("criteria") {
        None => return Err(format!("No key criteria in MapLike[{value}]")),
        Some(Value::Object(criteria)) => criteria,
        Some(other) => return Err(format!("Not a map: {other}")),
    };
    if criteria.is_empty() {
        return Err("Advancement criteria cannot be empty".to_string());
    }
    let mut decoded = HashMap::new();
    for (name, criterion) in criteria {
        let trigger = match criterion.get("trigger") {
            None => return Err(format!("No key trigger in MapLike[{criterion}]")),
            Some(Value::String(trigger)) => trigger,
            Some(other) => return Err(format!("Not a string: {other}")),
        };
        let conditions = criterion
            .get("conditions")
            .map_or_else(|| "{}".to_string(), ToString::to_string);
        decoded.insert(
            name.clone(),
            (
                trigger.clone(),
                Arc::new(CriterionConditions::parse(trigger, &conditions)),
            ),
        );
    }
    Ok(decoded)
}

/// Loads all advancement JSON files from a directory recursively into `registry`. A file that
/// does not decode is logged and skipped.
pub fn load_advancements_from_dir<S: std::hash::BuildHasher>(
    namespace: &str,
    pack: &str,
    dir: &Path,
    registry: &mut HashMap<String, DynamicAdvancementDefinition, S>,
) -> usize {
    if !dir.is_dir() {
        return 0;
    }
    let before = registry.len();
    load_advancements_recursive(namespace, pack, dir, dir, registry);
    registry.len() - before
}

fn load_advancements_recursive<S: std::hash::BuildHasher>(
    namespace: &str,
    pack: &str,
    base_dir: &Path,
    current_dir: &Path,
    registry: &mut HashMap<String, DynamicAdvancementDefinition, S>,
) {
    let Ok(entries) = fs::read_dir(current_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_advancements_recursive(namespace, pack, base_dir, &path, registry);
        } else if let Some(adv_id) = load_error::entry_id(namespace, base_dir, &path) {
            let file = Entry {
                registry: load_error::ADVANCEMENT,
                id: &adv_id,
                pack,
            };
            let Some(content) = file.read(&path) else {
                continue;
            };
            let Some(val) = file.json(&content) else {
                continue;
            };
            match decode_criteria(&val) {
                Ok(criteria) => {
                    registry.insert(
                        adv_id.clone(),
                        DynamicAdvancementDefinition {
                            id: adv_id,
                            json: content,
                            criteria,
                        },
                    );
                }
                Err(cause) => file.skip(cause),
            }
        }
    }
}
