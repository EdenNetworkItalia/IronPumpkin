# Porting mod data packs from NeoForge

On NeoForge, every mod jar is also a data pack. The `data/` folder of the jar loads as mod resources: loot tables, tags, recipes, advancements, functions, loot modifiers, data maps and the other data files. The mod does not register them. NeoForge builds one `mod/<mod id>` pack for each jar, puts the packs in a required, hidden `mod_data` pack, and adds that pack to the server pack list. A world data pack overrides it.

An IronPumpkin native mod is compiled into the server binary, so it has no jar to read. The mod embeds a folder of its crate and returns it from `NativeMod::data_pack`. The server writes the files to `mod_data/<mod id>/` in the server directory and loads that folder with the loaders of the world data packs. Every file type that a world data pack supports works from a mod pack.

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Where the files are** | `src/main/resources/data/<namespace>/...` in the jar | A folder of the crate, for example `resources/data/<namespace>/...` |
| **How the mod ships them** | The Gradle build packs `src/main/resources` into the jar | `NativeMod::data_pack` returns `include_data_pack!("resources")`. The compiler embeds the files in the binary. |
| **Pack name** | `mod/<mod id>`, inside the required pack `mod_data` | `mod/<mod id>` in the loaded pack list. The server writes the files to `mod_data/<mod id>/`. |
| **Load order** | `vanilla`, then the mod packs in mod order, then the packs of the world | The embedded vanilla data, then the mod packs in mod load order, then the packs of the world. A later mod overrides an earlier mod. |
| **Pack on/off** | Required, hidden. `showAsDataPack` shows it. | Always enabled. It is not in the `/datapack` list and a player cannot disable it. |
| **`pack.mcmeta`** | Optional for a mod jar | Optional. The server reads the description from it. |
| **File types** | Every type that the game loads from a data pack | Every type that a world data pack supports in IronPumpkin, from the same loaders |
| **Conditions** | `neoforge:conditions` in a data file | Not supported yet. It waits for the datapack loader step of milestone M5. See [What is unsupported](./unsupported.md). |
| **Add a pack at run time** | `AddPackFindersEvent` | `NativeMod::add_pack_finders` gets `AddPackFindersEvent` once, after the mod lifecycle. `add_pack` takes a pack id and the files of `include_data_pack!`. The packs load after the packs of the mods and before the packs of the world. |
| **Client files** | `assets/` of the same jar | Not part of the server pack. Keep them in the Java client mod. |

---

## Code Comparison: a mod that ships a loot table and a tag

### 1. NeoForge implementation (Java)

The mod has no code for its data. The files sit in the resources folder of the Gradle project:

```text
src/main/resources/
  data/gems/loot_table/blocks/ruby_ore.json
  data/gems/tags/item/gems.json
```

### 2. IronPumpkin implementation (Rust)

The crate has the same folder. The path of `include_data_pack!` is relative to the folder of the `Cargo.toml` of the crate:

```text
my-mod/
  Cargo.toml
  src/lib.rs
  resources/
    pack.mcmeta               (optional)
    data/gems/loot_table/blocks/ruby_ore.json
    data/gems/tags/item/gems.json
```

The snippet is a doc-test of `ironpumpkin-mods`. It embeds the test folder of that crate, which has the same layout. A mod crate writes `include_data_pack!("resources")`.

```rust
use ironpumpkin_mods::{
    ModInit, NativeMod,
    data_pack::{PackFiles, include_data_pack},
    register_mod,
};

struct Gems;

impl NativeMod for Gems {
    fn id(&self) -> &'static str {
        "gems"
    }

    fn display_name(&self) -> &'static str {
        "Gems"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, _cx: &mut ModInit) {}

    fn data_pack(&self) -> PackFiles {
        include_data_pack!("tests/data_pack")
    }
}

register_mod!(Gems);
```

The server loads the pack at startup and on `/reload`. The tag file merges with the tag of the same name in other packs, and the loot table replaces the table of the same id in the packs below it.

---

## The order of the packs

From the lowest to the highest priority:

1. The vanilla data of the server.
2. The mod packs, in the load order of the mods. See [Lifecycle](./lifecycle.md) for the order.
3. The data packs of the world, in `world/datapacks`.

A file in a higher pack replaces a file with the same id in a lower pack. A tag file merges with the lower packs unless it sets `"replace": true`, as in vanilla. A mod pack can replace a vanilla loot table, for example `data/minecraft/loot_table/blocks/stone.json`. A world data pack can replace the table of a mod.

This is the order of NeoForge for a new world. NeoForge appends `mod_data` to the end of the saved pack list of a world that lacks it, so in an older world `mod_data` can sit above the packs of the world. IronPumpkin always puts the mod packs below the world packs. The bundled feature packs (`trade_rebalance`, `minecart_improvements` and `redstone_experiments`) load after both, as they do for every data pack in IronPumpkin.

## The `mod_data` folder

The first load of a process writes `mod_data/<mod id>/` from the embedded files. A `/reload` and the load of a second server in the same process only read the folder, because the embedded files cannot change inside a process. The next start replaces the folder, so a file that a newer version of the mod no longer ships does not stay.

The server writes the new files to `mod_data/.staging-<mod id>/`, renames the old folder to `.trash-<mod id>/`, renames the staging folder to `<mod id>/` and deletes the trash. A crash leaves only dot folders, which the next start deletes, so the pack never stays out after a crash. The folder holds a marker file. The server never touches a folder of that name that has files and no marker. It logs a warning and skips the pack of that mod.

Do not edit `mod_data/`. Edit the files in the crate and rebuild. A server with no mod that ships files does not create the folder. The mod id is a folder name, so the names that Windows reserves (`con`, `prn`, `aux`, `nul`, `com1` and the others) cannot be written on Windows.

## Rebuild after you add a file

Cargo rebuilds the crate when an embedded file changes. It does not notice a file that you add or remove. Add a `build.rs` to the mod crate that watches the folder:

```rust,ignore
fn main() {
    println!("cargo::rerun-if-changed=resources");
}
```

Cargo then rebuilds the crate when anything in `resources/` changes, files that are added or removed included. The macro skips the files and folders whose name starts with a dot.

## Do and don't

- Do keep the layout of a data pack: `data/<namespace>/<type>/...`. The folder names are those of the Minecraft version of the server.
- Do put the loot modifiers and the data maps of the mod in this folder too. Their loaders read the files through the same pack manager, when the loot modifier loader and the data map loader land.
- Don't use `neoforge:conditions` in a file yet. The loaders ignore the field, so the file always loads. See [What is unsupported](./unsupported.md).
- Don't ship `assets/` here. The server ignores client resources.
- Don't depend on the files of `mod_data/` at run time. Use the registry and the data manager of the server to read the data.
