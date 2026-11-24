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

/// The names in the registry `minecraft:trigger_type`, from `CriteriaTriggers.java`.
const VANILLA_TRIGGERS: [&str; 58] = [
    "impossible",
    "player_killed_entity",
    "entity_killed_player",
    "enter_block",
    "inventory_changed",
    "recipe_unlocked",
    "player_hurt_entity",
    "entity_hurt_player",
    "enchanted_item",
    "filled_bucket",
    "brewed_potion",
    "construct_beacon",
    "used_ender_eye",
    "summoned_entity",
    "bred_animals",
    "location",
    "slept_in_bed",
    "cured_zombie_villager",
    "villager_trade",
    "item_durability_changed",
    "levitation",
    "changed_dimension",
    "tick",
    "tame_animal",
    "placed_block",
    "consume_item",
    "effects_changed",
    "used_totem",
    "nether_travel",
    "fishing_rod_hooked",
    "channeled_lightning",
    "shot_crossbow",
    "spear_mobs",
    "killed_by_arrow",
    "hero_of_the_village",
    "voluntary_exile",
    "slide_down_block",
    "bee_nest_destroyed",
    "target_hit",
    "item_used_on_block",
    "default_block_use",
    "any_block_use",
    "player_generates_container_loot",
    "thrown_item_picked_up_by_entity",
    "thrown_item_picked_up_by_player",
    "player_interacted_with_entity",
    "player_sheared_equipment",
    "started_riding",
    "lightning_strike",
    "using_item",
    "fall_from_height",
    "ride_entity_in_lava",
    "kill_mob_near_sculk_catalyst",
    "allay_drop_item_on_block",
    "avoid_vibration",
    "recipe_crafted",
    "crafter_recipe_crafted",
    "fall_after_explosion",
];

/// The trigger of a criterion, as `Criterion.CODEC` reads it: a valid identifier that names a
/// trigger type, with the `minecraft:` namespace. No native mod registers a trigger type yet.
fn trigger_of(criterion: &Value) -> Result<String, String> {
    let map = load_error::root_object(criterion)?;
    let trigger = match map.get("trigger") {
        None => return Err(format!("No key trigger in MapLike[{criterion}]")),
        Some(Value::String(trigger)) => trigger,
        Some(other) => return Err(format!("Not a string: {other}")),
    };
    if let Some(error) = load_error::identifier_error(trigger) {
        return Err(error);
    }
    let name = trigger.strip_prefix(':').unwrap_or(trigger);
    let full = if name.contains(':') {
        name.to_string()
    } else {
        format!("minecraft:{name}")
    };
    if full
        .strip_prefix("minecraft:")
        .is_some_and(|name| VANILLA_TRIGGERS.contains(&name))
    {
        Ok(full)
    } else {
        Err(format!(
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:trigger_type]: {full}"
        ))
    }
}

/// Decodes the `criteria` of an advancement as vanilla `Advancement.CODEC` does: the key is
/// required, the map is not empty (`Advancement.java:39`), and each criterion names a trigger.
fn decode_criteria(
    value: &Value,
) -> Result<HashMap<String, (String, Arc<CriterionConditions>)>, String> {
    let root = load_error::root_object(value)?;
    let criteria = match root.get("criteria") {
        None => return Err(format!("No key criteria in MapLike[{value}]")),
        Some(Value::Object(criteria)) => criteria,
        Some(other) => return Err(format!("Not a map: {other}")),
    };
    if criteria.is_empty() {
        return Err("Advancement criteria cannot be empty".to_string());
    }
    let mut decoded = HashMap::new();
    for (name, criterion) in criteria {
        let trigger = trigger_of(criterion)?;
        let conditions = criterion
            .get("conditions")
            .map_or_else(|| "{}".to_string(), ToString::to_string);
        decoded.insert(
            name.clone(),
            (
                trigger.clone(),
                Arc::new(CriterionConditions::parse(&trigger, &conditions)),
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
            let Some((content, val)) = file.read_json(&path) else {
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
