//! A data pack file that does not decode is skipped. The loader logs one line with the file id,
//! and the other files of the pack and of the reload load. Vanilla `RegistryDataLoader` fails the
//! whole load for such a file; `IronPumpkin` keeps the Pumpkin rule (see
//! `docs/porting-from-neoforge/data-packs.md`).
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first poisoned lock or missing file"
)]

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use pumpkin_core::data::datapack::DatapackManager;
use pumpkin_core::server::recipe::{DynamicRecipe, RecipeManager};

const PACK: &str = "file/mypack";

struct Capture(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A world with the data pack `mypack` that holds `files`, relative to its `data` folder.
fn world(files: &[(&str, &str)]) -> tempfile::TempDir {
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join("data_pack_decode_errors");
    fs::create_dir_all(&scratch).unwrap();
    let world = tempfile::tempdir_in(scratch).unwrap();
    write(world.path(), files);
    world
}

fn write(world: &Path, files: &[(&str, &str)]) {
    let data = world.join("datapacks/mypack/data");
    for (path, text) in files {
        let file = data.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
}

/// Loads the data packs of `world`, as the server start and `/reload` do, and returns the log.
fn load(manager: &DatapackManager, recipes: &RecipeManager, world: &Path) -> String {
    let out = Arc::new(Mutex::new(Vec::new()));
    let writer = Arc::clone(&out);
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_writer(move || Capture(Arc::clone(&writer)))
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        manager.load_all(world, &[PACK.to_string()], recipes);
    });
    String::from_utf8(out.lock().unwrap().clone()).unwrap()
}

/// The log lines that report a skipped file.
fn skipped(log: &str) -> Vec<&str> {
    log.lines()
        .filter(|line| line.contains("Failed to parse"))
        .collect()
}

fn one_skipped<'a>(log: &'a str, id: &str, cause: &str) -> &'a str {
    let lines = skipped(log);
    assert_eq!(lines.len(), 1, "{log}");
    let line = lines[0];
    assert!(
        line.contains(&format!("Failed to parse {id} from pack {PACK}: {cause}")),
        "{line}"
    );
    line
}

const GOOD_RECIPE: &str = r#"{
    "type": "minecraft:smelting",
    "ingredient": "minecraft:dirt",
    "result": { "id": "minecraft:diamond" }
}"#;

fn recipe_ids(recipes: &RecipeManager) -> Vec<String> {
    recipes
        .get_dynamic_recipes_internal()
        .iter()
        .filter(|recipe| !matches!(recipe, DynamicRecipe::Ignored { .. }))
        .filter_map(|recipe| recipe.recipe_id().map(str::to_owned))
        .collect()
}

#[test]
fn a_recipe_that_does_not_decode_is_skipped() {
    let world = world(&[
        ("test/recipe/good.json", GOOD_RECIPE),
        (
            "test/recipe/no_fields.json",
            r#"{"type":"minecraft:smelting"}"#,
        ),
    ]);
    let recipes = RecipeManager::default();
    let log = load(&DatapackManager::new(), &recipes, world.path());
    assert_eq!(recipe_ids(&recipes), ["test:good"]);
    one_skipped(
        &log,
        "test:no_fields",
        "The minecraft:smelting recipe has a field that does not decode",
    );
    assert!(log.contains("(skipped minecraft:recipe entry)"), "{log}");
}

#[test]
fn a_recipe_with_an_unknown_type_or_a_json_error_is_skipped() {
    let world = world(&[
        ("test/recipe/good.json", GOOD_RECIPE),
        (
            "test/recipe/unknown_type.json",
            r#"{"type":"test:nothing"}"#,
        ),
        (
            "test/recipe/no_type.json",
            r#"{"ingredient":"minecraft:dirt"}"#,
        ),
        ("test/recipe/broken.json", r#"{"type": "#),
    ]);
    let recipes = RecipeManager::default();
    let log = load(&DatapackManager::new(), &recipes, world.path());
    assert_eq!(recipe_ids(&recipes), ["test:good"]);
    let lines = skipped(&log);
    assert_eq!(lines.len(), 3, "{log}");
    let has =
        |id: &str, cause: &str| {
            assert!(
                lines.iter().any(|line| line
                    .contains(&format!("Failed to parse {id} from pack {PACK}: {cause}"))),
                "{log}"
            );
        };
    has(
        "test:unknown_type",
        "Unknown registry key in ResourceKey[minecraft:root / minecraft:recipe_serializer]: test:nothing",
    );
    has("test:no_type", "No key type in MapLike[");
    has("test:broken", "EOF while parsing");
}

/// The recipe manager reports a recipe that decodes but names an item that does not exist, with
/// the vanilla message. The loader keeps the other recipes.
#[test]
fn a_recipe_with_an_unknown_result_item_is_skipped() {
    let world = world(&[
        ("test/recipe/good.json", GOOD_RECIPE),
        (
            "test/recipe/missing_item.json",
            r#"{
                "type": "minecraft:smelting",
                "ingredient": "minecraft:dirt",
                "result": { "id": "test:missing" }
            }"#,
        ),
    ]);
    let recipes = RecipeManager::default();
    let log = load(&DatapackManager::new(), &recipes, world.path());
    assert_eq!(recipe_ids(&recipes), ["test:good"]);
    let lines = skipped(&log);
    assert_eq!(lines.len(), 1, "{log}");
    assert!(
        lines[0].contains("Failed to parse test:missing_item: Unknown registry key in ResourceKey[minecraft:root / minecraft:item]: test:missing"),
        "{log}"
    );
}

#[test]
fn a_loot_table_that_does_not_decode_is_skipped() {
    let world = world(&[
        ("test/loot_table/good.json", r#"{"pools": []}"#),
        ("test/loot_table/bad_pools.json", r#"{"pools": 5}"#),
        ("test/loot_table/broken.json", r#"{"pools": ["#),
        ("test/loot_table/not_a_map.json", "[]"),
    ]);
    let manager = DatapackManager::new();
    let log = load(&manager, &RecipeManager::default(), world.path());
    assert_eq!(
        manager
            .get_loot_table_names()
            .into_iter()
            .filter(|name| name.starts_with("test:"))
            .collect::<Vec<_>>(),
        ["test:good"]
    );
    let lines = skipped(&log);
    assert_eq!(lines.len(), 3, "{log}");
    for (id, cause) in [
        ("test:bad_pools", "does not decode"),
        ("test:broken", "EOF while parsing"),
        ("test:not_a_map", "Not a map: []"),
    ] {
        assert!(
            lines.iter().any(|line| line
                .contains(&format!("Failed to parse {id} from pack {PACK}: "))
                && line.contains(cause)
                && line.contains("(skipped minecraft:loot_table entry)")),
            "{id}: {log}"
        );
    }
}

#[test]
fn a_predicate_that_does_not_decode_is_skipped() {
    let world = world(&[
        (
            "test/predicate/good.json",
            r#"{"type": "minecraft:survives_explosion"}"#,
        ),
        ("test/predicate/no_condition.json", r#"{"chance": 0.5}"#),
        ("test/predicate/broken.json", "{"),
    ]);
    let manager = DatapackManager::new();
    let log = load(&manager, &RecipeManager::default(), world.path());
    assert_eq!(manager.get_predicate_ids(), ["test:good"]);
    let lines = skipped(&log);
    assert_eq!(lines.len(), 2, "{log}");
    assert!(
        lines.iter().any(|line| line.contains(&format!(
            "Failed to parse test:no_condition from pack {PACK}: No key type in MapLike["
        )) && line.contains("(skipped minecraft:predicate entry)")),
        "{log}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Failed to parse test:broken from pack")),
        "{log}"
    );
}

#[test]
fn an_item_modifier_that_does_not_decode_is_skipped() {
    let world = world(&[
        (
            "test/item_modifier/good.json",
            r#"{"type": "minecraft:set_count", "count": 5}"#,
        ),
        ("test/item_modifier/no_function.json", r#"{"count": 5}"#),
        ("test/item_modifier/not_a_string.json", r#"{"type": 5}"#),
    ]);
    let manager = DatapackManager::new();
    let log = load(&manager, &RecipeManager::default(), world.path());
    assert_eq!(manager.get_item_modifier_ids(), ["test:good"]);
    let lines = skipped(&log);
    assert_eq!(lines.len(), 2, "{log}");
    assert!(
        lines.iter().any(|line| line.contains(&format!(
            "Failed to parse test:no_function from pack {PACK}: No key type in MapLike["
        )) && line.contains("(skipped minecraft:item_modifier entry)")),
        "{log}"
    );
    assert!(
        lines.iter().any(|line| line.contains(&format!(
            "Failed to parse test:not_a_string from pack {PACK}: Not a string: 5"
        ))),
        "{log}"
    );
}

#[test]
fn an_advancement_that_does_not_decode_is_skipped() {
    let world = world(&[
        (
            "test/advancement/good.json",
            r#"{"criteria": {"tick": {"trigger": "minecraft:tick"}}}"#,
        ),
        ("test/advancement/empty.json", r#"{"criteria": {}}"#),
        (
            "test/advancement/no_criteria.json",
            r#"{"parent": "minecraft:story/root"}"#,
        ),
        (
            "test/advancement/no_trigger.json",
            r#"{"criteria": {"a": {"conditions": {}}}}"#,
        ),
        ("test/advancement/broken.json", r#"{"criteria": "#),
    ]);
    let manager = DatapackManager::new();
    let log = load(&manager, &RecipeManager::default(), world.path());
    let loaded: Vec<String> = manager
        .get_advancement_ids()
        .into_iter()
        .filter(|id| id.starts_with("test:"))
        .collect();
    assert_eq!(loaded, ["test:good"]);
    let lines = skipped(&log);
    assert_eq!(lines.len(), 4, "{log}");
    for (id, cause) in [
        ("test:empty", "Advancement criteria cannot be empty"),
        ("test:no_criteria", "No key criteria in MapLike["),
        ("test:no_trigger", "No key trigger in MapLike["),
        ("test:broken", "EOF while parsing"),
    ] {
        assert!(
            lines.iter().any(|line| line
                .contains(&format!("Failed to parse {id} from pack {PACK}: {cause}"))
                && line.contains("(skipped minecraft:advancement entry)")),
            "{id}: {log}"
        );
    }
}

/// `/reload` loads the packs again. A bad file in the reload does not drop the good files of the
/// same reload.
#[test]
fn a_reload_with_a_bad_file_keeps_the_good_files() {
    let world = world(&[
        ("test/loot_table/good.json", r#"{"pools": []}"#),
        (
            "test/predicate/good.json",
            r#"{"type": "minecraft:survives_explosion"}"#,
        ),
    ]);
    let manager = DatapackManager::new();
    let recipes = RecipeManager::default();
    let first = load(&manager, &recipes, world.path());
    assert!(skipped(&first).is_empty(), "{first}");

    write(
        world.path(),
        &[
            ("test/loot_table/bad.json", r#"{"pools": 5}"#),
            ("test/predicate/bad.json", "{"),
            ("test/loot_table/good_too.json", r#"{"pools": []}"#),
        ],
    );
    let second = load(&manager, &recipes, world.path());
    assert_eq!(skipped(&second).len(), 2, "{second}");
    assert!(manager.get_loot_table("test:good").is_some());
    assert!(manager.get_loot_table("test:good_too").is_some());
    assert!(
        manager
            .get_loot_table_names()
            .iter()
            .all(|name| name != "test:bad")
    );
    assert_eq!(manager.get_predicate_ids(), ["test:good"]);
}

/// A file with a `neoforge:conditions` key is not an error line. The server does not evaluate
/// the key yet (task 6.9 of `modded-gameplay-parity`).
#[test]
fn a_file_with_load_conditions_logs_no_error() {
    let world = world(&[(
        "test/predicate/conditional.json",
        r#"{
            "neoforge:conditions": [{"type": "neoforge:never"}],
            "type": "minecraft:survives_explosion"
        }"#,
    )]);
    let log = load(
        &DatapackManager::new(),
        &RecipeManager::default(),
        world.path(),
    );
    assert!(skipped(&log).is_empty(), "{log}");
}

const GOOD_TRADE: &str = r#"{
    "wants": {"id": "minecraft:rotten_flesh", "count": 32},
    "gives": {"id": "minecraft:emerald", "count": 1}
}"#;

/// The cause that the log line of the file `id` has, or `None` when the log has no such line.
fn cause_of<'a>(log: &'a str, id: &str) -> Option<&'a str> {
    let prefix = format!("Failed to parse {id} from pack {PACK}: ");
    log.lines()
        .find_map(|line| line.split_once(&prefix).map(|(_, cause)| cause))
}

#[test]
fn a_slot_source_that_does_not_decode_is_skipped() {
    let world = world(&[
        (
            "test/slot_source/good.json",
            r#"{"type": "minecraft:slot_range", "slot_range": "armor.*"}"#,
        ),
        (
            "test/slot_source/no_type.json",
            r#"{"slot_range": "armor.*"}"#,
        ),
        ("test/slot_source/broken.json", "{"),
    ]);
    let manager = DatapackManager::new();
    let log = load(&manager, &RecipeManager::default(), world.path());
    assert_eq!(manager.get_slot_source_ids(), ["test:good"]);
    assert_eq!(skipped(&log).len(), 2, "{log}");
    assert!(
        cause_of(&log, "test:no_type")
            .unwrap()
            .starts_with("No key type in MapLike["),
        "{log}"
    );
    assert!(
        cause_of(&log, "test:broken")
            .unwrap()
            .contains("(skipped minecraft:slot_source entry)"),
        "{log}"
    );
}

#[test]
fn a_context_provider_that_does_not_decode_is_skipped() {
    let world = world(&[
        ("test/context_int_provider/constant.json", "5"),
        (
            "test/context_int_provider/typed.json",
            r#"{"type": "minecraft:constant", "value": 3}"#,
        ),
        ("test/context_int_provider/text.json", r#""five""#),
        ("test/context_int_provider/no_type.json", r#"{"value": 3}"#),
        ("test/context_float_provider/constant.json", "0.5"),
        ("test/context_float_provider/list.json", "[1.0]"),
        ("test/context_float_provider/broken.json", "{"),
    ]);
    let manager = DatapackManager::new();
    let log = load(&manager, &RecipeManager::default(), world.path());
    let test_ids = |names: Vec<String>| -> Vec<String> {
        let mut ids: Vec<String> = names
            .into_iter()
            .filter(|name| name.starts_with("test:"))
            .collect();
        ids.sort();
        ids
    };
    assert_eq!(
        test_ids(manager.get_context_int_provider_names()),
        ["test:constant", "test:typed"]
    );
    assert_eq!(
        test_ids(manager.get_context_float_provider_names()),
        ["test:constant"]
    );
    assert_eq!(skipped(&log).len(), 4, "{log}");
    assert!(
        cause_of(&log, "test:text")
            .unwrap()
            .starts_with("Not a number: \"five\""),
        "{log}"
    );
    assert!(
        cause_of(&log, "test:no_type")
            .unwrap()
            .contains("(skipped minecraft:context_int_provider entry)"),
        "{log}"
    );
    assert!(
        cause_of(&log, "test:list")
            .unwrap()
            .contains("(skipped minecraft:context_float_provider entry)"),
        "{log}"
    );
}

#[test]
fn a_villager_trade_and_a_trade_set_that_do_not_decode_are_skipped() {
    let world = world(&[
        ("test/villager_trade/good.json", GOOD_TRADE),
        ("test/villager_trade/bad.json", r#"{"wants": 5}"#),
        (
            "test/trade_set/good.json",
            r#"{"amount": 1, "trades": ["test:good"]}"#,
        ),
        ("test/trade_set/bad.json", r#"{"amount": "many"}"#),
        (
            "test/tags/villager_trade/good_tag.json",
            r#"{"values": ["test:good"]}"#,
        ),
        ("test/tags/villager_trade/bad_tag.json", r#"{"values": 5}"#),
        (
            "test/trade_set/tagged.json",
            r##"{"amount": 1, "trades": "#test:good_tag"}"##,
        ),
        (
            "test/trade_set/badly_tagged.json",
            r##"{"amount": 1, "trades": "#test:bad_tag"}"##,
        ),
    ]);
    let manager = DatapackManager::new();
    let log = load(&manager, &RecipeManager::default(), world.path());
    let trades = |set: &str| manager.get_trade_set(set).map(|set| set.trades.len());
    assert_eq!(trades("test:good"), Some(1));
    assert_eq!(trades("test:bad"), None);
    assert_eq!(trades("test:tagged"), Some(1));
    assert_ne!(trades("test:badly_tagged"), Some(1));
    assert_eq!(skipped(&log).len(), 3, "{log}");
    assert!(
        cause_of(&log, "test:bad")
            .unwrap()
            .contains("(skipped minecraft:villager_trade entry)")
            || log.contains("(skipped minecraft:trade_set entry)"),
        "{log}"
    );
    for registry in [
        "minecraft:villager_trade",
        "minecraft:trade_set",
        "minecraft:tags/villager_trade",
    ] {
        assert!(
            log.contains(&format!("(skipped {registry} entry)")),
            "{registry}: {log}"
        );
    }
}

/// A file that holds only `neoforge:conditions` removes an entry on NeoForge. It cannot decode,
/// so the loader skips it with a debug line and loads no empty entry.
#[test]
fn a_removal_stub_is_skipped_without_an_error_line() {
    let stub = r#"{"neoforge:conditions": [{"type": "neoforge:false"}]}"#;
    let world = world(&[
        ("test/recipe/stub.json", stub),
        ("test/loot_table/stub.json", stub),
        ("test/predicate/stub.json", stub),
        ("test/item_modifier/stub.json", stub),
        ("test/advancement/stub.json", stub),
        ("test/slot_source/stub.json", stub),
        ("test/context_int_provider/stub.json", stub),
        ("test/context_float_provider/stub.json", stub),
        ("test/villager_trade/stub.json", stub),
        ("test/trade_set/stub.json", stub),
        ("test/tags/villager_trade/stub.json", stub),
    ]);
    let manager = DatapackManager::new();
    let recipes = RecipeManager::default();
    let log = load(&manager, &recipes, world.path());
    assert!(skipped(&log).is_empty(), "{log}");
    assert!(recipe_ids(&recipes).is_empty());
    assert!(
        manager
            .get_loot_table_names()
            .iter()
            .all(|name| !name.starts_with("test:"))
    );
    assert!(manager.get_predicate_ids().is_empty());
    assert!(manager.get_item_modifier_ids().is_empty());
    assert!(
        manager
            .get_advancement_ids()
            .iter()
            .all(|id| !id.starts_with("test:"))
    );
    assert!(manager.get_slot_source_ids().is_empty());
    assert!(manager.get_trade_set("test:stub").is_none());
}

/// Vanilla reads a data file as a map. A list at the root is "Not a map", not a missing key.
#[test]
fn a_file_with_a_list_at_the_root_is_not_a_map() {
    let world = world(&[
        ("test/recipe/list.json", "[]"),
        ("test/predicate/list.json", "[]"),
        ("test/item_modifier/list.json", "[]"),
        ("test/advancement/list.json", "[]"),
        ("test/slot_source/list.json", "[]"),
    ]);
    let manager = DatapackManager::new();
    let log = load(&manager, &RecipeManager::default(), world.path());
    let lines = skipped(&log);
    assert_eq!(lines.len(), 5, "{log}");
    assert!(
        lines.iter().all(|line| line.contains(": Not a map: []")),
        "{log}"
    );
}

#[test]
fn a_type_that_is_no_identifier_is_skipped() {
    let world = world(&[
        (
            "test/predicate/bad_case.json",
            r#"{"type": "Minecraft:Bad"}"#,
        ),
        ("test/item_modifier/bad_path.json", r#"{"type": "a:b:c"}"#),
        ("test/slot_source/bad_char.json", r#"{"type": "no way"}"#),
        ("test/recipe/bad_type.json", r#"{"type": "Bad"}"#),
        (
            "test/predicate/unprefixed.json",
            r#"{"type": "survives_explosion"}"#,
        ),
    ]);
    let manager = DatapackManager::new();
    let log = load(&manager, &RecipeManager::default(), world.path());
    assert_eq!(skipped(&log).len(), 4, "{log}");
    assert_eq!(
        cause_of(&log, "test:bad_case")
            .unwrap()
            .split(" (skipped")
            .next(),
        Some(
            "Not a valid resource location: Minecraft:Bad Non [a-z0-9_.-] character in namespace of identifier: Minecraft:Bad"
        )
    );
    assert!(
        cause_of(&log, "test:bad_path")
            .unwrap()
            .starts_with("Not a valid resource location: a:b:c Non [a-z0-9/._-] character in path of location: a:b:c"),
        "{log}"
    );
    assert!(cause_of(&log, "test:bad_char").is_some(), "{log}");
    assert!(cause_of(&log, "test:bad_type").is_some(), "{log}");
    assert_eq!(manager.get_predicate_ids(), ["test:unprefixed"]);
}

#[test]
fn an_unknown_special_crafting_recipe_is_skipped() {
    let world = world(&[
        (
            "test/recipe/known.json",
            r#"{"type": "minecraft:crafting_special_repairitem"}"#,
        ),
        (
            "test/recipe/unknown.json",
            r#"{"type": "minecraft:crafting_special_nothing"}"#,
        ),
    ]);
    let recipes = RecipeManager::default();
    let log = load(&DatapackManager::new(), &recipes, world.path());
    assert_eq!(recipe_ids(&recipes), ["test:known"]);
    one_skipped(
        &log,
        "test:unknown",
        "Unknown registry key in ResourceKey[minecraft:root / minecraft:recipe_serializer]: minecraft:crafting_special_nothing",
    );
}

#[test]
fn an_advancement_with_an_unknown_trigger_is_skipped() {
    let world = world(&[
        (
            "test/advancement/known.json",
            r#"{"criteria": {"a": {"trigger": "tick"}}}"#,
        ),
        (
            "test/advancement/unknown.json",
            r#"{"criteria": {"a": {"trigger": "test:nothing"}}}"#,
        ),
        (
            "test/advancement/bad_id.json",
            r#"{"criteria": {"a": {"trigger": "Not Valid"}}}"#,
        ),
        (
            "test/advancement/not_a_map.json",
            r#"{"criteria": {"a": 5}}"#,
        ),
    ]);
    let manager = DatapackManager::new();
    let log = load(&manager, &RecipeManager::default(), world.path());
    let loaded: Vec<String> = manager
        .get_advancement_ids()
        .into_iter()
        .filter(|id| id.starts_with("test:"))
        .collect();
    assert_eq!(loaded, ["test:known"]);
    assert_eq!(skipped(&log).len(), 3, "{log}");
    assert!(
        cause_of(&log, "test:unknown").unwrap().starts_with(
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:trigger_type]: test:nothing"
        ),
        "{log}"
    );
    assert!(
        cause_of(&log, "test:bad_id")
            .unwrap()
            .starts_with("Not a valid resource location: Not Valid"),
        "{log}"
    );
    assert!(
        cause_of(&log, "test:not_a_map")
            .unwrap()
            .starts_with("Not a map: 5"),
        "{log}"
    );
    // The unprefixed name reads as `minecraft:tick`.
    assert_eq!(
        manager.get_advancement("test:known").unwrap().criteria["a"].0,
        "minecraft:tick"
    );
}
