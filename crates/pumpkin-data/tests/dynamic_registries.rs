//! The sound event, mob effect, attribute and data component type registries of mods. The
//! registries are process-wide and freeze once, so this file is its own test binary.
#![expect(
    clippy::expect_used,
    reason = "a test fails on the first missing value"
)]

use std::io::Cursor;
use std::sync::Once;

use pumpkin_data::attributes::Attributes;
use pumpkin_data::data_component::DataComponent;
use pumpkin_data::data_component_impl::{IDSetContent, Operation};
use pumpkin_data::dynamic::{self, ContentKind, RegistryError};
use pumpkin_data::dynamic_registries::{
    self, AttributeDefinition, AttributeRange, DataComponentTypeDefinition, MobEffectDefinition,
    MobEffectModifierDefinition, SoundEventDefinition,
};
use pumpkin_data::effect::{MobEffectCategory, StatusEffect};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::Sound;
use pumpkin_nbt::Nbt;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::deserializer::NbtReadHelperJava;
use pumpkin_nbt::tag::NbtTag;

fn mana() -> AttributeDefinition {
    AttributeDefinition {
        name: "test:mana".to_owned(),
        default_value: 20.0,
        range: AttributeRange::Ranged {
            min: 0.0,
            max: 100.0,
        },
        syncable: true,
    }
}

fn frozen() {
    static FREEZE: Once = Once::new();
    FREEZE.call_once(|| {
        for name in ["test:zap", "test:alpha"] {
            dynamic_registries::register_sound_event(SoundEventDefinition {
                name: name.to_owned(),
                fixed_range: None,
            })
            .expect("register sound event");
        }
        // The effect names the mod attribute before it registers: it resolves at the freeze.
        dynamic_registries::register_mob_effect(MobEffectDefinition {
            name: "test:shock".to_owned(),
            category: MobEffectCategory::Harmful,
            color: 0x00_FF_FF,
            attribute_modifiers: vec![MobEffectModifierDefinition {
                attribute: "test:mana".to_owned(),
                id: "test:effect.shock".to_owned(),
                amount: -2.0,
                operation: Operation::AddValue,
            }],
        })
        .expect("register mob effect");
        dynamic_registries::register_attribute(mana()).expect("register attribute");
        dynamic_registries::register_data_component_type(DataComponentTypeDefinition {
            name: "test:charge".to_owned(),
            persistent: true,
        })
        .expect("register data component type");
        dynamic_registries::register_data_component_type(DataComponentTypeDefinition {
            name: "test:cache".to_owned(),
            persistent: false,
        })
        .expect("register data component type");
        dynamic::freeze().expect("freeze registry");
    });
}

#[test]
fn entries_bind_after_the_generated_ids_in_name_order() {
    frozen();
    let sounds = Sound::slice().len() as u16;
    let alpha = dynamic_registries::sound_event_by_name("test:alpha").expect("sound binds");
    let zap = dynamic_registries::sound_event_by_name("test:zap").expect("sound binds");
    assert_eq!((alpha.id, zap.id), (sounds, sounds + 1));

    let shock = StatusEffect::from_minecraft_name("test:shock").expect("effect falls through");
    assert_eq!(shock.id, StatusEffect::COUNT);
    assert_eq!(shock.translation_key, "effect.test.shock");
    assert_eq!(
        <StatusEffect as IDSetContent>::from_id(u16::from(StatusEffect::COUNT)),
        Some(shock)
    );

    let mana = Attributes::from_name("test:mana").expect("attribute falls through");
    // After the generated attributes and the three of `NeoForgeMod`.
    assert_eq!(mana.id, Attributes::COUNT + 3);
    assert_eq!(Attributes::from_id(mana.id), Some(mana));
    assert_eq!(shock.attribute_modifiers[0].attribute, mana);

    let cache = dynamic_registries::data_component_type_by_name("test:cache").expect("type binds");
    assert_eq!(cache.id, DataComponent::COUNT);
    assert_eq!(
        dynamic_registries::data_component_type(DataComponent::COUNT + 1).map(|entry| entry.name),
        Some("test:charge")
    );
}

/// The `minecraft:attribute` snapshot of run (b) of the `NeoForge` 26.3.0.64-beta capture in
/// `tools/pumpkin-neoforge-client/captures` ends with `neoforge:swim_speed` at 40,
/// `neoforge:creative_flight` at 41 and `neoforge:gliding_flight` at 42.
#[test]
fn neoforge_attributes_take_the_ids_of_a_neoforge_server() {
    frozen();
    let tables = dynamic::tables().expect("content tables");
    let snapshot = tables.attribute_snapshot();
    let names: Vec<(i32, String)> = snapshot
        .iter()
        .map(|(id, name)| (*id, name.to_string()))
        .collect();
    assert_eq!(
        names[40..],
        [
            (40, "neoforge:swim_speed".to_owned()),
            (41, "neoforge:creative_flight".to_owned()),
            (42, "neoforge:gliding_flight".to_owned()),
            (43, "test:mana".to_owned()),
        ]
    );
    assert_eq!(
        Attributes::from_name("neoforge:creative_flight"),
        Some(&Attributes::NEOFORGE_CREATIVE_FLIGHT)
    );
    let effects = tables.mob_effect_snapshot();
    assert_eq!(effects.len(), usize::from(StatusEffect::COUNT) + 1);
    assert_eq!(effects[0].1.to_string(), "minecraft:speed");
    let components = tables.data_component_type_snapshot();
    assert_eq!(components[121].1.to_string(), "minecraft:cushion/color");
    assert_eq!(components.len(), usize::from(DataComponent::COUNT) + 2);
}

#[test]
fn ranged_and_boolean_attributes_sanitize_like_neoforge() {
    let ranged = AttributeRange::Ranged {
        min: 0.0,
        max: 1024.0,
    };
    assert_eq!(ranged.sanitize(2048.0), 1024.0);
    assert_eq!(ranged.sanitize(-1.0), 0.0);
    assert_eq!(ranged.sanitize(f64::NAN), 0.0);
    assert_eq!(AttributeRange::Boolean.sanitize(0.5), 1.0);
    assert_eq!(AttributeRange::Boolean.sanitize(-1.0), 0.0);
    // `BooleanAttribute`: an enabling modifier and a disabling multiply-total of -1.
    assert_eq!(
        AttributeRange::Boolean.sanitize((0.0 + 1.0) * (1.0 - 1.0)),
        0.0
    );
}

#[test]
fn invalid_and_late_registrations_fail() {
    let reserved = AttributeDefinition {
        name: "neoforge:swim_speed".to_owned(),
        ..mana()
    };
    let below = AttributeDefinition {
        name: "test:below".to_owned(),
        default_value: -1.0,
        ..mana()
    };
    let vanilla = SoundEventDefinition {
        name: "minecraft:entity.pig.ambient".to_owned(),
        fixed_range: None,
    };
    frozen();
    assert_eq!(
        dynamic_registries::register_attribute(below),
        Err(RegistryError::InvalidDefinition {
            kind: ContentKind::Attribute,
            name: "test:below".to_owned(),
            reason: "default value cannot be lower than minimum value",
        })
    );
    // After the freeze every other registration fails as frozen.
    assert_eq!(
        dynamic_registries::register_attribute(reserved),
        Err(RegistryError::RegistryFrozen)
    );
    assert_eq!(
        dynamic_registries::register_sound_event(vanilla),
        Err(RegistryError::RegistryFrozen)
    );
}

/// Writes the stack, encodes the compound to NBT bytes and decodes it again.
fn nbt_round_trip(stack: &ItemStack) -> NbtCompound {
    let mut compound = NbtCompound::new();
    stack.write_item_stack(&mut compound);
    let bytes = Nbt::new(String::new(), compound).write();
    Nbt::read(&mut NbtReadHelperJava::new(Cursor::new(bytes.as_ref())))
        .expect("written item stack re-reads")
        .root_tag
}

#[test]
fn persistent_mod_components_survive_a_save_and_load() {
    frozen();
    let charge = dynamic_registries::data_component_type_by_name("test:charge")
        .expect("type binds")
        .id;
    let cache = dynamic_registries::data_component_type_by_name("test:cache")
        .expect("type binds")
        .id;
    let mut stack = ItemStack::new(1, &Item::STICK);
    stack.set_custom_component(charge, NbtTag::Int(5));
    stack.set_custom_component(cache, NbtTag::Int(9));

    let mut compound = nbt_round_trip(&stack);
    let components = compound
        .get_compound("components")
        .expect("components compound");
    assert_eq!(components.get_int("test:charge"), Some(5));
    assert_eq!(components.get_int("test:cache"), None);

    // The value of a type whose mod is gone is dropped; the stack still loads.
    let mut components = components.clone();
    components.put_int("gone:thing", 1);
    compound.put_compound("components", components);
    let loaded = ItemStack::read_item_stack(&compound).expect("stack loads");
    assert_eq!(loaded.get_custom_component(charge), Some(&NbtTag::Int(5)));
    assert_eq!(loaded.custom_components.len(), 1);

    let mut other = loaded.clone();
    other.set_custom_component(charge, NbtTag::Int(6));
    assert!(!loaded.are_equal(&other));
}
