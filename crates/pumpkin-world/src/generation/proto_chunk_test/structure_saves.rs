use crate::chunk_system::chunk_state::StagedChunkEnum;
use crate::generation::{generator::WorldGenerator, get_world_gen, proto_chunk::ProtoChunk};
use pumpkin_data::dimension::Dimension;
use pumpkin_util::world_seed::Seed;

/// A village generated, saved and read back with an empty structure cache is still found
/// at its position, through the `structures` tag of the chunk data.
#[test]
fn structure_starts_survive_a_save_and_a_cleared_cache() {
    use crate::chunk::ChunkData;
    use crate::chunk::format::anvil::SingleChunkDataSerializer;
    use crate::chunk_system::chunk_state::Chunk;
    use crate::generation::structure::{placement::GlobalStructureCache, start::pack_chunk_pos};
    use pumpkin_config::lighting::LightingEngineConfig;
    use pumpkin_data::structures::StructureKeys;
    use pumpkin_util::math::{position::BlockPos, vector2::Vector2};
    use std::sync::Arc;

    let seed = Seed(1_782_124_772_053_846_960);
    let world_gen = get_world_gen(seed, Dimension::OVERWORLD, false, Vec::new(), String::new());
    let WorldGenerator::Noise(generator) = &*world_gen else {
        unreachable!()
    };
    // `/locate structure minecraft:village_plains` from 0 0 for this seed: the start chunk
    // and the located position, which `find_nearest_structure_start` takes minutes to find
    // in a debug build.
    let chunk_pos = Vector2::new(36, -126);
    let village_pos = BlockPos::new(583, 56, -2019);

    let mut proto = ProtoChunk::new(chunk_pos.x, chunk_pos.y, &world_gen);
    proto.step_to_biomes(generator);
    proto.set_structure_starts(generator);
    proto.set_structure_references(generator);
    assert!(proto.has_structure(StructureKeys::VillagePlains));

    let mut staged = Chunk::Proto(Box::new(proto));
    staged.upgrade_to_level_chunk(&Dimension::OVERWORLD, &LightingEngineConfig::Default);
    let Chunk::Level(chunk_data) = staged else {
        unreachable!()
    };
    let bytes = chunk_data.to_bytes().expect("chunk serializes");
    let reloaded = ChunkData::from_bytes(&bytes, chunk_pos).expect("chunk parses");

    let start = reloaded
        .structures
        .get_compound("starts")
        .and_then(|starts| starts.get_compound("minecraft:village_plains"))
        .expect("the village start is saved");
    assert_eq!(start.get_string("id"), Some("minecraft:village_plains"));
    assert_eq!(start.get_int("ChunkX"), Some(chunk_pos.x));
    assert_eq!(start.get_int("ChunkZ"), Some(chunk_pos.y));
    let first_piece = start
        .get_list("Children")
        .and_then(|children| children.first())
        .and_then(pumpkin_nbt::tag::NbtTag::extract_compound)
        .expect("the start has pieces");
    assert_eq!(first_piece.get_string("id"), Some("minecraft:jigsaw"));
    assert_eq!(first_piece.get_int_array("BB").map(<[i32]>::len), Some(6));
    let own_reference = reloaded
        .structures
        .get_compound("References")
        .and_then(|references| references.get("minecraft:village_plains"))
        .and_then(pumpkin_nbt::tag::NbtTag::extract_long_array)
        .expect("the chunk references its own start");
    assert!(own_reference.contains(&pack_chunk_pos(chunk_pos.x, chunk_pos.y)));

    // A new cache stands for a restarted server: only the chunk data knows the village.
    let cache = GlobalStructureCache::new();
    for start in reloaded.structure_starts() {
        cache.add_saved_start(start);
    }
    let found = cache.structure_starts_at(&village_pos, &reloaded.structure_references());
    assert!(
        found
            .iter()
            .any(|start| start.structure == StructureKeys::VillagePlains
                && start.chunk_pos == chunk_pos),
        "village at {village_pos:?} not found after reload: {found:?}"
    );

    // A chunk resumed past the structure stages keeps the tag it was read with.
    let mut full = Arc::into_inner(chunk_data).expect("only owner");
    full.status = pumpkin_data::chunk::ChunkStatus::Full;
    let resumed = ProtoChunk::from_chunk_data(&full, &world_gen);
    assert_eq!(resumed.structure_data(), full.structures);
    assert_eq!(
        full.structures, reloaded.structures,
        "save and load keep the structures tag"
    );
}

/// The village of `structure_starts_survive_a_save_and_a_cleared_cache` is written with the
/// piece fields that vanilla 26.3 writes: the fixture is the start of chunk 36 -126 from a
/// world that the vanilla server generated with the same seed.
#[test]
fn village_start_tag_matches_vanilla() {
    use pumpkin_nbt::{nbt_compress::read_gzip_compound_tag, tag::NbtTag};

    let expected = read_gzip_compound_tag(std::io::Cursor::new(include_bytes!(
        "../../../../../assets/tests/village_plains_start_36_-126.nbt"
    )))
    .expect("fixture parses");

    let seed = Seed(1_782_124_772_053_846_960);
    let world_gen = get_world_gen(seed, Dimension::OVERWORLD, false, Vec::new(), String::new());
    let WorldGenerator::Noise(generator) = &*world_gen else {
        unreachable!()
    };
    let mut proto = ProtoChunk::new(36, -126, &world_gen);
    proto.step_to_biomes(generator);
    proto.set_structure_starts(generator);
    let structures = proto.structure_data();
    let start = structures
        .get_compound("starts")
        .and_then(|starts| starts.get_compound("minecraft:village_plains"))
        .expect("the village start is written");

    let pieces = |start: &pumpkin_nbt::compound::NbtCompound| {
        start
            .get_list("Children")
            .unwrap_or_default()
            .iter()
            .filter_map(NbtTag::extract_compound)
            .cloned()
            .collect::<Vec<_>>()
    };
    let (written, vanilla) = (pieces(start), pieces(&expected));
    for (index, (written, vanilla)) in written.iter().zip(&vanilla).enumerate() {
        assert_eq!(written, vanilla, "piece {index}");
    }
    assert_eq!(written.len(), vanilla.len());
    assert_eq!(start, &expected);
}

/// Template structure starts are written with the piece fields that vanilla 26.3 writes:
/// the fixture holds the starts of a world that the vanilla server generated with the
/// same seed, keyed by structure. Vanilla saved them after placement, which moves some
/// pieces to the terrain height; this test stops before placement, so it leaves out the
/// fields that placement changes.
#[test]
fn template_start_tags_match_vanilla() {
    use pumpkin_nbt::{compound::NbtCompound, nbt_compress::read_gzip_compound_tag, tag::NbtTag};

    let expected = read_gzip_compound_tag(std::io::Cursor::new(include_bytes!(
        "../../../../../assets/tests/template_structure_starts.nbt"
    )))
    .expect("fixture parses");
    let without = |start: &NbtCompound, placement_fields: &[&str]| {
        let mut start = start.clone();
        if let Some(NbtTag::List(children)) = start.child_tags.get_mut("Children") {
            for child in children {
                if let NbtTag::Compound(piece) = child {
                    for field in placement_fields {
                        piece.child_tags.remove(*field);
                    }
                }
            }
        }
        start
    };

    let seed = Seed(1_782_124_772_053_846_960);
    let mut mismatches = Vec::new();
    for (structure, dimension, chunk_x, chunk_z, placement_fields) in [
        (
            "minecraft:igloo",
            Dimension::OVERWORLD,
            134,
            -224,
            &["BB"][..],
        ),
        (
            "minecraft:shipwreck",
            Dimension::OVERWORLD,
            0,
            -164,
            &["BB", "TPY", "height_adjusted"][..],
        ),
        (
            "minecraft:shipwreck_beached",
            Dimension::OVERWORLD,
            24,
            -142,
            &["BB", "TPY", "height_adjusted"][..],
        ),
        (
            "minecraft:ocean_ruin_warm",
            Dimension::OVERWORLD,
            29,
            -135,
            &["BB", "TPY"][..],
        ),
        (
            "minecraft:ocean_ruin_cold",
            Dimension::OVERWORLD,
            40,
            -176,
            &["BB", "TPY"][..],
        ),
        ("minecraft:end_city", Dimension::THE_END, 88, 24, &[][..]),
    ] {
        let world_gen = get_world_gen(seed, dimension, false, Vec::new(), String::new());
        let WorldGenerator::Noise(generator) = &*world_gen else {
            unreachable!()
        };
        let mut proto = ProtoChunk::new(chunk_x, chunk_z, &world_gen);
        proto.step_to_biomes(generator);
        proto.set_structure_starts(generator);
        let structures = proto.structure_data();
        let written = structures
            .get_compound("starts")
            .and_then(|starts| starts.get_compound(structure))
            .map(|start| without(start, placement_fields));
        let vanilla = expected
            .get_compound(structure)
            .map(|start| without(start, placement_fields));
        if written != vanilla {
            mismatches.push(structure);
        }
    }
    assert!(mismatches.is_empty(), "differ from vanilla: {mismatches:?}");
}

/// A chunk saved before the structure stages gets its starts and references in the tag
/// once generation resumes, instead of the empty tag it was saved with.
#[test]
fn structure_tag_is_rebuilt_for_a_chunk_saved_before_structure_stages() {
    use crate::chunk_system::chunk_state::Chunk;
    use crate::generation::structure::start::pack_chunk_pos;
    use pumpkin_config::lighting::LightingEngineConfig;

    let seed = Seed(1_782_124_772_053_846_960);
    let world_gen = get_world_gen(seed, Dimension::OVERWORLD, false, Vec::new(), String::new());
    let WorldGenerator::Noise(generator) = &*world_gen else {
        unreachable!()
    };
    // The start chunk of the village from `structure_starts_survive_a_save_and_a_cleared_cache`.
    let mut proto = ProtoChunk::new(36, -126, &world_gen);
    proto.step_to_biomes(generator);
    let mut staged = Chunk::Proto(Box::new(proto));
    staged.upgrade_to_level_chunk(&Dimension::OVERWORLD, &LightingEngineConfig::Default);
    let Chunk::Level(at_biomes) = staged else {
        unreachable!()
    };
    assert!(!at_biomes.structures.is_empty(), "the empty tag is written");

    let mut resumed = ProtoChunk::from_chunk_data(&at_biomes, &world_gen);
    assert_eq!(resumed.stage, StagedChunkEnum::Biomes);
    resumed.set_structure_starts(generator);
    resumed.set_structure_references(generator);
    let mut staged = Chunk::Proto(Box::new(resumed));
    staged.upgrade_to_level_chunk(&Dimension::OVERWORLD, &LightingEngineConfig::Default);
    let Chunk::Level(chunk_data) = staged else {
        unreachable!()
    };

    assert!(
        chunk_data
            .structures
            .get_compound("starts")
            .is_some_and(|starts| starts.get_compound("minecraft:village_plains").is_some()),
        "the village start is in the tag"
    );
    assert!(
        chunk_data
            .structures
            .get_compound("References")
            .and_then(|references| references.get("minecraft:village_plains"))
            .and_then(pumpkin_nbt::tag::NbtTag::extract_long_array)
            .is_some_and(|chunks| chunks.contains(&pack_chunk_pos(36, -126))),
        "the village reference is in the tag"
    );
}
