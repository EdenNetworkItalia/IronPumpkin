use super::*;

/// Two custom block states with one display state are different values: change detection
/// does not go through the Java egress.
#[test]
fn custom_states_with_one_display_state_differ() {
    use pumpkin_data::BlockStateId;
    use pumpkin_data::dynamic::{BlockDefinition, BlockPropertyDefinition};
    use pumpkin_data::tracked_data::block_display::DATA_BLOCK_STATE_ID;
    use pumpkin_protocol::VarInt;

    // The registry is process-wide and freezes once: only this test of the binary freezes.
    pumpkin_data::dynamic::register_block(BlockDefinition {
        name: "test:synched_lamp".to_string(),
        display: pumpkin_data::Block::REDSTONE_LAMP.default_state.id,
        properties: vec![BlockPropertyDefinition::bool("lit", false)],
        tags: Vec::new(),
    })
    .unwrap();
    pumpkin_data::dynamic::freeze().unwrap();
    let lamp = pumpkin_data::Block::from_name("test:synched_lamp").unwrap();
    let [lit, unlit] = [lamp.states[0].id, lamp.states[1].id];
    assert_eq!(lit.display_state(), unlit.display_state());
    let raw = |id: BlockStateId| VarInt(i32::from(id.as_u16()));

    let data = SynchedEntityData::new();
    data.define(DATA_BLOCK_STATE_ID, VarInt(0));
    assert!(data.set(DATA_BLOCK_STATE_ID, raw(lit)));
    assert!(data.set(DATA_BLOCK_STATE_ID, raw(unlit)));
    assert!(!data.set(DATA_BLOCK_STATE_ID, raw(unlit)));
}
