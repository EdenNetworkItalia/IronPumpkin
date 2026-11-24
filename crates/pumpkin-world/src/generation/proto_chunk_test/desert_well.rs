use pumpkin_data::{BlockId, dimension::Dimension};
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::world_seed::Seed;

use super::stage_parity::{TestBlockRegistry, TestGridCache};
use super::surface_biomes;
use crate::generation::proto_chunk::GenerationCache;
use crate::generation::{generator::WorldGenerator, get_world_gen, proto_chunk::ProtoChunk};

const SEED: u64 = 0;

/// Runs the feature step of chunk (`cx`, `cz`) of the overworld as `stage_parity` does, and
/// returns the 3x3 neighbourhood after it.
fn generate_features(cx: i32, cz: i32) -> TestGridCache {
    let world_gen = get_world_gen(
        Seed(SEED),
        Dimension::OVERWORLD,
        false,
        Vec::new(),
        String::new(),
    );
    let WorldGenerator::Noise(generator) = &*world_gen else {
        unreachable!()
    };
    // Vanilla needs the structure starts in a radius of 8 chunks.
    let mut chunks = std::collections::HashMap::new();
    for nx in (cx - 8)..=(cx + 8) {
        for nz in (cz - 8)..=(cz + 8) {
            let mut chunk = ProtoChunk::new(nx, nz, &world_gen);
            chunk.step_to_biomes(generator);
            chunk.set_structure_starts(generator);
            chunk.set_structure_references(generator);
            if (nx - cx).abs() <= 1 && (nz - cz).abs() <= 1 {
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
    ProtoChunk::generate_features_and_structure(
        &mut cache,
        &TestBlockRegistry,
        &generator.random_config,
    );
    cache
}

/// Counts the well blocks in the 3x3 neighbourhood of (`cx`, `cz`): sandstone slabs, suspicious
/// sand, and the box that holds all of them.
fn well_blocks(
    cache: &TestGridCache,
    cx: i32,
    cz: i32,
) -> (usize, usize, Vector3<i32>, Vector3<i32>) {
    let mut slabs = 0;
    let mut suspicious_sand = 0;
    let mut min = Vector3::new(i32::MAX, i32::MAX, i32::MAX);
    let mut max = Vector3::new(i32::MIN, i32::MIN, i32::MIN);
    for x in (cx - 1) * 16..(cx + 2) * 16 {
        for z in (cz - 1) * 16..(cz + 2) * 16 {
            for y in 40..140 {
                let pos = Vector3::new(x, y, z);
                let block = GenerationCache::get_block_state(cache, &pos).to_block_id();
                if block == BlockId::SANDSTONE_SLAB {
                    slabs += 1;
                } else if block == BlockId::SUSPICIOUS_SAND {
                    suspicious_sand += 1;
                } else {
                    continue;
                }
                min = Vector3::new(min.x.min(x), min.y.min(y), min.z.min(z));
                max = Vector3::new(max.x.max(x), max.y.max(y), max.z.max(z));
            }
        }
    }
    (slabs, suspicious_sand, min, max)
}

/// Chunk (-39, -154) of seed 0 is a desert chunk whose `desert_well` placement passes the rarity
/// filter. The well is 5 blocks wide and 7 blocks high: 12 sandstone slabs and 2 suspicious sand.
#[test]
fn desert_well_generates_in_chunk() {
    let (cx, cz) = (-39, -154);
    let cache = generate_features(cx, cz);
    let (slabs, suspicious_sand, min, max) = well_blocks(&cache, cx, cz);
    assert_eq!(slabs, 12, "sandstone slabs");
    assert_eq!(suspicious_sand, 2, "suspicious sand");
    assert_eq!(
        max.sub(&min),
        Vector3::new(4, 6, 4),
        "well box from {min:?} to {max:?}"
    );
}
