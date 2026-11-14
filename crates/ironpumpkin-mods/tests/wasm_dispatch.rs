//! Task 2.18 of `openspec/changes/neoforge-shaped-plugin-api`: v0.2 Wasm plugins and native mods
//! share the one dispatch of `PluginManager::fire_dyn` for every event of the table "Mutators and
//! their events" of `design.md`, and the scenarios "One dispatch for native and Wasm handlers" and
//! "One event pipeline" of the spec `neoforge-event-bus`.
//!
//! The two plugins are real v0.2 guests of `crates/ironpumpkin-test-guest` (`dispatch-guest` and
//! `dispatch-peer`), loaded by the v0.2 host next to the native mods of this file. The test builds
//! them itself, with a nested `cargo build` into `target/tmp/wasm-dispatch/target`, so it needs
//! the `wasm32-wasip1` standard library and a host cargo. Android runs no host cargo, so the file
//! is empty there.
//!
//! ```text
//! rustup target add wasm32-wasip1
//! cargo test -p ironpumpkin-mods --test wasm_dispatch
//! ```
//!
//! Each event has four blocking handlers that receive cancelled events (native ones with
//! receive-canceled, Wasm ones always, see `Context::register_event`): the mod `dispatch` at `High` (N1),
//! `dispatch-guest` at `High` (W1, registered after the mod) and at `Normal` (W2), and the mod
//! `dispatch` at `Low` (N2). Each handler logs the field of the event that the test changes and
//! the cancel flag, and then runs the action that the test gave it. `dispatch-peer` registers the
//! same handlers after `dispatch-guest` and only logs, except in the scenario of the v0.2 world
//! call.
#![cfg(not(target_os = "android"))]
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

use std::{
    any::Any,
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError},
};

use ironpumpkin_mods::{ModInit, NativeMod, register_mod};
use pumpkin_core::{
    block::entities::{BlockEntity, barrel::BarrelBlockEntity},
    entity::{
        Entity, EntityBase, item::ItemEntity, living::LivingEntity, player::Player,
        projectile::owner::ProjectileOwner, r#type::from_type,
    },
    item::items::bow::BowItem,
    plugin::{
        BoxFuture, EventHandler, EventPriority, Payload,
        block::{block_break::BlockBreakEvent, block_place::BlockPlaceEvent},
        entity::{
            creature_spawn::{CreatureSpawnEvent, CreatureSpawnReason},
            entity_damage::EntityDamageEvent,
            entity_damage_by_block::EntityDamageByBlockEvent,
            entity_damage_by_entity::EntityDamageByEntityEvent,
            entity_death::{EntityDeathEvent, PlayerDeathEvent},
            entity_explode::EntityExplodeEvent,
            entity_potion_effect::EntityPotionEffectEvent,
            entity_regain_health::EntityRegainHealthEvent,
            entity_spawn::EntitySpawnEvent,
            item_spawn::ItemSpawnEvent,
        },
        level::{
            BlockEventEntityPlaceEvent, EntityInvulnerabilityCheckEvent, EntityLeaveLevelEvent,
            ExplosionEventDetonate, ExplosionKnockbackEvent,
        },
        living::{
            LivingDamageEventPost, LivingDamageEventPre, LivingEntityUseItemEventStart,
            LivingEntityUseItemEventStop, LivingExperienceDropEvent, LivingKnockBackEvent,
            LivingShieldBlockEvent, MobEffectEventApplicable, MobEffectEventApplicableResult,
        },
        player::ArrowLooseEvent,
        startup::STARTUP_HOOK,
        world::lightning_strike::LightningStrikeEvent,
    },
    server::Server,
    world::ExplosionInteraction,
};
use pumpkin_data::{
    Block, BlockStateId, damage::DamageType, effect::StatusEffect, entity::EntityType, item::Item,
    item_stack::ItemStack, potion::Effect,
};
use pumpkin_inventory::inventory::Inventory;
use pumpkin_util::{
    Hand,
    math::{position::BlockPos, vector3::Vector3},
    permission::PermissionLvl,
};
use pumpkin_wasm_host::WasmPluginLoader;
use pumpkin_world::world::BlockFlags;

const GUEST: &str = "dispatch-guest";
const PEER: &str = "dispatch-peer";

/// A field that the test reads and sets as text, the same text as in the guest.
trait Field {
    fn show(&self) -> String;
    fn assign(&mut self, value: &str);
}

macro_rules! parsed_field {
    ($($ty:ty),*) => {$(
        impl Field for $ty {
            fn show(&self) -> String {
                self.to_string()
            }

            fn assign(&mut self, value: &str) {
                *self = value.parse().unwrap();
            }
        }
    )*};
}

parsed_field!(bool, u8, u32, i32, f32);

/// The affected blocks of an explosion: the length, and a truncation.
impl Field for Vec<BlockPos> {
    fn show(&self) -> String {
        self.len().to_string()
    }

    fn assign(&mut self, value: &str) {
        self.truncate(value.parse().unwrap());
    }
}

/// A knockback: the vertical part, and a push straight up.
impl Field for Vector3<f64> {
    fn show(&self) -> String {
        self.y.to_string()
    }

    fn assign(&mut self, value: &str) {
        *self = Self::new(0.0, value.parse().unwrap(), 0.0);
    }
}

impl Field for MobEffectEventApplicableResult {
    fn show(&self) -> String {
        match self {
            Self::Apply => "apply",
            Self::Default => "default",
            Self::DoNotApply => "do-not-apply",
        }
        .to_owned()
    }

    fn assign(&mut self, value: &str) {
        *self = match value {
            "apply" => Self::Apply,
            "do-not-apply" => Self::DoNotApply,
            _ => Self::Default,
        };
    }
}

/// What one native handler does with one event: on its fire number `nth` only, or on every fire.
#[derive(Default)]
struct Action {
    nth: Option<u32>,
    value: Option<String>,
    cancel: Option<bool>,
}

impl Action {
    /// Parses the settings of the IPC message `on` of the guest: `nth=2 value=3 cancel=true`.
    fn parse(settings: &str) -> Self {
        let mut action = Self::default();
        for setting in settings.split_whitespace() {
            let (name, value) = setting.split_once('=').unwrap();
            match name {
                "nth" => action.nth = Some(value.parse().unwrap()),
                "value" => action.value = Some(value.to_owned()),
                _ => {
                    assert_eq!(name, "cancel", "not a setting: {setting}");
                    action.cancel = Some(value.parse().unwrap());
                }
            }
        }
        action
    }
}

#[derive(Default)]
struct Native {
    actions: HashMap<(&'static str, u32), Action>,
    fires: HashMap<(&'static str, u32), u32>,
    log: Vec<String>,
    modb: Vec<String>,
}

static NATIVE: LazyLock<Mutex<Native>> = LazyLock::new(Mutex::default);

fn native() -> MutexGuard<'static, Native> {
    NATIVE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Logs what native handler `slot` sees, then runs its action.
fn record(
    name: &'static str,
    slot: u32,
    mut field: Option<&mut dyn Field>,
    mut cancelled: Option<&mut bool>,
) {
    let mut native = native();
    let fire = native.fires.entry((name, slot)).or_default();
    *fire += 1;
    let fire = *fire;
    let entry = format!(
        "{name} N{} {} {}",
        slot + 1,
        field
            .as_ref()
            .map_or_else(|| "-".to_owned(), |field| field.show()),
        cancelled
            .as_ref()
            .map_or_else(|| "-".to_owned(), ToString::to_string),
    );
    native.log.push(entry);
    let Some(action) = native.actions.get(&(name, slot)) else {
        return;
    };
    if action.nth.is_some_and(|nth| nth != fire) {
        return;
    }
    if let (Some(value), Some(field)) = (&action.value, field.as_mut()) {
        field.assign(value);
    }
    if let (Some(cancel), Some(cancelled)) = (action.cancel, cancelled.as_mut()) {
        **cancelled = cancel;
    }
}

type View<E> = for<'a> fn(&'a mut E) -> (Option<&'a mut dyn Field>, Option<&'a mut bool>);

/// A native handler of the mod `dispatch`: N1 for slot 0, N2 for slot 1.
struct Probe<E> {
    name: &'static str,
    slot: u32,
    view: View<E>,
}

impl<E: Payload + Send + Sync + 'static> EventHandler<E> for Probe<E> {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut E,
    ) -> BoxFuture<'a, ()> {
        let (field, cancelled) = (self.view)(event);
        record(self.name, self.slot, field, cancelled);
        Box::pin(async {})
    }
}

/// Registers N1 and N2 for each event, with the field that the test changes and the cancel flag.
macro_rules! probes {
    ($cx:ident; $($event:ident => |$e:ident| ($field:expr, $cancel:expr)),* $(,)?) => {$(
        for (slot, priority) in [(0, EventPriority::High), (1, EventPriority::Low)] {
            let view: View<$event> = |$e| ($field, $cancel);
            $cx.register_event::<$event, _>(
                Arc::new(Probe { name: stringify!($event), slot, view }),
                priority,
                true,
                true,
            );
        }
    )*};
}

/// The native mod of the four-handler chain.
struct DispatchMod;

impl NativeMod for DispatchMod {
    fn id(&self) -> &'static str {
        "dispatch"
    }

    fn display_name(&self) -> &'static str {
        "Dispatch"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        probes!(cx;
            BlockBreakEvent => |e| (Some(&mut e.exp), Some(&mut e.cancelled)),
            BlockPlaceEvent => |e| (None, Some(&mut e.cancelled)),
            BlockEventEntityPlaceEvent => |e| (None, Some(&mut e.cancelled)),
            EntitySpawnEvent => |e| (None, Some(&mut e.cancelled)),
            CreatureSpawnEvent => |e| (None, Some(&mut e.cancelled)),
            ItemSpawnEvent => |e| (None, Some(&mut e.cancelled)),
            LightningStrikeEvent => |e| (None, Some(&mut e.cancelled)),
            EntityExplodeEvent => |e| (Some(&mut e.yield_rate), Some(&mut e.cancelled)),
            ExplosionEventDetonate => |e| (Some(&mut e.affected_blocks), None),
            ExplosionKnockbackEvent => |e| (Some(&mut e.knockback), None),
            EntityInvulnerabilityCheckEvent => |e| (Some(&mut e.invulnerable), None),
            EntityDamageEvent => |e| (Some(&mut e.damage), Some(&mut e.cancelled)),
            EntityDamageByEntityEvent => |e| (Some(&mut e.damage), Some(&mut e.cancelled)),
            EntityDamageByBlockEvent => |e| (Some(&mut e.damage), Some(&mut e.cancelled)),
            LivingShieldBlockEvent => |e| (Some(&mut e.blocked_damage), None),
            LivingKnockBackEvent => |e| (Some(&mut e.strength), Some(&mut e.cancelled)),
            LivingDamageEventPre => |e| (Some(&mut e.new_damage), None),
            LivingDamageEventPost => |_e| (None, None),
            EntityDeathEvent => |e| (Some(&mut e.dropped_exp), None),
            PlayerDeathEvent => |e| (Some(&mut e.dropped_exp), Some(&mut e.cancelled)),
            LivingExperienceDropEvent => |e| (
                Some(&mut e.dropped_experience),
                Some(&mut e.cancelled)
            ),
            EntityRegainHealthEvent => |e| (Some(&mut e.amount), Some(&mut e.cancelled)),
            EntityLeaveLevelEvent => |_e| (None, None),
            LivingEntityUseItemEventStart => |e| (Some(&mut e.duration), Some(&mut e.cancelled)),
            LivingEntityUseItemEventStop => |e| (None, Some(&mut e.cancelled)),
            ArrowLooseEvent => |e| (Some(&mut e.charge), Some(&mut e.cancelled)),
            MobEffectEventApplicable => |e| (Some(&mut e.result), None),
            EntityPotionEffectEvent => |e| (Some(&mut e.amplifier), Some(&mut e.cancelled)),
        );
    }
}

/// Mod B of the scenarios: it watches breaks and spawns last, at `Lowest`.
struct ModB;

impl NativeMod for ModB {
    fn id(&self) -> &'static str {
        "modb"
    }

    fn display_name(&self) -> &'static str {
        "Mod B"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        cx.register_event::<BlockBreakEvent, _>(Arc::new(Watch), EventPriority::Lowest, true, true);
        cx.register_event::<EntitySpawnEvent, _>(
            Arc::new(Watch),
            EventPriority::Lowest,
            true,
            true,
        );
    }
}

register_mod!(DispatchMod);
register_mod!(ModB);

struct Watch;

impl EventHandler<BlockBreakEvent> for Watch {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut BlockBreakEvent,
    ) -> BoxFuture<'a, ()> {
        let player = event
            .player
            .as_ref()
            .map_or("none", |player| player.gameprofile.name.as_str());
        native()
            .modb
            .push(format!("break by {player}, cancelled {}", event.cancelled));
        Box::pin(async {})
    }
}

impl EventHandler<EntitySpawnEvent> for Watch {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut EntitySpawnEvent,
    ) -> BoxFuture<'a, ()> {
        let joined = event.world.get_entity_by_id(event.entity_id).is_some();
        native().modb.push(format!(
            "spawn of {}, joined {joined}, cancelled {}",
            event.entity_id, event.cancelled
        ));
        Box::pin(async {})
    }
}

/// Builds both guest targets into `target`, and returns their release directory. The nested
/// build needs a target directory of its own: the outer `cargo test` holds the lock of the main
/// one until the tests end. Cargo makes the build a no-op when the guests are fresh.
fn build_guests(target: &Path) -> PathBuf {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(&cargo)
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .env("CARGO_TARGET_DIR", target)
        .args([
            "build",
            "--manifest-path",
            "crates/ironpumpkin-test-guest/Cargo.toml",
            "--target",
            "wasm32-wasip1",
            "--release",
            "--locked",
            "--lib",
            "--examples",
        ])
        .output();
    assert!(
        output.is_ok(),
        "{} does not start: {:?}",
        Path::new(&cargo).display(),
        output.as_ref().err()
    );
    let output = output.unwrap();
    assert!(
        output.status.success(),
        "the build of the v0.2 test guests failed. If `std` is missing for `wasm32-wasip1`, run \
         `rustup target add wasm32-wasip1`, or on Fedora install \
         `rust-std-static-wasm32-wasip1`:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    target.join("wasm32-wasip1/release")
}

/// The module `module` made a component in `dir` with the reactor adapter of the `wasmtime`
/// version in `Cargo.lock`.
fn component(dir: &Path, name: &str, module: &Path) -> PathBuf {
    let bytes = std::fs::read(module).unwrap();
    let component = wit_component::ComponentEncoder::default()
        .module(&bytes)
        .unwrap()
        .validate(true)
        .adapter(
            wasi_preview1_component_adapter_provider::WASI_SNAPSHOT_PREVIEW1_ADAPTER_NAME,
            wasi_preview1_component_adapter_provider::WASI_SNAPSHOT_PREVIEW1_REACTOR_ADAPTER,
        )
        .unwrap()
        .encode()
        .unwrap();
    let path = dir.join(format!("{name}.wasm"));
    std::fs::write(&path, component).unwrap();
    path
}

/// The server with both mods and both plugins, created once per process.
///
/// The guests, the components and the compiled code that the loader keeps in `cache` live in one
/// directory for all processes of this test, so only the first process builds and compiles them
/// (about 6 s for each component in a debug build). The lock keeps a process from reading a file
/// that another process writes.
static SETUP: LazyLock<()> = LazyLock::new(|| {
    STARTUP_HOOK.set(ironpumpkin_mods::init_mods).unwrap();
    let server = support::server();
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("wasm-dispatch");
    std::fs::create_dir_all(&dir).unwrap();
    let lock = File::create(dir.join("load.lock")).unwrap();
    lock.lock().unwrap();
    let release = build_guests(&dir.join("target"));
    let guest = component(&dir, GUEST, &release.join("ironpumpkin_test_guest.wasm"));
    let peer = component(&dir, PEER, &release.join("examples/dispatch_peer.wasm"));
    support::RUNTIME.block_on(async {
        server
            .plugin_manager
            .add_loader(server, Arc::new(WasmPluginLoader::new(false)))
            .await;
        for path in [guest, peer] {
            server
                .plugin_manager
                .try_load_plugin(server, &path)
                .await
                .unwrap();
        }
    });
    drop(lock);
    assert!(server.plugin_manager.is_plugin_active(GUEST));
    assert!(server.plugin_manager.is_plugin_active(PEER));
});

/// Takes the lock of the shared world, and starts the first run.
fn setup() -> MutexGuard<'static, ()> {
    LazyLock::force(&SETUP);
    let serial = support::serial();
    next_run();
    serial
}

fn ipc(plugin: &str, message: &str) -> String {
    let reply = support::RUNTIME
        .block_on(support::server().plugin_manager.send_message(
            "wasm-dispatch-test",
            plugin,
            message.as_bytes(),
        ))
        .unwrap()
        .unwrap();
    String::from_utf8(reply).unwrap()
}

/// Gives handler `handler` (N1, W1, W2, N2, or P1 and P2 of `dispatch-peer`) its action on
/// `event`: `nth=<n>`, `value=<v>`, `cancel=<bool>`.
fn act(handler: &str, event: &'static str, settings: &str) {
    match handler {
        "N1" | "N2" => {
            let slot = u32::from(handler == "N2");
            native()
                .actions
                .insert((event, slot), Action::parse(settings));
        }
        _ => {
            let (plugin, slot) = match handler {
                "W1" => (GUEST, 1),
                "W2" => (GUEST, 2),
                "P1" => (PEER, 1),
                _ => (PEER, 2),
            };
            ipc(plugin, &format!("on {event} {slot} {settings}"));
        }
    }
}

/// The entries of `event` in the log of `plugin`, without the event name, and the log taken.
fn guest_log(plugin: &str, event: &str) -> Vec<String> {
    let log = ipc(plugin, "log");
    assert!(
        !log.lines().any(|line| line.starts_with("error")),
        "{plugin}: {log}"
    );
    log.lines()
        .filter_map(|line| line.strip_prefix(event)?.strip_prefix(' '))
        .map(str::to_owned)
        .collect()
}

/// What the four handlers saw of `event`, in the order of the dispatch: for each fire, the
/// entries of N1, W1, W2 and N2.
fn seen(event: &str) -> Vec<String> {
    let native: Vec<String> = std::mem::take(&mut native().log)
        .into_iter()
        .filter_map(|line| Some(line.strip_prefix(event)?.strip_prefix(' ')?.to_owned()))
        .collect();
    let guest = guest_log(GUEST, event);
    let slot = |entries: &[String], slot: &str| -> Vec<String> {
        entries
            .iter()
            .filter(|entry| entry.starts_with(slot))
            .cloned()
            .collect()
    };
    let columns = [
        slot(&native, "N1"),
        slot(&guest, "W1"),
        slot(&guest, "W2"),
        slot(&native, "N2"),
    ];
    assert!(
        columns
            .iter()
            .all(|column| column.len() == columns[0].len()),
        "{event}: each handler sees each fire: {columns:?}"
    );
    (0..columns[0].len())
        .flat_map(|fire| columns.iter().map(move |column| column[fire].clone()))
        .collect()
}

/// Asserts that every fire of `event` matches `pattern`, the four entries of one fire, where `*`
/// matches any value.
fn assert_fires(event: &str, pattern: &[String; 4]) {
    let seen = seen(event);
    assert!(!seen.is_empty(), "{event} did not fire");
    let matches = |entry: &str, expected: &str| {
        let (entry, expected): (Vec<&str>, Vec<&str>) =
            (entry.split(' ').collect(), expected.split(' ').collect());
        entry.len() == expected.len()
            && entry
                .iter()
                .zip(&expected)
                .all(|(word, expected)| *expected == "*" || word == expected)
    };
    for fire in seen.chunks(4) {
        assert!(
            fire.iter()
                .zip(pattern)
                .all(|(entry, expected)| matches(entry, expected)),
            "{event}: {seen:?} does not match {pattern:?}"
        );
    }
}

fn fire(n1: &str, w1: &str, w2: &str, n2: &str) -> [String; 4] {
    [
        format!("N1 {n1}"),
        format!("W1 {w1}"),
        format!("W2 {w2}"),
        format!("N2 {n2}"),
    ]
}

/// Each handler sets the field of `event` to the next of `values`: N1 the first, W1 the second
/// and W2 the third. Each handler sees the value of the handler before it, so the chain proves
/// the order N1, W1, W2, N2 and the write-back from native to Wasm and from Wasm to native.
fn chain<T>(
    event: &'static str,
    values: [&str; 3],
    cancellable: bool,
    trigger: impl FnOnce() -> T,
) -> T {
    act("N1", event, &format!("value={}", values[0]));
    act("W1", event, &format!("value={}", values[1]));
    act("W2", event, &format!("value={}", values[2]));
    let result = trigger();
    let cancelled = if cancellable { "false" } else { "-" };
    assert_fires(
        event,
        &fire(
            &format!("* {cancelled}"),
            &format!("{} {cancelled}", values[0]),
            &format!("{} {cancelled}", values[1]),
            &format!("{} {cancelled}", values[2]),
        ),
    );
    result
}

/// The chain of [`chain`] on the cancel flag of an event without another settable field: N1
/// cancels, W1 un-cancels, W2 cancels and N2 un-cancels, so the action runs.
fn cancel_chain<T>(event: &'static str, trigger: impl FnOnce() -> T) -> T {
    act("N1", event, "cancel=true");
    act("W1", event, "cancel=false");
    act("W2", event, "cancel=true");
    act("N2", event, "cancel=false");
    let result = trigger();
    assert_fires(event, &fire("* false", "* true", "* false", "* true"));
    result
}

/// W1 cancels `event`: W2 and N2 get it cancelled.
fn wasm_cancels<T>(event: &'static str, trigger: impl FnOnce() -> T) -> T {
    act("W1", event, "cancel=true");
    let result = trigger();
    assert_fires(event, &fire("* false", "* false", "* true", "* true"));
    result
}

/// N1 cancels `event`: W1, W2 and N2 get it cancelled.
fn native_cancels<T>(event: &'static str, trigger: impl FnOnce() -> T) -> T {
    act("N1", event, "cancel=true");
    let result = trigger();
    assert_fires(event, &fire("* false", "* true", "* true", "* true"));
    result
}

/// An event without a settable field: all four handlers get each fire.
fn observe<T>(event: &'static str, trigger: impl FnOnce() -> T) -> T {
    let result = trigger();
    assert_fires(event, &fire("- -", "- -", "- -", "- -"));
    result
}

/// Clears the actions and the logs of all handlers, before a test and between two runs of one.
fn next_run() {
    *native() = Native::default();
    ipc(GUEST, "reset");
    ipc(PEER, "reset");
}

/// A position of the test `slot`, in a chunk of its own.
const fn spot(slot: i32) -> BlockPos {
    BlockPos::new(slot * 16 + 8, 120, 8)
}

fn load(pos: BlockPos) {
    let world = support::world();
    let chunk = pos.chunk_position();
    {
        let mut loading = world.level.chunk_loading.lock().unwrap();
        loading.add_force_ticket(chunk);
        loading.send_change();
    };
    let deadline = std::time::Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&chunk) {
        assert!(
            std::time::Instant::now() < deadline,
            "the chunk of {pos} did not load"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn set(pos: BlockPos, block: &Block) {
    support::world().set_block_state(&pos, block.default_state.id, BlockFlags::empty());
}

fn state_at(pos: BlockPos) -> BlockStateId {
    support::world().get_block_state_id(&pos)
}

/// A player at `x`, with the permission that ignores the spawn protection.
fn player_at(name: &str, x: f64) -> Arc<Player> {
    let player = support::player(name);
    player.permission_lvl.store(PermissionLvl::Four);
    player
        .living_entity
        .entity
        .set_pos(Vector3::new(x, 100.0, 0.5));
    player
}

fn create(entity_type: &'static EntityType, x: f64) -> Arc<dyn EntityBase> {
    from_type(
        entity_type,
        Vector3::new(x, 120.0, 0.5),
        &support::world(),
        uuid::Uuid::new_v4(),
    )
}

/// Spawns a mob of `entity_type` at `x`, in the air and away from the other tests.
fn spawn(entity_type: &'static EntityType, x: f64) -> Arc<dyn EntityBase> {
    let entity = create(entity_type, x);
    assert!(support::world().spawn_entity(entity.clone()));
    entity
}

fn living(entity: &Arc<dyn EntityBase>) -> &LivingEntity {
    entity.get_living_entity().unwrap()
}

fn health(entity: &Arc<dyn EntityBase>) -> f32 {
    living(entity).health.load()
}

fn is_live(entity: &Arc<dyn EntityBase>) -> bool {
    support::world()
        .get_entity_by_uuid(entity.get_entity().entity_uuid)
        .is_some()
}

fn count_near(entity_type: &EntityType, pos: Vector3<f64>) -> usize {
    support::world()
        .entities
        .load()
        .iter()
        .filter(|entity| {
            let entity = entity.get_entity();
            entity.entity_type == entity_type
                && entity.pos.load().squared_distance_to_vec(&pos) < 4.0
        })
        .count()
}

/// The table row `World::break_block`, and the first scenario of "One dispatch for native and
/// Wasm handlers": a native handler at `High` cancels, and the Wasm handler at `Normal` gets the
/// event cancelled.
#[test]
fn block_break_event_of_break_block() {
    let _serial = setup();
    let pos = spot(2);
    load(pos);
    let player = player_at("Breaker", 40.5);
    let break_stone = || {
        set(pos, &Block::STONE);
        support::world().break_block(&pos, Some(&player), BlockFlags::NOTIFY_ALL);
        state_at(pos) != Block::STONE.default_state.id
    };

    assert!(chain("BlockBreakEvent", ["3", "4", "5"], true, break_stone));
    next_run();
    assert!(
        !wasm_cancels("BlockBreakEvent", break_stone),
        "the block stays"
    );
    next_run();
    assert!(
        !native_cancels("BlockBreakEvent", break_stone),
        "the block stays"
    );
}

/// The projectile of a mod: a mining laser with its shooter as the owner.
struct MiningLaser {
    entity: Entity,
    owner: ProjectileOwner,
}

impl EntityBase for MiningLaser {
    fn get_entity(&self) -> &Entity {
        &self.entity
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }

    fn cast_any(&self) -> &dyn Any {
        self
    }

    fn get_projectile_owner(&self) -> Option<&ProjectileOwner> {
        Some(&self.owner)
    }
}

/// The table row `World::destroy_block`, and the scenario "A projectile of a mod breaks a block":
/// the laser of mod A calls `destroy_block` with itself as the breaker, a v0.2 plugin cancels, and
/// the handler of mod B with receive-canceled gets the event cancelled, with the shooter.
#[test]
fn block_break_event_of_destroy_block_by_a_projectile() {
    let _serial = setup();
    let pos = spot(3);
    load(pos);
    let shooter = player_at("Shooter", 56.5);
    let world = support::world();
    let laser = MiningLaser {
        entity: Entity::new(world.clone(), shooter.position(), &EntityType::SNOWBALL),
        owner: ProjectileOwner::new(Some(&shooter.living_entity.entity)),
    };
    let hit = || {
        set(pos, &Block::STONE);
        world.destroy_block(&pos, true, Some(&laser))
    };

    assert!(!wasm_cancels("BlockBreakEvent", hit), "the block stays");
    assert_eq!(state_at(pos), Block::STONE.default_state.id);
    assert_eq!(native().modb, ["break by Shooter, cancelled true"]);
    next_run();
    assert!(!native_cancels("BlockBreakEvent", hit), "the block stays");
    next_run();
    assert!(chain("BlockBreakEvent", ["3", "4", "5"], true, hit));
    assert_eq!(state_at(pos), Block::AIR.default_state.id);
}

/// The table row `World::place_block` with a player: `BlockPlaceEvent`.
#[test]
fn block_place_event_of_place_block() {
    let _serial = setup();
    let pos = spot(4);
    load(pos);
    let player = player_at("Placer", 72.5);
    let place = || {
        set(pos, &Block::AIR);
        support::world().place_block(&pos, Block::STONE.default_state.id, player.as_ref())
            && state_at(pos) == Block::STONE.default_state.id
    };

    assert!(cancel_chain("BlockPlaceEvent", place));
    next_run();
    assert!(!wasm_cancels("BlockPlaceEvent", place));
    next_run();
    assert!(!native_cancels("BlockPlaceEvent", place));
}

/// The table row `World::place_block` with an entity that is not a player:
/// `BlockEventEntityPlaceEvent`.
#[test]
fn block_event_entity_place_event_of_place_block() {
    let _serial = setup();
    let pos = spot(5);
    load(pos);
    let pig = spawn(&EntityType::PIG, 88.5);
    let place = || {
        set(pos, &Block::AIR);
        support::world().place_block(&pos, Block::STONE.default_state.id, pig.as_ref())
            && state_at(pos) == Block::STONE.default_state.id
    };

    assert!(cancel_chain("BlockEventEntityPlaceEvent", place));
    next_run();
    assert!(!wasm_cancels("BlockEventEntityPlaceEvent", place));
    next_run();
    assert!(!native_cancels("BlockEventEntityPlaceEvent", place));
}

/// The table row `World::spawn_entity`, and the scenario "A plugin sees a mod spawn": the plugin
/// gets the event before the entity joins (mod B, which runs after the plugin, does not find the
/// entity yet), and its cancel returns `false` to the mod.
#[test]
fn entity_spawn_event_of_spawn_entity() {
    let _serial = setup();
    let spawn_pig = || {
        let pig = create(&EntityType::PIG, 1000.5);
        let spawned = support::world().spawn_entity(pig.clone());
        (spawned, is_live(&pig), pig.get_entity().entity_id)
    };

    let (spawned, live, id) = cancel_chain("EntitySpawnEvent", spawn_pig);
    assert!(spawned && live);
    assert_eq!(
        native().modb,
        [format!("spawn of {id}, joined false, cancelled false")]
    );
    next_run();
    let (spawned, live, _) = wasm_cancels("EntitySpawnEvent", spawn_pig);
    assert!(!spawned && !live, "the cancel returns false to the mod");
    next_run();
    let (spawned, live, _) = native_cancels("EntitySpawnEvent", spawn_pig);
    assert!(!spawned && !live);
}

/// The table row `World::spawn_entity_non_save`.
#[test]
fn entity_spawn_event_of_spawn_entity_non_save() {
    let _serial = setup();
    let spawn_pig = || {
        let pig = create(&EntityType::PIG, 1016.5);
        support::world().spawn_entity_non_save(pig.clone());
        is_live(&pig)
    };

    assert!(cancel_chain("EntitySpawnEvent", spawn_pig));
    next_run();
    assert!(!wasm_cancels("EntitySpawnEvent", spawn_pig));
    next_run();
    assert!(!native_cancels("EntitySpawnEvent", spawn_pig));
}

/// The table row `World::add_entity_silent`: the rider of a spawned mob fires its own
/// `EntitySpawnEvent`, the second fire, and a cancel keeps only the rider out.
#[test]
fn entity_spawn_event_of_a_rider() {
    let _serial = setup();
    let spawn_jockey = || {
        let spider = create(&EntityType::SPIDER, 1032.5);
        let rider = create(&EntityType::SKELETON, 1032.5);
        spider
            .get_mob()
            .unwrap()
            .get_mob_entity()
            .add_pending_rider(rider.clone());
        assert!(support::world().spawn_entity(spider.clone()));
        (is_live(&spider), is_live(&rider))
    };
    let first = fire("- false", "- false", "- false", "- false");

    act("W1", "EntitySpawnEvent", "nth=2 cancel=true");
    assert_eq!(spawn_jockey(), (true, false));
    let second = fire("- false", "- false", "- true", "- true");
    assert_eq!(seen("EntitySpawnEvent"), [first.clone(), second].concat());
    next_run();
    act("N1", "EntitySpawnEvent", "nth=2 cancel=true");
    assert_eq!(spawn_jockey(), (true, false));
    let second = fire("- false", "- true", "- true", "- true");
    assert_eq!(seen("EntitySpawnEvent"), [first, second].concat());
}

/// The table row `World::spawn_creature`: `CreatureSpawnEvent`, then `EntitySpawnEvent`.
#[test]
fn creature_spawn_event_of_spawn_creature() {
    let _serial = setup();
    let player = player_at("Breeder", 1048.5);
    let spawn_zombie = || {
        let zombie = create(&EntityType::ZOMBIE, 1048.5);
        let spawned = support::world().spawn_creature(
            zombie.clone(),
            CreatureSpawnReason::SpawnerEgg,
            Some(player.clone()),
        );
        spawned && is_live(&zombie)
    };

    assert!(cancel_chain("CreatureSpawnEvent", spawn_zombie));
    next_run();
    assert!(!wasm_cancels("CreatureSpawnEvent", spawn_zombie));
    next_run();
    assert!(!native_cancels("CreatureSpawnEvent", spawn_zombie));
}

/// The table row `World::drop_stack`: `ItemSpawnEvent`, then `EntitySpawnEvent`.
#[test]
fn item_spawn_event_of_drop_stack() {
    let _serial = setup();
    let pos = spot(6);
    load(pos);
    let center = pos.to_centered_f64();
    let drop_diamond = || {
        let before = count_near(&EntityType::ITEM, center);
        support::world().drop_stack(&pos, ItemStack::new(1, &Item::DIAMOND));
        count_near(&EntityType::ITEM, center) - before
    };

    assert_eq!(cancel_chain("ItemSpawnEvent", drop_diamond), 1);
    next_run();
    assert_eq!(wasm_cancels("ItemSpawnEvent", drop_diamond), 0);
    next_run();
    assert_eq!(native_cancels("ItemSpawnEvent", drop_diamond), 0);
}

/// The table row `World::scatter_stack`: `EntitySpawnEvent` for the stack.
#[test]
fn entity_spawn_event_of_scatter_stack() {
    let _serial = setup();
    let pos = spot(7);
    load(pos);
    let center = pos.to_centered_f64();
    let scatter = || {
        let before = count_near(&EntityType::ITEM, center);
        support::world().scatter_stack(
            center.x,
            center.y,
            center.z,
            ItemStack::new(1, &Item::STICK),
        );
        count_near(&EntityType::ITEM, center) - before
    };

    assert_eq!(cancel_chain("EntitySpawnEvent", scatter), 1);
    next_run();
    assert_eq!(wasm_cancels("EntitySpawnEvent", scatter), 0);
    next_run();
    assert_eq!(native_cancels("EntitySpawnEvent", scatter), 0);
}

/// The table row `World::strike_lightning`: `LightningStrikeEvent`, then `EntitySpawnEvent`.
#[test]
fn lightning_strike_event_of_strike_lightning() {
    let _serial = setup();
    let pos = Vector3::new(1064.5, 120.0, 0.5);
    let strike = || {
        let before = count_near(&EntityType::LIGHTNING_BOLT, pos);
        support::world().strike_lightning(pos, true);
        count_near(&EntityType::LIGHTNING_BOLT, pos) - before
    };

    assert_eq!(cancel_chain("LightningStrikeEvent", strike), 1);
    next_run();
    assert_eq!(wasm_cancels("LightningStrikeEvent", strike), 0);
    next_run();
    assert_eq!(native_cancels("LightningStrikeEvent", strike), 0);
}

/// Dirt at `pos` and air around it, for an explosion at its center.
fn dirt_at(pos: BlockPos) {
    load(pos);
    for dx in -2..=2 {
        for dy in -2..=2 {
            for dz in -2..=2 {
                set(pos.offset(Vector3::new(dx, dy, dz)), &Block::AIR);
            }
        }
    }
    set(pos, &Block::DIRT);
}

fn explode(pos: BlockPos) -> bool {
    dirt_at(pos);
    support::world().explode(pos.to_centered_f64(), 4.0, ExplosionInteraction::Tnt);
    state_at(pos) == Block::DIRT.default_state.id
}

/// The table row `World::explode`: `EntityExplodeEvent` (`ExplosionEvent.Start`). The fire site
/// reads only the cancel flag; the chain changes `yield_rate` to show the order.
#[test]
fn entity_explode_event_of_explode() {
    let _serial = setup();
    let pos = spot(8);

    assert!(!chain(
        "EntityExplodeEvent",
        ["1.5", "2.5", "3.5"],
        true,
        || explode(pos)
    ));
    next_run();
    assert!(
        wasm_cancels("EntityExplodeEvent", || explode(pos)),
        "the dirt stays"
    );
    next_run();
    assert!(
        native_cancels("EntityExplodeEvent", || explode(pos)),
        "the dirt stays"
    );
}

/// The table row `World::explode`: `ExplosionEventDetonate`. Five blocks of dirt are affected,
/// and the chain truncates them to 3, 2 and none, so the dirt stays.
#[test]
fn explosion_event_detonate_of_explode() {
    let _serial = setup();
    let pos = spot(9);
    let column = [
        pos,
        pos.up(),
        pos.down(),
        pos.offset(Vector3::new(1, 0, 0)),
        pos.offset(Vector3::new(-1, 0, 0)),
    ];

    chain("ExplosionEventDetonate", ["3", "2", "0"], false, || {
        dirt_at(pos);
        for dirt in column {
            set(dirt, &Block::DIRT);
        }
        support::world().explode(pos.to_centered_f64(), 4.0, ExplosionInteraction::Tnt);
    });
    assert!(
        column
            .iter()
            .all(|dirt| state_at(*dirt) == Block::DIRT.default_state.id)
    );
}

/// The table row `World::explode`: `ExplosionKnockbackEvent` for an item next to the explosion,
/// which takes the knockback of the last handler.
#[test]
fn explosion_knockback_event_of_explode() {
    let _serial = setup();
    let pos = spot(10);
    dirt_at(pos);
    let world = support::world();
    let item = Arc::new(ItemEntity::new(
        Entity::new(
            world.clone(),
            pos.up().offset(Vector3::new(1, 0, 0)).to_centered_f64(),
            &EntityType::ITEM,
        ),
        ItemStack::new(1, &Item::STICK),
    ));
    world.spawn_entity(item.clone());
    next_run();

    chain("ExplosionKnockbackEvent", ["5", "6", "7"], false, || {
        world.explode(pos.up().to_centered_f64(), 4.0, ExplosionInteraction::Tnt);
    });
    let velocity = item.get_entity().velocity.load();
    assert!(velocity.y >= 7.0 && velocity.x < 1.0, "{velocity:?}");
}

/// The table row `EntityBase::damage`: `EntityInvulnerabilityCheckEvent`. W2 leaves the pig
/// invulnerable, so it takes no damage.
#[test]
fn entity_invulnerability_check_event_of_damage() {
    let _serial = setup();
    let pig = spawn(&EntityType::PIG, 1080.5);
    let health = health(&pig);

    // A bool cannot give three handlers three distinct values, so this chain does not prove the
    // order; the rows with numeric or enum fields do.
    chain(
        "EntityInvulnerabilityCheckEvent",
        ["true", "false", "true"],
        false,
        || pig.damage(pig.as_ref(), 2.0, DamageType::GENERIC),
    );
    assert_eq!(self::health(&pig), health);
}

/// Damages a fresh pig at `x` by 5 with `hit`, and returns the damage it took.
fn damage_taken(x: f64, hit: impl FnOnce(&Arc<dyn EntityBase>)) -> f32 {
    let pig = spawn(&EntityType::PIG, x);
    let before = health(&pig);
    hit(&pig);
    before - health(&pig)
}

fn generic(pig: &Arc<dyn EntityBase>) {
    pig.damage(pig.as_ref(), 5.0, DamageType::GENERIC);
}

/// The table row `EntityBase::damage`: `EntityDamageEvent` (`NeoForge`
/// `LivingIncomingDamageEvent`), and the second scenario of "One dispatch for native and Wasm
/// handlers": the Wasm handler at `High` cancels, and the native handler at `Low` gets the event
/// cancelled, and the entity takes no damage.
#[test]
fn entity_damage_event_of_damage() {
    let _serial = setup();

    let taken = chain("EntityDamageEvent", ["4", "3", "2"], true, || {
        damage_taken(1096.5, generic)
    });
    assert_eq!(taken, 2.0);
    next_run();
    assert_eq!(
        wasm_cancels("EntityDamageEvent", || damage_taken(1112.5, generic)),
        0.0
    );
    next_run();
    assert_eq!(
        native_cancels("EntityDamageEvent", || damage_taken(1128.5, generic)),
        0.0
    );
}

/// The table row `EntityBase::kill`: the damage events of the kill damage.
#[test]
fn entity_damage_event_of_kill() {
    let _serial = setup();
    let kill = |x: f64| {
        let pig = spawn(&EntityType::PIG, x);
        pig.kill(pig.as_ref());
        health(&pig) > 0.0
    };

    assert!(
        wasm_cancels("EntityDamageEvent", || kill(1144.5)),
        "the pig lives"
    );
    next_run();
    assert!(
        native_cancels("EntityDamageEvent", || kill(1160.5)),
        "the pig lives"
    );
    next_run();
    assert!(!kill(1176.5));
}

/// The table row `EntityBase::damage` with a source entity: `EntityDamageByEntityEvent`.
#[test]
fn entity_damage_by_entity_event_of_damage() {
    let _serial = setup();
    let attacker = player_at("Attacker", 1192.5);
    let by_attacker = |pig: &Arc<dyn EntityBase>| {
        pig.damage_with_context(
            pig.as_ref(),
            5.0,
            DamageType::GENERIC,
            None,
            Some(attacker.as_ref()),
            Some(attacker.as_ref()),
        );
    };

    let taken = chain("EntityDamageByEntityEvent", ["4", "3", "2"], true, || {
        damage_taken(1192.5, by_attacker)
    });
    assert_eq!(taken, 2.0);
    next_run();
    let taken = wasm_cancels("EntityDamageByEntityEvent", || {
        damage_taken(1208.5, by_attacker)
    });
    assert_eq!(taken, 0.0);
    next_run();
    let taken = native_cancels("EntityDamageByEntityEvent", || {
        damage_taken(1224.5, by_attacker)
    });
    assert_eq!(taken, 0.0);
}

/// The table row `EntityBase::damage` with a block position: `EntityDamageByBlockEvent`.
#[test]
fn entity_damage_by_block_event_of_damage() {
    let _serial = setup();
    let by_block = |pig: &Arc<dyn EntityBase>| {
        let at = pig.get_entity().pos.load();
        pig.damage_with_context(pig.as_ref(), 5.0, DamageType::GENERIC, Some(at), None, None);
    };

    let taken = chain("EntityDamageByBlockEvent", ["4", "3", "2"], true, || {
        damage_taken(1240.5, by_block)
    });
    assert_eq!(taken, 2.0);
    next_run();
    assert_eq!(
        wasm_cancels("EntityDamageByBlockEvent", || damage_taken(
            1256.5, by_block
        )),
        0.0
    );
    next_run();
    assert_eq!(
        native_cancels("EntityDamageByBlockEvent", || damage_taken(
            1272.5, by_block
        )),
        0.0
    );
}

/// The table row `EntityBase::damage` of a player with a raised shield: `LivingShieldBlockEvent`.
/// The shield blocks 1 of the 4 damage that W2 leaves.
#[test]
fn living_shield_block_event_of_damage() {
    let _serial = setup();
    let player = player_at("Blocker", 1288.5);
    let shield = ItemStack::new(1, &Item::SHIELD);
    player.living_entity.set_active_hand(
        Hand::Right,
        shield.clone(),
        shield.get_max_use_time() - 10,
    );
    let health = player.living_entity.health.load();
    next_run();

    chain("LivingShieldBlockEvent", ["3", "2", "1"], false, || {
        // In front of the player, which looks along +z.
        player.living_entity.damage_with_context(
            player.as_ref(),
            4.0,
            DamageType::MOB_ATTACK,
            Some(Vector3::new(1288.5, 100.0, 3.5)),
            None,
            None,
        )
    });
    assert_eq!(player.living_entity.health.load(), health - 3.0);
}

fn horizontal_speed(entity: &Arc<dyn EntityBase>) -> f64 {
    let velocity = entity.get_entity().velocity.load();
    velocity.x.hypot(velocity.z)
}

/// The table row `EntityBase::damage` by an attacker: `LivingKnockBackEvent`.
#[test]
fn living_knock_back_event_of_damage() {
    let _serial = setup();
    let attacker = player_at("Shover", 1304.5);
    let knock = |x: f64| {
        let pig = spawn(&EntityType::PIG, x);
        pig.damage_with_context(
            pig.as_ref(),
            1.0,
            DamageType::PLAYER_ATTACK,
            None,
            Some(attacker.as_ref()),
            Some(attacker.as_ref()),
        );
        horizontal_speed(&pig)
    };

    let speed = chain(
        "LivingKnockBackEvent",
        ["0.25", "0.5", "0.75"],
        true,
        || knock(1306.5),
    );
    assert!((speed - 0.75).abs() < 1e-6, "{speed}");
    next_run();
    assert_eq!(wasm_cancels("LivingKnockBackEvent", || knock(1322.5)), 0.0);
    next_run();
    assert_eq!(
        native_cancels("LivingKnockBackEvent", || knock(1338.5)),
        0.0
    );
}

/// The table row `EntityBase::damage`: `LivingDamageEventPre` sets the damage that the health
/// takes, and `LivingDamageEventPost` reaches all handlers.
#[test]
fn living_damage_events_of_damage() {
    let _serial = setup();

    let taken = chain("LivingDamageEventPre", ["4", "3", "2"], false, || {
        damage_taken(1354.5, generic)
    });
    assert_eq!(taken, 2.0);
    next_run();
    assert_eq!(
        observe("LivingDamageEventPost", || damage_taken(1370.5, generic)),
        5.0
    );
}

/// The table row "the damage that kills" for a mob: `EntityDeathEvent`. The fire site reads no
/// field; the chain changes `dropped_exp` to show the order.
#[test]
fn entity_death_event_of_a_kill_damage() {
    let _serial = setup();

    let taken = chain("EntityDeathEvent", ["1", "2", "3"], false, || {
        damage_taken(1386.5, |pig| {
            pig.damage(pig.as_ref(), 1000.0, DamageType::GENERIC);
        })
    });
    assert!(taken > 0.0);
}

/// The table rows "the damage that kills" and `Player::kill` for a player: `PlayerDeathEvent`.
/// A cancel skips the death handling of the player, so no death location is recorded.
#[test]
fn player_death_event_of_player_kill() {
    let _serial = setup();
    let die = |name: &str, x: f64| {
        let player = player_at(name, x);
        player.kill();
        player.last_death_location.lock().unwrap().is_some()
    };

    assert!(chain("PlayerDeathEvent", ["1", "2", "3"], true, || die(
        "Victim", 1402.5
    )));
    next_run();
    assert!(!wasm_cancels("PlayerDeathEvent", || die("Spared", 1418.5)));
    next_run();
    assert!(!native_cancels("PlayerDeathEvent", || die("Saved", 1434.5)));
}

/// The table row "the damage that kills" for a mob that a player kills:
/// `LivingExperienceDropEvent`. 12 points are 4 orbs (7, 3, 1, 1).
#[test]
fn living_experience_drop_event_of_a_kill_damage() {
    let _serial = setup();
    let killer = player_at("Hunter", 1450.5);
    let hunt = |x: f64| {
        let zombie = spawn(&EntityType::ZOMBIE, x);
        let pos = zombie.get_entity().pos.load();
        zombie.damage_with_context(
            zombie.as_ref(),
            1000.0,
            DamageType::GENERIC,
            None,
            Some(killer.as_ref()),
            Some(killer.as_ref()),
        );
        count_near(&EntityType::EXPERIENCE_ORB, pos)
    };

    assert_eq!(
        chain(
            "LivingExperienceDropEvent",
            ["20", "16", "12"],
            true,
            || hunt(1452.5)
        ),
        4
    );
    next_run();
    assert_eq!(
        wasm_cancels("LivingExperienceDropEvent", || hunt(1468.5)),
        0
    );
    next_run();
    assert_eq!(
        native_cancels("LivingExperienceDropEvent", || hunt(1484.5)),
        0
    );
}

/// The table row `LivingEntity::heal`: `EntityRegainHealthEvent` sets the amount.
#[test]
fn entity_regain_health_event_of_heal() {
    let _serial = setup();
    let zombie = spawn(&EntityType::ZOMBIE, 1500.5);
    let heal = || {
        living(&zombie).set_health(5.0);
        living(&zombie).heal(1.0);
        health(&zombie)
    };

    assert_eq!(
        chain("EntityRegainHealthEvent", ["2", "3", "4"], true, heal),
        9.0
    );
    next_run();
    assert_eq!(wasm_cancels("EntityRegainHealthEvent", heal), 5.0);
    next_run();
    assert_eq!(native_cancels("EntityRegainHealthEvent", heal), 5.0);
}

/// The table row `Entity::remove`: `EntityLeaveLevelEvent` reaches all handlers.
#[test]
fn entity_leave_level_event_of_remove() {
    let _serial = setup();
    let pig = spawn(&EntityType::PIG, 1516.5);
    next_run();

    assert!(!observe("EntityLeaveLevelEvent", || {
        pig.get_entity().remove();
        is_live(&pig)
    }));
}

/// The table row `LivingEntity::set_active_hand`: `LivingEntityUseItemEventStart` sets the
/// duration of the use.
#[test]
fn living_entity_use_item_event_start_of_set_active_hand() {
    let _serial = setup();
    let eat = |x: f64| {
        let zombie = spawn(&EntityType::ZOMBIE, x);
        living(&zombie).set_active_hand(Hand::Right, ItemStack::new(1, &Item::BREAD), 32);
        living(&zombie)
            .item_in_use
            .lock()
            .unwrap()
            .is_some()
            .then(|| {
                living(&zombie)
                    .item_use_time
                    .load(std::sync::atomic::Ordering::Relaxed)
            })
    };

    assert_eq!(
        chain(
            "LivingEntityUseItemEventStart",
            ["30", "20", "10"],
            true,
            || eat(1532.5)
        ),
        Some(10)
    );
    next_run();
    assert_eq!(
        wasm_cancels("LivingEntityUseItemEventStart", || eat(1548.5)),
        None
    );
    next_run();
    assert_eq!(
        native_cancels("LivingEntityUseItemEventStart", || eat(1564.5)),
        None
    );
}

/// The table row `Player::release_using_item`: `LivingEntityUseItemEventStop`. A cancel shoots
/// nothing, so the arrows stay.
#[test]
fn living_entity_use_item_event_stop_of_release_using_item() {
    let _serial = setup();
    let player = player_at("Archer", 1580.5);
    let shoot = || {
        let inventory = player.inventory();
        inventory.set_selected_slot(0);
        inventory.set_stack(0, ItemStack::new(1, &Item::BOW));
        inventory.set_stack(1, ItemStack::new(4, &Item::ARROW));
        player.living_entity.set_active_hand(
            Hand::Right,
            ItemStack::new(1, &Item::BOW),
            BowItem::USE_DURATION - 20,
        );
        player.release_using_item();
        player.inventory().get_stack(1).item_count
    };

    assert_eq!(cancel_chain("LivingEntityUseItemEventStop", shoot), 3);
    next_run();
    assert_eq!(wasm_cancels("LivingEntityUseItemEventStop", shoot), 4);
    next_run();
    assert_eq!(native_cancels("LivingEntityUseItemEventStop", shoot), 4);
}

/// The table row `Player::release_using_item`: the release of a bow fires `ArrowLooseEvent`
/// after `LivingEntityUseItemEventStop`. Each handler sees the charge of the handler before it,
/// and a cancel shoots nothing.
#[test]
fn arrow_loose_event_of_release_using_item() {
    let _serial = setup();
    let player = player_at("LooseArcher", 1600.5);
    let shoot = || {
        let inventory = player.inventory();
        inventory.set_selected_slot(0);
        inventory.set_stack(0, ItemStack::new(1, &Item::BOW));
        inventory.set_stack(1, ItemStack::new(4, &Item::ARROW));
        player.living_entity.set_active_hand(
            Hand::Right,
            ItemStack::new(1, &Item::BOW),
            BowItem::USE_DURATION - 20,
        );
        player.release_using_item();
        player.inventory().get_stack(1).item_count
    };

    assert_eq!(chain("ArrowLooseEvent", ["5", "10", "15"], true, shoot), 3);
    next_run();
    assert_eq!(wasm_cancels("ArrowLooseEvent", shoot), 4);
    next_run();
    assert_eq!(native_cancels("ArrowLooseEvent", shoot), 4);
}

const fn speed() -> Effect {
    Effect {
        effect_type: &StatusEffect::SPEED,
        duration: 200,
        amplifier: 0,
        ambient: false,
        show_particles: true,
        show_icon: true,
        blend: false,
    }
}

/// The table row `LivingEntity::add_effect`: `MobEffectEventApplicable`. W2 leaves
/// `DoNotApply`, so the zombie gets no effect.
#[test]
fn mob_effect_event_applicable_of_add_effect() {
    let _serial = setup();
    let zombie = spawn(&EntityType::ZOMBIE, 1596.5);
    next_run();

    chain(
        "MobEffectEventApplicable",
        ["apply", "default", "do-not-apply"],
        false,
        || living(&zombie).add_effect(speed()),
    );
    assert!(!living(&zombie).has_effect(&StatusEffect::SPEED));
}

/// The table row `LivingEntity::add_effect`: `EntityPotionEffectEvent` (`NeoForge`
/// `MobEffectEvent.Added`). The fire site reads only the cancel flag; the chain changes
/// `amplifier` to show the order.
#[test]
fn entity_potion_effect_event_of_add_effect() {
    let _serial = setup();
    let affect = |x: f64| {
        let zombie = spawn(&EntityType::ZOMBIE, x);
        living(&zombie).add_effect(speed());
        living(&zombie).has_effect(&StatusEffect::SPEED)
    };

    assert!(chain(
        "EntityPotionEffectEvent",
        ["1", "2", "3"],
        true,
        || affect(1612.5)
    ));
    next_run();
    assert!(!wasm_cancels("EntityPotionEffectEvent", || affect(1628.5)));
    next_run();
    assert!(!native_cancels("EntityPotionEffectEvent", || affect(
        1644.5
    )));
}

/// The scenario "A v0.2 world call from a Wasm plugin": `dispatch-guest` calls the v0.2
/// `world.spawn-entity`, and `dispatch-peer` cancels the `EntitySpawnEvent`, so no entity spawns.
#[test]
fn a_v0_2_world_call_from_a_wasm_plugin() {
    let _serial = setup();
    let world = support::world();
    let spawn_pig = || -> i32 { ipc(GUEST, "spawn 1660.5 120 0.5").parse().unwrap() };

    let id = spawn_pig();
    assert!(
        world.get_entity_by_id(id).is_some(),
        "the world call spawns"
    );
    next_run();
    act("P1", "EntitySpawnEvent", "cancel=true");
    let id = spawn_pig();
    assert!(world.get_entity_by_id(id).is_none(), "no entity spawns");
    assert_eq!(
        guest_log(PEER, "EntitySpawnEvent"),
        ["W1 - false", "W2 - true"]
    );
    assert_eq!(
        seen("EntitySpawnEvent"),
        fire("- false", "- false", "- true", "- true")
    );
}

/// The scenario "Internal machine state", and the table rows without an event:
/// `World::set_block_state`, `LivingEntity::set_health` and the block entity mutators reach no
/// handler.
#[test]
fn mutators_without_an_event_reach_no_handler() {
    let _serial = setup();
    let pos = spot(11);
    load(pos);
    let zombie = spawn(&EntityType::ZOMBIE, 1676.5);
    let world = support::world();
    next_run();

    set(pos, &Block::BARREL);
    let barrel: Arc<dyn BlockEntity> = Arc::new(BarrelBlockEntity::new(pos));
    world.add_block_entity(barrel.clone());
    world.update_block_entity(&barrel);
    world.remove_block_entity(&pos);
    set(pos, &Block::AIR);
    living(&zombie).set_health(5.0);

    assert_eq!(state_at(pos), Block::AIR.default_state.id);
    assert_eq!(health(&zombie), 5.0);
    assert_eq!(native().log, Vec::<String>::new());
    assert_eq!(ipc(GUEST, "log"), "");
    assert_eq!(ipc(PEER, "log"), "");
}
