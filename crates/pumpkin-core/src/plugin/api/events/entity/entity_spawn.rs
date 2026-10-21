use crate::world::World;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::vector3::Vector3;
use std::sync::Arc;

/// An event that occurs when an entity is spawned into a world.
#[cancellable]
#[derive(Event, Clone)]
pub struct EntitySpawnEvent {
    /// The ID of the spawned entity.
    pub entity_id: i32,

    /// The registry name of the entity type.
    pub entity_type: String,

    /// The position where the entity is spawned.
    pub position: Vector3<f64>,

    /// The world in which the entity is spawned.
    pub world: Arc<World>,

    /// Whether the entity loads from its chunk instead of spawning new, as `NeoForge`
    /// `EntityJoinLevelEvent.loadedFromDisk`. A cancel of a loaded entity keeps it out of the
    /// world, and the next save of its chunk no longer has it.
    pub loaded_from_disk: bool,
}

impl EntitySpawnEvent {
    #[must_use]
    pub const fn new(
        entity_id: i32,
        entity_type: String,
        position: Vector3<f64>,
        world: Arc<World>,
    ) -> Self {
        Self {
            entity_id,
            entity_type,
            position,
            world,
            loaded_from_disk: false,
            cancelled: false,
        }
    }
}
