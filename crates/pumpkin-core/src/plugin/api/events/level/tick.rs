//! The entity and player tick events of `NeoForge` (`event.tick`). Pumpkin has no event around
//! the tick of one entity.

use std::sync::Arc;

use pumpkin_macros::{Event, cancellable};

use crate::entity::{EntityBase, player::Player};

/// Fires before an entity that is not a player ticks.
///
/// `NeoForge`: `net.neoforged.neoforge.event.tick.EntityTickEvent.Pre`, fired by
/// `EventHooks.fireEntityTickPre` in `ServerLevel.tickNonPassenger`. A cancel skips the tick and
/// [`EntityTickEventPost`]. The entities tick in parallel, so the handlers of two entities can
/// run at the same time.
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityTickEventPre {
    pub entity: Arc<dyn EntityBase>,
}

/// Fires after an entity that is not a player ticked.
///
/// `NeoForge`: `net.neoforged.neoforge.event.tick.EntityTickEvent.Post`, fired by
/// `EventHooks.fireEntityTickPost` in `ServerLevel.tickNonPassenger`.
#[derive(Event, Clone)]
pub struct EntityTickEventPost {
    pub entity: Arc<dyn EntityBase>,
}

impl EntityTickEventPre {
    #[must_use]
    pub const fn new(entity: Arc<dyn EntityBase>) -> Self {
        Self {
            entity,
            cancelled: false,
        }
    }
}

impl EntityTickEventPost {
    #[must_use]
    pub const fn new(entity: Arc<dyn EntityBase>) -> Self {
        Self { entity }
    }
}

/// Fires at the start of the tick of a player, after its packets of the tick.
///
/// `NeoForge`: `net.neoforged.neoforge.event.tick.PlayerTickEvent.Pre`, fired by
/// `EventHooks.firePlayerTickPre` at the start of `Player.tick`. The players tick in parallel.
#[derive(Event, Clone)]
pub struct PlayerTickEventPre {
    pub player: Arc<Player>,
}

/// Fires at the end of the tick of a player.
///
/// `NeoForge`: `net.neoforged.neoforge.event.tick.PlayerTickEvent.Post`, fired by
/// `EventHooks.firePlayerTickPost` at the end of `Player.tick`.
#[derive(Event, Clone)]
pub struct PlayerTickEventPost {
    pub player: Arc<Player>,
}

impl PlayerTickEventPre {
    #[must_use]
    pub const fn new(player: Arc<Player>) -> Self {
        Self { player }
    }
}

impl PlayerTickEventPost {
    #[must_use]
    pub const fn new(player: Arc<Player>) -> Self {
        Self { player }
    }
}
