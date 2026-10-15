//! `RegisterEvent` through the event bus of a real server, and the registry and tag lookups over
//! the frozen tables (`openspec/changes/neoforge-shaped-plugin-api`, spec
//! `neoforge-registration`). The server runs `init_mods` as its startup hook and freezes the
//! content registry once per process, so this file is its own test binary. The server lives in a
//! directory of the process under `CARGO_TARGET_TMPDIR`, runs no ticker and binds no port.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex, OnceLock, PoisonError},
};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    content::{
        BlockBuilder, ContentKind, EntityTypeBuilder, ItemBuilder, RegisterEventExt, TagBuilder,
    },
    entity::{Entity, EntityBase},
    event::{BoxFuture, EventHandler, EventPriority, registries::RegisterEvent},
    pumpkin_core::{data::VanillaData, plugin::startup::STARTUP_HOOK},
    pumpkin_data::{Block, BlockId, entity::EntityType, item::Item},
    register_mod, registry,
    server::Server,
};
use pumpkin_config::{AdvancedConfiguration, BasicConfiguration, TelemetryConfig};
use tokio::runtime::Runtime;

static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn log(entry: String) {
    LOG.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(entry);
}

fn spawn(entity: Entity) -> Arc<dyn EntityBase> {
    Arc::new(entity)
}

/// Registers a block and a mod tag in `init`, and a block, an item and an entity type from two
/// `RegisterEvent` handlers.
struct EventMod;

impl NativeMod for EventMod {
    fn id(&self) -> &'static str {
        "event-mod"
    }

    fn display_name(&self) -> &'static str {
        "Event mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        log("init".to_owned());
        cx.register_block(BlockBuilder::new(
            "event-mod:init_block",
            Block::STONE.default_state.id,
        ))
        .unwrap();
        cx.register_tag(
            TagBuilder::new(ContentKind::Block, "event-mod:lamps")
                .value("minecraft:redstone_lamp")
                .value("event-mod:init_block"),
        )
        .unwrap();
        cx.register_event::<RegisterEvent, _>(
            Arc::new(RegisterBlocking),
            EventPriority::Normal,
            true,
            false,
        );
        cx.register_event::<RegisterEvent, _>(
            Arc::new(RegisterNonBlocking),
            EventPriority::Normal,
            false,
            false,
        );
    }

    fn common_setup(&self) {
        log("common_setup".to_owned());
    }
}

register_mod!(EventMod);

/// Logs each fire and registers a block and the item that places it.
struct RegisterBlocking;

impl EventHandler<RegisterEvent> for RegisterBlocking {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut RegisterEvent,
    ) -> BoxFuture<'a, ()> {
        log(format!("RegisterEvent {}", event.registry_key()));
        event
            .register_block(BlockBuilder::new(
                "event-mod:event_block",
                Block::REDSTONE_LAMP.default_state.id,
            ))
            .unwrap();
        event
            .register_item(
                ItemBuilder::new("event-mod:event_item", &Item::REDSTONE_LAMP)
                    .places("event-mod:event_block"),
            )
            .unwrap();
        Box::pin(async {})
    }
}

/// Registers an entity type from a handler that gets the event by shared reference.
struct RegisterNonBlocking;

impl EventHandler<RegisterEvent> for RegisterNonBlocking {
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a RegisterEvent,
    ) -> BoxFuture<'a, ()> {
        event
            .register_entity_type(
                EntityTypeBuilder::new("event-mod:event_golem", &EntityType::ZOMBIE),
                spawn,
            )
            .unwrap();
        Box::pin(async {})
    }
}

/// The server keeps tasks on this runtime.
static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
});

/// The lock that marks the directory of this process as in use. The OS releases it when the
/// process ends, however it ends.
static DIR_LOCK: OnceLock<File> = OnceLock::new();

/// Creates the directory of this process and removes the directories of ended processes of the
/// same binary. A test runner with one process per test (`cargo nextest`) starts many processes
/// of one binary at once, so they cannot share a directory. Same logic as `process_dir` in
/// `crates/pumpkin-core/tests/support/mod.rs`.
fn process_dir() -> PathBuf {
    let binary = std::env::current_exe().unwrap();
    let name = binary.file_stem().unwrap().to_string_lossy().into_owned();
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(&root).unwrap();
    remove_ended_dirs(&root, &name);

    let id = format!("{name}-{}", std::process::id());
    // The lock comes before the directory, so the scan never finds a directory in creation.
    let lock = File::create(root.join(format!("{id}.lock"))).unwrap();
    lock.lock().unwrap();
    DIR_LOCK.set(lock).unwrap();
    let dir = root.join(id);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Removes each `<name>-<pid>` directory in `root` whose lock nobody holds.
fn remove_ended_dirs(root: &Path, name: &str) {
    let prefix = format!("{name}-");
    for entry in std::fs::read_dir(root).unwrap().flatten() {
        let lock = entry.path();
        let Some(id) = lock
            .file_name()
            .and_then(|file| file.to_str())
            .and_then(|file| file.strip_suffix(".lock"))
        else {
            continue;
        };
        let Some(pid) = id.strip_prefix(&prefix) else {
            continue;
        };
        if pid.is_empty() || !pid.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let Ok(file) = File::options().write(true).open(&lock) else {
            continue;
        };
        if file.try_lock().is_err() {
            continue;
        }
        // Windows refuses to delete a file that is open.
        drop(file);
        let _ = std::fs::remove_dir_all(root.join(id));
        let _ = std::fs::remove_file(&lock);
    }
}

static SERVER: LazyLock<Arc<Server>> = LazyLock::new(|| {
    let dir = process_dir();
    // `VanillaData::load`, the configs and the plugin data folders use the working directory.
    std::env::set_current_dir(&dir).unwrap();
    STARTUP_HOOK.set(ironpumpkin_mods::init_mods).unwrap();

    let basic = BasicConfiguration {
        default_level_name: dir.join("world").to_string_lossy().into_owned(),
        allow_nether: false,
        allow_end: false,
        allow_chat_reports: false,
        ..BasicConfiguration::default()
    };
    let mut advanced = AdvancedConfiguration::default();
    advanced.networking.bedrock.enabled = false;
    RUNTIME.block_on(async {
        Server::new(
            basic,
            advanced,
            TelemetryConfig::default(),
            VanillaData::load(),
            Vec::new(),
        )
        .await
        .unwrap()
    })
});

fn key(text: &str) -> String {
    text.to_owned()
}

fn keys(identifiers: &[ironpumpkin_mods::identifier::Identifier]) -> Vec<String> {
    identifiers.iter().map(ToString::to_string).collect()
}

/// Scenario "Entries bind": a block registered from a `RegisterEvent` handler is in the frozen
/// tables.
#[test]
fn register_event_fires_once_per_registry_between_init_and_common_setup() {
    LazyLock::force(&SERVER);
    assert_eq!(
        *LOG.lock().unwrap(),
        [
            "init",
            "RegisterEvent minecraft:block",
            "RegisterEvent minecraft:entity_type",
            "RegisterEvent minecraft:item",
            "common_setup",
        ]
    );
    let tables = ironpumpkin_mods::pumpkin_data::dynamic::tables().unwrap();
    assert!(
        tables
            .blocks()
            .iter()
            .any(|b| b.name == "event-mod:event_block")
    );
    let block_id = registry::id_of(ContentKind::Block, "event-mod:event_block").unwrap();
    assert!(block_id >= BlockId::COUNT);
    let item = Item::from_registry_key("event-mod:event_item").unwrap();
    assert_eq!(
        Block::from_item_id(item.id).map(|block| block.name),
        Some("event-mod:event_block")
    );
    assert!(registry::contains(
        ContentKind::EntityType,
        "event-mod:event_golem"
    ));
}

/// Scenario "Entries bind" through the lookups: `contains`, `id_of` and `key_of` round trip for
/// vanilla and mod entries, and `keys` lists both.
#[test]
fn lookups_round_trip_vanilla_and_mod_entries() {
    LazyLock::force(&SERVER);
    for (kind, name) in [
        (ContentKind::Block, "minecraft:stone"),
        (ContentKind::Block, "event-mod:init_block"),
        (ContentKind::Item, "minecraft:iron_ingot"),
        (ContentKind::Item, "event-mod:event_item"),
        (ContentKind::EntityType, "minecraft:zombie"),
        (ContentKind::EntityType, "event-mod:event_golem"),
    ] {
        assert!(registry::contains(kind, name), "{kind} {name}");
        let id = registry::id_of(kind, name).unwrap();
        assert_eq!(registry::key_of(kind, id).unwrap().to_string(), name);
        assert!(
            keys(&registry::keys(kind)).contains(&key(name)),
            "{kind} {name}"
        );
    }
    // A name without a namespace is in `minecraft`.
    assert_eq!(
        registry::id_of(ContentKind::Item, "iron_ingot"),
        Some(Item::IRON_INGOT.id)
    );
    assert_eq!(
        registry::keys(ContentKind::Block).len(),
        usize::from(BlockId::COUNT) + 2
    );
}

/// An unknown entry, id or tag returns `None` or `false`.
#[test]
fn unknown_keys_return_none() {
    LazyLock::force(&SERVER);
    for kind in [
        ContentKind::Block,
        ContentKind::Item,
        ContentKind::EntityType,
    ] {
        assert!(!registry::contains(kind, "event-mod:missing"));
        assert_eq!(registry::id_of(kind, "event-mod:missing"), None);
        assert_eq!(registry::key_of(kind, u16::MAX - 1), None);
        assert_eq!(registry::tag_entries(kind, "event-mod:missing"), None);
        assert!(!registry::is_in_tag(kind, "event-mod:missing", "c:ingots"));
    }
    assert_eq!(
        registry::key_of(ContentKind::EntityType, EntityType::COUNT + 1),
        None
    );
}

/// Scenario "Conventional tag", and the generated and mod tags of mod entries.
#[test]
fn tag_lookups_answer_for_generated_and_mod_tags() {
    LazyLock::force(&SERVER);
    assert!(registry::is_in_tag(
        ContentKind::Item,
        "minecraft:iron_ingot",
        "c:ingots"
    ));
    assert!(!registry::is_in_tag(
        ContentKind::Item,
        "minecraft:stick",
        "c:ingots"
    ));
    let ingots = keys(&registry::tag_entries(ContentKind::Item, "c:ingots").unwrap());
    assert!(ingots.contains(&key("minecraft:iron_ingot")), "{ingots:?}");
    // A mod block joins the generated tags of its display block, here stone.
    assert!(registry::is_in_tag(
        ContentKind::Block,
        "event-mod:init_block",
        "minecraft:mineable/pickaxe"
    ));
    assert_eq!(
        keys(&registry::tag_entries(ContentKind::Block, "#event-mod:lamps").unwrap()),
        ["minecraft:redstone_lamp", "event-mod:init_block"]
    );
    assert!(registry::is_in_tag(
        ContentKind::Block,
        "event-mod:init_block",
        "event-mod:lamps"
    ));
    assert!(!registry::is_in_tag(
        ContentKind::Block,
        "event-mod:event_block",
        "event-mod:lamps"
    ));
}
