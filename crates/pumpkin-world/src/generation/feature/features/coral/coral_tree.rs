use crate::generation::feature::placed_features::PlacedFeatureWrapper;
use crate::{generation::proto_chunk::GenerationCache, world::WorldPortalExt};
use pumpkin_data::BlockDirection;
use pumpkin_util::{
    math::position::BlockPos,
    random::{RandomGenerator, RandomImpl},
};

use super::place_piece;

/// Vanilla `CoralTreeFeature`: a trunk and branches of `feature` pieces.
pub struct CoralTreeFeature {
    pub feature: Box<PlacedFeatureWrapper>,
}

impl CoralTreeFeature {
    #[expect(clippy::too_many_arguments)]
    pub fn generate<T: GenerationCache>(
        &self,
        chunk: &mut T,
        block_registry: &dyn WorldPortalExt,
        min_y: i8,
        height: u16,
        feature_name: pumpkin_data::placed_feature::PlacedFeature,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        let place = |chunk: &mut T, random: &mut RandomGenerator, pos| {
            place_piece(
                &self.feature,
                chunk,
                block_registry,
                min_y,
                height,
                feature_name,
                random,
                pos,
            )
        };
        let mut cur_pos = pos;
        let trunk_height = random.next_bounded_i32(3) + 1;
        for _ in 0..trunk_height {
            if !place(chunk, random, cur_pos) {
                return true;
            }
            cur_pos = cur_pos.up();
        }

        let trunk_top_pos = cur_pos;
        let n_branches = random.next_bounded_i32(3) + 2;

        let mut directions: Vec<BlockDirection> = BlockDirection::horizontal_worldgen()
            .iter()
            .copied()
            .map(BlockDirection::from_cardinal_direction)
            .collect();
        for i in (1..directions.len()).rev() {
            let j = random.next_bounded_i32(i as i32 + 1) as usize;
            directions.swap(i, j);
        }

        for &branch_direction in directions.iter().take(n_branches as usize) {
            let mut branch_pos = trunk_top_pos.offset(branch_direction.to_offset());
            let branch_height = random.next_bounded_i32(5) + 2;
            let mut segment_length = 0;

            for j in 0..branch_height {
                if !place(chunk, random, branch_pos) {
                    break;
                }
                segment_length += 1;
                branch_pos = branch_pos.up();
                if j == 0 || (segment_length >= 2 && random.next_f32() < 0.25) {
                    branch_pos = branch_pos.offset(branch_direction.to_offset());
                    segment_length = 0;
                }
            }
        }
        true
    }
}
