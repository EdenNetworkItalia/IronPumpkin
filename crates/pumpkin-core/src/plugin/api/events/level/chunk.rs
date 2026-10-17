//! The chunk events of `NeoForge` (`event.level.ChunkWatchEvent`) at fire sites where Pumpkin
//! has no event.

use std::sync::Arc;

use pumpkin_macros::Event;
use pumpkin_util::math::vector2::Vector2;

use crate::entity::player::Player;
use crate::world::World;

/// A player stops watching a chunk.
///
/// `NeoForge`: `net.neoforged.neoforge.event.level.ChunkWatchEvent.UnWatch`, fired by
/// `EventHooks.fireChunkUnWatch` in `ChunkMap.dropChunk`: when the chunk leaves the view of the
/// player, when the player changes world and when the player leaves, once per chunk of the view.
/// `ChunkWatchEvent.Watch` and `.Sent` are `ChunkSend`.
#[derive(Event, Clone)]
pub struct ChunkWatchEventUnWatch {
    pub player: Arc<Player>,
    pub world: Arc<World>,
    /// The chunk position (`getPos`).
    pub pos: Vector2<i32>,
}

impl ChunkWatchEventUnWatch {
    #[must_use]
    pub const fn new(player: Arc<Player>, world: Arc<World>, pos: Vector2<i32>) -> Self {
        Self { player, world, pos }
    }
}
