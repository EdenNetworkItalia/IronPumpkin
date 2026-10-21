//! The `IronPumpkin` fire sites of the `NeoForge` entity, mob and spawn events (`event.entity`
//! and the mob and spawn events of `event.entity.living`).
//!
//! Each function checks `has_handlers` before it looks up an entity or builds an event, so a
//! fire site with no handler allocates nothing. The upstream code calls these functions with one
//! line.

use std::sync::Arc;

use pumpkin_data::{damage::DamageType, entity::EntityType};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use crate::entity::{Entity, EntityBase, player::Player, projectile::ProjectileHit};
use crate::plugin::api::events::{
    Payload,
    level::{
        EndermanAngerEvent, EntityInvulnerabilityCheckEvent, EntityLeaveLevelEvent,
        EntityMobGriefingEvent, EntitySpawnReason, EntityStruckByLightningEvent,
        EntityTeleportEventSpreadPlayersCommand, MobDespawnEvent, MobDespawnResult,
        MobSpawnEventPositionCheck, MobSpawnEventSpawnPlacementCheck, MobSpawnResult,
        MobSplitEvent, ProjectileImpactEvent, SpawnClusterSizeEvent,
    },
};
use crate::server::Server;
use crate::world::World;

/// The server of `world` when a handler of `E` is registered.
fn listening<E: Payload + 'static>(world: &World) -> Option<Arc<Server>> {
    world
        .server
        .upgrade()
        .filter(|server| server.plugin_manager.has_handlers::<E>())
}

/// The live entity of `entity` in `world`. The tracker holds every entity of the world, so the
/// linear scan of `World::get_entity_by_id` is only the fallback.
pub fn live(world: &World, entity: &Entity) -> Option<Arc<dyn EntityBase>> {
    world
        .entity_tracker
        .entity_map
        .get(&entity.entity_id)
        .map(|tracked| tracked.entity.clone())
        .or_else(|| world.get_entity_by_id(entity.entity_id))
}

/// The live entity of `entity` when a handler of `EntityLeaveLevelEvent` is registered. Call it
/// before the entity leaves the tracker, then [`entity_left`].
pub fn leaving(world: &World, entity: &Entity) -> Option<Arc<dyn EntityBase>> {
    listening::<EntityLeaveLevelEvent>(world)?;
    live(world, entity)
}

/// Fires `EntityLeaveLevelEvent` for an entity that left its world.
pub fn entity_left(entity: Option<Arc<dyn EntityBase>>) {
    let Some(entity) = entity else {
        return;
    };
    let world = entity.get_entity().world.load_full();
    let Some(server) = world.server.upgrade() else {
        return;
    };
    server.plugin_manager.fire_blocking_with(&server, || {
        EntityLeaveLevelEvent::new(entity.clone(), world.clone())
    });
}

/// Fires `EntityLeaveLevelEvent` for each entity of `entities`, which left `world`.
pub async fn entities_left(world: &World, entities: &[Arc<dyn EntityBase>]) {
    let Some(server) = listening::<EntityLeaveLevelEvent>(world) else {
        return;
    };
    for entity in entities {
        let world = entity.get_entity().world.load_full();
        server
            .plugin_manager
            .fire_with(&server, || {
                EntityLeaveLevelEvent::new(entity.clone(), world)
            })
            .await;
    }
}

/// `EventHooks.canEntityGrief`: the `mobGriefing` game rule, or the result of
/// `EntityMobGriefingEvent` for `entity`.
pub fn can_entity_grief(world: &World, entity: &Entity) -> bool {
    let mob_griefing = world.level_info.load().game_rules.mob_griefing;
    let Some(server) = listening::<EntityMobGriefingEvent>(world) else {
        return mob_griefing;
    };
    let Some(entity) = live(world, entity) else {
        return mob_griefing;
    };
    server
        .plugin_manager
        .fire_blocking_with(&server, || {
            EntityMobGriefingEvent::new(entity, mob_griefing)
        })
        .map_or(mob_griefing, |event| event.can_grief)
}

/// Fires `EntityStruckByLightningEvent` before `bolt` strikes `entity`. Returns `true` when a
/// handler cancelled it.
pub fn struck_by_lightning(world: &World, entity: &Arc<dyn EntityBase>, bolt: &Entity) -> bool {
    let Some(server) = listening::<EntityStruckByLightningEvent>(world) else {
        return false;
    };
    let Some(bolt) = live(world, bolt) else {
        return false;
    };
    server
        .plugin_manager
        .fire_blocking_with(&server, || {
            EntityStruckByLightningEvent::new(entity.clone(), bolt)
        })
        .is_some_and(|event| event.cancelled)
}

/// Fires `EntityTeleportEventSpreadPlayersCommand` before `/spreadplayers` moves `entity` to
/// `target` in `world`. Returns `None` when a handler cancelled it, else the target.
pub fn spread_players(
    entity: &Arc<dyn EntityBase>,
    world: &Arc<World>,
    target: Vector3<f64>,
) -> Option<Vector3<f64>> {
    let Some(server) = listening::<EntityTeleportEventSpreadPlayersCommand>(world) else {
        return Some(target);
    };
    let event = server.plugin_manager.fire_blocking_with(&server, || {
        EntityTeleportEventSpreadPlayersCommand::new(entity.clone(), world.clone(), target)
    });
    match event {
        Some(event) if event.cancelled => None,
        Some(event) => Some(event.target),
        None => Some(target),
    }
}

/// Fires `ProjectileImpactEvent` before `projectile` handles the hit that `hit` builds. Returns
/// `true` when a handler cancelled it.
pub fn projectile_impact(projectile: &Entity, hit: impl FnOnce() -> ProjectileHit) -> bool {
    let world = projectile.world.load();
    let Some(server) = listening::<ProjectileImpactEvent>(&world) else {
        return false;
    };
    let Some(projectile) = live(&world, projectile) else {
        return false;
    };
    server
        .plugin_manager
        .fire_blocking_with(&server, || ProjectileImpactEvent::new(projectile, hit()))
        .is_some_and(|event| event.cancelled)
}

/// `CommonHooks.shouldSuppressEnderManAnger`: fires `EndermanAngerEvent` for a player that
/// stares at `enderman`. Returns `true` when a handler cancelled it.
pub fn suppress_enderman_anger(enderman: &Entity, player: &Player) -> bool {
    let world = enderman.world.load();
    let Some(server) = listening::<EndermanAngerEvent>(&world) else {
        return false;
    };
    let (Some(enderman), Some(player)) = (
        live(&world, enderman),
        world.get_player_by_id(player.entity_id()),
    ) else {
        return false;
    };
    server
        .plugin_manager
        .fire_blocking_with(&server, || EndermanAngerEvent::new(enderman, player))
        .is_some_and(|event| event.cancelled)
}

/// Fires `MobDespawnEvent` at the start of the despawn check of `mob`. Returns the result,
/// `Default` without a handler.
pub fn mob_despawn(mob: &Entity) -> MobDespawnResult {
    let world = mob.world.load_full();
    let Some(server) = listening::<MobDespawnEvent>(&world) else {
        return MobDespawnResult::Default;
    };
    let Some(entity) = live(&world, mob) else {
        return MobDespawnResult::Default;
    };
    let pos = mob.pos.load();
    server
        .plugin_manager
        .fire_blocking_with(&server, || MobDespawnEvent::new(entity, world.clone(), pos))
        .map_or(MobDespawnResult::Default, |event| event.result)
}

/// `EventHooks.checkSpawnPosition`: fires `MobSpawnEventPositionCheck` for `entity`, a created
/// mob, and returns whether the position is valid. `vanilla` is the check of the site, run only
/// for the `Default` result.
pub fn check_spawn_position(
    entity: &Arc<dyn EntityBase>,
    world: &Arc<World>,
    spawn_type: EntitySpawnReason,
    spawner: Option<BlockPos>,
    vanilla: impl FnOnce() -> bool,
) -> bool {
    let result = listening::<MobSpawnEventPositionCheck>(world)
        .and_then(|server| {
            server.plugin_manager.fire_blocking_with(&server, || {
                MobSpawnEventPositionCheck::new(entity.clone(), world.clone(), spawn_type, spawner)
            })
        })
        .map_or(MobSpawnResult::Default, |event| event.result);
    match result {
        MobSpawnResult::Succeed => true,
        MobSpawnResult::Default => vanilla(),
        MobSpawnResult::Fail => false,
    }
}

/// `EventHooks.checkSpawnPlacements`: fires `MobSpawnEventSpawnPlacementCheck` with the result
/// of the spawn rules and returns the result of the check.
pub fn check_spawn_placements(
    entity_type: &'static EntityType,
    world: &Arc<World>,
    spawn_type: EntitySpawnReason,
    pos: &BlockPos,
    default_result: bool,
) -> bool {
    let Some(server) = listening::<MobSpawnEventSpawnPlacementCheck>(world) else {
        return default_result;
    };
    server
        .plugin_manager
        .fire_blocking_with(&server, || {
            MobSpawnEventSpawnPlacementCheck::new(
                entity_type,
                world.clone(),
                spawn_type,
                *pos,
                default_result,
            )
        })
        .map_or(default_result, |event| event.placement_check_result())
}

/// Fires `MobSplitEvent` for the children of `parent`. Returns the children to spawn: the
/// children without a handler, none when a handler cancelled it.
pub fn mob_split(parent: &Entity, children: Vec<Arc<dyn EntityBase>>) -> Vec<Arc<dyn EntityBase>> {
    let world = parent.world.load();
    let Some(server) = listening::<MobSplitEvent>(&world) else {
        return children;
    };
    let Some(parent) = live(&world, parent) else {
        return children;
    };
    let Some(event) = server
        .plugin_manager
        .fire_blocking_with(&server, || MobSplitEvent::new(parent, children.clone()))
    else {
        return children;
    };
    if event.cancelled {
        return Vec::new();
    }
    // A child must be a new entity: spawning a live entity again would list it twice.
    event
        .children
        .into_iter()
        .filter(|child| {
            let child = child.get_entity();
            !child.removed.load(std::sync::atomic::Ordering::Relaxed)
                && world.get_entity_by_uuid(child.entity_uuid).is_none()
        })
        .collect()
}

/// `EventHooks.getMaxSpawnClusterSize`: fires `SpawnClusterSizeEvent` for `entity`, which just
/// spawned, and returns the size, `size` without a handler.
pub fn max_spawn_cluster_size(world: &World, entity: &Arc<dyn EntityBase>, size: i32) -> i32 {
    let Some(server) = listening::<SpawnClusterSizeEvent>(world) else {
        return size;
    };
    server
        .plugin_manager
        .fire_blocking_with(&server, || SpawnClusterSizeEvent::new(entity.clone(), size))
        .map_or(size, |event| event.size)
}

/// `CommonHooks.isEntityInvulnerableTo`: fires `EntityInvulnerabilityCheckEvent` with the vanilla
/// result and returns the result of the handlers.
pub fn is_entity_invulnerable_to(
    entity: &Entity,
    damage_type: &DamageType,
    cause: Option<&dyn EntityBase>,
    invulnerable: bool,
) -> bool {
    let world = entity.world.load();
    let Some(server) = listening::<EntityInvulnerabilityCheckEvent>(&world) else {
        return invulnerable;
    };
    let Some(target) = live(&world, entity) else {
        return invulnerable;
    };
    let attacker = cause.and_then(|cause| live(&world, cause.get_entity()));
    server
        .plugin_manager
        .fire_blocking_with(&server, || {
            EntityInvulnerabilityCheckEvent::new(target, *damage_type, attacker, invulnerable)
        })
        .map_or(invulnerable, |event| event.invulnerable)
}
