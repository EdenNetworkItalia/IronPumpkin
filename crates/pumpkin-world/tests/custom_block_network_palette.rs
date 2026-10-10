//! The Java chunk palette sends a custom block state as its display state to a `Display` client and
//! as itself to a `Real` client. The content registry is process-wide and freezes once, so this
//! file is its own test binary.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

use std::sync::Once;

use pumpkin_data::dynamic::{self, BlockDefinition, BlockPropertyDefinition, ContentIds};
use pumpkin_data::{Block, BlockStateId};
use pumpkin_world::chunk::palette::{BlockPalette, NetworkPalette};

/// The two states of the custom lamp, which shows as an unlit redstone lamp.
fn lamp_states() -> [BlockStateId; 2] {
    static FREEZE: Once = Once::new();
    FREEZE.call_once(|| {
        dynamic::register_block(BlockDefinition {
            name: "test:lamp".to_string(),
            display: Block::REDSTONE_LAMP.default_state.id,
            properties: vec![BlockPropertyDefinition::bool("lit", false)],
            tags: Vec::new(),
        })
        .expect("register test block");
        dynamic::register_placeholder_block(BlockDefinition {
            name: "test:gone".to_string(),
            display: Block::REDSTONE_LAMP.default_state.id,
            properties: Vec::new(),
            tags: Vec::new(),
        })
        .expect("register placeholder block");
        dynamic::freeze().expect("freeze registry");
    });
    let lamp = Block::from_name("test:lamp").expect("custom block resolves after the freeze");
    [lamp.states[0].id, lamp.states[1].id]
}

fn display() -> u16 {
    Block::REDSTONE_LAMP.default_state.id.as_u16()
}

#[test]
fn a_homogeneous_section_of_custom_blocks_sends_the_display_state() {
    let [lit, _] = lamp_states();
    let filled = |state: BlockStateId| {
        let mut palette = BlockPalette::default();
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    palette.set(x, y, z, state);
                }
            }
        }
        palette
    };
    let network = filled(lit).convert_network(ContentIds::Display);
    assert_eq!(network.bits_per_entry, 0);
    assert!(matches!(network.palette, NetworkPalette::Single(id) if id == display()));

    let network = filled(lit).convert_network(ContentIds::Real);
    let real = lit.to_java_network_id(ContentIds::Real);
    assert!(matches!(network.palette, NetworkPalette::Single(id) if id == real));

    // A placeholder keeps its display state in both modes.
    let gone = Block::from_name("test:gone")
        .expect("placeholder resolves after the freeze")
        .default_state
        .id;
    let network = filled(gone).convert_network(ContentIds::Real);
    assert!(matches!(network.palette, NetworkPalette::Single(id) if id == display()));
}

#[test]
fn an_indirect_palette_lists_the_display_state() {
    let [lit, unlit] = lamp_states();
    let mut palette = BlockPalette::default();
    palette.set(1, 2, 3, lit);
    palette.set(4, 5, 6, unlit);
    palette.set(7, 8, 9, Block::STONE.default_state.id);

    let network = palette.convert_network(ContentIds::Display);
    let NetworkPalette::Indirect(values) = network.palette else {
        panic!("a section with four states uses an indirect palette");
    };
    assert!(values.iter().all(|&id| id < BlockStateId::COUNT));
    assert_eq!(values.iter().filter(|&&id| id == display()).count(), 2);
    assert!(values.contains(&Block::STONE.default_state.id.as_u16()));
    assert!(values.contains(&BlockStateId::AIR.as_u16()));

    let NetworkPalette::Indirect(values) = palette.convert_network(ContentIds::Real).palette else {
        panic!("a section with four states uses an indirect palette");
    };
    assert!(values.contains(&lit.to_java_network_id(ContentIds::Real)));
    assert!(values.contains(&unlit.to_java_network_id(ContentIds::Real)));
    assert!(!values.contains(&display()));
}

#[test]
fn a_direct_palette_packs_the_display_state() {
    let [lit, _] = lamp_states();
    let mut palette = BlockPalette::default();
    for (index, id) in (1..300u16).enumerate() {
        let state = BlockStateId::new(id).expect("a generated state");
        palette.set(index % 16, index / 256, (index / 16) % 16, state);
    }
    // The last block of the section, in network order.
    palette.set(15, 15, 15, lit);

    let last_value = |ids| {
        let network = palette.convert_network(ids);
        assert!(matches!(network.palette, NetworkPalette::Direct));
        let bits = usize::from(network.bits_per_entry);
        let per_long = 64 / bits;
        let last = BlockPalette::VOLUME - 1;
        let long = network.packed_data[last / per_long] as u64;
        (long >> (bits * (last % per_long))) & ((1 << bits) - 1)
    };
    assert_eq!(last_value(ContentIds::Display), u64::from(display()));
    assert_eq!(
        last_value(ContentIds::Real),
        u64::from(lit.to_java_network_id(ContentIds::Real))
    );
}
