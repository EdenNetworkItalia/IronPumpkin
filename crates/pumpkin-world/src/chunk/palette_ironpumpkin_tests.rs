use super::tests::network_palette_values;
use super::{BlockPalette, ContentIds, NetworkPalette};
use pumpkin_data::{Block, BlockStateId};

/// Bits per entry, palette and packed data of a Java network encoding.
type Network = (u8, Option<Box<[u16]>>, Box<[i64]>);

/// The Java network encoding with raw state ids, as written before the block state egress.
fn raw_network(palette: &BlockPalette) -> Network {
    let bits = palette.convert_network(ContentIds::Display).bits_per_entry;
    match palette {
        BlockPalette::Homogeneous(value) => (0, Some(Box::new([value.as_u16()])), Box::new([])),
        BlockPalette::Heterogeneous(_) if bits <= 8 => {
            let (values, packed) = palette.to_palette_and_packed_data(bits);
            (
                bits,
                Some(values.iter().map(|v| v.as_u16()).collect()),
                packed,
            )
        }
        BlockPalette::Heterogeneous(_) => {
            let raw: Vec<u16> = (0..BlockPalette::VOLUME)
                .map(|i| palette.get(i % 16, i / 256, (i / 16) % 16).as_u16())
                .collect();
            let packed = raw
                .chunks(64 / bits as usize)
                .map(|chunk| {
                    chunk.iter().enumerate().fold(0u64, |acc, (i, value)| {
                        acc | (u64::from(*value) << (bits as usize * i))
                    }) as i64
                })
                .collect();
            (bits, None, packed)
        }
    }
}

#[test]
fn vanilla_sections_keep_their_network_bytes() {
    let states = [
        Block::AIR.default_state.id,
        Block::STONE.default_state.id,
        Block::WATER.default_state.id,
        Block::REDSTONE_LAMP.default_state.id,
    ];
    let palettes = [
        BlockPalette::from_fn(|_, _, _| Block::STONE.default_state.id),
        BlockPalette::from_fn(|x, y, z| states[(x + y + z) % states.len()]),
        BlockPalette::from_fn(|x, y, z| {
            BlockStateId::new(((y * 256 + z * 16 + x) * 7 % BlockStateId::COUNT as usize) as u16)
                .unwrap()
        }),
    ];
    for palette in &palettes {
        let network = palette.convert_network(ContentIds::Display);
        assert_eq!(
            (
                network.bits_per_entry,
                network_palette_values(network.palette),
                network.packed_data
            ),
            raw_network(palette)
        );
    }
    assert!(matches!(
        palettes[2].convert_network(ContentIds::Display).palette,
        NetworkPalette::Direct
    ));
}
