//! The generated lookups resolve the states of custom blocks with properties. An integration test
//! has its own process, so it can install the process-wide content tables.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use pumpkin_data::data_component_impl::IDSetContent;
use pumpkin_data::dynamic::{self, BlockDefinition, BlockPropertyDefinition};
use pumpkin_data::{Block, BlockState, BlockStateId};

fn register(name: &str, properties: Vec<BlockPropertyDefinition>) {
    dynamic::register_block(BlockDefinition {
        name: name.to_string(),
        display: Block::STONE.default_state.id,
        properties,
        tags: Vec::new(),
    })
    .unwrap();
}

fn props(block: &Block, id: BlockStateId) -> Option<Vec<(&'static str, &'static str)>> {
    block.properties(id).map(|properties| properties.to_props())
}

#[test]
fn custom_state_ids_resolve_to_their_block_and_values() {
    register(
        "test:crop",
        vec![
            BlockPropertyDefinition::bool("powered", false),
            BlockPropertyDefinition::int("age", 0, 3, 1),
        ],
    );
    register("test:plain", Vec::new());
    register(
        "test:lamp",
        vec![BlockPropertyDefinition::bool("lit", false)],
    );
    dynamic::freeze().unwrap();

    // Name order: crop, lamp, plain.
    let base = BlockStateId::COUNT;
    let crop = Block::from_name("test:crop").unwrap();
    let lamp = Block::from_name("test:lamp").unwrap();
    let plain = Block::from_name("test:plain").unwrap();

    // age sorts before powered, so powered varies fastest and `true` comes first.
    for age in 0..4u16 {
        for (powered_index, powered) in ["true", "false"].into_iter().enumerate() {
            let raw = base + age * 2 + powered_index as u16;
            let id = BlockStateId::new(raw).unwrap();
            let age = age.to_string();
            let expected = vec![("age", age.as_str()), ("powered", powered)];

            assert_eq!(BlockState::from_id(id).id, id);
            assert_eq!(Block::from_state_id(id), crop);
            assert_eq!(BlockState::from_id_with_block(id).0, crop);
            assert_eq!(props(crop, id).unwrap(), expected);
            assert_eq!(crop.from_properties(&expected).to_state_id(crop), id);
            assert_eq!(crop.state_from_properties(&expected).unwrap().id, id);
        }
    }
    assert_eq!(crop.default_state.id.as_u16(), base + 3);
    assert_eq!(
        props(crop, crop.default_state.id).unwrap(),
        vec![("age", "1"), ("powered", "false")]
    );
    // Missing and unknown values keep the default.
    assert_eq!(
        crop.from_properties(&[("powered", "true"), ("age", "9"), ("x", "1")])
            .to_state_id(crop)
            .as_u16(),
        base + 2
    );

    let lamp_on = BlockStateId::new(base + 8).unwrap();
    let lamp_off = BlockStateId::new(base + 9).unwrap();
    assert_eq!(Block::from_state_id(lamp_on), lamp);
    assert_eq!(props(lamp, lamp_on).unwrap(), vec![("lit", "true")]);
    assert_eq!(props(lamp, lamp_off).unwrap(), vec![("lit", "false")]);
    assert_eq!(lamp.default_state.id, lamp_off);

    let plain_state = BlockStateId::new(base + 10).unwrap();
    assert_eq!(Block::from_state_id(plain_state), plain);
    assert_eq!(plain.default_state.id, plain_state);
    assert_eq!(plain.states.len(), 1);
    assert!(props(plain, plain_state).is_none());
    assert_eq!(BlockStateId::new(base + 11), None);

    // Generated blocks keep their own properties.
    let detector = &Block::DAYLIGHT_DETECTOR;
    assert_eq!(
        props(detector, detector.default_state.id).unwrap(),
        vec![("inverted", "false"), ("power", "0")]
    );
    assert!(props(&Block::STONE, Block::STONE.default_state.id).is_none());

    // A client knows a custom block as its display block, and a generated block as itself.
    for custom in [crop, lamp, plain] {
        assert_eq!(
            custom.to_java_network_id(pumpkin_data::dynamic::ContentIds::Display),
            Block::STONE.id.as_u16()
        );
        assert_eq!(
            IDSetContent::registry_id(custom, pumpkin_data::dynamic::ContentIds::Display),
            Block::STONE.id.as_u16()
        );
    }
    for generated in [&Block::STONE, &Block::DAYLIGHT_DETECTOR, &Block::AIR] {
        assert_eq!(
            generated.to_java_network_id(pumpkin_data::dynamic::ContentIds::Display),
            generated.id.as_u16()
        );
        assert_eq!(
            IDSetContent::registry_id(generated, pumpkin_data::dynamic::ContentIds::Display),
            generated.id.as_u16()
        );
    }
}
