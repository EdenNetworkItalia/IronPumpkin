use super::tests::{test_chunk, test_section};
use super::*;
use pumpkin_data::Block;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;

fn repeater_state() -> BlockStateId {
    Block::REPEATER
        .from_properties(&[
            pumpkin_data::Property::Facing(pumpkin_data::block_properties::Facing::East),
            pumpkin_data::Property::Delay(2),
            pumpkin_data::Property::Locked(false),
            pumpkin_data::Property::Powered(false),
        ])
        .to_state_id(&Block::REPEATER)
}

#[test]
fn chunk_palette_writes_the_26_3_block_state_keys() {
    // 26.3 `StateHolder.codec` dispatches on `id` and reads `properties`.
    let chunk = ChunkData::empty(0, 0);
    chunk.set_block_absolute_y(1, 70, 2, repeater_state());

    let bytes = chunk.to_bytes().unwrap();
    let mut cursor = std::io::Cursor::new(&bytes[..]);
    let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(&mut cursor);
    let root = pumpkin_nbt::Nbt::read(&mut reader).unwrap().root_tag;

    let sections = root.get_list("sections").unwrap();
    let section = sections
        .iter()
        .filter_map(NbtTag::extract_compound)
        .find(|section| section.get_byte("Y") == Some(4))
        .unwrap();
    let palette = section
        .get_compound("block_states")
        .and_then(|block_states| block_states.get_list("palette"))
        .unwrap();
    let repeater = palette
        .iter()
        .filter_map(NbtTag::extract_compound)
        .find(|entry| entry.get_string("id") == Some("minecraft:repeater"))
        .expect("the palette has a repeater entry under `id`");
    let properties = repeater.get_compound("properties").unwrap();
    assert_eq!(properties.get_string("facing"), Some("east"));
    assert_eq!(properties.get_string("delay"), Some("2"));
    assert_eq!(properties.get_string("locked"), Some("false"));
    assert_eq!(properties.get_string("powered"), Some("false"));

    for entry in palette.iter().filter_map(NbtTag::extract_compound) {
        assert!(entry.get_string("Name").is_none());
        assert!(entry.get_compound("Properties").is_none());
    }
}

#[test]
fn chunk_palette_reads_both_key_shapes_and_bare_names() {
    // 26.3 writes `id` and `properties`, 26.2 and older wrote `Name` and `Properties`,
    // and a palette entry can also be a bare block name.
    let mut new_entry = NbtCompound::new();
    new_entry.put_string("id", "minecraft:repeater".to_string());
    let mut new_properties = NbtCompound::new();
    new_properties.put_string("facing", "east".to_string());
    new_properties.put_string("delay", "2".to_string());
    new_properties.put_string("locked", "false".to_string());
    new_properties.put_string("powered", "false".to_string());
    new_entry.put_compound("properties", new_properties.clone());

    let mut old_entry = NbtCompound::new();
    old_entry.put_string("Name", "minecraft:repeater".to_string());
    old_entry.put_compound("Properties", new_properties);

    let palette = NbtTag::List(vec![
        NbtTag::Compound(new_entry),
        NbtTag::Compound(old_entry),
        NbtTag::String("minecraft:stone".to_string().into()),
    ]);
    let result = extract_u16_array(&palette).expect("should extract palette");
    assert_eq!(
        &*result,
        &[
            repeater_state(),
            repeater_state(),
            Block::STONE.default_state.id
        ]
    );
}

#[test]
fn signs_read_from_data_older_than_5002_get_the_op_flag() {
    use crate::chunk::ChunkData;
    use pumpkin_util::math::vector2::Vector2;

    // `SignsOpFlagFix` is registered at schema 5002 in `DataFixers`: it sets
    // `allow_op_features` on every sign and hanging sign of older data.
    let pos = BlockPos::new(1, -60, 2);
    let flag_after_load = |data_version: Option<i32>, id: &str| {
        let mut sign = NbtCompound::new();
        sign.put_string("id", id.to_string());
        sign.put_int("x", pos.0.x);
        sign.put_int("y", pos.0.y);
        sign.put_int("z", pos.0.z);

        let mut nbt = test_chunk(vec![test_section(-4, "minecraft:stone", true)]);
        match data_version {
            Some(version) => nbt.root_tag.put_int("DataVersion", version),
            None => {
                nbt.root_tag.child_tags.remove("DataVersion");
            }
        }
        nbt.root_tag
            .put_list("block_entities", vec![NbtTag::Compound(sign)]);

        let chunk = ChunkData::from_bytes(&nbt.write(), Vector2::new(0, 0)).unwrap();
        let block_entities = chunk.pending_block_entities.lock().unwrap();
        block_entities[&pos].get_bool("allow_op_features")
    };

    for id in ["minecraft:sign", "minecraft:hanging_sign"] {
        assert_eq!(flag_after_load(Some(5001), id), Some(true));
        assert_eq!(flag_after_load(Some(5002), id), None);
        assert_eq!(flag_after_load(Some(WORLD_DATA_VERSION), id), None);
        // 26.2.
        assert_eq!(flag_after_load(Some(4903), id), Some(true));
        // Vanilla reads a missing version as the oldest one.
        assert_eq!(flag_after_load(None, id), Some(true));
    }
    assert_eq!(flag_after_load(Some(4903), "minecraft:chest"), None);
}

#[test]
fn signs_read_from_a_pump_chunk_older_than_5002_get_the_op_flag() {
    use crate::chunk::ChunkData;
    use pumpkin_util::math::vector2::Vector2;

    // The pump header stores the data version after the magic and the format byte.
    const DATA_VERSION_OFFSET: usize = 5;
    let pos = BlockPos::new(1, -60, 2);
    let flag_after_load = |data_version: i32, id: &str| {
        let chunk = ChunkData::empty(0, 0);
        let mut sign = NbtCompound::new();
        sign.put_string("id", id.to_string());
        sign.put_int("x", pos.0.x);
        sign.put_int("y", pos.0.y);
        sign.put_int("z", pos.0.z);
        chunk
            .pending_block_entities
            .lock()
            .unwrap()
            .insert(pos, sign);

        let mut bytes = chunk.internal_to_pump_bytes().to_vec();
        bytes[DATA_VERSION_OFFSET..DATA_VERSION_OFFSET + 4]
            .copy_from_slice(&data_version.to_be_bytes());
        let loaded = ChunkData::internal_from_pump_bytes(&bytes, Vector2::new(0, 0)).unwrap();
        let block_entities = loaded.pending_block_entities.lock().unwrap();
        block_entities[&pos].get_bool("allow_op_features")
    };

    for id in ["minecraft:sign", "minecraft:hanging_sign"] {
        // IronPumpkin wrote pump chunks at 4903 before it wrote 5023.
        assert_eq!(flag_after_load(4903, id), Some(true));
        assert_eq!(flag_after_load(WORLD_DATA_VERSION, id), None);
    }
    assert_eq!(flag_after_load(4903, "minecraft:chest"), None);
}

#[test]
fn chunk_with_a_missing_section_loads_plains() {
    use crate::chunk::ChunkData;
    use pumpkin_util::math::vector2::Vector2;

    // Sections -4 and -2 are saved, section -3 is not.
    let bytes = test_chunk(vec![
        test_section(-4, "minecraft:stone", true),
        test_section(-2, "minecraft:stone", true),
    ])
    .write();

    let chunk = ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).expect("chunk parses");
    assert_eq!(
        chunk.section.get_rough_biome_absolute_y(0, -48, 0),
        Some(Biome::PLAINS.id)
    );
}

#[test]
fn extract_u16_array_reads_the_26_3_bare_names() {
    use crate::block::state_nbt::fixtures;

    // `BlockState.CODEC` of 26.3 writes the default state of a block as its name alone.
    let vanilla_grass = fixtures::read(fixtures::ENDER_GRASS_26_3)
        .get("carriedBlockState")
        .unwrap()
        .clone();
    assert!(matches!(vanilla_grass, NbtTag::String(_)));
    let list_tag = NbtTag::List(vec![
        NbtTag::String("minecraft:air".into()),
        NbtTag::String("minecraft:stone".into()),
        vanilla_grass,
        NbtTag::String("test:missing_bare_block".into()),
    ]);
    let result = extract_u16_array(&list_tag).expect("should extract palette");

    assert_eq!(
        result.as_ref(),
        [
            BlockStateId::AIR,
            Block::STONE.default_state.id,
            Block::GRASS_BLOCK.default_state.id,
            BlockStateId::AIR,
        ]
    );
}

#[test]
fn chunk_with_a_26_3_bare_name_palette_keeps_its_blocks() {
    use crate::chunk::ChunkData;
    use pumpkin_util::math::vector2::Vector2;

    let mut block_states = NbtCompound::new();
    block_states.put(
        "palette",
        NbtTag::List(vec![
            NbtTag::String("minecraft:air".into()),
            NbtTag::String("minecraft:stone".into()),
        ]),
    );
    block_states.put("data", NbtTag::LongArray(vec![1; 256]));
    let mut section = test_section(-4, "minecraft:dirt", true);
    section.put("block_states", NbtTag::Compound(block_states));
    let bytes = test_chunk(vec![section]).write();

    let chunk = ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).expect("chunk parses");
    assert_eq!(
        chunk.section.get_block_absolute_y(0, -64, 0),
        Some(Block::STONE.default_state.id)
    );
}

#[test]
fn palette_entries_equal_the_vanilla_26_3_compounds() {
    use crate::block::state_nbt::{block_state_from_nbt, fixtures};
    use pumpkin_util::math::vector2::Vector2;

    // Vanilla 26.3 writes a non-default state as this compound. A default state is a bare
    // name there, which `vanilla_26_3_chunk.rs` in pumpkin-core compares by state.
    let vanilla: Vec<NbtCompound> = [
        (fixtures::ENDER_GRASS_SNOWY_26_3, "carriedBlockState"),
        (fixtures::DISPLAY_STAIRS_26_3, "block_state"),
    ]
    .iter()
    .map(|(hex_bytes, key)| fixtures::read(hex_bytes).get_compound(key).unwrap().clone())
    .collect();

    let chunk = ChunkData::empty(0, 0);
    let mut states = Vec::new();
    for (x, entry) in vanilla.iter().enumerate() {
        let state = block_state_from_nbt(&NbtTag::Compound(entry.clone())).unwrap();
        chunk.set_block_absolute_y(x, 64, 0, state);
        states.push(state);
    }

    let bytes = chunk.to_bytes().unwrap();
    let mut cursor = std::io::Cursor::new(&bytes[..]);
    let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(
        pumpkin_nbt::deserializer::NbtStreamReader(&mut cursor),
    );
    let root = pumpkin_nbt::Nbt::read(&mut reader).unwrap().root_tag;
    let section = root
        .get_list("sections")
        .unwrap()
        .iter()
        .filter_map(NbtTag::extract_compound)
        .find(|section| section.get_byte("Y") == Some(4))
        .unwrap();
    let palette = section
        .get_compound("block_states")
        .unwrap()
        .get_list("palette")
        .unwrap();
    for entry in &vanilla {
        assert!(
            palette
                .iter()
                .any(|tag| tag.extract_compound() == Some(entry)),
            "the palette has no entry equal to {entry:?}"
        );
    }

    let loaded = ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).unwrap();
    for (x, state) in states.iter().enumerate() {
        assert_eq!(loaded.section.get_block_absolute_y(x, 64, 0), Some(*state));
    }
}

#[test]
fn chunk_and_entity_writers_declare_the_26_3_data_version() {
    use std::sync::atomic::AtomicBool;

    // Vanilla 26.3. The writers produce its shape, so vanilla runs no data fixer on it.
    const DATA_VERSION_26_3: i32 = 5023;

    let data_version = |bytes: &[u8]| {
        let mut cursor = std::io::Cursor::new(bytes);
        let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(
            pumpkin_nbt::deserializer::NbtStreamReader(&mut cursor),
        );
        pumpkin_nbt::Nbt::read(&mut reader)
            .unwrap()
            .root_tag
            .get_int("DataVersion")
    };

    let chunk = ChunkData::empty(0, 0).to_bytes().unwrap();
    assert_eq!(data_version(&chunk), Some(DATA_VERSION_26_3));

    let mut enderman = NbtCompound::new();
    enderman.put_string("id", "minecraft:enderman".to_string());
    enderman.put_compound(
        "carriedBlockState",
        crate::block::state_nbt::block_state_to_nbt(Block::STONE.default_state.id),
    );
    let entities = ChunkEntityData {
        x: 0,
        z: 0,
        data: std::sync::Mutex::new(vec![enderman]),
        live: AtomicBool::new(false),
        dirty: AtomicBool::new(false),
    }
    .to_bytes()
    .unwrap();
    assert_eq!(data_version(&entities), Some(DATA_VERSION_26_3));
}

#[test]
fn extract_u8_array_maps_an_unknown_biome_to_plains_and_warns_once() {
    let name = "test:missing_biome";
    let list_tag = NbtTag::List(vec![
        NbtTag::String(name.to_string().into()),
        NbtTag::String("minecraft:not_a_biome".to_string().into()),
        NbtTag::String("minecraft:the_void".to_string().into()),
        NbtTag::Compound(NbtCompound::new()),
    ]);
    // Id 0 is badlands.
    assert_ne!(Biome::PLAINS.id, 0);
    for _ in 0..3 {
        let result = extract_u8_array(&list_tag).expect("should extract biome palette");
        assert_eq!(
            result.as_ref(),
            [
                Biome::PLAINS.id,
                Biome::PLAINS.id,
                Biome::THE_VOID.id,
                Biome::PLAINS.id
            ]
        );
    }
    assert!(!first_report(&UNKNOWN_PALETTE_BIOMES, name));
}

#[test]
fn extract_u16_array_loads_an_unknown_bare_name_as_air_and_warns_once() {
    let name = "test:missing_bare_block";
    let list_tag = NbtTag::List(vec![
        NbtTag::String(name.to_string().into()),
        NbtTag::String("minecraft:stone".to_string().into()),
    ]);
    for _ in 0..3 {
        let result = extract_u16_array(&list_tag).expect("should extract block palette");
        assert_eq!(
            result.as_ref(),
            [BlockStateId::AIR, Block::STONE.default_state.id]
        );
    }
    assert!(!first_report(
        &crate::block::state_nbt::UNKNOWN_PALETTE_BLOCKS,
        name
    ));
}

#[test]
fn chunk_with_an_unknown_biome_loads_plains() {
    use crate::chunk::ChunkData;
    use pumpkin_util::math::vector2::Vector2;

    let mut section = test_section(-4, "minecraft:stone", false);
    let mut biomes = NbtCompound::new();
    biomes.put(
        "palette",
        NbtTag::List(vec![NbtTag::String("test:unknown_chunk_biome".into())]),
    );
    section.put("biomes", NbtTag::Compound(biomes));
    let bytes = test_chunk(vec![section]).write();

    let chunk = ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).expect("chunk parses");
    assert_eq!(
        chunk.section.get_rough_biome_absolute_y(0, -64, 0),
        Some(Biome::PLAINS.id)
    );
}

/// A small chunk with a `structures` tag in the layout of vanilla 26.3
/// `SerializableChunkData`: one village start with one piece, and references. Written by
/// hand, the repository has no region file from the vanilla server. The piece's extra
/// fields are not the real `PoolElementStructurePiece` layout: the test proves that the
/// box comes from `BB` and that fields Pumpkin does not read are kept on save.
fn vanilla_chunk_with_structures() -> pumpkin_nbt::Nbt {
    use crate::generation::structure::start::pack_chunk_pos;

    let mut pool_element = NbtCompound::new();
    pool_element.put_string(
        "element_type",
        "minecraft:legacy_single_pool_element".into(),
    );
    pool_element.put_string(
        "location",
        "minecraft:village/plains/town_centers/plains_fountain_01".into(),
    );
    pool_element.put_string("projection", "rigid".into());
    let mut piece = NbtCompound::new();
    piece.put_string("id", "minecraft:jigsaw".into());
    piece.put("BB", NbtTag::IntArray(vec![-31, 63, 17, -22, 72, 26]));
    piece.put_int("O", 0);
    piece.put_int("GD", 0);
    piece.put_int("pos_x", -31);
    piece.put_int("pos_y", 63);
    piece.put_int("pos_z", 17);
    piece.put_int("ground_level_delta", 1);
    piece.put("pool_element", NbtTag::Compound(pool_element));
    piece.put_string("rotation", "NONE".into());
    piece.put_list("junctions", Vec::new());

    let mut start = NbtCompound::new();
    start.put_string("id", "minecraft:village_plains".into());
    start.put_int("ChunkX", -2);
    start.put_int("ChunkZ", 1);
    start.put_int("references", 0);
    start.put_list("Children", vec![NbtTag::Compound(piece)]);
    let mut starts = NbtCompound::new();
    starts.put("minecraft:village_plains", NbtTag::Compound(start));

    let mut references = NbtCompound::new();
    references.put(
        "minecraft:village_plains",
        NbtTag::LongArray(vec![pack_chunk_pos(-2, 1)]),
    );
    // Vanilla drops a reference more than 8 chunks away and an unknown structure.
    references.put(
        "minecraft:mineshaft",
        NbtTag::LongArray(vec![pack_chunk_pos(-2, 1), pack_chunk_pos(20, 1)]),
    );
    references.put(
        "othermod:tower",
        NbtTag::LongArray(vec![pack_chunk_pos(-2, 1)]),
    );

    let mut structures = NbtCompound::new();
    structures.put("starts", NbtTag::Compound(starts));
    structures.put("References", NbtTag::Compound(references));

    let mut root = NbtCompound::new();
    root.put_int("DataVersion", 4903);
    root.put_int("xPos", -2);
    root.put_int("zPos", 1);
    root.put_int("yPos", -4);
    root.put_string("Status", "minecraft:full".to_string());
    root.put_list("sections", Vec::new());
    root.put("structures", NbtTag::Compound(structures));
    pumpkin_nbt::Nbt::new(String::new(), root)
}

#[test]
fn vanilla_structures_tag_feeds_the_lookup_and_survives_a_save() {
    use crate::chunk::ChunkData;
    use crate::generation::structure::placement::GlobalStructureCache;
    use pumpkin_data::structures::StructureKeys;
    use pumpkin_util::math::{block_box::BlockBox, position::BlockPos, vector2::Vector2};

    let bytes = vanilla_chunk_with_structures().write();
    let chunk = ChunkData::from_bytes(&bytes, Vector2::new(-2, 1))
        .expect("chunk with a structures tag parses");

    let references = chunk.structure_references();
    let mut structures: Vec<_> = references.iter().map(|(key, _)| *key).collect();
    structures.sort_by_key(StructureKeys::to_name);
    assert_eq!(
        structures,
        [StructureKeys::Mineshaft, StructureKeys::VillagePlains]
    );
    assert!(
        references
            .iter()
            .all(|(_, start)| *start == Vector2::new(-2, 1))
    );

    let cache = GlobalStructureCache::new();
    for start in chunk.structure_starts() {
        cache.add_saved_start(start);
    }
    let found = cache.structure_starts_at(&BlockPos::new(-26, 70, 20), &references);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].structure, StructureKeys::VillagePlains);
    assert_eq!(
        found[0].bounding_box,
        BlockBox::new(-43, 51, 5, -10, 84, 38)
    );

    // Saving writes the tag back as it was read, with the fields Pumpkin does not use.
    let saved = ChunkData::from_bytes(
        &chunk.to_bytes().expect("chunk serializes"),
        Vector2::new(-2, 1),
    )
    .expect("saved chunk parses");
    assert_eq!(saved.structures, chunk.structures);
    assert_eq!(
        saved
            .structures
            .get_compound("starts")
            .and_then(|starts| starts.get_compound("minecraft:village_plains"))
            .and_then(|start| start.get_list("Children"))
            .and_then(|children| children.first())
            .and_then(NbtTag::extract_compound)
            .and_then(|piece| piece.get_int("ground_level_delta")),
        Some(1)
    );
}

/// A chunk whose biome palette names a biome with and without the `minecraft:` prefix.
fn chunk_with_biome_palette() -> pumpkin_nbt::Nbt {
    let mut section = test_section(-4, "minecraft:stone", false);
    let mut biomes = NbtCompound::new();
    biomes.put(
        "palette",
        NbtTag::List(vec![
            NbtTag::String("minecraft:plains".into()),
            NbtTag::String("desert".into()),
            NbtTag::String("minecraft:the_void".into()),
        ]),
    );
    // 64 cells with 2 bits each, 32 cells per long, cycling through the three palette entries.
    let mut data = [0i64; 2];
    for cell in 0..64usize {
        data[cell / 32] |= ((cell % 3) as i64) << (2 * (cell % 32));
    }
    biomes.put("data", NbtTag::LongArray(data.to_vec()));
    section.put("biomes", NbtTag::Compound(biomes));
    test_chunk(vec![section])
}

#[test]
fn palette_name_keeps_its_namespace() {
    use pumpkin_data::dynamic::namespaced_name;

    assert_eq!(namespaced_name("plains"), "minecraft:plains");
    assert_eq!(namespaced_name("minecraft:plains"), "minecraft:plains");
    assert_eq!(namespaced_name("mymod:foo"), "mymod:foo");
}

#[test]
fn saved_biome_palette_prefixes_vanilla_names() {
    use crate::chunk::ChunkData;
    use pumpkin_util::math::vector2::Vector2;

    let bytes = chunk_with_biome_palette().write();
    let chunk = ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).unwrap();
    let bytes = chunk.to_bytes().unwrap();
    let mut cursor = std::io::Cursor::new(&bytes[..]);
    let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(
        pumpkin_nbt::deserializer::NbtStreamReader(&mut cursor),
    );
    let root = pumpkin_nbt::Nbt::read(&mut reader).unwrap().root_tag;
    let palette: Vec<&str> = root
        .get_list("sections")
        .and_then(|sections| sections.first())
        .and_then(NbtTag::extract_compound)
        .and_then(|section| section.get_compound("biomes"))
        .and_then(|biomes| biomes.get_list("palette"))
        .unwrap()
        .iter()
        .filter_map(NbtTag::extract_string)
        .collect();
    assert_eq!(
        palette,
        ["minecraft:plains", "minecraft:desert", "minecraft:the_void"]
    );
}
