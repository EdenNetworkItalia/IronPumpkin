use pumpkin_util::{math::position::BlockPos, random::RandomGenerator};

use crate::{
    ProtoChunk,
    generation::{
        feature::placed_features::{PLACED_FEATURES, PlacedFeature, PlacedFeatureWrapper},
        proto_chunk::GenerationCache,
    },
    world::WorldPortalExt,
};

/// Vanilla `SequenceFeature`: places the features in order and stops at the first one that
/// places nothing.
pub struct SequenceFeature {
    pub features: Vec<PlacedFeatureWrapper>,
}

impl SequenceFeature {
    #[expect(clippy::too_many_arguments)]
    pub fn generate<T: GenerationCache>(
        &self,
        chunk: &mut T,
        block_registry: &dyn WorldPortalExt,
        min_y: i8,
        height: u16,
        feature_name: pumpkin_data::placed_feature::PlacedFeature,
        random: &mut RandomGenerator,
        origin: BlockPos,
    ) -> bool {
        self.features.iter().all(|wrapper| {
            resolve(wrapper, feature_name).is_some_and(|(name, placed_feature)| {
                placed_feature.generate(chunk, block_registry, min_y, height, name, random, origin)
            })
        })
    }

    /// Entry point for features of jigsaw pool elements, which only have the proto chunk.
    pub fn generate_in_proto_chunk(
        &self,
        chunk: &mut ProtoChunk,
        block_registry: &dyn WorldPortalExt,
        feature_name: pumpkin_data::placed_feature::PlacedFeature,
        random: &mut RandomGenerator,
        origin: BlockPos,
    ) -> bool {
        self.features.iter().all(|wrapper| {
            resolve(wrapper, feature_name).is_some_and(|(name, placed_feature)| {
                placed_feature.generate_in_proto_chunk(chunk, block_registry, name, random, origin)
            })
        })
    }
}

fn resolve(
    wrapper: &PlacedFeatureWrapper,
    feature_name: pumpkin_data::placed_feature::PlacedFeature,
) -> Option<(pumpkin_data::placed_feature::PlacedFeature, &PlacedFeature)> {
    match wrapper {
        PlacedFeatureWrapper::Named(name) => PLACED_FEATURES.get(name).map(|f| (*name, f)),
        PlacedFeatureWrapper::Direct(feature) => Some((feature_name, feature)),
    }
}
