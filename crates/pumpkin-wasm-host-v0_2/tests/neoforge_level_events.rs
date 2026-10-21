//! The level events of `NeoForge` 26.3 reach v0.2 plugins (spec `neoforge-event-bus`, requirement
//! "Wasm plugins see the same events", #131): a v0.2 handler receives each event with its
//! `NeoForge` fields, writes back the settable fields and cancels it, and the host keeps the
//! fields that `NeoForge` cannot set.
#![expect(clippy::panic, reason = "a test fails on the first unexpected record")]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

mod guest;

use std::{
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use guest::{drop_resource, fire, plugin, resource};
use pumpkin_core::{
    entity::r#type::from_type,
    plugin::{
        entity::entity_explode::EntityExplodeEvent,
        level::{
            ChunkWatchEventUnWatch, ExplosionEventDetonate, ExplosionKnockbackEvent,
            GameRuleChangedEvent, LevelEventPotentialSpawns,
        },
        world::chunk_load::ChunkLoad,
    },
};
use pumpkin_data::{
    biome::Biome,
    entity::{EntityType, MobCategory},
    game_rules::{GameRule, GameRuleValue},
};
use pumpkin_util::math::{position::BlockPos, vector2::Vector2, vector3::Vector3};
use pumpkin_wasm_host_v0_2::pumpkin::plugin::{
    common::BlockPos as WitBlockPos,
    event::Event,
    game_rules::{GameRule as WitGameRule, GameRuleValue as WitGameRuleValue},
    neoforge_events::{MobCategory as WitMobCategory, SpawnerData},
};

const fn native(pos: &WitBlockPos) -> BlockPos {
    BlockPos::new(pos.x, pos.y, pos.z)
}

const fn wit_pos(pos: BlockPos) -> WitBlockPos {
    WitBlockPos {
        x: pos.0.x,
        y: pos.0.y,
        z: pos.0.z,
    }
}

#[test]
fn a_plugin_receives_the_world_of_an_explosion_start_and_cancels_it() {
    let world = support::world();
    let center = Vector3::new(1.5, 70.0, 1.5);
    let manager = plugin::<EntityExplodeEvent, _>(|event, state| {
        let Event::EntityExplodeEvent(data) = event else {
            panic!("expected entity explode");
        };
        assert!(Arc::ptr_eq(
            &resource(state, &data.target_world),
            &support::world()
        ));
        assert_eq!(data.position, (1.5, 70.0, 1.5));
        data.yield_rate = 0.5;
        data.cancelled = true;
    });

    let event = fire(&manager, EntityExplodeEvent::new(0, center, 4.0, world));

    assert!(event.cancelled);
    assert!((event.yield_rate - 0.5).abs() < f32::EPSILON);
    assert!(Arc::ptr_eq(&event.world, &support::world()));
}

#[test]
fn a_plugin_changes_the_blocks_and_entities_of_a_detonation() {
    let world = support::world();
    let first = from_type(
        &EntityType::PIG,
        Vector3::new(0.5, 70.0, 0.5),
        &world,
        uuid::Uuid::new_v4(),
    );
    let second = from_type(
        &EntityType::COW,
        Vector3::new(2.5, 70.0, 0.5),
        &world,
        uuid::Uuid::new_v4(),
    );
    let first_id = first.get_entity().entity_id;
    let second_id = second.get_entity().entity_id;
    let kept = BlockPos::new(0, 69, 0);
    let removed = BlockPos::new(1, 69, 0);
    let added = BlockPos::new(2, 69, 0);
    let manager = plugin::<ExplosionEventDetonate, _>(move |event, state| {
        let Event::ExplosionEventDetonate(data) = event else {
            panic!("expected explosion detonate");
        };
        assert_eq!(data.center, (1.0, 70.0, 0.0));
        assert!((data.power - 3.0).abs() < f32::EPSILON);
        let blocks: Vec<_> = data.affected_blocks.iter().map(native).collect();
        assert_eq!(blocks, [kept, removed]);
        let ids: Vec<_> = data
            .affected_entities
            .iter()
            .map(|entity| resource(state, entity).get_entity().entity_id)
            .collect();
        assert_eq!(ids, [first_id, second_id]);
        data.affected_blocks = vec![wit_pos(kept), wit_pos(added)];
        let dropped = data.affected_entities.remove(0);
        drop_resource(state, &dropped);
    });

    let event = fire(
        &manager,
        ExplosionEventDetonate::new(
            world,
            Vector3::new(1.0, 70.0, 0.0),
            3.0,
            vec![kept, removed],
            vec![first, second],
        ),
    );

    assert_eq!(event.affected_blocks, [kept, added]);
    let ids: Vec<_> = event
        .affected_entities
        .iter()
        .map(|entity| entity.get_entity().entity_id)
        .collect();
    assert_eq!(ids, [second_id]);
}

#[test]
fn a_plugin_sets_the_knockback_of_an_explosion() {
    let world = support::world();
    let pig = from_type(
        &EntityType::PIG,
        Vector3::new(0.5, 70.0, 0.5),
        &world,
        uuid::Uuid::new_v4(),
    );
    let pig_id = pig.get_entity().entity_id;
    let manager = plugin::<ExplosionKnockbackEvent, _>(move |event, state| {
        let Event::ExplosionKnockbackEvent(data) = event else {
            panic!("expected explosion knockback");
        };
        assert_eq!(resource(state, &data.entity).get_entity().entity_id, pig_id);
        assert_eq!(data.knockback, (0.0, 1.0, 0.0));
        assert_eq!(data.affected_blocks.len(), 1);
        data.knockback = (2.0, 0.5, -2.0);
        data.power = 100.0;
    });

    let event = fire(
        &manager,
        ExplosionKnockbackEvent::new(
            world,
            Vector3::new(0.0, 69.0, 0.0),
            4.0,
            pig,
            Vector3::new(0.0, 1.0, 0.0),
            vec![BlockPos::new(0, 68, 0)],
        ),
    );

    assert_eq!(event.knockback, Vector3::new(2.0, 0.5, -2.0));
    assert!(
        (event.power - 4.0).abs() < f32::EPSILON,
        "the power has no setter"
    );
}

/// Loads the chunk `pos` for the rest of the process and returns it.
fn loaded_chunk(pos: Vector2<i32>) -> Arc<pumpkin_world::chunk::ChunkData> {
    let world = support::world();
    {
        let mut loading = world
            .level
            .chunk_loading
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        loading.add_force_ticket(pos);
        loading.send_change();
    };
    let deadline = Instant::now() + support::SLOW_RUNNER;
    loop {
        if let Some(chunk) = world.level.loaded_chunks.get(&pos) {
            return chunk.clone();
        }
        assert!(Instant::now() < deadline, "the chunk {pos:?} did not load");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_plugin_reads_the_chunk_of_a_chunk_load() {
    let chunk = loaded_chunk(Vector2::new(0, 0));
    let expected = chunk.clone();
    let manager = plugin::<ChunkLoad, _>(move |event, state| {
        let Event::ChunkLoadEvent(data) = event else {
            panic!("expected chunk load");
        };
        let (world, loaded) = resource(state, &data.chunk);
        assert!(Arc::ptr_eq(&world, &support::world()));
        assert!(
            loaded
                .upgrade()
                .is_some_and(|loaded| Arc::ptr_eq(&loaded, &expected))
        );
        assert_eq!((data.chunk_x, data.chunk_z), (0, 0));
        data.cancelled = true;
    });

    let event = fire(&manager, ChunkLoad::new(support::world(), chunk.clone()));

    assert!(event.cancelled);
    assert!(Arc::ptr_eq(&event.chunk, &chunk));
}

#[test]
fn a_plugin_receives_a_chunk_unwatch() {
    let player = support::player("Unwatcher");
    let seen = Arc::new(Mutex::new(None));
    let record = seen.clone();
    let manager = plugin::<ChunkWatchEventUnWatch, _>(move |event, state| {
        let Event::ChunkWatchEventUnWatch(data) = event else {
            panic!("expected chunk unwatch");
        };
        *record.lock().unwrap_or_else(PoisonError::into_inner) = Some((
            resource(state, &data.player).gameprofile.name.clone(),
            Arc::ptr_eq(&resource(state, &data.target_world), &support::world()),
            data.chunk_x,
            data.chunk_z,
        ));
    });

    fire(
        &manager,
        ChunkWatchEventUnWatch::new(player, support::world(), Vector2::new(3, -4)),
    );

    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        Some(("Unwatcher".to_owned(), true, 3, -4))
    );
}

fn wit_value(value: WitGameRuleValue) -> String {
    match value {
        WitGameRuleValue::Int(value) => value.to_string(),
        WitGameRuleValue::Bool(value) => value.to_string(),
    }
}

#[test]
fn a_plugin_receives_the_rule_and_value_of_a_game_rule_change() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let manager = plugin::<GameRuleChangedEvent, _>(move |event, _state| {
        let Event::GameRuleChangedEvent(data) = event else {
            panic!("expected game rule changed");
        };
        record
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((data.rule, wit_value(data.value)));
    });

    fire(
        &manager,
        GameRuleChangedEvent::new(GameRule::KeepInventory, GameRuleValue::Bool(true)),
    );
    fire(
        &manager,
        GameRuleChangedEvent::new(GameRule::RandomTickSpeed, GameRuleValue::Int(7)),
    );
    fire(
        &manager,
        GameRuleChangedEvent::new(GameRule::WaterSourceConversion, GameRuleValue::Bool(false)),
    );

    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        [
            (WitGameRule::KeepInventory, "true".to_owned()),
            (WitGameRule::RandomTickSpeed, "7".to_owned()),
            (WitGameRule::WaterSourceConversion, "false".to_owned()),
        ]
    );
}

fn spawner(entity_type: &str, min_count: i32, max_count: i32) -> SpawnerData {
    SpawnerData {
        entity_type: entity_type.to_owned(),
        min_count,
        max_count,
    }
}

#[test]
fn a_plugin_changes_and_cancels_the_potential_spawns() {
    let spawns: Vec<_> = Biome::PLAINS.spawners.creature.iter().collect();
    let first = spawns[0];
    let pos = BlockPos::new(0, 70, 0);
    let manager = plugin::<LevelEventPotentialSpawns, _>(move |event, _state| {
        let Event::LevelEventPotentialSpawns(data) = event else {
            panic!("expected potential spawns");
        };
        assert_eq!(data.category, WitMobCategory::Creature);
        assert_eq!(native(&data.block_pos), pos);
        assert_eq!(data.spawns.len(), Biome::PLAINS.spawners.creature.len());
        assert_eq!(data.spawns[0].entity_type, first.r#type);
        let kept = data.spawns.swap_remove(0);
        data.spawns = vec![
            kept,
            spawner("minecraft:goat", 1, 3),
            spawner("minecraft:not_a_mob", 1, 1),
            spawner("minecraft:goat", 1, 3),
        ];
    });

    let event = fire(
        &manager,
        LevelEventPotentialSpawns::new(
            support::world(),
            &MobCategory::CREATURE,
            pos,
            spawns.clone(),
        ),
    );

    assert!(!event.cancelled);
    assert_eq!(event.spawns.len(), 3, "the unknown entity type is dropped");
    assert!(
        std::ptr::eq(event.spawns[0], first),
        "a kept entry keeps its static"
    );
    assert_eq!(
        (
            event.spawns[1].r#type,
            event.spawns[1].min_count,
            event.spawns[1].max_count
        ),
        ("minecraft:goat", 1, 3)
    );
    assert!(
        std::ptr::eq(event.spawns[1], event.spawns[2]),
        "an added entry is leaked once"
    );

    let cancel = plugin::<LevelEventPotentialSpawns, _>(|event, _state| {
        if let Event::LevelEventPotentialSpawns(data) = event {
            data.cancelled = true;
        }
    });
    let event = fire(
        &cancel,
        LevelEventPotentialSpawns::new(support::world(), &MobCategory::CREATURE, pos, spawns),
    );
    assert!(event.cancelled);
}
