//! Scenarios of the spec `neoforge-loot` (`openspec/changes/neoforge-shaped-plugin-api`) on a real
//! server: a mod registers a modifier type, a world data pack ships the modifier files, and the
//! rolls of the server see them, after startup and after `/reload`. The modifier types freeze once
//! per process, so this file is its own test binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    loot::{
        GlobalLootModifier, ItemStack, LootContext, LootContextParameters, LootModifierError, Value,
    },
    pumpkin_core::{
        block::collect_loot,
        plugin::startup::STARTUP_HOOK,
        world::loot::{generate_loot_from_handle, generate_loot_with_context},
    },
    pumpkin_data::{Block, item::Item, loot_table},
    pumpkin_util::math::position::BlockPos,
    register_mod,
};

static DUPLICATE: Mutex<Option<Result<(), LootModifierError>>> = Mutex::new(None);

struct RemoveItem(&'static Item);

impl GlobalLootModifier for RemoveItem {
    fn apply(
        &self,
        mut loot: Vec<ItemStack>,
        _context: &mut LootContext<'_, '_>,
    ) -> Vec<ItemStack> {
        loot.retain(|stack| stack.item != self.0);
        loot
    }
}

fn remove_item(json: &Value) -> Result<Box<dyn GlobalLootModifier>, String> {
    let item = json
        .get("item")
        .and_then(Value::as_str)
        .and_then(Item::from_registry_key)
        .ok_or("no item")?;
    Ok(Box::new(RemoveItem(item)))
}

struct LootMod;

impl NativeMod for LootMod {
    fn id(&self) -> &'static str {
        "lootmod"
    }

    fn display_name(&self) -> &'static str {
        "Loot mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        cx.register_loot_modifier_type("lootmod:remove_item", remove_item)
            .unwrap();
        *DUPLICATE.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(cx.register_loot_modifier_type("lootmod:remove_item", remove_item));
    }
}

register_mod!(LootMod);

const PACK: &str = "glm";

fn write(dir: &Path, name: &str, json: &str) {
    let path = dir.join(format!("{name}.json"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, json).unwrap();
}

fn names(loot: &[ItemStack]) -> Vec<&'static str> {
    loot.iter().map(|stack| stack.item.registry_key).collect()
}

/// Writes the world data pack `PACK` and returns its `loot_modifiers` directory.
fn write_pack(datapacks: &Path) -> PathBuf {
    let pack = datapacks.join(PACK);
    let modifiers = pack.join("data/lootmod/loot_modifiers");
    write(
        &modifiers,
        "no_diamond",
        r#"{"type": "lootmod:remove_item", "item": "minecraft:diamond",
            "condition": {"type": "neoforge:loot_table_id", "loot_table_id": "minecraft:blocks/diamond_ore"}}"#,
    );
    write(
        &modifiers,
        "chests/extra_dirt",
        r#"{"type": "neoforge:add_table", "table": "minecraft:blocks/dirt",
            "condition": {"type": "neoforge:loot_table_id", "loot_table_id": "minecraft:chests/simple_dungeon"}}"#,
    );
    write(&modifiers, "unknown_type", r#"{"type": "lootmod:missing"}"#);
    write(
        &modifiers,
        "failed_decode",
        r#"{"type": "lootmod:remove_item"}"#,
    );
    write(
        &modifiers,
        "unknown_condition",
        r#"{"type": "lootmod:remove_item", "item": "minecraft:stone", "condition": {"type": "lootmod:nope"}}"#,
    );
    std::fs::write(
        pack.join("pack.mcmeta"),
        r#"{"pack": {"pack_format": 94, "description": "loot modifiers"}}"#,
    )
    .unwrap();
    modifiers
}

#[test]
fn modifiers_load_from_world_data_packs_and_change_the_rolls() {
    STARTUP_HOOK.set(ironpumpkin_mods::init_mods).unwrap();
    let server = support::server();
    let world = support::world();

    assert_eq!(
        DUPLICATE
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone(),
        Some(Err(LootModifierError::Duplicate(
            "lootmod:remove_item".to_owned()
        )))
    );
    assert_eq!(
        ModInit::new("late").register_loot_modifier_type("late:type", remove_item),
        Err(LootModifierError::RegistrationClosed)
    );
    assert!(server.datapack_manager.get_loot_modifiers().is_empty());

    let modifiers = write_pack(&server.basic_config.get_world_path().join("datapacks"));
    let mut level = (**server.level_info.load()).clone();
    level.data_packs.enabled.push(format!("file/{PACK}"));
    server.level_info.store(Arc::new(level));
    // `/reload`.
    server.reload_datapacks(server);

    let loaded = server.datapack_manager.get_loot_modifiers();
    assert_eq!(
        loaded.ids().collect::<Vec<_>>(),
        ["lootmod:chests/extra_dirt", "lootmod:no_diamond"]
    );

    let params = LootContextParameters {
        world: Some(&world),
        ..Default::default()
    };
    let pos = BlockPos::new(0, 64, 0);
    for _ in 0..8 {
        assert_eq!(
            names(&collect_loot(&world, &Block::DIAMOND_ORE, &pos, &params)),
            [] as [&str; 0]
        );
        assert_eq!(
            names(&collect_loot(&world, &Block::DIRT, &pos, &params)),
            ["dirt"]
        );
    }

    let dungeon = world.get_loot_table("chests/simple_dungeon").unwrap();
    for seed in 0..8 {
        let mut expected = names(&generate_loot_with_context(
            &loot_table::CHESTS_SIMPLE_DUNGEON,
            seed,
            &params,
        ));
        expected.push("dirt");
        assert_eq!(
            names(&generate_loot_from_handle(&dungeon, seed, &params)),
            expected
        );
    }

    std::fs::remove_file(modifiers.join("no_diamond.json")).unwrap();
    server.reload_datapacks(server);
    assert_eq!(
        server
            .datapack_manager
            .get_loot_modifiers()
            .ids()
            .collect::<Vec<_>>(),
        ["lootmod:chests/extra_dirt"]
    );
    assert_eq!(
        names(&collect_loot(&world, &Block::DIAMOND_ORE, &pos, &params)),
        ["diamond"]
    );

    // Review 166: a reload that removes every modifier gives the raw rolls again.
    std::fs::remove_file(modifiers.join("chests/extra_dirt.json")).unwrap();
    for name in ["unknown_type", "failed_decode", "unknown_condition"] {
        let _ = std::fs::remove_file(modifiers.join(format!("{name}.json")));
    }
    server.reload_datapacks(server);
    assert!(server.datapack_manager.get_loot_modifiers().is_empty());
    let dungeon = world.get_loot_table("chests/simple_dungeon").unwrap();
    for seed in 0..8 {
        assert_eq!(
            names(&generate_loot_from_handle(&dungeon, seed, &params)),
            names(&generate_loot_with_context(
                &loot_table::CHESTS_SIMPLE_DUNGEON,
                seed,
                &params,
            ))
        );
    }
}
