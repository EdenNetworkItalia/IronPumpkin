//! The `NeoForge` living entity events (`event.entity.living`) at fire sites where Pumpkin has no
//! event. Where Pumpkin has an event at the fire site, the `NeoForge` event maps to it
//! (`mapping-table.md`).
//!
//! A nested class takes its parent name as a prefix: `LivingDamageEvent.Pre` is
//! [`LivingDamageEventPre`], `LivingEvent.LivingJumpEvent` is [`LivingEventLivingJumpEvent`]. The
//! entity of an event is the living entity, a player included.

use std::sync::Arc;

use pumpkin_data::{BlockState, damage::DamageType, item_stack::ItemStack, potion::Effect};
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::{
    Hand,
    math::{position::BlockPos, vector3::Vector3},
};

use crate::entity::{EntityBase, player::Player};

/// The source of a damage, `NeoForge` `DamageSource`.
#[derive(Clone)]
pub struct DamageSource {
    pub damage_type: DamageType,
    /// The entity that dealt the damage, such as an arrow.
    pub direct_entity: Option<Arc<dyn EntityBase>>,
    /// The entity responsible for the damage, such as the shooter of the arrow.
    pub causing_entity: Option<Arc<dyn EntityBase>>,
    /// The position that the damage comes from, for a damage without an entity.
    pub source_position: Option<Vector3<f64>>,
}

/// The reductions of a damage, `DamageContainer.Reduction` of `NeoForge`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DamageReductions {
    /// The damage that the hurt cooldown took: the last damage of a hit during the cooldown.
    pub invulnerability: f32,
    pub armor: f32,
    /// `ENCHANTMENTS` and `MOB_EFFECTS` together: Pumpkin computes the resistance effect and the
    /// protection enchantments in one step.
    pub magic: f32,
    pub absorption: f32,
}

/// Fires before a damage changes the health of a living entity, after the armor, the effects and
/// the hurt cooldown reduced it and before the absorption takes its part.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.LivingDamageEvent.Pre`, fired by
/// `CommonHooks.onLivingDamagePre` in `LivingEntity.actuallyHurt`. `new_damage` is settable; the
/// absorption and the health take it. Pumpkin has no modifiers of the reductions.
#[derive(Event, Clone)]
pub struct LivingDamageEventPre {
    pub entity: Arc<dyn EntityBase>,
    pub source: DamageSource,
    /// The damage when the hurt began, after `EntityDamageEvent` (`LivingIncomingDamageEvent`).
    pub original_damage: f32,
    pub new_damage: f32,
}

impl LivingDamageEventPre {
    #[must_use]
    pub const fn new(
        entity: Arc<dyn EntityBase>,
        source: DamageSource,
        original_damage: f32,
        new_damage: f32,
    ) -> Self {
        Self {
            entity,
            source,
            original_damage,
            new_damage,
        }
    }
}

/// Fires after a damage changed the health of a living entity, with the final values of the hurt.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.LivingDamageEvent.Post`, fired by
/// `CommonHooks.onLivingDamagePost` at the end of `LivingEntity.hurtServer`, also after a damage
/// that killed the entity.
#[derive(Event, Clone)]
pub struct LivingDamageEventPost {
    pub entity: Arc<dyn EntityBase>,
    pub source: DamageSource,
    pub original_damage: f32,
    /// The damage after every reduction except the absorption.
    pub inflicted_damage: f32,
    /// The health that the entity lost.
    pub health_damage: f32,
    /// The damage that a shield blocked.
    pub blocked_damage: f32,
    /// The durability that the shield lost.
    pub shield_damage: f32,
    pub post_attack_invulnerability_ticks: i32,
    /// `false` for a hit during the hurt cooldown, which plays no hurt sound and gives no
    /// knockback.
    pub should_cause_side_effects: bool,
    pub reductions: DamageReductions,
}

/// The concrete parent of the item use events, which the server never fires.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.LivingEntityUseItemEvent`. Register a
/// [`ParentEventHandler`](crate::plugin::ParentEventHandler) for it with
/// `ModInit::register_parent_event` to get [`LivingEntityUseItemEventStart`],
/// [`LivingEntityUseItemEventTick`] and [`LivingEntityUseItemEventStop`].
/// `LivingEntityUseItemEvent.Finish` is `PlayerItemConsumeEvent`, which does not reach a parent
/// handler.
#[derive(Event, Clone)]
pub struct LivingEntityUseItemEvent;

/// Fires when a living entity starts to use an item, such as a food, a bow or a shield.
///
/// `NeoForge`: `LivingEntityUseItemEvent.Start`, fired by `EventHooks.onItemUseStart` in
/// `LivingEntity.startUsingItem`. `duration` is settable. A cancel or a negative `duration` starts
/// no use.
#[cancellable]
#[derive(Event, Clone)]
#[event_parents(LivingEntityUseItemEvent)]
pub struct LivingEntityUseItemEventStart {
    pub entity: Arc<dyn EntityBase>,
    pub item: ItemStack,
    pub hand: Hand,
    pub duration: i32,
}

impl LivingEntityUseItemEventStart {
    #[must_use]
    pub const fn new(
        entity: Arc<dyn EntityBase>,
        item: ItemStack,
        hand: Hand,
        duration: i32,
    ) -> Self {
        Self {
            entity,
            item,
            hand,
            duration,
            cancelled: false,
        }
    }
}

/// Fires in each tick of an item use, before the item ticks.
///
/// `NeoForge`: `LivingEntityUseItemEvent.Tick`, fired by `EventHooks.onItemUseTick` in
/// `LivingEntity.updateUsingItem`. `duration` is settable and becomes the remaining use ticks. A
/// cancel sets them to -1. With no remaining tick the item does not tick, and the use finishes as
/// if its time ran out.
#[cancellable]
#[derive(Event, Clone)]
#[event_parents(LivingEntityUseItemEvent)]
pub struct LivingEntityUseItemEventTick {
    pub entity: Arc<dyn EntityBase>,
    pub item: ItemStack,
    pub hand: Hand,
    pub duration: i32,
}

impl LivingEntityUseItemEventTick {
    #[must_use]
    pub const fn new(
        entity: Arc<dyn EntityBase>,
        item: ItemStack,
        hand: Hand,
        duration: i32,
    ) -> Self {
        Self {
            entity,
            item,
            hand,
            duration,
            cancelled: false,
        }
    }
}

/// Fires when a player releases an item before its use ended, such as a drawn bow.
///
/// `NeoForge`: `LivingEntityUseItemEvent.Stop`, fired by `EventHooks.onUseItemStop` in
/// `LivingEntity.releaseUsingItem`. `duration` is the remaining use ticks. A cancel skips the
/// release of the item, so a bow shoots nothing; the use still stops.
#[cancellable]
#[derive(Event, Clone)]
#[event_parents(LivingEntityUseItemEvent)]
pub struct LivingEntityUseItemEventStop {
    pub entity: Arc<dyn EntityBase>,
    pub item: ItemStack,
    pub hand: Hand,
    pub duration: i32,
}

impl LivingEntityUseItemEventStop {
    #[must_use]
    pub const fn new(
        entity: Arc<dyn EntityBase>,
        item: ItemStack,
        hand: Hand,
        duration: i32,
    ) -> Self {
        Self {
            entity,
            item,
            hand,
            duration,
            cancelled: false,
        }
    }
}

/// Fires before a living entity takes a knockback, before the knockback resistance scales it.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.LivingKnockBackEvent`, fired by
/// `CommonHooks.onLivingKnockBack` in `LivingEntity.knockback`: the knockback of a damage and of a
/// player attack. `strength`, `ratio_x` and `ratio_z` are settable. A cancel gives no knockback.
#[cancellable]
#[derive(Event, Clone)]
pub struct LivingKnockBackEvent {
    pub entity: Arc<dyn EntityBase>,
    pub strength: f32,
    pub ratio_x: f64,
    pub ratio_z: f64,
    pub original_strength: f32,
    pub original_ratio_x: f64,
    pub original_ratio_z: f64,
}

impl LivingKnockBackEvent {
    #[must_use]
    pub const fn new(
        entity: Arc<dyn EntityBase>,
        strength: f32,
        ratio_x: f64,
        ratio_z: f64,
    ) -> Self {
        Self {
            entity,
            strength,
            ratio_x,
            ratio_z,
            original_strength: strength,
            original_ratio_x: ratio_x,
            original_ratio_z: ratio_z,
            cancelled: false,
        }
    }
}

/// Fires when a living entity lands and may take fall damage.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.LivingFallEvent`, fired by
/// `CommonHooks.onLivingFall` in `LivingEntity.causeFallDamage`, after the flight check and the
/// fall statistic of a player. `distance` and `damage_multiplier` are settable. A cancel deals no
/// fall damage. The fall distance of a player comes from its client.
#[cancellable]
#[derive(Event, Clone)]
pub struct LivingFallEvent {
    pub entity: Arc<dyn EntityBase>,
    pub distance: f64,
    pub damage_multiplier: f32,
}

impl LivingFallEvent {
    #[must_use]
    pub const fn new(entity: Arc<dyn EntityBase>, distance: f64, damage_multiplier: f32) -> Self {
        Self {
            entity,
            distance,
            damage_multiplier,
            cancelled: false,
        }
    }
}

/// Fires in each tick that a player is out of air under water.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.LivingDrownEvent`, fired by
/// `CommonHooks.onLivingBreathe`. `drowning` is `true` in the tick that deals the drowning damage.
/// `drowning`, `damage_amount` and `bubble_count` are settable. A cancel or `drowning` set to
/// `false` deals no damage. `bubble_count` has no effect on the server, as in vanilla. Pumpkin
/// ticks the air of players only.
#[cancellable]
#[derive(Event, Clone)]
pub struct LivingDrownEvent {
    pub entity: Arc<dyn EntityBase>,
    pub drowning: bool,
    pub damage_amount: f32,
    pub bubble_count: i32,
}

impl LivingDrownEvent {
    /// The vanilla count of bubbles.
    pub const BUBBLES: i32 = 8;

    #[must_use]
    pub const fn new(entity: Arc<dyn EntityBase>, drowning: bool) -> Self {
        Self {
            entity,
            drowning,
            damage_amount: crate::entity::breath::DROWNING_DAMAGE,
            bubble_count: Self::BUBBLES,
            cancelled: false,
        }
    }
}

/// Fires before a killed living entity drops its experience.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.LivingExperienceDropEvent`, fired by
/// `EventHooks.getExperienceDrop` in `LivingEntity.dropExperience`. `dropped_experience` is
/// settable. A cancel drops no experience.
#[cancellable]
#[derive(Event, Clone)]
pub struct LivingExperienceDropEvent {
    pub entity: Arc<dyn EntityBase>,
    pub attacking_player: Option<Arc<Player>>,
    pub original_experience: i32,
    pub dropped_experience: i32,
}

impl LivingExperienceDropEvent {
    #[must_use]
    pub const fn new(
        entity: Arc<dyn EntityBase>,
        attacking_player: Option<Arc<Player>>,
        experience: i32,
    ) -> Self {
        Self {
            entity,
            attacking_player,
            original_experience: experience,
            dropped_experience: experience,
            cancelled: false,
        }
    }
}

/// Fires when a living entity that uses a shield takes a damage.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.LivingShieldBlockEvent`, fired by
/// `CommonHooks.onDamageBlock` in `LivingEntity.applyItemBlocking`. `blocked`, `blocked_damage`
/// and `shield_damage` are settable. `original_blocked` is `false` for a damage that bypasses the
/// shield. With `blocked` set to `false` the shield blocks nothing. `blocked_damage` is clamped
/// to `damage`, and `shield_damage` -1 keeps the vanilla durability loss.
#[derive(Event, Clone)]
pub struct LivingShieldBlockEvent {
    pub entity: Arc<dyn EntityBase>,
    pub source: DamageSource,
    /// The damage at the shield.
    pub damage: f32,
    pub original_blocked_damage: f32,
    pub blocked_damage: f32,
    pub shield_damage: i32,
    pub original_blocked: bool,
    pub blocked: bool,
}

impl LivingShieldBlockEvent {
    #[must_use]
    pub const fn new(
        entity: Arc<dyn EntityBase>,
        source: DamageSource,
        damage: f32,
        blocked_damage: f32,
        original_blocked: bool,
    ) -> Self {
        Self {
            entity,
            source,
            damage,
            original_blocked_damage: blocked_damage,
            blocked_damage,
            shield_damage: -1,
            original_blocked,
            blocked: original_blocked,
        }
    }
}

/// Fires when a living entity looks for the ammunition of a bow or a crossbow.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.LivingGetProjectileEvent`, fired by
/// `CommonHooks.getProjectile` in `Player.getProjectile` and `Monster.getProjectile`.
/// `projectile` is settable, and empty is no ammunition. A projectile that a handler sets is a
/// copy: the server takes nothing from the inventory for it.
#[derive(Event, Clone)]
pub struct LivingGetProjectileEvent {
    pub entity: Arc<dyn EntityBase>,
    pub projectile_weapon: ItemStack,
    pub projectile: ItemStack,
}

impl LivingGetProjectileEvent {
    #[must_use]
    pub const fn new(
        entity: Arc<dyn EntityBase>,
        projectile_weapon: ItemStack,
        projectile: ItemStack,
    ) -> Self {
        Self {
            entity,
            projectile_weapon,
            projectile,
        }
    }
}

/// Fires after a living entity jumped from the ground.
///
/// `NeoForge`: `LivingEvent.LivingJumpEvent`, fired by `CommonHooks.onLivingJump` in
/// `LivingEntity.jumpFromGround`. A mob jumps in its own tick. The movement of a player comes
/// from its client: the server fires the event when the movement packets show the jump, after
/// the client jumped, and a handler cannot change the jump.
#[derive(Event, Clone)]
pub struct LivingEventLivingJumpEvent {
    pub entity: Arc<dyn EntityBase>,
}

impl LivingEventLivingJumpEvent {
    #[must_use]
    pub const fn new(entity: Arc<dyn EntityBase>) -> Self {
        Self { entity }
    }
}

/// Fires when a mob reads how visible a living entity is, for its targeting.
///
/// `NeoForge`: `LivingEvent.LivingVisibilityEvent`, fired by
/// `CommonHooks.getEntityVisibilityMultiplier` in `LivingEntity.getVisibilityPercent`.
/// `visibility_modifier` is settable and starts at the vanilla visibility; the result is at least
/// 0 and at most 10.
#[derive(Event, Clone)]
pub struct LivingEventLivingVisibilityEvent {
    pub entity: Arc<dyn EntityBase>,
    pub looking_entity: Option<Arc<dyn EntityBase>>,
    pub visibility_modifier: f64,
}

impl LivingEventLivingVisibilityEvent {
    #[must_use]
    pub const fn new(
        entity: Arc<dyn EntityBase>,
        looking_entity: Option<Arc<dyn EntityBase>>,
        visibility_modifier: f64,
    ) -> Self {
        Self {
            entity,
            looking_entity,
            visibility_modifier,
        }
    }

    /// Multiplies the visibility, `modifyVisibility` of `NeoForge`.
    pub fn modify_visibility(&mut self, modifier: f64) {
        self.visibility_modifier *= modifier;
    }
}

/// Fires before a living entity destroys a block: a wither that breaks the blocks around it, or a
/// mob that starts to break a door.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.LivingDestroyBlockEvent`, fired by
/// `EventHooks.onEntityDestroyBlock` in `WitherBoss.customServerAiStep` and through
/// `CommonHooks.canEntityDestroy` in `BreakDoorGoal.canUse`. A cancel keeps the block.
#[cancellable]
#[derive(Event, Clone)]
pub struct LivingDestroyBlockEvent {
    pub entity: Arc<dyn EntityBase>,
    pub pos: BlockPos,
    pub state: &'static BlockState,
}

impl LivingDestroyBlockEvent {
    #[must_use]
    pub const fn new(
        entity: Arc<dyn EntityBase>,
        pos: BlockPos,
        state: &'static BlockState,
    ) -> Self {
        Self {
            entity,
            pos,
            state,
            cancelled: false,
        }
    }
}

/// Fires when a mob converted to another mob, before the new mob joins the world.
///
/// `NeoForge`: `LivingConversionEvent.Post`, fired by `EventHooks.onLivingConvert` in the
/// conversion of a hoglin to a zoglin and of a piglin or a piglin brute to a zombified piglin.
#[derive(Event, Clone)]
pub struct LivingConversionEventPost {
    pub entity: Arc<dyn EntityBase>,
    pub outcome: Arc<dyn EntityBase>,
}

impl LivingConversionEventPost {
    #[must_use]
    pub const fn new(entity: Arc<dyn EntityBase>, outcome: Arc<dyn EntityBase>) -> Self {
        Self { entity, outcome }
    }
}

/// Fires for each active effect when a living entity updates the particles of its effects.
///
/// `NeoForge`: `net.neoforged.neoforge.event.entity.living.EffectParticleModificationEvent`,
/// fired in `LivingEntity.updateSynchronizedMobEffectParticles`. `visible` and `particle_color`
/// are settable. Pumpkin sends the effect particle `minecraft:entity_effect` only, so the
/// particle options are its ARGB colour.
#[derive(Event, Clone)]
pub struct EffectParticleModificationEvent {
    pub entity: Arc<dyn EntityBase>,
    pub effect: Effect,
    pub original_particle_color: i32,
    pub particle_color: i32,
    pub visible: bool,
}

impl EffectParticleModificationEvent {
    #[must_use]
    pub const fn new(entity: Arc<dyn EntityBase>, effect: Effect, particle_color: i32) -> Self {
        let visible = effect.show_particles;
        Self {
            entity,
            effect,
            original_particle_color: particle_color,
            particle_color,
            visible,
        }
    }
}

/// The result of [`MobEffectEventApplicable`], `MobEffectEvent.Applicable.Result` of `NeoForge`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MobEffectEventApplicableResult {
    /// The effect applies, whatever vanilla says.
    Apply,
    /// Vanilla decides.
    Default,
    /// The effect does not apply.
    DoNotApply,
}

/// Fires before an effect applies to a living entity.
///
/// `NeoForge`: `MobEffectEvent.Applicable`, fired by `CommonHooks.canMobEffectBeApplied` in
/// `LivingEntity.addEffect`, before `MobEffectEvent.Added` (`EntityPotionEffectEvent`). `result`
/// is settable. Pumpkin knows no source entity of an effect, so `effect_source` is `None`.
#[derive(Event, Clone)]
pub struct MobEffectEventApplicable {
    pub entity: Arc<dyn EntityBase>,
    pub effect: Effect,
    pub effect_source: Option<Arc<dyn EntityBase>>,
    pub result: MobEffectEventApplicableResult,
}

impl MobEffectEventApplicable {
    #[must_use]
    pub const fn new(
        entity: Arc<dyn EntityBase>,
        effect: Effect,
        effect_source: Option<Arc<dyn EntityBase>>,
    ) -> Self {
        Self {
            entity,
            effect,
            effect_source,
            result: MobEffectEventApplicableResult::Default,
        }
    }

    /// Whether the effect applies, `getApplicationResult` of `NeoForge`: `vanilla` is the check of
    /// `canBeAffected`.
    #[must_use]
    pub const fn application_result(&self, vanilla: bool) -> bool {
        match self.result {
            MobEffectEventApplicableResult::Apply => true,
            MobEffectEventApplicableResult::Default => vanilla,
            MobEffectEventApplicableResult::DoNotApply => false,
        }
    }
}
