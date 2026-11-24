//! The coral of a warm ocean chunk after the feature step.
//!
//! This is not a block-for-block comparison with vanilla. A coral piece stops where it meets
//! the coral of a neighbour chunk, which shifts the random of the clusters after it, and vanilla
//! runs the feature steps of neighbour chunks in an order that its threads set. Two runs of the
//! 26.3 server jar with seed 0 over chunks (48, -116) to (63, -101) gave different coral in 176
//! of 218 coral chunks. The single coral features are compared with vanilla captures in
//! `vanilla_feature_tests`. This test checks what vanilla always places: the properties and the
//! supports of the coral blocks, no air in their place, and a coral count in the vanilla range.

use std::collections::HashMap;

use pumpkin_data::{Block, BlockState, dimension::Dimension};
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::world_seed::Seed;

use super::stage_parity::{TestBlockRegistry, TestGridCache};
use super::surface_biomes;
use crate::generation::proto_chunk::GenerationCache;
use crate::generation::{generator::WorldGenerator, get_world_gen, proto_chunk::ProtoChunk};

/// The highest water block of the overworld ocean.
const WATER_TOP_Y: i32 = 62;

/// Runs the feature step of the 3x3 chunks around (`cx`, `cz`) of seed 0, so that the center
/// chunk also gets the blocks that the features of its neighbours place in it.
fn generate_features_around(cx: i32, cz: i32) -> TestGridCache {
    let world_gen = get_world_gen(
        Seed(0),
        Dimension::OVERWORLD,
        false,
        Vec::new(),
        String::new(),
    );
    let WorldGenerator::Noise(generator) = &*world_gen else {
        unreachable!()
    };
    // Vanilla needs the structure starts in a radius of 8 chunks of each feature chunk.
    let mut chunks = HashMap::new();
    for nx in (cx - 9)..=(cx + 9) {
        for nz in (cz - 9)..=(cz + 9) {
            let mut chunk = ProtoChunk::new(nx, nz, &world_gen);
            chunk.step_to_biomes(generator);
            chunk.set_structure_starts(generator);
            chunk.set_structure_references(generator);
            if (nx - cx).abs() <= 2 && (nz - cz).abs() <= 2 {
                chunk.step_to_noise(generator);
                chunk.step_to_surface(generator, &surface_biomes(&world_gen, nx, nz));
                chunk.step_to_carvers(generator);
            }
            chunks.insert((nx, nz), chunk);
        }
    }
    let mut cache = TestGridCache {
        center_pos: (cx, cz),
        chunks,
    };
    for fz in (cz - 1)..=(cz + 1) {
        for fx in (cx - 1)..=(cx + 1) {
            cache.center_pos = (fx, fz);
            ProtoChunk::generate_features_and_structure(
                &mut cache,
                &TestBlockRegistry,
                &generator.random_config,
            );
        }
    }
    cache.center_pos = (cx, cz);
    cache
}

fn property(state: &BlockState, key: &str) -> Option<&'static str> {
    Block::from_state_id(state.id)
        .properties(state.id)?
        .to_props()
        .into_iter()
        .find(|p| p.key().to_str() == key)
        .map(pumpkin_data::Property::value_str)
}

fn is_coral_block(block: &Block) -> bool {
    block.name.ends_with("_coral_block")
}

/// Chunk (48, -114) of seed 0 is a warm ocean chunk. In the vanilla capture it has 332 coral
/// blocks; for the chunks of that area with 150 or more coral blocks, the count of one chunk
/// differed by up to 59% between two vanilla runs. In vanilla every coral plant and fan is a
/// default state (`waterlogged=true`), stands on a coral block or hangs on one.
#[test]
fn warm_ocean_coral_is_valid() {
    let (cx, cz) = (48, -114);
    let cache = generate_features_around(cx, cz);
    let state_at = |x, y, z| GenerationCache::get_block_state(&cache, &Vector3::new(x, y, z));

    let mut coral = 0;
    let mut errors = Vec::new();
    for x in cx * 16..(cx + 1) * 16 {
        for z in cz * 16..(cz + 1) * 16 {
            for y in -64..=WATER_TOP_Y {
                let raw = state_at(x, y, z);
                let block = Block::from_state_id(raw);
                let state = BlockState::from_id(raw);
                let name = block.name;
                if block == &Block::AIR {
                    // The decoration of a coral block places above it and on its sides.
                    let next_to_coral = [(0, -1, 0), (1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1)]
                        .iter()
                        .any(|&(dx, dy, dz)| {
                            is_coral_block(Block::from_state_id(state_at(x + dx, y + dy, z + dz)))
                        });
                    if next_to_coral {
                        errors.push(format!("air next to a coral block at {x} {y} {z}"));
                    }
                    continue;
                }
                if !name.contains("coral") {
                    continue;
                }
                coral += 1;
                if is_coral_block(block) {
                    continue;
                }
                if property(state, "waterlogged") != Some("true") {
                    errors.push(format!("{name} at {x} {y} {z} is not waterlogged"));
                }
                let support = if name.ends_with("_wall_fan") {
                    match property(state, "facing") {
                        Some("north") => (x, y, z + 1),
                        Some("south") => (x, y, z - 1),
                        Some("east") => (x - 1, y, z),
                        Some("west") => (x + 1, y, z),
                        facing => panic!("{name} at {x} {y} {z} faces {facing:?}"),
                    }
                } else {
                    (x, y - 1, z)
                };
                let (sx, sy, sz) = support;
                let support_block = Block::from_state_id(state_at(sx, sy, sz));
                if !is_coral_block(support_block) {
                    errors.push(format!(
                        "{name} at {x} {y} {z} is on {} at {sx} {sy} {sz}",
                        support_block.name
                    ));
                }
            }
        }
    }
    assert!(errors.is_empty(), "{} errors: {errors:#?}", errors.len());
    // 332 +- 60%.
    assert!((133..=531).contains(&coral), "{coral} coral blocks");
}
