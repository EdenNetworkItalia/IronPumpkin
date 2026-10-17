use crate::world::World;
use pumpkin_macros::{Event, cancellable};
use pumpkin_world::chunk::ChunkData;
use std::sync::Arc;

/// An event that occurs when a chunk is loaded in a world.
///
/// This event contains information about the world and the chunk being loaded.
///
/// `NeoForge`: `net.neoforged.neoforge.event.level.ChunkEvent.Load`. It fires in the world tick
/// after the chunk joined the loaded chunks of the world. The chunk is already loaded, so
/// `cancelled` has no effect.
#[cancellable]
#[derive(Event, Clone)]
pub struct ChunkLoad {
    /// The world in which the chunk is being loaded.
    pub world: Arc<World>,

    /// The chunk data being loaded.
    pub chunk: Arc<ChunkData>,
}

impl ChunkLoad {
    #[must_use]
    pub const fn new(world: Arc<World>, chunk: Arc<ChunkData>) -> Self {
        Self {
            world,
            chunk,
            cancelled: false,
        }
    }
}
