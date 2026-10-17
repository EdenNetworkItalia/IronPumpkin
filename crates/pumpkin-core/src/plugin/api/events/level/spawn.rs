//! `LevelEvent.PotentialSpawns` of `NeoForge`. Pumpkin has no event on the spawn list.

use std::sync::Arc;

use pumpkin_data::biome::Spawner;
use pumpkin_data::entity::MobCategory;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;

use crate::world::World;

/// The natural spawner reads the mobs that may spawn at a position.
///
/// `NeoForge`: `net.neoforged.neoforge.event.level.LevelEvent.PotentialSpawns`, fired by
/// `EventHooks.getPotentialSpawns` in `NaturalSpawner.mobsAt`, which picks a mob and checks a
/// mob for a position. `spawns` is settable: the spawner picks from the list that the last
/// handler left. A new entry is a `&'static Spawner`, such as a `static` of the mod. A cancel
/// spawns nothing. The biome lists of Pumpkin have no weights, so the pick is uniform, and no
/// nether fortress list replaces the biome list.
#[cancellable]
#[derive(Event, Clone)]
pub struct LevelEventPotentialSpawns {
    pub world: Arc<World>,
    /// `getMobCategory`.
    pub category: &'static MobCategory,
    /// `getPos`.
    pub pos: BlockPos,
    /// `getSpawnerDataList`.
    pub spawns: Vec<&'static Spawner>,
}

impl LevelEventPotentialSpawns {
    #[must_use]
    pub const fn new(
        world: Arc<World>,
        category: &'static MobCategory,
        pos: BlockPos,
        spawns: Vec<&'static Spawner>,
    ) -> Self {
        Self {
            world,
            category,
            pos,
            spawns,
            cancelled: false,
        }
    }
}
