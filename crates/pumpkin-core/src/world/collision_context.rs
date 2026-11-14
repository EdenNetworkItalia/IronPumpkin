//! Vanilla `CollisionContext` for block collisions: the blocks whose collision shape depends on
//! the entity, and the block scan of `BlockCollisions`.

use pumpkin_data::block_properties::ScaffoldingLikeProperties;
use pumpkin_data::{Block, BlockState};
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;

use super::World;
use crate::block::blocks::powder_snow;
use crate::entity::EntityBase;

/// `ScaffoldingBlock.SHAPE_UNSTABLE_BOTTOM`: `Block.column(16, 0, 2)`.
const SCAFFOLDING_UNSTABLE_BOTTOM: BoundingBox =
    BoundingBox::new_array([0.0, 0.0, 0.0], [1.0, 0.125, 1.0]);

/// The collision shape of a block for the context of an entity, where it differs from the
/// shape of the state. Pumpkin extracts the shapes with the empty context.
#[derive(Clone, Copy, Debug)]
pub enum ContextShape {
    Empty,
    Box(BoundingBox),
}

/// The collision shape of `state` at `pos` for the context of `entity`, where it differs from
/// the shape of the state.
///
/// This is `getCollisionShape` with `CollisionContext.of(entity)`, block-local; `None` for
/// `entity` is the empty context. Powder snow and scaffolding depend on the context. Liquids
/// for striders and the light block outline do not, unlike vanilla.
#[must_use]
pub fn context_collision_shape(
    state: &BlockState,
    pos: &BlockPos,
    entity: Option<&dyn EntityBase>,
) -> Option<ContextShape> {
    let entity = entity?;
    if state.id == Block::POWDER_SNOW.default_state.id {
        return Some(
            powder_snow::collision_shape_for_entity(entity, pos)
                .map_or(ContextShape::Empty, ContextShape::Box),
        );
    }
    let block = Block::from_state_id(state.id);
    if block != &Block::SCAFFOLDING {
        return None;
    }
    // `ScaffoldingBlock.getCollisionShape`. `isAbove` compares the feet of the entity with the
    // top of the shape, minus `1.0E-5F`.
    let base = entity.get_entity();
    let feet = base.pos.load().y;
    let bottom = f64::from(pos.0.y);
    if feet > bottom + 1.0 - f64::from(1.0E-5f32) && !base.is_sneaking() {
        return None;
    }
    let properties = ScaffoldingLikeProperties::from_state_id(state.id);
    if properties.distance != 0 && properties.bottom && feet > bottom - f64::from(1.0E-5f32) {
        return Some(ContextShape::Box(SCAFFOLDING_UNSTABLE_BOTTOM));
    }
    Some(ContextShape::Empty)
}

/// `BlockState.hasLargeCollisionShape`: the collision shape leaves the block, or the block has
/// no shape cache (`dynamicShape`), so `BlockCollisions` also tests it in the outer ring of
/// cells.
#[must_use]
pub fn has_large_collision_shape(state: &BlockState) -> bool {
    let block = Block::from_state_id(state.id);
    [
        &Block::MOVING_PISTON,
        &Block::BAMBOO,
        &Block::SCAFFOLDING,
        &Block::POWDER_SNOW,
        &Block::POINTED_DRIPSTONE,
        &Block::SULFUR_SPIKE,
        &Block::FIREFLY_BUSH,
    ]
    .contains(&block)
        || state.get_block_collision_shapes().any(|shape| {
            shape.min.x < 0.0
                || shape.min.y < 0.0
                || shape.min.z < 0.0
                || shape.max.x > 1.0
                || shape.max.y > 1.0
                || shape.max.z > 1.0
        })
}

impl World {
    /// Whether `bounding_box` touches no block collision shape for the context of `entity`
    /// (`CollisionGetter#noBlockCollision(entity, box)`); `None` is the empty context.
    ///
    /// The scan is the one of `BlockCollisions`: one cell more on every side, where only a
    /// block whose shape leaves its cell can reach the box.
    #[must_use]
    pub fn no_block_collision(
        &self,
        entity: Option<&dyn EntityBase>,
        bounding_box: BoundingBox,
    ) -> bool {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "block coordinates fit in i32, as Mth.floor"
        )]
        let range = |min: f64, max: f64| {
            (
                (min - 1.0E-7).floor() as i32 - 1,
                (max + 1.0E-7).floor() as i32 + 1,
            )
        };
        let (x0, x1) = range(bounding_box.min.x, bounding_box.max.x);
        let (y0, y1) = range(bounding_box.min.y, bounding_box.max.y);
        let (z0, z1) = range(bounding_box.min.z, bounding_box.max.z);
        for y in y0..=y1 {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let edges = u8::from(x == x0 || x == x1)
                        + u8::from(y == y0 || y == y1)
                        + u8::from(z == z0 || z == z1);
                    // Two edges: only a moving piston, whose shape Pumpkin keeps empty.
                    if edges >= 2 {
                        continue;
                    }
                    let pos = BlockPos::new(x, y, z);
                    let state = self.get_block_state(&pos);
                    if edges == 1 && !has_large_collision_shape(state) {
                        continue;
                    }
                    if Self::check_collision(&bounding_box, pos, state, entity, false, |_| ()) {
                        return false;
                    }
                }
            }
        }
        true
    }
}
