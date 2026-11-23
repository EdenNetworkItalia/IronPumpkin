//! A native mod ships a data pack in its binary, and the server loads it as mod resources. It uses
//! `ironpumpkin-mods` and the data pack manager of the server, like the server does at startup.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::fs;

use ironpumpkin_mods::{
    ModInit, NativeMod,
    data_pack::{PackFiles, data_packs, include_data_pack},
    pumpkin_core::{
        data::datapack::DatapackManager, server::recipe::RecipeManager, world::loot::LootTableRef,
    },
    register_mod,
};

struct SampleMod;

impl NativeMod for SampleMod {
    fn id(&self) -> &'static str {
        "sample-mod"
    }

    fn display_name(&self) -> &'static str {
        "Sample Mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, _cx: &mut ModInit) {}

    fn data_pack(&self) -> PackFiles {
        include_data_pack!("tests/data_pack")
    }
}

/// A mod that ships no files has no pack.
struct PlainMod;

impl NativeMod for PlainMod {
    fn id(&self) -> &'static str {
        "plain-mod"
    }

    fn display_name(&self) -> &'static str {
        "Plain Mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, _cx: &mut ModInit) {}
}

register_mod!(SampleMod);
register_mod!(PlainMod);

const WORLD_LOOT_TABLE: &str = r#"{
  "type": "minecraft:block",
  "pools": [
    { "rolls": 1, "entries": [{ "type": "minecraft:item", "name": "minecraft:diamond" }] },
    { "rolls": 1, "entries": [{ "type": "minecraft:item", "name": "minecraft:emerald" }] }
  ]
}"#;

fn pools(manager: &DatapackManager, key: &str) -> usize {
    match manager.get_loot_table(key).unwrap().table {
        LootTableRef::Dynamic(table) => table.pools.len(),
        LootTableRef::Static(table) => table.pools.len(),
    }
}

/// Loads the packs of the linked mods, and the world data pack `file/world_pack` when `world_pack`
/// is a loot table for `sample:blocks/ruby_ore`.
fn load(world_pack: Option<&str>) -> DatapackManager {
    let world = tempfile::tempdir().unwrap();
    let mod_data = tempfile::tempdir().unwrap();
    let mut enabled = Vec::new();
    if let Some(loot_table) = world_pack {
        let dir = world
            .path()
            .join("datapacks/world_pack/data/sample/loot_table/blocks");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("ruby_ore.json"), loot_table).unwrap();
        enabled.push("file/world_pack".to_owned());
    }
    let manager = DatapackManager::new();
    manager.load_all_with_mod_packs(
        world.path(),
        &enabled,
        &RecipeManager::default(),
        &data_packs().unwrap(),
        mod_data.path(),
    );
    manager
}

#[test]
fn only_a_mod_that_ships_files_has_a_pack() {
    let packs = data_packs().unwrap();
    assert_eq!(packs.len(), 1);
    assert_eq!(packs[0].mod_id, "sample-mod");
    let paths: Vec<&str> = packs[0].files.iter().map(|(path, _)| *path).collect();
    assert_eq!(
        paths,
        [
            "data/sample/loot_table/blocks/ruby_ore.json",
            "data/sample/tags/item/gems.json",
            "pack.mcmeta"
        ]
    );
}

#[test]
fn the_server_sees_the_loot_table_and_the_tag_of_the_mod_pack() {
    let manager = load(None);
    assert_eq!(pools(&manager, "sample:blocks/ruby_ore"), 1);
    assert_eq!(
        manager.get_tag_values("item", "sample:gems"),
        Some(vec![
            "minecraft:diamond".to_owned(),
            "minecraft:emerald".to_owned()
        ])
    );
    let packs = manager.get_loaded_packs();
    assert_eq!(packs.len(), 1);
    assert_eq!(packs[0].id, "mod/sample-mod");
    assert_eq!(packs[0].description, "Data of the sample mod");
}

#[test]
fn a_world_data_pack_overrides_the_pack_of_the_mod() {
    // The same table id in both packs: the mod pack has one pool, the world pack two.
    let manager = load(Some(WORLD_LOOT_TABLE));
    assert_eq!(pools(&manager, "sample:blocks/ruby_ore"), 2);
}
