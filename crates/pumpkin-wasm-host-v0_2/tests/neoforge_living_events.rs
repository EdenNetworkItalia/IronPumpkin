//! The living entity events of `NeoForge` 26.3 reach v0.2 plugins (spec `neoforge-event-bus`,
//! requirement "Wasm plugins see the same events", #137): a v0.2 handler receives each event with
//! its `NeoForge` fields, writes back the settable fields and cancels it, and the host keeps the
//! fields that `NeoForge` cannot set. The guest helper checks that each round trip releases its
//! resources.
#![expect(
    clippy::panic,
    clippy::expect_used,
    reason = "a test fails on the first unexpected record"
)]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

mod guest;

use std::sync::Arc;

use guest::{drop_resource, fire, plugin, resource};
use pumpkin_core::{
    entity::{EntityBase, r#type::from_type},
    plugin::living::{
        DamageReductions, DamageSource, EffectParticleModificationEvent, LivingConversionEventPost,
        LivingDamageEventPost, LivingDamageEventPre, LivingDestroyBlockEvent, LivingDrownEvent,
        LivingEntityUseItemEventStart, LivingEntityUseItemEventStop, LivingEntityUseItemEventTick,
        LivingEventLivingJumpEvent, LivingEventLivingVisibilityEvent, LivingExperienceDropEvent,
        LivingFallEvent, LivingGetProjectileEvent, LivingKnockBackEvent, LivingShieldBlockEvent,
        MobEffectEventApplicable, MobEffectEventApplicableResult,
    },
};
use pumpkin_data::{
    Block, damage::DamageType, effect::StatusEffect, entity::EntityType, item::Item,
    item_stack::ItemStack, potion::Effect,
};
use pumpkin_util::{
    Hand,
    math::{position::BlockPos, vector3::Vector3},
};
use pumpkin_wasm_host_v0_2::pumpkin::plugin::{
    common::Hand as WitHand, damage_types::DamageType as WitDamageType, event::Event,
    item_stack::ItemStack as WitItemStack,
    neoforge_events::MobEffectEventApplicableResult as WitResult,
};
use wasmtime::component::Resource;

fn mob(entity_type: &'static EntityType) -> Arc<dyn EntityBase> {
    from_type(
        entity_type,
        Vector3::new(0.5, 70.0, 0.5),
        &support::world(),
        uuid::Uuid::new_v4(),
    )
}

fn id(entity: &Arc<dyn EntityBase>) -> i32 {
    entity.get_entity().entity_id
}

fn source(direct: &Arc<dyn EntityBase>) -> DamageSource {
    DamageSource {
        damage_type: DamageType::MOB_ATTACK,
        direct_entity: Some(direct.clone()),
        causing_entity: None,
        source_position: Some(Vector3::new(1.0, 2.0, 3.0)),
    }
}

const fn speed() -> Effect {
    Effect {
        effect_type: &StatusEffect::SPEED,
        duration: 200,
        amplifier: 1,
        ambient: false,
        show_particles: true,
        show_icon: true,
        blend: false,
    }
}

fn item_id(
    state: &pumpkin_wasm_host_common::state::PluginHostState,
    res: &Resource<WitItemStack>,
) -> u16 {
    resource(state, res)
        .try_lock()
        .map(|item| item.item.id)
        .expect("unlocked stack")
}

#[test]
fn a_plugin_changes_the_damage_of_a_living_damage_pre_and_reads_the_post() {
    let zombie = mob(&EntityType::ZOMBIE);
    let skeleton = mob(&EntityType::SKELETON);
    let (zombie_id, skeleton_id) = (id(&zombie), id(&skeleton));
    let pre = plugin::<LivingDamageEventPre, _>(move |event, state| {
        let Event::LivingDamageEventPre(data) = event else {
            panic!("expected living damage pre");
        };
        assert_eq!(
            resource(state, &data.entity).get_entity().entity_id,
            zombie_id
        );
        assert_eq!(data.source.damage_type, WitDamageType::MobAttack);
        let direct = data.source.direct_entity.as_ref().expect("direct entity");
        assert_eq!(resource(state, direct).get_entity().entity_id, skeleton_id);
        assert!(data.source.causing_entity.is_none());
        assert_eq!(data.source.source_position, Some((1.0, 2.0, 3.0)));
        assert_eq!((data.original_damage, data.new_damage), (6.0, 5.0));
        data.original_damage = 100.0;
        data.new_damage = 2.0;
    });

    let event = fire(
        &pre,
        LivingDamageEventPre::new(zombie.clone(), source(&skeleton), 6.0, 5.0),
    );

    assert_eq!(event.new_damage, 2.0);
    assert_eq!(
        event.original_damage, 6.0,
        "the original damage is not settable"
    );

    let post = plugin::<LivingDamageEventPost, _>(|event, _state| {
        let Event::LivingDamageEventPost(data) = event else {
            panic!("expected living damage post");
        };
        assert_eq!(
            (
                data.inflicted_damage,
                data.health_damage,
                data.blocked_damage
            ),
            (5.0, 3.0, 1.0)
        );
        assert_eq!(data.post_attack_invulnerability_ticks, 20);
        assert!(data.should_cause_side_effects);
        assert_eq!(
            (data.reductions.armor, data.reductions.absorption),
            (0.5, 2.0)
        );
    });
    fire(
        &post,
        LivingDamageEventPost {
            entity: zombie,
            source: source(&skeleton),
            original_damage: 6.0,
            inflicted_damage: 5.0,
            health_damage: 3.0,
            blocked_damage: 1.0,
            shield_damage: 1.0,
            post_attack_invulnerability_ticks: 20,
            should_cause_side_effects: true,
            reductions: DamageReductions {
                invulnerability: 0.0,
                armor: 0.5,
                magic: 0.0,
                absorption: 2.0,
            },
        },
    );
}

#[test]
fn a_plugin_unblocks_a_shield_and_sets_its_durability_loss() {
    let zombie = mob(&EntityType::ZOMBIE);
    let skeleton = mob(&EntityType::SKELETON);
    let manager = plugin::<LivingShieldBlockEvent, _>(|event, _state| {
        let Event::LivingShieldBlockEvent(data) = event else {
            panic!("expected living shield block");
        };
        assert_eq!(
            (data.damage, data.blocked_damage, data.shield_damage),
            (4.0, 4.0, -1)
        );
        assert!(data.original_blocked && data.blocked);
        data.blocked_damage = 1.5;
        data.shield_damage = 3;
        data.blocked = false;
        data.original_blocked = false;
    });

    let event = fire(
        &manager,
        LivingShieldBlockEvent::new(zombie, source(&skeleton), 4.0, 4.0, true),
    );

    assert_eq!((event.blocked_damage, event.shield_damage), (1.5, 3));
    assert!(!event.blocked);
    assert!(event.original_blocked, "the original block is not settable");
}

#[test]
fn a_plugin_sets_the_durations_of_the_item_use_events_and_cancels_them() {
    let zombie = mob(&EntityType::ZOMBIE);
    let start = plugin::<LivingEntityUseItemEventStart, _>(|event, state| {
        let Event::LivingEntityUseItemEventStart(data) = event else {
            panic!("expected use item start");
        };
        assert_eq!(item_id(state, &data.item), Item::BREAD.id);
        assert_eq!(data.hand, WitHand::Left);
        assert_eq!(data.duration, 32);
        data.duration = 8;
    });
    let event = fire(
        &start,
        LivingEntityUseItemEventStart::new(
            zombie.clone(),
            ItemStack::new(1, &Item::BREAD),
            Hand::Left,
            32,
        ),
    );
    assert_eq!(event.duration, 8);
    assert!(!event.cancelled);

    let tick = plugin::<LivingEntityUseItemEventTick, _>(|event, _state| {
        let Event::LivingEntityUseItemEventTick(data) = event else {
            panic!("expected use item tick");
        };
        assert_eq!(data.duration, 7);
        data.duration = 0;
        data.cancelled = true;
    });
    let event = fire(
        &tick,
        LivingEntityUseItemEventTick::new(
            zombie.clone(),
            ItemStack::new(1, &Item::BREAD),
            Hand::Right,
            7,
        ),
    );
    assert!(event.cancelled);
    assert_eq!(event.duration, 0);

    let stop = plugin::<LivingEntityUseItemEventStop, _>(|event, _state| {
        let Event::LivingEntityUseItemEventStop(data) = event else {
            panic!("expected use item stop");
        };
        assert_eq!(data.duration, 5);
        data.duration = 1;
        data.cancelled = true;
    });
    let event = fire(
        &stop,
        LivingEntityUseItemEventStop::new(zombie, ItemStack::new(1, &Item::BOW), Hand::Right, 5),
    );
    assert!(event.cancelled);
    assert_eq!(event.duration, 5, "the duration of a stop is not settable");
}

#[test]
fn a_plugin_changes_a_knockback_a_fall_and_a_drown() {
    let zombie = mob(&EntityType::ZOMBIE);
    let player = support::player("WasmDiver");
    let knockback = plugin::<LivingKnockBackEvent, _>(|event, _state| {
        let Event::LivingKnockBackEvent(data) = event else {
            panic!("expected knockback");
        };
        assert_eq!(
            (data.strength, data.ratio_x, data.ratio_z),
            (0.4, 1.0, -1.0)
        );
        data.strength = 2.0;
        data.ratio_x = 0.5;
        data.original_strength = 9.0;
        data.cancelled = true;
    });
    let event = fire(
        &knockback,
        LivingKnockBackEvent::new(zombie.clone(), 0.4, 1.0, -1.0),
    );
    assert_eq!(
        (event.strength, event.ratio_x, event.ratio_z),
        (2.0, 0.5, -1.0)
    );
    assert_eq!(event.original_strength, 0.4);
    assert!(event.cancelled);

    let fall = plugin::<LivingFallEvent, _>(|event, _state| {
        let Event::LivingFallEvent(data) = event else {
            panic!("expected fall");
        };
        assert_eq!((data.distance, data.damage_multiplier), (10.0, 1.0));
        data.distance = 2.0;
        data.damage_multiplier = 0.5;
    });
    let event = fire(&fall, LivingFallEvent::new(zombie, 10.0, 1.0));
    assert_eq!((event.distance, event.damage_multiplier), (2.0, 0.5));

    let drown = plugin::<LivingDrownEvent, _>(|event, state| {
        let Event::LivingDrownEvent(data) = event else {
            panic!("expected drown");
        };
        assert!(resource(state, &data.entity).get_player().is_some());
        assert!(data.drowning);
        assert_eq!((data.damage_amount, data.bubble_count), (2.0, 8));
        data.drowning = false;
        data.damage_amount = 0.0;
        data.bubble_count = 1;
    });
    let event = fire(&drown, LivingDrownEvent::new(player, true));
    assert!(!event.drowning);
    assert_eq!((event.damage_amount, event.bubble_count), (0.0, 1));
}

#[test]
fn a_plugin_cancels_an_experience_drop_and_replaces_a_projectile() {
    let zombie = mob(&EntityType::ZOMBIE);
    let player = support::player("WasmHunter");
    let drop = plugin::<LivingExperienceDropEvent, _>(|event, state| {
        let Event::LivingExperienceDropEvent(data) = event else {
            panic!("expected experience drop");
        };
        let attacker = data.attacking_player.as_ref().expect("attacking player");
        assert_eq!(resource(state, attacker).gameprofile.name, "WasmHunter");
        assert_eq!((data.original_experience, data.dropped_experience), (5, 5));
        data.dropped_experience = 12;
        data.original_experience = 0;
        data.cancelled = true;
    });
    let event = fire(
        &drop,
        LivingExperienceDropEvent::new(zombie.clone(), Some(player), 5),
    );
    assert_eq!(
        (event.original_experience, event.dropped_experience),
        (5, 12)
    );
    assert!(event.cancelled);

    let projectile = plugin::<LivingGetProjectileEvent, _>(|event, state| {
        let Event::LivingGetProjectileEvent(data) = event else {
            panic!("expected get projectile");
        };
        assert_eq!(item_id(state, &data.projectile_weapon), Item::BOW.id);
        assert_eq!(item_id(state, &data.projectile), Item::ARROW.id);
        // The guest drops its handle and hands back a stack of its own.
        drop_resource(state, &data.projectile);
        data.projectile = state
            .add(Arc::new(tokio::sync::Mutex::new(ItemStack::new(
                1,
                &Item::SPECTRAL_ARROW,
            ))))
            .expect("new stack");
    });
    let event = fire(
        &projectile,
        LivingGetProjectileEvent::new(
            zombie,
            ItemStack::new(1, &Item::BOW),
            ItemStack::new(1, &Item::ARROW),
        ),
    );
    assert_eq!(event.projectile.item.id, Item::SPECTRAL_ARROW.id);
    assert_eq!(event.projectile_weapon.item.id, Item::BOW.id);
}

#[test]
fn a_plugin_receives_a_jump_and_changes_a_visibility() {
    let zombie = mob(&EntityType::ZOMBIE);
    let looking = mob(&EntityType::SKELETON);
    let (zombie_id, looking_id) = (id(&zombie), id(&looking));
    let jump = plugin::<LivingEventLivingJumpEvent, _>(move |event, state| {
        let Event::LivingEventLivingJumpEvent(data) = event else {
            panic!("expected jump");
        };
        assert_eq!(
            resource(state, &data.entity).get_entity().entity_id,
            zombie_id
        );
    });
    fire(&jump, LivingEventLivingJumpEvent::new(zombie.clone()));

    let visibility = plugin::<LivingEventLivingVisibilityEvent, _>(move |event, state| {
        let Event::LivingEventLivingVisibilityEvent(data) = event else {
            panic!("expected visibility");
        };
        let looking = data.looking_entity.as_ref().expect("looking entity");
        assert_eq!(resource(state, looking).get_entity().entity_id, looking_id);
        assert_eq!(data.visibility_modifier, 0.8);
        data.visibility_modifier = 0.25;
    });
    let event = fire(
        &visibility,
        LivingEventLivingVisibilityEvent::new(zombie, Some(looking), 0.8),
    );
    assert_eq!(event.visibility_modifier, 0.25);
}

#[test]
fn a_plugin_cancels_a_destroy_block_and_receives_a_conversion() {
    let wither = mob(&EntityType::WITHER);
    let hoglin = mob(&EntityType::HOGLIN);
    let zoglin = mob(&EntityType::ZOGLIN);
    let zoglin_id = id(&zoglin);
    let stone = Block::STONE.default_state;
    let destroy = plugin::<LivingDestroyBlockEvent, _>(move |event, _state| {
        let Event::LivingDestroyBlockEvent(data) = event else {
            panic!("expected destroy block");
        };
        assert_eq!(
            (data.block_pos.x, data.block_pos.y, data.block_pos.z),
            (1, 2, 3)
        );
        assert_eq!(data.state_id, stone.id.as_u16());
        data.cancelled = true;
    });
    let event = fire(
        &destroy,
        LivingDestroyBlockEvent::new(wither, BlockPos::new(1, 2, 3), stone),
    );
    assert!(event.cancelled);

    let conversion = plugin::<LivingConversionEventPost, _>(move |event, state| {
        let Event::LivingConversionEventPost(data) = event else {
            panic!("expected conversion post");
        };
        assert_eq!(
            resource(state, &data.outcome).get_entity().entity_id,
            zoglin_id
        );
    });
    fire(&conversion, LivingConversionEventPost::new(hoglin, zoglin));
}

#[test]
fn a_plugin_hides_an_effect_particle_and_refuses_an_effect() {
    let zombie = mob(&EntityType::ZOMBIE);
    let particle = plugin::<EffectParticleModificationEvent, _>(|event, _state| {
        let Event::EffectParticleModificationEvent(data) = event else {
            panic!("expected effect particle");
        };
        assert_eq!(data.effect.effect_id, StatusEffect::SPEED.minecraft_name);
        assert_eq!((data.effect.duration, data.effect.amplifier), (200, 1));
        assert_eq!(data.original_particle_color, 0x0012_3456);
        assert!(data.visible);
        data.particle_color = 7;
        data.original_particle_color = 9;
        data.visible = false;
    });
    let event = fire(
        &particle,
        EffectParticleModificationEvent::new(zombie.clone(), speed(), 0x0012_3456),
    );
    assert_eq!(
        (event.particle_color, event.original_particle_color),
        (7, 0x0012_3456)
    );
    assert!(!event.visible);

    let applicable = plugin::<MobEffectEventApplicable, _>(|event, _state| {
        let Event::MobEffectEventApplicable(data) = event else {
            panic!("expected applicable");
        };
        assert!(data.effect_source.is_none());
        assert_eq!(data.result, WitResult::Default);
        data.result = WitResult::DoNotApply;
    });
    let event = fire(
        &applicable,
        MobEffectEventApplicable::new(zombie, speed(), None),
    );
    assert_eq!(event.result, MobEffectEventApplicableResult::DoNotApply);
    assert!(!event.application_result(true));
}
