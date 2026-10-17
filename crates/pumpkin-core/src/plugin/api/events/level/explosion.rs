//! The explosion events of `NeoForge` (`event.level.ExplosionEvent`,
//! `event.level.ExplosionKnockbackEvent`) at fire sites where Pumpkin has no event.

use std::sync::Arc;

use pumpkin_macros::Event;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use crate::entity::EntityBase;
use crate::world::World;

/// An explosion has its blocks and entities and has changed nothing yet.
///
/// `NeoForge`: `net.neoforged.neoforge.event.level.ExplosionEvent.Detonate`, fired by
/// `EventHooks.onExplosionDetonate` in `ServerExplosion.hurtEntities`, before the damage and
/// before the blocks change. Both lists are settable: the explosion damages only the entities and
/// changes only the blocks that the last handler left. `ExplosionEvent.Start` is
/// `EntityExplodeEvent`.
#[derive(Event, Clone)]
pub struct ExplosionEventDetonate {
    pub world: Arc<World>,
    /// The center of the explosion.
    pub center: Vector3<f64>,
    /// The power of the explosion (`ServerExplosion#radius`).
    pub power: f32,
    /// The blocks that the explosion changes (`getAffectedBlocks`).
    pub affected_blocks: Vec<BlockPos>,
    /// The entities in the range of the explosion (`getAffectedEntities`).
    pub affected_entities: Vec<Arc<dyn EntityBase>>,
}

impl ExplosionEventDetonate {
    #[must_use]
    pub const fn new(
        world: Arc<World>,
        center: Vector3<f64>,
        power: f32,
        affected_blocks: Vec<BlockPos>,
        affected_entities: Vec<Arc<dyn EntityBase>>,
    ) -> Self {
        Self {
            world,
            center,
            power,
            affected_blocks,
            affected_entities,
        }
    }
}

/// An explosion pushes an entity.
///
/// `NeoForge`: `net.neoforged.neoforge.event.level.ExplosionKnockbackEvent`, fired by
/// `EventHooks.getExplosionKnockback` in `ServerExplosion.hurtEntities` for each entity that the
/// explosion reaches, before the push. `knockback` is settable.
#[derive(Event, Clone)]
pub struct ExplosionKnockbackEvent {
    pub world: Arc<World>,
    pub center: Vector3<f64>,
    pub power: f32,
    /// The entity that the explosion pushes (`getAffectedEntity`).
    pub entity: Arc<dyn EntityBase>,
    /// The velocity that the push adds (`getKnockbackVelocity`).
    pub knockback: Vector3<f64>,
    /// The blocks that the explosion changes (`getAffectedBlocks`).
    pub affected_blocks: Vec<BlockPos>,
}

impl ExplosionKnockbackEvent {
    #[must_use]
    pub const fn new(
        world: Arc<World>,
        center: Vector3<f64>,
        power: f32,
        entity: Arc<dyn EntityBase>,
        knockback: Vector3<f64>,
        affected_blocks: Vec<BlockPos>,
    ) -> Self {
        Self {
            world,
            center,
            power,
            entity,
            knockback,
            affected_blocks,
        }
    }
}
