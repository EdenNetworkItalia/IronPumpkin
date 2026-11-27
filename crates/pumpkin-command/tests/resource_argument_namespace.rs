//! The resource arguments resolve the namespace first, so a mod entry named like a vanilla one does
//! not give the vanilla one, and an unknown namespace is an error. The registries are process-wide
//! and freeze once, so this file is its own test binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::sync::Once;

use pumpkin_command::argument_types::argument_type::ArgumentType;
use pumpkin_command::argument_types::resource::{
    DAMAGE_TYPE_ARGUMENT, ENCHANTMENT_ARGUMENT, MOB_EFFECT_ARGUMENT, ResourceArgument,
    register_dynamic_damage_types,
};
use pumpkin_command::source::DummySource;
use pumpkin_command::string_reader::StringReader;
use pumpkin_data::Enchantment;
use pumpkin_data::damage::DamageType;
use pumpkin_data::dynamic::{self};
use pumpkin_data::dynamic_registries::{self, MobEffectDefinition};
use pumpkin_data::effect::{MobEffectCategory, StatusEffect};

fn frozen() {
    static FREEZE: Once = Once::new();
    FREEZE.call_once(|| {
        dynamic_registries::register_mob_effect(MobEffectDefinition {
            name: "othermod:speed".to_owned(),
            category: MobEffectCategory::Beneficial,
            color: 0x00_FF_00,
            attribute_modifiers: Vec::new(),
        })
        .unwrap();
        dynamic::freeze().unwrap();
        // The server registers data pack damage types by their full name.
        register_dynamic_damage_types(
            Box::new(|name| (name == "othermod:generic").then_some(&MOD_DAMAGE_TYPE)),
            vec!["othermod:generic".to_owned()],
        );
    });
}

static MOD_DAMAGE_TYPE: DamageType = DamageType {
    id: u8::MAX,
    ..DamageType::GENERIC
};

fn parse_with<T: 'static>(argument: &ResourceArgument, input: &str) -> Option<&'static T> {
    let value = ArgumentType::<DummySource>::parse(argument, &mut StringReader::new(input)).ok()?;
    value.downcast_ref::<T>()
}

fn parse(input: &str) -> Option<&'static StatusEffect> {
    parse_with(&MOB_EFFECT_ARGUMENT, input)
}

#[test]
fn a_mod_effect_named_like_a_vanilla_one_resolves_to_the_mod_effect() {
    frozen();
    let mod_effect = parse("othermod:speed").unwrap();
    assert_eq!(mod_effect.id, StatusEffect::COUNT);
    assert_ne!(mod_effect.id, StatusEffect::SPEED.id);
}

#[test]
fn a_vanilla_effect_resolves_with_and_without_its_namespace() {
    frozen();
    assert_eq!(parse("speed").unwrap().id, StatusEffect::SPEED.id);
    assert_eq!(parse("minecraft:speed").unwrap().id, StatusEffect::SPEED.id);
}

#[test]
fn an_unknown_namespace_is_an_error() {
    frozen();
    assert!(parse("nomod:speed").is_none());
}

#[test]
fn an_enchantment_resolves_its_namespace() {
    frozen();
    let sharpness = |input| parse_with::<Enchantment>(&ENCHANTMENT_ARGUMENT, input);
    assert_eq!(
        sharpness("sharpness").unwrap().id,
        Enchantment::SHARPNESS.id
    );
    assert_eq!(
        sharpness("minecraft:sharpness").unwrap().id,
        Enchantment::SHARPNESS.id
    );
    assert!(sharpness("othermod:sharpness").is_none());
}

#[test]
fn a_damage_type_resolves_its_namespace() {
    frozen();
    let damage = |input| parse_with::<DamageType>(&DAMAGE_TYPE_ARGUMENT, input);
    assert_eq!(damage("generic").unwrap().id, DamageType::GENERIC.id);
    assert_eq!(
        damage("minecraft:generic").unwrap().id,
        DamageType::GENERIC.id
    );
    assert_eq!(damage("othermod:generic").unwrap().id, MOD_DAMAGE_TYPE.id);
    assert!(damage("nomod:generic").is_none());
}
