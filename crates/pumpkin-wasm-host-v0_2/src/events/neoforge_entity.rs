//! The v0.2 conversions of the `NeoForge` 26.3 entity, mob and spawn events (`event.entity` and
//! the mob and spawn events of `event.entity.living`).
//!
//! A blocking handler writes back only the settable fields; the other fields keep the native
//! values, as `NeoForge` has no setter for them.

use std::sync::Arc;

use pumpkin_core::{
    entity::projectile::ProjectileHit,
    plugin::{
        Context, EventPriority,
        level::{
            EndermanAngerEvent, EntityInvulnerabilityCheckEvent, EntityLeaveLevelEvent,
            EntityMobGriefingEvent, EntitySpawnReason, EntityStruckByLightningEvent,
            EntityTeleportEventSpreadPlayersCommand, MobDespawnEvent, MobDespawnResult,
            MobSpawnEventPositionCheck, MobSpawnEventSpawnPlacementCheck, MobSpawnResult,
            MobSplitEvent, ProjectileImpactEvent, SpawnClusterSizeEvent,
        },
    },
};
use pumpkin_wasm_host_common::state::PluginHostState;

use crate::{
    events::{
        ToFromWasmEvent, WasmPluginEventHandler, from_wasm_position,
        neoforge::{add, discard},
        to_wasm_block_position, to_wasm_entity_type, to_wasm_position,
    },
    living_entity::to_wit_damage_type,
    mob::to_wit_block_direction,
    pumpkin::plugin::{
        event::{Event, EventType},
        neoforge_events::{
            EndermanAngerEventData, EntityInvulnerabilityCheckEventData, EntityLeaveLevelEventData,
            EntityMobGriefingEventData, EntitySpawnReason as WitEntitySpawnReason,
            EntityStruckByLightningEventData, EntityTeleportEventSpreadPlayersCommandData,
            MobDespawnEventData, MobDespawnResult as WitMobDespawnResult,
            MobSpawnEventPositionCheckData, MobSpawnEventSpawnPlacementCheckData,
            MobSpawnResult as WitMobSpawnResult, MobSplitEventData, ProjectileBlockHit,
            ProjectileEntityHit, ProjectileHit as WitProjectileHit, ProjectileImpactEventData,
            SpawnClusterSizeEventData,
        },
    },
};

const fn to_wasm_spawn_reason(reason: EntitySpawnReason) -> WitEntitySpawnReason {
    match reason {
        EntitySpawnReason::Natural => WitEntitySpawnReason::Natural,
        EntitySpawnReason::ChunkGeneration => WitEntitySpawnReason::ChunkGeneration,
        EntitySpawnReason::Spawner => WitEntitySpawnReason::Spawner,
        EntitySpawnReason::Triggered => WitEntitySpawnReason::Triggered,
    }
}

const fn to_wasm_spawn_result(result: MobSpawnResult) -> WitMobSpawnResult {
    match result {
        MobSpawnResult::Succeed => WitMobSpawnResult::Succeed,
        MobSpawnResult::Default => WitMobSpawnResult::Default,
        MobSpawnResult::Fail => WitMobSpawnResult::Fail,
    }
}

const fn from_wasm_spawn_result(result: WitMobSpawnResult) -> MobSpawnResult {
    match result {
        WitMobSpawnResult::Succeed => MobSpawnResult::Succeed,
        WitMobSpawnResult::Default => MobSpawnResult::Default,
        WitMobSpawnResult::Fail => MobSpawnResult::Fail,
    }
}

const fn to_wasm_despawn_result(result: MobDespawnResult) -> WitMobDespawnResult {
    match result {
        MobDespawnResult::Allow => WitMobDespawnResult::Allow,
        MobDespawnResult::Default => WitMobDespawnResult::Default,
        MobDespawnResult::Deny => WitMobDespawnResult::Deny,
    }
}

const fn from_wasm_despawn_result(result: WitMobDespawnResult) -> MobDespawnResult {
    match result {
        WitMobDespawnResult::Allow => MobDespawnResult::Allow,
        WitMobDespawnResult::Default => MobDespawnResult::Default,
        WitMobDespawnResult::Deny => MobDespawnResult::Deny,
    }
}

fn to_wasm_projectile_hit(hit: &ProjectileHit, state: &mut PluginHostState) -> WitProjectileHit {
    match hit {
        ProjectileHit::Block {
            pos, face, hit_pos, ..
        } => WitProjectileHit::Block(ProjectileBlockHit {
            block_pos: to_wasm_block_position(*pos),
            face: to_wit_block_direction(*face),
            location: to_wasm_position(*hit_pos),
        }),
        ProjectileHit::Entity {
            entity, hit_pos, ..
        } => WitProjectileHit::Entity(ProjectileEntityHit {
            entity: add(state, entity.clone()),
            location: to_wasm_position(*hit_pos),
        }),
    }
}

impl ToFromWasmEvent for EntityLeaveLevelEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityLeaveLevelEvent(EntityLeaveLevelEventData {
            entity: add(state, self.entity.clone()),
            target_world: add(state, self.world.clone()),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_entity_event(&event, state);
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct EntityLeaveLevelEvent from WASM")
    }
}

impl ToFromWasmEvent for EntityMobGriefingEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityMobGriefingEvent(EntityMobGriefingEventData {
            entity: add(state, self.entity.clone()),
            mob_griefing_enabled: self.mob_griefing_enabled,
            can_grief: self.can_grief,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_entity_event(&event, state);
        if let Event::EntityMobGriefingEvent(data) = event {
            self.can_grief = data.can_grief;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct EntityMobGriefingEvent from WASM")
    }
}

impl ToFromWasmEvent for EntityStruckByLightningEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityStruckByLightningEvent(EntityStruckByLightningEventData {
            entity: add(state, self.entity.clone()),
            lightning: add(state, self.lightning.clone()),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_entity_event(&event, state);
        if let Event::EntityStruckByLightningEvent(data) = event {
            self.cancelled = data.cancelled;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct EntityStruckByLightningEvent from WASM")
    }
}

impl ToFromWasmEvent for EntityTeleportEventSpreadPlayersCommand {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityTeleportEventSpreadPlayersCommand(
            EntityTeleportEventSpreadPlayersCommandData {
                entity: add(state, self.entity.clone()),
                target_world: add(state, self.world.clone()),
                target: to_wasm_position(self.target),
                cancelled: self.cancelled,
            },
        )
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_entity_event(&event, state);
        if let Event::EntityTeleportEventSpreadPlayersCommand(data) = event {
            let target = from_wasm_position(data.target);
            // A position that is not finite would move the entity nowhere a client can follow.
            if target.x.is_finite() && target.y.is_finite() && target.z.is_finite() {
                self.target = target;
            }
            self.cancelled = data.cancelled;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct EntityTeleportEventSpreadPlayersCommand from WASM")
    }
}

impl ToFromWasmEvent for ProjectileImpactEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let ray = to_wasm_projectile_hit(&self.ray, state);
        Event::ProjectileImpactEvent(ProjectileImpactEventData {
            projectile: add(state, self.projectile.clone()),
            ray,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_entity_event(&event, state);
        if let Event::ProjectileImpactEvent(data) = event {
            self.cancelled = data.cancelled;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct ProjectileImpactEvent from WASM")
    }
}

impl ToFromWasmEvent for EndermanAngerEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EndermanAngerEvent(EndermanAngerEventData {
            entity: add(state, self.entity.clone()),
            player: add(state, self.player.clone()),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_entity_event(&event, state);
        if let Event::EndermanAngerEvent(data) = event {
            self.cancelled = data.cancelled;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct EndermanAngerEvent from WASM")
    }
}

impl ToFromWasmEvent for MobDespawnEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::MobDespawnEvent(MobDespawnEventData {
            entity: add(state, self.entity.clone()),
            target_world: add(state, self.world.clone()),
            position: to_wasm_position(self.pos),
            result: to_wasm_despawn_result(self.result),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_entity_event(&event, state);
        if let Event::MobDespawnEvent(data) = event {
            self.result = from_wasm_despawn_result(data.result);
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct MobDespawnEvent from WASM")
    }
}

impl ToFromWasmEvent for MobSpawnEventPositionCheck {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::MobSpawnEventPositionCheck(MobSpawnEventPositionCheckData {
            entity: add(state, self.entity.clone()),
            target_world: add(state, self.world.clone()),
            position: to_wasm_position(self.pos),
            spawn_type: to_wasm_spawn_reason(self.spawn_type),
            spawner: self.spawner.map(to_wasm_block_position),
            result: to_wasm_spawn_result(self.result),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_entity_event(&event, state);
        if let Event::MobSpawnEventPositionCheck(data) = event {
            self.result = from_wasm_spawn_result(data.result);
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct MobSpawnEventPositionCheck from WASM")
    }
}

impl ToFromWasmEvent for MobSpawnEventSpawnPlacementCheck {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::MobSpawnEventSpawnPlacementCheck(MobSpawnEventSpawnPlacementCheckData {
            entity_type: to_wasm_entity_type(self.entity_type),
            target_world: add(state, self.world.clone()),
            spawn_type: to_wasm_spawn_reason(self.spawn_type),
            block_pos: to_wasm_block_position(self.pos),
            default_result: self.default_result,
            result: to_wasm_spawn_result(self.result),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_entity_event(&event, state);
        if let Event::MobSpawnEventSpawnPlacementCheck(data) = event {
            self.result = from_wasm_spawn_result(data.result);
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct MobSpawnEventSpawnPlacementCheck from WASM")
    }
}

impl ToFromWasmEvent for MobSplitEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let children = self
            .children
            .iter()
            .map(|child| add(state, child.clone()))
            .collect();
        Event::MobSplitEvent(MobSplitEventData {
            parent: add(state, self.parent.clone()),
            children,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        let Event::MobSplitEvent(data) = event else {
            cleanup_neoforge_entity_event(&event, state);
            return;
        };
        discard(state, &data.parent);
        // The guest can only hand back entities it holds, which may be live ones. Only the
        // children of the event are new entities that the server may spawn.
        let mut children = Vec::with_capacity(self.children.len());
        for child in data.children {
            let Ok(child) = state.take(child) else {
                continue;
            };
            if self.children.iter().any(|own| Arc::ptr_eq(own, &child))
                && !children.iter().any(|kept| Arc::ptr_eq(kept, &child))
            {
                children.push(child);
            }
        }
        self.children = children;
        self.cancelled = data.cancelled;
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct MobSplitEvent from WASM")
    }
}

impl ToFromWasmEvent for SpawnClusterSizeEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::SpawnClusterSizeEvent(SpawnClusterSizeEventData {
            entity: add(state, self.entity.clone()),
            size: self.size,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_entity_event(&event, state);
        if let Event::SpawnClusterSizeEvent(data) = event {
            self.size = data.size;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct SpawnClusterSizeEvent from WASM")
    }
}

impl ToFromWasmEvent for EntityInvulnerabilityCheckEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let attacker = self
            .attacker
            .as_ref()
            .map(|attacker| add(state, attacker.clone()));
        Event::EntityInvulnerabilityCheckEvent(EntityInvulnerabilityCheckEventData {
            entity: add(state, self.entity.clone()),
            damage_type: to_wit_damage_type(&self.damage_type),
            attacker,
            original_invulnerable: self.original_invulnerable,
            invulnerable: self.invulnerable,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_entity_event(&event, state);
        if let Event::EntityInvulnerabilityCheckEvent(data) = event {
            self.invulnerable = data.invulnerable;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct EntityInvulnerabilityCheckEvent from WASM")
    }
}

/// Discards the resources of a returned event of this module.
pub(super) fn cleanup_neoforge_entity_event(event: &Event, state: &mut PluginHostState) {
    match event {
        Event::EntityLeaveLevelEvent(data) => {
            discard(state, &data.entity);
            discard(state, &data.target_world);
        }
        Event::EntityMobGriefingEvent(EntityMobGriefingEventData { entity, .. })
        | Event::SpawnClusterSizeEvent(SpawnClusterSizeEventData { entity, .. }) => {
            discard(state, entity);
        }
        Event::EntityStruckByLightningEvent(data) => {
            discard(state, &data.entity);
            discard(state, &data.lightning);
        }
        Event::EntityTeleportEventSpreadPlayersCommand(data) => {
            discard(state, &data.entity);
            discard(state, &data.target_world);
        }
        Event::ProjectileImpactEvent(data) => {
            discard(state, &data.projectile);
            if let WitProjectileHit::Entity(hit) = &data.ray {
                discard(state, &hit.entity);
            }
        }
        Event::EndermanAngerEvent(data) => {
            discard(state, &data.entity);
            discard(state, &data.player);
        }
        Event::MobDespawnEvent(MobDespawnEventData {
            entity,
            target_world,
            ..
        })
        | Event::MobSpawnEventPositionCheck(MobSpawnEventPositionCheckData {
            entity,
            target_world,
            ..
        }) => {
            discard(state, entity);
            discard(state, target_world);
        }
        Event::MobSpawnEventSpawnPlacementCheck(data) => discard(state, &data.target_world),
        Event::MobSplitEvent(data) => {
            discard(state, &data.parent);
            for child in &data.children {
                discard(state, child);
            }
        }
        Event::EntityInvulnerabilityCheckEvent(data) => {
            discard(state, &data.entity);
            if let Some(attacker) = &data.attacker {
                discard(state, attacker);
            }
        }
        _ => {}
    }
}

/// Registers a Wasm handler for an event of this module.
pub fn register_neoforge_entity_event(
    ctx: &Context,
    handler: &Arc<WasmPluginEventHandler>,
    priority: EventPriority,
    blocking: bool,
    event_type: EventType,
) {
    fn on<E: pumpkin_core::plugin::Payload + ToFromWasmEvent + Clone + 'static>(
        ctx: &Context,
        handler: &Arc<WasmPluginEventHandler>,
        priority: EventPriority,
        blocking: bool,
    ) {
        ctx.register_event::<E, _>(Arc::clone(handler), priority, blocking);
    }

    match event_type {
        EventType::EntityLeaveLevelEvent => {
            on::<EntityLeaveLevelEvent>(ctx, handler, priority, blocking);
        }
        EventType::EntityMobGriefingEvent => {
            on::<EntityMobGriefingEvent>(ctx, handler, priority, blocking);
        }
        EventType::EntityStruckByLightningEvent => {
            on::<EntityStruckByLightningEvent>(ctx, handler, priority, blocking);
        }
        EventType::EntityTeleportEventSpreadPlayersCommand => {
            on::<EntityTeleportEventSpreadPlayersCommand>(ctx, handler, priority, blocking);
        }
        EventType::ProjectileImpactEvent => {
            on::<ProjectileImpactEvent>(ctx, handler, priority, blocking);
        }
        EventType::EndermanAngerEvent => {
            on::<EndermanAngerEvent>(ctx, handler, priority, blocking);
        }
        EventType::MobDespawnEvent => on::<MobDespawnEvent>(ctx, handler, priority, blocking),
        EventType::MobSpawnEventPositionCheck => {
            on::<MobSpawnEventPositionCheck>(ctx, handler, priority, blocking);
        }
        EventType::MobSpawnEventSpawnPlacementCheck => {
            on::<MobSpawnEventSpawnPlacementCheck>(ctx, handler, priority, blocking);
        }
        EventType::MobSplitEvent => on::<MobSplitEvent>(ctx, handler, priority, blocking),
        EventType::SpawnClusterSizeEvent => {
            on::<SpawnClusterSizeEvent>(ctx, handler, priority, blocking);
        }
        EventType::EntityInvulnerabilityCheckEvent => {
            on::<EntityInvulnerabilityCheckEvent>(ctx, handler, priority, blocking);
        }
        _ => tracing::error!(
            "non-NeoForge entity event should not be routed to register_neoforge_entity_event"
        ),
    }
}
