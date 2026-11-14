//! Block shapes as unions of boxes, and the shape reads of block states.
//!
//! A [`VoxelShape`] is the set of boxes that Pumpkin collision, ray traces and support checks
//! read, in block-local coordinates (one block is `0..1` on each axis). Vanilla stores a shape as
//! a discrete voxel grid; the boolean operations here give the same set of points.
//!
//! Pumpkin extracts two shapes per state: the outline (`getShape`) and the collision shape
//! (`getCollisionShape`). The other getters use the vanilla defaults of `BlockBehaviour`: the
//! interaction shape is empty, the block support and visual shapes are the collision shape, and
//! the occlusion shape is the outline when the state can occlude. The vanilla blocks that override
//! these getters answer with the default (see [`ShapeKind`]).

use pumpkin_util::math::{boundingbox::BoundingBox, position::BlockPos, vector3::Vector3};

use crate::block_properties::COLLISION_SHAPES;
use crate::{Block, BlockDirection, BlockState};

/// Vanilla `Shapes.EPSILON`: a box thinner than this on an axis is empty.
const EPSILON: f64 = 1.0e-7;

/// Vanilla `VoxelShape.calculateFace` samples the face cell this far inside the block.
const FACE_SAMPLE: f64 = 0.999_999_9;

/// A block shape: a union of boxes in block-local coordinates. The boxes may overlap.
#[derive(Clone, Debug, Default)]
pub struct VoxelShape {
    boxes: Vec<BoundingBox>,
}

/// The boolean operations of vanilla `BooleanOp` that block shapes use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanOp {
    /// The points of either shape (`BooleanOp.OR`).
    Or,
    /// The points of both shapes (`BooleanOp.AND`).
    And,
    /// The points of the first shape that are not in the second (`BooleanOp.ONLY_FIRST`).
    OnlyFirst,
    /// The points of the second shape that are not in the first (`BooleanOp.ONLY_SECOND`).
    OnlySecond,
    /// The points of exactly one shape (`BooleanOp.NOT_SAME`).
    NotSame,
}

/// The shape getters of vanilla `BlockBehaviour`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    /// `getShape`: the outline that a player targets and that the selection box shows.
    Outline,
    /// `getCollisionShape`: the shape that stops entities.
    Collision,
    /// `getInteractionShape`: empty for every block except the vanilla cauldrons, composter,
    /// hopper and scaffolding, which Pumpkin answers with the default too.
    Interaction,
    /// `getBlockSupportShape`: the collision shape, or the shape of the vanilla override for
    /// the leaves (empty), the snow layer, the fence gates, the shulker boxes (full, as closed),
    /// soul sand and mud (full), the chorus flower and the hanging signs (the outline).
    BlockSupport,
    /// `getOcclusionShape` as the state caches it: the outline when the state can occlude, else
    /// empty. The vanilla fences, fence gates, lectern and sculk shrieker override it; Pumpkin
    /// answers with the default.
    Occlusion,
    /// `getVisualShape`: the collision shape. The vanilla fences, iron bars, mud, powder snow,
    /// snow layer, soul sand and transparent blocks override it; Pumpkin answers with the
    /// default.
    Visual,
}

/// Vanilla `SupportType`: how much of a face must be solid to hold a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportType {
    /// The whole face.
    Full,
    /// The center of the face, as a torch or a lantern needs.
    Center,
    /// The face without its middle, as a rail or a bell needs.
    Rigid,
}

impl VoxelShape {
    /// The shape with no box (`Shapes.empty()`).
    #[must_use]
    pub const fn empty() -> Self {
        Self { boxes: Vec::new() }
    }

    /// The full block (`Shapes.block()`).
    #[must_use]
    pub fn block() -> Self {
        Self {
            boxes: vec![BoundingBox::full_block()],
        }
    }

    /// One box in block units, `0..1` for one block (`Shapes.box`). A box thinner than vanilla
    /// `Shapes.EPSILON` on an axis gives the empty shape.
    ///
    /// # Panics
    ///
    /// When a min value is greater than its max value, as vanilla `Shapes.box` throws an
    /// `IllegalArgumentException`.
    #[must_use]
    pub fn cuboid(min_x: f64, min_y: f64, min_z: f64, max_x: f64, max_y: f64, max_z: f64) -> Self {
        assert!(
            min_x <= max_x && min_y <= max_y && min_z <= max_z,
            "The min values need to be smaller or equals to the max values"
        );
        Self::from_boxes([BoundingBox::new(
            Vector3::new(min_x, min_y, min_z),
            Vector3::new(max_x, max_y, max_z),
        )])
    }

    /// One box in pixels, `0..16` for one block (`Block.box`).
    #[must_use]
    pub fn pixels(min_x: f64, min_y: f64, min_z: f64, max_x: f64, max_y: f64, max_z: f64) -> Self {
        Self::cuboid(
            min_x / 16.0,
            min_y / 16.0,
            min_z / 16.0,
            max_x / 16.0,
            max_y / 16.0,
            max_z / 16.0,
        )
    }

    /// A box centered on the block on x and z, in pixels (`Block.column`).
    #[must_use]
    pub fn column(size_xz: f64, min_y: f64, max_y: f64) -> Self {
        let half = size_xz / 2.0;
        Self::pixels(8.0 - half, min_y, 8.0 - half, 8.0 + half, max_y, 8.0 + half)
    }

    /// The union of `boxes`. Boxes thinner than vanilla `Shapes.EPSILON` on an axis are dropped.
    #[must_use]
    pub fn from_boxes(boxes: impl IntoIterator<Item = BoundingBox>) -> Self {
        Self {
            boxes: boxes.into_iter().filter(|b| !is_thin(b)).collect(),
        }
    }

    /// The boxes of the shape, in block-local coordinates.
    #[must_use]
    pub fn boxes(&self) -> &[BoundingBox] {
        &self.boxes
    }

    /// Whether the shape has no point (`VoxelShape.isEmpty`).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }

    /// The smallest box around the shape (`VoxelShape.bounds`), or `None` for the empty shape.
    #[must_use]
    pub fn bounds(&self) -> Option<BoundingBox> {
        self.boxes.iter().copied().reduce(|a, b| a.minmax(&b))
    }

    /// The union of both shapes (`Shapes.or`).
    #[must_use]
    pub fn or(mut self, other: Self) -> Self {
        self.boxes.extend(other.boxes);
        self
    }

    /// The shape that `op` makes of `self` and `other` (`Shapes.join`).
    #[must_use]
    pub fn join(&self, other: &Self, op: BooleanOp) -> Self {
        match op {
            BooleanOp::Or => self.clone().or(other.clone()),
            BooleanOp::And => Self::from_boxes(self.boxes.iter().flat_map(|a| {
                other.boxes.iter().map(move |b| {
                    BoundingBox::new(
                        Vector3::new(
                            a.min.x.max(b.min.x),
                            a.min.y.max(b.min.y),
                            a.min.z.max(b.min.z),
                        ),
                        Vector3::new(
                            a.max.x.min(b.max.x),
                            a.max.y.min(b.max.y),
                            a.max.z.min(b.max.z),
                        ),
                    )
                })
            })),
            BooleanOp::OnlyFirst => self.subtract(other),
            BooleanOp::OnlySecond => other.subtract(self),
            BooleanOp::NotSame => self.subtract(other).or(other.subtract(self)),
        }
    }

    /// Whether `op` of the two shapes has a point (`Shapes.joinIsNotEmpty`).
    #[must_use]
    pub fn join_is_not_empty(&self, other: &Self, op: BooleanOp) -> bool {
        !self.join(other, op).is_empty()
    }

    /// The shape moved by `dx`, `dy` and `dz` blocks (`VoxelShape.move`).
    #[must_use]
    pub fn move_by(&self, dx: f64, dy: f64, dz: f64) -> Self {
        let delta = Vector3::new(dx, dy, dz);
        Self {
            boxes: self.boxes.iter().map(|b| b.shift(delta)).collect(),
        }
    }

    /// The shape of a block that faces `facing`, from its shape when it faces north, turned
    /// around the block center (`Shapes.rotateHorizontal`). `Up` and `Down` keep the shape.
    #[must_use]
    pub fn rotate_horizontal(&self, facing: BlockDirection) -> Self {
        let turn: fn(f64, f64) -> (f64, f64) = match facing {
            BlockDirection::East => |x, z| (1.0 - z, x),
            BlockDirection::South => |x, z| (1.0 - x, 1.0 - z),
            BlockDirection::West => |x, z| (z, 1.0 - x),
            BlockDirection::North | BlockDirection::Up | BlockDirection::Down => {
                return self.clone();
            }
        };
        Self {
            boxes: self
                .boxes
                .iter()
                .map(|b| {
                    let (ax, az) = turn(b.min.x, b.min.z);
                    let (bx, bz) = turn(b.max.x, b.max.z);
                    BoundingBox::new(
                        Vector3::new(ax.min(bx), b.min.y, az.min(bz)),
                        Vector3::new(ax.max(bx), b.max.y, az.max(bz)),
                    )
                })
                .collect(),
        }
    }

    /// The part of the shape that touches the face `direction` of the block, stretched over the
    /// whole block along the face normal (`VoxelShape.getFaceShape`).
    #[must_use]
    pub fn face_shape(&self, direction: BlockDirection) -> Self {
        let axis = axis_index(direction);
        let sample = if is_positive(direction) {
            FACE_SAMPLE
        } else {
            EPSILON
        };
        Self {
            boxes: self
                .boxes
                .iter()
                .filter(|b| component(&b.min, axis) <= sample && sample < component(&b.max, axis))
                .map(|b| {
                    let mut face = *b;
                    set_component(&mut face.min, axis, 0.0);
                    set_component(&mut face.max, axis, 1.0);
                    face
                })
                .collect(),
        }
    }

    /// Whether the shape is the full block and nothing more (`Block.isShapeFullBlock`).
    #[must_use]
    pub fn is_full_block(&self) -> bool {
        !Self::block().join_is_not_empty(self, BooleanOp::NotSame)
    }

    /// Whether the face `direction` of the shape covers the whole block face (`Block.isFaceFull`).
    #[must_use]
    pub fn is_face_full(&self, direction: BlockDirection) -> bool {
        self.face_shape(direction).is_full_block()
    }

    /// Whether the face `direction` of this support shape holds a block that needs `support`
    /// (`SupportType.isSupporting`).
    #[must_use]
    pub fn is_supporting(&self, direction: BlockDirection, support: SupportType) -> bool {
        match support {
            SupportType::Full => self.is_face_full(direction),
            // `CENTER_SUPPORT_SHAPE` is `Block.column(2, 0, 10)`.
            SupportType::Center => !self
                .face_shape(direction)
                .join_is_not_empty(&Self::column(2.0, 0.0, 10.0), BooleanOp::OnlySecond),
            // `RIGID_SUPPORT_SHAPE` is the block without `Block.column(12, 0, 16)`.
            SupportType::Rigid => !self.face_shape(direction).join_is_not_empty(
                &Self::block().join(&Self::column(12.0, 0.0, 16.0), BooleanOp::OnlyFirst),
                BooleanOp::OnlySecond,
            ),
        }
    }

    /// Vanilla `BlockStateBase.calculateSolid` for a collision shape, without the
    /// `forceSolidOn` and `forceSolidOff` properties.
    #[must_use]
    pub fn is_legacy_solid(&self) -> bool {
        self.bounds().is_some_and(|b| {
            let size = b.max.sub(&b.min);
            (size.x + size.y + size.z) / 3.0 >= 0.729_166_666_666_666_6 || size.y >= 1.0
        })
    }

    fn subtract(&self, other: &Self) -> Self {
        let mut pieces = self.boxes.clone();
        for cut in &other.boxes {
            pieces = pieces
                .iter()
                .flat_map(|piece| piece.subtract(cut))
                .filter(|piece| !is_thin(piece))
                .collect();
        }
        Self { boxes: pieces }
    }
}

impl From<BoundingBox> for VoxelShape {
    fn from(value: BoundingBox) -> Self {
        Self::from_boxes([value])
    }
}

fn is_thin(b: &BoundingBox) -> bool {
    b.max.x - b.min.x < EPSILON || b.max.y - b.min.y < EPSILON || b.max.z - b.min.z < EPSILON
}

const fn axis_index(direction: BlockDirection) -> usize {
    match direction {
        BlockDirection::West | BlockDirection::East => 0,
        BlockDirection::Down | BlockDirection::Up => 1,
        BlockDirection::North | BlockDirection::South => 2,
    }
}

const fn is_positive(direction: BlockDirection) -> bool {
    matches!(
        direction,
        BlockDirection::East | BlockDirection::Up | BlockDirection::South
    )
}

const fn component(v: &Vector3<f64>, axis: usize) -> f64 {
    match axis {
        0 => v.x,
        1 => v.y,
        _ => v.z,
    }
}

const fn set_component(v: &mut Vector3<f64>, axis: usize, value: f64) {
    match axis {
        0 => v.x = value,
        1 => v.y = value,
        _ => v.z = value,
    }
}

/// The box with index `id` of the shape table: a generated box, or a box of a mod block.
#[inline(always)]
#[must_use]
pub fn shape_box(id: u16) -> BoundingBox {
    // Both arms yield a reference, so the vanilla arm stays one table load.
    *COLLISION_SHAPES
        .get(id as usize)
        .unwrap_or_else(|| crate::dynamic::custom_shape_box(id))
}

fn shape_of(ids: &[u16]) -> VoxelShape {
    VoxelShape::from_boxes(ids.iter().map(|&id| shape_box(id)))
}

/// The shape of the vanilla `getBlockSupportShape` overrides, or `None` for the default.
fn vanilla_support_shape(state: &BlockState) -> Option<VoxelShape> {
    use crate::block_properties::{
        HorizontalFacing, OakFenceGateLikeProperties, SnowLikeProperties,
    };
    use crate::tag::{self, Taggable};

    let block = Block::from_state_id(state.id);
    if block.has_tag(&tag::Block::MINECRAFT_LEAVES) {
        return Some(VoxelShape::empty());
    }
    if block.has_tag(&tag::Block::MINECRAFT_ALL_HANGING_SIGNS) {
        return Some(shape_of(state.outline_shapes));
    }
    if block.has_tag(&tag::Block::MINECRAFT_FENCE_GATES) {
        let gate = OakFenceGateLikeProperties::from_state_id(state.id);
        if gate.open {
            return Some(VoxelShape::empty());
        }
        // `Block.column(16, 4, 5, 24)` for a gate along the x axis, rotated for the z axis.
        return Some(match gate.facing {
            HorizontalFacing::North | HorizontalFacing::South => {
                VoxelShape::pixels(0.0, 5.0, 6.0, 16.0, 24.0, 10.0)
            }
            HorizontalFacing::West | HorizontalFacing::East => {
                VoxelShape::pixels(6.0, 5.0, 0.0, 10.0, 24.0, 16.0)
            }
        });
    }
    if block.has_tag(&tag::Block::MINECRAFT_SHULKER_BOXES)
        || block == &Block::SOUL_SAND
        || block == &Block::MUD
    {
        return Some(VoxelShape::block());
    }
    if block == &Block::SNOW {
        let layers = SnowLikeProperties::from_state_id(state.id).layers;
        return Some(VoxelShape::column(16.0, 0.0, f64::from(layers) * 2.0));
    }
    if block == &Block::CHORUS_FLOWER {
        return Some(VoxelShape::column(14.0, 0.0, 15.0));
    }
    None
}

impl BlockState {
    /// The shape `kind` of this state, block-local, as vanilla reads it at `BlockPos.ZERO` in an
    /// empty level with the empty collision context. See [`ShapeKind`] for the getters that
    /// answer with the vanilla default.
    #[must_use]
    pub fn shape(&self, kind: ShapeKind) -> VoxelShape {
        match kind {
            ShapeKind::Outline => shape_of(self.outline_shapes),
            ShapeKind::BlockSupport => {
                vanilla_support_shape(self).unwrap_or_else(|| shape_of(self.collision_shapes))
            }
            ShapeKind::Collision | ShapeKind::Visual => shape_of(self.collision_shapes),
            ShapeKind::Interaction => VoxelShape::empty(),
            ShapeKind::Occlusion if self.can_occlude() => shape_of(self.outline_shapes),
            ShapeKind::Occlusion => VoxelShape::empty(),
        }
    }

    /// The shape `kind` of this state at `pos`, block-local, with the position offset of the
    /// block (bamboo, flowers), as vanilla `getShape(level, pos)` returns it.
    #[must_use]
    pub fn shape_at(&self, kind: ShapeKind, pos: &BlockPos) -> VoxelShape {
        let offset = Block::from_state_id(self.id).shape_offset_delta(pos);
        self.shape(kind).move_by(offset.x, offset.y, offset.z)
    }

    /// Vanilla `BlockState.isFaceSturdy` for `support`. The full face and the center of the top
    /// and bottom faces read the extracted flags; the center of a side face and the rigid
    /// support read the block support shape, with the vanilla overrides.
    #[must_use]
    pub fn is_face_sturdy(&self, direction: BlockDirection, support: SupportType) -> bool {
        match (support, direction) {
            (SupportType::Full, _) => self.is_side_solid(direction),
            (SupportType::Center, BlockDirection::Up | BlockDirection::Down) => {
                self.is_center_solid(direction)
            }
            _ => self
                .shape(ShapeKind::BlockSupport)
                .is_supporting(direction, support),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corners(shape: &VoxelShape) -> Vec<[f64; 6]> {
        let mut boxes: Vec<[f64; 6]> = shape
            .boxes()
            .iter()
            .map(|b| [b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z])
            .collect();
        boxes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        boxes
    }

    /// The volume of a union of boxes that do not overlap.
    fn volume(shape: &VoxelShape) -> f64 {
        shape
            .boxes()
            .iter()
            .map(|b| (b.max.x - b.min.x) * (b.max.y - b.min.y) * (b.max.z - b.min.z))
            .sum()
    }

    #[test]
    fn vanilla_shape_reads_match_vanilla_26_3() {
        // `SlabBlock.SHAPE_BOTTOM` is `Block.column(16, 0, 8)`.
        let slab = Block::STONE_SLAB.default_state;
        assert_eq!(
            corners(&slab.shape(ShapeKind::Collision)),
            [[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]
        );
        assert_eq!(
            corners(&slab.shape(ShapeKind::Outline)),
            [[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]
        );
        // `CarpetBlock.SHAPE` is `Block.column(16, 0, 1)`.
        assert_eq!(
            corners(
                &Block::WHITE_CARPET
                    .default_state
                    .shape(ShapeKind::Collision)
            ),
            [[0.0, 0.0, 0.0, 1.0, 0.0625, 1.0]]
        );
        // `BaseTorchBlock.SHAPE` is `Block.column(4, 0, 10)` and the torch has no collision.
        let torch = Block::TORCH.default_state;
        assert_eq!(
            corners(&torch.shape(ShapeKind::Outline)),
            [[0.375, 0.0, 0.375, 0.625, 0.625, 0.625]]
        );
        assert!(torch.shape(ShapeKind::Collision).is_empty());
        // A fence post collides up to 1.5 (`CrossCollisionBlock` with `collisionHeight` 24).
        let fence = Block::OAK_FENCE.default_state.shape(ShapeKind::Collision);
        assert_eq!(fence.bounds().map(|b| b.max.y), Some(1.5));
        // The default getters.
        let stone = Block::STONE.default_state;
        assert!(stone.shape(ShapeKind::Interaction).is_empty());
        assert!(stone.shape(ShapeKind::Occlusion).is_full_block());
        assert!(
            Block::GLASS
                .default_state
                .shape(ShapeKind::Occlusion)
                .is_empty()
        );
    }

    #[test]
    fn support_types_follow_the_vanilla_support_shapes() {
        let stone = Block::STONE.default_state;
        for support in [SupportType::Full, SupportType::Center, SupportType::Rigid] {
            assert!(stone.is_face_sturdy(BlockDirection::North, support));
        }
        // A bottom slab holds a torch and a rail on its bottom face only.
        let slab = Block::STONE_SLAB.default_state;
        assert!(slab.is_face_sturdy(BlockDirection::Down, SupportType::Rigid));
        assert!(!slab.is_face_sturdy(BlockDirection::Up, SupportType::Rigid));
        // The side face of a bottom slab covers the center column only up to 8 of 10 pixels.
        assert!(!slab.is_face_sturdy(BlockDirection::North, SupportType::Center));
        // A wall post holds a torch on its top, not a rail.
        let wall = Block::COBBLESTONE_WALL.default_state;
        assert!(wall.is_face_sturdy(BlockDirection::Up, SupportType::Center));
        assert!(!wall.is_face_sturdy(BlockDirection::Up, SupportType::Rigid));
        // With the vanilla overrides of `getBlockSupportShape`, the computed center agrees
        // with the extracted flag on every top and bottom face.
        let mut differ: Vec<&str> = (0..crate::BlockStateId::STATE_COUNT)
            .map(|raw| crate::BlockStateId::new(raw).unwrap().to_state())
            .filter(|state| {
                [BlockDirection::Up, BlockDirection::Down]
                    .into_iter()
                    .any(|face| {
                        state.is_center_solid(face)
                            != state
                                .shape(ShapeKind::BlockSupport)
                                .is_supporting(face, SupportType::Center)
                    })
            })
            .map(|state| Block::from_state_id(state.id).name)
            .collect();
        differ.dedup();
        assert_eq!(differ, Vec::<&str>::new());
    }

    #[test]
    fn the_overrides_of_the_support_shape_answer_rigid_and_side_center() {
        let up_rigid =
            |state: &BlockState| state.is_face_sturdy(BlockDirection::Up, SupportType::Rigid);
        assert!(up_rigid(Block::SOUL_SAND.default_state));
        assert!(up_rigid(Block::MUD.default_state));
        let snow8 = Block::SNOW
            .states
            .iter()
            .find(|state| {
                crate::block_properties::SnowLikeProperties::from_state_id(state.id).layers == 8
            })
            .unwrap();
        assert!(up_rigid(snow8));
        let leaves = Block::OAK_LEAVES.default_state;
        assert!(!up_rigid(leaves));
        assert!(!leaves.is_face_sturdy(BlockDirection::North, SupportType::Center));
        // The default gate faces north and is closed: its support shape starts 5 pixels up.
        let gate = Block::OAK_FENCE_GATE.default_state;
        assert!(!gate.shape(ShapeKind::BlockSupport).is_empty());
        assert!(!gate.is_face_sturdy(BlockDirection::East, SupportType::Center));
    }

    #[test]
    #[should_panic(expected = "The min values need to be smaller or equals to the max values")]
    fn an_inverted_box_panics_like_vanilla() {
        let _ = VoxelShape::cuboid(0.5, 0.0, 0.0, 0.25, 1.0, 1.0);
    }

    #[test]
    fn only_first_cuts_the_inside_of_a_hopper_like_shape() {
        let outside = VoxelShape::block();
        let inside = VoxelShape::pixels(2.0, 4.0, 2.0, 14.0, 16.0, 14.0);
        let bowl = outside.join(&inside, BooleanOp::OnlyFirst);
        assert!((volume(&bowl) - (1.0 - 0.75 * 0.75 * 0.75)).abs() < 1e-9);
        assert!(!bowl.is_face_full(BlockDirection::Up));
        assert!(bowl.is_face_full(BlockDirection::Down));
        assert!(bowl.is_supporting(BlockDirection::Up, SupportType::Rigid));
        assert!(!bowl.is_supporting(BlockDirection::Up, SupportType::Center));
        assert!(bowl.join(&inside, BooleanOp::And).is_empty());
        assert!(
            !bowl
                .or(inside)
                .join_is_not_empty(&VoxelShape::block(), BooleanOp::NotSame)
        );
    }

    #[test]
    fn rotate_horizontal_turns_a_north_shape_clockwise() {
        // A box on the north half with a notch on its west side.
        let north = VoxelShape::pixels(0.0, 0.0, 0.0, 4.0, 16.0, 8.0);
        assert_eq!(
            corners(&north.rotate_horizontal(BlockDirection::East)),
            [[0.5, 0.0, 0.0, 1.0, 1.0, 0.25]]
        );
        assert_eq!(
            corners(&north.rotate_horizontal(BlockDirection::South)),
            [[0.75, 0.0, 0.5, 1.0, 1.0, 1.0]]
        );
        assert_eq!(
            corners(&north.rotate_horizontal(BlockDirection::West)),
            [[0.0, 0.0, 0.75, 0.5, 1.0, 1.0]]
        );
    }
}
