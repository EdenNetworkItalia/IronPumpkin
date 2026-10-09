//! Native mods compiled into the server binary.
//!
//! A mod crate implements [`NativeMod`] and registers it with [`register_mod!`]. A modpack binary
//! references each mod crate (`use hello_mod as _;`) and calls `pumpkin::run()`. The server calls
//! the [`NativeMod::init`] of every linked mod once, in id order, before the first world loads.
//!
//! This crate is the only `IronPumpkin` dependency a mod needs. It re-exports the server types:
//!
//! | Module          | Contents                                                                  |
//! |-----------------|---------------------------------------------------------------------------|
//! | [`command`]     | command tree builders, argument types, `CommandContext`, `CommandSender`  |
//! | [`event`]       | the event types, `EventHandler`, `EventPriority`, `Payload`, `Cancellable` |
//! | [`text`]        | `TextComponent`, colours, click and hover events, `translate_cross!`      |
//! | [`permission`]  | `Permission`, `PermissionDefault`, `PermissionLvl`                        |
//! | [`identifier`]  | `Identifier`, the key of registries and resources                         |
//! | [`math`]        | `Vector3`, `BlockPos` and the other positions                             |
//! | [`world`]       | `World` and its parts                                                     |
//! | [`server`]      | `Server`                                                                  |
//! | [`entity`]      | `Player` and the other entities                                           |
//! | [`content`]     | builders for custom blocks, items and entity types, and `RegistryError`   |
//!
//! The modules are the modules of the pumpkin crates, so what those crates add appears here. The
//! crates themselves are re-exported too, for what the modules do not cover: [`pumpkin_core`],
//! [`pumpkin_util`], [`pumpkin_data`] (translation keys, registries), [`pumpkin_macros`],
//! [`pumpkin_world`] and [`tracing`] (the server log).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use pumpkin_core::{
    command::node::detached::CommandDetachedNode,
    entity::custom,
    plugin::{Context, PluginMetadata, startup},
};
use pumpkin_data::dynamic::EntityTypeDefinition;
use tracing::{error, info};

#[doc(hidden)]
pub use inventory;

pub use pumpkin_core;
pub use pumpkin_data;
pub use pumpkin_macros;
pub use pumpkin_util;
pub use pumpkin_world;
pub use tracing;

pub use pumpkin_core::{command, entity, server, world};
pub use pumpkin_util::{identifier, math, permission};

pub mod content;

/// The events a mod can listen to, and the traits to handle them.
pub mod event {
    pub use pumpkin_core::plugin::api::events::*;
    pub use pumpkin_core::plugin::{BoxFuture, EventHandler};
}

/// Text components and the translation macro.
pub mod text {
    pub use crate::__translate_cross as translate_cross;
    pub use pumpkin_util::text::*;
}

// `pumpkin_macros::translate_cross!` expands to a `pumpkin_util::` path, which a mod without that
// dependency cannot resolve.
#[doc(hidden)]
#[macro_export]
macro_rules! __translate_cross {
    ($($tokens:tt)*) => {{
        use $crate::pumpkin_util;
        $crate::pumpkin_macros::translate_cross!($($tokens)*)
    }};
}

use content::{ContentKind, EntityFactory, RegistryError};
use event::{EventHandler, EventPriority, Payload};
use permission::Permission;
use server::Server;

/// A mod compiled into the server binary.
pub trait NativeMod: Sync {
    /// Unique id, also the namespace of the mod's commands and permissions.
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    /// Called once at startup, before the first world loads.
    ///
    /// If a content registration fails, the server logs the first error with the mod id and
    /// stops after `init` returns, before the content phase. Do not panic or unwrap the `Result`
    /// of a content registration: the server panic hook writes a crash report and exits at once.
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

/// What a mod registers during [`NativeMod::init`].
///
/// The server applies the registrations in the mod's name when `init` returns, except the content
/// registrations ([`ModInit::register_block`], [`ModInit::register_item`] and
/// [`ModInit::register_entity_type`]), which apply at once and return a `Result`.
///
/// `ModInit` keeps the first content registration error, even if the mod ignores the `Result`.
/// After `init` returns, the server logs that error with the mod id and stops before the content
/// phase, as it does for a mod id conflict. A mod can still read the `Result` to log more detail
/// or to skip the rest of its content, but it cannot make the server start without that content.
///
/// It passes `pumpkin-core` types through until the native API lands: the stable boundary for
/// mods is `ironpumpkin-neo`, not this crate.
pub struct ModInit {
    id: &'static str,
    data_root: PathBuf,
    registrations: Vec<Registration>,
    content_error: Option<ContentRegistrationError>,
}

/// The first content registration that failed in a mod's [`NativeMod::init`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentRegistrationError {
    /// The mod whose `init` made the registration.
    pub mod_id: &'static str,
    /// The registry of the content: block, item or entity type.
    pub kind: ContentKind,
    /// The name passed to the registration.
    pub name: String,
    /// What the registry returned.
    pub error: RegistryError,
}

impl fmt::Display for ContentRegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "native mod \"{}\" cannot register the {} \"{}\": {}",
            self.mod_id, self.kind, self.name, self.error
        )
    }
}

impl std::error::Error for ContentRegistrationError {}

impl ModInit {
    /// Creates the registration handle of the mod `id`. The server does this for each mod; the
    /// method is public for tests that drive [`NativeMod::init`] without a server. The commands,
    /// events, permissions and services a test registers are dropped.
    #[doc(hidden)]
    #[must_use]
    pub fn new(id: &'static str) -> Self {
        Self {
            id,
            // Must agree with `Context::get_data_folder`, which is not reachable without a server.
            data_root: Path::new("plugins").join("data"),
            registrations: Vec::new(),
            content_error: None,
        }
    }

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

    /// See [`Context::register_command_with_aliases`].
    pub fn register_command_with_aliases(
        &mut self,
        node: impl Into<CommandDetachedNode>,
        aliases: &[String],
        permission: impl Into<String>,
    ) {
        let node = node.into();
        let aliases = aliases.to_vec();
        let permission = permission.into();
        self.registrations.push(Box::new(move |cx| {
            cx.register_command_with_aliases(node, &aliases, permission);
        }));
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

    /// See [`Context::register_service`]. The service is registered before the first world
    /// loads, so plugins find it from their `on_load`.
    pub fn register_service<T: Payload + 'static>(
        &mut self,
        name: impl Into<String>,
        service: Arc<T>,
    ) {
        let name = name.into();
        self.registrations.push(Box::new(move |cx| {
            // The startup hook runs synchronously in `Server::new`, inside the runtime's
            // `block_on`. Blocking is safe: no plugin has loaded yet, so the services lock is
            // uncontended. `unconstrained` keeps an exhausted coop budget from spinning.
            futures::executor::block_on(tokio::task::unconstrained(
                cx.register_service(name, service),
            ));
        }));
    }

    /// Registers a custom block and its behaviour, if the [`BlockBuilder`](content::BlockBuilder)
    /// has one.
    ///
    /// Content registers at once, not when `init` returns: the server freezes the content
    /// registry after the last mod's `init` and before the first world loads. The registry
    /// allocates the id when it freezes.
    ///
    /// # Errors
    ///
    /// Returns the [`RegistryError`] of the registry: the name is invalid, uses the `minecraft`
    /// namespace or is already registered as a block, the display state is not a vanilla state, a
    /// property is invalid, a tag is unknown, or the registry is frozen.
    pub fn register_block(&mut self, block: content::BlockBuilder) -> Result<(), RegistryError> {
        let name = block.name().to_owned();
        self.keep_content_error(ContentKind::Block, name, block.register())
    }

    /// Registers a custom item and its behaviour, if the [`ItemBuilder`](content::ItemBuilder) has
    /// one. It applies at once, like [`ModInit::register_block`].
    ///
    /// # Errors
    ///
    /// Returns the [`RegistryError`] of the registry: the name is invalid, uses the `minecraft`
    /// namespace or is already registered as an item, the display item is not a vanilla item, a tag
    /// is unknown, or the registry is frozen. The block that the item places is not checked here:
    /// the freeze checks it.
    pub fn register_item(&mut self, item: content::ItemBuilder) -> Result<(), RegistryError> {
        let name = item.name().to_owned();
        self.keep_content_error(ContentKind::Item, name, item.register())
    }

    /// Registers a custom entity type and the factory that spawns it. Pass an
    /// [`EntityTypeBuilder`](content::EntityTypeBuilder). It applies at once, like
    /// [`ModInit::register_block`]. See [`pumpkin_core::entity::custom::register_entity_type`].
    ///
    /// # Errors
    ///
    /// Returns the [`RegistryError`] of the registry: the name is invalid, uses the `minecraft`
    /// namespace or is already registered as an entity type, the display type is not a vanilla
    /// type, a dimension is not positive and finite, a tag is unknown, or the registry is
    /// frozen.
    pub fn register_entity_type(
        &mut self,
        definition: impl Into<EntityTypeDefinition>,
        factory: EntityFactory,
    ) -> Result<(), RegistryError> {
        let definition = definition.into();
        let name = definition.name.clone();
        self.keep_content_error(
            ContentKind::EntityType,
            name,
            custom::register_entity_type(definition, factory),
        )
    }

    /// Passes `result` through and keeps its error if it is the first one of this mod.
    fn keep_content_error(
        &mut self,
        kind: ContentKind,
        name: String,
        result: Result<(), RegistryError>,
    ) -> Result<(), RegistryError> {
        if let Err(error) = &result {
            self.content_error
                .get_or_insert_with(|| ContentRegistrationError {
                    mod_id: self.id,
                    kind,
                    name,
                    error: error.clone(),
                });
        }
        result
    }

    /// See [`Context::get_data_folder`]: `plugins/data/<mod id>`, created on first use.
    #[must_use]
    pub fn get_data_folder(&self) -> PathBuf {
        let path = self.data_root.join(self.id);
        if !path.exists() {
            let _ = std::fs::create_dir_all(&path);
        }
        path
    }

    /// See [`Context::init_log`]. A mod linked into the server binary shares the server's
    /// tracing subscriber, so its log lines already reach the server log; the subscriber is
    /// installed only when the server has none.
    pub fn init_log(&mut self) {
        self.registrations.push(Box::new(|cx| {
            // `Context::init_log` panics when a global subscriber is already set.
            if !tracing::dispatcher::has_been_set() {
                cx.init_log();
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

/// Runs [`NativeMod::init`] of `native_mod` and returns what it registered, or the first content
/// registration error. [`init_mods`] stops the server on that error.
#[doc(hidden)]
pub fn init_mod(native_mod: &dyn NativeMod) -> Result<ModInit, ContentRegistrationError> {
    let mut init = ModInit::new(native_mod.id());
    native_mod.init(&mut init);
    init.content_error.take().map_or(Ok(init), Err)
}

fn refuse_to_start(reason: &dyn fmt::Display) -> ! {
    error!("[ironpumpkin] {reason}; refusing to start");
    std::process::exit(1);
}

/// Initializes every linked mod against `server`, or stops the process if the mods conflict or a
/// mod fails to register its content. Set as the startup hook of `pumpkin-core`.
pub fn init_mods(server: &Arc<Server>) {
    let mods = mods().unwrap_or_else(|err| refuse_to_start(&err));
    startup::set_native_mod_ids(
        mods.iter()
            .map(|native_mod| native_mod.id().to_owned())
            .collect(),
    );
    for native_mod in &mods {
        let init = init_mod(*native_mod).unwrap_or_else(|err| refuse_to_start(&err));
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
    use std::{
        any::Any,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use crate::{
        command::argument_builder::command, event::Payload, permission::PermissionDefault,
    };

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

        let mut init = ModInit::new(mods[0].id());
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

    struct TestService;

    impl Payload for TestService {
        fn get_name_static() -> &'static str {
            "TestService"
        }

        fn get_name(&self) -> &'static str {
            Self::get_name_static()
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    #[test]
    fn records_a_command_with_aliases() {
        let mut init = ModInit::new("test-mod");
        init.register_command_with_aliases(
            command("ping", "Test command."),
            &["p".to_owned()],
            "command.ping",
        );
        assert_eq!(init.registrations.len(), 1);
    }

    #[test]
    fn records_a_service() {
        let mut init = ModInit::new("test-mod");
        init.register_service("test-mod:service", Arc::new(TestService));
        assert_eq!(init.registrations.len(), 1);
    }

    #[test]
    fn records_the_log_initialization() {
        let mut init = ModInit::new("test-mod");
        init.init_log();
        assert_eq!(init.registrations.len(), 1);
    }

    #[test]
    fn creates_the_data_folder_on_first_use() {
        let root = tempfile::tempdir().unwrap();
        let mut init = ModInit::new("test-mod");
        init.data_root = root.path().join("plugins").join("data");
        let expected = init.data_root.join("test-mod");
        assert!(!expected.exists());

        let folder = init.get_data_folder();
        assert_eq!(folder, expected);
        assert!(folder.is_dir());
        assert_eq!(init.get_data_folder(), expected);
        assert!(init.registrations.is_empty());
    }
}
