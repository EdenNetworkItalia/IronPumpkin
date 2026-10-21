//! The v0.2 conversions of the `NeoForge` 26.3 living entity events (`event.entity.living`) of
//! `pumpkin_core::plugin::living`.
//!
//! A blocking handler writes back only the settable fields; the other fields keep the native
//! values, as `NeoForge` has no setter for them.

use std::sync::Arc;

use pumpkin_core::plugin::{
    Context, EventPriority,
    living::{
        DamageReductions, DamageSource, EffectParticleModificationEvent, LivingConversionEventPost,
        LivingDamageEventPost, LivingDamageEventPre, LivingDestroyBlockEvent, LivingDrownEvent,
        LivingEntityUseItemEventStart, LivingEntityUseItemEventStop, LivingEntityUseItemEventTick,
        LivingEventLivingJumpEvent, LivingEventLivingVisibilityEvent, LivingExperienceDropEvent,
        LivingFallEvent, LivingGetProjectileEvent, LivingKnockBackEvent, LivingShieldBlockEvent,
        MobEffectEventApplicable, MobEffectEventApplicableResult,
    },
};
use pumpkin_data::{item_stack::ItemStack, potion::Effect};
use pumpkin_wasm_host_common::state::{FromResource, PluginHostState};
use tokio::sync::Mutex;
use wasmtime::component::Resource;

use crate::{
    events::{ToFromWasmEvent, WasmPluginEventHandler, to_wasm_block_position, to_wasm_hand},
    living_entity::to_wit_damage_type,
    pumpkin::plugin::{
        event::{Event, EventType},
        neoforge_events::{
            DamageReductions as WitDamageReductions, DamageSource as WitDamageSource,
            EffectParticleModificationEventData, LivingConversionEventPostData,
            LivingDamageEventPostData, LivingDamageEventPreData, LivingDestroyBlockEventData,
            LivingDrownEventData, LivingEntityUseItemEventStartData,
            LivingEntityUseItemEventStopData, LivingEntityUseItemEventTickData,
            LivingEventLivingJumpEventData, LivingEventLivingVisibilityEventData,
            LivingExperienceDropEventData, LivingFallEventData, LivingGetProjectileEventData,
            LivingKnockBackEventData, LivingShieldBlockEventData, MobEffectData,
            MobEffectEventApplicableData,
            MobEffectEventApplicableResult as WitMobEffectEventApplicableResult,
        },
    },
};

fn add<T: FromResource>(state: &mut PluginHostState, item: T::Internal) -> Resource<T> {
    state.add(item).expect("failed to add resource")
}

fn add_item_stack(
    state: &mut PluginHostState,
    item: &ItemStack,
) -> Resource<crate::pumpkin::plugin::item_stack::ItemStack> {
    add(state, Arc::new(Mutex::new(item.clone())))
}

fn discard<T: FromResource>(state: &mut PluginHostState, res: &Resource<T>) {
    state.discard_to_be_removed(res);
}

fn to_wasm_damage_source(state: &mut PluginHostState, source: &DamageSource) -> WitDamageSource {
    WitDamageSource {
        damage_type: to_wit_damage_type(&source.damage_type),
        direct_entity: source
            .direct_entity
            .clone()
            .map(|entity| add(state, entity)),
        causing_entity: source
            .causing_entity
            .clone()
            .map(|entity| add(state, entity)),
        source_position: source.source_position.map(super::to_wasm_position),
    }
}

fn discard_damage_source(state: &mut PluginHostState, source: &WitDamageSource) {
    if let Some(entity) = &source.direct_entity {
        discard(state, entity);
    }
    if let Some(entity) = &source.causing_entity {
        discard(state, entity);
    }
}

const fn to_wasm_reductions(reductions: DamageReductions) -> WitDamageReductions {
    WitDamageReductions {
        invulnerability: reductions.invulnerability,
        armor: reductions.armor,
        magic: reductions.magic,
        absorption: reductions.absorption,
    }
}

fn to_wasm_effect(effect: &Effect) -> MobEffectData {
    MobEffectData {
        effect_id: effect.effect_type.minecraft_name.to_string(),
        duration: effect.duration,
        amplifier: effect.amplifier,
        ambient: effect.ambient,
        show_particles: effect.show_particles,
        show_icon: effect.show_icon,
    }
}

const fn to_wasm_applicable_result(
    result: MobEffectEventApplicableResult,
) -> WitMobEffectEventApplicableResult {
    match result {
        MobEffectEventApplicableResult::Apply => WitMobEffectEventApplicableResult::Apply,
        MobEffectEventApplicableResult::Default => WitMobEffectEventApplicableResult::Default,
        MobEffectEventApplicableResult::DoNotApply => WitMobEffectEventApplicableResult::DoNotApply,
    }
}

const fn from_wasm_applicable_result(
    result: WitMobEffectEventApplicableResult,
) -> MobEffectEventApplicableResult {
    match result {
        WitMobEffectEventApplicableResult::Apply => MobEffectEventApplicableResult::Apply,
        WitMobEffectEventApplicableResult::Default => MobEffectEventApplicableResult::Default,
        WitMobEffectEventApplicableResult::DoNotApply => MobEffectEventApplicableResult::DoNotApply,
    }
}

/// Implements [`ToFromWasmEvent`] for an event of this module: `to` builds the record, `apply`
/// writes the settable fields of the returned record back.
macro_rules! living_event {
    ($event:ty, $variant:ident, |$this:ident, $state:ident| $to:expr, |$target:ident, $data:ident| $apply:block) => {
        impl ToFromWasmEvent for $event {
            fn to_wasm_event(&self, $state: &mut PluginHostState) -> Event {
                let $this = self;
                Event::$variant($to)
            }

            #[allow(unused_variables)]
            fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
                cleanup_neoforge_living_event(&event, state);
                if let Event::$variant($data) = event {
                    let $target = self;
                    $apply
                }
            }

            fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
                panic!(concat!(
                    "Cannot construct ",
                    stringify!($event),
                    " from WASM"
                ))
            }
        }
    };
}

living_event!(
    LivingDamageEventPre,
    LivingDamageEventPre,
    |this, state| LivingDamageEventPreData {
        entity: add(state, this.entity.clone()),
        source: to_wasm_damage_source(state, &this.source),
        original_damage: this.original_damage,
        new_damage: this.new_damage,
    },
    |target, data| {
        target.new_damage = data.new_damage;
    }
);

living_event!(
    LivingDamageEventPost,
    LivingDamageEventPost,
    |this, state| LivingDamageEventPostData {
        entity: add(state, this.entity.clone()),
        source: to_wasm_damage_source(state, &this.source),
        original_damage: this.original_damage,
        inflicted_damage: this.inflicted_damage,
        health_damage: this.health_damage,
        blocked_damage: this.blocked_damage,
        shield_damage: this.shield_damage,
        post_attack_invulnerability_ticks: this.post_attack_invulnerability_ticks,
        should_cause_side_effects: this.should_cause_side_effects,
        reductions: to_wasm_reductions(this.reductions),
    },
    |target, data| {}
);

living_event!(
    LivingEntityUseItemEventStart,
    LivingEntityUseItemEventStart,
    |this, state| LivingEntityUseItemEventStartData {
        entity: add(state, this.entity.clone()),
        item: add_item_stack(state, &this.item),
        hand: to_wasm_hand(this.hand),
        duration: this.duration,
        cancelled: this.cancelled,
    },
    |target, data| {
        target.duration = data.duration;
        target.cancelled = data.cancelled;
    }
);

living_event!(
    LivingEntityUseItemEventTick,
    LivingEntityUseItemEventTick,
    |this, state| LivingEntityUseItemEventTickData {
        entity: add(state, this.entity.clone()),
        item: add_item_stack(state, &this.item),
        hand: to_wasm_hand(this.hand),
        duration: this.duration,
        cancelled: this.cancelled,
    },
    |target, data| {
        target.duration = data.duration;
        target.cancelled = data.cancelled;
    }
);

living_event!(
    LivingEntityUseItemEventStop,
    LivingEntityUseItemEventStop,
    |this, state| LivingEntityUseItemEventStopData {
        entity: add(state, this.entity.clone()),
        item: add_item_stack(state, &this.item),
        hand: to_wasm_hand(this.hand),
        duration: this.duration,
        cancelled: this.cancelled,
    },
    |target, data| {
        target.cancelled = data.cancelled;
    }
);

living_event!(
    LivingKnockBackEvent,
    LivingKnockBackEvent,
    |this, state| LivingKnockBackEventData {
        entity: add(state, this.entity.clone()),
        strength: this.strength,
        ratio_x: this.ratio_x,
        ratio_z: this.ratio_z,
        original_strength: this.original_strength,
        original_ratio_x: this.original_ratio_x,
        original_ratio_z: this.original_ratio_z,
        cancelled: this.cancelled,
    },
    |target, data| {
        target.strength = data.strength;
        target.ratio_x = data.ratio_x;
        target.ratio_z = data.ratio_z;
        target.cancelled = data.cancelled;
    }
);

living_event!(
    LivingFallEvent,
    LivingFallEvent,
    |this, state| LivingFallEventData {
        entity: add(state, this.entity.clone()),
        distance: this.distance,
        damage_multiplier: this.damage_multiplier,
        cancelled: this.cancelled,
    },
    |target, data| {
        target.distance = data.distance;
        target.damage_multiplier = data.damage_multiplier;
        target.cancelled = data.cancelled;
    }
);

living_event!(
    LivingDrownEvent,
    LivingDrownEvent,
    |this, state| LivingDrownEventData {
        entity: add(state, this.entity.clone()),
        drowning: this.drowning,
        damage_amount: this.damage_amount,
        bubble_count: this.bubble_count,
        cancelled: this.cancelled,
    },
    |target, data| {
        target.drowning = data.drowning;
        target.damage_amount = data.damage_amount;
        target.bubble_count = data.bubble_count;
        target.cancelled = data.cancelled;
    }
);

living_event!(
    LivingExperienceDropEvent,
    LivingExperienceDropEvent,
    |this, state| LivingExperienceDropEventData {
        entity: add(state, this.entity.clone()),
        attacking_player: this
            .attacking_player
            .clone()
            .map(|player| add(state, player)),
        original_experience: this.original_experience,
        dropped_experience: this.dropped_experience,
        cancelled: this.cancelled,
    },
    |target, data| {
        target.dropped_experience = data.dropped_experience;
        target.cancelled = data.cancelled;
    }
);

living_event!(
    LivingShieldBlockEvent,
    LivingShieldBlockEvent,
    |this, state| LivingShieldBlockEventData {
        entity: add(state, this.entity.clone()),
        source: to_wasm_damage_source(state, &this.source),
        damage: this.damage,
        original_blocked_damage: this.original_blocked_damage,
        blocked_damage: this.blocked_damage,
        shield_damage: this.shield_damage,
        original_blocked: this.original_blocked,
        blocked: this.blocked,
    },
    |target, data| {
        target.blocked_damage = data.blocked_damage;
        target.shield_damage = data.shield_damage;
        target.blocked = data.blocked;
    }
);

impl ToFromWasmEvent for LivingGetProjectileEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::LivingGetProjectileEvent(LivingGetProjectileEventData {
            entity: add(state, self.entity.clone()),
            projectile_weapon: add_item_stack(state, &self.projectile_weapon),
            projectile: add_item_stack(state, &self.projectile),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        let Event::LivingGetProjectileEvent(data) = event else {
            cleanup_neoforge_living_event(&event, state);
            return;
        };
        discard(state, &data.entity);
        discard(state, &data.projectile_weapon);
        // The guest hands back the stack of `projectile`, its own or a new one.
        if let Ok(stack) = state.take(data.projectile)
            && let Ok(stack) = stack.try_lock()
        {
            self.projectile = stack.clone();
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct LivingGetProjectileEvent from WASM")
    }
}

living_event!(
    LivingEventLivingJumpEvent,
    LivingEventLivingJumpEvent,
    |this, state| LivingEventLivingJumpEventData {
        entity: add(state, this.entity.clone()),
    },
    |target, data| {}
);

living_event!(
    LivingEventLivingVisibilityEvent,
    LivingEventLivingVisibilityEvent,
    |this, state| LivingEventLivingVisibilityEventData {
        entity: add(state, this.entity.clone()),
        looking_entity: this.looking_entity.clone().map(|entity| add(state, entity)),
        visibility_modifier: this.visibility_modifier,
    },
    |target, data| {
        target.visibility_modifier = data.visibility_modifier;
    }
);

living_event!(
    LivingDestroyBlockEvent,
    LivingDestroyBlockEvent,
    |this, state| LivingDestroyBlockEventData {
        entity: add(state, this.entity.clone()),
        block_pos: to_wasm_block_position(this.pos),
        state_id: this.state.id.as_u16(),
        cancelled: this.cancelled,
    },
    |target, data| {
        target.cancelled = data.cancelled;
    }
);

living_event!(
    LivingConversionEventPost,
    LivingConversionEventPost,
    |this, state| LivingConversionEventPostData {
        entity: add(state, this.entity.clone()),
        outcome: add(state, this.outcome.clone()),
    },
    |target, data| {}
);

living_event!(
    EffectParticleModificationEvent,
    EffectParticleModificationEvent,
    |this, state| EffectParticleModificationEventData {
        entity: add(state, this.entity.clone()),
        effect: to_wasm_effect(&this.effect),
        original_particle_color: this.original_particle_color,
        particle_color: this.particle_color,
        visible: this.visible,
    },
    |target, data| {
        target.particle_color = data.particle_color;
        target.visible = data.visible;
    }
);

living_event!(
    MobEffectEventApplicable,
    MobEffectEventApplicable,
    |this, state| MobEffectEventApplicableData {
        entity: add(state, this.entity.clone()),
        effect: to_wasm_effect(&this.effect),
        effect_source: this.effect_source.clone().map(|entity| add(state, entity)),
        result: to_wasm_applicable_result(this.result),
    },
    |target, data| {
        target.result = from_wasm_applicable_result(data.result);
    }
);

/// Discards the resources of a returned event of this module.
pub(super) fn cleanup_neoforge_living_event(event: &Event, state: &mut PluginHostState) {
    match event {
        Event::LivingDamageEventPre(LivingDamageEventPreData { entity, source, .. })
        | Event::LivingDamageEventPost(LivingDamageEventPostData { entity, source, .. })
        | Event::LivingShieldBlockEvent(LivingShieldBlockEventData { entity, source, .. }) => {
            discard(state, entity);
            discard_damage_source(state, source);
        }
        Event::LivingEntityUseItemEventStart(LivingEntityUseItemEventStartData {
            entity,
            item,
            ..
        })
        | Event::LivingEntityUseItemEventTick(LivingEntityUseItemEventTickData {
            entity,
            item,
            ..
        })
        | Event::LivingEntityUseItemEventStop(LivingEntityUseItemEventStopData {
            entity,
            item,
            ..
        }) => {
            discard(state, entity);
            discard(state, item);
        }
        Event::LivingExperienceDropEvent(data) => {
            discard(state, &data.entity);
            if let Some(player) = &data.attacking_player {
                discard(state, player);
            }
        }
        Event::LivingGetProjectileEvent(data) => {
            discard(state, &data.entity);
            discard(state, &data.projectile_weapon);
            discard(state, &data.projectile);
        }
        Event::LivingEventLivingVisibilityEvent(data) => {
            discard(state, &data.entity);
            if let Some(entity) = &data.looking_entity {
                discard(state, entity);
            }
        }
        Event::LivingConversionEventPost(data) => {
            discard(state, &data.entity);
            discard(state, &data.outcome);
        }
        Event::MobEffectEventApplicable(data) => {
            discard(state, &data.entity);
            if let Some(entity) = &data.effect_source {
                discard(state, entity);
            }
        }
        Event::LivingKnockBackEvent(LivingKnockBackEventData { entity, .. })
        | Event::LivingFallEvent(LivingFallEventData { entity, .. })
        | Event::LivingDrownEvent(LivingDrownEventData { entity, .. })
        | Event::LivingEventLivingJumpEvent(LivingEventLivingJumpEventData { entity })
        | Event::LivingDestroyBlockEvent(LivingDestroyBlockEventData { entity, .. })
        | Event::EffectParticleModificationEvent(EffectParticleModificationEventData {
            entity,
            ..
        }) => discard(state, entity),
        _ => {}
    }
}

/// Registers a Wasm handler for an event of this module.
pub fn register_neoforge_living_event(
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
        EventType::LivingDamageEventPre => {
            on::<LivingDamageEventPre>(ctx, handler, priority, blocking);
        }
        EventType::LivingDamageEventPost => {
            on::<LivingDamageEventPost>(ctx, handler, priority, blocking);
        }
        EventType::LivingEntityUseItemEventStart => {
            on::<LivingEntityUseItemEventStart>(ctx, handler, priority, blocking);
        }
        EventType::LivingEntityUseItemEventTick => {
            on::<LivingEntityUseItemEventTick>(ctx, handler, priority, blocking);
        }
        EventType::LivingEntityUseItemEventStop => {
            on::<LivingEntityUseItemEventStop>(ctx, handler, priority, blocking);
        }
        EventType::LivingKnockBackEvent => {
            on::<LivingKnockBackEvent>(ctx, handler, priority, blocking);
        }
        EventType::LivingFallEvent => on::<LivingFallEvent>(ctx, handler, priority, blocking),
        EventType::LivingDrownEvent => on::<LivingDrownEvent>(ctx, handler, priority, blocking),
        EventType::LivingExperienceDropEvent => {
            on::<LivingExperienceDropEvent>(ctx, handler, priority, blocking);
        }
        EventType::LivingShieldBlockEvent => {
            on::<LivingShieldBlockEvent>(ctx, handler, priority, blocking);
        }
        EventType::LivingGetProjectileEvent => {
            on::<LivingGetProjectileEvent>(ctx, handler, priority, blocking);
        }
        EventType::LivingEventLivingJumpEvent => {
            on::<LivingEventLivingJumpEvent>(ctx, handler, priority, blocking);
        }
        EventType::LivingEventLivingVisibilityEvent => {
            on::<LivingEventLivingVisibilityEvent>(ctx, handler, priority, blocking);
        }
        EventType::LivingDestroyBlockEvent => {
            on::<LivingDestroyBlockEvent>(ctx, handler, priority, blocking);
        }
        EventType::LivingConversionEventPost => {
            on::<LivingConversionEventPost>(ctx, handler, priority, blocking);
        }
        EventType::EffectParticleModificationEvent => {
            on::<EffectParticleModificationEvent>(ctx, handler, priority, blocking);
        }
        EventType::MobEffectEventApplicable => {
            on::<MobEffectEventApplicable>(ctx, handler, priority, blocking);
        }
        _ => tracing::error!(
            "non-living NeoForge event should not be routed to register_neoforge_living_event"
        ),
    }
}
