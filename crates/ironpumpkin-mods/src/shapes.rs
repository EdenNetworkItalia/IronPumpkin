//! Block shapes and collision tests, the `NeoForge` `VoxelShape`, `Shapes`, `CollisionContext`
//! and the shape getters of `BlockState`.
//!
//! A [`VoxelShape`] is a union of boxes in block-local coordinates, `0..1` for one block. The
//! helpers build the shapes of `Shapes` and `Block`: [`VoxelShape::pixels`] is `Block.box`,
//! [`VoxelShape::column`] is `Block.column`, [`VoxelShape::join`] is `Shapes.join` and
//! [`VoxelShape::rotate_horizontal`] is `Shapes.rotateHorizontal`.
//!
//! Reads: [`get_state_shape`] gives a shape of a state on its own, as vanilla reads it at
//! `BlockPos.ZERO`, and [`get_block_shape`] the shape of the block at a position, with the
//! position offset of plants and the collision context of an entity. Pumpkin extracts the outline
//! and the collision shape of every vanilla state; the interaction, block support, occlusion and
//! visual shapes are the vanilla defaults (see [`ShapeKind`]), so the few vanilla blocks that
//! override those getters answer with the default.
//!
//! Tests: [`is_face_sturdy`] is `BlockState#isFaceSturdy` and [`no_collision`] is
//! `CollisionGetter#noCollision`.
//!
//! A mod block declares its shapes per state with
//! [`BlockBuilder::shape`](crate::content::BlockBuilder::shape) and
//! [`BlockBuilder::collision_shape`](crate::content::BlockBuilder::collision_shape). The server
//! evaluates them once per state when the registry freezes, and uses them in entity collision,
//! ray traces, support checks and the reads of this module. Shapes depend on the state only: a
//! shape that changes with the position or the entity (vanilla `dynamicShape`, scaffolding) has
//! no equivalent.
//!
//! ```
//! use ironpumpkin_mods::{
//!     pumpkin_data::Block,
//!     shapes::{ShapeKind, get_state_shape},
//! };
//!
//! let slab = get_state_shape(Block::STONE_SLAB.default_state, ShapeKind::Collision);
//! assert_eq!(slab.bounds().map(|b| b.max.y), Some(0.5));
//! ```

use pumpkin_core::{block::blocks::powder_snow, entity::EntityBase, world::World};
use pumpkin_data::{Block, BlockDirection, BlockState};
use pumpkin_util::math::{boundingbox::BoundingBox, position::BlockPos};

pub use pumpkin_data::dynamic_shapes::StateProperties;
pub use pumpkin_data::voxel_shape::{BooleanOp, ShapeKind, SupportType, VoxelShape};

/// The shape `kind` of `state` on its own (`BlockState#getShape` and the other getters with an
/// empty level, `BlockPos.ZERO` and the empty collision context).
#[must_use]
pub fn get_state_shape(state: &BlockState, kind: ShapeKind) -> VoxelShape {
    state.shape(kind)
}

/// The shape `kind` of the block at `pos`, block-local (`BlockState#getShape(level, pos,
/// context)` and the other getters).
///
/// The shape takes the position offset of the block, as for bamboo and flowers. `entity` is the
/// collision context (`CollisionContext.of(entity)`), and `None` the empty context. The context
/// changes only the collision shape of powder snow, as in Pumpkin entity collision.
#[must_use]
pub fn get_block_shape(
    world: &World,
    pos: &BlockPos,
    kind: ShapeKind,
    entity: Option<&dyn EntityBase>,
) -> VoxelShape {
    let state = world.get_block_state(pos);
    if kind == ShapeKind::Collision
        && let Some(entity) = entity
        && state.id.to_block() == &Block::POWDER_SNOW
    {
        return powder_snow::collision_shape_for_entity(entity, pos)
            .map_or_else(VoxelShape::empty, VoxelShape::from);
    }
    state.shape_at(kind, pos)
}

/// Whether the face `face` of the block at `pos` holds a block that needs `support`
/// (`BlockState#isFaceSturdy`).
#[must_use]
pub fn is_face_sturdy(
    world: &World,
    pos: &BlockPos,
    face: BlockDirection,
    support: SupportType,
) -> bool {
    world.get_block_state(pos).is_face_sturdy(face, support)
}

/// Whether `bounding_box` is free for `entity` (`CollisionGetter#noCollision(Entity, AABB)`).
///
/// The box is in world coordinates. It must touch no block collision shape, no entity that
/// collides like a boat or a shulker, and no world border.
#[must_use]
pub fn no_collision(world: &World, entity: &dyn EntityBase, bounding_box: BoundingBox) -> bool {
    world.no_collision(entity, bounding_box)
}
