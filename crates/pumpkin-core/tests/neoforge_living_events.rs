//! The `NeoForge` living entity events (`event.entity.living`, task 2.15 of
//! `openspec/changes/neoforge-shaped-plugin-api`) as Pumpkin events: one test per row that fires
//! from its `NeoForge` fire site, and the cancel and write-back tests of the family, against a
//! server of the support module.
//!
//! Each test filters its handlers by the id of its own entity, because the handlers stay
//! registered on the shared server.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::{
    marker::PhantomData,
    sync::{Arc, Mutex, atomic::Ordering},
    time::{Duration, Instant},
};

use pumpkin_core::{
    entity::{
        Entity, EntityBase,
        boss::wither::WitherEntity,
        living::LivingEntity,
        mob::{Mob, hoglin::HoglinEntity},
        player::Player,
        r#type::from_type,
    },
    item::items::bow::BowItem,
    plugin::{
        BoxFuture, Cancellable, EventHandler, EventPriority, ParentEventHandler, Payload,
        living::{
            EffectParticleModificationEvent, LivingConversionEventPost, LivingDamageEventPost,
            LivingDamageEventPre, LivingDestroyBlockEvent, LivingDrownEvent,
            LivingEntityUseItemEvent, LivingEntityUseItemEventStart, LivingEntityUseItemEventStop,
            LivingEntityUseItemEventTick, LivingEventLivingJumpEvent,
            LivingEventLivingVisibilityEvent, LivingExperienceDropEvent, LivingFallEvent,
            LivingGetProjectileEvent, LivingKnockBackEvent, LivingShieldBlockEvent,
            MobEffectEventApplicable, MobEffectEventApplicableResult,
        },
    },
    server::Server,
};
use pumpkin_data::{
    Block, damage::DamageType, effect::StatusEffect, entity::EntityType, item::Item,
    item_stack::ItemStack, potion::Effect,
};
use pumpkin_inventory::inventory::Inventory;
use pumpkin_util::{
    Hand,
    math::{position::BlockPos, vector3::Vector3},
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

fn id(entity: &Arc<dyn EntityBase>) -> i32 {
    entity.get_entity().entity_id
}

/// Spawns a mob of `entity_type` at `x`, in the air and away from the other tests.
fn spawn(entity_type: &'static EntityType, x: f64) -> Arc<dyn EntityBase> {
    let world = support::world();
    let entity = from_type(
        entity_type,
        Vector3::new(x, 120.0, 0.5),
        &world,
        uuid::Uuid::new_v4(),
    );
    world.spawn_entity(entity.clone());
    entity
}

fn living(entity: &Arc<dyn EntityBase>) -> &LivingEntity {
    entity.get_living_entity().unwrap()
}

/// A player of its own at `x`.
fn player_at(name: &str, x: f64) -> Arc<Player> {
    let player = support::player(name);
    player
        .living_entity
        .entity
        .set_pos(Vector3::new(x, 100.0, 0.5));
    player
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

const fn speed(duration: i32) -> Effect {
    Effect {
        effect_type: &StatusEffect::SPEED,
        duration,
        amplifier: 0,
        ambient: false,
        show_particles: true,
        show_icon: true,
        blend: false,
    }
}

/// `LivingDamageEvent.Pre` (`LivingDamageEventPre`) fires after the reductions with a settable
/// damage that the health takes, and `LivingDamageEvent.Post` (`LivingDamageEventPost`) fires
/// after the health changed.
#[test]
fn change_a_settable_field_living_damage_event_pre() {
    let _serial = support::serial();
    let zombie = spawn(&EntityType::ZOMBIE, 16.5);
    let zombie_id = id(&zombie);
    let seen = log();
    let (pre, post) = (seen.clone(), seen.clone());
    on::<LivingDamageEventPre, _>(move |event| {
        if id(&event.entity) == zombie_id {
            push(
                &pre,
                format!(
                    "pre {} {} {}",
                    event.original_damage,
                    event.new_damage,
                    event.source.damage_type == DamageType::GENERIC
                ),
            );
            event.new_damage = 1.0;
        }
    });
    on::<LivingDamageEventPost, _>(move |event| {
        if id(&event.entity) == zombie_id {
            push(
                &post,
                format!(
                    "post {} {} {} {}",
                    event.original_damage,
                    event.inflicted_damage,
                    event.health_damage,
                    event.should_cause_side_effects
                ),
            );
        }
    });
    let health = living(&zombie).health.load();

    assert!(living(&zombie).damage(zombie.as_ref(), 5.0, DamageType::GENERIC));

    assert_eq!(living(&zombie).health.load(), health - 1.0);
    assert_eq!(entries(&seen), ["pre 5 5 true", "post 5 1 1 true"]);
}

/// `LivingShieldBlockEvent` fires for a damage to a player that holds up a shield. With
/// `blocked` set to `false` the shield blocks nothing.
#[test]
fn change_a_settable_field_living_shield_block_event() {
    let _serial = support::serial();
    let player = player_at("Blocker", 2000.5);
    let player_id = player.entity_id();
    let shield = ItemStack::new(1, &Item::SHIELD);
    player.living_entity.set_active_hand(
        Hand::Right,
        shield.clone(),
        shield.get_max_use_time() - 10,
    );
    let unblock = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen = log();
    let (observed, flag) = (seen.clone(), unblock.clone());
    on::<LivingShieldBlockEvent, _>(move |event| {
        if id(&event.entity) == player_id {
            push(
                &observed,
                format!(
                    "{} {} {} {}",
                    event.damage, event.blocked_damage, event.original_blocked, event.blocked
                ),
            );
            event.blocked = !flag.load(Ordering::Relaxed);
        }
    });
    // In front of the player, which looks along +z.
    let front = Some(Vector3::new(2000.5, 100.0, 3.5));
    let health = player.living_entity.health.load();
    let hit = || {
        player.living_entity.damage_with_context(
            player.as_ref(),
            4.0,
            DamageType::MOB_ATTACK,
            front,
            None,
            None,
        )
    };

    assert!(!hit(), "the shield blocks the damage");
    assert_eq!(player.living_entity.health.load(), health);
    unblock.store(true, Ordering::Relaxed);
    assert!(hit());
    assert_eq!(player.living_entity.health.load(), health - 4.0);
    assert_eq!(entries(&seen), ["4 4 true true", "4 4 true true"]);
}

struct UseProbe(Log, i32);

impl ParentEventHandler for UseProbe {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut (dyn Payload + Send + Sync),
    ) -> BoxFuture<'a, ()> {
        if <dyn Payload>::downcast_ref::<LivingEntityUseItemEventStart>(event)
            .is_some_and(|start| id(&start.entity) == self.1)
        {
            push(&self.0, format!("parent {}", event.get_name()));
        }
        Box::pin(async {})
    }
}

/// `LivingEntityUseItemEvent.Start` (`LivingEntityUseItemEventStart`) fires when an item use
/// starts, also for a handler on the parent `LivingEntityUseItemEvent`. A cancel starts no use.
#[test]
fn cancel_living_entity_use_item_event_start() {
    let _serial = support::serial();
    let zombie = spawn(&EntityType::ZOMBIE, 32.5);
    let zombie_id = id(&zombie);
    let seen = log();
    let observed = seen.clone();
    on::<LivingEntityUseItemEventStart, _>(move |event| {
        if id(&event.entity) == zombie_id {
            push(
                &observed,
                format!("start {} {}", event.item.item.registry_key, event.duration),
            );
            event.set_cancelled(true);
        }
    });
    support::server()
        .plugin_manager
        .register_parent::<LivingEntityUseItemEvent, _>(
            Arc::new(UseProbe(seen.clone(), zombie_id)),
            EventPriority::Low,
            true,
            true,
        );

    living(&zombie).set_active_hand(Hand::Right, ItemStack::new(1, &Item::BREAD), 32);

    assert!(living(&zombie).item_in_use.lock().unwrap().is_none());
    assert_eq!(
        entries(&seen),
        ["start bread 32", "parent LivingEntityUseItemEventStart"]
    );
}

/// `LivingEntityUseItemEvent.Tick` (`LivingEntityUseItemEventTick`) fires in each tick of a use.
/// A duration of 0 finishes the use of a consumable in that tick.
#[test]
fn change_a_settable_field_living_entity_use_item_event_tick() {
    let _serial = support::serial();
    let zombie = spawn(&EntityType::ZOMBIE, 48.5);
    let zombie_id = id(&zombie);
    let seen = log();
    let observed = seen.clone();
    on::<LivingEntityUseItemEventTick, _>(move |event| {
        if id(&event.entity) == zombie_id {
            push(&observed, format!("tick {}", event.duration));
            event.duration = 0;
        }
    });
    living(&zombie).set_active_hand(Hand::Right, ItemStack::new(1, &Item::BREAD), 32);

    living(&zombie).tick(zombie.as_ref(), support::server());

    assert_eq!(entries(&seen), ["tick 32"]);
    assert!(
        living(&zombie).item_in_use.lock().unwrap().is_none(),
        "the use finished"
    );
}

fn bow_and_arrows(player: &Player, arrows: u8) {
    let inventory = player.inventory();
    inventory.set_selected_slot(0);
    inventory.set_stack(0, ItemStack::new(1, &Item::BOW));
    inventory.set_stack(1, ItemStack::new(arrows, &Item::ARROW));
}

fn arrows(player: &Player) -> u8 {
    player.inventory().get_stack(1).item_count
}

/// `LivingEntityUseItemEvent.Stop` (`LivingEntityUseItemEventStop`) fires when the player
/// releases a drawn bow. A cancel shoots nothing, and the use still stops.
#[test]
fn cancel_living_entity_use_item_event_stop() {
    let _serial = support::serial();
    let player = player_at("Releaser", 2100.5);
    let player_id = player.entity_id();
    bow_and_arrows(&player, 4);
    let seen = log();
    let observed = seen.clone();
    on::<LivingEntityUseItemEventStop, _>(move |event| {
        if id(&event.entity) == player_id {
            push(&observed, format!("stop {}", event.duration));
            event.set_cancelled(true);
        }
    });
    player.living_entity.set_active_hand(
        Hand::Right,
        ItemStack::new(1, &Item::BOW),
        BowItem::USE_DURATION - 20,
    );

    player.release_using_item();

    assert_eq!(
        entries(&seen),
        [format!("stop {}", BowItem::USE_DURATION - 20)]
    );
    assert_eq!(arrows(&player), 4, "a cancel shoots nothing");
    assert!(player.living_entity.item_in_use.lock().unwrap().is_none());
}

/// `LivingGetProjectileEvent` fires when a bow looks for its arrow. A projectile that a handler
/// sets is shot, and the arrows in the inventory stay.
#[test]
fn change_a_settable_field_living_get_projectile_event() {
    let _serial = support::serial();
    let player = player_at("Quiver", 2200.5);
    let player_id = player.entity_id();
    bow_and_arrows(&player, 4);
    let seen = log();
    let observed = seen.clone();
    on::<LivingGetProjectileEvent, _>(move |event| {
        if id(&event.entity) == player_id {
            push(
                &observed,
                format!(
                    "{} {} {}",
                    event.projectile_weapon.item.registry_key,
                    event.projectile.item.registry_key,
                    event.projectile.item_count
                ),
            );
            event.projectile = ItemStack::new(1, &Item::SPECTRAL_ARROW);
        }
    });
    player
        .living_entity
        .item_use_time
        .store(BowItem::USE_DURATION - 20, Ordering::Relaxed);

    BowItem::release_bow(&player, &ItemStack::new(1, &Item::BOW));

    assert_eq!(entries(&seen), ["bow arrow 4"]);
    assert_eq!(arrows(&player), 4, "the replaced projectile takes no arrow");
}

/// `LivingKnockBackEvent` fires for the knockback of a damage, before the knockback resistance.
/// A cancel gives no knockback.
#[test]
fn cancel_living_knock_back_event() {
    let _serial = support::serial();
    let attacker = player_at("Pusher", 64.5);
    let zombie = spawn(&EntityType::ZOMBIE, 66.5);
    let zombie_id = id(&zombie);
    let seen = log();
    let observed = seen.clone();
    on::<LivingKnockBackEvent, _>(move |event| {
        if id(&event.entity) == zombie_id {
            push(
                &observed,
                format!("{} {}", event.strength, event.original_strength),
            );
            event.set_cancelled(true);
        }
    });

    assert!(living(&zombie).damage_with_context(
        zombie.as_ref(),
        1.0,
        DamageType::GENERIC,
        None,
        Some(attacker.as_ref()),
        Some(attacker.as_ref()),
    ));

    assert_eq!(entries(&seen), ["0.4 0.4"]);
    let velocity = zombie.get_entity().velocity.load();
    assert!(velocity.x == 0.0 && velocity.z == 0.0, "no knockback");
}

/// `LivingFallEvent` fires when a mob lands, with a settable distance.
#[test]
fn change_a_settable_field_living_fall_event() {
    let _serial = support::serial();
    let zombie = spawn(&EntityType::ZOMBIE, 80.5);
    let zombie_id = id(&zombie);
    let seen = log();
    let observed = seen.clone();
    on::<LivingFallEvent, _>(move |event| {
        if id(&event.entity) == zombie_id {
            push(
                &observed,
                format!("{} {}", event.distance, event.damage_multiplier),
            );
            event.distance = 4.0;
        }
    });
    let health = living(&zombie).health.load();

    living(&zombie).handle_fall_damage(zombie.as_ref(), 10.0, 1.0);

    assert_eq!(entries(&seen), ["10 1"]);
    // 4 blocks minus the safe fall distance of 3.
    assert_eq!(living(&zombie).health.load(), health - 1.0);
}

/// `LivingDrownEvent` fires for a player out of air under water. `damage_amount` is settable.
#[test]
fn change_a_settable_field_living_drown_event() {
    let _serial = support::serial();
    let player = player_at("Diver", 2300.5);
    let player_id = player.entity_id();
    let eye = BlockPos::new(2300, 101, 0);
    load(eye);
    let world = support::world();
    let water = Block::WATER.default_state.id;
    world.set_block_state(&eye, water, BlockFlags::empty());
    world.set_block_state(&eye.down(), water, BlockFlags::empty());
    let seen = log();
    let observed = seen.clone();
    on::<LivingDrownEvent, _>(move |event| {
        if id(&event.entity) == player_id {
            push(
                &observed,
                format!("{} {}", event.drowning, event.damage_amount),
            );
            event.damage_amount = 5.0;
        }
    });
    let breath = &player.breath_manager;
    breath.air_supply.store(1, Ordering::Relaxed);
    breath.drowning_tick.store(19, Ordering::Relaxed);
    let health = player.living_entity.health.load();

    breath.tick(&player);

    assert_eq!(entries(&seen), ["true 2"]);
    assert_eq!(player.living_entity.health.load(), health - 5.0);
}

fn orbs_near(pos: Vector3<f64>) -> usize {
    support::world()
        .entities
        .load()
        .iter()
        .filter(|entity| {
            let entity = entity.get_entity();
            entity.entity_type == &EntityType::EXPERIENCE_ORB
                && entity.pos.load().squared_distance_to_vec(&pos) < 4.0
        })
        .count()
}

/// `LivingExperienceDropEvent` fires when a player kills a mob. A cancel drops no experience.
#[test]
fn cancel_living_experience_drop_event() {
    let _serial = support::serial();
    let killer = player_at("Hunter", 96.5);
    let zombie = spawn(&EntityType::ZOMBIE, 98.5);
    let zombie_id = id(&zombie);
    let seen = log();
    let observed = seen.clone();
    on::<LivingExperienceDropEvent, _>(move |event| {
        if id(&event.entity) == zombie_id {
            push(
                &observed,
                format!(
                    "{} {}",
                    event.attacking_player.as_ref().unwrap().gameprofile.name,
                    event.original_experience > 0
                ),
            );
            event.set_cancelled(true);
        }
    });
    let pos = zombie.get_entity().pos.load();

    living(&zombie).damage_with_context(
        zombie.as_ref(),
        1000.0,
        DamageType::GENERIC,
        None,
        Some(killer.as_ref()),
        Some(killer.as_ref()),
    );

    assert_eq!(entries(&seen), ["Hunter true"]);
    assert_eq!(orbs_near(pos), 0);
}

/// `LivingEvent.LivingJumpEvent` (`LivingEventLivingJumpEvent`) fires for the jump of a player
/// that the movement packets show.
#[test]
fn one_event_per_fire_site_living_jump_event() {
    let _serial = support::serial();
    let player = player_at("Jumper", 2400.5);
    let player_id = player.entity_id();
    let seen = log();
    let observed = seen.clone();
    on::<LivingEventLivingJumpEvent, _>(move |event| {
        if id(&event.entity) == player_id {
            push(&observed, "jump");
        }
    });

    player.jump();

    assert_eq!(entries(&seen), ["jump"]);
}

/// `LivingEvent.LivingVisibilityEvent` (`LivingEventLivingVisibilityEvent`) fires when a mob
/// reads how visible an entity is. `visibility_modifier` is settable.
#[test]
fn change_a_settable_field_living_visibility_event() {
    let _serial = support::serial();
    let player = player_at("Seen", 2500.5);
    let player_id = player.entity_id();
    let zombie = spawn(&EntityType::ZOMBIE, 112.5);
    let zombie_id = id(&zombie);
    let seen = log();
    let observed = seen.clone();
    on::<LivingEventLivingVisibilityEvent, _>(move |event| {
        if id(&event.entity) == player_id {
            push(
                &observed,
                format!(
                    "{} {}",
                    event.visibility_modifier,
                    event.looking_entity.as_ref().map(id) == Some(zombie_id)
                ),
            );
            event.modify_visibility(0.5);
        }
    });

    let visibility = player
        .living_entity
        .visibility_percent(Some(zombie.as_ref()));

    assert!((visibility - 0.5).abs() < f64::EPSILON);
    assert_eq!(entries(&seen), ["1 true"]);
}

/// `LivingDestroyBlockEvent` fires for each block that a wither breaks around it. A cancel keeps
/// the block.
#[test]
fn cancel_living_destroy_block_event() {
    let _serial = support::serial();
    let world = support::world();
    let base = BlockPos::new(136, 120, 8);
    load(base);
    let kept = base.up();
    let broken = base.up().up();
    let stone = Block::STONE.default_state.id;
    world.set_block_state(&kept, stone, BlockFlags::empty());
    world.set_block_state(&broken, stone, BlockFlags::empty());
    let wither = WitherEntity::new(Entity::new(
        world.clone(),
        Vector3::new(136.5, 120.0, 8.5),
        &EntityType::WITHER,
    ));
    let wither_dyn: Arc<dyn EntityBase> = wither.clone();
    world.spawn_entity(wither_dyn.clone());
    let wither_id = id(&wither_dyn);
    let seen = log();
    let observed = seen.clone();
    on::<LivingDestroyBlockEvent, _>(move |event| {
        if id(&event.entity) == wither_id && event.pos == kept {
            push(&observed, format!("{}", event.state.id == stone));
            event.set_cancelled(true);
        }
    });
    wither.destroy_blocks_tick.store(1, Ordering::Relaxed);

    wither.mob_tick(wither_dyn.as_ref());

    assert_eq!(entries(&seen), ["true"]);
    assert_eq!(world.get_block_state_id(&kept), stone);
    assert_eq!(
        world.get_block_state_id(&broken),
        Block::AIR.default_state.id
    );
}

/// `LivingConversionEvent.Post` (`LivingConversionEventPost`) fires when a hoglin in the
/// overworld converts to a zoglin.
#[test]
fn one_event_per_fire_site_living_conversion_event_post() {
    let _serial = support::serial();
    let hoglin = spawn(&EntityType::HOGLIN, 144.5);
    let hoglin_id = id(&hoglin);
    let seen = log();
    let observed = seen.clone();
    on::<LivingConversionEventPost, _>(move |event| {
        if id(&event.entity) == hoglin_id {
            push(
                &observed,
                event.outcome.get_entity().entity_type.resource_name,
            );
        }
    });
    let hoglin_entity = hoglin.cast_any().downcast_ref::<HoglinEntity>().unwrap();
    hoglin_entity
        .time_in_overworld
        .store(300, Ordering::Relaxed);

    hoglin_entity.mob_tick(hoglin.as_ref());

    assert_eq!(entries(&seen), ["zoglin"]);
}

/// `EffectParticleModificationEvent` fires for each active effect when the particles of a living
/// entity change.
#[test]
fn one_event_per_fire_site_effect_particle_modification_event() {
    let _serial = support::serial();
    let zombie = spawn(&EntityType::ZOMBIE, 160.5);
    let zombie_id = id(&zombie);
    let seen = log();
    let observed = seen.clone();
    on::<EffectParticleModificationEvent, _>(move |event| {
        if id(&event.entity) == zombie_id {
            push(
                &observed,
                format!(
                    "{} {} {}",
                    event.effect.effect_type.minecraft_name,
                    event.original_particle_color
                        == (0xFF << 24 | StatusEffect::SPEED.color as u32) as i32,
                    event.visible
                ),
            );
            event.visible = false;
        }
    });

    living(&zombie).add_effect(speed(200));

    assert_eq!(
        entries(&seen),
        [format!("{} true true", StatusEffect::SPEED.minecraft_name)]
    );
}

/// `MobEffectEvent.Applicable` (`MobEffectEventApplicable`) fires before an effect applies.
/// `DoNotApply` refuses an effect, and `Apply` gives an effect that vanilla refuses.
#[test]
fn change_a_settable_field_mob_effect_event_applicable() {
    let _serial = support::serial();
    let zombie = spawn(&EntityType::ZOMBIE, 176.5);
    let skeleton = spawn(&EntityType::WITHER_SKELETON, 178.5);
    let (zombie_id, skeleton_id) = (id(&zombie), id(&skeleton));
    on::<MobEffectEventApplicable, _>(move |event| {
        if id(&event.entity) == zombie_id {
            event.result = MobEffectEventApplicableResult::DoNotApply;
        } else if id(&event.entity) == skeleton_id {
            event.result = MobEffectEventApplicableResult::Apply;
        }
    });

    living(&zombie).add_effect(speed(200));
    living(&skeleton).add_effect(Effect {
        effect_type: &StatusEffect::WITHER,
        ..speed(200)
    });

    assert!(!living(&zombie).has_effect(&StatusEffect::SPEED));
    assert!(living(&skeleton).has_effect(&StatusEffect::WITHER));
}
