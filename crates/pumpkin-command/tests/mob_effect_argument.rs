//! The mob effect argument resolves the namespace of an effect, so a mod effect named like a
//! vanilla one does not give the vanilla one. The registries are process-wide and freeze once, so
//! this file is its own test binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::sync::Once;

use pumpkin_command::argument_types::argument_type::ArgumentType;
use pumpkin_command::argument_types::resource::MOB_EFFECT_ARGUMENT;
use pumpkin_command::source::DummySource;
use pumpkin_command::string_reader::StringReader;
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
    });
}

fn parse(input: &str) -> Option<&'static StatusEffect> {
    let effect =
        ArgumentType::<DummySource>::parse(&MOB_EFFECT_ARGUMENT, &mut StringReader::new(input))
            .ok()?;
    effect.downcast_ref::<StatusEffect>()
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
