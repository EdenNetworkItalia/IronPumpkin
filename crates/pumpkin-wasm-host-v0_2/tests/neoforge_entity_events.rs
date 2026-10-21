//! The entity, mob and spawn events of `NeoForge` 26.3 reach v0.2 plugins (spec
//! `neoforge-event-bus`, requirement "Wasm plugins see the same events", #138): a v0.2 handler
//! receives each event with its `NeoForge` fields, writes back the settable fields and cancels
//! it, and the host keeps the fields that `NeoForge` cannot set.
#![expect(
    clippy::panic,
    clippy::expect_used,
    reason = "a test fails on the first unexpected record"
)]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

mod guest;

use std::sync::Arc;

use guest::{drop_resource, fire, is_shared_world, plugin, resource};
use pumpkin_core::{
    entity::{EntityBase, projectile::ProjectileHit, r#type::from_type},
    plugin::level::{
        EndermanAngerEvent, EntityInvulnerabilityCheckEvent, EntityLeaveLevelEvent,
        EntityMobGriefingEvent, EntitySpawnReason, EntityStruckByLightningEvent,
        EntityTeleportEventSpreadPlayersCommand, MobDespawnEvent, MobDespawnResult,
        MobSpawnEventPositionCheck, MobSpawnEventSpawnPlacementCheck, MobSpawnResult,
        MobSplitEvent, ProjectileImpactEvent, SpawnClusterSizeEvent,
    },
};
use pumpkin_data::{BlockDirection, damage::DamageType, entity::EntityType};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use pumpkin_wasm_host_v0_2::pumpkin::plugin::{
    common::BlockPos as WitBlockPos,
    damage_types::DamageType as WitDamageType,
    event::Event,
    neoforge_events::{
        EntitySpawnReason as WitEntitySpawnReason, MobDespawnResult as WitMobDespawnResult,
        MobSpawnResult as WitMobSpawnResult, ProjectileHit as WitProjectileHit,
    },
    world::BlockDirection as WitBlockDirection,
};

fn create(entity_type: &'static EntityType, x: f64) -> Arc<dyn EntityBase> {
    from_type(
        entity_type,
        Vector3::new(x, 70.0, 0.5),
        &support::world(),
        uuid::Uuid::new_v4(),
    )
}

const fn native(pos: &WitBlockPos) -> BlockPos {
    BlockPos::new(pos.x, pos.y, pos.z)
}

fn id(entity: &Arc<dyn EntityBase>) -> i32 {
    entity.get_entity().entity_id
}

#[test]
fn a_plugin_receives_an_entity_that_leaves_its_world() {
    let pig = create(&EntityType::PIG, 0.5);
    let pig_id = id(&pig);
    let manager = plugin::<EntityLeaveLevelEvent, _>(move |event, state| {
        let Event::EntityLeaveLevelEvent(data) = event else {
            panic!("expected entity leave level");
        };
        assert_eq!(resource(state, &data.entity).get_entity().entity_id, pig_id);
        assert!(is_shared_world(state, &data.target_world));
    });

    let event = fire(
        &manager,
        EntityLeaveLevelEvent::new(pig.clone(), support::world()),
    );

    assert!(Arc::ptr_eq(&event.entity, &pig));
}

#[test]
fn a_plugin_refuses_mob_griefing() {
    let zombie = create(&EntityType::ZOMBIE, 0.5);
    let manager = plugin::<EntityMobGriefingEvent, _>(|event, _state| {
        let Event::EntityMobGriefingEvent(data) = event else {
            panic!("expected entity mob griefing");
        };
        assert!(data.mob_griefing_enabled);
        assert!(data.can_grief);
        data.can_grief = false;
        data.mob_griefing_enabled = false;
    });

    let event = fire(&manager, EntityMobGriefingEvent::new(zombie, true));

    assert!(!event.can_grief);
    assert!(event.mob_griefing_enabled, "the game rule has no setter");
}

#[test]
fn a_plugin_spares_an_entity_from_lightning() {
    let pig = create(&EntityType::PIG, 0.5);
    let bolt = create(&EntityType::LIGHTNING_BOLT, 0.5);
    let (pig_id, bolt_id) = (id(&pig), id(&bolt));
    let manager = plugin::<EntityStruckByLightningEvent, _>(move |event, state| {
        let Event::EntityStruckByLightningEvent(data) = event else {
            panic!("expected entity struck by lightning");
        };
        assert_eq!(resource(state, &data.entity).get_entity().entity_id, pig_id);
        assert_eq!(
            resource(state, &data.lightning).get_entity().entity_id,
            bolt_id
        );
        data.cancelled = true;
    });

    let event = fire(&manager, EntityStruckByLightningEvent::new(pig, bolt));

    assert!(event.cancelled);
}

#[test]
fn a_plugin_moves_the_target_of_spread_players() {
    let pig = create(&EntityType::PIG, 0.5);
    let manager = plugin::<EntityTeleportEventSpreadPlayersCommand, _>(|event, state| {
        let Event::EntityTeleportEventSpreadPlayersCommand(data) = event else {
            panic!("expected spread players command");
        };
        assert!(is_shared_world(state, &data.target_world));
        assert_eq!(data.target, (10.5, 64.0, -3.5));
        data.target = (1.5, 100.0, 2.5);
    });

    let event = fire(
        &manager,
        EntityTeleportEventSpreadPlayersCommand::new(
            pig,
            support::world(),
            Vector3::new(10.5, 64.0, -3.5),
        ),
    );

    assert_eq!(event.target, Vector3::new(1.5, 100.0, 2.5));
    assert!(!event.cancelled);
}

#[test]
fn a_plugin_cannot_send_an_entity_to_a_position_that_is_not_finite() {
    let pig = create(&EntityType::PIG, 0.5);
    let manager = plugin::<EntityTeleportEventSpreadPlayersCommand, _>(|event, _state| {
        let Event::EntityTeleportEventSpreadPlayersCommand(data) = event else {
            panic!("expected spread players command");
        };
        data.target = (f64::NAN, 100.0, f64::INFINITY);
        data.cancelled = true;
    });

    let event = fire(
        &manager,
        EntityTeleportEventSpreadPlayersCommand::new(
            pig,
            support::world(),
            Vector3::new(10.5, 64.0, -3.5),
        ),
    );

    assert_eq!(event.target, Vector3::new(10.5, 64.0, -3.5));
    assert!(event.cancelled);
}

#[test]
fn a_plugin_receives_the_block_and_the_entity_of_a_projectile_impact() {
    let snowball = create(&EntityType::SNOWBALL, 0.5);
    let pig = create(&EntityType::PIG, 2.5);
    let pig_id = id(&pig);
    let manager = plugin::<ProjectileImpactEvent, _>(move |event, state| {
        let Event::ProjectileImpactEvent(data) = event else {
            panic!("expected projectile impact");
        };
        match &data.ray {
            WitProjectileHit::Block(hit) => {
                assert_eq!(native(&hit.block_pos), BlockPos::new(1, 69, 0));
                assert_eq!(hit.face, WitBlockDirection::Up);
                assert_eq!(hit.location, (1.5, 70.0, 0.5));
            }
            WitProjectileHit::Entity(hit) => {
                assert_eq!(resource(state, &hit.entity).get_entity().entity_id, pig_id);
                assert_eq!(hit.location, (2.5, 70.5, 0.5));
            }
        }
        data.cancelled = true;
    });

    let block = fire(
        &manager,
        ProjectileImpactEvent::new(
            snowball.clone(),
            ProjectileHit::Block {
                pos: BlockPos::new(1, 69, 0),
                face: BlockDirection::Up,
                hit_pos: Vector3::new(1.5, 70.0, 0.5),
                normal: Vector3::new(0.0, 1.0, 0.0),
            },
        ),
    );
    let entity = fire(
        &manager,
        ProjectileImpactEvent::new(
            snowball,
            ProjectileHit::Entity {
                entity: pig,
                hit_pos: Vector3::new(2.5, 70.5, 0.5),
                normal: Vector3::new(-1.0, 0.0, 0.0),
            },
        ),
    );

    assert!(block.cancelled);
    assert!(entity.cancelled);
}

#[test]
fn a_plugin_suppresses_the_anger_of_an_enderman() {
    let enderman = create(&EntityType::ENDERMAN, 0.5);
    let player = support::player("Starer");
    let manager = plugin::<EndermanAngerEvent, _>(|event, state| {
        let Event::EndermanAngerEvent(data) = event else {
            panic!("expected enderman anger");
        };
        assert_eq!(
            resource(state, &data.player).gameprofile.name.as_str(),
            "Starer"
        );
        data.cancelled = true;
    });

    let event = fire(&manager, EndermanAngerEvent::new(enderman, player));

    assert!(event.cancelled);
}

#[test]
fn a_plugin_sets_the_result_of_a_despawn() {
    let zombie = create(&EntityType::ZOMBIE, 0.5);
    let manager = plugin::<MobDespawnEvent, _>(|event, state| {
        let Event::MobDespawnEvent(data) = event else {
            panic!("expected mob despawn");
        };
        assert!(is_shared_world(state, &data.target_world));
        assert_eq!(data.position, (0.5, 70.0, 0.5));
        assert_eq!(data.result, WitMobDespawnResult::Default);
        data.result = WitMobDespawnResult::Deny;
        data.position = (9.0, 9.0, 9.0);
    });

    let event = fire(
        &manager,
        MobDespawnEvent::new(zombie, support::world(), Vector3::new(0.5, 70.0, 0.5)),
    );

    assert_eq!(event.result, MobDespawnResult::Deny);
    assert_eq!(
        event.pos,
        Vector3::new(0.5, 70.0, 0.5),
        "the position has no setter"
    );
}

#[test]
fn a_plugin_fails_a_spawn_position_check() {
    let zombie = create(&EntityType::ZOMBIE, 0.5);
    let manager = plugin::<MobSpawnEventPositionCheck, _>(|event, state| {
        let Event::MobSpawnEventPositionCheck(data) = event else {
            panic!("expected position check");
        };
        assert!(is_shared_world(state, &data.target_world));
        assert_eq!(data.position, (0.5, 70.0, 0.5));
        assert_eq!(data.spawn_type, WitEntitySpawnReason::Spawner);
        assert_eq!(
            data.spawner.as_ref().map(native),
            Some(BlockPos::new(3, 64, 3))
        );
        assert_eq!(data.result, WitMobSpawnResult::Default);
        data.result = WitMobSpawnResult::Fail;
    });

    let event = fire(
        &manager,
        MobSpawnEventPositionCheck::new(
            zombie,
            support::world(),
            EntitySpawnReason::Spawner,
            Some(BlockPos::new(3, 64, 3)),
        ),
    );

    assert_eq!(event.result, MobSpawnResult::Fail);
}

#[test]
fn a_plugin_passes_a_spawn_placement_check() {
    let manager = plugin::<MobSpawnEventSpawnPlacementCheck, _>(|event, state| {
        let Event::MobSpawnEventSpawnPlacementCheck(data) = event else {
            panic!("expected spawn placement check");
        };
        assert!(is_shared_world(state, &data.target_world));
        assert_eq!(data.entity_type, "minecraft:zombie");
        assert_eq!(data.spawn_type, WitEntitySpawnReason::ChunkGeneration);
        assert_eq!(native(&data.block_pos), BlockPos::new(1, 64, 2));
        assert!(!data.default_result);
        data.result = WitMobSpawnResult::Succeed;
        data.default_result = true;
    });

    let event = fire(
        &manager,
        MobSpawnEventSpawnPlacementCheck::new(
            &EntityType::ZOMBIE,
            support::world(),
            EntitySpawnReason::ChunkGeneration,
            BlockPos::new(1, 64, 2),
            false,
        ),
    );

    assert_eq!(event.result, MobSpawnResult::Succeed);
    assert!(!event.default_result, "the default result has no setter");
    assert!(event.placement_check_result());
}

#[test]
fn a_plugin_keeps_only_children_of_the_split() {
    let parent = create(&EntityType::SLIME, 0.5);
    let first = create(&EntityType::SLIME, 1.5);
    let second = create(&EntityType::SLIME, 2.5);
    let second_id = id(&second);
    let manager = plugin::<MobSplitEvent, _>(move |event, state| {
        let Event::MobSplitEvent(data) = event else {
            panic!("expected mob split");
        };
        assert_eq!(data.children.len(), 2);
        let dropped = data.children.remove(0);
        drop_resource(state, &dropped);
        // The parent is not a child, so it cannot spawn as one.
        let parent = resource(state, &data.parent);
        let foreign = state.add(parent).expect("resource");
        data.children.push(foreign);
    });

    let event = fire(
        &manager,
        MobSplitEvent::new(parent.clone(), vec![first, second]),
    );

    let ids: Vec<_> = event.children.iter().map(id).collect();
    assert_eq!(ids, [second_id]);
    assert!(Arc::ptr_eq(&event.parent, &parent));
}

#[test]
fn a_plugin_cancels_a_split() {
    let parent = create(&EntityType::MAGMA_CUBE, 0.5);
    let child = create(&EntityType::MAGMA_CUBE, 1.5);
    let manager = plugin::<MobSplitEvent, _>(|event, _state| {
        let Event::MobSplitEvent(data) = event else {
            panic!("expected mob split");
        };
        data.cancelled = true;
    });

    let event = fire(&manager, MobSplitEvent::new(parent, vec![child]));

    assert!(event.cancelled);
    assert_eq!(event.children.len(), 1);
}

#[test]
fn a_plugin_sets_the_size_of_a_spawn_cluster() {
    let pig = create(&EntityType::PIG, 0.5);
    let manager = plugin::<SpawnClusterSizeEvent, _>(|event, _state| {
        let Event::SpawnClusterSizeEvent(data) = event else {
            panic!("expected spawn cluster size");
        };
        assert_eq!(data.size, 4);
        data.size = 1;
    });

    let event = fire(&manager, SpawnClusterSizeEvent::new(pig, 4));

    assert_eq!(event.size, 1);
}

#[test]
fn a_plugin_makes_an_entity_invulnerable() {
    let pig = create(&EntityType::PIG, 0.5);
    let attacker = create(&EntityType::ZOMBIE, 1.5);
    let attacker_id = id(&attacker);
    let manager = plugin::<EntityInvulnerabilityCheckEvent, _>(move |event, state| {
        let Event::EntityInvulnerabilityCheckEvent(data) = event else {
            panic!("expected invulnerability check");
        };
        assert_eq!(data.damage_type, WitDamageType::MobAttack);
        let attacker = data.attacker.as_ref().expect("an attacker");
        assert_eq!(
            resource(state, attacker).get_entity().entity_id,
            attacker_id
        );
        assert!(!data.original_invulnerable);
        data.invulnerable = true;
        data.original_invulnerable = true;
    });

    let event = fire(
        &manager,
        EntityInvulnerabilityCheckEvent::new(pig, DamageType::MOB_ATTACK, Some(attacker), false),
    );

    assert!(event.invulnerable);
    assert!(
        !event.original_invulnerable,
        "the original result has no setter"
    );
}
