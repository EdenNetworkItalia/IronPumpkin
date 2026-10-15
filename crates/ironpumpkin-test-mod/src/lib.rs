//! Native mod for the boot tests of the root workspace. It registers one block, one item and one
//! entity type, each with a vanilla display entry, the synced config [`SYNCED_CONFIG`], the
//! startup config [`STARTUP_CONFIG`], and a handler for each server lifecycle and tick event that
//! logs [`EVENT_LOG_PREFIX`] and the event name the first time the event fires. A test binary links
//! it by using one of its items, such as
//! `use ironpumpkin_test_mod::BLOCK;`, and starts the server with `pumpkin::run`.

use std::{
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    config::{ConfigError, ConfigSpec, ConfigType, ModConfig},
    content::{BlockBuilder, EntityTypeBuilder, ItemBuilder, RegistryError},
    entity::{Entity, EntityBase},
    event::{
        BoxFuture, EventHandler, EventPriority, Payload,
        server::{
            ServerLoadEvent, ServerStartingEvent, ServerStoppedEvent, ServerStoppingEvent,
            ServerTickEndEvent, ServerTickStartEvent,
        },
        world::{LevelTickEventPost, LevelTickEventPre},
    },
    pumpkin_data::{Block, entity::EntityType, item::Item},
    register_mod,
    server::Server,
    tracing::{error, info},
};

pub const ID: &str = "test-mod";
pub const BLOCK: &str = "test-mod:test_block";
pub const ITEM: &str = "test-mod:test_item";
pub const ENTITY_TYPE: &str = "test-mod:test_entity";
/// The default file name of the synced config, in `config/`.
pub const SYNCED_CONFIG: &str = "test-mod-synced.toml";
/// The default file name of the startup config, in `config/`. Its `greeting` is logged from
/// `init` after [`STARTUP_GREETING`].
pub const STARTUP_CONFIG: &str = "test-mod-startup.toml";
pub const STARTUP_GREETING: &str = "test-mod startup greeting: ";
/// The log line of the reloading hook for the synced config.
pub const SYNCED_RELOADED: &str = "test-mod reloaded test-mod-synced.toml";
/// The start of the log line of an event handler, followed by the `NeoForge` event name.
pub const EVENT_LOG_PREFIX: &str = "[test-mod] event ";

struct TestMod;

impl NativeMod for TestMod {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        "Test Mod"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn init(&self, cx: &mut ModInit) {
        if let Err(err) = register_configs(cx) {
            error!("{ID}: {err}");
        }
        // `ModInit` keeps the first content error, and the server stops on it after `init`.
        let _ = register_content(cx);
        register_events(cx);
    }

    fn config_reloading(&self, config: &ModConfig) {
        info!("{ID} reloaded {}", config.file_name());
    }
}

fn register_configs(cx: &mut ModInit) -> Result<(), ConfigError> {
    let mut startup = ConfigSpec::builder();
    startup
        .comment(" Logged from init")
        .define("greeting", "hello".to_owned());
    let startup = cx.register_config(ConfigType::Startup, startup.build()?)?;
    info!("{STARTUP_GREETING}{}", startup.get::<String>("greeting")?);

    let mut synced = ConfigSpec::builder();
    synced.comment(" General settings").push("general");
    synced
        .comment(" Whether the mod is on")
        .define("enabled", true);
    synced.define_in_range("damage", 10i64, 0, 100);
    synced.pop();
    cx.register_config(ConfigType::Synced, synced.build()?)?;
    Ok(())
}

fn register_content(cx: &mut ModInit) -> Result<(), RegistryError> {
    cx.register_block(BlockBuilder::new(
        BLOCK,
        Block::REDSTONE_LAMP.default_state.id,
    ))?;
    cx.register_item(ItemBuilder::new(ITEM, &Item::STICK))?;
    cx.register_entity_type(EntityTypeBuilder::new(ENTITY_TYPE, &EntityType::PIG), spawn)
}

fn spawn(entity: Entity) -> Arc<dyn EntityBase> {
    Arc::new(entity)
}

/// Logs `name` the first time its event fires.
struct LogOnce<E> {
    name: &'static str,
    logged: AtomicBool,
    _event: PhantomData<fn() -> E>,
}

impl<E: Payload + Send + Sync + 'static> EventHandler<E> for LogOnce<E> {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        _event: &'a mut E,
    ) -> BoxFuture<'a, ()> {
        if !self.logged.swap(true, Ordering::Relaxed) {
            info!("{EVENT_LOG_PREFIX}{}", self.name);
        }
        Box::pin(async {})
    }
}

fn log_once<E: Payload + Send + Sync + 'static>(cx: &mut ModInit, name: &'static str) {
    let handler = Arc::new(LogOnce::<E> {
        name,
        logged: AtomicBool::new(false),
        _event: PhantomData,
    });
    cx.register_event::<E, _>(handler, EventPriority::Normal, true, false);
}

fn register_events(cx: &mut ModInit) {
    log_once::<ServerStartingEvent>(cx, "ServerStartingEvent");
    log_once::<ServerLoadEvent>(cx, "ServerStartedEvent");
    log_once::<ServerTickStartEvent>(cx, "ServerTickEventPre");
    log_once::<ServerTickEndEvent>(cx, "ServerTickEventPost");
    log_once::<LevelTickEventPre>(cx, "LevelTickEventPre");
    log_once::<LevelTickEventPost>(cx, "LevelTickEventPost");
    log_once::<ServerStoppingEvent>(cx, "ServerStoppingEvent");
    log_once::<ServerStoppedEvent>(cx, "ServerStoppedEvent");
}

register_mod!(TestMod);
