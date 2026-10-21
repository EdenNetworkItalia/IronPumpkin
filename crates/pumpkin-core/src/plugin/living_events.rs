//! The `IronPumpkin` fire sites of the `NeoForge` living entity events (`event.entity.living`).
//!
//! Each function checks `has_handlers` before it builds an event, so a fire site with no handler
//! allocates nothing and returns the vanilla value. The upstream code calls these functions with
//! one line. An event carries the shared handle of its entity, so it fires only for an entity of
//! its world.

use std::sync::Arc;

use pumpkin_data::{BlockState, damage::DamageType, item_stack::ItemStack, potion::Effect};
use pumpkin_util::{
    Hand,
    math::{position::BlockPos, vector3::Vector3},
};

use crate::entity::{EntityBase, living::LivingEntity, player::Player};
use crate::plugin::api::events::{
    Payload,
    living::{
        DamageSource, EffectParticleModificationEvent, LivingConversionEventPost,
        LivingDamageEventPost, LivingDamageEventPre, LivingDestroyBlockEvent, LivingDrownEvent,
        LivingEntityUseItemEventStart, LivingEntityUseItemEventStop, LivingEntityUseItemEventTick,
        LivingEventLivingJumpEvent, LivingEventLivingVisibilityEvent, LivingExperienceDropEvent,
        LivingFallEvent, LivingGetProjectileEvent, LivingKnockBackEvent, LivingShieldBlockEvent,
        MobEffectEventApplicable,
    },
};
use crate::server::Server;

/// The server of `entity` when a handler of `E` listens.
fn listening<E: Payload + 'static>(entity: &crate::entity::Entity) -> Option<Arc<Server>> {
    let server = entity.world.load().server.upgrade()?;
    server.plugin_manager.has_handlers::<E>().then_some(server)
}

/// The shared handle of `entity`, from its world.
fn shared(entity: &crate::entity::Entity) -> Option<Arc<dyn EntityBase>> {
    crate::plugin::entity_events::live(&entity.world.load(), entity)
}

/// Fires `E` built by `make` for `entity` when a handler listens, and returns it with the changes.
fn fire<E, F>(entity: &crate::entity::Entity, make: F) -> Option<E>
where
    E: Payload + Send + Sync + 'static,
    F: FnOnce(Arc<dyn EntityBase>) -> E,
{
    let server = listening::<E>(entity)?;
    let mut event = make(shared(entity)?);
    server.plugin_manager.fire_blocking(&server, &mut event);
    Some(event)
}

/// The [`DamageSource`] of a damage of `damage_with_context`.
#[must_use]
pub fn damage_source(
    living: &LivingEntity,
    damage_type: DamageType,
    position: Option<Vector3<f64>>,
    source: Option<&dyn EntityBase>,
    cause: Option<&dyn EntityBase>,
) -> DamageSource {
    let world = living.entity.world.load();
    let find =
        |entity: &dyn EntityBase| crate::plugin::entity_events::live(&world, entity.get_entity());
    DamageSource {
        damage_type,
        direct_entity: source.and_then(find),
        causing_entity: cause.and_then(find),
        source_position: position,
    }
}

/// Whether a handler of `E` listens to an event of `living`.
fn listens<E: Payload + 'static>(living: &LivingEntity) -> bool {
    listening::<E>(&living.entity).is_some()
}

/// Fires `LivingDamageEventPre` and returns the damage for the absorption and the health. A
/// damage that is not finite keeps the damage before the event: the health would turn NaN.
pub fn damage_pre(
    living: &LivingEntity,
    source: impl FnOnce() -> DamageSource,
    original_damage: f32,
    damage: f32,
) -> f32 {
    fire(&living.entity, |entity| {
        LivingDamageEventPre::new(entity, source(), original_damage, damage)
    })
    .map_or(damage, |event| {
        if event.new_damage.is_finite() {
            event.new_damage
        } else {
            damage
        }
    })
}

/// Fires `LivingDamageEventPost` built by `make` after a damage changed the health.
pub fn damage_post(
    living: &LivingEntity,
    make: impl FnOnce(Arc<dyn EntityBase>) -> LivingDamageEventPost,
) {
    let _ = fire(&living.entity, make);
}

/// Fires `LivingShieldBlockEvent` for a damage of `damage` to `living`, which uses a shield.
/// `vanilla_blocked` is whether the shield faces the damage, which gives the blocked damage of
/// the event, and `original_blocked` whether the damage does not bypass the shield. Returns the blocked damage and the durability loss of the
/// shield (-1 for the vanilla loss), or `None` when the shield blocks nothing.
pub fn shield_block(
    living: &LivingEntity,
    source: impl FnOnce() -> DamageSource,
    damage: f32,
    vanilla_blocked: bool,
    original_blocked: bool,
) -> Option<(f32, i32)> {
    let vanilla = (vanilla_blocked && original_blocked).then_some((damage, -1));
    // Vanilla `applyItemBlocking` returns before the event for a damage of 0.
    if damage <= 0.0 {
        return vanilla;
    }
    let Some(event) = fire(&living.entity, |entity| {
        let blocked_damage = if vanilla_blocked { damage } else { 0.0 };
        LivingShieldBlockEvent::new(entity, source(), damage, blocked_damage, original_blocked)
    }) else {
        return vanilla;
    };
    let blocked_damage = event.blocked_damage.clamp(0.0, damage);
    (event.blocked && blocked_damage > 0.0).then_some((blocked_damage, event.shield_damage))
}

/// Fires `LivingEntityUseItemEventStart` and returns the use duration, `None` when no use starts.
pub fn use_item_start(
    living: &LivingEntity,
    hand: Hand,
    item: &ItemStack,
    duration: i32,
) -> Option<i32> {
    // Vanilla `startUsingItem` starts only when no use runs; Pumpkin items call
    // `set_active_hand` again to refresh a running use.
    let using = || {
        living
            .item_in_use
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    };
    if listening::<LivingEntityUseItemEventStart>(&living.entity).is_none() || using() {
        return Some(duration);
    }
    let Some(event) = fire(&living.entity, |entity| {
        LivingEntityUseItemEventStart::new(entity, item.clone(), hand, duration)
    }) else {
        return Some(duration);
    };
    (!event.cancelled && event.duration >= 0).then_some(event.duration)
}

/// Fires `LivingEntityUseItemEventTick` for the item that `living` uses, before the item ticks,
/// and stores the remaining use ticks of the handlers. Returns `true` when the handlers left no
/// tick to a consumable, which then finishes in this tick. A crossbow, which vanilla releases on
/// `useOnRelease`, keeps its use, and the event no longer fires for it. Another item stops being
/// used: the server finishes only consumables.
pub fn use_item_tick(living: &LivingEntity) -> bool {
    use std::sync::atomic::Ordering::Relaxed;

    // Most entities use no item: the atomic load comes before the handler check.
    let remaining = living.item_use_time.load(Relaxed);
    if remaining <= 0 || listening::<LivingEntityUseItemEventTick>(&living.entity).is_none() {
        return false;
    }
    let item = living
        .item_in_use
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let Some(item) = item else {
        return false;
    };
    let hand = active_hand(living);
    let Some(event) = fire(&living.entity, |entity| {
        LivingEntityUseItemEventTick::new(entity, item.clone(), hand, remaining)
    }) else {
        return false;
    };
    let remaining = if event.cancelled { -1 } else { event.duration };
    living.item_use_time.store(remaining, Relaxed);
    if remaining > 0 {
        return false;
    }
    if item
        .get_data_component::<pumpkin_data::data_component_impl::food::ConsumableImpl>()
        .is_some()
    {
        return true;
    }
    if item.item.id != pumpkin_data::item::Item::CROSSBOW.id {
        living.clear_active_hand();
    }
    false
}

/// The hand of the item use of `living`, the main hand without one.
fn active_hand(living: &LivingEntity) -> Hand {
    living
        .active_hand
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .unwrap_or(Hand::Right)
}

/// Fires `LivingEntityUseItemEventStop` when `player` releases the item it uses. Returns `false`
/// when a handler cancelled it, so the item is not released.
pub fn use_item_stop(player: &Player, item: &ItemStack) -> bool {
    let living = &player.living_entity;
    !fire(&living.entity, |entity| {
        let hand = active_hand(living);
        let remaining = living
            .item_use_time
            .load(std::sync::atomic::Ordering::Relaxed);
        LivingEntityUseItemEventStop::new(entity, item.clone(), hand, remaining)
    })
    .is_some_and(|event| event.cancelled)
}

/// Fires `LivingKnockBackEvent` before the knockback resistance. Returns the strength and the
/// ratios, or `None` when a handler cancelled it. A strength that is negative or not finite, or a
/// ratio that is not finite, keeps the values before the event: the velocity would turn NaN.
pub fn knockback(
    entity: &dyn EntityBase,
    strength: f64,
    x: f64,
    z: f64,
) -> Option<(f64, f64, f64)> {
    if entity.get_living_entity().is_none() {
        return Some((strength, x, z));
    }
    // `NeoForge` hands the strength to the event as a `float`.
    let Some(event) = fire(entity.get_entity(), |entity| {
        LivingKnockBackEvent::new(entity, strength as f32, x, z)
    }) else {
        return Some((strength, x, z));
    };
    if event.cancelled {
        return None;
    }
    let valid = event.strength.is_finite()
        && event.strength >= 0.0
        && event.ratio_x.is_finite()
        && event.ratio_z.is_finite();
    Some(if valid {
        (f64::from(event.strength), event.ratio_x, event.ratio_z)
    } else {
        (strength, x, z)
    })
}

/// Fires `LivingFallEvent`. Returns the fall distance and the damage multiplier, or `None` when
/// a handler cancelled it.
pub fn fall(living: &LivingEntity, distance: f32, damage_multiplier: f32) -> Option<(f32, f32)> {
    let Some(event) = fire(&living.entity, |entity| {
        LivingFallEvent::new(entity, f64::from(distance), damage_multiplier)
    }) else {
        return Some((distance, damage_multiplier));
    };
    (!event.cancelled).then_some((event.distance as f32, event.damage_multiplier))
}

/// Fires `LivingDrownEvent` for a player out of air. Returns the drowning damage to deal, or
/// `None` when the player does not drown in this tick.
pub fn drown(player: &Player, drowning: bool) -> Option<f32> {
    let Some(event) = fire(&player.living_entity.entity, |entity| {
        LivingDrownEvent::new(entity, drowning)
    }) else {
        return drowning.then_some(crate::entity::breath::DROWNING_DAMAGE);
    };
    (!event.cancelled && event.drowning).then_some(event.damage_amount)
}

/// The player that `living` counts as its attacker, vanilla `getLastHurtByPlayer`: `killer` when
/// it is a player, else the player that hurt `living` last.
#[must_use]
pub fn attacking_player(
    living: &LivingEntity,
    killer: Option<&dyn EntityBase>,
) -> Option<Arc<Player>> {
    let world = living.entity.world.load();
    killer
        .and_then(EntityBase::get_player)
        .map(Player::entity_id)
        .or_else(|| {
            let id = living
                .last_hurt_by_player_id
                .load(std::sync::atomic::Ordering::Relaxed);
            (id != 0).then_some(id)
        })
        .and_then(|id| world.get_player_by_id(id))
}

/// Fires `LivingExperienceDropEvent` and returns the experience to drop.
pub fn experience_drop(
    living: &LivingEntity,
    attacking_player: impl FnOnce() -> Option<Arc<Player>>,
    experience: u32,
) -> u32 {
    let original = i32::try_from(experience).unwrap_or(i32::MAX);
    fire(&living.entity, |entity| {
        LivingExperienceDropEvent::new(entity, attacking_player(), original)
    })
    .map_or(experience, |event| {
        if event.cancelled {
            0
        } else {
            u32::try_from(event.dropped_experience).unwrap_or(0)
        }
    })
}

/// Fires `LivingGetProjectileEvent` for the ammunition `projectile` of `weapon`, which comes from
/// the inventory slot `slot`. Returns the projectile and its slot; a projectile that a handler
/// changed comes from no slot.
pub fn get_projectile(
    entity: &crate::entity::Entity,
    weapon: &ItemStack,
    projectile: ItemStack,
    slot: Option<usize>,
) -> (ItemStack, Option<usize>) {
    let Some(server) = listening::<LivingGetProjectileEvent>(entity) else {
        return (projectile, slot);
    };
    let Some(shared) = shared(entity) else {
        return (projectile, slot);
    };
    let mut event = LivingGetProjectileEvent::new(shared, weapon.clone(), projectile.clone());
    server.plugin_manager.fire_blocking(&server, &mut event);
    if event.projectile.item_count == projectile.item_count
        && event.projectile.are_items_and_components_equal(&projectile)
    {
        (projectile, slot)
    } else {
        (event.projectile, None)
    }
}

/// The ammunition of `player` for `weapon`, vanilla `Player.getProjectile`: the stack in the
/// inventory `slot` that the weapon found, or an arrow for a creative player, after
/// `LivingGetProjectileEvent`. Returns the projectile, empty for none, and the slot that it comes
/// from.
#[must_use]
pub fn player_projectile(
    player: &Player,
    weapon: &ItemStack,
    slot: Option<usize>,
) -> (ItemStack, Option<usize>) {
    let projectile = slot.map_or_else(
        || {
            if player.gamemode.load() == pumpkin_util::GameMode::Creative {
                ItemStack::new(1, &pumpkin_data::item::Item::ARROW)
            } else {
                ItemStack::EMPTY.clone()
            }
        },
        |slot| player.inventory.get_slot(slot),
    );
    get_projectile(player.get_entity(), weapon, projectile, slot)
}

/// The ammunition of a mob for `weapon`, vanilla `Monster.getProjectile`: an arrow, after
/// `LivingGetProjectileEvent`. A mob that gets no ammunition shoots an arrow, as
/// `ProjectileUtil.getMobArrow` does.
pub fn mob_projectile(entity: &crate::entity::Entity, weapon: &ItemStack) -> ItemStack {
    let arrow = || ItemStack::new(1, &pumpkin_data::item::Item::ARROW);
    let (projectile, _) = get_projectile(entity, weapon, arrow(), None);
    if projectile.is_empty() {
        arrow()
    } else {
        projectile
    }
}

/// Fires `LivingEventLivingJumpEvent` after `entity` jumped.
pub fn jump(entity: &crate::entity::Entity) {
    let _ = fire(entity, LivingEventLivingJumpEvent::new);
}

/// Fires `LivingEventLivingJumpEvent` for the jump that the movement packets of `player` show,
/// where vanilla calls `jumpFromGround`: the event fires only with a jump power above `1.0E-5`.
/// The client already moved the player.
pub fn player_jump(player: &Player) {
    let living = &player.living_entity;
    if listening::<LivingEventLivingJumpEvent>(&living.entity).is_some()
        && living.get_jump_velocity(1.0) > 1.0e-5
    {
        jump(&living.entity);
    }
}

/// Fires `LivingEventLivingVisibilityEvent` and returns the visibility, at least 0.
pub fn visibility(living: &LivingEntity, looking: Option<&dyn EntityBase>, visibility: f64) -> f64 {
    fire(&living.entity, |entity| {
        let looking = looking.and_then(|looking| shared(looking.get_entity()));
        LivingEventLivingVisibilityEvent::new(entity, looking, visibility)
    })
    .map_or(visibility, |event| event.visibility_modifier.max(0.0))
}

/// Fires `LivingDestroyBlockEvent` before `entity` destroys `state` at `pos`. Returns `false` when
/// a handler cancelled it.
pub fn destroy_block(
    entity: &crate::entity::Entity,
    pos: BlockPos,
    state: impl FnOnce() -> &'static BlockState,
) -> bool {
    !fire(entity, |entity| {
        LivingDestroyBlockEvent::new(entity, pos, state())
    })
    .is_some_and(|event| event.cancelled)
}

/// Fires `LivingConversionEventPost` after `entity` converted to `outcome`.
pub fn conversion_post(entity: &crate::entity::Entity, outcome: &Arc<dyn EntityBase>) {
    let _ = fire(entity, |entity| {
        LivingConversionEventPost::new(entity, outcome.clone())
    });
}

/// Whether a handler listens to `EffectParticleModificationEvent` for `living`.
#[must_use]
pub fn listens_to_effect_particles(living: &LivingEntity) -> bool {
    listens::<EffectParticleModificationEvent>(living)
}

/// Fires `EffectParticleModificationEvent` for `effect`, whose particle has the colour `color`.
/// Returns the colour of the particle, or `None` when the particle is not visible.
pub fn effect_particle(living: &LivingEntity, effect: &Effect, color: i32) -> Option<i32> {
    let Some(event) = fire(&living.entity, |entity| {
        EffectParticleModificationEvent::new(entity, effect.clone(), color)
    }) else {
        return effect.show_particles.then_some(color);
    };
    event.visible.then_some(event.particle_color)
}

/// Fires `MobEffectEventApplicable` and returns whether `effect` applies; `vanilla` is the
/// vanilla check.
pub fn effect_applicable(living: &LivingEntity, effect: &Effect, vanilla: bool) -> bool {
    fire(&living.entity, |entity| {
        MobEffectEventApplicable::new(entity, effect.clone(), None)
    })
    .map_or(vanilla, |event| event.application_result(vanilla))
}
