//! The mod list (FML 12.0.8 `ModList`), the dist (`FMLEnvironment.getDist`), and the dependency
//! check and load order of the mods (FML 12.0.8 `ModSorter`).
//!
//! The list holds the native mods and the system mods `minecraft` and `neoforge`, as on
//! `NeoForge`, so a mod that requires them loads and `is_loaded("neoforge")` is true. Their
//! versions are the Minecraft version of the server and the `NeoForge` version that the server
//! emulates for clients.

use std::{
    collections::{BTreeSet, HashMap},
    sync::OnceLock,
};

use pumpkin_core::net::java::neoforge::EMULATED_NEOFORGE_VERSION;
use pumpkin_data::packet::CURRENT_MC_VERSION;
use tracing::warn;

use crate::{
    mods_toml::{DependencySide, DependencyType, ModInfo, Ordering},
    version::ArtifactVersion,
};

/// `net.neoforged.api.distmarker.Dist`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dist {
    Client,
    DedicatedServer,
}

/// `FMLEnvironment.getDist()`: always [`Dist::DedicatedServer`]. Code for the client dist never
/// runs.
#[must_use]
pub const fn dist() -> Dist {
    Dist::DedicatedServer
}

/// The loaded mods in load order (`ModList`).
#[derive(Debug, Default)]
pub struct ModList {
    mods: Vec<ModInfo>,
}

static MOD_LIST: OnceLock<ModList> = OnceLock::new();
static EMPTY: ModList = ModList { mods: Vec::new() };

impl ModList {
    pub(crate) const fn new(mods: Vec<ModInfo>) -> Self {
        Self { mods }
    }

    /// `ModList.get()`: the mod list of the server. It is set before the first
    /// [`NativeMod::init`](crate::NativeMod::init); before that it is empty.
    #[must_use]
    pub fn get() -> &'static Self {
        MOD_LIST.get().unwrap_or(&EMPTY)
    }

    pub(crate) fn set(self) {
        let _ = MOD_LIST.set(self);
    }

    /// `ModList.isLoaded`.
    #[must_use]
    pub fn is_loaded(&self, mod_id: &str) -> bool {
        self.get_mod_container_by_id(mod_id).is_some()
    }

    /// `ModList.getMods`, in load order. `forEachModContainer` and `applyForEachModContainer` are
    /// an iteration of this slice.
    #[must_use]
    pub fn get_mods(&self) -> &[ModInfo] {
        &self.mods
    }

    /// `ModList.getSortedMods`: the mods in load order. `forEachModInOrder` is an iteration of
    /// this slice.
    #[must_use]
    pub fn get_sorted_mods(&self) -> &[ModInfo] {
        &self.mods
    }

    /// `ModList.getModContainerById`. The container of a native mod is its [`ModInfo`]:
    /// `getModId`, `getNamespace` and `getModInfo` are its fields.
    #[must_use]
    pub fn get_mod_container_by_id(&self, mod_id: &str) -> Option<&ModInfo> {
        self.mods.iter().find(|info| info.mod_id == mod_id)
    }

    /// `ModList.size`.
    #[must_use]
    pub const fn size(&self) -> usize {
        self.mods.len()
    }
}

/// The mods that `NeoForge` always loads (`ModSorter.detectSystemMods`).
pub(crate) fn system_mods() -> Vec<ModInfo> {
    let system_mod = |mod_id: &str, display_name: &str, version: &str| ModInfo {
        mod_id: mod_id.to_owned(),
        namespace: mod_id.to_owned(),
        version: ArtifactVersion::new(version),
        display_name: display_name.to_owned(),
        description: display_name.to_owned(),
        authors: None,
        dependencies: Vec::new(),
    };
    vec![
        system_mod("minecraft", "Minecraft", &CURRENT_MC_VERSION.to_string()),
        system_mod("neoforge", "NeoForge", EMULATED_NEOFORGE_VERSION),
    ]
}

/// `ModSorter.verifyDependencyVersions`: a required dependency must be loaded in its range, an
/// optional one must be in its range when it is loaded, and an incompatible one must not be
/// loaded in its range. A discouraged one that is loaded in its range logs a warning. `CLIENT`
/// dependencies are ignored on a dedicated server.
///
/// # Errors
///
/// Returns every unmet dependency, one per line, each naming the mod and the dependency.
pub(crate) fn check_dependencies(mods: &[ModInfo]) -> Result<(), String> {
    let versions: HashMap<&str, &ArtifactVersion> = mods
        .iter()
        .map(|info| (info.mod_id.as_str(), &info.version))
        .collect();
    let mut errors = Vec::new();
    for info in mods {
        for dep in &info.dependencies {
            if dep.side == DependencySide::Client {
                continue;
            }
            let loaded = versions.get(dep.mod_id.as_str());
            let in_range = loaded.is_some_and(|version| dep.version_range.contains(version));
            let state = loaded.map_or_else(
                || format!("\"{}\" is not loaded", dep.mod_id),
                |version| format!("\"{}\" {version} is loaded", dep.mod_id),
            );
            let reason = dep
                .reason
                .as_deref()
                .map_or_else(String::new, |reason| format!(": {reason}"));
            match dep.dependency_type {
                DependencyType::Required if !in_range => errors.push(format!(
                    "native mod \"{}\" requires \"{}\" {}, and {state}",
                    info.mod_id, dep.mod_id, dep.version_range
                )),
                DependencyType::Optional if loaded.is_some() && !in_range => {
                    errors.push(format!(
                        "native mod \"{}\" supports \"{}\" {} only, and {state}",
                        info.mod_id, dep.mod_id, dep.version_range
                    ));
                }
                DependencyType::Incompatible if in_range => errors.push(format!(
                    "native mod \"{}\" is incompatible with \"{}\" {}, and {state}{reason}",
                    info.mod_id, dep.mod_id, dep.version_range
                )),
                DependencyType::Discouraged if in_range => warn!(
                    "[ironpumpkin] native mod \"{}\" discourages \"{}\" {}, and {state}{reason}",
                    info.mod_id, dep.mod_id, dep.version_range
                ),
                _ => {}
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

/// `ModSorter.sort`: the indices of `mods` in load order. A dependency with the ordering `BEFORE`
/// loads the mod before the dependency, `AFTER` after it; `NONE` and a dependency that is not
/// loaded add no constraint. Among mods that no ordering constrains, the mod id decides; FML uses
/// the discovery order there, which a binary does not have.
///
/// # Errors
///
/// Returns the mods of an ordering cycle.
pub(crate) fn sort(mods: &[ModInfo]) -> Result<Vec<usize>, String> {
    let index: HashMap<&str, usize> = mods
        .iter()
        .enumerate()
        .map(|(i, info)| (info.mod_id.as_str(), i))
        .collect();
    let mut successors = vec![BTreeSet::new(); mods.len()];
    for (i, info) in mods.iter().enumerate() {
        for dep in &info.dependencies {
            let Some(&target) = index.get(dep.mod_id.as_str()) else {
                continue;
            };
            match dep.ordering {
                _ if target == i => {}
                Ordering::Before => {
                    successors[i].insert(target);
                }
                Ordering::After => {
                    successors[target].insert(i);
                }
                Ordering::None => {}
            }
        }
    }
    let mut in_degree = vec![0usize; mods.len()];
    for &next in successors.iter().flatten() {
        in_degree[next] += 1;
    }
    let mut ready: BTreeSet<(&str, usize)> = mods
        .iter()
        .enumerate()
        .filter(|(i, _)| in_degree[*i] == 0)
        .map(|(i, info)| (info.mod_id.as_str(), i))
        .collect();
    let mut order = Vec::with_capacity(mods.len());
    while let Some((_, current)) = ready.pop_first() {
        order.push(current);
        for &next in &successors[current] {
            in_degree[next] -= 1;
            if in_degree[next] == 0 {
                ready.insert((mods[next].mod_id.as_str(), next));
            }
        }
    }
    if order.len() == mods.len() {
        return Ok(order);
    }
    // The mods left over are in a cycle or after one; drop those that lead to no cycle.
    let mut left: BTreeSet<usize> = (0..mods.len()).filter(|i| in_degree[*i] > 0).collect();
    while let Some(sink) = left
        .iter()
        .copied()
        .find(|i| successors[*i].iter().all(|next| !left.contains(next)))
    {
        left.remove(&sink);
    }
    let mut ids: Vec<&str> = left.iter().map(|&i| mods[i].mod_id.as_str()).collect();
    ids.sort_unstable();
    Err(format!(
        "the ordering of the native mods {} has a cycle",
        ids.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::{Dist, ModList, check_dependencies, dist, sort, system_mods};
    use crate::mods_toml::{ModInfo, parse};

    fn mod_info(id: &str, version: &str, dependencies: &str) -> ModInfo {
        let text = format!(
            "license = \"MIT\"\n[[mods]]\nmodId = \"{id}\"\nversion = \"{version}\"\n{}",
            dependencies.replace("DEP", &format!("[[dependencies.{id}]]"))
        );
        parse(&text, id, version).unwrap()
    }

    fn order(mods: &[ModInfo]) -> Vec<&str> {
        sort(mods)
            .unwrap()
            .into_iter()
            .map(|i| mods[i].mod_id.as_str())
            .collect()
    }

    #[test]
    fn missing_required_dependency_names_it() {
        let mods = [mod_info(
            "alpha",
            "1.0",
            "DEP\nmodId = \"geckolib\"\nversionRange = \"[4.0,5.0)\"\n",
        )];
        let error = check_dependencies(&mods).unwrap_err();
        assert_eq!(
            error,
            "native mod \"alpha\" requires \"geckolib\" [4.0,5.0), and \"geckolib\" is not loaded"
        );
    }

    #[test]
    fn checks_version_ranges_types_and_sides() {
        let old = mod_info("geckolib", "3.0.1", "");
        let out_of_range = mod_info(
            "alpha",
            "1.0",
            "DEP\nmodId = \"geckolib\"\nversionRange = \"[4.0,5.0)\"\n",
        );
        let error = check_dependencies(&[old.clone(), out_of_range]).unwrap_err();
        assert!(error.contains("\"geckolib\" 3.0.1 is loaded"), "{error}");

        let fine = [
            old.clone(),
            mod_info(
                "beta",
                "1.0",
                "DEP\nmodId = \"jei\"\ntype = \"optional\"\n\
                 DEP\nmodId = \"geckolib\"\ntype = \"discouraged\"\n\
                 DEP\nmodId = \"clientonly\"\nside = \"CLIENT\"\n\
                 DEP\nmodId = \"geckolib\"\ntype = \"incompatible\"\nversionRange = \"[4,)\"\n",
            ),
        ];
        assert!(check_dependencies(&fine).is_ok());

        let incompatible = mod_info(
            "gamma",
            "1.0",
            "DEP\nmodId = \"geckolib\"\ntype = \"incompatible\"\nreason = \"crashes\"\n",
        );
        let error = check_dependencies(&[old, incompatible]).unwrap_err();
        assert!(
            error.contains("incompatible with \"geckolib\"") && error.ends_with(": crashes"),
            "{error}"
        );
    }

    #[test]
    fn the_system_mods_satisfy_the_client_mod_descriptor() {
        let client = parse(
            include_str!(
                "../../../examples/modpack/client-mod/src/main/resources/META-INF/neoforge.mods.toml"
            ),
            "hello_mod",
            "1.0.0",
        )
        .unwrap();
        let mut mods = system_mods();
        mods.push(client);
        assert!(check_dependencies(&mods).is_ok());
    }

    #[test]
    fn before_and_after_order_three_mods() {
        // Without ordering, the id would give alpha, beta, gamma. `gamma` loads BEFORE `alpha`
        // and `alpha` loads AFTER `beta`, so `alpha` is last; the id puts `beta` before `gamma`.
        let mods = [
            mod_info(
                "alpha",
                "1",
                "DEP\nmodId = \"beta\"\nordering = \"AFTER\"\n",
            ),
            mod_info("beta", "1", ""),
            mod_info(
                "gamma",
                "1",
                "DEP\nmodId = \"alpha\"\nordering = \"BEFORE\"\n",
            ),
        ];
        assert_eq!(order(&mods), ["beta", "gamma", "alpha"]);

        // `beta` loads AFTER `gamma` now, so gamma comes first.
        let mods = [
            mods[0].clone(),
            mod_info(
                "beta",
                "1",
                "DEP\nmodId = \"gamma\"\nordering = \"AFTER\"\n",
            ),
            mods[2].clone(),
        ];
        assert_eq!(order(&mods), ["gamma", "beta", "alpha"]);
    }

    #[test]
    fn id_breaks_ties_and_none_adds_no_order() {
        let mods = [
            mod_info("zeta", "1", ""),
            mod_info("alpha", "1", "DEP\nmodId = \"zeta\"\n"),
            mod_info("beta", "1", ""),
        ];
        assert_eq!(order(&mods), ["alpha", "beta", "zeta"]);
    }

    #[test]
    fn ordering_cycle_names_its_mods() {
        let mods = [
            mod_info(
                "alpha",
                "1",
                "DEP\nmodId = \"beta\"\nordering = \"AFTER\"\n",
            ),
            mod_info(
                "beta",
                "1",
                "DEP\nmodId = \"alpha\"\nordering = \"AFTER\"\n",
            ),
            mod_info(
                "gamma",
                "1",
                "DEP\nmodId = \"alpha\"\nordering = \"AFTER\"\n",
            ),
        ];
        assert_eq!(
            sort(&mods).unwrap_err(),
            "the ordering of the native mods alpha, beta has a cycle"
        );
    }

    #[test]
    fn mod_list_answers_like_neoforge() {
        let mut mods = system_mods();
        mods.push(mod_info("examplemod", "1.2", ""));
        let list = ModList::new(mods);
        assert!(!list.is_loaded("jei"));
        assert!(list.is_loaded("examplemod"));
        assert!(list.is_loaded("minecraft"));
        assert!(list.is_loaded("neoforge"));
        assert_eq!(list.size(), 3);
        assert_eq!(list.get_mods().len(), 3);
        assert_eq!(list.get_sorted_mods()[2].mod_id, "examplemod");
        let container = list.get_mod_container_by_id("examplemod").unwrap();
        assert_eq!(container.version.to_string(), "1.2");
        assert_eq!(container.namespace, "examplemod");
        assert!(list.get_mod_container_by_id("jei").is_none());
        assert_eq!(dist(), Dist::DedicatedServer);
    }
}
