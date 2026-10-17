//! Scenario "A mod cancels the break of another mod's block" of the spec `neoforge-event-bus`
//! (`openspec/changes/neoforge-shaped-plugin-api`), with two native mods on a real server: mod A
//! registers the block `moda:ore`, mod B cancels `BlockBreakEvent` (`NeoForge` `BreakBlockEvent`)
//! for it. The content registry freezes once per process, so this file is its own test binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

use std::sync::{Arc, Mutex, PoisonError};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    content::BlockBuilder,
    event::{
        BoxFuture, Cancellable, EventHandler, EventPriority,
        block::{
            block_break::BlockBreakEvent, block_drop_item::BlockDropItemEvent,
            block_exp::BlockExpEvent,
        },
    },
    pumpkin_core::plugin::startup::STARTUP_HOOK,
    pumpkin_data::Block,
    pumpkin_util::{math::position::BlockPos, permission::PermissionLvl},
    pumpkin_world::world::BlockFlags,
    register_mod,
    server::Server,
};

static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn log(entry: String) {
    LOG.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(entry);
}

fn is_ore(event: &BlockBreakEvent) -> bool {
    event.block.name == "moda:ore"
}

/// Mod A: owns the block, logs its own break handler, and watches the drops.
struct ModA;

impl NativeMod for ModA {
    fn id(&self) -> &'static str {
        "moda"
    }

    fn display_name(&self) -> &'static str {
        "Mod A"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        cx.register_block(BlockBuilder::new(
            "moda:ore",
            Block::IRON_ORE.default_state.id,
        ))
        .unwrap();
        cx.register_event::<BlockBreakEvent, _>(
            Arc::new(BreakLogger),
            EventPriority::Normal,
            true,
            false,
        );
        cx.register_event::<BlockDropItemEvent, _>(
            Arc::new(DropLogger),
            EventPriority::Normal,
            true,
            true,
        );
        cx.register_event::<BlockExpEvent, _>(
            Arc::new(DropLogger),
            EventPriority::Normal,
            true,
            true,
        );
    }
}

/// Mod B: cancels the break of mod A's block and observes it after the cancel.
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
        cx.register_event::<BlockBreakEvent, _>(
            Arc::new(Canceller),
            EventPriority::High,
            true,
            false,
        );
        cx.register_event::<BlockBreakEvent, _>(Arc::new(Observer), EventPriority::Low, true, true);
    }
}

register_mod!(ModA);
register_mod!(ModB);

struct Canceller;

impl EventHandler<BlockBreakEvent> for Canceller {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut BlockBreakEvent,
    ) -> BoxFuture<'a, ()> {
        if is_ore(event) {
            log("modb cancels".to_owned());
            event.set_cancelled(true);
        }
        Box::pin(async {})
    }
}

struct Observer;

impl EventHandler<BlockBreakEvent> for Observer {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut BlockBreakEvent,
    ) -> BoxFuture<'a, ()> {
        if is_ore(event) {
            log(format!("modb observes cancelled {}", event.cancelled()));
        }
        Box::pin(async {})
    }
}

struct BreakLogger;

impl EventHandler<BlockBreakEvent> for BreakLogger {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut BlockBreakEvent,
    ) -> BoxFuture<'a, ()> {
        if is_ore(event) {
            log("moda handles the break".to_owned());
        }
        Box::pin(async {})
    }
}

struct DropLogger;

impl EventHandler<BlockDropItemEvent> for DropLogger {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut BlockDropItemEvent,
    ) -> BoxFuture<'a, ()> {
        log(format!("drop at {}", event.block_pos));
        Box::pin(async {})
    }
}

impl EventHandler<BlockExpEvent> for DropLogger {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut BlockExpEvent,
    ) -> BoxFuture<'a, ()> {
        log(format!("experience at {}", event.block_pos));
        Box::pin(async {})
    }
}

#[test]
fn a_mod_cancels_the_break_of_another_mods_block() {
    STARTUP_HOOK.set(ironpumpkin_mods::init_mods).unwrap();
    let player = support::player("OreBreaker");
    // The world spawn depends on the seed: an operator ignores the spawn protection.
    player.permission_lvl.store(PermissionLvl::Four);
    let world = support::world();
    let pos = BlockPos::new(40, 120, 8);
    {
        let chunk = pos.chunk_position();
        let mut loading = world.level.chunk_loading.lock().unwrap();
        loading.add_force_ticket(chunk);
        loading.send_change();
    };
    let deadline = std::time::Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&pos.chunk_position()) {
        assert!(
            std::time::Instant::now() < deadline,
            "the chunk did not load"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let ore = Block::from_name("moda:ore").unwrap();
    world.set_block_state(&pos, ore.default_state.id, BlockFlags::empty());

    let broken = world.break_block(&pos, Some(&player), BlockFlags::NOTIFY_ALL);

    assert_eq!(broken, None);
    assert_eq!(world.get_block_state_id(&pos), ore.default_state.id);
    assert_eq!(
        *LOG.lock().unwrap(),
        ["modb cancels", "modb observes cancelled true"]
    );
}
