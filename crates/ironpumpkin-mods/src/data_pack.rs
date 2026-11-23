//! Data packs of native mods.
//!
//! On `NeoForge` every mod jar is also a data pack: the `data/` folder of the jar loads as mod
//! resources, so a mod ships its loot tables, tags, recipes, advancements, functions, loot
//! modifiers and data maps next to its code. A native mod of `IronPumpkin` is compiled into the
//! server binary, so it embeds a folder of its crate with [`include_data_pack!`] and returns it
//! from [`NativeMod::data_pack`](crate::NativeMod::data_pack).
//!
//! The folder is the root of the pack, like the root of a jar: `data/<namespace>/<type>/...` and
//! an optional `pack.mcmeta`. The same folder layout as in a world data pack works.
//!
//! ```
//! use ironpumpkin_mods::{
//!     ModInit, NativeMod,
//!     data_pack::{PackFiles, include_data_pack},
//! };
//!
//! struct Gems;
//!
//! impl NativeMod for Gems {
//!     fn id(&self) -> &'static str { "gems" }
//!     fn display_name(&self) -> &'static str { "Gems" }
//!     fn version(&self) -> &'static str { "1.0.0" }
//!     fn init(&self, _cx: &mut ModInit) {}
//!
//!     // A folder of the crate with `data/gems/loot_table/...`, usually `resources`. This
//!     // example embeds the test folder of this crate.
//!     fn data_pack(&self) -> PackFiles {
//!         include_data_pack!("tests/data_pack")
//!     }
//! }
//! ```
//!
//! # Load order
//!
//! The server loads the packs at startup and on `/reload`, through the loaders of the world data
//! packs, so every file type that a world data pack supports works from a mod pack. A pack
//! overrides the vanilla data, a later mod overrides an earlier mod ([`crate::mod_list`] gives the
//! load order), and the data packs of the world override the mod packs. Tags merge across packs
//! unless a file sets `"replace": true`, as in vanilla. This is the order of `NeoForge` for a new world: the
//! `mod_data` pack sits above `vanilla` and below the packs that a world enables.
//!
//! The first load of a process writes each pack to `mod_data/<mod id>/` in the server directory.
//! A `/reload` and a second server in the same process read that folder. The next start replaces
//! it, so a file that a newer version of the mod no longer ships does not stay. Do not edit it. A
//! folder of that name that has files and no marker file is left alone and the pack of that mod is
//! skipped.
//!
//! `NeoForge` appends `mod_data` to the end of the saved pack list of a world that lacks it, so in
//! an older world it can sit above the packs of the world. `IronPumpkin` always puts the mod packs
//! below the world packs. A mod id is a folder name, so the names that Windows reserves (`con`,
//! `prn`, `aux`, `nul`, `com1` and the others) cannot be written on Windows.
//!
//! Cargo does not notice a file that is added to or removed from the embedded folder. Add a
//! `build.rs` to the mod crate: `println!("cargo::rerun-if-changed=resources");`.

pub use pumpkin_core::data::datapack::mod_packs::{MOD_DATA_DIR, ModDataPack, PackFiles};
pub use pumpkin_macros::include_data_pack;

use crate::{LoadedMod, in_order, linked_mods, load};

/// The packs of `mods`, in the order of `mods`. A mod without files has no pack.
pub(crate) fn of_mods(mods: &[LoadedMod<'_>]) -> Vec<ModDataPack> {
    mods.iter()
        .filter_map(|loaded| {
            let files = loaded.native_mod.data_pack();
            (!files.is_empty()).then(|| ModDataPack {
                mod_id: loaded.mod_id.to_owned(),
                files,
            })
        })
        .collect()
}

/// The data packs of the linked mods in load order, as the server loads them.
///
/// # Errors
///
/// Fails like [`mods`](crate::mods): on an invalid or duplicate id, an invalid
/// `neoforge.mods.toml`, an unmet dependency or an ordering cycle.
pub fn data_packs() -> Result<Vec<ModDataPack>, String> {
    let (by_id, mod_list) = load(linked_mods())?;
    Ok(of_mods(&in_order(&by_id, &mod_list)))
}
