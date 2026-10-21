//! The entity, mob and spawn events of `NeoForge` (`event.entity`, the mob and spawn events of
//! `event.entity.living`) at fire sites where Pumpkin has no event.
//!
//! The entity of an event is the live entity of the world. A handler runs inside the entity tick
//! or the damage of the entity, so it keeps its work short.

use std::sync::Arc;

use pumpkin_data::{damage::DamageType, entity::EntityType};
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use crate::entity::{EntityBase, player::Player, projectile::ProjectileHit};
use crate::world::World;

/// An entity left its world: it was removed, it unloaded with its chunk, or a player left.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.EntityLeaveLevelEvent`, fired in
/// `ServerLevel.EntityCallbacks.onTrackingEnd`. It fires after the entity left the entity list,
/// so a lookup in `world` no longer finds it.
#[derive(Event, Clone)]
pub struct EntityLeaveLevelEvent {
    pub entity: Arc<dyn EntityBase>,
    /// `getLevel`.
    pub world: Arc<World>,
}

impl EntityLeaveLevelEvent {
    #[must_use]
    pub fn new(entity: Arc<dyn EntityBase>, world: Arc<World>) -> Self {
        Self { entity, world }
    }
}

/// An entity checks the `mobGriefing` game rule before it changes the world.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.EntityMobGriefingEvent`, fired by
/// `EventHooks.canEntityGrief` where vanilla reads the game rule for an entity. `can_grief` is
/// settable and starts at the game rule.
#[derive(Event, Clone)]
pub struct EntityMobGriefingEvent {
    pub entity: Arc<dyn EntityBase>,
    /// `isMobGriefingEnabled`: the game rule.
    pub mob_griefing_enabled: bool,
    /// `canGrief`.
    pub can_grief: bool,
}

impl EntityMobGriefingEvent {
    #[must_use]
    pub fn new(entity: Arc<dyn EntityBase>, mob_griefing_enabled: bool) -> Self {
        Self {
            entity,
            mob_griefing_enabled,
            can_grief: mob_griefing_enabled,
        }
    }
}

/// A lightning bolt is about to strike an entity.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.EntityStruckByLightningEvent`, fired by
/// `EventHooks.onEntityStruckByLightning` in `LightningBolt.tick` before `thunderHit`. A cancel
/// spares the entity from this bolt.
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityStruckByLightningEvent {
    pub entity: Arc<dyn EntityBase>,
    /// `getLightning`.
    pub lightning: Arc<dyn EntityBase>,
}

impl EntityStruckByLightningEvent {
    #[must_use]
    pub fn new(entity: Arc<dyn EntityBase>, lightning: Arc<dyn EntityBase>) -> Self {
        Self {
            entity,
            lightning,
            cancelled: false,
        }
    }
}

/// `/spreadplayers` is about to move an entity.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.EntityTeleportEvent.SpreadPlayersCommand`,
/// fired by `EventHooks.onEntityTeleportSpreadPlayersCommand` in
/// `SpreadPlayersCommand.setPlayerPositions`. `target` is settable. A cancel leaves the entity
/// where it is; the command still counts it. The previous position is the position of `entity`.
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityTeleportEventSpreadPlayersCommand {
    pub entity: Arc<dyn EntityBase>,
    /// `getTargetLevel`.
    pub world: Arc<World>,
    /// `getTarget`.
    pub target: Vector3<f64>,
}

impl EntityTeleportEventSpreadPlayersCommand {
    #[must_use]
    pub fn new(entity: Arc<dyn EntityBase>, world: Arc<World>, target: Vector3<f64>) -> Self {
        Self {
            entity,
            world,
            target,
            cancelled: false,
        }
    }
}

/// A projectile hits a block or an entity.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.ProjectileImpactEvent`, fired by
/// `EventHooks.onProjectileImpact` before the projectile handles the hit. A cancel skips the hit,
/// and the projectile flies on.
#[cancellable]
#[derive(Event, Clone)]
pub struct ProjectileImpactEvent {
    pub projectile: Arc<dyn EntityBase>,
    /// `getRayTraceResult`.
    pub ray: ProjectileHit,
}

impl ProjectileImpactEvent {
    #[must_use]
    pub fn new(projectile: Arc<dyn EntityBase>, ray: ProjectileHit) -> Self {
        Self {
            projectile,
            ray,
            cancelled: false,
        }
    }
}

/// A player stares at an enderman, which would make it angry.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.EndermanAngerEvent`, fired by
/// `CommonHooks.shouldSuppressEnderManAnger` in `EnderMan.isBeingStaredBy` after the stare
/// check found the stare. A cancel makes the stare not count.
#[cancellable]
#[derive(Event, Clone)]
pub struct EndermanAngerEvent {
    /// The enderman.
    pub entity: Arc<dyn EntityBase>,
    pub player: Arc<Player>,
}

impl EndermanAngerEvent {
    #[must_use]
    pub fn new(entity: Arc<dyn EntityBase>, player: Arc<Player>) -> Self {
        Self {
            entity,
            player,
            cancelled: false,
        }
    }
}

/// The result of [`MobDespawnEvent`], as `MobDespawnEvent.Result`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MobDespawnResult {
    /// The mob despawns now.
    Allow,
    /// The vanilla despawn check decides.
    #[default]
    Default,
    /// The mob does not despawn in this tick.
    Deny,
}

/// A mob checks whether it despawns.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.MobDespawnEvent`, fired by
/// `EventHooks.checkMobDespawn` at the start of `Mob.checkDespawn`, once per mob and tick, also
/// for a persistent mob. `result` is settable.
#[derive(Event, Clone)]
pub struct MobDespawnEvent {
    pub entity: Arc<dyn EntityBase>,
    pub world: Arc<World>,
    /// `getX`, `getY` and `getZ`: the position of the mob.
    pub pos: Vector3<f64>,
    pub result: MobDespawnResult,
}

impl MobDespawnEvent {
    #[must_use]
    pub fn new(entity: Arc<dyn EntityBase>, world: Arc<World>, pos: Vector3<f64>) -> Self {
        Self {
            entity,
            world,
            pos,
            result: MobDespawnResult::Default,
        }
    }
}

/// Why a mob spawns, as vanilla `EntitySpawnReason`. Pumpkin fires the spawn checks with
/// `Natural`, `ChunkGeneration`, `Spawner` and `Triggered` (a warden that a sculk shrieker
/// summons).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntitySpawnReason {
    Natural,
    ChunkGeneration,
    Spawner,
    Structure,
    Breeding,
    MobSummoned,
    Jockey,
    Event,
    Conversion,
    Reinforcement,
    Triggered,
    Bucket,
    SpawnItemUse,
    Command,
    Dispenser,
    Patrol,
    TrialSpawner,
    Load,
    DimensionTravel,
}

/// The result of a spawn check, as `MobSpawnEvent.PositionCheck.Result` and
/// `MobSpawnEvent.SpawnPlacementCheck.Result`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MobSpawnResult {
    /// The check passes.
    Succeed,
    /// The vanilla check decides.
    #[default]
    Default,
    /// The check fails.
    Fail,
}

/// The spawner checks the position of a created mob before it spawns it.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.MobSpawnEvent.PositionCheck`, fired by
/// `EventHooks.checkSpawnPosition` in `NaturalSpawner.isValidPositionForMob`, in the chunk
/// generation spawns of `NaturalSpawner`, in `SpawnUtil.trySpawnMob`, and by
/// `EventHooks.checkSpawnPositionSpawner` in `BaseSpawner.serverTick`. `result` is settable.
/// The default is the check of Pumpkin at the site: the obstruction check of the summon,
/// and no check for the others, which have no `Mob.checkSpawnRules`.
#[derive(Event, Clone)]
pub struct MobSpawnEventPositionCheck {
    pub entity: Arc<dyn EntityBase>,
    pub world: Arc<World>,
    /// `getX`, `getY` and `getZ`: the position of the mob.
    pub pos: Vector3<f64>,
    /// `getSpawnType`.
    pub spawn_type: EntitySpawnReason,
    /// `getSpawner`: the position of the mob spawner block, `None` for the other spawns.
    pub spawner: Option<BlockPos>,
    pub result: MobSpawnResult,
}

impl MobSpawnEventPositionCheck {
    #[must_use]
    pub fn new(
        entity: Arc<dyn EntityBase>,
        world: Arc<World>,
        spawn_type: EntitySpawnReason,
        spawner: Option<BlockPos>,
    ) -> Self {
        let pos = entity.get_entity().pos.load();
        Self {
            entity,
            world,
            pos,
            spawn_type,
            spawner,
            result: MobSpawnResult::Default,
        }
    }
}

/// The spawn rules of an entity type decide whether it may spawn at a position.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.MobSpawnEvent.SpawnPlacementCheck`,
/// fired by `EventHooks.checkSpawnPlacements` at the end of `SpawnPlacements.checkSpawnRules`.
/// `result` is settable. It has no random source: the spawn rules of Pumpkin use their own.
#[derive(Event, Clone)]
pub struct MobSpawnEventSpawnPlacementCheck {
    pub entity_type: &'static EntityType,
    pub world: Arc<World>,
    /// `getSpawnType`.
    pub spawn_type: EntitySpawnReason,
    pub pos: BlockPos,
    /// `getDefaultResult`: the result of the spawn rules.
    pub default_result: bool,
    pub result: MobSpawnResult,
}

impl MobSpawnEventSpawnPlacementCheck {
    #[must_use]
    pub const fn new(
        entity_type: &'static EntityType,
        world: Arc<World>,
        spawn_type: EntitySpawnReason,
        pos: BlockPos,
        default_result: bool,
    ) -> Self {
        Self {
            entity_type,
            world,
            spawn_type,
            pos,
            default_result,
            result: MobSpawnResult::Default,
        }
    }

    /// `getPlacementCheckResult`: whether the check passes with the current result.
    #[must_use]
    pub const fn placement_check_result(&self) -> bool {
        match self.result {
            MobSpawnResult::Succeed => true,
            MobSpawnResult::Default => self.default_result,
            MobSpawnResult::Fail => false,
        }
    }
}

/// A dead slime or magma cube splits into smaller ones.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.MobSplitEvent`, fired by
/// `EventHooks.onMobSplit` in `AbstractCubeMob.remove` before the children spawn. `children` is
/// settable: the children are new entities that are not in the world yet, and the server spawns
/// the list that the last handler left. A cancel spawns no child.
#[cancellable]
#[derive(Event, Clone)]
pub struct MobSplitEvent {
    pub parent: Arc<dyn EntityBase>,
    pub children: Vec<Arc<dyn EntityBase>>,
}

impl MobSplitEvent {
    #[must_use]
    pub fn new(parent: Arc<dyn EntityBase>, children: Vec<Arc<dyn EntityBase>>) -> Self {
        Self {
            parent,
            children,
            cancelled: false,
        }
    }
}

/// The natural spawner reads how many mobs of one pack may spawn in a chunk.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.SpawnClusterSizeEvent`, fired by
/// `EventHooks.getMaxSpawnClusterSize` in `NaturalSpawner.spawnCategoryForPosition` after each
/// spawn of the pack. `size` is settable and starts at the limit of the entity type.
#[derive(Event, Clone)]
pub struct SpawnClusterSizeEvent {
    /// The mob that spawned last.
    pub entity: Arc<dyn EntityBase>,
    pub size: i32,
}

impl SpawnClusterSizeEvent {
    #[must_use]
    pub fn new(entity: Arc<dyn EntityBase>, size: i32) -> Self {
        Self { entity, size }
    }
}

/// An entity checks whether a damage can hurt it.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.EntityInvulnerabilityCheckEvent`, fired by
/// `CommonHooks.isEntityInvulnerableTo` at the end of `Entity.isInvulnerableToBase`, which the
/// damage of living entities, item entities, item frames, paintings and end crystals reads.
/// `invulnerable` is settable and starts at the vanilla result.
#[derive(Event, Clone)]
pub struct EntityInvulnerabilityCheckEvent {
    pub entity: Arc<dyn EntityBase>,
    /// The type of the damage source.
    pub damage_type: DamageType,
    /// `DamageSource.getEntity`: the entity that caused the damage.
    pub attacker: Option<Arc<dyn EntityBase>>,
    /// `getOriginalInvulnerability`.
    pub original_invulnerable: bool,
    /// `isInvulnerable`.
    pub invulnerable: bool,
}

impl EntityInvulnerabilityCheckEvent {
    #[must_use]
    pub fn new(
        entity: Arc<dyn EntityBase>,
        damage_type: DamageType,
        attacker: Option<Arc<dyn EntityBase>>,
        invulnerable: bool,
    ) -> Self {
        Self {
            entity,
            damage_type,
            attacker,
            original_invulnerable: invulnerable,
            invulnerable,
        }
    }
}
