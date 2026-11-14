//! Block collisions as vanilla `BlockCollisions` tests them: the block scan, and the blocks whose
//! shape leaves their cell.

use pumpkin_data::{Block, BlockState};
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;

use super::World;

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
    /// Whether `bounding_box` touches no block collision shape (`noBlockCollision`).
    ///
    /// The scan is the one of `BlockCollisions`: one cell more on every side, where only a
    /// block whose shape leaves its cell can reach the box.
    #[must_use]
    pub fn no_block_collision(&self, bounding_box: BoundingBox) -> bool {
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
                    if Self::check_collision(&bounding_box, pos, state, false, |_| ()) {
                        return false;
                    }
                }
            }
        }
        true
    }
}
