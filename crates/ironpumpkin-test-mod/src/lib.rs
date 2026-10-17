//! Native mod for the boot tests of the root workspace. It registers one block, one item and one
//! entity type, each with a vanilla display entry, a stair block [`STAIRS`] that takes the block
//! type `minecraft:oak_stairs`, one sound event, mob effect, attribute and data component type, the
//! default attributes of its entity type through `EntityAttributeCreationEvent`, the startup config
//! [`STARTUP_CONFIG`], the local config [`LOCAL_CONFIG`], the synced config [`SYNCED_CONFIG`], and
//! a handler for each server lifecycle and tick event. Its `init`, mod lifecycle methods, config loading hook and
//! event handlers log [`EVENT_LOG_PREFIX`] and the `NeoForge` event name, an event handler only
//! the first time its event fires. A test binary links it by using one of its items, such as
//! `use ironpumpkin_test_mod::BLOCK;`, and starts the server with `pumpkin::run`.
//!
//! It also registers optional payload channels and two configuration tasks ([`LICENCE_CHANNEL`]
//! and the other channel constants). A client that negotiated [`LICENCE_CHANNEL`] gets the
//! licence task, which sends [`LICENCE_CHALLENGE`] and finishes when the client sends it back,
//! then the greeting task, which sends [`GREETING`] and finishes at once. A player whose name
//! starts with [`STRAY_PLAYER_PREFIX`] gets only the stray task instead, which sends the challenge
//! and never finishes, so the answer arrives while no licence task waits. When a player joins,
//! the mod sends [`ECHO_PING`] on [`ECHO_CHANNEL`], and its handler of that channel replies on
//! [`ECHO_REPLY_CHANNEL`] and logs [`ECHO_LOG`] with the player name.

use std::{
    marker::PhantomData,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    config::{ConfigError, ConfigSpec, ConfigType, ModConfig},
    content::{BlockBuilder, EntityTypeBuilder, ItemBuilder, RegistryError},
    data_components::DataComponentTypeBuilder,
    entity::{Entity, EntityBase},
    entity_attributes::AttributeBuilder,
    event::{
        BoxFuture, EventHandler, EventPriority, Payload,
        player::player_join::PlayerJoinEvent,
        server::{
            ServerAboutToStartEvent, ServerLoadEvent, ServerStartingEvent, ServerStoppedEvent,
            ServerStoppingEvent, ServerTickEndEvent, ServerTickStartEvent,
        },
        world::{LevelTickEventPost, LevelTickEventPre},
    },
    mob_effects::{MobEffectBuilder, MobEffectCategory},
    network::{
        CustomConfigurationTask, PayloadContext, RegisterConfigurationTasksEvent,
        RegisterPayloadHandlersEvent, TaskContext, send_to_player,
    },
    pumpkin_data::{Block, attributes::Attributes, entity::EntityType, item::Item},
    register_mod,
    registration::{AttributeSupplier, EntityAttributeCreationEvent},
    server::Server,
    sound_events::SoundEventBuilder,
    tracing::{error, info},
};

pub const ID: &str = "test-mod";
pub const BLOCK: &str = "test-mod:test_block";
pub const ITEM: &str = "test-mod:test_item";
pub const ENTITY_TYPE: &str = "test-mod:test_entity";
/// A stair with the block type `minecraft:oak_stairs` and no behaviour of its own.
pub const STAIRS: &str = "test-mod:test_stairs";
pub const SOUND_EVENT: &str = "test-mod:test_sound";
pub const MOB_EFFECT: &str = "test-mod:test_effect";
pub const ATTRIBUTE: &str = "test-mod:test_attribute";
pub const DATA_COMPONENT_TYPE: &str = "test-mod:test_component";
/// The default file name of the synced config, in `config/`.
pub const SYNCED_CONFIG: &str = "test-mod-synced.toml";
/// The default file name of the startup config, in `config/`. Its `greeting` is logged from
/// `init` after [`STARTUP_GREETING`].
pub const STARTUP_CONFIG: &str = "test-mod-startup.toml";
/// The default file name of the local config, in `config/`. `common_setup` reads it.
pub const LOCAL_CONFIG: &str = "test-mod-local.toml";
pub const STARTUP_GREETING: &str = "test-mod startup greeting: ";
/// The log line of the reloading hook for the synced config.
pub const SYNCED_RELOADED: &str = "test-mod reloaded test-mod-synced.toml";
/// The start of the log line of an event handler, followed by the `NeoForge` event name.
pub const EVENT_LOG_PREFIX: &str = "[test-mod] event ";

/// The configuration channel of the licence task, in both directions.
pub const LICENCE_CHANNEL: &str = "test-mod:licence";
/// The client-bound configuration channel of the greeting task.
pub const GREETING_CHANNEL: &str = "test-mod:greeting";
/// The play channel that the client echoes and the mod handles, in both directions.
pub const ECHO_CHANNEL: &str = "test-mod:echo";
/// The client-bound play channel of the reply to an echo.
pub const ECHO_REPLY_CHANNEL: &str = "test-mod:echo_reply";
pub const LICENCE_TASK: &str = "test-mod:licence";
pub const GREETING_TASK: &str = "test-mod:greeting";
pub const STRAY_TASK: &str = "test-mod:stray";
pub const LICENCE_CHALLENGE: &[u8] = b"licence?";
pub const GREETING: &[u8] = b"hello";
pub const ECHO_PING: &[u8] = b"ping";
/// The start of the name of a player who gets the stray task.
pub const STRAY_PLAYER_PREFIX: &str = "Stray";
/// The log line of the echo handler, followed by the player name.
pub const ECHO_LOG: &str = "test-mod echo from ";

/// The local config, which `common_setup` reads.
static LOCAL: OnceLock<Arc<ModConfig>> = OnceLock::new();

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
        info!("{EVENT_LOG_PREFIX}FMLConstructModEvent");
        if let Err(err) = register_configs(cx) {
            error!("{ID}: {err}");
        }
        // `ModInit` keeps the first content error, and the server stops on it after `init`.
        let _ = register_content(cx);
        register_events(cx);
    }

    fn common_setup(&self) {
        info!("{EVENT_LOG_PREFIX}FMLCommonSetupEvent");
        // NeoForge loads the local configs before the common setup.
        if let Some(Err(err)) = LOCAL.get().map(|local| local.get::<bool>("verbose")) {
            error!("{ID}: the common setup cannot read {LOCAL_CONFIG}: {err}");
        }
    }

    fn load_complete(&self) {
        info!("{EVENT_LOG_PREFIX}FMLLoadCompleteEvent");
    }

    fn config_loading(&self, config: &ModConfig) {
        info!(
            "{EVENT_LOG_PREFIX}ModConfigEvent.Loading {}",
            config.config_type().extension().to_ascii_uppercase()
        );
    }

    fn config_reloading(&self, config: &ModConfig) {
        info!("{ID} reloaded {}", config.file_name());
    }

    fn entity_attribute_creation(&self, event: &mut EntityAttributeCreationEvent) {
        info!("{EVENT_LOG_PREFIX}EntityAttributeCreationEvent");
        // The handle keeps an error, and the server stops on it after this step.
        let _ = event.put(
            ENTITY_TYPE,
            AttributeSupplier::of(&EntityType::PIG).add(Attributes::MAX_HEALTH, 20.0),
        );
    }
}

fn register_configs(cx: &mut ModInit) -> Result<(), ConfigError> {
    let mut startup = ConfigSpec::builder();
    startup
        .comment(" Logged from init")
        .define("greeting", "hello".to_owned());
    let startup = cx.register_config(ConfigType::Startup, startup.build()?)?;
    info!("{STARTUP_GREETING}{}", startup.get::<String>("greeting")?);

    let mut local = ConfigSpec::builder();
    local
        .comment(" Read by the common setup")
        .define("verbose", false);
    let _ = LOCAL.set(cx.register_config(ConfigType::Local, local.build()?)?);

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
    cx.register_block(
        BlockBuilder::new(STAIRS, Block::OAK_STAIRS.default_state.id)
            .block_type("minecraft:oak_stairs"),
    )?;
    cx.register_item(ItemBuilder::new(ITEM, &Item::STICK))?;
    cx.register_entity_type(EntityTypeBuilder::new(ENTITY_TYPE, &EntityType::PIG), spawn)?;
    cx.register_sound_event(SoundEventBuilder::new(SOUND_EVENT))?;
    cx.register_mob_effect(MobEffectBuilder::new(
        MOB_EFFECT,
        MobEffectCategory::Beneficial,
        0x00_FF_00,
    ))?;
    cx.register_attribute(AttributeBuilder::new(ATTRIBUTE, 1.0).syncable(true))?;
    cx.register_data_component_type(DataComponentTypeBuilder::<i32>::new(DATA_COMPONENT_TYPE))
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
    log_once::<ServerAboutToStartEvent>(cx, "ServerAboutToStartEvent");
    log_once::<ServerStartingEvent>(cx, "ServerStartingEvent");
    log_once::<ServerLoadEvent>(cx, "ServerStartedEvent");
    log_once::<ServerTickStartEvent>(cx, "ServerTickEventPre");
    log_once::<ServerTickEndEvent>(cx, "ServerTickEventPost");
    log_once::<LevelTickEventPre>(cx, "LevelTickEventPre");
    log_once::<LevelTickEventPost>(cx, "LevelTickEventPost");
    log_once::<ServerStoppingEvent>(cx, "ServerStoppingEvent");
    log_once::<ServerStoppedEvent>(cx, "ServerStoppedEvent");
    cx.register_event::<RegisterPayloadHandlersEvent, _>(
        Arc::new(Payloads),
        EventPriority::Normal,
        false,
        false,
    );
    cx.register_event::<RegisterConfigurationTasksEvent, _>(
        Arc::new(Tasks),
        EventPriority::Normal,
        false,
        false,
    );
    cx.register_event::<PlayerJoinEvent, _>(Arc::new(Ping), EventPriority::Normal, false, false);
}

/// Registers the channels of the mod, all optional, so a client without them still joins.
struct Payloads;

impl EventHandler<RegisterPayloadHandlersEvent> for Payloads {
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a RegisterPayloadHandlersEvent,
    ) -> BoxFuture<'a, ()> {
        info!("{EVENT_LOG_PREFIX}RegisterPayloadHandlersEvent");
        let registrar = event.registrar("1").optional();
        let registered = registrar
            .configuration_bidirectional(LICENCE_CHANNEL, on_licence)
            .and_then(|registrar| registrar.configuration_to_client(GREETING_CHANNEL))
            .and_then(|registrar| registrar.play_bidirectional(ECHO_CHANNEL, on_echo))
            .and_then(|registrar| registrar.play_to_client(ECHO_REPLY_CHANNEL));
        if let Err(err) = registered {
            error!("{ID}: {err}");
        }
        Box::pin(async {})
    }
}

/// The client's answer to the challenge finishes the licence task.
fn on_licence(data: &[u8], context: &mut PayloadContext<'_>) {
    if data != LICENCE_CHALLENGE {
        context.disconnect(ironpumpkin_mods::text::TextComponent::text("Wrong licence"));
        return;
    }
    info!("{ID} licence answered");
    if let Err(err) = context.finish_current_task(LICENCE_TASK) {
        error!("{ID}: {err}");
    }
}

fn on_echo(data: &[u8], context: &mut PayloadContext<'_>) {
    let name = context
        .player()
        .map_or_else(String::new, |player| player.gameprofile.name.clone());
    info!("{ECHO_LOG}{name}");
    if let Err(err) = context.reply(ECHO_REPLY_CHANNEL, data) {
        error!("{ID}: {err}");
    }
}

struct Tasks;

impl EventHandler<RegisterConfigurationTasksEvent> for Tasks {
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a RegisterConfigurationTasksEvent,
    ) -> BoxFuture<'a, ()> {
        if event.has_channel(LICENCE_CHANNEL) {
            if event.player_name().starts_with(STRAY_PLAYER_PREFIX) {
                event.register(Arc::new(Task(STRAY_TASK)));
            } else {
                event.register(Arc::new(Task(LICENCE_TASK)));
                event.register(Arc::new(Task(GREETING_TASK)));
            }
        }
        Box::pin(async {})
    }
}

/// The licence, greeting and stray tasks, by type.
struct Task(&'static str);

impl CustomConfigurationTask for Task {
    fn task_type(&self) -> &'static str {
        self.0
    }

    fn run(&self, context: &mut TaskContext<'_>) {
        let sent = match self.0 {
            GREETING_TASK => {
                context.finish();
                context.send(GREETING_CHANNEL, GREETING)
            }
            _ => context.send(LICENCE_CHANNEL, LICENCE_CHALLENGE),
        };
        if let Err(err) = sent {
            error!("{ID}: {err}");
        }
    }
}

/// Sends the ping of the echo channel to a joining player whose client has the channel.
struct Ping;

impl EventHandler<PlayerJoinEvent> for Ping {
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a PlayerJoinEvent,
    ) -> BoxFuture<'a, ()> {
        // A client without the channel gets nothing: the call fails with `ChannelNotNegotiated`.
        let _ = send_to_player(&event.player, ECHO_CHANNEL, ECHO_PING);
        Box::pin(async {})
    }
}

register_mod!(TestMod);
