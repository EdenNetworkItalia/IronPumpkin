//! The level tick events of `NeoForge`. Pumpkin has no event around the tick of one world.

use std::sync::Arc;

use pumpkin_macros::Event;

use crate::world::World;

/// Fires before a world ticks, once per world and tick.
///
/// `NeoForge`: `net.neoforged.neoforge.event.tick.LevelTickEvent.Pre`, fired by
/// `EventHooks.fireLevelTickPre` in `MinecraftServer.tickChildren` before the level tick. The
/// worlds tick in parallel, so the handlers of two worlds can run at the same time.
#[derive(Event, Clone)]
pub struct LevelTickEventPre {
    /// The world that ticks (`LevelTickEvent#getLevel`).
    pub world: Arc<World>,
}

/// Fires after a world ticked, once per world and tick.
///
/// `NeoForge`: `net.neoforged.neoforge.event.tick.LevelTickEvent.Post`, fired by
/// `EventHooks.fireLevelTickPost` in `MinecraftServer.tickChildren` after the level tick.
#[derive(Event, Clone)]
pub struct LevelTickEventPost {
    /// The world that ticked (`LevelTickEvent#getLevel`).
    pub world: Arc<World>,
}

impl LevelTickEventPre {
    #[must_use]
    pub const fn new(world: Arc<World>) -> Self {
        Self { world }
    }

    /// Whether the tick has time left before its deadline (`LevelTickEvent#hasTime`).
    #[must_use]
    pub fn has_time(&self) -> bool {
        crate::server::ticker::has_time()
    }
}

impl LevelTickEventPost {
    #[must_use]
    pub const fn new(world: Arc<World>) -> Self {
        Self { world }
    }

    /// Whether the tick has time left before its deadline (`LevelTickEvent#hasTime`).
    #[must_use]
    pub fn has_time(&self) -> bool {
        crate::server::ticker::has_time()
    }
}
