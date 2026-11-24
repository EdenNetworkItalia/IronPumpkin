use pumpkin_data::block_properties::is_air;
use pumpkin_data::{Block, BlockDirection, BlockStateId};
use pumpkin_util::HeightMap;
use std::collections::HashMap;
use std::iter;
use std::sync::LazyLock;

use pumpkin_util::biome::FOLIAGE_NOISE;
use pumpkin_util::math::int_provider::IntProvider;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::random::{RandomGenerator, RandomImpl};

use crate::generation::block_predicate::BlockPredicate;
use crate::generation::height_provider::HeightProvider;
use crate::generation::proto_chunk::GenerationCache;
use crate::world::WorldPortalExt;

use super::configured_features::{CONFIGURED_FEATURES, ConfiguredFeature};

pub static PLACED_FEATURES: LazyLock<
    HashMap<pumpkin_data::placed_feature::PlacedFeature, PlacedFeature>,
> = LazyLock::new(build_placed_features);

pub enum PlacedFeatureWrapper {
    Direct(PlacedFeature),
    Named(pumpkin_data::placed_feature::PlacedFeature),
}

impl PlacedFeatureWrapper {
    #[must_use]
    pub fn get(&self) -> Option<&PlacedFeature> {
        match self {
            Self::Named(name) => PLACED_FEATURES.get(name),
            Self::Direct(feature) => Some(feature),
        }
    }
}

pub struct PlacedFeature {
    /// The name of the configuired feature
    pub feature: Feature,
    pub placement: Vec<PlacementModifier>,
}

pub enum Feature {
    Named(pumpkin_data::configured_feature::ConfiguredFeature),
    Inlined(Box<ConfiguredFeature>),
}

impl PlacedFeature {
    pub fn generate_in_proto_chunk(
        &self,
        chunk: &mut crate::ProtoChunk,
        block_registry: &dyn crate::world::WorldPortalExt,
        feature_name: pumpkin_data::placed_feature::PlacedFeature,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        let Some(feature) = (match &self.feature {
            Feature::Named(name) => CONFIGURED_FEATURES.get(name),
            Feature::Inlined(feature) => Some(feature.as_ref()),
        }) else {
            return false;
        };
        if let ConfiguredFeature::SculkPatch(feature) = feature {
            feature.generate_in_proto_chunk(chunk, random, pos)
        } else {
            let min_y = chunk.generation_bottom_y();
            let height = chunk.generation_height();
            self.generate(
                chunk,
                block_registry,
                min_y,
                height,
                feature_name,
                random,
                pos,
            )
        }
    }

    #[expect(clippy::too_many_arguments)]
    pub fn generate<T: GenerationCache>(
        &self,
        chunk: &mut T,
        block_registry: &dyn WorldPortalExt,
        min_y: i8,
        height: u16,
        feature_name: pumpkin_data::placed_feature::PlacedFeature, // This placed feature
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        let feature = match &self.feature {
            Feature::Named(name) => CONFIGURED_FEATURES.get(name),
            Feature::Inlined(feature) => Some(feature.as_ref()),
        };

        generate_with_modifiers(
            chunk,
            0,
            self.placement.len(),
            random,
            pos,
            &|chunk, modifier_index, random, pos| {
                self.placement[modifier_index].get_positions(
                    chunk,
                    block_registry,
                    min_y,
                    height,
                    feature_name,
                    random,
                    pos,
                )
            },
            &|chunk, random, pos| {
                feature.is_some_and(|feature| {
                    feature.generate(
                        chunk,
                        block_registry,
                        min_y,
                        height,
                        feature_name,
                        random,
                        pos,
                    )
                })
            },
        )
    }
}

fn generate_with_modifiers<T, P, G>(
    state: &mut T,
    modifier_index: usize,
    modifier_count: usize,
    random: &mut RandomGenerator,
    pos: BlockPos,
    get_positions: &P,
    generate: &G,
) -> bool
where
    P: Fn(&T, usize, &mut RandomGenerator, BlockPos) -> Box<dyn Iterator<Item = BlockPos>>,
    G: Fn(&mut T, &mut RandomGenerator, BlockPos) -> bool,
{
    if modifier_index == modifier_count {
        return generate(state, random, pos);
    }

    let positions = get_positions(state, modifier_index, random, pos);
    let mut generated = false;
    for next_pos in positions {
        if generate_with_modifiers(
            state,
            modifier_index + 1,
            modifier_count,
            random,
            next_pos,
            get_positions,
            generate,
        ) {
            generated = true;
        }
    }
    generated
}

pub enum PlacementModifier {
    BlockPredicateFilter(BlockFilterPlacementModifier),
    RarityFilter(RarityFilterPlacementModifier),
    RandomChance(RandomChancePlacementModifier),
    SurfaceRelativeThresholdFilter(SurfaceThresholdFilterPlacementModifier),
    SurfaceWaterDepthFilter(SurfaceWaterDepthFilterPlacementModifier),
    Biome(BiomePlacementModifier),
    Count(CountPlacementModifier),
    NoiseBasedCount(NoiseBasedCountPlacementModifier),
    NoiseThresholdCount(NoiseThresholdCountPlacementModifier),
    CountOnEveryLayer(CountOnEveryLayerPlacementModifier),
    Cuboid(CuboidPlacementModifier),
    EnvironmentScan(EnvironmentScanPlacementModifier),
    Heightmap(HeightmapPlacementModifier),
    HeightRange(HeightRangePlacementModifier),
    InSquare(SquarePlacementModifier),
    RandomOffset(RandomOffsetPlacementModifier),
    Offset(OffsetPlacementModifier),
    RandomlySelected(RandomlySelectedPlacementModifier),
    FixedPlacement(Vec<BlockPos>),
}

impl PlacementModifier {
    #[expect(clippy::too_many_arguments)]
    pub fn get_positions<T: GenerationCache>(
        &self,
        chunk: &T,
        block_registry: &dyn WorldPortalExt,
        min_y: i8,
        height: u16,
        feature: pumpkin_data::placed_feature::PlacedFeature,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> Box<dyn Iterator<Item = BlockPos>> {
        match self {
            Self::BlockPredicateFilter(modifier) => {
                modifier.get_positions(block_registry, chunk, feature, random, pos)
            }
            Self::RarityFilter(modifier) => {
                modifier.get_positions(block_registry, chunk, feature, random, pos)
            }
            Self::RandomChance(modifier) => {
                modifier.get_positions(block_registry, chunk, feature, random, pos)
            }
            Self::SurfaceRelativeThresholdFilter(modifier) => {
                modifier.get_positions(block_registry, chunk, feature, random, pos)
            }
            Self::SurfaceWaterDepthFilter(modifier) => {
                modifier.get_positions(block_registry, chunk, feature, random, pos)
            }
            Self::Biome(modifier) => {
                modifier.get_positions(block_registry, chunk, feature, random, pos)
            }
            Self::Count(modifier) => modifier.get_positions(random, pos),
            Self::NoiseBasedCount(modifier) => Box::new(modifier.get_positions(random, pos)),
            Self::NoiseThresholdCount(modifier) => modifier.get_positions(random, pos),
            Self::CountOnEveryLayer(modifier) => modifier.get_positions(random, chunk, pos),
            Self::Cuboid(modifier) => modifier.get_positions(random, pos),
            Self::EnvironmentScan(modifier) => modifier.get_positions(chunk, block_registry, pos),
            Self::Heightmap(modifier) => modifier.get_positions(chunk, min_y, height, random, pos),
            Self::HeightRange(modifier) => modifier.get_positions(min_y, height, random, pos),
            Self::InSquare(_) => SquarePlacementModifier::get_positions(random, pos),
            Self::RandomOffset(modifier) => modifier.get_positions(random, pos),
            Self::Offset(modifier) => modifier.get_positions(random, pos),
            Self::RandomlySelected(modifier) => {
                let index = random.next_bounded_i32(modifier.placements.len() as i32) as usize;
                modifier.placements[index].get_positions(
                    chunk,
                    block_registry,
                    min_y,
                    height,
                    feature,
                    random,
                    pos,
                )
            }
            Self::FixedPlacement(positions) => Box::new(positions.clone().into_iter()),
        }
    }
}

pub struct NoiseBasedCountPlacementModifier {
    pub to_count_ratio: i32,
    pub factor: f64,
    pub offset: f64,
}

impl CountPlacementModifierBase for NoiseBasedCountPlacementModifier {
    fn get_count(&self, _random: &mut RandomGenerator, pos: BlockPos) -> i32 {
        let noise = FOLIAGE_NOISE.sample(
            pos.0.x as f64 / self.factor,
            pos.0.z as f64 / self.factor,
            false,
        );
        ((noise + self.offset) * self.to_count_ratio as f64).ceil() as i32
    }
}

pub struct NoiseThresholdCountPlacementModifier {
    pub noise_level: f64,
    pub below_noise: i32,
    pub above_noise: i32,
}

impl CountPlacementModifierBase for NoiseThresholdCountPlacementModifier {
    fn get_count(&self, _random: &mut RandomGenerator, pos: BlockPos) -> i32 {
        let noise = FOLIAGE_NOISE.sample(pos.0.x as f64 / 200.0, pos.0.z as f64 / 200.0, false);
        if noise < self.noise_level {
            self.below_noise
        } else {
            self.above_noise
        }
    }
}

pub struct EnvironmentScanPlacementModifier {
    pub direction_of_search: BlockDirection,
    pub target_condition: BlockPredicate,
    pub allowed_search_condition: Option<BlockPredicate>,
    pub max_steps: i32,
}

impl EnvironmentScanPlacementModifier {
    pub fn get_positions<T: GenerationCache>(
        &self,
        chunk: &T,
        block_registry: &dyn WorldPortalExt,
        pos: BlockPos,
    ) -> Box<dyn Iterator<Item = BlockPos>> {
        let allowed_search_condition = self
            .allowed_search_condition
            .as_ref()
            .unwrap_or(&BlockPredicate::AlwaysTrue);

        if !allowed_search_condition.test(block_registry, chunk, &pos) {
            return Box::new(iter::empty());
        }
        let mut pos = pos;
        for _ in 0..self.max_steps {
            if self.target_condition.test(block_registry, chunk, &pos) {
                return Box::new(iter::once(pos));
            }
            pos = pos.offset(self.direction_of_search.to_offset());

            if chunk.out_of_height(pos.0.y as i16) {
                return Box::new(iter::empty());
            }

            if !allowed_search_condition.test(block_registry, chunk, &pos) {
                break;
            }
        }
        if self.target_condition.test(block_registry, chunk, &pos) {
            return Box::new(iter::once(pos));
        }

        Box::new(iter::empty())
    }
}

pub struct RandomOffsetPlacementModifier {
    pub xz_spread: IntProvider,
    pub y_spread: IntProvider,
}

impl RandomOffsetPlacementModifier {
    pub fn get_positions(
        &self,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> Box<dyn Iterator<Item = BlockPos>> {
        let x = pos.0.x + self.xz_spread.get(random);
        let y = pos.0.y + self.y_spread.get(random);
        let z = pos.0.z + self.xz_spread.get(random);
        Box::new(iter::once(BlockPos(Vector3::new(x, y, z))))
    }
}

pub struct OffsetPlacementModifier {
    pub x: IntProvider,
    pub y: IntProvider,
    pub z: IntProvider,
}

impl OffsetPlacementModifier {
    pub fn get_positions(
        &self,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> Box<dyn Iterator<Item = BlockPos>> {
        let x = pos.0.x + self.x.get(random);
        let y = pos.0.y + self.y.get(random);
        let z = pos.0.z + self.z.get(random);
        Box::new(iter::once(BlockPos(Vector3::new(x, y, z))))
    }
}

pub struct CountOnEveryLayerPlacementModifier {
    pub count: IntProvider,
}

impl CountOnEveryLayerPlacementModifier {
    pub fn get_positions<T: GenerationCache>(
        &self,
        random: &mut RandomGenerator,
        chunk: &T,
        pos: BlockPos,
    ) -> Box<dyn Iterator<Item = BlockPos>> {
        let mut positions = Vec::new(); // Using a Vec to collect results, analogous to Stream.builder()
        let mut i = 0; // Represents the 'targetY' in findPos
        let mut bl;

        loop {
            bl = false;
            for _j in 0..self.count.get(random) {
                let x = random.next_bounded_i32(16) + pos.0.x;
                let z = random.next_bounded_i32(16) + pos.0.z;
                let y = chunk.top_motion_blocking_block_height_exclusive(x, z);

                let n = Self::find_pos(chunk, x, y, z, i);

                if n == i32::MAX {
                    continue;
                }
                positions.push(BlockPos::new(x, n, z));
                bl = true;
            }
            i += 1;
            if !bl {
                break;
            }
        }
        Box::new(positions.into_iter())
    }

    fn find_pos<T: GenerationCache>(chunk: &T, x: i32, y: i32, z: i32, target_y: i32) -> i32 {
        let mut mutable_pos = BlockPos::new(x, y, z);
        let mut found_count = 0;
        let mut current_block_state = GenerationCache::get_block_state(chunk, &mutable_pos.0);

        for j in (chunk.bottom_y() as i32 + 1..=y).rev() {
            mutable_pos.0.y = j - 1;
            let next_block_state = GenerationCache::get_block_state(chunk, &mutable_pos.0);

            if !Self::blocks_spawn(next_block_state)
                && Self::blocks_spawn(current_block_state)
                && next_block_state.to_block_id() != Block::BEDROCK
            {
                if found_count == target_y {
                    return mutable_pos.0.y + 1;
                }
                found_count += 1;
            }
            current_block_state = next_block_state;
        }
        i32::MAX
    }

    fn blocks_spawn(state: BlockStateId) -> bool {
        let block = state.to_block_id();
        is_air(state) || block == Block::WATER || block == Block::LAVA
    }
}

pub struct BlockFilterPlacementModifier {
    pub predicate: BlockPredicate,
}

impl ConditionalPlacementModifier for BlockFilterPlacementModifier {
    fn should_place<T: GenerationCache>(
        &self,
        block_registry: &dyn WorldPortalExt,
        _feature: pumpkin_data::placed_feature::PlacedFeature,
        chunk: &T,
        _random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        self.predicate.test(block_registry, chunk, &pos)
    }
}

pub struct SurfaceThresholdFilterPlacementModifier {
    pub heightmap: HeightMap,
    pub min_inclusive: Option<i32>,
    pub max_inclusive: Option<i32>,
}

impl ConditionalPlacementModifier for SurfaceThresholdFilterPlacementModifier {
    fn should_place<T: GenerationCache>(
        &self,
        _block_registry: &dyn WorldPortalExt,
        _feature: pumpkin_data::placed_feature::PlacedFeature,
        chunk: &T,
        _random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        let y = chunk.get_top_y(&self.heightmap, pos.0.x, pos.0.z);
        let min = y.saturating_add(self.min_inclusive.unwrap_or(i32::MIN));
        let max = y.saturating_add(self.max_inclusive.unwrap_or(i32::MAX));
        min <= pos.0.y && pos.0.y <= max
    }
}

pub struct RarityFilterPlacementModifier {
    pub chance: u32,
}

impl ConditionalPlacementModifier for RarityFilterPlacementModifier {
    fn should_place<T: GenerationCache>(
        &self,
        _block_registry: &dyn WorldPortalExt,
        _feature: pumpkin_data::placed_feature::PlacedFeature,
        _chunk: &T,
        random: &mut RandomGenerator,
        _pos: BlockPos,
    ) -> bool {
        random.next_f32() < 1.0 / self.chance as f32
    }
}

pub struct RandomChancePlacementModifier {
    pub chance: f32,
}

impl ConditionalPlacementModifier for RandomChancePlacementModifier {
    fn should_place<T: GenerationCache>(
        &self,
        _block_registry: &dyn WorldPortalExt,
        _feature: pumpkin_data::placed_feature::PlacedFeature,
        _chunk: &T,
        random: &mut RandomGenerator,
        _pos: BlockPos,
    ) -> bool {
        random.next_f32() < self.chance
    }
}

pub struct RandomlySelectedPlacementModifier {
    pub placements: Vec<PlacementModifier>,
}

pub struct CuboidPlacementModifier {
    pub xz_size: IntProvider,
    pub y_size: IntProvider,
    pub include_edges: bool,
    pub include_interior: bool,
}

impl CuboidPlacementModifier {
    pub fn get_positions(
        &self,
        random: &mut RandomGenerator,
        origin: BlockPos,
    ) -> Box<dyn Iterator<Item = BlockPos>> {
        let height = self.y_size.get(random);
        let width = self.xz_size.get(random);
        let length = self.xz_size.get(random);
        let mut positions = Vec::new();
        for x in 0..=width {
            let x_face = u8::from(x == 0 || x == width);
            for y in 0..=height {
                let y_face = u8::from(y == 0 || y == height);
                for z in 0..=length {
                    let faces = x_face + y_face + u8::from(z == 0 || z == length);
                    // Two faces meet on an edge or corner, none means interior.
                    if (self.include_edges || faces < 2) && (self.include_interior || faces > 0) {
                        positions.push(origin.add(x, y, z));
                    }
                }
            }
        }
        Box::new(positions.into_iter())
    }
}

pub struct SquarePlacementModifier;

impl SquarePlacementModifier {
    pub fn get_positions(
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> Box<dyn Iterator<Item = BlockPos>> {
        let x = random.next_bounded_i32(16) + pos.0.x;
        let z = random.next_bounded_i32(16) + pos.0.z;
        Box::new(iter::once(BlockPos(Vector3::new(x, pos.0.y, z))))
    }
}

pub struct CountPlacementModifier {
    pub count: IntProvider,
}

impl CountPlacementModifierBase for CountPlacementModifier {
    fn get_count(&self, random: &mut RandomGenerator, _pos: BlockPos) -> i32 {
        self.count.get(random)
    }
}

pub struct SurfaceWaterDepthFilterPlacementModifier {
    pub max_water_depth: i32,
}

impl ConditionalPlacementModifier for SurfaceWaterDepthFilterPlacementModifier {
    fn should_place<T: GenerationCache>(
        &self,
        _block_registry: &dyn WorldPortalExt,
        _feature: pumpkin_data::placed_feature::PlacedFeature,
        chunk: &T,
        _random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        let world_top = chunk.top_block_height_exclusive(pos.0.x, pos.0.z);
        let ocean = chunk.ocean_floor_height_exclusive(pos.0.x, pos.0.z);
        world_top - ocean <= self.max_water_depth
    }
}

pub struct BiomePlacementModifier;

impl ConditionalPlacementModifier for BiomePlacementModifier {
    fn should_place<T: GenerationCache>(
        &self,
        _block_registry: &dyn WorldPortalExt,
        this_feature: pumpkin_data::placed_feature::PlacedFeature,
        chunk: &T,
        _random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        let biome = chunk.get_biome_for_terrain_gen(pos.0.x, pos.0.y, pos.0.z);

        for step in biome.features {
            if step.contains(&this_feature) {
                return true;
            }
        }
        false
    }
}

pub struct HeightRangePlacementModifier {
    pub height: HeightProvider,
}

impl HeightRangePlacementModifier {
    pub fn get_positions(
        &self,
        min_y: i8,
        height: u16,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> Box<dyn Iterator<Item = BlockPos>> {
        let mut pos = pos;
        pos.0.y = self.height.get(random, min_y, height);
        Box::new(iter::once(pos))
    }
}

pub struct HeightmapPlacementModifier {
    pub heightmap: HeightMap,
}

impl HeightmapPlacementModifier {
    pub fn get_positions<T: GenerationCache>(
        &self,
        chunk: &T,
        min_y: i8,
        _height: u16,
        _random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> Box<dyn Iterator<Item = BlockPos>> {
        let x = pos.0.x;
        let z = pos.0.z;
        let top = chunk.get_top_y(&self.heightmap, x, z);
        if top > min_y as i32 {
            return Box::new(iter::once(BlockPos(Vector3::new(x, top, z))));
        }
        Box::new(iter::empty())
    }
}

pub trait CountPlacementModifierBase {
    fn get_positions(
        &self,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> Box<dyn Iterator<Item = BlockPos>> {
        let count = self.get_count(random, pos).max(0); // Vanilla treats negative counts as 0, but `i32 as usize` in Rust would wrap.
        Box::new(std::iter::repeat_n(pos, count as usize))
    }

    fn get_count(&self, random: &mut RandomGenerator, pos: BlockPos) -> i32;
}

pub trait ConditionalPlacementModifier {
    fn get_positions<T: GenerationCache>(
        &self,
        block_registry: &dyn WorldPortalExt,
        chunk: &T,
        feature: pumpkin_data::placed_feature::PlacedFeature,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> Box<dyn Iterator<Item = BlockPos>> {
        if self.should_place(block_registry, feature, chunk, random, pos) {
            Box::new(iter::once(pos))
        } else {
            Box::new(iter::empty())
        }
    }

    fn should_place<T: GenerationCache>(
        &self,
        block_registry: &dyn WorldPortalExt,
        feature: pumpkin_data::placed_feature::PlacedFeature,
        chunk: &T,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool;
}

// generated code is now placed alongside other codegen outputs
// in `src/generated` so it's easier to find when upgrading MC versions.
// the path is relative to this file (up two levels to reach `src`).

/// Returns all placed feature names for tab-completion in `/place feature`.
#[must_use]
pub const fn all_placed_feature_names() -> &'static [&'static str] {
    pumpkin_data::placed_feature::PlacedFeature::all_names()
}
include!("../../../../pumpkin-data/src/generated/placed_features_generated.rs");

#[cfg(test)]
mod tests {
    use pumpkin_data::dimension::Dimension;
    use pumpkin_util::math::int_provider::{NormalIntProvider, UniformIntProvider};
    use pumpkin_util::random::xoroshiro128::Xoroshiro;
    use pumpkin_util::world_seed::Seed;

    use super::*;
    use crate::ProtoChunk;
    use crate::generation::generator::{GeneratorInit, VanillaGenerator, WorldGenerator};

    struct TestPortal;

    impl WorldPortalExt for TestPortal {
        fn can_place_at(
            &self,
            _block: &Block,
            _state: &pumpkin_data::BlockState,
            _block_accessor: &dyn crate::world::BlockAccessor,
            _block_pos: &BlockPos,
        ) -> bool {
            true
        }

        fn mirror(
            &self,
            block: &Block,
            state_id: BlockStateId,
            mirror: pumpkin_data::Mirror,
        ) -> &'static pumpkin_data::BlockState {
            block.mirror(state_id, mirror)
        }

        fn rotate(
            &self,
            block: &Block,
            state_id: BlockStateId,
            rotation: pumpkin_data::Rotation,
        ) -> &'static pumpkin_data::BlockState {
            block.rotate(state_id, rotation)
        }

        fn spawn_mobs_for_chunk_generation(
            &self,
            _cache: &mut dyn GenerationCache,
            _biome: &'static pumpkin_data::chunk::Biome,
            _chunk_x: i32,
            _chunk_z: i32,
        ) {
        }
    }

    const ORIGIN: BlockPos = BlockPos(Vector3::new(0, 64, 0));

    fn empty_chunk() -> ProtoChunk {
        let world_gen = WorldGenerator::Noise(Box::new(VanillaGenerator::new(
            Seed(42),
            Dimension::OVERWORLD,
        )));
        ProtoChunk::new(0, 0, &world_gen)
    }

    fn positions(
        modifier: &PlacementModifier,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> Vec<(i32, i32, i32)> {
        let chunk = empty_chunk();
        modifier
            .get_positions(
                &chunk,
                &TestPortal,
                -64,
                384,
                pumpkin_data::placed_feature::PlacedFeature::WarmOceanVegetation,
                random,
                pos,
            )
            .map(|p| (p.0.x, p.0.y, p.0.z))
            .collect()
    }

    fn uniform(min_inclusive: i32, max_inclusive: i32) -> IntProvider {
        IntProvider::Object(NormalIntProvider::Uniform(UniformIntProvider {
            min_inclusive,
            max_inclusive,
        }))
    }

    fn cuboid(include_edges: bool, include_interior: bool) -> PlacementModifier {
        PlacementModifier::Cuboid(CuboidPlacementModifier {
            xz_size: uniform(3, 5),
            y_size: uniform(3, 5),
            include_edges,
            include_interior,
        })
    }

    // Expected values in these tests come from the vanilla 26.3 classes run on
    // `new XoroshiroRandomSource(seed)` with origin (0, 64, 0).
    #[test]
    fn random_chance_matches_vanilla() {
        let mut random = RandomGenerator::Xoroshiro(Xoroshiro::from_seed(42));
        let modifier =
            PlacementModifier::RandomChance(RandomChancePlacementModifier { chance: 0.2 });
        let placed: Vec<bool> = (0..16)
            .map(|_| !positions(&modifier, &mut random, ORIGIN).is_empty())
            .collect();
        let mut expected = [false; 16];
        expected[12] = true;
        expected[13] = true;
        assert_eq!(placed, expected);
        assert_eq!(random.next_i32(), 1_596_580_675);
    }

    const VANILLA_CUBOID_SHELL: [(i32, i32, i32); 64] = [
        (0, 65, 1),
        (0, 65, 2),
        (0, 65, 3),
        (0, 65, 4),
        (0, 66, 1),
        (0, 66, 2),
        (0, 66, 3),
        (0, 66, 4),
        (0, 67, 1),
        (0, 67, 2),
        (0, 67, 3),
        (0, 67, 4),
        (0, 68, 1),
        (0, 68, 2),
        (0, 68, 3),
        (0, 68, 4),
        (1, 64, 1),
        (1, 64, 2),
        (1, 64, 3),
        (1, 64, 4),
        (1, 65, 0),
        (1, 65, 5),
        (1, 66, 0),
        (1, 66, 5),
        (1, 67, 0),
        (1, 67, 5),
        (1, 68, 0),
        (1, 68, 5),
        (1, 69, 1),
        (1, 69, 2),
        (1, 69, 3),
        (1, 69, 4),
        (2, 64, 1),
        (2, 64, 2),
        (2, 64, 3),
        (2, 64, 4),
        (2, 65, 0),
        (2, 65, 5),
        (2, 66, 0),
        (2, 66, 5),
        (2, 67, 0),
        (2, 67, 5),
        (2, 68, 0),
        (2, 68, 5),
        (2, 69, 1),
        (2, 69, 2),
        (2, 69, 3),
        (2, 69, 4),
        (3, 65, 1),
        (3, 65, 2),
        (3, 65, 3),
        (3, 65, 4),
        (3, 66, 1),
        (3, 66, 2),
        (3, 66, 3),
        (3, 66, 4),
        (3, 67, 1),
        (3, 67, 2),
        (3, 67, 3),
        (3, 67, 4),
        (3, 68, 1),
        (3, 68, 2),
        (3, 68, 3),
        (3, 68, 4),
    ];

    #[test]
    fn cuboid_shell_matches_vanilla() {
        let mut random = RandomGenerator::Xoroshiro(Xoroshiro::from_seed(7));
        let placed = positions(&cuboid(false, false), &mut random, ORIGIN);
        assert_eq!(placed, VANILLA_CUBOID_SHELL);
        assert_eq!(random.next_i32(), -800_923_970);
    }

    #[test]
    fn cuboid_flags_match_vanilla() {
        for (include_edges, include_interior, count, last) in [
            (true, false, 112, (3, 69, 5)),
            (false, true, 96, (3, 68, 4)),
        ] {
            let mut random = RandomGenerator::Xoroshiro(Xoroshiro::from_seed(7));
            let placed = positions(
                &cuboid(include_edges, include_interior),
                &mut random,
                ORIGIN,
            );
            assert_eq!(placed.len(), count);
            assert_eq!(placed.last(), Some(&last));
            assert_eq!(random.next_i32(), -800_923_970);
        }
    }

    #[test]
    fn randomly_selected_matches_vanilla() {
        let offset = |x, z| {
            PlacementModifier::Offset(OffsetPlacementModifier {
                x: IntProvider::Constant(x),
                y: IntProvider::Constant(-1),
                z: IntProvider::Constant(z),
            })
        };
        let modifier = PlacementModifier::RandomlySelected(RandomlySelectedPlacementModifier {
            placements: vec![
                offset(0, 0),
                offset(-1, 0),
                offset(0, 1),
                offset(1, 0),
                offset(0, -1),
            ],
        });
        let mut random = RandomGenerator::Xoroshiro(Xoroshiro::from_seed(1234));
        let placed: Vec<_> = (0..10)
            .flat_map(|_| positions(&modifier, &mut random, ORIGIN))
            .collect();
        assert_eq!(
            placed,
            [
                (0, 63, 0),
                (0, 63, -1),
                (0, 63, -1),
                (0, 63, -1),
                (0, 63, 1),
                (0, 63, 1),
                (0, 63, -1),
                (1, 63, 0),
                (0, 63, 0),
                (-1, 63, 0),
            ]
        );
        assert_eq!(random.next_i32(), 1_790_576_565);
    }

    // Vanilla 26.3 FeaturePlacer with the four modifiers of warm_ocean_vegetation entry 2 and a
    // plain tube coral block; coral/block_decoration is left out because the `random_block`
    // provider is not generated yet.
    const VANILLA_CORAL_MUSHROOM: [(i32, i32, i32); 58] = [
        (5, 64, 6),
        (5, 64, 7),
        (5, 64, 8),
        (5, 65, 6),
        (5, 65, 7),
        (5, 65, 8),
        (5, 66, 6),
        (5, 66, 7),
        (5, 66, 8),
        (5, 67, 7),
        (5, 67, 8),
        (6, 63, 6),
        (6, 63, 7),
        (6, 63, 8),
        (6, 64, 5),
        (6, 64, 9),
        (6, 65, 5),
        (6, 65, 9),
        (6, 66, 5),
        (6, 66, 9),
        (6, 67, 5),
        (6, 68, 6),
        (6, 68, 7),
        (6, 68, 8),
        (7, 63, 6),
        (7, 63, 7),
        (7, 64, 5),
        (7, 64, 9),
        (7, 65, 5),
        (7, 65, 9),
        (7, 66, 9),
        (7, 67, 9),
        (7, 68, 6),
        (7, 68, 7),
        (7, 68, 8),
        (8, 63, 6),
        (8, 63, 7),
        (8, 63, 8),
        (8, 64, 5),
        (8, 64, 9),
        (8, 65, 5),
        (8, 65, 9),
        (8, 66, 5),
        (8, 66, 9),
        (8, 67, 5),
        (8, 67, 9),
        (8, 68, 6),
        (8, 68, 7),
        (9, 64, 6),
        (9, 64, 7),
        (9, 64, 8),
        (9, 65, 7),
        (9, 65, 8),
        (9, 66, 6),
        (9, 66, 8),
        (9, 67, 6),
        (9, 67, 7),
        (9, 67, 8),
    ];

    #[test]
    fn warm_ocean_coral_mushroom_matches_vanilla() {
        let mut chunk = empty_chunk();
        for x in 0..16 {
            for z in 0..16 {
                for y in 50..80 {
                    chunk.set_block_state(x, y, z, Block::WATER.default_state);
                }
            }
        }
        let Some(ConfiguredFeature::SimpleRandomSelector(selector)) = CONFIGURED_FEATURES
            .get(&pumpkin_data::configured_feature::ConfiguredFeature::WarmOceanVegetation)
        else {
            panic!("warm_ocean_vegetation must be a simple random selector");
        };
        let mushroom = &selector.features[2];
        assert!(matches!(
            mushroom.feature,
            Feature::Named(pumpkin_data::configured_feature::ConfiguredFeature::CoralTubeBlock)
        ));

        let placed = std::cell::RefCell::new(Vec::new());
        let mut random = RandomGenerator::Xoroshiro(Xoroshiro::from_seed(3));
        generate_with_modifiers(
            &mut chunk,
            0,
            mushroom.placement.len(),
            &mut random,
            BlockPos::new(5, 64, 5),
            &|chunk, index, random, pos| {
                mushroom.placement[index].get_positions(
                    chunk,
                    &TestPortal,
                    -64,
                    384,
                    pumpkin_data::placed_feature::PlacedFeature::WarmOceanVegetation,
                    random,
                    pos,
                )
            },
            &|chunk, _random, pos| {
                GenerationCache::set_block_state(
                    chunk,
                    &pos.0,
                    Block::TUBE_CORAL_BLOCK.default_state,
                );
                placed.borrow_mut().push((pos.0.x, pos.0.y, pos.0.z));
                true
            },
        );
        assert_eq!(placed.into_inner(), VANILLA_CORAL_MUSHROOM);
        assert_eq!(random.next_i32(), 1_801_432_760);
    }

    #[test]
    fn modifier_candidates_interleave_with_feature_rng() {
        let seed = 0x5EED_CAFEu64;
        let mut expected_random = RandomGenerator::Xoroshiro(Xoroshiro::from_seed(seed));
        let expected = vec![
            (expected_random.next_i32(), expected_random.next_i32()),
            (expected_random.next_i32(), expected_random.next_i32()),
        ];

        let mut random = RandomGenerator::Xoroshiro(Xoroshiro::from_seed(seed));
        let mut generated = Vec::new();
        let result = generate_with_modifiers(
            &mut generated,
            0,
            2,
            &mut random,
            BlockPos::new(0, 0, 0),
            &|_, modifier_index, random, pos| -> Box<dyn Iterator<Item = BlockPos>> {
                match modifier_index {
                    0 => Box::new([pos, pos].into_iter()),
                    1 => Box::new(iter::once(BlockPos::new(random.next_i32(), 0, 0))),
                    _ => unreachable!(),
                }
            },
            &|generated, random, pos| {
                generated.push((pos.0.x, random.next_i32()));
                true
            },
        );

        assert!(result);
        assert_eq!(generated, expected);
    }
}
