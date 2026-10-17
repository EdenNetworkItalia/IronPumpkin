//! The one event pipeline (`openspec/changes/neoforge-shaped-plugin-api`, spec
//! `neoforge-event-bus`, requirement "One event pipeline"): a world mutation that a mod performs
//! through the Pumpkin types fires the Pumpkin event of the vanilla action, with the cause
//! resolved to its owner, and a cancel leaves the world as it was. The table "Mutators and their
//! events" of `design.md` names the test of each mutator.
//!
//! A handler of a v0.2 Wasm plugin goes through `Context::register_event`, as the Wasm hosts
//! register it. The check with a real Wasm plugin is task 2.18.
//!
//! Scenarios of the requirement and the tests that cover them:
//!
//! - [x] Every mutator is listed: the table "Mutators and their events" of `design.md`
//! - [x] A projectile of a mod breaks a block: `a_projectile_of_a_mod_breaks_a_block`
//! - [x] A plugin sees a mod spawn: `a_mod_spawns_damages_and_kills_an_entity`
//! - [ ] A v0.2 world call from a Wasm plugin: task 2.18
//! - [ ] A mod cancels, a plugin observes: task 2.18
//! - [x] Internal machine state: `a_cause_less_set_block_state_fires_no_event`
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::{
    any::Any,
    marker::PhantomData,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use pumpkin_core::{
    entity::{
        Entity, EntityBase, living::LivingEntity, player::Player, projectile::ProjectileHit,
        r#type::from_type,
    },
    plugin::{
        BoxFuture, Cancellable, EventHandler, EventPriority, Payload, PluginMetadata,
        block::{
            block_break::BlockBreakEvent, block_exp::BlockExpEvent, block_place::BlockPlaceEvent,
        },
        entity::{
            entity_change_block::EntityChangeBlockEvent, entity_damage::EntityDamageEvent,
            entity_damage_by_entity::EntityDamageByEntityEvent, entity_death::EntityDeathEvent,
            entity_place::EntityPlaceEvent, entity_spawn::EntitySpawnEvent,
            item_spawn::ItemSpawnEvent,
        },
        level::BlockEventEntityPlaceEvent,
        startup::create_context,
    },
    server::Server,
    world::World,
};
use pumpkin_data::{
    Block, BlockDirection, damage::DamageType, entity::EntityType, item::Item,
    item_stack::ItemStack,
};
use pumpkin_util::{
    math::{position::BlockPos, vector3::Vector3},
    permission::PermissionLvl,
};
use pumpkin_world::world::BlockFlags;

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

/// Registers a blocking handler of a native mod for `E`.
fn on_with<E, F>(priority: EventPriority, receive_canceled: bool, handler: F)
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&mut E) + Send + Sync + 'static,
{
    support::server().plugin_manager.register_with::<E, _>(
        Arc::new(Blocking(handler, PhantomData)),
        priority,
        true,
        receive_canceled,
    );
}

/// Registers a blocking handler for `E` through the plugin context, as the Wasm hosts do.
fn plugin_on<E, F>(name: &str, priority: EventPriority, handler: F)
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&mut E) + Send + Sync + 'static,
{
    let metadata = PluginMetadata {
        name: name.to_owned(),
        version: "1.0.0".to_owned(),
        authors: Vec::new(),
        description: "Event pipeline test plugin".to_owned(),
        dependencies: Vec::new(),
        permissions: Vec::new(),
    };
    create_context(support::server(), metadata).register_event::<E, _>(
        Arc::new(Blocking(handler, PhantomData)),
        priority,
        true,
    );
}

/// The projectile of a mod: a mining laser that breaks the block it hits and damages the entity
/// it hits, with itself as the direct entity and its shooter as the cause.
struct MiningLaser {
    entity: Entity,
    owner_id: i32,
}

impl EntityBase for MiningLaser {
    fn get_entity(&self) -> &Entity {
        &self.entity
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }

    fn cast_any(&self) -> &dyn Any {
        self
    }

    fn get_owner_id(&self) -> Option<i32> {
        Some(self.owner_id)
    }

    fn on_hit(&self, hit: ProjectileHit) {
        let world = self.entity.world.load();
        match hit {
            ProjectileHit::Block { pos, .. } => {
                world.destroy_block(&pos, true, Some(self));
            }
            ProjectileHit::Entity { entity, .. } => {
                let owner = world.get_entity_by_id(self.owner_id);
                entity.damage_with_context(
                    entity.as_ref(),
                    LASER_DAMAGE,
                    DamageType::THROWN,
                    None,
                    Some(self),
                    owner.as_deref(),
                );
            }
        }
    }
}

const LASER_DAMAGE: f32 = 4.0;

fn laser(world: &Arc<World>, shooter: &Player) -> MiningLaser {
    let shooter = &shooter.living_entity.entity;
    MiningLaser {
        entity: Entity::new(world.clone(), shooter.pos.load(), &EntityType::SNOWBALL),
        owner_id: shooter.entity_id,
    }
}

fn block_hit(pos: BlockPos) -> ProjectileHit {
    ProjectileHit::Block {
        pos,
        face: BlockDirection::Up,
        hit_pos: pos.to_centered_f64(),
        normal: Vector3::new(0.0, 1.0, 0.0),
    }
}

/// Loads the chunk of `pos` for the rest of the process and sets `pos` to `state`. The positions
/// of these tests lie in the chunk of the spawn.
fn set_loaded(pos: BlockPos, block: &Block) {
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
    world.set_block_state(&pos, block.default_state.id, BlockFlags::empty());
    assert!(is(pos, block));
}

fn is(pos: BlockPos, block: &Block) -> bool {
    support::world().get_block_state_id(&pos) == block.default_state.id
}

fn player_name(player: Option<&Arc<Player>>) -> &str {
    player.map_or("none", |player| player.gameprofile.name.as_str())
}

/// The acceptance example of the one event pipeline: a v0.2 plugin cancels the break by a mod
/// projectile, and a handler of another mod with receive-canceled sees the cancelled event with
/// the player that shot the projectile.
#[test]
fn a_projectile_of_a_mod_breaks_a_block() {
    let _serial = support::serial();
    let shooter = support::player("LaserShooter");
    let world = support::world();
    let pos = BlockPos::new(1, 120, 1);
    set_loaded(pos, &Block::STONE);
    let cancel = Arc::new(AtomicBool::new(true));
    let flag = cancel.clone();
    plugin_on::<BlockBreakEvent, _>("pipeline-break-plugin", EventPriority::High, move |event| {
        if event.block_position == pos {
            event.set_cancelled(flag.load(Ordering::Relaxed));
        }
    });
    let seen = log();
    let observed = seen.clone();
    on_with::<BlockBreakEvent, _>(EventPriority::Low, true, move |event| {
        if event.block_position == pos {
            push(
                &observed,
                format!(
                    "{} {}",
                    event.cancelled(),
                    player_name(event.player.as_ref())
                ),
            );
        }
    });
    let laser = laser(&world, &shooter);

    laser.on_hit(block_hit(pos));
    assert!(
        is(pos, &Block::STONE),
        "the cancelled break removed the block"
    );

    cancel.store(false, Ordering::Relaxed);
    laser.on_hit(block_hit(pos));
    assert!(is(pos, &Block::AIR), "the break kept the block");
    assert_eq!(entries(&seen), ["true LaserShooter", "false LaserShooter"]);
}

/// `World::destroy_block` takes the settable fields of `BlockBreakEvent`: the experience that a
/// handler sets drops, and `drop = false` drops no item and no experience.
#[test]
fn destroy_block_takes_the_experience_and_the_drop_of_the_event() {
    let _serial = support::serial();
    let world = support::world();
    let pos = BlockPos::new(7, 120, 1);
    set_loaded(pos, &Block::DIRT);
    let drop = Arc::new(AtomicBool::new(true));
    let flag = drop.clone();
    on_with::<BlockBreakEvent, _>(EventPriority::Normal, false, move |event| {
        if event.block_position == pos {
            event.exp = 5;
            event.drop = flag.load(Ordering::Relaxed);
        }
    });
    let seen = log();
    let observed = seen.clone();
    on_with::<ItemSpawnEvent, _>(EventPriority::Normal, false, move |event| {
        if BlockPos::floored_v(event.position) == pos {
            push(&observed, "item");
        }
    });
    let observed = seen.clone();
    on_with::<BlockExpEvent, _>(EventPriority::Normal, false, move |event| {
        if event.block_pos == pos {
            push(&observed, format!("exp {}", event.exp));
        }
    });

    assert!(world.destroy_block(&pos, true, None));
    assert!(is(pos, &Block::AIR));
    assert_eq!(entries(&seen), ["item", "exp 5"]);

    set_loaded(pos, &Block::DIRT);
    drop.store(false, Ordering::Relaxed);
    assert!(world.destroy_block(&pos, true, None));
    assert!(is(pos, &Block::AIR));
    assert_eq!(
        entries(&seen),
        ["item", "exp 5"],
        "a break without drops dropped"
    );
}

#[test]
fn a_player_breaks_a_block_through_the_world() {
    let _serial = support::serial();
    let player = support::player("WorldBreaker");
    // The block is inside the spawn protection.
    player.permission_lvl.store(PermissionLvl::Four);
    let world = support::world();
    let pos = BlockPos::new(2, 120, 1);
    set_loaded(pos, &Block::STONE);
    let cancel = Arc::new(AtomicBool::new(true));
    let seen = log();
    let (observed, flag) = (seen.clone(), cancel.clone());
    on_with::<BlockBreakEvent, _>(EventPriority::Normal, false, move |event| {
        if event.block_position == pos {
            push(&observed, player_name(event.player.as_ref()));
            event.set_cancelled(flag.load(Ordering::Relaxed));
        }
    });

    assert_eq!(
        world.break_block(&pos, Some(&player), BlockFlags::NOTIFY_ALL),
        None
    );
    assert!(
        is(pos, &Block::STONE),
        "the cancelled break removed the block"
    );

    cancel.store(false, Ordering::Relaxed);
    assert!(
        world
            .break_block(&pos, Some(&player), BlockFlags::NOTIFY_ALL)
            .is_some()
    );
    assert!(is(pos, &Block::AIR), "the break kept the block");
    assert_eq!(entries(&seen), ["WorldBreaker", "WorldBreaker"]);
}

/// `World::place_block` fires the event of the entity behind the placer: `BlockPlaceEvent` for a
/// projectile that a player shot, `BlockEventEntityPlaceEvent` for one that a mob owns.
#[test]
fn a_mod_places_a_block_through_the_world() {
    let _serial = support::serial();
    let shooter = support::player("LaserPlacer");
    let world = support::world();
    let pos = BlockPos::new(3, 120, 1);
    set_loaded(pos, &Block::AIR);
    let cancel = Arc::new(AtomicBool::new(true));
    let flag = cancel.clone();
    plugin_on::<BlockPlaceEvent, _>("pipeline-place-plugin", EventPriority::High, move |event| {
        if event.block_position == pos {
            event.set_cancelled(flag.load(Ordering::Relaxed));
        }
    });
    let seen = log();
    let observed = seen.clone();
    on_with::<BlockPlaceEvent, _>(EventPriority::Low, true, move |event| {
        if event.block_position == pos {
            push(
                &observed,
                format!(
                    "{} {} {}",
                    event.cancelled(),
                    event.player.gameprofile.name,
                    event.placed_state.id.to_block().name
                ),
            );
        }
    });
    let laser = laser(&world, &shooter);

    let stone = Block::STONE.default_state.id;
    assert!(!world.place_block(&pos, stone, &laser));
    assert!(is(pos, &Block::AIR), "the cancelled place set the block");

    cancel.store(false, Ordering::Relaxed);
    assert!(world.place_block(&pos, stone, &laser));
    assert!(is(pos, &Block::STONE), "the place did not set the block");
    assert_eq!(
        entries(&seen),
        ["true LaserPlacer stone", "false LaserPlacer stone"]
    );

    let pig = from_type(
        &EntityType::PIG,
        Vector3::new(3.5, 121.0, 1.5),
        &world,
        uuid::Uuid::new_v4(),
    );
    assert!(world.spawn_entity(pig.clone()));
    let pig_id = pig.get_entity().entity_id;
    let pig_laser = MiningLaser {
        entity: Entity::new(
            world.clone(),
            Vector3::new(3.5, 121.0, 1.5),
            &EntityType::SNOWBALL,
        ),
        owner_id: pig_id,
    };
    let at = BlockPos::new(3, 121, 1);
    set_loaded(at, &Block::AIR);
    let placers = log();
    let observed = placers.clone();
    on_with::<BlockEventEntityPlaceEvent, _>(EventPriority::Normal, false, move |event| {
        if event.pos == at {
            push(
                &observed,
                format!(
                    "{} {}",
                    event.entity.get_entity().entity_id == pig_id,
                    event.placed_state.id.to_block().name
                ),
            );
            event.set_cancelled(true);
        }
    });
    assert!(!world.place_block(&at, stone, &pig_laser));
    assert!(is(at, &Block::AIR), "the cancelled place set the block");
    assert_eq!(entries(&placers), ["true stone"]);
}

#[test]
fn a_cause_less_set_block_state_fires_no_event() {
    let _serial = support::serial();
    let world = support::world();
    let pos = BlockPos::new(4, 120, 1);
    set_loaded(pos, &Block::AIR);
    let seen = log();
    let observed = seen.clone();
    on_with::<BlockBreakEvent, _>(EventPriority::Normal, true, move |event| {
        if event.block_position == pos {
            push(&observed, "break");
        }
    });
    let observed = seen.clone();
    on_with::<BlockPlaceEvent, _>(EventPriority::Normal, true, move |event| {
        if event.block_position == pos {
            push(&observed, "player place");
        }
    });
    let observed = seen.clone();
    on_with::<EntityPlaceEvent, _>(EventPriority::Normal, true, move |event| {
        if event.block_pos == pos {
            push(&observed, "entity place");
        }
    });
    let observed = seen.clone();
    on_with::<EntityChangeBlockEvent, _>(EventPriority::Normal, true, move |event| {
        if event.block_pos == pos {
            push(&observed, "entity change");
        }
    });

    world.set_block_state(&pos, Block::STONE.default_state.id, BlockFlags::NOTIFY_ALL);
    assert!(is(pos, &Block::STONE));
    world.set_block_state(&pos, Block::AIR.default_state.id, BlockFlags::NOTIFY_ALL);
    assert!(is(pos, &Block::AIR));
    assert!(entries(&seen).is_empty(), "{:?}", entries(&seen));
}

/// A mod spawns an entity, then hits it with its projectile and kills it. A v0.2 plugin cancels
/// the first spawn and the first damage.
#[test]
fn a_mod_spawns_damages_and_kills_an_entity() {
    let _serial = support::serial();
    let shooter = support::player("LaserHunter");
    let world = support::world();
    let at = Vector3::new(5.5, 120.0, 1.5);
    let pig = from_type(&EntityType::PIG, at, &world, uuid::Uuid::new_v4());
    let pig_id = pig.get_entity().entity_id;
    let other = from_type(&EntityType::PIG, at, &world, uuid::Uuid::new_v4());
    let other_id = other.get_entity().entity_id;
    let laser = laser(&world, &shooter);
    let laser_id = laser.entity.entity_id;
    let cancel = Arc::new(AtomicBool::new(true));
    let seen = log();

    let (observed, flag) = (seen.clone(), cancel.clone());
    plugin_on::<EntitySpawnEvent, _>("pipeline-spawn-plugin", EventPriority::High, move |event| {
        if event.entity_id == pig_id || event.entity_id == other_id {
            push(&observed, format!("spawn {}", event.entity_id == pig_id));
            event.set_cancelled(flag.load(Ordering::Relaxed));
        }
    });
    let (observed, flag) = (seen.clone(), cancel.clone());
    plugin_on::<EntityDamageEvent, _>(
        "pipeline-damage-plugin",
        EventPriority::High,
        move |event| {
            if event.entity_id == pig_id {
                push(
                    &observed,
                    format!("damage {}", event.damage_type.message_id),
                );
                event.set_cancelled(flag.load(Ordering::Relaxed));
            }
        },
    );
    let observed = seen.clone();
    on_with::<EntityDamageByEntityEvent, _>(EventPriority::Normal, false, move |event| {
        if event.entity_id == pig_id {
            push(
                &observed,
                format!("by laser {}", event.damager_id == laser_id),
            );
        }
    });
    let observed = seen.clone();
    on_with::<EntityDeathEvent, _>(EventPriority::Normal, false, move |event| {
        if event.entity_id == pig_id {
            push(&observed, "death");
        }
    });

    assert!(!world.spawn_entity(pig.clone()));
    world.spawn_entity_non_save(other.clone());
    assert!(world.get_entity_by_id(pig_id).is_none());
    assert!(world.get_entity_by_id(other_id).is_none());
    let hit = || ProjectileHit::Entity {
        entity: pig.clone(),
        hit_pos: at,
        normal: Vector3::new(0.0, 1.0, 0.0),
    };
    laser.on_hit(hit());
    let health = || pig.get_living_entity().unwrap().health.load();
    let full = health();

    cancel.store(false, Ordering::Relaxed);
    assert!(world.spawn_entity(pig.clone()));
    world.spawn_entity_non_save(other.clone());
    assert!(world.get_entity_by_id(pig_id).is_some());
    assert!(world.get_entity_by_id(other_id).is_some());
    laser.on_hit(hit());
    assert!(
        (full - health() - LASER_DAMAGE).abs() < 1e-4,
        "{full} -> {}",
        health()
    );
    pig.kill(pig.as_ref());
    assert!(health() <= 0.0);

    assert_eq!(
        entries(&seen),
        [
            "spawn true",
            "spawn false",
            "damage thrown",
            "spawn true",
            "spawn false",
            "damage thrown",
            "by laser true",
            "damage genericKill",
            "death",
        ]
    );
}

#[test]
fn a_mod_drops_an_item() {
    let _serial = support::serial();
    let world = support::world();
    let pos = BlockPos::new(6, 120, 1);
    let cancel = Arc::new(AtomicBool::new(true));
    let seen = log();
    let (observed, flag) = (seen.clone(), cancel.clone());
    plugin_on::<ItemSpawnEvent, _>("pipeline-item-plugin", EventPriority::High, move |event| {
        if BlockPos::floored_v(event.position) == pos {
            push(&observed, "item");
            event.set_cancelled(flag.load(Ordering::Relaxed));
        }
    });
    let spawned = log();
    let observed = spawned.clone();
    on_with::<EntitySpawnEvent, _>(EventPriority::Normal, false, move |event| {
        if BlockPos::floored_v(event.position) == pos {
            push(&observed, event.entity_id.to_string());
        }
    });

    world.drop_stack(&pos, ItemStack::new(1, &Item::DIAMOND));
    assert!(
        entries(&spawned).is_empty(),
        "the cancelled drop spawned an item"
    );

    cancel.store(false, Ordering::Relaxed);
    world.drop_stack(&pos, ItemStack::new(1, &Item::DIAMOND));
    let spawned = entries(&spawned);
    assert_eq!(spawned.len(), 1);
    let id = spawned[0].parse().unwrap();
    assert!(world.get_entity_by_id(id).is_some());
    assert_eq!(entries(&seen), ["item", "item"]);
}
