use pumpkin_util::{math::position::BlockPos, random::RandomGenerator};

use crate::generation::feature::placed_features::PlacedFeatureWrapper;
use crate::{generation::proto_chunk::GenerationCache, world::WorldPortalExt};

pub mod coral_claw;
pub mod coral_tree;

/// Places the `feature` of a coral feature at one position, as vanilla
/// `this.feature.value().place(...)` does.
#[expect(clippy::too_many_arguments)]
fn place_piece<T: GenerationCache>(
    feature: &PlacedFeatureWrapper,
    chunk: &mut T,
    block_registry: &dyn WorldPortalExt,
    min_y: i8,
    height: u16,
    feature_name: pumpkin_data::placed_feature::PlacedFeature,
    random: &mut RandomGenerator,
    pos: BlockPos,
) -> bool {
    feature.get().is_some_and(|feature| {
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
}
