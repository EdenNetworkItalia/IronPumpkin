//! Native mods compiled into the server binary.
//!
//! A mod crate implements [`NativeMod`] and registers it with [`register_mod!`]. A modpack binary
//! references each mod crate (`use hello_mod as _;`) and calls `pumpkin::run()`. Before the first
//! world loads, the server reads the metadata of every linked mod, checks the dependencies, calls
//! each [`NativeMod::init`] once in load order, and then calls the lifecycle methods
//! ([`lifecycle`]).
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
//! | [`content`]     | builders for custom blocks, items, entity types, tags, `RegistryError`    |
//! | [`mod_list`]    | `ModList`, `dist()`                                                       |
//! | [`mods_toml`]   | `ModInfo`, `ModDependency`, the `neoforge.mods.toml` reader               |
//! | [`imc`]         | inter-mod messages                                                        |
//! | [`config`]      | TOML config specs: `ConfigSpec`, `ConfigType`, `ModConfig`                |
//!
//! The modules are the modules of the pumpkin crates, so what those crates add appears here. The
//! crates themselves are re-exported too, for what the modules do not cover: [`pumpkin_core`],
//! [`pumpkin_util`], [`pumpkin_data`] (translation keys, registries), [`pumpkin_macros`],
//! [`pumpkin_world`] and [`tracing`] (the server log).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use pumpkin_core::{
    command::node::detached::CommandDetachedNode,
    entity::custom,
    plugin::{
        Context, PluginMetadata,
        startup::{self, NativeModInfo},
    },
};
use pumpkin_data::dynamic::{self, EntityTypeDefinition};
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

pub mod config;
pub mod content;
pub mod imc;
pub mod lifecycle;
pub mod mod_list;
pub mod mods_toml;
pub mod version;

/// The events a mod can listen to, and the traits to handle them.
pub mod event {
    pub use pumpkin_core::plugin::api::events::*;
    pub use pumpkin_core::plugin::{BoxFuture, EventHandler, ParentEventHandler};
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
use event::{EventHandler, EventPriority, ParentEventHandler, Payload};
use imc::{InterModEnqueue, InterModProcess};
use mod_list::ModList;
use mods_toml::ModInfo;
use permission::Permission;
use server::Server;
use version::ArtifactVersion;

// The Rust snippets of the porting guide are doc-tests of this crate, so an API change that breaks
// a snippet fails `cargo test --doc -p ironpumpkin-mods`.
#[cfg(doctest)]
mod porting_guide {
    #[doc = include_str!("../../../docs/porting-from-neoforge/index.md")]
    struct Index;

    #[doc = include_str!("../../../docs/porting-from-neoforge/setup-and-build.md")]
    struct SetupAndBuild;

    #[doc = include_str!("../../../docs/porting-from-neoforge/registration.md")]
    struct Registration;

    #[doc = include_str!("../../../docs/porting-from-neoforge/lifecycle.md")]
    struct Lifecycle;

    #[doc = include_str!("../../../docs/porting-from-neoforge/configuration.md")]
    struct Configuration;

    #[doc = include_str!("../../../docs/porting-from-neoforge/events.md")]
    struct Events;

    #[doc = include_str!("../../../docs/porting-from-neoforge/mixins-and-patches.md")]
    struct MixinsAndPatches;

    #[doc = include_str!("../../../docs/porting-from-neoforge/unsupported.md")]
    struct Unsupported;
}

/// A mod compiled into the server binary.
///
/// The lifecycle methods have a default that does nothing. The server calls them after the last
/// `init`, in the order of [`lifecycle`], each on its own mod. The config hooks run when a config
/// loads or reloads ([`config`]).
pub trait NativeMod: Sync {
    /// Unique id, also the namespace of the mod's commands and permissions. Without a
    /// [`NativeMod::mods_toml`], use only `a-z 0-9 _ . -`.
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    /// The text of the mod's `neoforge.mods.toml`, usually `include_str!` of the file. When it is
    /// present, its `[[mods]]` entry decides the mod id, namespace, version, display name,
    /// description, authors and dependencies (see [`mods_toml`]), and `id`, `display_name` and
    /// `version` are used only to pick the entry and for `${file.jarVersion}`. The mod id then
    /// follows the `NeoForge` charset ([`mods_toml::is_valid_mod_id`]). A mod without one is
    /// known by `id`, `display_name` and `version`, and has no dependencies.
    fn mods_toml(&self) -> Option<&'static str> {
        None
    }
    /// Whether a client must have this mod to join. A client that cannot have it is disconnected
    /// at the configuration handshake. Override it to return `false` for a server-only mod.
    fn client_required(&self) -> bool {
        true
    }
    /// Called once at startup, before the first world loads.
    ///
    /// If a content registration fails, the server logs the first error with the mod id and
    /// stops after `init` returns, before the content phase. Do not panic or unwrap the `Result`
    /// of a content registration: the server panic hook writes a crash report and exits at once.
    fn init(&self, cx: &mut ModInit);
    /// `FMLCommonSetupEvent`: runs after the `init` of every mod, so the content of every mod is
    /// registered.
    fn common_setup(&self) {}
    /// `FMLDedicatedServerSetupEvent`: runs after the common setup of every mod.
    fn dedicated_server_setup(&self) {}
    /// `InterModEnqueueEvent`: send messages to other mods with [`InterModEnqueue::send`].
    fn inter_mod_enqueue(&self, _imc: &mut InterModEnqueue) {}
    /// `InterModProcessEvent`: read the messages sent to this mod with
    /// [`InterModProcess::get_messages`].
    fn inter_mod_process(&self, _imc: &mut InterModProcess) {}
    /// `FMLLoadCompleteEvent`: the last lifecycle step before the first world loads.
    fn load_complete(&self) {}
    /// Called when a config of this mod has loaded, like `ModConfigEvent.Loading`: a startup
    /// config inside [`ModInit::register_config`], a local config before `common_setup`, a synced
    /// config after `load_complete`. See [`config`].
    fn config_loading(&self, _config: &config::ModConfig) {}
    /// Called when a config of this mod reloads after an edit of its file, or after
    /// [`ModConfig::save`](config::ModConfig::save), like `ModConfigEvent.Reloading`.
    ///
    /// The config hooks of every mod run one at a time. A `save` of the same config from inside
    /// this method writes the file and does not call this method again.
    fn config_reloading(&self, _config: &config::ModConfig) {}
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
/// registrations ([`ModInit::register_block`], [`ModInit::register_item`],
/// [`ModInit::register_entity_type`] and [`ModInit::register_tag`]), which apply at once and
/// return a `Result`.
///
/// `ModInit` keeps the first content registration error, even if the mod ignores the `Result`.
/// After `init` returns, the server logs that error with the mod id and stops before the content
/// phase, as it does for a mod id conflict. A mod can still read the `Result` to log more detail
/// or to skip the rest of its content, but it cannot make the server start without that content.
///
/// This crate is the native API for mods. It passes `pumpkin-core` types through.
pub struct ModInit {
    id: &'static str,
    data_root: PathBuf,
    registrations: Vec<Registration>,
    content_error: Option<ContentRegistrationError>,
    config_dir: PathBuf,
    configs: Vec<Arc<config::ModConfig>>,
    /// The mod whose config hooks run, `None` when a test drives `init` without a server.
    owner: Option<&'static dyn NativeMod>,
}

/// The first content registration that failed in a mod's [`NativeMod::init`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentRegistrationError {
    /// The mod whose `init` made the registration.
    pub mod_id: &'static str,
    /// The registry of the content: block, item or entity type.
    pub kind: ContentKind,
    /// The name passed to the registration. A tag name starts with `#`, for example
    /// `#mymod:ores`.
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
            // `FMLPaths.CONFIGDIR`: the `config` folder in the working directory of the server.
            config_dir: PathBuf::from("config"),
            configs: Vec::new(),
            owner: None,
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

    /// Registers a handler for the event `E`, like `IEventBus#addListener` of `NeoForge`.
    ///
    /// The dispatch calls the blocking handlers from [`EventPriority::Highest`] to
    /// [`EventPriority::Lowest`], in registration order inside one priority, and then the
    /// non-blocking handlers in the same order. A blocking handler gets the event by mutable
    /// reference, and the server acts on the fields as the last handler left them. A handler
    /// without `receive_canceled` does not run once a handler before it cancelled the event, as a
    /// `NeoForge` listener without `receiveCanceled`.
    ///
    /// Handlers are synchronous inside the async server: the future a handler returns runs to the
    /// end before the next handler. A handler that needs a world call uses
    /// `futures::executor::block_on(tokio::task::unconstrained(..))`, as
    /// [`ModInit::register_service`] does, and never holds a lock of its own state across a fire.
    pub fn register_event<E: Payload + 'static, H: EventHandler<E> + 'static>(
        &mut self,
        handler: Arc<H>,
        priority: EventPriority,
        blocking: bool,
        receive_canceled: bool,
    ) {
        self.registrations.push(Box::new(move |cx| {
            cx.register_event_with::<E, H>(handler, priority, blocking, receive_canceled);
        }));
    }

    /// Registers a handler for the concrete parent event `P` and for each event that names `P` in
    /// `Payload::get_parent_names_static`, as a `NeoForge` listener on a parent class receives the
    /// subclasses. The handler gets the subclass struct as `dyn Payload` and downcasts it with
    /// `<dyn Payload>::downcast_mut`. The order and the flags are those of
    /// [`ModInit::register_event`].
    pub fn register_parent_event<P: Payload + 'static, H: ParentEventHandler + 'static>(
        &mut self,
        handler: Arc<H>,
        priority: EventPriority,
        blocking: bool,
        receive_canceled: bool,
    ) {
        self.registrations.push(Box::new(move |cx| {
            cx.register_parent_event::<P, H>(handler, priority, blocking, receive_canceled);
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
    /// property is invalid, a tag in the `minecraft` namespace or without one is unknown, a tag
    /// name is malformed, or the registry is frozen. A mod tag is not checked here: the freeze
    /// fails for a mod tag that no mod registers, and its error names the block but not the mod.
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
    /// in the `minecraft` namespace or without one is unknown, a tag name is malformed, or the
    /// registry is frozen. The block that the item places and a mod tag are not checked here: the
    /// freeze checks them, and its error names the item but not the mod.
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
    /// type, a dimension is not positive and finite, a tag in the `minecraft` namespace or without
    /// one is unknown, a tag name is malformed, or the registry is frozen. A mod tag is not checked
    /// here: the freeze fails for a mod tag that no mod registers, and its error names the type
    /// but not the mod.
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

    /// Registers a tag of the mod, with the members that the [`TagBuilder`](content::TagBuilder)
    /// lists. It applies at once, like [`ModInit::register_block`]. A block, item or entity type
    /// joins the tag with `.tag(..)` on its builder, before or after this call.
    ///
    /// # Errors
    ///
    /// Returns the [`RegistryError`] of the registry: the name is invalid, uses the `minecraft`
    /// namespace or is already registered as a tag of this registry, a vanilla member does not
    /// exist, or the registry is frozen. A generated tag of the registry, such as `c:ores`, counts
    /// as registered. Custom members are not checked here: the freeze checks them.
    pub fn register_tag(&mut self, tag: content::TagBuilder) -> Result<(), RegistryError> {
        let definition: dynamic::TagDefinition = tag.into();
        let (kind, name) = (definition.kind, format!("#{}", definition.name));
        self.keep_content_error(kind, name, dynamic::register_tag(definition))
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

/// A native mod with the id that the mod list knows it by.
pub(crate) struct LoadedMod<'a> {
    pub(crate) native_mod: &'static dyn NativeMod,
    pub(crate) mod_id: &'a str,
}

/// The linked mods by mod id, and the mod list in load order.
type Loaded = (HashMap<String, &'static dyn NativeMod>, ModList);

/// The native mods of `mod_list` in load order.
fn in_order<'a>(
    by_id: &HashMap<String, &'static dyn NativeMod>,
    mod_list: &'a ModList,
) -> Vec<LoadedMod<'a>> {
    mod_list
        .get_sorted_mods()
        .iter()
        .filter_map(|info| {
            by_id.get(&info.mod_id).map(|native_mod| LoadedMod {
                native_mod: *native_mod,
                mod_id: &info.mod_id,
            })
        })
        .collect()
}

/// The linked mods in load order. Fails on an invalid or duplicate id, an invalid
/// `neoforge.mods.toml`, an unmet dependency or an ordering cycle.
pub fn mods() -> Result<Vec<&'static dyn NativeMod>, String> {
    let (by_id, mod_list) = load(linked_mods())?;
    Ok(in_order(&by_id, &mod_list)
        .into_iter()
        .map(|loaded| loaded.native_mod)
        .collect())
}

fn linked_mods() -> Vec<&'static dyn NativeMod> {
    inventory::iter::<ModRegistration>
        .into_iter()
        .map(|registration| registration.0)
        .collect()
}

/// Reads the metadata of `mods`, checks the ids and the dependencies, and orders the mods and
/// the system mods ([`mod_list::sort`]).
fn load(mods: Vec<&'static dyn NativeMod>) -> Result<Loaded, String> {
    let mut infos = mod_list::system_mods();
    let system_count = infos.len();
    let mut by_id = HashMap::new();
    for native_mod in mods {
        let info = mod_info(native_mod)?;
        by_id.insert(info.mod_id.clone(), native_mod);
        infos.push(info);
    }
    let mut by_name: Vec<&ModInfo> = infos.iter().collect();
    by_name.sort_by(|a, b| a.mod_id.cmp(&b.mod_id));
    if let Some(pair) = by_name
        .windows(2)
        .find(|pair| pair[0].mod_id == pair[1].mod_id)
    {
        let kind = if infos[..system_count]
            .iter()
            .any(|system| system.mod_id == pair[0].mod_id)
        {
            "the system mod and the native mod"
        } else {
            "native mods"
        };
        return Err(format!(
            "{kind} \"{} {}\" and \"{} {}\" share the id \"{}\"",
            pair[0].display_name,
            pair[0].version,
            pair[1].display_name,
            pair[1].version,
            pair[0].mod_id
        ));
    }
    mod_list::check_dependencies(&infos)?;
    let order = mod_list::sort(&infos)?;
    let mut slots: Vec<Option<ModInfo>> = infos.into_iter().map(Some).collect();
    let sorted = order.into_iter().filter_map(|i| slots[i].take()).collect();
    Ok((by_id, ModList::new(sorted)))
}

/// The metadata of a mod: its `neoforge.mods.toml`, or its trait methods.
fn mod_info(native_mod: &dyn NativeMod) -> Result<ModInfo, String> {
    let (id, display_name, version) = (
        native_mod.id(),
        native_mod.display_name(),
        native_mod.version(),
    );
    if let Some(text) = native_mod.mods_toml() {
        return mods_toml::parse(text, id, version).map_err(|err| {
            format!(
                "native mod \"{display_name} {version}\" has an invalid neoforge.mods.toml: {err}"
            )
        });
    }
    if !is_valid_id(id) {
        return Err(format!(
            "native mod \"{display_name} {version}\" has the invalid id \"{id}\": use only a-z \
             0-9 _ . -"
        ));
    }
    Ok(ModInfo {
        mod_id: id.to_owned(),
        namespace: id.to_owned(),
        version: ArtifactVersion::new(version),
        display_name: display_name.to_owned(),
        description: display_name.to_owned(),
        authors: None,
        dependencies: Vec::new(),
    })
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
    run_init(native_mod, ModInit::new(native_mod.id()))
}

fn run_init(
    native_mod: &dyn NativeMod,
    mut init: ModInit,
) -> Result<ModInit, ContentRegistrationError> {
    native_mod.init(&mut init);
    init.content_error.take().map_or(Ok(init), Err)
}

fn refuse_to_start(reason: &dyn fmt::Display) -> ! {
    error!("[ironpumpkin] {reason}; refusing to start");
    std::process::exit(1);
}

fn native_mod_infos(mods: &[LoadedMod], mod_list: &ModList) -> Vec<NativeModInfo> {
    mods.iter()
        .filter_map(|loaded| {
            let info = mod_list.get_mod_container_by_id(loaded.mod_id)?;
            Some(NativeModInfo {
                id: info.mod_id.clone(),
                display_name: info.display_name.clone(),
                version: info.version.to_string(),
                client_required: loaded.native_mod.client_required(),
            })
        })
        .collect()
}

/// Initializes every linked mod against `server` in load order, then runs the [`lifecycle`] and
/// the config loads around it.
///
/// It stops the process if the mods conflict, a dependency is unmet or a mod fails to register
/// its content. Set as the startup hook of `pumpkin-core`.
pub fn init_mods(server: &Arc<Server>) {
    let (by_id, mod_list) = load(linked_mods()).unwrap_or_else(|err| refuse_to_start(&err));
    mod_list.set();
    let mod_list = ModList::get();
    let mods = in_order(&by_id, mod_list);
    startup::set_native_mods(native_mod_infos(&mods, mod_list));
    let mut configs = Vec::new();
    for loaded in &mods {
        let Some(info) = mod_list.get_mod_container_by_id(loaded.mod_id) else {
            continue;
        };
        let mut init = ModInit::new(loaded.mod_id);
        init.owner = Some(loaded.native_mod);
        let mut init =
            run_init(loaded.native_mod, init).unwrap_or_else(|err| refuse_to_start(&err));
        configs.append(&mut init.configs);
        let metadata = PluginMetadata {
            name: info.mod_id.clone(),
            version: info.version.to_string(),
            authors: info.authors.iter().cloned().collect(),
            description: info.description.clone(),
            dependencies: Vec::new(),
            permissions: Vec::new(),
        };
        init.apply(&startup::create_context(server, metadata));
    }
    lifecycle::run(&mods, mod_list, &configs).unwrap_or_else(|err| refuse_to_start(&err));
    config::start(server, configs).unwrap_or_else(|err| refuse_to_start(&err));
    let names: Vec<String> = startup::native_mods()
        .iter()
        .map(|native_mod| {
            format!(
                "{} (client_required = {})",
                native_mod.id, native_mod.client_required
            )
        })
        .collect();
    info!(
        "[ironpumpkin] loaded {} native mod{}{}",
        names.len(),
        if names.len() == 1 { "" } else { "s" },
        if names.is_empty() {
            String::new()
        } else {
            format!(": {}", names.join(", "))
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

    use super::{ModInit, NativeMod, in_order, load, mods, native_mod_infos};

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

    struct ServerOnlyMod;

    impl NativeMod for ServerOnlyMod {
        fn id(&self) -> &'static str {
            "server-only-mod"
        }

        fn display_name(&self) -> &'static str {
            "Server-only mod"
        }

        fn version(&self) -> &'static str {
            "2.0.0"
        }

        fn client_required(&self) -> bool {
            false
        }

        fn init(&self, _cx: &mut ModInit) {}
    }

    static DEFAULT: TestMod = TestMod("default-mod");
    static SERVER_ONLY: ServerOnlyMod = ServerOnlyMod;

    #[test]
    fn mods_are_client_required_unless_they_opt_out() {
        let (by_id, mod_list) = load(vec![&SERVER_ONLY, &DEFAULT]).unwrap();
        let infos: Vec<(String, bool)> = native_mod_infos(&in_order(&by_id, &mod_list), &mod_list)
            .into_iter()
            .map(|info| (info.id, info.client_required))
            .collect();
        assert_eq!(
            infos,
            [
                ("default-mod".to_owned(), true),
                ("server-only-mod".to_owned(), false)
            ]
        );
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
        let duplicate = load(vec![&DUPLICATE_A, &DUPLICATE_B]).err().unwrap();
        assert!(
            duplicate.contains("share the id \"dup-mod\""),
            "{duplicate}"
        );
        let invalid = load(vec![&INVALID]).err().unwrap();
        assert!(invalid.contains("invalid id \"Bad Id\""), "{invalid}");
        let system = load(vec![&MINECRAFT]).err().unwrap();
        assert!(
            system.contains("the system mod and the native mod"),
            "{system}"
        );
    }

    static MINECRAFT: TestMod = TestMod("minecraft");

    /// A mod whose `neoforge.mods.toml` is authoritative over its trait methods.
    struct TomlMod(&'static str, &'static str);

    impl NativeMod for TomlMod {
        fn id(&self) -> &'static str {
            self.0
        }

        fn display_name(&self) -> &'static str {
            "Trait name"
        }

        fn version(&self) -> &'static str {
            "3.1.4"
        }

        fn mods_toml(&self) -> Option<&'static str> {
            Some(self.1)
        }

        fn init(&self, _cx: &mut ModInit) {}
    }

    static EXAMPLE: TomlMod = TomlMod(
        "example-mod",
        "license = \"MIT\"\n[[mods]]\nmodId = \"examplemod\"\nnamespace = \"example\"\n\
         version = \"${file.jarVersion}\"\ndisplayName = \"Example Mod\"\n",
    );
    static BAD_ID: TomlMod = TomlMod("bad", "license = \"MIT\"\n[[mods]]\nmodId = \"My-Mod\"\n");
    static NEEDS_GECKOLIB: TomlMod = TomlMod(
        "needy",
        "license = \"MIT\"\n[[mods]]\nmodId = \"needy\"\n[[dependencies.needy]]\nmodId = \"geckolib\"\n\
         versionRange = \"[4.0,5.0)\"\n",
    );
    static ALPHA_AFTER_ZETA: TomlMod = TomlMod(
        "alpha",
        "license = \"MIT\"\n[[mods]]\nmodId = \"alpha\"\n[[dependencies.alpha]]\nmodId = \"zeta\"\n\
         ordering = \"AFTER\"\n",
    );
    static ZETA: TestMod = TestMod("zeta");

    #[test]
    fn the_mods_toml_decides_the_metadata() {
        let (by_id, mod_list) = load(vec![&EXAMPLE, &DEFAULT]).unwrap();
        let example = mod_list.get_mod_container_by_id("examplemod").unwrap();
        assert_eq!(example.namespace, "example");
        assert_eq!(example.display_name, "Example Mod");
        assert_eq!(example.version.to_string(), "3.1.4");
        assert!(!mod_list.is_loaded("example-mod"));
        // A mod without a mods.toml is known by its trait methods.
        let default = mod_list.get_mod_container_by_id("default-mod").unwrap();
        assert_eq!(default.display_name, "Test mod");
        assert_eq!(default.namespace, "default-mod");
        let ids: Vec<&str> = in_order(&by_id, &mod_list)
            .iter()
            .map(|loaded| loaded.mod_id)
            .collect();
        assert_eq!(ids, ["default-mod", "examplemod"]);
        assert_eq!(mod_list.size(), 4);
    }

    #[test]
    fn a_bad_mods_toml_id_or_dependency_stops_the_load() {
        let bad_id = load(vec![&BAD_ID]).err().unwrap();
        assert!(bad_id.contains("invalid modId \"My-Mod\""), "{bad_id}");
        let missing = load(vec![&NEEDS_GECKOLIB]).err().unwrap();
        assert!(missing.contains("requires \"geckolib\""), "{missing}");
    }

    #[test]
    fn init_runs_in_dependency_order() {
        let (by_id, mod_list) = load(vec![&ALPHA_AFTER_ZETA, &ZETA]).unwrap();
        let ids: Vec<&str> = in_order(&by_id, &mod_list)
            .iter()
            .map(|loaded| loaded.mod_id)
            .collect();
        assert_eq!(ids, ["zeta", "alpha"]);
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
