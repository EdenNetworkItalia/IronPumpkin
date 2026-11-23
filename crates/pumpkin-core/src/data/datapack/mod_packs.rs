//! Data packs that native mods ship inside the server binary.
//!
//! On `NeoForge` every mod jar is also a data pack: the `data/` folder of the jar loads as mod
//! resources. A native mod of `IronPumpkin` is compiled into the binary, so it embeds the files of
//! its pack and hands them to this module with [`set_mod_data_packs`].
//!
//! The loaders of [`DatapackManager`](super::DatapackManager) read directories. So each load
//! writes the embedded files of every mod to `mod_data/<mod id>/` in the server directory, then
//! runs the same loaders as for a world data pack on that directory. Every file type that a world
//! data pack supports works from a mod pack, and the loaders need no second code path.
//!
//! Position: the packs load after the embedded vanilla data and before the world data packs, in
//! mod load order, so a world data pack overrides a mod pack and a mod pack overrides vanilla.
//! `NeoForge` gives the same order: the `mod_data` pack is required, and sits above `vanilla` and
//! below the packs that a world enables.

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
    sync::OnceLock,
};

use tracing::{info, warn};

use super::{LoadedDatapack, PackContentAccumulators, load_pack_contents, read_pack_mcmeta};

/// The folder, in the server directory, that the embedded packs are written to.
pub const MOD_DATA_DIR: &str = "mod_data";

/// The file that marks a folder of [`MOD_DATA_DIR`] as written by the server. A folder without
/// it is never replaced or deleted.
const MARKER_FILE: &str = ".ironpumpkin-mod-data";

/// The files of an embedded pack: the path relative to the pack root, with `/` as separator, and
/// the contents.
pub type PackFiles = &'static [(&'static str, &'static [u8])];

/// The data pack of one native mod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModDataPack {
    /// The id of the mod. It names the folder in [`MOD_DATA_DIR`] and the pack (`mod/<mod id>`).
    pub mod_id: String,
    /// The files of the pack, for example `data/mymod/loot_table/gems.json` and `pack.mcmeta`.
    pub files: PackFiles,
}

static MOD_DATA_PACKS: OnceLock<Vec<ModDataPack>> = OnceLock::new();

/// Records the data packs of the native mods, once, in mod load order, before the first load of
/// the data packs. A mod without files has no pack and is left out by the caller.
pub fn set_mod_data_packs(packs: Vec<ModDataPack>) {
    let _ = MOD_DATA_PACKS.set(packs);
}

/// The data packs of the native mods in load order. Empty before [`set_mod_data_packs`] and
/// when no mod ships files.
#[must_use]
pub fn mod_data_packs() -> &'static [ModDataPack] {
    MOD_DATA_PACKS.get().map_or(&[], Vec::as_slice)
}

/// The id of the pack of a mod: `mod/<mod id>`, as `NeoForge` names it.
#[must_use]
pub fn pack_id(mod_id: &str) -> String {
    format!("mod/{mod_id}")
}

/// Writes the pack of every mod below `root` and loads it, in order, into the accumulators.
pub(super) fn load_mod_packs(
    root: &Path,
    packs: &[ModDataPack],
    acc: &mut PackContentAccumulators<'_>,
    loaded_packs: &mut Vec<LoadedDatapack>,
) {
    for pack in packs {
        let pack_path = match extract(root, pack) {
            Ok(path) => path,
            Err(error) => {
                warn!(
                    "Skipping the data pack of mod '{}': cannot write it to '{}': {error}",
                    pack.mod_id,
                    root.display()
                );
                continue;
            }
        };
        let (description, pack_format, _) = read_pack_mcmeta(&pack_path);
        let counts = load_pack_contents(&pack_path, acc);
        info!(
            "Loaded the data pack of mod '{}': {} file(s), {} recipe(s), {} function(s), {} loot table(s), {} advancement(s), {} tag(s)",
            pack.mod_id,
            pack.files.len(),
            counts.recipes,
            counts.functions,
            counts.loot_tables,
            counts.advancements,
            counts.tags,
        );
        let id = pack_id(&pack.mod_id);
        loaded_packs.push(LoadedDatapack {
            name: id.clone(),
            id,
            description: if description.is_empty() {
                format!("Data of mod {}", pack.mod_id)
            } else {
                description
            },
            pack_format,
            root_path: pack_path,
            recipe_count: counts.recipes,
            function_count: counts.functions,
            // A client that knows a pack skips its registry entries. The client has the Java half
            // of the mod, whose files can differ from these, so the server always sends them.
            known_packs: Vec::new(),
        });
    }
}

/// Replaces `root/<mod id>` with the files of `pack` and returns that folder.
///
/// # Errors
///
/// Returns an error when the mod id or a file path is not a plain relative path, when the folder
/// exists and the server did not write it, or when a write fails.
fn extract(root: &Path, pack: &ModDataPack) -> io::Result<PathBuf> {
    // The mod id is one folder name.
    check_relative(&pack.mod_id)?;
    if pack.mod_id.contains('/') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("'{}' is not a single folder name", pack.mod_id),
        ));
    }
    let dir = root.join(&pack.mod_id);
    if dir.exists() {
        if !dir.join(MARKER_FILE).is_file() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "'{}' exists and was not written by the server, move it away",
                    dir.display()
                ),
            ));
        }
        // A file that a newer version of the mod no longer ships must not load again.
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(MARKER_FILE), b"")?;
    for (path, contents) in pack.files {
        check_relative(path)?;
        let target = dir.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, contents)?;
    }
    Ok(dir)
}

/// Accepts only a path of normal components, so a pack cannot write outside its folder.
fn check_relative(path: &str) -> io::Result<()> {
    let plain = !path.is_empty()
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if plain {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("'{path}' is not a plain relative path"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data::datapack::DatapackManager, server::recipe::RecipeManager,
        world::loot::LootTableHandle,
    };

    const LOOT_TABLE: &[u8] = br#"{"type":"minecraft:block","pools":[]}"#;

    fn pack(mod_id: &str, files: PackFiles) -> ModDataPack {
        ModDataPack {
            mod_id: mod_id.to_owned(),
            files,
        }
    }

    #[test]
    fn extract_writes_every_file_below_the_mod_folder() {
        let root = tempfile::tempdir().unwrap();
        let files: PackFiles = &[
            ("pack.mcmeta", b"{}"),
            ("data/gems/loot_table/blocks/ruby.json", LOOT_TABLE),
        ];
        let dir = extract(root.path(), &pack("gems", files)).unwrap();
        assert_eq!(dir, root.path().join("gems"));
        assert_eq!(
            fs::read(dir.join("data/gems/loot_table/blocks/ruby.json")).unwrap(),
            LOOT_TABLE
        );
        assert!(dir.join("pack.mcmeta").is_file());
    }

    #[test]
    fn extract_drops_the_files_a_newer_pack_no_longer_ships() {
        let root = tempfile::tempdir().unwrap();
        let old: PackFiles = &[("data/gems/loot_table/old.json", LOOT_TABLE)];
        let new: PackFiles = &[("data/gems/loot_table/new.json", LOOT_TABLE)];
        extract(root.path(), &pack("gems", old)).unwrap();
        let dir = extract(root.path(), &pack("gems", new)).unwrap();
        assert!(!dir.join("data/gems/loot_table/old.json").exists());
        assert!(dir.join("data/gems/loot_table/new.json").is_file());
    }

    #[test]
    fn extract_never_replaces_a_folder_the_server_did_not_write() {
        let root = tempfile::tempdir().unwrap();
        let foreign = root.path().join("gems");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("notes.txt"), b"keep").unwrap();
        let error = extract(root.path(), &pack("gems", &[])).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(foreign.join("notes.txt")).unwrap(), b"keep");
    }

    #[test]
    fn extract_refuses_a_path_that_leaves_the_pack_folder() {
        let root = tempfile::tempdir().unwrap();
        for bad in ["../escape.json", "/abs.json", "", "a\\b.json"] {
            let files: &'static [(&'static str, &'static [u8])] =
                Box::leak(Box::new([(bad, b"x".as_slice())]));
            let error = extract(root.path(), &pack("gems", files)).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{bad}");
        }
        for bad_id in ["..", ".", "a/b", ""] {
            let error = extract(root.path(), &pack(bad_id, &[])).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{bad_id}");
        }
        assert!(!root.path().join("escape.json").exists());
    }

    #[test]
    fn the_pack_id_is_mod_slash_the_mod_id() {
        assert_eq!(pack_id("gems"), "mod/gems");
    }

    const ONE_POOL: &[u8] =
        br#"{"type":"minecraft:block","pools":[{"rolls":1,"entries":[{"type":"minecraft:item","name":"minecraft:diamond"}]}]}"#;
    const TWO_POOLS: &[u8] = br#"{"type":"minecraft:block","pools":[
        {"rolls":1,"entries":[{"type":"minecraft:item","name":"minecraft:diamond"}]},
        {"rolls":1,"entries":[{"type":"minecraft:item","name":"minecraft:emerald"}]}]}"#;
    const TAG: &[u8] = br#"{"values":["minecraft:diamond"]}"#;
    const WORLD_TAG: &[u8] = br#"{"values":["minecraft:emerald"]}"#;
    const FUNCTION: &[u8] = b"say hello\n";

    /// The number of pools of a loot table that the manager resolves, or `None` when it has none.
    fn pools(manager: &DatapackManager, key: &str) -> Option<usize> {
        match manager.get_loot_table(key)? {
            LootTableHandle::Dynamic(table) => Some(table.pools.len()),
            LootTableHandle::Static(table) => Some(table.pools.len()),
        }
    }

    /// Loads `mod_packs` and, when `world_files` is not empty, one enabled world data pack
    /// `file/world_pack` with those files, into a fresh manager.
    fn load(mod_packs: &[ModDataPack], world_files: &[(&str, &[u8])]) -> DatapackManager {
        let world = tempfile::tempdir().unwrap();
        let mod_root = tempfile::tempdir().unwrap();
        let mut enabled = Vec::new();
        if !world_files.is_empty() {
            let pack_dir = world.path().join("datapacks/world_pack");
            for (path, contents) in world_files {
                let target = pack_dir.join(path);
                fs::create_dir_all(target.parent().unwrap()).unwrap();
                fs::write(target, contents).unwrap();
            }
            enabled.push("file/world_pack".to_owned());
        }
        let manager = DatapackManager::new();
        manager.load_all_with_mod_packs(
            world.path(),
            &enabled,
            &RecipeManager::default(),
            mod_packs,
            mod_root.path(),
        );
        manager
    }

    #[test]
    fn a_mod_pack_loads_through_the_loaders_of_a_world_data_pack() {
        let files: PackFiles = &[
            ("data/gems/loot_table/blocks/ruby.json", ONE_POOL),
            ("data/gems/tags/item/gems.json", TAG),
            ("data/gems/function/hello.mcfunction", FUNCTION),
        ];
        let manager = load(&[pack("gems", files)], &[]);

        assert_eq!(pools(&manager, "gems:blocks/ruby"), Some(1));
        assert_eq!(
            manager.get_tag_values("item", "gems:gems"),
            Some(vec!["minecraft:diamond".to_owned()])
        );
        assert!(
            manager
                .get_function_names()
                .contains(&"gems:hello".to_owned())
        );
        let loaded = manager.get_loaded_packs();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "mod/gems");
        assert_eq!(loaded[0].function_count, 1);
        assert!(loaded[0].known_packs.is_empty());
    }

    #[test]
    fn a_mod_pack_overrides_vanilla() {
        let vanilla = pools(&DatapackManager::new(), "minecraft:blocks/stone").unwrap();
        assert_ne!(
            vanilla, 2,
            "the test needs a vanilla table that is not the override"
        );
        let files: PackFiles = &[("data/minecraft/loot_table/blocks/stone.json", TWO_POOLS)];
        let manager = load(&[pack("gems", files)], &[]);
        assert_eq!(pools(&manager, "minecraft:blocks/stone"), Some(2));
    }

    #[test]
    fn a_world_data_pack_overrides_a_mod_pack() {
        let files: PackFiles = &[("data/gems/loot_table/blocks/ruby.json", ONE_POOL)];
        let manager = load(
            &[pack("gems", files)],
            &[("data/gems/loot_table/blocks/ruby.json", TWO_POOLS)],
        );
        assert_eq!(pools(&manager, "gems:blocks/ruby"), Some(2));
    }

    #[test]
    fn a_world_data_pack_that_does_not_replace_a_tag_adds_to_the_mod_tag() {
        let files: PackFiles = &[("data/gems/tags/item/gems.json", TAG)];
        let manager = load(
            &[pack("gems", files)],
            &[("data/gems/tags/item/gems.json", WORLD_TAG)],
        );
        assert_eq!(
            manager.get_tag_values("item", "gems:gems"),
            Some(vec![
                "minecraft:diamond".to_owned(),
                "minecraft:emerald".to_owned()
            ])
        );
    }

    #[test]
    fn a_later_mod_overrides_an_earlier_mod() {
        let first: PackFiles = &[("data/shared/loot_table/box.json", ONE_POOL)];
        let second: PackFiles = &[("data/shared/loot_table/box.json", TWO_POOLS)];
        let manager = load(&[pack("first", first), pack("second", second)], &[]);
        assert_eq!(pools(&manager, "shared:box"), Some(2));
        let ids: Vec<String> = manager
            .get_loaded_packs()
            .into_iter()
            .map(|pack| pack.id)
            .collect();
        assert_eq!(ids, ["mod/first", "mod/second"]);

        let manager = load(&[pack("second", second), pack("first", first)], &[]);
        assert_eq!(pools(&manager, "shared:box"), Some(1));
    }

    #[test]
    fn a_reload_picks_up_a_changed_pack_and_drops_a_removed_file() {
        let world = tempfile::tempdir().unwrap();
        let mod_root = tempfile::tempdir().unwrap();
        let manager = DatapackManager::new();
        let reload = |files: PackFiles| {
            manager.load_all_with_mod_packs(
                world.path(),
                &[],
                &RecipeManager::default(),
                &[pack("gems", files)],
                mod_root.path(),
            );
        };
        reload(&[("data/gems/loot_table/box.json", ONE_POOL)]);
        assert_eq!(pools(&manager, "gems:box"), Some(1));
        reload(&[("data/gems/loot_table/chest.json", TWO_POOLS)]);
        assert_eq!(pools(&manager, "gems:box"), None);
        assert_eq!(pools(&manager, "gems:chest"), Some(2));
    }

    #[test]
    fn no_mod_pack_writes_nothing() {
        let world = tempfile::tempdir().unwrap();
        let mod_root = tempfile::tempdir().unwrap();
        let missing = mod_root.path().join("mod_data");
        DatapackManager::new().load_all_with_mod_packs(
            world.path(),
            &[],
            &RecipeManager::default(),
            &[],
            &missing,
        );
        assert!(!missing.exists());
    }
}

/// A world data pack above a mod pack follows the override rules of vanilla for every loader.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod override_tests {
    use super::*;
    use crate::{data::datapack::DatapackManager, server::recipe::RecipeManager};

    fn load(
        mod_files: PackFiles,
        world_files: &[(&str, &[u8])],
    ) -> (DatapackManager, RecipeManager) {
        let world = tempfile::tempdir().unwrap();
        let mod_root = tempfile::tempdir().unwrap();
        let pack_dir = world.path().join("datapacks/world_pack");
        for (path, contents) in world_files {
            let target = pack_dir.join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, contents).unwrap();
        }
        let manager = DatapackManager::new();
        let recipes = RecipeManager::default();
        manager.load_all_with_mod_packs(
            world.path(),
            &["file/world_pack".to_owned()],
            &recipes,
            &[ModDataPack {
                mod_id: "gems".to_owned(),
                files: mod_files,
            }],
            mod_root.path(),
        );
        (manager, recipes)
    }

    const RECIPE_MOD: &[u8] = br#"{"type":"minecraft:crafting_shapeless","ingredients":["minecraft:diamond"],"result":{"id":"minecraft:emerald","count":1}}"#;
    const RECIPE_WORLD: &[u8] = br#"{"type":"minecraft:crafting_shapeless","ingredients":["minecraft:diamond"],"result":{"id":"minecraft:emerald","count":9}}"#;

    /// Vanilla: a later pack with the same recipe id replaces the earlier recipe.
    #[test]
    fn r_a_world_recipe_replaces_the_mod_recipe_with_the_same_id() {
        let (_, recipes) = load(
            &[("data/gems/recipe/ruby.json", RECIPE_MOD)],
            &[("data/gems/recipe/ruby.json", RECIPE_WORLD)],
        );
        let same_id = recipes
            .get_dynamic_recipes_internal()
            .iter()
            .filter(|r| r.recipe_id() == Some("gems:ruby"))
            .count();
        assert_eq!(same_id, 1, "both recipes are registered");
    }

    /// Vanilla: function tags follow the tag rules, so `replace: true` drops earlier entries.
    #[test]
    fn r_a_world_function_tag_with_replace_drops_the_mod_entries() {
        let (manager, _) = load(
            &[(
                "data/minecraft/tags/function/load.json",
                br#"{"values":["gems:init"]}"#,
            )],
            &[(
                "data/minecraft/tags/function/load.json",
                br#"{"replace":true,"values":["world:init"]}"#,
            )],
        );
        let tags = manager.function_tags.read().unwrap();
        assert_eq!(
            tags.get("minecraft:load"),
            Some(&vec!["world:init".to_owned()])
        );
    }
}
