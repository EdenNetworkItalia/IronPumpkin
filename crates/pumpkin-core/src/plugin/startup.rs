//! The seam through which native mods compiled into the binary initialize.

use std::sync::{Arc, OnceLock};

use crate::{
    LOGGER_IMPL,
    plugin::{Context, PluginMetadata},
    server::Server,
};

/// Runs once in [`Server::new`], after the server exists and before the first world loads.
pub static STARTUP_HOOK: OnceLock<fn(&Arc<Server>)> = OnceLock::new();

static NATIVE_MOD_IDS: OnceLock<Vec<String>> = OnceLock::new();

/// Records the ids of the native mods. A plugin with one of these names is refused, because
/// unloading it would also remove the mod's handlers and commands, which are keyed by name.
pub fn set_native_mod_ids(ids: Vec<String>) {
    let _ = NATIVE_MOD_IDS.set(ids);
}

pub(crate) fn is_native_mod_id(name: &str) -> bool {
    NATIVE_MOD_IDS
        .get()
        .is_some_and(|ids| ids.iter().any(|id| id == name))
}

pub(crate) fn run(server: &Arc<Server>) {
    if let Some(hook) = STARTUP_HOOK.get() {
        hook(server);
    }
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
