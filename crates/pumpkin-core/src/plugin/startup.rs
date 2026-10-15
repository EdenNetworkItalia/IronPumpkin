//! The seam through which native mods compiled into the binary initialize.

use std::{
    collections::BTreeSet,
    sync::{Arc, OnceLock},
};

use crate::{
    LOGGER_IMPL,
    net::java::configuration_payloads::NEOFORGE_SYNCED_CONFIG_NAME,
    plugin::{Context, PluginMetadata, server::server_lifecycle::ServerAboutToStartEvent},
    server::Server,
};

/// Runs once in [`Server::new`], after the server exists and before the first world loads.
pub static STARTUP_HOOK: OnceLock<fn(&Arc<Server>)> = OnceLock::new();

/// A native mod compiled into the binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeModInfo {
    pub id: String,
    pub display_name: String,
    pub version: String,
    /// Whether a client must have the mod. A connection that cannot have it is refused.
    pub client_required: bool,
}

static NATIVE_MODS: OnceLock<Vec<NativeModInfo>> = OnceLock::new();

/// Records the native mods, once, before the content freeze.
///
/// A plugin with the id of a native mod is refused, because unloading it would also remove the
/// mod's handlers and commands, which are keyed by name.
pub fn set_native_mods(mods: Vec<NativeModInfo>) {
    let _ = NATIVE_MODS.set(sorted_by_id(mods));
}

fn sorted_by_id(mut mods: Vec<NativeModInfo>) -> Vec<NativeModInfo> {
    mods.sort_by(|a, b| a.id.cmp(&b.id));
    mods
}

/// The native mods sorted by id. Empty before [`set_native_mods`] and without native mods.
#[must_use]
pub fn native_mods() -> &'static [NativeModInfo] {
    NATIVE_MODS.get().map_or(&[], Vec::as_slice)
}

/// A synced config file of a native mod, like a `ModConfig.Type.SYNCED` config of `NeoForge`. A
/// `NeoForge` client gets it as `neoforge:config_file` during configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncedConfig {
    pub file_name: String,
    pub contents: Box<[u8]>,
}

/// A live synced config, read for each connection.
///
/// The `SyncConfig` configuration task reads the contents for each connection, so a file that
/// the host reloads reaches every client that configures afterwards, as
/// `ConfigSync.syncAllConfigs` of `NeoForge` reads the file for each connection.
pub trait SyncedConfigSource: Send + Sync {
    fn file_name(&self) -> &str;
    /// The current file contents.
    fn contents(&self) -> Box<[u8]>;
}

static SYNCED_CONFIGS: OnceLock<Vec<Arc<dyn SyncedConfigSource>>> = OnceLock::new();

/// Records the synced configs of the native mods, once, in load order: the mods in load order,
/// the configs of a mod in registration order.
///
/// # Errors
///
/// Returns the reason when a file name is `NeoForge`'s own or appears twice: `NeoForge` collects
/// the configs into a map by file name (`ConfigSync.syncAllConfigs`), and the client applies a
/// file by its name.
pub fn set_synced_configs(configs: Vec<Arc<dyn SyncedConfigSource>>) -> Result<(), String> {
    check_synced_config_names(configs.iter().map(|config| config.file_name()))?;
    let _ = SYNCED_CONFIGS.set(configs);
    Ok(())
}

fn check_synced_config_names<'a>(names: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut seen = BTreeSet::from([NEOFORGE_SYNCED_CONFIG_NAME]);
    for name in names {
        if !seen.insert(name) {
            return Err(format!(
                "the synced config \"{name}\" is registered twice or is NeoForge's own"
            ));
        }
    }
    Ok(())
}

/// The current contents of the synced configs of the native mods in load order. Empty before
/// [`set_synced_configs`].
#[must_use]
pub fn synced_configs() -> Vec<SyncedConfig> {
    SYNCED_CONFIGS.get().map_or_else(Vec::new, |configs| {
        configs
            .iter()
            .map(|config| SyncedConfig {
                file_name: config.file_name().to_owned(),
                contents: config.contents(),
            })
            .collect()
    })
}

pub(crate) fn is_native_mod_id(name: &str) -> bool {
    has_mod_id(native_mods(), name)
}

fn has_mod_id(mods: &[NativeModInfo], name: &str) -> bool {
    mods.iter().any(|native_mod| native_mod.id == name)
}

pub(crate) fn run(server: &Arc<Server>) {
    if let Some(hook) = STARTUP_HOOK.get() {
        hook(server);
    }
    crate::net::java::neoforge::warn_if_detection_forced(&server.basic_config);
}

/// Fires `ServerAboutToStartEvent` after the mod lifecycle (with the synced configs) and before
/// the worlds load, as `ServerLifecycleHooks.handleServerAboutToStart` does before `loadLevel`.
pub(crate) async fn fire_about_to_start(server: &Arc<Server>) {
    server
        .plugin_manager
        .fire_with(server, || ServerAboutToStartEvent)
        .await;
}

/// Creates the context of a plugin that no loader manages, such as a native mod.
#[must_use]
pub fn create_context(server: &Arc<Server>, metadata: PluginMetadata) -> Context {
    Context::new(
        metadata,
        server.clone(),
        Arc::clone(&server.plugin_manager.handlers),
        server.plugin_manager.clone(),
        Arc::clone(&LOGGER_IMPL),
    )
}

#[cfg(test)]
mod tests {
    use super::{NativeModInfo, check_synced_config_names, has_mod_id, sorted_by_id};

    #[test]
    fn synced_config_names_are_unique() {
        assert!(check_synced_config_names(["a.toml", "b.toml"].into_iter()).is_ok());
        let twice = check_synced_config_names(["a.toml", "a.toml"].into_iter());
        assert!(twice.unwrap_err().contains("a.toml"));
        assert!(check_synced_config_names(["neoforge-synced.toml"].into_iter()).is_err());
    }

    fn info(id: &str, client_required: bool) -> NativeModInfo {
        NativeModInfo {
            id: id.to_owned(),
            display_name: format!("{id} name"),
            version: "1.0.0".to_owned(),
            client_required,
        }
    }

    /// Works on a local list: a client-required mod in the process-wide list would force the
    /// `NeoForge` detection on for every other test of the crate.
    #[test]
    fn native_mods_are_sorted_by_id() {
        let mods = sorted_by_id(vec![
            info("startup-test-zeta", true),
            info("startup-test-alpha", false),
        ]);
        assert_eq!(
            mods,
            [
                info("startup-test-alpha", false),
                info("startup-test-zeta", true)
            ]
        );
        assert!(has_mod_id(&mods, "startup-test-zeta"));
        assert!(!has_mod_id(&mods, "startup-test"));
    }
}
