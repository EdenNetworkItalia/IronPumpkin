//! Block states as NBT compounds, like vanilla's `BlockState.CODEC` in saved data.
//!
//! The chunk palette writer and the entities that save a block state use these functions, so this
//! file is the only place that writes the keys.

use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

use pumpkin_data::{Block, BlockState, BlockStateId};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use tracing::warn;

// Pumpkin saves world data at DataVersion 4903 (26.2), which uses these keys.
const NAME_TAG: &str = "Name";
const PROPERTIES_TAG: &str = "Properties";
// 26.3 renamed them (data version 5006).
const NAME_TAG_26_3: &str = "id";
const PROPERTIES_TAG_26_3: &str = "properties";

const _: () = assert!(
    crate::chunk::format::anvil::WORLD_DATA_VERSION < 5006,
    "WORLD_DATA_VERSION reached 26.3: write `id` and `properties` here, and keep reading `Name` and `Properties`"
);

/// Writes the compound that vanilla 26.2 writes: `Name`, plus `Properties` with every property of
/// the state when the block has properties.
#[must_use]
pub fn block_state_to_nbt(id: BlockStateId) -> NbtCompound {
    let block = Block::from_state_id(id);
    let mut compound = NbtCompound::new();
    compound.put_string(
        NAME_TAG,
        pumpkin_data::dynamic::namespaced_name(block.name).into_owned(),
    );
    if let Some(properties) = block.properties(id) {
        let properties = properties.to_props();
        if !properties.is_empty() {
            let mut properties_compound = NbtCompound::new();
            for (name, value) in properties {
                properties_compound.put_string(name, value.to_string());
            }
            compound.put_compound(PROPERTIES_TAG, properties_compound);
        }
    }
    compound
}

/// The block name and the properties compound of a block state compound, with the keys of 26.3 or
/// of 26.2. `PaletteEntry::from_nbt_compound` reads templates through this too.
pub(crate) fn state_compound_parts(compound: &NbtCompound) -> (Option<&str>, Option<&NbtCompound>) {
    let name = compound
        .get_string(NAME_TAG_26_3)
        .or_else(|| compound.get_string(NAME_TAG));
    let properties = compound
        .get_compound(PROPERTIES_TAG_26_3)
        .or_else(|| compound.get_compound(PROPERTIES_TAG));
    (name, properties)
}

/// Reads a block state from every form this server meets:
/// - the 26.2 compound, with `Name` and `Properties`
/// - the 26.3 compound, with `id` and `properties`
/// - the 26.3 bare name, which stands for the default state of the block
/// - the raw state id, which Pumpkin wrote before it used compounds
///
/// Returns `None` when the block is unknown or the tag has another shape. An unknown block name
/// logs a warning, as vanilla reports it to its `ProblemReporter`.
#[must_use]
pub fn block_state_from_nbt(tag: &NbtTag) -> Option<BlockStateId> {
    match tag {
        NbtTag::String(name) => Some(known_block(name)?.default_state.id),
        NbtTag::Compound(compound) => {
            let (name, properties) = state_compound_parts(compound);
            let block = known_block(name?)?;
            let state = properties.map_or(block.default_state, |properties| {
                state_with_properties(block, properties)
            });
            Some(state.id)
        }
        NbtTag::Int(id) => u16::try_from(*id).ok().and_then(BlockStateId::new),
        _ => None,
    }
}

/// Block names the chunk palette reader has warned about. A world copied from another server
/// references the same missing block in many sections and chunks; one warning per name is enough.
static UNKNOWN_PALETTE_BLOCKS: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
static UNKNOWN_PALETTE_PROPERTIES: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

#[cold]
pub(crate) fn first_report(reported: &Mutex<BTreeSet<String>>, name: &str) -> bool {
    let mut reported = reported.lock().unwrap_or_else(PoisonError::into_inner);
    !reported.contains(name) && reported.insert(name.to_owned())
}

/// Reads one entry of a chunk section's block palette.
///
/// A block name the registry does not know (in neither the generated data, the registered
/// content nor the content manifest) loads as air. A property the block does not have is ignored,
/// and a value the block does not allow keeps the default of that property. Each case warns once
/// per block name for the whole process.
pub(crate) fn palette_entry_to_state(compound: &NbtCompound) -> BlockStateId {
    let (Some(name), properties) = state_compound_parts(compound) else {
        return BlockStateId::AIR;
    };
    let Some(block) = Block::from_name(name) else {
        if first_report(&UNKNOWN_PALETTE_BLOCKS, name) {
            warn!(
                "Block {name} in a chunk palette is not registered: it loads as air. Other references to it are not logged"
            );
        }
        return BlockStateId::AIR;
    };
    let properties: Vec<(&str, &str)> = properties
        .map(|properties| {
            properties
                .child_tags
                .iter()
                .filter_map(|(name, value)| Some((name.as_ref(), value.extract_string()?)))
                .collect()
        })
        .unwrap_or_default();
    if properties.is_empty() {
        return block.default_state.id;
    }
    let id = block.from_properties(&properties).to_state_id(block);
    let state_properties = block
        .properties(id)
        .map(|state| state.to_props())
        .unwrap_or_default();
    let mut unknown = properties
        .iter()
        .filter(|(name, value)| !state_properties.contains(&(*name, *value)))
        .peekable();
    if unknown.peek().is_some() && first_report(&UNKNOWN_PALETTE_PROPERTIES, name) {
        let unknown: Vec<String> = unknown
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        warn!(
            "Block {name} in a chunk palette has properties its schema does not know ({}): they load as their default values. Other references to it are not logged",
            unknown.join(", ")
        );
    }
    id
}

/// Reads a bare-name entry of a chunk section's block palette. 26.3 writes the default state of a
/// block as its name alone (`BlockState.CODEC` is `Codec.either(block, full state)`), so a section
/// full of stone has the entry `minecraft:stone`.
pub(crate) fn palette_name_to_state(name: &str) -> BlockStateId {
    let mut compound = NbtCompound::new();
    compound.put_string(NAME_TAG_26_3, name.to_string());
    palette_entry_to_state(&compound)
}

fn known_block(name: &str) -> Option<&'static Block> {
    let block = Block::from_name(name);
    if block.is_none() {
        warn!("Unknown block {name} in a saved block state");
    }
    block
}

/// Vanilla decodes each property on its own: a missing property, an unknown one or an invalid
/// value leaves the default value of the block.
fn state_with_properties(block: &'static Block, properties: &NbtCompound) -> &'static BlockState {
    let Some(defaults) = block.properties(block.default_state.id) else {
        return block.default_state;
    };
    let mut merged: Vec<(&str, &str)> = defaults.to_props();
    for (name, value) in &properties.child_tags {
        let Some(value) = value.extract_string() else {
            continue;
        };
        let Some(entry) = merged.iter_mut().find(|(known, _)| *known == name.as_ref()) else {
            continue;
        };
        let previous = std::mem::replace(&mut entry.1, value);
        if block.state_from_properties(&merged).is_none()
            && let Some(entry) = merged.iter_mut().find(|(known, _)| *known == name.as_ref())
        {
            entry.1 = previous;
        }
    }
    block
        .state_from_properties(&merged)
        .unwrap_or(block.default_state)
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! NBT written by the vanilla server jars with the call the entities make in
    //! `addAdditionalSaveData`: `TagValueOutput.store(key, BlockState.CODEC, state)`, then
    //! `NbtIo.write`. The jars are 26.2 and 26.3 from piston-meta.mojang.com.

    use pumpkin_nbt::{
        Nbt,
        compound::NbtCompound,
        deserializer::{NbtReadHelperJava, NbtStreamReader},
    };

    // 26.2, `carriedBlockState`: stone.
    pub const ENDER_STONE_26_2: &str = "0a00000a001163617272696564426c6f636b53746174650800044e616d65000f6d696e6563726166743a73746f6e650000";
    // 26.2, `carriedBlockState`: grass_block, snowy=false (default state, properties still written).
    pub const ENDER_GRASS_26_2: &str = "0a00000a001163617272696564426c6f636b53746174650a000a50726f70657274696573080005736e6f7779000566616c7365000800044e616d6500156d696e6563726166743a67726173735f626c6f636b0000";
    // 26.2, `carriedBlockState`: grass_block, snowy=true.
    pub const ENDER_GRASS_SNOWY_26_2: &str = "0a00000a001163617272696564426c6f636b53746174650a000a50726f70657274696573080005736e6f7779000474727565000800044e616d6500156d696e6563726166743a67726173735f626c6f636b0000";
    // 26.2, `block_state`: oak_stairs, facing=south, half=top, shape=outer_right, waterlogged=true.
    pub const DISPLAY_STAIRS_26_2: &str = "0a00000a000b626c6f636b5f73746174650a000a50726f7065727469657308000b77617465726c6f6767656400047472756508000468616c660003746f700800057368617065000b6f757465725f7269676874080006666163696e670005736f757468000800044e616d6500146d696e6563726166743a6f616b5f7374616972730000";
    // 26.2, `block_state`: repeater, delay=3, facing=east, locked=false, powered=true.
    pub const DISPLAY_REPEATER_26_2: &str = "0a00000a000b626c6f636b5f73746174650a000a50726f7065727469657308000564656c6179000133080007706f7765726564000474727565080006666163696e670004656173740800066c6f636b6564000566616c7365000800044e616d6500126d696e6563726166743a72657065617465720000";
    // 26.2, `block_state`: air, the default of a block display.
    pub const DISPLAY_AIR_26_2: &str =
        "0a00000a000b626c6f636b5f73746174650800044e616d65000d6d696e6563726166743a6169720000";
    // 26.3, `carriedBlockState`: grass_block, snowy=false (default state, bare name).
    pub const ENDER_GRASS_26_3: &str = "0a000008001163617272696564426c6f636b537461746500156d696e6563726166743a67726173735f626c6f636b00";
    // 26.3, `carriedBlockState`: grass_block, snowy=true.
    pub const ENDER_GRASS_SNOWY_26_3: &str = "0a00000a001163617272696564426c6f636b5374617465080002696400156d696e6563726166743a67726173735f626c6f636b0a000a70726f70657274696573080005736e6f7779000474727565000000";
    // 26.3, `block_state`: oak_stairs, facing=south, half=top, shape=outer_right, waterlogged=true.
    pub const DISPLAY_STAIRS_26_3: &str = "0a00000a000b626c6f636b5f7374617465080002696400146d696e6563726166743a6f616b5f7374616972730a000a70726f7065727469657308000b77617465726c6f6767656400047472756508000468616c660003746f700800057368617065000b6f757465725f7269676874080006666163696e670005736f757468000000";
    // 26.3, `block_state`: air (bare name).
    pub const DISPLAY_AIR_26_3: &str =
        "0a000008000b626c6f636b5f7374617465000d6d696e6563726166743a61697200";

    /// Decodes the named root compound of one of the constants above.
    pub fn read(hex_bytes: &str) -> NbtCompound {
        let bytes: Vec<u8> = (0..hex_bytes.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex_bytes[index..index + 2], 16).unwrap())
            .collect();
        let mut cursor = std::io::Cursor::new(&bytes[..]);
        let mut reader = NbtReadHelperJava::new(NbtStreamReader(&mut cursor));
        Nbt::read(&mut reader).unwrap().root_tag
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{self, read};
    use super::{block_state_from_nbt, block_state_to_nbt};
    use pumpkin_data::{Block, BlockStateId};
    use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};

    fn properties_of(id: BlockStateId) -> Vec<(String, String)> {
        let mut properties: Vec<(String, String)> = Block::from_state_id(id)
            .properties(id)
            .map(|properties| properties.to_props())
            .unwrap_or_default()
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        properties.sort();
        properties
    }

    fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = expected
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect();
        pairs.sort();
        pairs
    }

    fn decode(hex_bytes: &str, key: &str) -> BlockStateId {
        let root = read(hex_bytes);
        block_state_from_nbt(root.get(key).unwrap()).unwrap()
    }

    #[test]
    fn decodes_vanilla_26_2_compounds() {
        let stone = decode(fixtures::ENDER_STONE_26_2, "carriedBlockState");
        assert_eq!(stone, Block::STONE.default_state.id);

        let grass = decode(fixtures::ENDER_GRASS_26_2, "carriedBlockState");
        assert_eq!(grass, Block::GRASS_BLOCK.default_state.id);

        let snowy = decode(fixtures::ENDER_GRASS_SNOWY_26_2, "carriedBlockState");
        assert_eq!(Block::from_state_id(snowy), &Block::GRASS_BLOCK);
        assert_eq!(properties_of(snowy), pairs(&[("snowy", "true")]));

        let stairs = decode(fixtures::DISPLAY_STAIRS_26_2, "block_state");
        assert_eq!(Block::from_state_id(stairs), &Block::OAK_STAIRS);
        assert_eq!(
            properties_of(stairs),
            pairs(&[
                ("facing", "south"),
                ("half", "top"),
                ("shape", "outer_right"),
                ("waterlogged", "true"),
            ])
        );

        let repeater = decode(fixtures::DISPLAY_REPEATER_26_2, "block_state");
        assert_eq!(Block::from_state_id(repeater), &Block::REPEATER);
        assert_eq!(
            properties_of(repeater),
            pairs(&[
                ("delay", "3"),
                ("facing", "east"),
                ("locked", "false"),
                ("powered", "true"),
            ])
        );

        let air = decode(fixtures::DISPLAY_AIR_26_2, "block_state");
        assert_eq!(air, BlockStateId::AIR);
    }

    #[test]
    fn decodes_vanilla_26_3_compounds_and_bare_names() {
        let grass = decode(fixtures::ENDER_GRASS_26_3, "carriedBlockState");
        assert_eq!(grass, Block::GRASS_BLOCK.default_state.id);

        let snowy = decode(fixtures::ENDER_GRASS_SNOWY_26_3, "carriedBlockState");
        assert_eq!(
            snowy,
            decode(fixtures::ENDER_GRASS_SNOWY_26_2, "carriedBlockState")
        );

        let stairs = decode(fixtures::DISPLAY_STAIRS_26_3, "block_state");
        assert_eq!(stairs, decode(fixtures::DISPLAY_STAIRS_26_2, "block_state"));

        let air = decode(fixtures::DISPLAY_AIR_26_3, "block_state");
        assert_eq!(air, BlockStateId::AIR);
    }

    #[test]
    fn writes_what_vanilla_26_2_writes() {
        for (hex_bytes, key) in [
            (fixtures::ENDER_STONE_26_2, "carriedBlockState"),
            (fixtures::ENDER_GRASS_26_2, "carriedBlockState"),
            (fixtures::ENDER_GRASS_SNOWY_26_2, "carriedBlockState"),
            (fixtures::DISPLAY_STAIRS_26_2, "block_state"),
            (fixtures::DISPLAY_REPEATER_26_2, "block_state"),
            (fixtures::DISPLAY_AIR_26_2, "block_state"),
        ] {
            let root = read(hex_bytes);
            let vanilla = root.get_compound(key).unwrap();
            let id = block_state_from_nbt(root.get(key).unwrap()).unwrap();
            assert_eq!(&block_state_to_nbt(id), vanilla, "{key} in {hex_bytes}");
        }
    }

    #[test]
    fn round_trips_every_state_of_a_few_blocks() {
        for block in [
            &Block::STONE,
            &Block::OAK_STAIRS,
            &Block::REPEATER,
            &Block::REDSTONE_WIRE,
        ] {
            for state in block.states {
                let tag = NbtTag::Compound(block_state_to_nbt(state.id));
                assert_eq!(block_state_from_nbt(&tag), Some(state.id), "{}", block.name);
            }
        }
    }

    #[test]
    fn reads_the_old_pumpkin_int_form() {
        let id = Block::REPEATER.default_state.id;
        assert_eq!(
            block_state_from_nbt(&NbtTag::Int(i32::from(id.as_u16()))),
            Some(id)
        );
        assert_eq!(block_state_from_nbt(&NbtTag::Int(-1)), None);
        assert_eq!(block_state_from_nbt(&NbtTag::Int(i32::MAX)), None);
    }

    #[test]
    fn keeps_the_default_for_missing_unknown_and_invalid_properties() {
        let mut properties = NbtCompound::new();
        properties.put_string("facing", "west".to_string());
        properties.put_string("delay", "99".to_string());
        properties.put_string("no_such_property", "x".to_string());
        properties.put_int("powered", 1);
        let mut compound = NbtCompound::new();
        compound.put_string("Name", "minecraft:repeater".to_string());
        compound.put_compound("Properties", properties);

        let id = block_state_from_nbt(&NbtTag::Compound(compound)).unwrap();
        let defaults = properties_of(Block::REPEATER.default_state.id);
        let mut expected = defaults;
        for (name, value) in &mut expected {
            if name == "facing" {
                *value = "west".to_string();
            }
        }
        assert_eq!(properties_of(id), expected);
    }

    #[test]
    fn palette_entries_decode_like_the_template_resolver_for_every_vanilla_state() {
        use super::palette_entry_to_state;
        use crate::generation::structure::template::{BlockStateResolver, PaletteEntry};

        for raw in 0..BlockStateId::COUNT {
            let id = BlockStateId::new(raw).unwrap();
            let compound = block_state_to_nbt(id);
            let resolver = BlockStateResolver::resolve_simple(
                &PaletteEntry::from_nbt_compound(&compound).unwrap(),
            )
            .unwrap()
            .id;
            assert_eq!(palette_entry_to_state(&compound), id);
            assert_eq!(resolver, id);
        }
    }

    #[test]
    fn unknown_palette_blocks_load_as_air_and_warn_once() {
        use super::{UNKNOWN_PALETTE_BLOCKS, first_report, palette_entry_to_state};

        let name = "test:missing";
        let mut compound = NbtCompound::new();
        compound.put_string("Name", name.to_string());
        for _ in 0..3 {
            assert_eq!(palette_entry_to_state(&compound), BlockStateId::AIR);
        }
        assert!(!first_report(&UNKNOWN_PALETTE_BLOCKS, name));
        assert_eq!(
            palette_entry_to_state(&NbtCompound::new()),
            BlockStateId::AIR
        );
    }

    #[test]
    fn palette_entries_ignore_properties_the_block_does_not_have() {
        use super::palette_entry_to_state;

        let mut properties = NbtCompound::new();
        properties.put_string("lit", "true".to_string());
        let mut stone = NbtCompound::new();
        stone.put_string("Name", "minecraft:stone".to_string());
        stone.put_compound("Properties", properties.clone());
        assert_eq!(
            palette_entry_to_state(&stone),
            Block::STONE.default_state.id
        );

        properties.put_string("facing", "west".to_string());
        let mut repeater = NbtCompound::new();
        repeater.put_string("Name", "minecraft:repeater".to_string());
        repeater.put_compound("Properties", properties);
        let id = palette_entry_to_state(&repeater);
        assert_eq!(Block::from_state_id(id), &Block::REPEATER);
        let mut expected = properties_of(Block::REPEATER.default_state.id);
        for (name, value) in &mut expected {
            if name == "facing" {
                *value = "west".to_string();
            }
        }
        assert_eq!(properties_of(id), expected);
    }

    #[test]
    fn palette_entries_keep_the_default_for_an_unknown_value() {
        use super::palette_entry_to_state;

        let entry = |name: &str, pairs: &[(&str, &str)]| {
            let mut properties = NbtCompound::new();
            for (key, value) in pairs {
                properties.put_string(key, (*value).to_string());
            }
            let mut compound = NbtCompound::new();
            compound.put_string("Name", name.to_string());
            compound.put_compound("Properties", properties);
            compound
        };

        // The default level of a light block is 15.
        let light = palette_entry_to_state(&entry(
            "minecraft:light",
            &[("level", "99"), ("waterlogged", "true")],
        ));
        assert_eq!(Block::from_state_id(light), &Block::LIGHT);
        assert_eq!(
            properties_of(light),
            pairs(&[("level", "15"), ("waterlogged", "true")])
        );

        // The default of a hopper's `enabled` is true, and an unknown facing keeps its default.
        let hopper = palette_entry_to_state(&entry(
            "minecraft:hopper",
            &[("enabled", "maybe"), ("facing", "sideways")],
        ));
        assert_eq!(hopper, Block::HOPPER.default_state.id);
    }

    #[test]
    fn rejects_unknown_blocks_and_other_shapes() {
        let mut compound = NbtCompound::new();
        compound.put_string("Name", "minecraft:not_a_block".to_string());
        assert_eq!(block_state_from_nbt(&NbtTag::Compound(compound)), None);
        assert_eq!(
            block_state_from_nbt(&NbtTag::String("not_a_block".into())),
            None
        );
        assert_eq!(block_state_from_nbt(&NbtTag::Byte(1)), None);
        assert_eq!(
            block_state_from_nbt(&NbtTag::Compound(NbtCompound::new())),
            None
        );
    }
}
