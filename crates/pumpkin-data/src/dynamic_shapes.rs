//! The declared shapes of custom blocks.
//!
//! A custom block declares its outline and its collision shape per state, as a function of the
//! property values of the state. The freeze calls the functions once per state and stores the
//! boxes in the shape table after the generated boxes, as the vanilla `BlockStateBase.Cache` keeps
//! the shapes of a block without `dynamicShape`. Shapes never depend on the position or the
//! entity.
//!
//! A custom block without declared shapes is a full cube, or takes the shapes of its block type
//! when it has one. The flags that vanilla derives from the shapes (the sturdy faces, the full
//! cube, the legacy solid flag, the solid render and the redstone conductor) follow the shapes of
//! the state.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use pumpkin_util::math::boundingbox::BoundingBox;

use crate::block_properties::COLLISION_SHAPES;
use crate::block_state::{
    CAN_OCCLUDE, DOWN_CENTER_SOLID, DOWN_SIDE_SOLID, EAST_SIDE_SOLID, IS_FULL_CUBE, IS_SOLID,
    IS_SOLID_BLOCK, IS_SOLID_RENDER, NORTH_SIDE_SOLID, SOUTH_SIDE_SOLID, UP_CENTER_SOLID,
    UP_SIDE_SOLID, WEST_SIDE_SOLID,
};
use crate::dynamic::{self, BlockPropertyDefinition, RegistryError};
use crate::voxel_shape::{SupportType, VoxelShape};
use crate::{BlockDirection, BlockState};

/// A function that gives the shape of one state from its property values.
pub type ShapeFn = Arc<dyn Fn(&StateProperties<'_>) -> VoxelShape + Send + Sync>;

/// The shapes that a custom block declares. A missing outline is the full cube, as vanilla
/// `BlockBehaviour.getShape`; a missing collision shape is the outline, as vanilla
/// `getCollisionShape` for a block with collision.
#[derive(Clone, Default)]
pub struct DeclaredShapes {
    pub shape: Option<ShapeFn>,
    pub collision_shape: Option<ShapeFn>,
}

impl fmt::Debug for DeclaredShapes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeclaredShapes")
            .field("shape", &self.shape.is_some())
            .field("collision_shape", &self.collision_shape.is_some())
            .finish()
    }
}

/// Declares the shapes of the registered custom block `block`. A second call replaces the first.
///
/// # Errors
///
/// [`RegistryError::RegistryFrozen`] when [`dynamic::register_block`] fails with it: after the
/// freeze, and after a failed freeze. [`RegistryError::InvalidDefinition`] when no block
/// `block` is registered.
pub fn declare(block: &str, shapes: DeclaredShapes) -> Result<(), RegistryError> {
    dynamic::with_builder(|builder| builder.declare_shapes(block, shapes))
}

/// The property values of one state of a custom block, as a shape function sees them.
#[derive(Clone, Copy, Debug)]
pub struct StateProperties<'a> {
    properties: &'a [BlockPropertyDefinition],
    index: usize,
}

impl<'a> StateProperties<'a> {
    /// The state `index` of a block whose properties are sorted by name.
    pub(crate) const fn new(properties: &'a [BlockPropertyDefinition], index: usize) -> Self {
        Self { properties, index }
    }

    /// The value of the property `name`, or `None` when the block has no such property.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&'a str> {
        // The index orders the property values like `DynamicProperties`: the last property
        // changes fastest.
        let mut rest = self.index;
        for property in self.properties.iter().rev() {
            let len = property.values.len();
            if property.name == name {
                return Some(property.values[rest % len].as_str());
            }
            rest /= len;
        }
        None
    }

    /// The value of the boolean property `name`.
    #[must_use]
    pub fn get_bool(&self, name: &str) -> Option<bool> {
        self.get(name).and_then(|value| value.parse().ok())
    }

    /// The value of the integer property `name`.
    #[must_use]
    pub fn get_int(&self, name: &str) -> Option<u8> {
        self.get(name).and_then(|value| value.parse().ok())
    }

    /// The direction value of the property `name`, such as `facing`.
    #[must_use]
    pub fn get_direction(&self, name: &str) -> Option<BlockDirection> {
        Some(match self.get(name)? {
            "down" => BlockDirection::Down,
            "up" => BlockDirection::Up,
            "north" => BlockDirection::North,
            "south" => BlockDirection::South,
            "west" => BlockDirection::West,
            "east" => BlockDirection::East,
            _ => return None,
        })
    }
}

/// The boxes of the declared shapes, numbered after the generated boxes.
pub(crate) struct ShapeTable {
    vanilla: HashMap<[u64; 6], u16>,
    custom: HashMap<[u64; 6], u16>,
    boxes: Vec<BoundingBox>,
    slices: HashMap<Vec<u16>, &'static [u16]>,
}

impl ShapeTable {
    pub(crate) fn new() -> Self {
        Self {
            vanilla: COLLISION_SHAPES
                .iter()
                .enumerate()
                .map(|(index, shape)| (key(shape), index as u16))
                .collect(),
            custom: HashMap::new(),
            boxes: Vec::new(),
            slices: HashMap::new(),
        }
    }

    /// The shape table indices of `shape`, reusing a generated box with the same corners.
    pub(crate) fn intern(&mut self, shape: &VoxelShape) -> Result<&'static [u16], RegistryError> {
        let mut ids = Vec::with_capacity(shape.boxes().len());
        for b in shape.boxes() {
            let key = key(b);
            let id = match self.vanilla.get(&key).or_else(|| self.custom.get(&key)) {
                Some(&id) => id,
                None => {
                    let id = u16::try_from(COLLISION_SHAPES.len() + self.boxes.len())
                        .ok()
                        .filter(|&id| id < u16::MAX)
                        .ok_or(RegistryError::RangeExceeded {
                            registry: "block shape box",
                            count: self.boxes.len() + 1,
                        })?;
                    self.boxes.push(*b);
                    self.custom.insert(key, id);
                    id
                }
            };
            ids.push(id);
        }
        if let Some(slice) = self.slices.get(&ids) {
            return Ok(slice);
        }
        let slice: &'static [u16] = Box::leak(ids.clone().into_boxed_slice());
        self.slices.insert(ids, slice);
        Ok(slice)
    }

    pub(crate) fn into_boxes(self) -> Vec<BoundingBox> {
        self.boxes
    }
}

fn key(b: &BoundingBox) -> [u64; 6] {
    [
        b.min.x.to_bits(),
        b.min.y.to_bits(),
        b.min.z.to_bits(),
        b.max.x.to_bits(),
        b.max.y.to_bits(),
        b.max.z.to_bits(),
    ]
}

/// The outline and the collision shape of the state `index` of a block with declared shapes.
pub(crate) fn evaluate(
    shapes: &DeclaredShapes,
    properties: &[BlockPropertyDefinition],
    index: usize,
) -> (VoxelShape, VoxelShape) {
    let state = StateProperties::new(properties, index);
    let outline = shapes
        .shape
        .as_ref()
        .map_or_else(VoxelShape::block, |shape| shape(&state));
    let collision = shapes
        .collision_shape
        .as_ref()
        .map_or_else(|| outline.clone(), |shape| shape(&state));
    (outline, collision)
}

/// The state and side flags of `source` with the flags that vanilla derives from the shapes
/// recomputed for `outline` and `collision`.
pub(crate) fn shape_flags(
    source: &BlockState,
    outline: &VoxelShape,
    collision: &VoxelShape,
) -> (u16, u8) {
    let full_cube = collision.is_full_block();
    let mut state_flags = source.state_flags & !(IS_SOLID | IS_FULL_CUBE | IS_SOLID_RENDER);
    if collision.is_legacy_solid() {
        state_flags |= IS_SOLID;
    }
    if full_cube {
        state_flags |= IS_FULL_CUBE;
    }
    // `isRedstoneConductor` defaults to `isCollisionShapeFullBlock`. The flag of a full cube
    // source tells whether its block overrides the default (glass never conducts); the flag of
    // any other source tells nothing.
    let source_overrides = source.state_flags & IS_FULL_CUBE != 0;
    if full_cube && (!source_overrides || source.state_flags & IS_SOLID_BLOCK != 0) {
        state_flags |= IS_SOLID_BLOCK;
    } else {
        state_flags &= !IS_SOLID_BLOCK;
    }
    if source.state_flags & CAN_OCCLUDE != 0 && outline.is_full_block() {
        state_flags |= IS_SOLID_RENDER;
    }
    let mut side_flags = 0;
    for (direction, flag) in [
        (BlockDirection::Down, DOWN_SIDE_SOLID),
        (BlockDirection::Up, UP_SIDE_SOLID),
        (BlockDirection::North, NORTH_SIDE_SOLID),
        (BlockDirection::South, SOUTH_SIDE_SOLID),
        (BlockDirection::West, WEST_SIDE_SOLID),
        (BlockDirection::East, EAST_SIDE_SOLID),
    ] {
        if collision.is_face_full(direction) {
            side_flags |= flag;
        }
    }
    if collision.is_supporting(BlockDirection::Down, SupportType::Center) {
        side_flags |= DOWN_CENTER_SOLID;
    }
    if collision.is_supporting(BlockDirection::Up, SupportType::Center) {
        side_flags |= UP_CENTER_SOLID;
    }
    (state_flags, side_flags)
}
