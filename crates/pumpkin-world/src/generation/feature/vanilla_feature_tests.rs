//! Configured features compared with vanilla 26.3.
//!
//! Each file in `testdata/` is the output of a Java oracle. The oracle loads the world registries
//! and the block tags from the vanilla data pack of the 26.3 server jar, as a server does, and runs
//! the vanilla configured feature with `new XoroshiroRandomSource(seed)`. A `<feature>:<index>`
//! capture runs entry `<index>` of a simple random selector through `FeaturePlacer`. Its level is one
//! vanilla `ProtoChunk` at chunk (0, 0) for every chunk position, as the Pumpkin `ProtoChunk` of the
//! test wraps x and z with `& 15`. The biome is plains and the sea level is 63. The first line gives
//! the result of `place` and the next `nextInt()` of the random. Each other line is a block that
//! differs from the start world, sorted by x, y and z, with its vanilla block state id.

use pumpkin_data::{Block, BlockState, BlockStateId};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use pumpkin_util::random::{RandomGenerator, RandomImpl, xoroshiro128::Xoroshiro};

use super::configured_features::{CONFIGURED_FEATURES, ConfiguredFeature};
use super::placed_features::tests::{TestPortal, empty_chunk};
use crate::ProtoChunk;
use crate::generation::proto_chunk::GenerationCache;

#[derive(Clone, Copy)]
enum World {
    /// Sand up to y 63, air above.
    Desert,
    /// Air from y 60 to 72, stone elsewhere.
    Cave,
    /// Air from y -30 to -20, deepslate elsewhere.
    DeepDark,
    /// Sand up to y 50, water up to y 62, air above.
    Ocean,
}

impl World {
    fn state(self, y: i32) -> &'static BlockState {
        match self {
            Self::Desert if y <= 63 => Block::SAND.default_state,
            Self::Cave if !(60..=72).contains(&y) => Block::STONE.default_state,
            Self::DeepDark if !(-30..=-20).contains(&y) => Block::DEEPSLATE.default_state,
            Self::Ocean if y <= 50 => Block::SAND.default_state,
            Self::Ocean if y <= 62 => Block::WATER.default_state,
            _ => Block::AIR.default_state,
        }
    }

    fn chunk(self) -> ProtoChunk {
        let mut chunk = empty_chunk();
        for y in -64..320 {
            let state = self.state(y);
            if state.id == BlockStateId::AIR {
                continue;
            }
            for x in 0..16 {
                for z in 0..16 {
                    chunk.set_block_state(x, y, z, state);
                }
            }
        }
        chunk
    }
}

struct Expected {
    placed: bool,
    next_int: i32,
    blocks: Vec<(i32, i32, i32, u16)>,
}

fn parse(data: &str) -> Expected {
    let mut lines = data.lines();
    let header = lines.next().expect("oracle header");
    let field = |name: &str| {
        header
            .split_whitespace()
            .find_map(|word| word.strip_prefix(name))
            .unwrap_or_else(|| panic!("missing {name} in {header}"))
    };
    let blocks = lines
        .map(|line| {
            let tuple = line
                .split("//")
                .next()
                .unwrap()
                .trim()
                .trim_end_matches(',')
                .trim_start_matches('(')
                .trim_end_matches(')');
            let values: Vec<i32> = tuple
                .split(',')
                .map(|value| value.trim().parse().unwrap())
                .collect();
            (values[0], values[1], values[2], values[3] as u16)
        })
        .collect();
    Expected {
        placed: field("placed=") == "true",
        next_int: field("next=").parse().unwrap(),
        blocks,
    }
}

fn assert_matches_vanilla(
    feature: pumpkin_data::configured_feature::ConfiguredFeature,
    world: World,
    seed: u64,
    origin: BlockPos,
    data: &str,
) {
    assert_place_matches_vanilla(
        &format!("{feature:?}"),
        world,
        seed,
        data,
        |chunk, random| {
            CONFIGURED_FEATURES[&feature].generate(
                chunk,
                &TestPortal,
                -64,
                384,
                pumpkin_data::placed_feature::PlacedFeature::DesertWell,
                random,
                origin,
            )
        },
    );
}

/// Compares one entry of a simple random selector with a `<feature>:<index>` capture.
fn assert_selector_entry_matches_vanilla(
    feature: pumpkin_data::configured_feature::ConfiguredFeature,
    index: usize,
    world: World,
    seed: u64,
    origin: BlockPos,
    data: &str,
) {
    let ConfiguredFeature::SimpleRandomSelector(selector) = &CONFIGURED_FEATURES[&feature] else {
        panic!("{feature:?} is not a simple random selector");
    };
    assert_place_matches_vanilla(
        &format!("{feature:?}:{index}"),
        world,
        seed,
        data,
        |chunk, random| {
            selector.features[index].generate(
                chunk,
                &TestPortal,
                -64,
                384,
                pumpkin_data::placed_feature::PlacedFeature::WarmOceanVegetation,
                random,
                origin,
            )
        },
    );
}

fn assert_place_matches_vanilla(
    feature: &str,
    world: World,
    seed: u64,
    data: &str,
    place: impl FnOnce(&mut ProtoChunk, &mut RandomGenerator) -> bool,
) {
    let expected = parse(data);
    let mut chunk = world.chunk();
    let mut random = RandomGenerator::Xoroshiro(Xoroshiro::from_seed(seed));
    let placed = place(&mut chunk, &mut random);

    let mut blocks = Vec::new();
    for x in 0..16 {
        for y in -64..320 {
            for z in 0..16 {
                let state = GenerationCache::get_block_state(&chunk, &Vector3::new(x, y, z));
                if state != world.state(y).id {
                    blocks.push((x, y, z, state.as_u16()));
                }
            }
        }
    }
    let differences: Vec<_> = blocks
        .iter()
        .filter(|block| !expected.blocks.contains(block))
        .map(|block| ("extra", *block))
        .chain(
            expected
                .blocks
                .iter()
                .filter(|block| !blocks.contains(block))
                .map(|block| ("missing", *block)),
        )
        .collect();
    assert!(
        differences.is_empty(),
        "{feature} seed {seed}: {differences:?}"
    );
    assert_eq!(placed, expected.placed, "{feature} seed {seed}: result");
    assert_eq!(
        random.next_i32(),
        expected.next_int,
        "{feature} seed {seed}: random"
    );
}

const DESERT_ORIGIN: BlockPos = BlockPos(Vector3::new(8, 63, 8));
const CAVE_ORIGIN: BlockPos = BlockPos(Vector3::new(8, 60, 8));
const DEEP_DARK_ORIGIN: BlockPos = BlockPos(Vector3::new(8, -30, 8));
const OCEAN_ORIGIN: BlockPos = BlockPos(Vector3::new(8, 51, 8));

#[test]
fn desert_well_matches_vanilla() {
    use pumpkin_data::configured_feature::ConfiguredFeature::DesertWell;
    for (seed, data) in [
        (1, include_str!("testdata/desert_well_seed_1.txt")),
        (7, include_str!("testdata/desert_well_seed_7.txt")),
    ] {
        assert_matches_vanilla(DesertWell, World::Desert, seed, DESERT_ORIGIN, data);
    }
}

#[test]
fn sulfur_spring_matches_vanilla() {
    use pumpkin_data::configured_feature::ConfiguredFeature::SulfurSpring;
    for (seed, data) in [
        (1, include_str!("testdata/sulfur_spring_seed_1.txt")),
        (5, include_str!("testdata/sulfur_spring_seed_5.txt")),
    ] {
        assert_matches_vanilla(SulfurSpring, World::Cave, seed, CAVE_ORIGIN, data);
    }
}

#[test]
fn sulfur_pool_matches_vanilla() {
    use pumpkin_data::configured_feature::ConfiguredFeature::SulfurPool;
    let data = include_str!("testdata/sulfur_pool_seed_1.txt");
    assert_matches_vanilla(SulfurPool, World::Cave, 1, CAVE_ORIGIN, data);
}

#[test]
fn sculk_patch_matches_vanilla() {
    use pumpkin_data::configured_feature::ConfiguredFeature::{
        SculkPatchAncientCity, SculkPatchDeepDark,
    };
    // Seed 2 stops the sequence at the catalyst: the sculk stays and `place` gives false. Seed 1
    // of the ancient city places shriekers.
    for (feature, seed, data) in [
        (
            SculkPatchDeepDark,
            1,
            include_str!("testdata/sculk_patch_deep_dark_seed_1.txt"),
        ),
        (
            SculkPatchDeepDark,
            2,
            include_str!("testdata/sculk_patch_deep_dark_seed_2.txt"),
        ),
        (
            SculkPatchAncientCity,
            1,
            include_str!("testdata/sculk_patch_ancient_city_seed_1.txt"),
        ),
    ] {
        assert_matches_vanilla(feature, World::DeepDark, seed, DEEP_DARK_ORIGIN, data);
    }
}

/// The coral block feature places its block and the coral decoration: `random_block` providers
/// in a `rotated` provider.
#[test]
fn coral_block_matches_vanilla() {
    use pumpkin_data::configured_feature::ConfiguredFeature::CoralHornBlock;
    for (seed, data) in [
        (1, include_str!("testdata/coral_horn_block_seed_1.txt")),
        (2, include_str!("testdata/coral_horn_block_seed_2.txt")),
        (3, include_str!("testdata/coral_horn_block_seed_3.txt")),
    ] {
        assert_matches_vanilla(CoralHornBlock, World::Ocean, seed, OCEAN_ORIGIN, data);
    }
}

/// The coral tree and the coral claw entries of `warm_ocean_vegetation` place through their
/// `feature` field.
#[test]
fn coral_tree_and_claw_match_vanilla() {
    use pumpkin_data::configured_feature::ConfiguredFeature::WarmOceanVegetation;
    for (index, seed, data) in [
        (
            0,
            1,
            include_str!("testdata/warm_ocean_vegetation_0_seed_1.txt"),
        ),
        (
            0,
            2,
            include_str!("testdata/warm_ocean_vegetation_0_seed_2.txt"),
        ),
        (
            1,
            1,
            include_str!("testdata/warm_ocean_vegetation_1_seed_1.txt"),
        ),
        (
            1,
            2,
            include_str!("testdata/warm_ocean_vegetation_1_seed_2.txt"),
        ),
        (
            1,
            3,
            include_str!("testdata/warm_ocean_vegetation_1_seed_3.txt"),
        ),
        (
            2,
            1,
            include_str!("testdata/warm_ocean_vegetation_2_seed_1.txt"),
        ),
    ] {
        assert_selector_entry_matches_vanilla(
            WarmOceanVegetation,
            index,
            World::Ocean,
            seed,
            OCEAN_ORIGIN,
            data,
        );
    }
}
