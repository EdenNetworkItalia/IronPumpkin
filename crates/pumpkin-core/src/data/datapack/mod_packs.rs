//! Data packs that native mods ship inside the server binary.
//!
//! On `NeoForge` every mod jar is also a data pack: the `data/` folder of the jar loads as mod
//! resources. A native mod of `IronPumpkin` is compiled into the binary, so it embeds the files of
//! its pack and hands them to this module with [`set_mod_data_packs`].
//!
//! The loaders of [`DatapackManager`](super::DatapackManager) read directories. So the first load
//! of a process writes the embedded files of every mod to `mod_data/<mod id>/` in the server
//! directory, then runs the same loaders as for a world data pack on that directory. Every file
//! type that a world data pack supports works from a mod pack, and the loaders need no second code
//! path. The embedded files cannot change inside a process, so a `/reload` and the load of a
//! second server of the same process only read the folder.
//!
//! Position: the packs load after the embedded vanilla data and before the world data packs, in
//! mod load order, so a world data pack overrides a mod pack and a mod pack overrides vanilla.
//! This is the order of `NeoForge` for a new world: the required `mod_data` pack sits above
//! `vanilla` and below the packs that the world enables. `NeoForge` appends `mod_data` to the end
//! of the saved pack list of a world that lacks it, so in an older world it can sit above the
//! packs of the world. `IronPumpkin` always puts the mod packs below the world packs.
//!
//! A mod id is a folder name, so the names that Windows reserves (`con`, `prn`, `aux`, `nul`,
//! `com1` and the others) cannot be written on Windows.

use std::{
    collections::BTreeSet,
    fs, io,
    path::{Component, Path, PathBuf},
    sync::{Mutex, OnceLock},
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
        let pack_path = match ensure_extracted(root, pack) {
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

/// The packs that this process has written, by folder. The embedded files cannot change inside a
/// process, so a load after the first only reads the folder. The lock is held while a pack is
/// written, so a second server that loads at the same time waits for the files instead of reading
/// a half-written folder.
static WRITTEN: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// Writes the pack to `root/<mod id>` once per process and returns the folder. Later calls, such
/// as a `/reload`, return the folder without a write, unless it lost its marker file.
fn ensure_extracted(root: &Path, pack: &ModDataPack) -> io::Result<PathBuf> {
    check_relative(&pack.mod_id)?;
    let dir = root.join(&pack.mod_id);
    let mut written = WRITTEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if written.contains(&dir) && dir.join(MARKER_FILE).is_file() {
        return Ok(dir);
    }
    extract(root, pack)?;
    written.insert(dir.clone());
    Ok(dir)
}

/// Replaces `root/<mod id>` with the files of `pack` and returns that folder.
///
/// The files go to `root/.staging-<mod id>` first, with the marker file as the first one. The old
/// folder is renamed to `root/.trash-<mod id>`, the staging folder takes its name, and the trash
/// is deleted. So `root/<mod id>` never exists without its marker file, whatever the moment of a
/// crash. A folder with a name that starts with a dot is the server's by name: the next call
/// deletes the leftovers of this mod.
///
/// # Errors
///
/// Returns an error when the mod id or a file path is not a plain relative path, when the folder
/// exists, is not empty and the server did not write it, or when a write fails.
fn extract(root: &Path, pack: &ModDataPack) -> io::Result<PathBuf> {
    // The mod id is one folder name, and no path may leave it.
    check_relative(&pack.mod_id)?;
    if pack.mod_id.contains('/') || pack.mod_id.starts_with('.') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("'{}' is not a valid folder name", pack.mod_id),
        ));
    }
    for (path, _) in pack.files {
        check_relative(path)?;
    }
    fs::create_dir_all(root)?;
    let dir = root.join(&pack.mod_id);
    let staging = root.join(format!(".staging-{}", pack.mod_id));
    let trash = root.join(format!(".trash-{}", pack.mod_id));
    remove_if_exists(&staging)?;
    remove_if_exists(&trash)?;

    let has_old = match dir.symlink_metadata() {
        Ok(_) if is_owned(&dir) => true,
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "'{}' exists and was not written by the server, move it away",
                    dir.display()
                ),
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error),
    };

    fs::create_dir(&staging)?;
    fs::write(staging.join(MARKER_FILE), b"")?;
    for (path, contents) in pack.files {
        let target = staging.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, contents)?;
    }
    if has_old {
        fs::rename(&dir, &trash)?;
    }
    if let Err(error) = fs::rename(&staging, &dir) {
        if has_old {
            let _ = fs::rename(&trash, &dir);
        }
        return Err(error);
    }
    // The pack is in place. A failed delete leaves a dot folder that the next load removes.
    if let Err(error) = remove_if_exists(&trash) {
        warn!("Cannot delete '{}': {error}", trash.display());
    }
    Ok(dir)
}

/// Whether the server may replace `dir`: it holds the marker file, or it is an empty folder that
/// a crash of an older version can have left. A symbolic link counts as what it points to, and a
/// replace removes the link, not the target.
fn is_owned(dir: &Path) -> bool {
    dir.join(MARKER_FILE).is_file()
        || (dir.is_dir() && fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_none()))
}

/// Deletes a file, a link or a folder that the server named itself. A link is unlinked, never
/// followed.
fn remove_if_exists(path: &Path) -> io::Result<()> {
    match path.symlink_metadata() {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Accepts only a path of normal components, so a pack cannot write outside its folder.
fn check_relative(path: &str) -> io::Result<()> {
    let plain = !path.is_empty()
        && !path.contains(['\\', '\0'])
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
    fn a_reload_reads_the_folder_that_the_first_load_wrote() {
        let world = tempfile::tempdir().unwrap();
        let mod_root = tempfile::tempdir().unwrap();
        let manager = DatapackManager::new();
        let files: PackFiles = &[("data/gems/loot_table/box.json", ONE_POOL)];
        let reload = || {
            manager.load_all_with_mod_packs(
                world.path(),
                &[],
                &RecipeManager::default(),
                &[pack("gems", files)],
                mod_root.path(),
            );
        };
        reload();
        assert_eq!(pools(&manager, "gems:box"), Some(1));
        // The second load does not write: a file that appears in the folder stays.
        let extra = mod_root.path().join("gems/data/gems/loot_table/extra.json");
        fs::write(&extra, TWO_POOLS).unwrap();
        reload();
        assert_eq!(pools(&manager, "gems:box"), Some(1));
        assert_eq!(pools(&manager, "gems:extra"), Some(2));
        // A folder that lost its marker is no longer the server's: the pack stays out.
        fs::remove_file(mod_root.path().join("gems").join(MARKER_FILE)).unwrap();
        reload();
        assert_eq!(pools(&manager, "gems:box"), None);
        assert!(extra.is_file());
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

    /// Vanilla: a tag without `replace` merges, also for villager trade tags.
    #[test]
    fn r_a_world_trade_tag_without_replace_merges_with_the_mod_tag() {
        let (manager, _) = load(
            &[(
                "data/gems/tags/villager_trade/t.json",
                br#"{"values":["gems:a"]}"#,
            )],
            &[(
                "data/gems/tags/villager_trade/t.json",
                br#"{"values":["gems:b"]}"#,
            )],
        );
        let registry = manager.trade_registry.read().unwrap();
        assert_eq!(
            registry.trade_tags.get("gems:t"),
            Some(&vec!["gems:a".to_owned(), "gems:b".to_owned()])
        );
    }

    #[test]
    fn a_world_trade_tag_with_replace_drops_the_mod_values() {
        let (manager, _) = load(
            &[(
                "data/gems/tags/villager_trade/t.json",
                br#"{"values":["gems:a"]}"#,
            )],
            &[(
                "data/gems/tags/villager_trade/t.json",
                br#"{"replace":true,"values":["gems:b"]}"#,
            )],
        );
        let registry = manager.trade_registry.read().unwrap();
        assert_eq!(
            registry.trade_tags.get("gems:t"),
            Some(&vec!["gems:b".to_owned()])
        );
    }

    #[test]
    fn r_a_world_tag_with_replace_drops_the_mod_values() {
        let (manager, _) = load(
            &[(
                "data/gems/tags/item/gems.json",
                br#"{"values":["minecraft:diamond"]}"#,
            )],
            &[(
                "data/gems/tags/item/gems.json",
                br#"{"replace":true,"values":["minecraft:emerald"]}"#,
            )],
        );
        assert_eq!(
            manager.get_tag_values("item", "gems:gems"),
            Some(vec!["minecraft:emerald".to_owned()])
        );
    }

    #[test]
    fn r_a_world_function_overrides_the_mod_function() {
        let (manager, _) = load(
            &[("data/gems/function/hello.mcfunction", b"say mod\n")],
            &[("data/gems/function/hello.mcfunction", b"say world\n")],
        );
        assert_eq!(
            manager.get_functions().get("gems:hello"),
            Some(&vec!["say world".to_owned()])
        );
    }

    #[test]
    fn r_a_world_predicate_and_item_modifier_override_the_mod_ones() {
        let (manager, _) = load(
            &[
                (
                    "data/gems/predicate/p.json",
                    br#"{"condition":"minecraft:survives_explosion","from":"mod"}"#,
                ),
                (
                    "data/gems/item_modifier/m.json",
                    br#"{"function":"minecraft:set_count","count":1}"#,
                ),
            ],
            &[
                (
                    "data/gems/predicate/p.json",
                    br#"{"condition":"minecraft:survives_explosion","from":"world"}"#,
                ),
                (
                    "data/gems/item_modifier/m.json",
                    br#"{"function":"minecraft:set_count","count":2}"#,
                ),
            ],
        );
        assert_eq!(manager.get_predicate("gems:p").unwrap()["from"], "world");
        assert_eq!(manager.get_item_modifier("gems:m").unwrap()["count"], 2);
    }
}

/// Adversarial tests of the extraction: folder names, links, crashes and concurrent loads.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod review_tests {
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    use super::*;
    use crate::{
        data::datapack::DatapackManager, server::recipe::RecipeManager,
        world::loot::LootTableHandle,
    };

    const LT: &[u8] = br#"{"type":"minecraft:block","pools":[]}"#;

    fn pack(mod_id: &str, files: PackFiles) -> ModDataPack {
        ModDataPack {
            mod_id: mod_id.to_owned(),
            files,
        }
    }

    fn leak(files: Vec<(&'static str, &'static [u8])>) -> PackFiles {
        Box::leak(files.into_boxed_slice())
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn r_bad_mod_ids_never_write_outside() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("mod_data");
        fs::create_dir(&root).unwrap();
        for id in [
            "..", ".", "", "a/b", "a\\b", "/abs", "../x", "x/..", "./a", "a\0b",
        ] {
            let result = extract(&root, &pack(id, &[("pack.mcmeta", b"{}")]));
            assert!(result.is_err(), "{id:?} accepted");
        }
        assert_eq!(names(base.path()), ["mod_data"]);
        assert!(names(&root).is_empty());
    }

    #[test]
    fn r_bad_file_paths_never_write_outside() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("mod_data");
        for bad in [
            "../escape.json",
            "data/../../escape.json",
            "data/../../../escape.json",
            "./x.json",
            "data/..",
            "..",
            "a\\..\\..\\escape.json",
        ] {
            let files = leak(vec![
                ("pack.mcmeta", b"{}".as_slice()),
                (bad, b"x".as_slice()),
            ]);
            let error = extract(&root, &pack("gems", files)).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{bad}");
        }
        let abs = base.path().join("abs_escape.json");
        let abs: &'static str = Box::leak(abs.to_str().unwrap().to_owned().into_boxed_str());
        let error = extract(&root, &pack("gems", leak(vec![(abs, b"x".as_slice())]))).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        // The paths are checked before anything is written.
        assert!(names(base.path()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn r_symlinked_mod_folder_with_marker_is_unlinked_not_followed() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("mod_data");
        fs::create_dir(&root).unwrap();
        let outside = base.path().join("precious");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join(MARKER_FILE), b"").unwrap();
        fs::write(outside.join("keep.txt"), b"keep").unwrap();
        symlink(&outside, root.join("gems")).unwrap();
        let dir = extract(&root, &pack("gems", &[("pack.mcmeta", b"{}")])).unwrap();
        assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"keep");
        assert!(!outside.join("pack.mcmeta").exists());
        assert!(!dir.symlink_metadata().unwrap().file_type().is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn r_symlinked_mod_folder_without_marker_is_refused() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("mod_data");
        fs::create_dir(&root).unwrap();
        let outside = base.path().join("precious");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep.txt"), b"keep").unwrap();
        symlink(&outside, root.join("gems")).unwrap();
        let error = extract(&root, &pack("gems", &[("pack.mcmeta", b"{}")])).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(names(&outside), ["keep.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn r_symlink_inside_owned_folder_is_not_followed_on_delete() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("mod_data");
        let outside = base.path().join("precious");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep.txt"), b"keep").unwrap();
        extract(&root, &pack("gems", &[("pack.mcmeta", b"{}")])).unwrap();
        symlink(&outside, root.join("gems/data")).unwrap();
        symlink(outside.join("keep.txt"), root.join("gems/link.txt")).unwrap();
        extract(&root, &pack("gems", &[("data/gems/loot_table/a.json", LT)])).unwrap();
        assert_eq!(names(&outside), ["keep.txt"]);
        assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"keep");
    }

    #[cfg(unix)]
    #[test]
    fn r_dangling_symlink_is_not_followed_on_create() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("mod_data");
        fs::create_dir(&root).unwrap();
        let target = base.path().join("created_by_server");
        symlink(&target, root.join("gems")).unwrap();
        let result = extract(&root, &pack("gems", &[("pack.mcmeta", b"{}")]));
        assert!(result.is_err());
        assert!(!target.exists());
    }

    #[test]
    fn r_a_regular_file_in_place_of_the_folder_is_refused_and_kept() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("mod_data");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("gems"), b"keep").unwrap();
        let error = extract(&root, &pack("gems", &[("pack.mcmeta", b"{}")])).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(root.join("gems")).unwrap(), b"keep");
    }

    #[test]
    fn r_planted_marker_lets_the_server_delete_foreign_files() {
        // By design: the marker is the only ownership proof.
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("mod_data");
        fs::create_dir_all(root.join("gems")).unwrap();
        fs::write(root.join("gems").join(MARKER_FILE), b"").unwrap();
        fs::write(root.join("gems/notes.txt"), b"mine").unwrap();
        extract(&root, &pack("gems", &[("pack.mcmeta", b"{}")])).unwrap();
        assert!(!root.join("gems/notes.txt").exists());
    }

    /// A crash before the write of the marker left an empty folder in an older layout. It holds
    /// nothing, so the pack replaces it.
    #[test]
    fn r_an_empty_folder_left_by_a_crash_does_not_block_the_pack() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("mod_data");
        fs::create_dir_all(root.join("gems")).unwrap();
        let files: PackFiles = &[("data/gems/loot_table/box.json", LT)];
        let dir = extract(&root, &pack("gems", files)).unwrap();
        assert!(dir.join("data/gems/loot_table/box.json").is_file());
        assert!(dir.join(MARKER_FILE).is_file());
    }

    /// Files without the marker are not the server's: the pack stays out and the files stay.
    #[test]
    fn r_files_without_marker_keep_the_mod_pack_out() {
        let world = tempfile::tempdir().unwrap();
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("mod_data");
        fs::create_dir_all(root.join("gems/data")).unwrap();
        fs::write(root.join("gems/data/left.json"), b"{}").unwrap();
        let files: PackFiles = &[("data/gems/loot_table/box.json", LT)];
        let manager = DatapackManager::new();
        manager.load_all_with_mod_packs(
            world.path(),
            &[],
            &RecipeManager::default(),
            &[pack("gems", files)],
            &root,
        );
        assert!(manager.get_loot_table("gems:box").is_none());
        assert!(root.join("gems/data/left.json").is_file());
    }

    /// The old folder goes to a trash folder and the new one comes from a staging folder, so the
    /// leftovers of a crash are dot folders. The next load deletes them.
    #[test]
    fn r_leftovers_of_a_crash_are_cleaned_and_never_block_the_pack() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("mod_data");
        fs::create_dir_all(root.join(".staging-gems/data")).unwrap();
        fs::write(root.join(".staging-gems/data/half.json"), b"{").unwrap();
        fs::create_dir_all(root.join(".trash-gems")).unwrap();
        fs::write(root.join(".trash-gems/old.json"), b"{}").unwrap();
        let files: PackFiles = &[("data/gems/loot_table/box.json", LT)];
        extract(&root, &pack("gems", files)).unwrap();
        assert_eq!(names(&root), ["gems"]);
        assert!(!root.join("gems/data/half.json").exists());
    }

    #[test]
    fn r_two_servers_loading_at_once_see_every_mod_file() {
        const N: usize = 300;
        let files = leak(
            (0..N)
                .map(|i| {
                    let path: &'static str =
                        Box::leak(format!("data/gems/loot_table/t{i}.json").into_boxed_str());
                    (path, LT)
                })
                .collect(),
        );
        let world = tempfile::tempdir().unwrap();
        let mod_root = tempfile::tempdir().unwrap();
        let packs = [pack("gems", files)];
        let bad = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| {
                    for _ in 0..20 {
                        let manager = DatapackManager::new();
                        manager.load_all_with_mod_packs(
                            world.path(),
                            &[],
                            &RecipeManager::default(),
                            &packs,
                            mod_root.path(),
                        );
                        let loaded = (0..N)
                            .filter(|i| {
                                matches!(
                                    manager.get_loot_table(&format!("gems:t{i}")),
                                    Some(LootTableHandle::Dynamic(_))
                                )
                            })
                            .count();
                        if loaded != N {
                            bad.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                });
            }
        });
        assert_eq!(bad.into_inner(), 0, "loads that missed mod files");
    }
}
