//! Native mods compiled into the server binary.
//!
//! A mod crate implements [`NativeMod`] and registers it with [`register_mod!`]. A modpack binary
//! references each mod crate (`use hello_mod as _;`) and calls `pumpkin::run()`. The server calls
//! the [`NativeMod::init`] of every linked mod once, in id order, before the first world loads.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::sync::Arc;

use pumpkin_core::{
    command::node::detached::CommandDetachedNode,
    plugin::{Context, EventHandler, EventPriority, Payload, PluginMetadata, startup},
    server::Server,
};
use pumpkin_util::permission::Permission;
use tracing::{error, info};

#[doc(hidden)]
pub use inventory;

/// A mod compiled into the server binary.
pub trait NativeMod: Sync {
    /// Unique id, also the namespace of the mod's commands and permissions.
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    /// Called once at startup, before the first world loads.
    fn init(&self, cx: &mut ModInit);
}

#[doc(hidden)]
pub struct ModRegistration(pub &'static dyn NativeMod);

inventory::collect!(ModRegistration);

/// Registers a [`NativeMod`] value so that the server initializes it at startup.
#[macro_export]
macro_rules! register_mod {
    ($native_mod:expr) => {
        $crate::inventory::submit! {
            $crate::ModRegistration(&$native_mod)
        }
    };
}

type Registration = Box<dyn FnOnce(&Context)>;

/// What a mod registers during [`NativeMod::init`]. The server applies the registrations in the
/// mod's name when `init` returns.
///
/// It passes `pumpkin-core` types through until the native API lands: the stable boundary for
/// mods is `ironpumpkin-neo`, not this crate.
#[derive(Default)]
pub struct ModInit {
    registrations: Vec<Registration>,
}

impl ModInit {
    /// See [`Context::register_command`].
    pub fn register_command(
        &mut self,
        node: impl Into<CommandDetachedNode>,
        permission: impl Into<String>,
    ) {
        let node = node.into();
        let permission = permission.into();
        self.registrations
            .push(Box::new(move |cx| cx.register_command(node, permission)));
    }

    /// See [`Context::register_event`].
    pub fn register_event<E: Payload + 'static, H: EventHandler<E> + 'static>(
        &mut self,
        handler: Arc<H>,
        priority: EventPriority,
        blocking: bool,
    ) {
        self.registrations.push(Box::new(move |cx| {
            cx.register_event::<E, H>(handler, priority, blocking);
        }));
    }

    /// See [`Context::register_permission`].
    pub fn register_permission(&mut self, permission: Permission) {
        self.registrations.push(Box::new(move |cx| {
            if let Err(err) = cx.register_permission(permission) {
                error!("[ironpumpkin] {}: {err}", cx.get_metadata().name);
            }
        }));
    }

    fn apply(self, cx: &Context) {
        for registration in self.registrations {
            registration(cx);
        }
    }
}

/// The linked mods in id order. Fails on an invalid or duplicate id.
pub fn mods() -> Result<Vec<&'static dyn NativeMod>, String> {
    sorted(
        inventory::iter::<ModRegistration>
            .into_iter()
            .map(|registration| registration.0)
            .collect(),
    )
}

fn sorted(mut mods: Vec<&'static dyn NativeMod>) -> Result<Vec<&'static dyn NativeMod>, String> {
    if let Some(invalid) = mods.iter().find(|native_mod| !is_valid_id(native_mod.id())) {
        return Err(format!(
            "native mod \"{} {}\" has the invalid id \"{}\": use only a-z 0-9 _ . -",
            invalid.display_name(),
            invalid.version(),
            invalid.id()
        ));
    }
    mods.sort_by(|a, b| a.id().cmp(b.id()));
    if let Some(pair) = mods.windows(2).find(|pair| pair[0].id() == pair[1].id()) {
        return Err(format!(
            "native mods \"{} {}\" and \"{} {}\" share the id \"{}\"",
            pair[0].display_name(),
            pair[0].version(),
            pair[1].display_name(),
            pair[1].version(),
            pair[0].id()
        ));
    }
    Ok(mods)
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-'))
}

/// Initializes every linked mod against `server`, or stops the process if the mods conflict.
/// Set as the startup hook of `pumpkin-core`.
pub fn init_mods(server: &Arc<Server>) {
    let mods = mods().unwrap_or_else(|err| {
        error!("[ironpumpkin] {err}; refusing to start");
        std::process::exit(1);
    });
    startup::set_native_mod_ids(
        mods.iter()
            .map(|native_mod| native_mod.id().to_owned())
            .collect(),
    );
    for native_mod in &mods {
        let mut init = ModInit::default();
        native_mod.init(&mut init);
        let metadata = PluginMetadata {
            name: native_mod.id().to_owned(),
            version: native_mod.version().to_owned(),
            authors: Vec::new(),
            description: native_mod.display_name().to_owned(),
            dependencies: Vec::new(),
            permissions: Vec::new(),
        };
        init.apply(&startup::create_context(server, metadata));
    }
    let ids: Vec<&str> = mods.iter().map(|native_mod| native_mod.id()).collect();
    info!(
        "[ironpumpkin] loaded {} native mod{}{}",
        ids.len(),
        if ids.len() == 1 { "" } else { "s" },
        if ids.is_empty() {
            String::new()
        } else {
            format!(": {}", ids.join(", "))
        }
    );
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use pumpkin_util::permission::PermissionDefault;

    use super::{ModInit, NativeMod, mods, sorted};

    static INITS: AtomicUsize = AtomicUsize::new(0);

    struct TestMod(&'static str);

    impl NativeMod for TestMod {
        fn id(&self) -> &'static str {
            self.0
        }

        fn display_name(&self) -> &'static str {
            "Test mod"
        }

        fn version(&self) -> &'static str {
            "1.0.0"
        }

        fn init(&self, cx: &mut ModInit) {
            INITS.fetch_add(1, Ordering::Relaxed);
            cx.register_permission(pumpkin_util::permission::Permission::new(
                &format!("{}:ping", self.0),
                "Test permission.",
                PermissionDefault::Allow,
            ));
        }
    }

    static DUPLICATE_A: TestMod = TestMod("dup-mod");
    static DUPLICATE_B: TestMod = TestMod("dup-mod");
    static INVALID: TestMod = TestMod("Bad Id");

    crate::register_mod!(TestMod("zeta-mod"));
    crate::register_mod!(TestMod("alpha-mod"));

    #[test]
    fn collects_registered_mods_in_id_order_and_runs_init() {
        let mods = mods().unwrap();
        let ids: Vec<&str> = mods.iter().map(|native_mod| native_mod.id()).collect();
        assert_eq!(ids, ["alpha-mod", "zeta-mod"]);

        let mut init = ModInit::default();
        mods[0].init(&mut init);
        assert_eq!(INITS.load(Ordering::Relaxed), 1);
        assert_eq!(init.registrations.len(), 1);
    }

    #[test]
    fn rejects_duplicate_and_invalid_ids() {
        let duplicate = sorted(vec![&DUPLICATE_A, &DUPLICATE_B]).err().unwrap();
        assert!(
            duplicate.contains("share the id \"dup-mod\""),
            "{duplicate}"
        );
        let invalid = sorted(vec![&INVALID]).err().unwrap();
        assert!(invalid.contains("invalid id \"Bad Id\""), "{invalid}");
    }
}
