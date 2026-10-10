//! The seam through which native mods compiled into the binary initialize.

use std::sync::{Arc, OnceLock};

use crate::{
    LOGGER_IMPL,
    plugin::{Context, PluginMetadata},
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
pub fn set_native_mods(mut mods: Vec<NativeModInfo>) {
    mods.sort_by(|a, b| a.id.cmp(&b.id));
    let _ = NATIVE_MODS.set(mods);
}

/// The native mods sorted by id. Empty before [`set_native_mods`] and without native mods.
#[must_use]
pub fn native_mods() -> &'static [NativeModInfo] {
    NATIVE_MODS.get().map_or(&[], Vec::as_slice)
}

pub(crate) fn is_native_mod_id(name: &str) -> bool {
    native_mods().iter().any(|native_mod| native_mod.id == name)
}

pub(crate) fn run(server: &Arc<Server>) {
    if let Some(hook) = STARTUP_HOOK.get() {
        hook(server);
    }
    crate::net::java::neoforge::warn_if_detection_forced(&server.basic_config);
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
    use super::{NativeModInfo, is_native_mod_id, native_mods, set_native_mods};

    fn info(id: &str, client_required: bool) -> NativeModInfo {
        NativeModInfo {
            id: id.to_owned(),
            display_name: format!("{id} name"),
            version: "1.0.0".to_owned(),
            client_required,
        }
    }

    /// The only test that sets the process-wide list.
    #[test]
    fn native_mods_are_sorted_by_id() {
        set_native_mods(vec![
            info("startup-test-zeta", true),
            info("startup-test-alpha", false),
        ]);
        assert_eq!(
            native_mods(),
            [
                info("startup-test-alpha", false),
                info("startup-test-zeta", true)
            ]
        );
        assert!(is_native_mod_id("startup-test-zeta"));
        assert!(!is_native_mod_id("startup-test"));
    }
}
