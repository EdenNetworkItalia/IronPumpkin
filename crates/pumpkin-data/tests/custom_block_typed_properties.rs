//! The typed property paths keep the properties of a custom block that have no [`Property`]
//! variant, or whose value `value_str` cannot name. The content registry is process-wide and
//! freezes once, so this file is its own test binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::sync::LazyLock;

use pumpkin_data::block_properties::Facing;
use pumpkin_data::dynamic::{self, BlockDefinition, BlockPropertyDefinition};
use pumpkin_data::{Block, BlockStateId, Mirror, Property, Rotation};

/// A block with an int property above the table of `value_str`, a property without a
/// [`Property`] variant, and two typed properties.
static SPROUT: LazyLock<&'static Block> = LazyLock::new(|| {
    dynamic::register_block(BlockDefinition {
        name: "test:sprout".to_string(),
        display: Block::STONE.default_state.id,
        properties: vec![
            BlockPropertyDefinition::int("age", 0, 30, 0),
            BlockPropertyDefinition::enumeration("color", &["red", "green", "blue"], "red"),
            BlockPropertyDefinition::enumeration(
                "facing",
                &["north", "east", "south", "west"],
                "north",
            ),
            BlockPropertyDefinition::bool("waterlogged", false),
        ],
        tags: Vec::new(),
    })
    .unwrap();
    dynamic::freeze().unwrap();
    Block::from_name("test:sprout").unwrap()
});

fn state(props: &[(&str, &str)]) -> BlockStateId {
    SPROUT.from_string_properties(props).to_state_id(&SPROUT)
}

fn value(id: BlockStateId, name: &str) -> &'static str {
    SPROUT
        .string_properties(id)
        .unwrap()
        .into_iter()
        .find(|(property, _)| *property == name)
        .unwrap()
        .1
}

/// An old blue sprout facing north: the properties the typed paths lost.
fn old_blue() -> BlockStateId {
    state(&[("age", "30"), ("color", "blue"), ("facing", "north")])
}

#[test]
fn to_props_drops_an_int_that_value_str_cannot_name() {
    let id = old_blue();
    let props = SPROUT.properties(id).unwrap().to_props();
    assert!(!props.iter().any(|p| p.key_str() == "age"), "{props:?}");
    assert!(props.contains(&Property::Facing(Facing::North)));
    assert!(props.contains(&Property::Waterlogged(false)));

    let young = state(&[("age", "25")]);
    assert!(
        SPROUT
            .properties(young)
            .unwrap()
            .to_props()
            .contains(&Property::Age(25))
    );
}

#[test]
fn every_state_survives_a_typed_round_trip() {
    for block_state in SPROUT.states {
        let props = SPROUT.properties(block_state.id).unwrap().to_props();
        assert_eq!(
            SPROUT.with_properties(block_state.id, &props),
            block_state.id,
            "{:?}",
            SPROUT.string_properties(block_state.id)
        );
    }
}

#[test]
fn with_properties_sets_named_values_and_keeps_the_rest() {
    let id = old_blue();
    let young = SPROUT.with_properties(id, &[Property::Age(5)]);
    assert_eq!(value(young, "age"), "5");
    assert_eq!(value(young, "color"), "blue");

    // `Age(30).value_str()` is "0", so the typed value cannot set 30: the age stays.
    assert_eq!(SPROUT.with_properties(young, &[Property::Age(30)]), young);
    assert_eq!(
        SPROUT.with_properties(id, &[Property::Facing(Facing::Up)]),
        id
    );
}

#[test]
fn set_waterlogged_keeps_the_custom_properties() {
    let id = old_blue();
    let wet = SPROUT.set_waterlogged(id, true).unwrap();
    assert_eq!(value(wet, "waterlogged"), "true");
    assert_eq!(value(wet, "age"), "30");
    assert_eq!(value(wet, "color"), "blue");
    assert_eq!(value(wet, "facing"), "north");
    assert_eq!(SPROUT.set_waterlogged(wet, false), Some(id));
}

#[test]
fn rotate_and_mirror_keep_the_custom_properties() {
    let id = old_blue();
    let rotated = SPROUT.rotate(id, Rotation::Clockwise90).id;
    assert_eq!(value(rotated, "facing"), "east");
    assert_eq!(value(rotated, "age"), "30");
    assert_eq!(value(rotated, "color"), "blue");

    let mirrored = SPROUT.mirror(id, Mirror::LeftRight).id;
    assert_eq!(value(mirrored, "facing"), "south");
    assert_eq!(value(mirrored, "age"), "30");
    assert_eq!(value(mirrored, "color"), "blue");
}
