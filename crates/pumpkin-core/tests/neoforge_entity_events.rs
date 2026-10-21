//! The `NeoForge` entity, mob and spawn events (`event.entity` and the mob and spawn events of
//! `event.entity.living`, task 2.15 of `openspec/changes/neoforge-shaped-plugin-api`) as Pumpkin
//! events: one test per row that fires from its `NeoForge` fire site, with the cancel or the
//! write-back of the row, against a server of the support module.
//!
//! Each test works at its own position and filters its handlers by its own entities, because
//! the handlers stay registered on the shared server.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use pumpkin_core::{
    block::blocks::turtle_egg::TurtleEggBlock,
    command::CommandSender,
    entity::{
        EntityBase,
        mob::{enderman::EndermanEntity, slime::SlimeEntity, warden::WardenEntity},
        projectile::ProjectileHit,
        spawn_util::{SpawnStrategy, try_spawn_mob},
        r#type::{check_spawn_rules, from_type},
    },
    plugin::{
        BoxFuture, Cancellable, EventHandler, EventPriority, Payload,
        entity::{entity_spawn::EntitySpawnEvent, projectile_hit::ProjectileHitEvent},
        level::{
            EndermanAngerEvent, EntityInvulnerabilityCheckEvent, EntityLeaveLevelEvent,
            EntityMobGriefingEvent, EntitySpawnReason, EntityStruckByLightningEvent,
            EntityTeleportEventSpreadPlayersCommand, LevelEventPotentialSpawns, MobDespawnEvent,
            MobDespawnResult, MobSpawnEventPositionCheck, MobSpawnEventSpawnPlacementCheck,
            MobSpawnResult, MobSplitEvent, ProjectileImpactEvent, SpawnClusterSizeEvent,
        },
    },
    server::Server,
    world::natural_spawner::{self, SpawnState},
};
use pumpkin_data::{
    Block, BlockStateId,
    biome::Spawner,
    damage::DamageType,
    entity::{EntityType, MobCategory},
};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::{position::BlockPos, vector2::Vector2, vector3::Vector3};
use pumpkin_world::{chunk::ChunkHeightmapType, world::BlockFlags};
use uuid::Uuid;

type Log = Arc<Mutex<Vec<String>>>;

fn log() -> Log {
    Arc::new(Mutex::new(Vec::new()))
}

fn push(log: &Log, entry: impl Into<String>) {
    log.lock().unwrap().push(entry.into());
}

fn entries(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

/// A blocking handler that runs a closure on the event.
struct Blocking<E, F>(F, PhantomData<fn() -> E>);

impl<E, F> EventHandler<E> for Blocking<E, F>
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&mut E) + Send + Sync,
{
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut E,
    ) -> BoxFuture<'a, ()> {
        (self.0)(event);
        Box::pin(async {})
    }
}

/// Registers a blocking handler for `E` with priority `Normal` and without receive-canceled.
fn on<E, F>(handler: F)
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&mut E) + Send + Sync + 'static,
{
    support::server().plugin_manager.register_with::<E, _>(
        Arc::new(Blocking(handler, PhantomData)),
        EventPriority::Normal,
        true,
        false,
    );
}

/// Loads the chunk of `pos` for the rest of the process.
fn load(pos: BlockPos) {
    let world = support::world();
    let chunk = pos.chunk_position();
    {
        let mut loading = world.level.chunk_loading.lock().unwrap();
        loading.add_force_ticket(chunk);
        loading.send_change();
    };
    let deadline = Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&chunk) {
        assert!(Instant::now() < deadline, "the chunk of {pos} did not load");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Loads the chunk of `pos` and sets `pos` to `state`.
fn set(pos: BlockPos, state: BlockStateId) {
    load(pos);
    support::world().set_block_state(&pos, state, BlockFlags::empty());
}

/// A position of the test `slot`, away from the other tests. The blocks around it in x and z
/// stay in one chunk.
const fn spot(slot: i32) -> BlockPos {
    BlockPos::new(slot * 16 + 8, 120, 8)
}

fn center(pos: BlockPos) -> Vector3<f64> {
    Vector3::new(
        f64::from(pos.0.x) + 0.5,
        f64::from(pos.0.y),
        f64::from(pos.0.z) + 0.5,
    )
}

/// Creates an entity of `entity_type` at `pos`, outside the world.
fn create(entity_type: &'static EntityType, pos: Vector3<f64>) -> Arc<dyn EntityBase> {
    from_type(entity_type, pos, &support::world(), Uuid::new_v4())
}

/// Creates an entity of `entity_type` at `pos` and spawns it.
fn spawn(entity_type: &'static EntityType, pos: Vector3<f64>) -> Arc<dyn EntityBase> {
    let entity = create(entity_type, pos);
    assert!(support::world().spawn_entity(entity.clone()));
    entity
}

fn id(entity: &Arc<dyn EntityBase>) -> i32 {
    entity.get_entity().entity_id
}

fn is_live(entity: &Arc<dyn EntityBase>) -> bool {
    support::world()
        .get_entity_by_uuid(entity.get_entity().entity_uuid)
        .is_some()
}

/// `EntityLeaveLevelEvent` fires when an entity leaves its world, after it left the entity list.
#[test]
fn one_event_per_fire_site_entity_leave_level_event() {
    let _serial = support::serial();
    let pos = spot(1);
    load(pos);
    let removed = spawn(&EntityType::PIG, center(pos));
    let unloaded_pos = spot(17);
    load(unloaded_pos);
    let unloaded = spawn(&EntityType::COW, center(unloaded_pos));
    let player = support::player("Leaver");
    let ids = [id(&removed), id(&unloaded), player.entity_id()];
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<EntityLeaveLevelEvent, _>(move |event| {
        let entity_id = event.entity.get_entity().entity_id;
        if ids.contains(&entity_id) {
            push(
                &seen_by_handler,
                format!(
                    "{entity_id} {} {}",
                    Arc::ptr_eq(&event.world, &support::world()),
                    event.world.get_entity_by_id(entity_id).is_some()
                ),
            );
        }
    });
    let world = support::world();

    removed.get_entity().remove();
    support::RUNTIME.block_on(async {
        world
            .remove_entities_in_chunks([unloaded_pos.chunk_position()])
            .await;
        world.remove_player(&player, false).await;
    });

    assert_eq!(
        entries(&seen),
        ids.map(|entity_id| format!("{entity_id} true false"))
    );
}

/// The record of a pig as its chunk saves it.
fn pig_record(pos: BlockPos) -> (Arc<dyn EntityBase>, NbtCompound) {
    let pig = create(&EntityType::PIG, center(pos));
    let mut nbt = NbtCompound::new();
    pig.write_nbt(&mut nbt);
    (pig, nbt)
}

/// `EntityJoinLevelEvent` (`EntitySpawnEvent`) fires for an entity that loads from its chunk,
/// with `loaded_from_disk`, and the entity goes live.
#[test]
fn one_event_per_fire_site_entity_join_level_event_on_load() {
    let pos = spot(18);
    load(pos);
    let (pig, record) = pig_record(pos);
    let pig_uuid = pig.get_entity().entity_uuid;
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<EntitySpawnEvent, _>(move |event| {
        if event.position == center(pos) {
            push(&seen_by_handler, format!("{}", event.loaded_from_disk));
        }
    });

    let loaded = support::world().load_chunk_entity(&record).unwrap();

    assert_eq!(entries(&seen), ["true"]);
    assert_eq!(loaded.get_entity().entity_uuid, pig_uuid);
    assert!(is_live(&loaded));
}

/// A cancelled `EntitySpawnEvent` of a loaded entity keeps it out of the world.
#[test]
fn cancel_entity_join_level_event_on_load() {
    let pos = spot(19);
    load(pos);
    let (pig, record) = pig_record(pos);
    on::<EntitySpawnEvent, _>(move |event| {
        if event.position == center(pos) && event.loaded_from_disk {
            event.set_cancelled(true);
        }
    });

    let loaded = support::world().load_chunk_entity(&record);

    assert!(loaded.is_none());
    assert!(!is_live(&pig));
}

/// `EntityJoinLevelEvent` (`EntitySpawnEvent`) fires for each rider of a mob that spawns, as
/// vanilla `addFreshEntityWithPassengers` adds each rider, and a cancel leaves the rider out.
#[test]
fn cancel_entity_join_level_event_of_a_rider() {
    let pos = spot(2);
    load(pos);
    let spider = create(&EntityType::SPIDER, center(pos));
    let rider = create(&EntityType::SKELETON, center(pos));
    let rider_id = id(&rider);
    spider
        .get_mob()
        .unwrap()
        .get_mob_entity()
        .add_pending_rider(rider.clone());
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<EntitySpawnEvent, _>(move |event| {
        if event.entity_id == rider_id {
            push(
                &seen_by_handler,
                format!(
                    "{}",
                    event.entity_type == EntityType::SKELETON.id.to_string()
                ),
            );
            event.set_cancelled(true);
        }
    });

    assert!(support::world().spawn_entity(spider.clone()));

    assert_eq!(entries(&seen), ["true"]);
    assert!(is_live(&spider));
    assert!(!is_live(&rider));
    assert!(spider.get_entity().passengers.lock().unwrap().is_empty());
}

/// `EntityMobGriefingEvent` fires where vanilla reads `mobGriefing` for an entity, such as the
/// turtle egg trample, with the game rule, and `can_grief` decides.
#[test]
fn change_a_settable_field_entity_mob_griefing_event() {
    let pos = spot(3);
    load(pos);
    let zombie = spawn(&EntityType::ZOMBIE, center(pos));
    let zombie_id = id(&zombie);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<EntityMobGriefingEvent, _>(move |event| {
        if event.entity.get_entity().entity_id == zombie_id {
            push(
                &seen_by_handler,
                format!("{} {}", event.mob_griefing_enabled, event.can_grief),
            );
            event.can_grief = !event.mob_griefing_enabled;
        }
    });
    let world = support::world();
    let rule = world.level_info.load().game_rules.mob_griefing;

    let can_destroy = TurtleEggBlock::can_destroy_egg(&world, zombie.as_ref());

    assert_eq!(entries(&seen), [format!("{rule} {rule}")]);
    assert_eq!(can_destroy, !rule);
}

/// `EntityStruckByLightningEvent` fires before a bolt strikes an entity, with the bolt, and a
/// cancel spares the entity.
#[test]
fn cancel_entity_struck_by_lightning_event() {
    let pos = spot(4);
    load(pos);
    let pig = spawn(&EntityType::PIG, center(pos));
    let pig_id = id(&pig);
    let bolt = spawn(&EntityType::LIGHTNING_BOLT, center(pos));
    let bolt_id = id(&bolt);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<EntityStruckByLightningEvent, _>(move |event| {
        if event.entity.get_entity().entity_id == pig_id {
            push(
                &seen_by_handler,
                format!("{}", event.lightning.get_entity().entity_id == bolt_id),
            );
            event.set_cancelled(true);
        }
    });

    bolt.tick(bolt.as_ref(), support::server());

    assert_eq!(entries(&seen), ["true"]);
    assert!(is_live(&pig));
    assert!(!pig.get_entity().is_on_fire());
}

/// Runs `/spreadplayers` around `pos` for `pig`. The top block of each column of the area turns
/// to stone, so no pile lands on a liquid.
fn spread(pig: &Arc<dyn EntityBase>, pos: BlockPos) {
    let world = support::world();
    for x in -2..=2 {
        for z in -2..=2 {
            let (x, z) = (pos.0.x + x, pos.0.z + z);
            let top = world.get_heightmap_height(ChunkHeightmapType::WorldSurface, x, z);
            world.set_block_state(
                &BlockPos::new(x, top, z),
                Block::STONE.default_state.id,
                BlockFlags::empty(),
            );
        }
    }
    let server = support::server();
    let source = CommandSender::Console.into_source(server).with_silent();
    server.command_dispatcher.load().handle_command(
        &source,
        &format!(
            "spreadplayers {} {} 0 1 false {}",
            pos.0.x,
            pos.0.z,
            pig.get_entity().entity_uuid
        ),
    );
}

/// `EntityTeleportEvent.SpreadPlayersCommand` fires before `/spreadplayers` moves an entity,
/// and the entity goes to the target of the handler.
#[test]
fn change_a_settable_field_entity_teleport_event_spread_players_command() {
    let pos = spot(5);
    load(pos);
    let pig = spawn(&EntityType::PIG, center(pos));
    let pig_id = id(&pig);
    let target = Vector3::new(f64::from(pos.0.x) + 0.5, 200.0, 8.5);
    on::<EntityTeleportEventSpreadPlayersCommand, _>(move |event| {
        if event.entity.get_entity().entity_id == pig_id {
            assert!(Arc::ptr_eq(&event.world, &support::world()));
            assert!((event.target.x - (f64::from(pos.0.x) + 0.5)).abs() <= 1.0);
            event.target = target;
        }
    });

    spread(&pig, pos);

    assert_eq!(pig.get_entity().pos.load(), target);
}

/// A cancelled `EntityTeleportEventSpreadPlayersCommand` leaves the entity where it is.
#[test]
fn cancel_entity_teleport_event_spread_players_command() {
    let pos = spot(6);
    load(pos);
    let start = center(pos);
    let pig = spawn(&EntityType::PIG, start);
    let pig_id = id(&pig);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<EntityTeleportEventSpreadPlayersCommand, _>(move |event| {
        if event.entity.get_entity().entity_id == pig_id {
            push(&seen_by_handler, "spread");
            event.set_cancelled(true);
        }
    });

    spread(&pig, pos);

    assert_eq!(entries(&seen), ["spread"]);
    assert_eq!(pig.get_entity().pos.load(), start);
}

/// `ProjectileImpactEvent` fires before a projectile handles a hit, with the hit, and a cancel
/// keeps the projectile flying.
#[test]
fn cancel_projectile_impact_event() {
    let pos = spot(7);
    set(pos, Block::STONE.default_state.id);
    let snowball = spawn(
        &EntityType::SNOWBALL,
        center(pos.up()) + Vector3::new(0.0, 0.5, 0.0),
    );
    let snowball_id = id(&snowball);
    snowball
        .get_entity()
        .velocity
        .store(Vector3::new(0.0, -1.0, 0.0));
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<ProjectileImpactEvent, _>(move |event| {
        if event.projectile.get_entity().entity_id == snowball_id {
            let ProjectileHit::Block { pos: hit, .. } = &event.ray else {
                panic!("expected a block hit");
            };
            push(&seen_by_handler, format!("{}", *hit == pos));
            event.set_cancelled(true);
        }
    });

    snowball.tick(snowball.as_ref(), support::server());

    assert_eq!(entries(&seen), ["true"]);
    assert!(is_live(&snowball));
}

/// `EndermanAngerEvent` fires when a player stares at an enderman, and a cancel makes the stare
/// not count.
#[test]
fn cancel_enderman_anger_event() {
    let _serial = support::serial();
    let pos = spot(8);
    load(pos);
    let enderman = spawn(
        &EntityType::ENDERMAN,
        center(pos) + Vector3::new(0.0, 0.0, 5.0),
    );
    let enderman_id = id(&enderman);
    let player = support::player("Starer");
    let player_pos = center(pos);
    player.get_entity().set_pos(player_pos);
    // Look at the eyes of the enderman (2.55 above its feet) from the eyes of the player.
    let rise = 2.55 - 1.62;
    let pitch = -(rise / f64::hypot(rise, 5.0)).asin().to_degrees();
    player.get_entity().set_rotation(0.0, pitch as f32);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<EndermanAngerEvent, _>(move |event| {
        if event.entity.get_entity().entity_id == enderman_id {
            push(&seen_by_handler, event.player.gameprofile.name.clone());
            event.set_cancelled(true);
        }
    });

    let staring = enderman
        .cast_any()
        .downcast_ref::<EndermanEntity>()
        .unwrap()
        .is_player_staring(&player);

    assert_eq!(entries(&seen), ["Starer"]);
    assert!(!staring);
}

/// `MobDespawnEvent` fires at the start of the despawn check, and `Allow` despawns the mob
/// without a player in range.
#[test]
fn change_a_settable_field_mob_despawn_event() {
    let _serial = support::serial();
    let pos = spot(9);
    load(pos);
    let zombie = spawn(&EntityType::ZOMBIE, center(pos));
    let zombie_id = id(&zombie);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<MobDespawnEvent, _>(move |event| {
        if event.entity.get_entity().entity_id == zombie_id {
            push(&seen_by_handler, format!("{}", event.pos == center(pos)));
            event.result = MobDespawnResult::Allow;
        }
    });

    let mob = zombie.get_mob().unwrap();
    mob.get_mob_entity().check_despawn(mob);

    assert_eq!(entries(&seen), ["true"]);
    assert!(!is_live(&zombie));
}

fn check_despawn(mob: &Arc<dyn EntityBase>) {
    let mob = mob.get_mob().unwrap();
    mob.get_mob_entity().check_despawn(mob);
}

/// Scenario "Deny a despawn": a mob far from the only player despawns, and the same mob with a
/// `Deny` result stays.
#[test]
fn deny_mob_despawn_event() {
    let _serial = support::serial();
    let pos = spot(16);
    load(pos);
    let denied = spawn(&EntityType::ZOMBIE, center(pos));
    let vanilla = spawn(&EntityType::ZOMBIE, center(pos));
    let denied_id = id(&denied);
    on::<MobDespawnEvent, _>(move |event| {
        if event.entity.get_entity().entity_id == denied_id {
            event.result = MobDespawnResult::Deny;
        }
    });
    let player = support::player("Far");
    player
        .get_entity()
        .set_pos(center(pos) + Vector3::new(200.0, 0.0, 0.0));

    check_despawn(&denied);
    check_despawn(&vanilla);

    assert!(is_live(&denied));
    assert!(!is_live(&vanilla));
}

/// `MobSpawnEvent.PositionCheck` fires for the summon of a warden with its spawn type, and
/// `Fail` stops the spawn.
#[test]
fn change_a_settable_field_mob_spawn_event_position_check() {
    let pos = spot(10);
    set(pos, Block::STONE.default_state.id);
    // The world seed is random: terrain in the search column moves the spawn or stops it.
    set(pos.up(), Block::AIR.default_state.id);
    set(pos.up_height(2), Block::AIR.default_state.id);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<MobSpawnEventPositionCheck, _>(move |event| {
        if event.entity.get_entity().entity_type == &EntityType::WARDEN
            && event.pos == center(pos.up())
        {
            push(
                &seen_by_handler,
                format!("{:?} {:?}", event.spawn_type, event.spawner),
            );
            event.result = MobSpawnResult::Fail;
        }
    });

    let warden = try_spawn_mob(
        &EntityType::WARDEN,
        WardenEntity::new,
        EntitySpawnReason::Triggered,
        &support::world(),
        &pos.up(),
        1,
        0,
        1,
        SpawnStrategy::OnTopOfCollider,
        false,
    );

    assert_eq!(entries(&seen), ["Triggered None"]);
    assert!(warden.is_none());
}

/// `MobSpawnEvent.SpawnPlacementCheck` fires at the end of the spawn rules with their result,
/// and the result of the handler wins.
#[test]
fn change_a_settable_field_mob_spawn_event_spawn_placement_check() {
    let pos = spot(11);
    load(pos);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<MobSpawnEventSpawnPlacementCheck, _>(move |event| {
        if event.pos == pos && event.entity_type == &EntityType::ZOMBIE {
            push(
                &seen_by_handler,
                format!("{:?} {}", event.spawn_type, event.default_result),
            );
            event.result = if event.default_result {
                MobSpawnResult::Fail
            } else {
                MobSpawnResult::Succeed
            };
        }
    });

    let passes = check_spawn_rules(
        &EntityType::ZOMBIE,
        &support::world(),
        EntitySpawnReason::Natural,
        &pos,
        false,
    );

    // The handler turns the result of the spawn rules around.
    assert_eq!(entries(&seen), [format!("Natural {}", !passes)]);
}

/// The children of a dead slime.
fn split(slot: i32) -> (Arc<dyn EntityBase>, Log) {
    let pos = spot(slot);
    load(pos);
    let slime = create(&EntityType::SLIME, center(pos));
    slime
        .cast_any()
        .downcast_ref::<SlimeEntity>()
        .unwrap()
        .set_size(2, true);
    assert!(support::world().spawn_entity(slime.clone()));
    let seen = log();
    (slime, seen)
}

fn kill_and_split(slime: &Arc<dyn EntityBase>) {
    slime
        .get_living_entity()
        .unwrap()
        .dead
        .store(true, std::sync::atomic::Ordering::Relaxed);
    slime.get_mob().unwrap().post_tick();
}

/// `MobSplitEvent` fires before the children of a dead slime spawn, and the server spawns the
/// children that the handler left.
#[test]
fn change_a_settable_field_mob_split_event() {
    let (slime, seen) = split(12);
    let slime_id = id(&slime);
    let seen_by_handler = seen.clone();
    let kept = Arc::new(Mutex::new(Vec::new()));
    let kept_by_handler = kept.clone();
    on::<MobSplitEvent, _>(move |event| {
        if event.parent.get_entity().entity_id == slime_id {
            push(
                &seen_by_handler,
                format!(
                    "{} {}",
                    (2..=4).contains(&event.children.len()),
                    event.children.iter().all(|child| !is_live(child))
                ),
            );
            event.children.truncate(1);
            *kept_by_handler.lock().unwrap() = event.children.clone();
        }
    });

    kill_and_split(&slime);

    assert_eq!(entries(&seen), ["true true"]);
    let kept = kept.lock().unwrap();
    assert_eq!(kept.len(), 1);
    assert!(is_live(&kept[0]));
}

/// A cancelled `MobSplitEvent` spawns no child.
#[test]
fn cancel_mob_split_event() {
    let (slime, seen) = split(13);
    let slime_id = id(&slime);
    let seen_by_handler = seen.clone();
    let children = Arc::new(Mutex::new(Vec::new()));
    let children_by_handler = children.clone();
    on::<MobSplitEvent, _>(move |event| {
        if event.parent.get_entity().entity_id == slime_id {
            push(&seen_by_handler, "split");
            *children_by_handler.lock().unwrap() = event.children.clone();
            event.set_cancelled(true);
        }
    });

    kill_and_split(&slime);

    assert_eq!(entries(&seen), ["split"]);
    assert!(!children.lock().unwrap().iter().any(is_live));
}

/// A pig spawn entry of the test.
static PIG_PACK: Spawner = Spawner {
    r#type: "minecraft:pig",
    min_count: 4,
    max_count: 4,
};

/// `SpawnClusterSizeEvent` fires after each natural spawn of a pack with the limit of the
/// entity type, and the spawner stops the pack at the size of the handler.
#[test]
fn change_a_settable_field_spawn_cluster_size_event() {
    let _serial = support::serial();
    let pos = spot(40);
    load(pos);
    let world = support::world();
    let floor = pos.down();
    for x in 0..16 {
        for z in 0..16 {
            let column = BlockPos::new(floor.0.x - 8 + x, floor.0.y, floor.0.z - 8 + z);
            world.set_block_state(
                &column,
                Block::GRASS_BLOCK.default_state.id,
                BlockFlags::empty(),
            );
        }
    }
    let player = support::player("Herder");
    player
        .get_entity()
        .set_pos(center(pos) + Vector3::new(40.0, 0.0, 0.0));
    let near = move |at: &BlockPos| at.chunk_position() == pos.chunk_position();
    on::<LevelEventPotentialSpawns, _>(move |event| {
        if near(&event.pos) {
            event.spawns = vec![&PIG_PACK];
        }
    });
    on::<MobSpawnEventSpawnPlacementCheck, _>(move |event| {
        if near(&event.pos) {
            event.result = MobSpawnResult::Succeed;
        }
    });
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<SpawnClusterSizeEvent, _>(move |event| {
        if near(&event.entity.get_entity().block_pos.load()) {
            push(&seen_by_handler, format!("{}", event.size));
            event.size = 1;
        }
    });

    let spawned = natural_spawner::spawn_category_for_position(
        &MobCategory::CREATURE,
        &world,
        pos,
        &Vector2::new(pos.chunk_position().x, pos.chunk_position().y),
        &SpawnState::empty(),
        false,
    );

    assert_eq!(spawned.len(), 1);
    assert_eq!(
        entries(&seen),
        [EntityType::PIG.limit_per_chunk.to_string()]
    );
}

/// `EntityInvulnerabilityCheckEvent` fires in the damage of a living entity with the vanilla
/// result, and `invulnerable` decides.
#[test]
fn change_a_settable_field_entity_invulnerability_check_event() {
    let pos = spot(14);
    load(pos);
    let pig = spawn(&EntityType::PIG, center(pos));
    let pig_id = id(&pig);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<EntityInvulnerabilityCheckEvent, _>(move |event| {
        if event.entity.get_entity().entity_id == pig_id {
            push(
                &seen_by_handler,
                format!(
                    "{} {}",
                    event.damage_type == DamageType::GENERIC,
                    event.original_invulnerable
                ),
            );
            event.invulnerable = true;
        }
    });
    let health = pig.get_living_entity().unwrap().health.load();

    let damaged = pig.damage(pig.as_ref(), 1.0, DamageType::GENERIC);

    assert_eq!(entries(&seen), ["true false"]);
    assert!(!damaged);
    assert!((pig.get_living_entity().unwrap().health.load() - health).abs() < f32::EPSILON);
}

/// The damage of an item frame fires `EntityInvulnerabilityCheckEvent` too, as vanilla
/// `ItemFrame.hurtServer` reads `isInvulnerableToBase`.
#[test]
fn an_item_frame_fires_the_invulnerability_check() {
    let pos = spot(15);
    load(pos);
    let frame = spawn(&EntityType::ITEM_FRAME, center(pos));
    let frame_id = id(&frame);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<EntityInvulnerabilityCheckEvent, _>(move |event| {
        if event.entity.get_entity().entity_id == frame_id {
            push(&seen_by_handler, "frame");
            event.invulnerable = true;
        }
    });

    let damaged = frame.damage(frame.as_ref(), 1.0, DamageType::GENERIC);

    assert_eq!(entries(&seen), ["frame"]);
    assert!(!damaged);
    assert!(is_live(&frame));
}

/// A cancelled `ProjectileImpactEvent` of an arrow skips the hit, so the arrow fires no
/// `ProjectileHitEvent`.
#[test]
fn cancel_projectile_impact_event_of_an_arrow() {
    let pos = spot(20);
    set(pos, Block::STONE.default_state.id);
    let arrow = spawn(
        &EntityType::ARROW,
        center(pos.up()) + Vector3::new(0.0, 0.5, 0.0),
    );
    let arrow_id = id(&arrow);
    arrow
        .get_entity()
        .velocity
        .store(Vector3::new(0.0, -1.0, 0.0));
    let seen = log();
    let seen_by_impact = seen.clone();
    on::<ProjectileImpactEvent, _>(move |event| {
        if event.projectile.get_entity().entity_id == arrow_id {
            push(&seen_by_impact, "impact");
            event.set_cancelled(true);
        }
    });
    let seen_by_hit = seen.clone();
    on::<ProjectileHitEvent, _>(move |event| {
        if event.entity_id == arrow_id {
            push(&seen_by_hit, "hit");
        }
    });

    arrow.tick(arrow.as_ref(), support::server());

    assert_eq!(entries(&seen), ["impact"]);
}

/// An invulnerable painting does not break, as vanilla `BlockAttachedEntity.hurtServer` reads
/// `isInvulnerableToBase`.
#[test]
fn an_invulnerable_painting_does_not_break() {
    let pos = spot(21);
    load(pos);
    let painting = spawn(&EntityType::PAINTING, center(pos));
    painting
        .get_entity()
        .invulnerable
        .store(true, std::sync::atomic::Ordering::Relaxed);

    let damaged = painting.damage(painting.as_ref(), 1.0, DamageType::GENERIC);

    assert!(!damaged);
    assert!(is_live(&painting));
}

/// An end crystal is fire immune, so fire damage does not destroy it.
#[test]
fn fire_does_not_destroy_an_end_crystal() {
    let pos = spot(22);
    load(pos);
    let crystal = spawn(&EntityType::END_CRYSTAL, center(pos));

    let damaged = crystal.damage(crystal.as_ref(), 1.0, DamageType::IN_FIRE);

    assert!(!damaged);
    assert!(is_live(&crystal));
}
