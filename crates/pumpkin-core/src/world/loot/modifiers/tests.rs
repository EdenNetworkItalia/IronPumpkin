use serde_json::json;

use super::*;
use crate::world::loot::{LootTableRef, generate_loot};

/// Appends one `item`, so the order of the modifiers shows in the loot.
struct Push(&'static Item);

impl GlobalLootModifier for Push {
    fn apply(
        &self,
        mut loot: Vec<ItemStack>,
        _context: &mut LootContext<'_, '_>,
    ) -> Vec<ItemStack> {
        loot.push(ItemStack::new(1, self.0));
        loot
    }
}

fn push(json: &Value) -> Result<Box<dyn GlobalLootModifier>, String> {
    let item = json
        .get("item")
        .and_then(Value::as_str)
        .and_then(Item::from_registry_key)
        .ok_or("no item")?;
    Ok(Box::new(Push(item)))
}

fn types() -> LootModifierTypes {
    let types = LootModifierTypes::new();
    types.register("test:push", Box::new(push)).unwrap();
    types
}

fn no_predicates(_: &str) -> Option<Value> {
    None
}

fn load(types: &LootModifierTypes, files: &[(&str, Value)]) -> LootModifiers {
    let files = files
        .iter()
        .map(|(id, json)| ((*id).to_owned(), Ok(json.clone())))
        .collect();
    types.load(files, &no_predicates)
}

fn names(loot: &[ItemStack]) -> Vec<&'static str> {
    loot.iter().map(|stack| stack.item.registry_key).collect()
}

fn apply(modifiers: &LootModifiers, table_id: &str) -> Vec<&'static str> {
    let params = LootContextParameters::default();
    let mut random = Xoroshiro::from_seed(0);
    let mut context = LootContext::new(&params, table_id, &mut random);
    names(&modifiers.apply(Vec::new(), &mut context))
}

#[test]
fn registration_fails_for_a_duplicate_an_invalid_id_and_after_close() {
    let types = types();
    assert_eq!(
        types.register("test:push", Box::new(push)),
        Err(LootModifierError::Duplicate("test:push".to_owned()))
    );
    assert_eq!(
        types.register(ADD_TABLE, Box::new(push)),
        Err(LootModifierError::Duplicate(ADD_TABLE.to_owned()))
    );
    assert_eq!(
        types.register("test:Bad Id", Box::new(push)),
        Err(LootModifierError::InvalidId("test:Bad Id".to_owned()))
    );
    types.close();
    assert_eq!(
        types.register("test:late", Box::new(push)),
        Err(LootModifierError::RegistrationClosed)
    );
}

#[test]
fn bad_files_are_skipped_and_the_others_load() {
    let types = types();
    let mut files: HashMap<String, Result<Value, String>> = [
        ("test:good", json!({"type": "test:push", "item": "stone"})),
        ("test:unknown_type", json!({"type": "test:missing"})),
        ("test:failed_decode", json!({"type": "test:push"})),
        ("test:no_type", json!({"item": "stone"})),
        (
            "test:unknown_condition",
            json!({"type": "test:push", "item": "stone", "condition": {"type": "test:nope"}}),
        ),
        (
            "test:missing_predicate",
            json!({"type": "test:push", "item": "stone", "condition": "test:nope"}),
        ),
        (
            "test:bad_priority",
            json!({"type": "test:push", "item": "stone", "priority": 1.5}),
        ),
    ]
    .into_iter()
    .map(|(id, json)| (id.to_owned(), Ok(json)))
    .collect();
    files.insert("test:broken_json".to_owned(), Err("EOF".to_owned()));
    let modifiers = types.load(files, &no_predicates);
    assert_eq!(modifiers.ids().collect::<Vec<_>>(), ["test:good"]);
}

#[test]
fn neoforge_conditions_are_ignored() {
    let modifiers = load(
        &types(),
        &[(
            "test:conditional",
            json!({
                "neoforge:conditions": [{"type": "neoforge:false"}],
                "type": "test:push",
                "item": "stone"
            }),
        )],
    );
    assert_eq!(apply(&modifiers, "minecraft:blocks/stone"), ["stone"]);
}

#[test]
fn modifiers_run_by_priority_then_id() {
    let modifiers = load(
        &types(),
        &[
            ("test:b", json!({"type": "test:push", "item": "stone"})),
            (
                "test:z",
                json!({"type": "test:push", "item": "dirt", "priority": 2000}),
            ),
            (
                "test:a",
                json!({"type": "test:push", "item": "diamond", "priority": 1000}),
            ),
            (
                "test:low",
                json!({"type": "test:push", "item": "apple", "priority": -5}),
            ),
        ],
    );
    assert_eq!(
        modifiers.ids().collect::<Vec<_>>(),
        ["test:z", "test:a", "test:b", "test:low"]
    );
    assert_eq!(
        apply(&modifiers, UNKNOWN_LOOT_TABLE),
        ["dirt", "diamond", "stone", "apple"]
    );
}

#[test]
fn the_loot_table_id_condition_passes_for_its_table_only() {
    let modifiers = load(
        &types(),
        &[
            (
                "test:only_dungeon",
                json!({
                    "type": "test:push",
                    "item": "stone",
                    "condition": {"type": LOOT_TABLE_ID, "loot_table_id": "chests/simple_dungeon"}
                }),
            ),
            (
                "test:not_dungeon",
                json!({
                    "type": "test:push",
                    "item": "dirt",
                    "condition": {"type": "minecraft:inverted", "term": {
                        "type": "minecraft:all_of",
                        "terms": [
                            {"type": LOOT_TABLE_ID, "loot_table_id": "minecraft:chests/simple_dungeon"},
                            {"type": "minecraft:survives_explosion"}
                        ]
                    }}
                }),
            ),
        ],
    );
    assert_eq!(
        apply(&modifiers, "minecraft:chests/simple_dungeon"),
        ["stone"]
    );
    assert_eq!(apply(&modifiers, "minecraft:blocks/stone"), ["dirt"]);
    assert_eq!(apply(&modifiers, UNKNOWN_LOOT_TABLE), ["dirt"]);
}

#[test]
fn a_condition_can_name_a_predicate() {
    let types = types();
    let files = HashMap::from([(
        "test:by_predicate".to_owned(),
        Ok(json!({"type": "test:push", "item": "stone", "condition": "test:is_dirt"})),
    )]);
    let modifiers = types.load(files, &|id| {
        (id == "test:is_dirt")
            .then(|| json!({"type": LOOT_TABLE_ID, "loot_table_id": "minecraft:blocks/dirt"}))
    });
    assert_eq!(modifiers.len(), 1);
    assert_eq!(apply(&modifiers, "minecraft:blocks/dirt"), ["stone"]);
    assert!(apply(&modifiers, "minecraft:blocks/stone").is_empty());
}

#[test]
fn add_table_rolls_the_other_table_with_the_same_context() {
    let modifiers = load(
        &types(),
        &[
            (
                "test:on_dirt",
                json!({
                    "type": "test:push",
                    "item": "stone",
                    "condition": {"type": LOOT_TABLE_ID, "loot_table_id": "minecraft:blocks/dirt"}
                }),
            ),
            (
                "test:extra_dirt",
                json!({
                    "type": ADD_TABLE,
                    "table": "minecraft:blocks/dirt",
                    "condition": {"type": LOOT_TABLE_ID, "loot_table_id": "minecraft:chests/simple_dungeon"}
                }),
            ),
        ],
    );
    let dungeon = get_loot_table("chests/simple_dungeon").unwrap();
    assert_eq!(
        dungeon.id.as_deref(),
        Some("minecraft:chests/simple_dungeon")
    );
    let params = LootContextParameters::default();
    for seed in 0..8 {
        let LootTableRef::Static(table) = dungeon.table else {
            panic!("a static table");
        };
        let mut expected = names(&generate_loot(table, seed));
        expected.push("dirt");
        assert_eq!(
            names(&generate(&dungeon, seed, &params, &modifiers)),
            expected,
            "seed {seed}"
        );
    }
    // The added `blocks/dirt` roll is raw: `test:on_dirt` runs on a roll of the dirt table only.
    let dirt = get_loot_table("blocks/dirt").unwrap();
    assert_eq!(
        names(&generate(&dirt, 0, &params, &modifiers)),
        ["dirt", "stone"]
    );
}

#[test]
fn add_table_needs_a_table() {
    let modifiers = load(&types(), &[("test:no_table", json!({"type": ADD_TABLE}))]);
    assert!(modifiers.is_empty());
}

#[test]
fn a_roll_without_a_world_runs_no_modifiers() {
    assert!(active(&LootContextParameters::default()).is_none());
}
