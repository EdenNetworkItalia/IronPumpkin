use super::{Block, BlockId, ShapeOffsetType};
use crate::block_properties::{EnumVariants, FacingHopper};
use pumpkin_util::math::position::BlockPos;

#[test]
fn from_properties_on_a_block_without_properties_returns_its_only_state() {
    let mut checked = 0;
    for raw_id in 0..BlockId::BLOCK_COUNT {
        let block = Block::from_id(BlockId::new(raw_id).unwrap());
        if block.properties(block.default_state.id).is_some() {
            continue;
        }
        assert_eq!(block.states.len(), 1, "{}", block.name);
        let properties = block.from_string_properties(&[("facing", "north"), ("lit", "true")]);
        assert_eq!(
            properties.to_state_id(block),
            block.default_state.id,
            "{}",
            block.name
        );
        checked += 1;
    }
    assert!(checked > 0);
}

fn props_after(block: &Block, props: &[(&str, &str)]) -> Vec<(&'static str, &'static str)> {
    block
        .from_string_properties(props)
        .to_props()
        .into_iter()
        .map(|property| (property.key_str(), property.value_str()))
        .collect()
}

#[test]
fn from_properties_keeps_the_default_for_an_unknown_value() {
    // The default level is 15.
    let light = &Block::LIGHT;
    let light_default = vec![("level", "15"), ("waterlogged", "false")];
    assert_eq!(props_after(light, &[]), light_default);
    for level in ["99", "16", "-1", "", "high"] {
        assert_eq!(props_after(light, &[("level", level)]), light_default);
    }
    assert_eq!(
        props_after(light, &[("level", "3")]),
        [("level", "3"), ("waterlogged", "false")]
    );
    for level in [16, 99, u8::MAX] {
        let props = light.from_properties(&[crate::Property::Level(level)]);
        assert_eq!(props.to_state_id(light), light.default_state.id);
    }

    // The default of `enabled` is true.
    let hopper = &Block::HOPPER;
    let hopper_default = vec![("enabled", "true"), ("facing", "down")];
    assert_eq!(props_after(hopper, &[]), hopper_default);
    for enabled in ["maybe", "", "True"] {
        assert_eq!(props_after(hopper, &[("enabled", enabled)]), hopper_default);
    }
    assert_eq!(
        props_after(hopper, &[("enabled", "false")]),
        [("enabled", "false"), ("facing", "down")]
    );

    // An unknown enum value does not panic, and the other properties still apply.
    for facing in ["sideways", "", "NORTH"] {
        assert_eq!(props_after(hopper, &[("facing", facing)]), hopper_default);
    }
    assert_eq!(
        props_after(hopper, &[("facing", "sideways"), ("enabled", "false")]),
        [("enabled", "false"), ("facing", "down")]
    );
    assert_eq!(
        props_after(hopper, &[("facing", "north"), ("enabled", "maybe")]),
        [("enabled", "true"), ("facing", "north")]
    );
}

#[test]
fn enum_from_value_returns_none_for_an_unknown_value() {
    assert_eq!(FacingHopper::from_value("north"), Some(FacingHopper::North));
    assert_eq!(FacingHopper::from_value("sideways"), None);
    assert_eq!(FacingHopper::from_value(""), None);
}
