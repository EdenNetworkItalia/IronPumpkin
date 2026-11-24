use std::sync::Arc;

use pumpkin_data::{
    Block, BlockDirection, BlockId, BlockState, BlockStateId, Rotation,
    block_properties::{
        PointedDripstoneLikeProperties, PotentSulfurLikeProperties, PotentSulfurState,
        SpeleothemThickness, VerticalDirection,
    },
    fluid::Fluid,
    processor_list::StaticProcessor,
    tag,
};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::{
    math::{position::BlockPos, vector3::Vector3},
    random::{RandomGenerator, RandomImpl},
};

use crate::generation::{
    proto_chunk::GenerationCache,
    structure::template::{
        BlockPlacer, StructureProcessor, get_template, load_processor_list, place_template,
    },
};

/// Vanilla `TemplateFeature`: places a weighted random structure template with a random
/// rotation, centered on the origin in X and Z.
pub struct TemplateFeature {
    pub templates: &'static [TemplateEntry],
    pub processors: TemplateProcessors,
}

/// Vanilla `TemplateFeature.TemplateEntry` and its weight in the weighted list.
pub struct TemplateEntry {
    pub id: &'static str,
    pub rotations: &'static [Rotation],
    pub weight: i32,
}

pub enum TemplateProcessors {
    None,
    /// A processor list of the registry. It resolves at placement, so a data pack can replace it.
    Named(&'static str),
    Inline(&'static [StaticProcessor]),
}

impl TemplateFeature {
    pub fn generate<T: GenerationCache>(
        &self,
        chunk: &mut T,
        random: &mut RandomGenerator,
        origin: BlockPos,
    ) -> bool {
        // Vanilla `WeightedList.getRandomOrThrow`, then `Util.getRandom` for the rotation.
        let total_weight: i32 = self.templates.iter().map(|entry| entry.weight).sum();
        if total_weight <= 0 {
            return false;
        }
        let mut selection = random.next_bounded_i32(total_weight);
        let Some(entry) = self.templates.iter().find(|entry| {
            selection -= entry.weight;
            selection < 0
        }) else {
            return false;
        };
        if entry.rotations.is_empty() {
            return false;
        }
        let rotation =
            entry.rotations[random.next_bounded_i32(entry.rotations.len() as i32) as usize];

        // Vanilla `StructureTemplateManager.getOrCreate` gives an empty template for a missing id,
        // and an empty template places nothing.
        let Some(template) = get_template(entry.id.strip_prefix("minecraft:").unwrap_or(entry.id))
        else {
            return false;
        };
        if template.palettes.is_empty() {
            return false;
        }
        // Vanilla `StructurePlaceSettings.getRandomPalette` takes the feature random, also for a
        // single palette. `place_template` places the first palette.
        random.next_bounded_i32(template.palettes.len() as i32);
        if (template.blocks.is_empty() && template.entities.is_empty())
            || template.size.x < 1
            || template.size.y < 1
            || template.size.z < 1
        {
            return false;
        }

        let processors: Arc<[StructureProcessor]> = match self.processors {
            TemplateProcessors::None => Arc::from([]),
            TemplateProcessors::Named(name) => load_processor_list(name),
            TemplateProcessors::Inline(list) => {
                list.iter().map(StructureProcessor::from_static).collect()
            }
        };

        let corner = template_corner(template.size, origin.0, rotation);
        let mut placer = CachePlacer {
            chunk,
            placed: Vec::new(),
        };
        place_template(
            &mut placer,
            &template,
            corner,
            (0, 0),
            rotation,
            false,
            true,
            &processors,
            None,
        );
        update_shapes(placer.chunk, &placer.placed);
        true
    }
}

/// Returns the world position of the minimum corner of the rotated template.
///
/// Vanilla moves the origin by `getRotatedOffset` on the X and Z axis, then rotates each block
/// around that position (pivot zero). `place_template` rotates inside the template bounds, so the
/// corner is the vanilla position minus the shift between the two rotations.
const fn template_corner(
    size: Vector3<i32>,
    origin: Vector3<i32>,
    rotation: Rotation,
) -> Vector3<i32> {
    let (offset_x, offset_z) = rotation.rotate_offset(-(size.x / 2), -(size.z / 2));
    let (shift_x, shift_z) = match rotation {
        Rotation::None => (0, 0),
        Rotation::Clockwise90 => (size.z - 1, 0),
        Rotation::Rotate180 => (size.x - 1, size.z - 1),
        Rotation::CounterClockwise90 => (0, size.x - 1),
    };
    Vector3::new(
        origin.x + offset_x - shift_x,
        origin.y,
        origin.z + offset_z - shift_z,
    )
}

/// Lets `place_template` write into the decoration cache, and records the placed positions in
/// placement order.
struct CachePlacer<'a, T: GenerationCache> {
    chunk: &'a mut T,
    placed: Vec<Vector3<i32>>,
}

impl<T: GenerationCache> BlockPlacer for CachePlacer<'_, T> {
    fn get_block_state(&self, pos: &Vector3<i32>) -> BlockStateId {
        GenerationCache::get_block_state(self.chunk, pos)
    }

    fn set_block_state(&mut self, pos: &Vector3<i32>, state: &BlockState) {
        GenerationCache::set_block_state(self.chunk, pos, state);
        self.placed.push(*pos);
    }

    fn add_block_entity(&mut self, nbt: NbtCompound) {
        let (Some(x), Some(y), Some(z)) = (nbt.get_int("x"), nbt.get_int("y"), nbt.get_int("z"))
        else {
            return;
        };
        GenerationCache::add_block_entity(self.chunk, &Vector3::new(x, y, z), nbt);
    }
}

/// Vanilla `BlockBehaviour.UPDATE_SHAPE_ORDER`.
const UPDATE_SHAPE_ORDER: [BlockDirection; 6] = [
    BlockDirection::West,
    BlockDirection::East,
    BlockDirection::North,
    BlockDirection::South,
    BlockDirection::Down,
    BlockDirection::Up,
];

/// The shape update pass at the end of vanilla `StructureTemplate.placeInWorld`:
/// `updateShapeAtEdge` on every face of the placed volume, then `Block.updateFromNeighbourShapes`
/// on each placed block in placement order.
///
/// Only the blocks of [`update_shape`] change. They are the blocks of the vanilla feature
/// templates whose `updateShape` changes the state.
fn update_shapes<T: GenerationCache>(chunk: &mut T, placed: &[Vector3<i32>]) {
    let Some(min) = placed
        .iter()
        .copied()
        .reduce(|a, b| Vector3::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)))
    else {
        return;
    };
    let max = placed.iter().copied().fold(min, |a, b| {
        Vector3::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z))
    });
    let size = max.sub(&min).add(&Vector3::new(1, 1, 1));
    let index = |p: Vector3<i32>| ((p.x * size.y + p.y) * size.z + p.z) as usize;
    let mut filled = vec![false; (size.x * size.y * size.z) as usize];
    for pos in placed {
        filled[index(pos.sub(&min))] = true;
    }
    let is_full = |x: i32, y: i32, z: i32| filled[index(Vector3::new(x, y, z))];

    // Vanilla `DiscreteVoxelShape.forAllFaces`: one sweep along Z, then Y, then X.
    let mut faces = Vec::new();
    for (sweep_size, outer, inner, negative, positive) in [
        (
            size.z,
            (size.x, size.y),
            0,
            BlockDirection::North,
            BlockDirection::South,
        ),
        (
            size.y,
            (size.z, size.x),
            1,
            BlockDirection::Down,
            BlockDirection::Up,
        ),
        (
            size.x,
            (size.y, size.z),
            2,
            BlockDirection::West,
            BlockDirection::East,
        ),
    ] {
        let local = |a: i32, b: i32, c: i32| match inner {
            0 => Vector3::new(a, b, c),
            1 => Vector3::new(b, c, a),
            _ => Vector3::new(c, a, b),
        };
        for a in 0..outer.0 {
            for b in 0..outer.1 {
                let mut last_full = false;
                for c in 0..=sweep_size {
                    let full = c != sweep_size && {
                        let p = local(a, b, c);
                        is_full(p.x, p.y, p.z)
                    };
                    if !last_full && full {
                        faces.push((negative, local(a, b, c)));
                    }
                    if last_full && !full {
                        faces.push((positive, local(a, b, c - 1)));
                    }
                    last_full = full;
                }
            }
        }
    }

    for (direction, local) in faces {
        let pos = min.add(&local);
        let neighbor_pos = pos.add(&direction.to_offset());
        let state = GenerationCache::get_block_state(chunk, &pos);
        if let Some(new_state) = update_shape(chunk, state, pos, direction) {
            chunk.set_block_state(&pos, new_state);
        }
        let neighbor = GenerationCache::get_block_state(chunk, &neighbor_pos);
        if let Some(new_neighbor) =
            update_shape(chunk, neighbor, neighbor_pos, direction.opposite())
        {
            chunk.set_block_state(&neighbor_pos, new_neighbor);
        }
    }

    for &pos in placed {
        let old_state = GenerationCache::get_block_state(chunk, &pos);
        let new_state = UPDATE_SHAPE_ORDER
            .iter()
            .fold(old_state, |state, &direction| {
                update_shape(chunk, state, pos, direction).map_or(state, |new| new.id)
            });
        if new_state != old_state {
            chunk.set_block_state(&pos, new_state.to_state());
        }
    }
}

/// Vanilla `BlockState.updateShape` for the blocks of the vanilla feature templates that change
/// state: speleothems (`SpeleothemBlock`) and potent sulfur (`PotentSulfurBlock`). Returns `None`
/// when the state does not change. Scheduled ticks and block entity changes are not ported.
fn update_shape<T: GenerationCache>(
    chunk: &T,
    state: BlockStateId,
    pos: Vector3<i32>,
    direction: BlockDirection,
) -> Option<&'static BlockState> {
    let block = Block::from_state_id(state);
    let new_state = if block.id.has_tag(tag::Block::MINECRAFT_SPELEOTHEMS) {
        update_speleothem_shape(chunk, block, state, pos, direction)?
    } else if block.id == BlockId::POTENT_SULFUR {
        potent_sulfur_state(chunk, state, pos)
    } else {
        return None;
    };
    (new_state != state).then(|| new_state.to_state())
}

fn speleothem_tip(state: BlockStateId) -> BlockDirection {
    match PointedDripstoneLikeProperties::from_state_id(state).vertical_direction {
        VerticalDirection::Up => BlockDirection::Up,
        VerticalDirection::Down => BlockDirection::Down,
    }
}

/// Vanilla `SpeleothemBlock.isSpeleothemWithDirection`.
fn is_speleothem_with_direction(state: BlockStateId, tip: BlockDirection) -> bool {
    state
        .to_block_id()
        .has_tag(tag::Block::MINECRAFT_SPELEOTHEMS)
        && speleothem_tip(state) == tip
}

/// Vanilla `SpeleothemBlock.updateShape`, without the scheduled ticks.
fn update_speleothem_shape<T: GenerationCache>(
    chunk: &T,
    block: &'static Block,
    state: BlockStateId,
    pos: Vector3<i32>,
    direction: BlockDirection,
) -> Option<BlockStateId> {
    if direction != BlockDirection::Up && direction != BlockDirection::Down {
        return None;
    }
    let tip = speleothem_tip(state);
    let base = tip.opposite();
    let get = |p: Vector3<i32>| GenerationCache::get_block_state(chunk, &p);
    if direction == base {
        // Vanilla `isValidSpeleothemPlacement`. A speleothem that cannot survive only schedules a
        // tick here.
        let behind = pos.add(&base.to_offset());
        let behind_state = get(behind);
        let survives = behind_state.to_state().is_side_solid(tip)
            || (is_speleothem_with_direction(behind_state, tip)
                && Block::from_state_id(behind_state) == block);
        if !survives {
            return None;
        }
    }

    // Vanilla `calculateSpeleothemThickness`.
    let mut properties = PointedDripstoneLikeProperties::from_state_id(state);
    let merge_opposing_tips = properties.thickness == SpeleothemThickness::TipMerge;
    let in_front = get(pos.add(&tip.to_offset()));
    properties.thickness = if is_speleothem_with_direction(in_front, base)
        && Block::from_state_id(in_front) == block
    {
        if !merge_opposing_tips
            && PointedDripstoneLikeProperties::from_state_id(in_front).thickness
                != SpeleothemThickness::TipMerge
        {
            SpeleothemThickness::Tip
        } else {
            SpeleothemThickness::TipMerge
        }
    } else if !is_speleothem_with_direction(in_front, tip) {
        SpeleothemThickness::Tip
    } else {
        let in_front_thickness = PointedDripstoneLikeProperties::from_state_id(in_front).thickness;
        if in_front_thickness == SpeleothemThickness::Tip
            || in_front_thickness == SpeleothemThickness::TipMerge
        {
            SpeleothemThickness::Frustum
        } else if is_speleothem_with_direction(get(pos.add(&base.to_offset())), tip) {
            SpeleothemThickness::Middle
        } else {
            SpeleothemThickness::Base
        }
    };
    Some(properties.to_state_id(block))
}

/// Vanilla `PotentSulfurBlock.validBlockState`, without the countdown reset of the block entity.
fn potent_sulfur_state<T: GenerationCache>(
    chunk: &T,
    state: BlockStateId,
    pos: Vector3<i32>,
) -> BlockStateId {
    let mut properties = PotentSulfurLikeProperties::from_state_id(state);
    let (above_fluid, above_fluid_state) =
        GenerationCache::get_fluid_and_fluid_state(chunk, &pos.add(&Vector3::new(0, 1, 0)));
    let below = pos.add(&Vector3::new(0, -1, 0));
    let below_block = Block::from_state_id(GenerationCache::get_block_state(chunk, &below));
    let (below_fluid, below_fluid_state) =
        GenerationCache::get_fluid_and_fluid_state(chunk, &below);
    let below_is_source_if_fluid = below_fluid == Fluid::EMPTY || below_fluid_state.is_source;
    properties.potent_sulfur_state =
        if !(above_fluid == Fluid::WATER && above_fluid_state.is_source) {
            PotentSulfurState::Dry
        } else if below_block
            .id
            .has_tag(tag::Block::MINECRAFT_CAUSES_CONTINUOUS_GEYSER_ERUPTIONS)
            && below_is_source_if_fluid
        {
            PotentSulfurState::Continuous
        } else if below_block
            .id
            .has_tag(tag::Block::MINECRAFT_CAUSES_PERIODIC_GEYSER_ERUPTIONS)
            && below_is_source_if_fluid
        {
            if properties.potent_sulfur_state == PotentSulfurState::Erupting {
                PotentSulfurState::Erupting
            } else {
                PotentSulfurState::Dormant
            }
        } else {
            PotentSulfurState::Wet
        };
    properties.to_state_id(&Block::POTENT_SULFUR)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vanilla block position for a template block: `origin + getRotatedOffset` plus the block
    /// rotated around pivot zero (`StructureTemplate.calculateRelativePosition`).
    fn vanilla_pos(
        size: Vector3<i32>,
        origin: Vector3<i32>,
        rotation: Rotation,
        local: Vector3<i32>,
    ) -> Vector3<i32> {
        let (ox, oz) = rotation.rotate_offset(-(size.x / 2), -(size.z / 2));
        let (rx, rz) = rotation.rotate_offset(local.x, local.z);
        Vector3::new(origin.x + ox + rx, origin.y + local.y, origin.z + oz + rz)
    }

    #[test]
    fn corner_matches_vanilla_rotation_around_pivot() {
        let size = Vector3::new(5, 3, 2);
        let origin = Vector3::new(100, 64, -40);
        for rotation in [
            Rotation::None,
            Rotation::Clockwise90,
            Rotation::Rotate180,
            Rotation::CounterClockwise90,
        ] {
            let corner = template_corner(size, origin, rotation);
            for x in 0..size.x {
                for z in 0..size.z {
                    let local = Vector3::new(x, 1, z);
                    let placed = corner + rotation.transform_pos(local, size);
                    assert_eq!(
                        placed,
                        vanilla_pos(size, origin, rotation, local),
                        "{rotation:?} {local:?}"
                    );
                }
            }
        }
    }
}
