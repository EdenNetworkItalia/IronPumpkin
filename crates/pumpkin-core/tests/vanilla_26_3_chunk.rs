//! A chunk that a vanilla 26.3 server saved (`DataVersion` 5023) loads with its palette, decorated
//! pots, signs and item components intact.
//!
//! `assets/tests/vanilla_26_3_chunk.nbt` is section `Y=-4` and six block entities of chunk 0,0 of
//! a flat world. The server placed them with these commands:
//!
//! ```text
//! setblock 1 -60 1 minecraft:decorated_pot[facing=east]{sherds:{back:{id:"minecraft:archer_pottery_sherd"},left:{id:"minecraft:brick"},right:{id:"minecraft:prize_pottery_sherd"},front:{id:"minecraft:miner_pottery_sherd"}},item:{id:"minecraft:diamond",count:3}}
//! setblock 2 -60 1 minecraft:decorated_pot{sherds:{front:{id:"minecraft:heart_pottery_sherd"}}}
//! setblock 3 -60 1 minecraft:oak_sign[rotation=4]{front_text:{messages:[{text:"run",click_event:{action:"run_command",command:"/say hi"}},"two","",""],color:"red",has_glowing_text:true},back_text:{messages:["back","","",""]},is_waxed:true,allow_op_features:true}
//! setblock 4 -60 1 minecraft:oak_sign{front_text:{messages:["plain","","",""]}}
//! setblock 5 -60 1 minecraft:oak_hanging_sign{front_text:{messages:[{text:"hang",click_event:{action:"run_command",command:"/say h"}},"","",""]},allow_op_features:true}
//! setblock 6 -60 1 minecraft:chest{Items:[{Slot:0b,id:"minecraft:decorated_pot",count:1,components:{"minecraft:pot_decorations":{back:{id:"minecraft:archer_pottery_sherd"},front:{id:"minecraft:brick"}}}},{Slot:1b,id:"minecraft:oak_sign",count:1,components:{"minecraft:block_entity_data":{id:"minecraft:sign",front_text:{messages:["item","","",""]}}}}, ...]}
//! setblock 8 -60 1 minecraft:oak_stairs[facing=south,half=top,waterlogged=true]
//! setblock 9 -60 1 minecraft:repeater[delay=3,facing=east,powered=true]
//! setblock 10 -60 1 minecraft:grass_block[snowy=true]
//! ```
//!
//! The section palette mixes bare names (default states) and `{id, properties}` compounds, the two
//! shapes of 26.3 `BlockState.CODEC`.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

use pumpkin_core::block::entities::block_entity_from_nbt;
use pumpkin_core::block::entities::sign::SignEntityRef;
use pumpkin_data::Block;
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::math::{position::BlockPos, vector2::Vector2};
use pumpkin_world::block::BlockStateCodec;
use pumpkin_world::block::state_nbt::block_state_from_nbt;
use pumpkin_world::chunk::ChunkData;

const VANILLA_CHUNK: &[u8] = include_bytes!("../../../assets/tests/vanilla_26_3_chunk.nbt");

fn read_nbt(bytes: &[u8]) -> NbtCompound {
    let mut cursor = std::io::Cursor::new(bytes);
    let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(
        pumpkin_nbt::deserializer::NbtStreamReader(&mut cursor),
    );
    pumpkin_nbt::Nbt::read(&mut reader).unwrap().root_tag
}

fn load(bytes: &[u8]) -> ChunkData {
    ChunkData::internal_from_bytes(bytes, Vector2::new(0, 0)).unwrap()
}

fn palette(root: &NbtCompound) -> &[NbtTag] {
    root.get_list("sections")
        .unwrap()
        .iter()
        .filter_map(NbtTag::extract_compound)
        .find(|section| section.get_byte("Y") == Some(-4))
        .and_then(|section| section.get_compound("block_states"))
        .and_then(|block_states| block_states.get_list("palette"))
        .unwrap()
}

fn vanilla_block_entity(x: i32, z: i32) -> NbtCompound {
    let root = read_nbt(VANILLA_CHUNK);
    root.get_list("block_entities")
        .unwrap()
        .iter()
        .filter_map(NbtTag::extract_compound)
        .find(|nbt| nbt.get_int("x") == Some(x) && nbt.get_int("z") == Some(z))
        .unwrap()
        .clone()
}

/// Loads the block entity of `nbt` as Pumpkin models it and writes it back.
fn round_trip(nbt: &NbtCompound) -> NbtCompound {
    let entity = block_entity_from_nbt(nbt).unwrap();
    let mut written = NbtCompound::new();
    entity.write_internal(&mut written);
    written
}

/// Vanilla adds an empty `components` and `keepPacked=false` to every chunk block entity
/// (`BlockEntity.saveWithFullMetadata`, `SerializableChunkData`). Neither carries data.
fn without_empty_metadata(nbt: &NbtCompound) -> NbtCompound {
    let mut nbt = nbt.clone();
    if nbt
        .get_compound("components")
        .is_some_and(NbtCompound::is_empty)
    {
        nbt.child_tags.remove("components");
    }
    if nbt.get_bool("keepPacked") == Some(false) {
        nbt.child_tags.remove("keepPacked");
    }
    nbt
}

fn sides(sherds: &NbtCompound) -> Vec<(&str, &str)> {
    ["back", "left", "right", "front"]
        .into_iter()
        .filter_map(|side| {
            let id = sherds.get_compound(side)?.get_string("id")?;
            Some((side, id))
        })
        .collect()
}

/// A bare string is the literal text component `{text}` in `ComponentSerialization.CODEC`.
fn text_components_as_compounds(tag: &NbtTag) -> NbtTag {
    match tag {
        NbtTag::Compound(compound) => {
            let mut out = NbtCompound::new();
            for (key, value) in &compound.child_tags {
                let value = if &**key == "messages" {
                    match value {
                        NbtTag::List(messages) => NbtTag::List(
                            messages
                                .iter()
                                .map(|message| match message {
                                    NbtTag::String(text) => {
                                        let mut literal = NbtCompound::new();
                                        literal.put_string("text", text.to_string());
                                        NbtTag::Compound(literal)
                                    }
                                    other => other.clone(),
                                })
                                .collect(),
                        ),
                        other => other.clone(),
                    }
                } else {
                    text_components_as_compounds(value)
                };
                out.put(key, value);
            }
            NbtTag::Compound(out)
        }
        other => other.clone(),
    }
}

#[test]
fn the_vanilla_palette_loads_every_state() {
    let chunk = load(VANILLA_CHUNK);
    let state = |x: usize, z: usize| chunk.section.get_block_absolute_y(x, -60, z).unwrap();
    let block = |id| Block::from_state_id(id).name;

    // Compound entries.
    let props = |id| Block::from_state_id(id).string_properties(id).unwrap();
    assert_eq!(block(state(1, 1)), "decorated_pot");
    assert!(props(state(1, 1)).contains(&("facing", "east")));
    assert_eq!(block(state(3, 1)), "oak_sign");
    assert!(props(state(3, 1)).contains(&("rotation", "4")));
    assert_eq!(block(state(8, 1)), "oak_stairs");
    for property in [
        ("facing", "south"),
        ("half", "top"),
        ("waterlogged", "true"),
    ] {
        assert!(props(state(8, 1)).contains(&property), "{property:?}");
    }
    assert_eq!(block(state(9, 1)), "repeater");
    for property in [("delay", "3"), ("powered", "true"), ("facing", "east")] {
        assert!(props(state(9, 1)).contains(&property), "{property:?}");
    }
    assert_eq!(block(state(10, 1)), "grass_block");
    assert!(props(state(10, 1)).contains(&("snowy", "true")));

    // Bare names: the default state.
    assert_eq!(state(2, 1), Block::DECORATED_POT.default_state.id);
    assert_eq!(state(6, 1), Block::CHEST.default_state.id);
    assert_eq!(
        chunk.section.get_block_absolute_y(0, -64, 0),
        Some(Block::BEDROCK.default_state.id)
    );
}

#[test]
fn the_palette_writes_back_what_vanilla_wrote() {
    let vanilla_root = read_nbt(VANILLA_CHUNK);
    let vanilla_palette = palette(&vanilla_root);
    let written_root = read_nbt(&load(VANILLA_CHUNK).internal_to_bytes());
    let written_palette = palette(&written_root);

    for entry in vanilla_palette {
        let id = block_state_from_nbt(entry).unwrap();
        let written = written_palette
            .iter()
            .filter_map(NbtTag::extract_compound)
            .find(|written| BlockStateCodec::state_from_nbt(written) == Some(id))
            .unwrap_or_else(|| panic!("{entry:?} is not in the written palette"));
        match entry {
            // Vanilla writes a non-default state as the full compound of `StateHolder.codec`.
            NbtTag::Compound(compound) => assert_eq!(written, compound),
            // A default state is a bare name; the writer gives its full compound.
            NbtTag::String(name) => assert_eq!(written.get_string("id"), Some(&**name)),
            other => panic!("unexpected palette entry {other:?}"),
        }
    }
}

#[test]
fn decorated_pots_round_trip_their_four_side_sherds() {
    // Four sides and an item: the whole block entity comes back. The item compound gets an
    // empty `components`, which carries no data.
    let vanilla = without_empty_metadata(&vanilla_block_entity(1, 1));
    assert_eq!(sides(vanilla.get_compound("sherds").unwrap()).len(), 4);
    let mut written = round_trip(&vanilla);
    let item = without_empty_metadata(written.get_compound("item").unwrap());
    written.put_compound("item", item);
    assert_eq!(written, vanilla);

    // One side: only that side comes back, the other three stay empty.
    let vanilla = without_empty_metadata(&vanilla_block_entity(2, 1));
    assert_eq!(
        sides(vanilla.get_compound("sherds").unwrap()),
        [("front", "minecraft:heart_pottery_sherd")]
    );
    assert_eq!(round_trip(&vanilla), vanilla);
}

#[test]
fn signs_round_trip_their_text_and_op_flag() {
    for (x, z, op) in [(3, 1, true), (4, 1, false), (5, 1, true)] {
        let vanilla = without_empty_metadata(&vanilla_block_entity(x, z));
        let written = round_trip(&vanilla);
        assert_eq!(
            text_components_as_compounds(&NbtTag::Compound(written)),
            text_components_as_compounds(&NbtTag::Compound(vanilla.clone())),
            "sign at {x} {z}"
        );
        let entity = block_entity_from_nbt(&vanilla).unwrap();
        let sign = SignEntityRef::from_block_entity(entity.as_ref()).unwrap();
        assert_eq!(sign.allow_op_features(), op, "sign at {x} {z}");
    }
}

#[test]
fn chest_items_keep_their_pot_decorations_and_block_entity_data() {
    let vanilla = vanilla_block_entity(6, 1);
    let written = round_trip(&vanilla);
    let slot = |nbt: &NbtCompound, slot: i8| {
        nbt.get_list("Items")
            .unwrap()
            .iter()
            .filter_map(NbtTag::extract_compound)
            .find(|item| item.get_byte("Slot") == Some(slot))
            .unwrap()
            .clone()
    };
    // The pot item has two sides, `back` and `front`, and keeps exactly those.
    for index in [0, 1] {
        assert_eq!(slot(&written, index), slot(&vanilla, index), "slot {index}");
    }
    let decorations = slot(&vanilla, 0)
        .get_compound("components")
        .and_then(|components| components.get_compound("minecraft:pot_decorations"))
        .unwrap()
        .clone();
    assert_eq!(
        sides(&decorations),
        [
            ("back", "minecraft:archer_pottery_sherd"),
            ("front", "minecraft:brick")
        ]
    );
}

#[test]
fn signs_of_a_chunk_older_than_5002_keep_their_click_commands() {
    // A 26.2 chunk (DataVersion 4903) has no `allow_op_features`: `SignsOpFlagFix` gives every
    // sign and hanging sign the flag.
    let mut root = read_nbt(VANILLA_CHUNK);
    root.put_int("DataVersion", 4903);
    let block_entities: Vec<NbtTag> = root
        .get_list("block_entities")
        .unwrap()
        .iter()
        .map(|tag| {
            let mut nbt = tag.extract_compound().unwrap().clone();
            nbt.child_tags.remove("allow_op_features");
            NbtTag::Compound(nbt)
        })
        .collect();
    root.put_list("block_entities", block_entities);
    let chunk = load(&pumpkin_nbt::Nbt::from(root).write());

    let pending = chunk.pending_block_entities.lock().unwrap();
    for (x, z) in [(3, 1), (4, 1), (5, 1)] {
        let nbt = &pending[&BlockPos::new(x, -60, z)];
        let entity = block_entity_from_nbt(nbt).unwrap();
        let sign = SignEntityRef::from_block_entity(entity.as_ref()).unwrap();
        assert!(sign.allow_op_features(), "sign at {x} {z}");
    }
    let front = pending[&BlockPos::new(3, -60, 1)]
        .get_compound("front_text")
        .and_then(|text| text.get_list("messages"))
        .unwrap()[0]
        .extract_compound()
        .unwrap()
        .get_compound("click_event")
        .unwrap()
        .clone();
    assert_eq!(front.get_string("action"), Some("run_command"));
    assert!(
        pending[&BlockPos::new(1, -60, 1)]
            .get_bool("allow_op_features")
            .is_none()
    );
}
