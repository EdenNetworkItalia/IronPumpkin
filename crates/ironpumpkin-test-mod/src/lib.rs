//! Native mod for the boot tests of the root workspace. It registers one block, one item and one
//! entity type, each with a vanilla display entry, a stair block [`STAIRS`] that takes the block
//! type `minecraft:oak_stairs`, one sound event, mob effect, attribute and data component type, the
//! default attributes of its entity type through `EntityAttributeCreationEvent`, the startup config
//! [`STARTUP_CONFIG`], the local config [`LOCAL_CONFIG`], the synced config [`SYNCED_CONFIG`], and
//! a handler for each server lifecycle and tick event, the command [`COMMAND`] and the service
//! [`SERVICE`]. Its block exposes an item handler through `Capabilities.Item.BLOCK`
//! ([`block_storage`]). Its `init`, mod lifecycle methods, config loading hook and event handlers log
//! [`EVENT_LOG_PREFIX`] and the `NeoForge` event name, an event handler only the first time its
//! event fires, and count each call ([`event_count`]). A test binary links it by using one of its items, such as
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
    collections::BTreeMap,
    marker::PhantomData,
    sync::{Arc, Mutex, OnceLock, PoisonError},
};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    capabilities::{self, RegisterCapabilitiesEvent},
    command::{
        argument_builder::{ArgumentBuilder, command},
        context::command_context::CommandContext,
        node::{CommandExecutor, CommandExecutorResult},
    },
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
    math::position::BlockPos,
    mob_effects::{MobEffectBuilder, MobEffectCategory},
    network::{
        CustomConfigurationTask, PayloadContext, RegisterConfigurationTasksEvent,
        RegisterPayloadHandlersEvent, TaskContext, send_to_player,
    },
    permission::{Permission, PermissionDefault},
    pumpkin_data::{Block, attributes::Attributes, entity::EntityType, item::Item},
    register_mod,
    registration::{AttributeSupplier, EntityAttributeCreationEvent},
    server::Server,
    sound_events::SoundEventBuilder,
    tracing::{error, info},
    transfer::{ItemResource, ItemStacksResourceHandler, ResourceHandler},
    world::World,
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
/// The command of the mod. It succeeds with 1 and counts as the event `"command"`.
pub const COMMAND: &str = "testmod";
/// The permission of [`COMMAND`].
pub const COMMAND_PERMISSION: &str = "test-mod:command.testmod";
/// The name of [`TestModService`].
pub const SERVICE: &str = "test-mod:service";

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
static SYNCED: OnceLock<Arc<ModConfig>> = OnceLock::new();

/// `general.damage` of the synced config, a `world_restart` value.
pub fn synced_damage() -> Option<i64> {
    SYNCED.get()?.get("general.damage").ok()
}

static EVENT_COUNTS: Mutex<BTreeMap<&'static str, usize>> = Mutex::new(BTreeMap::new());

/// How many times the mod saw the event `name` in this process.
///
/// `name` is the `NeoForge` event name of a log line, `"ModConfigEvent.Reloading SYNCED"` (or
/// `LOCAL`, `STARTUP`) for a config reload, `"command"` for a run of [`COMMAND`], or `"echo"`
/// for a payload on [`ECHO_CHANNEL`].
pub fn event_count(name: &str) -> usize {
    EVENT_COUNTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(name)
        .copied()
        .unwrap_or(0)
}

/// Counts the event `name` and returns whether it is its first time.
fn count(name: &'static str) -> bool {
    let mut counts = EVENT_COUNTS.lock().unwrap_or_else(PoisonError::into_inner);
    let seen = counts.entry(name).or_default();
    *seen += 1;
    *seen == 1
}

/// Counts and logs the event `name`.
fn log_event(name: &'static str) {
    count(name);
    info!("{EVENT_LOG_PREFIX}{name}");
}

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
        log_event("FMLConstructModEvent");
        if let Err(err) = register_configs(cx) {
            error!("{ID}: {err}");
        }
        // `ModInit` keeps the first content error, and the server stops on it after `init`.
        let _ = register_content(cx);
        register_events(cx);
        register_command(cx);
        cx.register_service(SERVICE, Arc::new(TestModService));
    }

    fn common_setup(&self) {
        log_event("FMLCommonSetupEvent");
        // NeoForge loads the local configs before the common setup.
        if let Some(Err(err)) = LOCAL.get().map(|local| local.get::<bool>("verbose")) {
            error!("{ID}: the common setup cannot read {LOCAL_CONFIG}: {err}");
        }
    }

    fn load_complete(&self) {
        log_event("FMLLoadCompleteEvent");
    }

    fn config_loading(&self, config: &ModConfig) {
        log_event(match config.config_type() {
            ConfigType::Local => "ModConfigEvent.Loading LOCAL",
            ConfigType::Synced => "ModConfigEvent.Loading SYNCED",
            ConfigType::Startup => "ModConfigEvent.Loading STARTUP",
        });
    }

    fn config_reloading(&self, config: &ModConfig) {
        count(match config.config_type() {
            ConfigType::Local => "ModConfigEvent.Reloading LOCAL",
            ConfigType::Synced => "ModConfigEvent.Reloading SYNCED",
            ConfigType::Startup => "ModConfigEvent.Reloading STARTUP",
        });
        info!("{ID} reloaded {}", config.file_name());
    }

    fn entity_attribute_creation(&self, event: &mut EntityAttributeCreationEvent) {
        log_event("EntityAttributeCreationEvent");
        // The handle keeps an error, and the server stops on it after this step.
        let _ = event.put(
            ENTITY_TYPE,
            AttributeSupplier::of(&EntityType::PIG).add(Attributes::MAX_HEALTH, 20.0),
        );
    }

    fn register_capabilities(&self, event: &mut RegisterCapabilitiesEvent) {
        if *event.phase() != EventPriority::Normal {
            return;
        }
        log_event("RegisterCapabilitiesEvent");
        // The handle keeps an error, and the server stops on it after the event.
        let _ = event.register_block(
            &capabilities::item::BLOCK,
            |_, pos, _, _, _| {
                let handler: Arc<dyn ResourceHandler<ItemResource>> = block_storage(*pos);
                Some(handler)
            },
            &[BLOCK],
        );
    }
}

type Storage = Mutex<BTreeMap<(i32, i32, i32), Arc<ItemStacksResourceHandler>>>;

static BLOCK_STORAGE: Storage = Mutex::new(BTreeMap::new());

/// The item storage of the [`BLOCK`] at `pos`: one slot of up to 64 items.
///
/// The block exposes it through `Capabilities.Item.BLOCK` from every side. The block has no block
/// entity, so the storage lives here, keyed by position.
pub fn block_storage(pos: BlockPos) -> Arc<ItemStacksResourceHandler> {
    BLOCK_STORAGE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry((pos.0.x, pos.0.y, pos.0.z))
        .or_insert_with(|| Arc::new(ItemStacksResourceHandler::new(1, 64)))
        .clone()
}

/// Drops the storage of the [`BLOCK`] at `pos`, as when the block breaks, and tells the
/// capability caches there, since a block without a block entity invalidates its capabilities
/// itself.
pub fn remove_block_storage(world: &World, pos: BlockPos) {
    BLOCK_STORAGE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&(pos.0.x, pos.0.y, pos.0.z));
    capabilities::invalidate_capabilities(world, &pos);
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
    synced
        .world_restart()
        .define_in_range("damage", 10i64, 0, 100);
    synced.pop();
    let _ = SYNCED.set(cx.register_config(ConfigType::Synced, synced.build()?)?);
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

/// Counts each fire of its event and logs `name` the first time.
struct LogOnce<E> {
    name: &'static str,
    _event: PhantomData<fn() -> E>,
}

impl<E: Payload + Send + Sync + 'static> EventHandler<E> for LogOnce<E> {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        _event: &'a mut E,
    ) -> BoxFuture<'a, ()> {
        if count(self.name) {
            info!("{EVENT_LOG_PREFIX}{}", self.name);
        }
        Box::pin(async {})
    }
}

fn log_once<E: Payload + Send + Sync + 'static>(cx: &mut ModInit, name: &'static str) {
    let handler = Arc::new(LogOnce::<E> {
        name,
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

fn register_command(cx: &mut ModInit) {
    let description = "Counts its runs.";
    cx.register_permission(Permission::new(
        COMMAND_PERMISSION,
        description,
        PermissionDefault::Allow,
    ));
    cx.register_command(
        command(COMMAND, description).executes(CountRuns),
        format!("command.{COMMAND}"),
    );
}

struct CountRuns;

/// The service that the mod registers as [`SERVICE`].
pub struct TestModService;

impl Payload for TestModService {
    fn get_name_static() -> &'static str {
        "TestModService"
    }

    fn get_name(&self) -> &'static str {
        Self::get_name_static()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl CommandExecutor for CountRuns {
    fn execute(&self, _context: &CommandContext) -> CommandExecutorResult {
        count("command");
        Ok(1)
    }
}

/// Registers the channels of the mod, all optional, so a client without them still joins.
struct Payloads;

impl EventHandler<RegisterPayloadHandlersEvent> for Payloads {
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a RegisterPayloadHandlersEvent,
    ) -> BoxFuture<'a, ()> {
        log_event("RegisterPayloadHandlersEvent");
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
    count("echo");
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
