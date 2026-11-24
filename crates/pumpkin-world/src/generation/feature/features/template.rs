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
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::{
    math::{position::BlockPos, vector3::Vector3},
    random::{RandomGenerator, RandomImpl},
};

use crate::generation::{
    proto_chunk::GenerationCache,
    structure::template::{
        BlockPlacer, StructureProcessor, StructureTemplate, get_template, load_processor_list,
        place_template_palette_with_random, template_entities,
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
        let Some(entry) = pick_entry(self.templates, random) else {
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
        let processors: Arc<[StructureProcessor]> = match self.processors {
            TemplateProcessors::None => Arc::from([]),
            TemplateProcessors::Named(name) => load_processor_list(name),
            TemplateProcessors::Inline(list) => {
                list.iter().map(StructureProcessor::from_static).collect()
            }
        };
        place(chunk, &template, rotation, &processors, random, origin)
    }
}

/// Vanilla `WeightedList.getRandom`: an entry with weight 0 is never picked. Vanilla
/// `getRandomOrThrow` throws when the total weight is 0; this places nothing.
fn pick_entry<'a>(
    templates: &'a [TemplateEntry],
    random: &mut RandomGenerator,
) -> Option<&'a TemplateEntry> {
    let total_weight: i32 = templates.iter().map(|entry| entry.weight).sum();
    if total_weight <= 0 {
        return None;
    }
    let mut selection = random.next_bounded_i32(total_weight);
    templates.iter().find(|entry| {
        selection -= entry.weight;
        selection < 0
    })
}

/// The rest of vanilla `TemplateFeature.place` and `StructureTemplate.placeInWorld`, after the
/// template and the rotation are chosen.
fn place<T: GenerationCache>(
    chunk: &mut T,
    template: &StructureTemplate,
    rotation: Rotation,
    processors: &[StructureProcessor],
    random: &mut RandomGenerator,
    origin: BlockPos,
) -> bool {
    if template.palettes.is_empty() {
        return false;
    }
    // Vanilla `StructurePlaceSettings.getRandomPalette` takes the feature random, also for a
    // single palette.
    let palette_index = random.next_bounded_i32(template.palettes.len() as i32) as usize;
    if (template.palettes[palette_index].blocks().is_empty() && template.entities.is_empty())
        || template.size.x < 1
        || template.size.y < 1
        || template.size.z < 1
    {
        return false;
    }

    let corner = template_corner(template.size, origin.0, rotation);
    let mut placer = CachePlacer {
        chunk,
        placed: Vec::new(),
    };
    place_template_palette_with_random(
        &mut placer,
        template,
        palette_index,
        corner,
        rotation,
        processors,
        random,
    );
    update_shapes(placer.chunk, &placer.placed);

    // Vanilla `addEntitiesToWorld`: `WorldGenRegion.addFreshEntity` stores each entity in the
    // chunk of its block position.
    for (_, nbt) in template_entities(template, corner, rotation) {
        let Some(pos) = entity_block_pos(&nbt) else {
            continue;
        };
        placer.chunk.add_entity(&pos, nbt);
    }
    true
}

/// Vanilla `Entity.blockPosition`: the floor of the `Pos` of the entity.
fn entity_block_pos(nbt: &NbtCompound) -> Option<Vector3<i32>> {
    let pos = nbt.get_list("Pos")?;
    let coord = |i: usize| {
        pos.get(i)
            .and_then(NbtTag::extract_double)
            .map(|v| v.floor() as i32)
    };
    Some(Vector3::new(coord(0)?, coord(1)?, coord(2)?))
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

    use crate::ProtoChunk;
    use crate::generation::feature::placed_features::tests::empty_chunk;
    use pumpkin_util::random::xoroshiro128::Xoroshiro;

    const ORIGIN: BlockPos = BlockPos(Vector3::new(8, 64, 8));

    fn int_list(values: [i32; 3]) -> NbtTag {
        NbtTag::List(values.into_iter().map(NbtTag::Int).collect())
    }

    fn block_entry(name: &str) -> NbtTag {
        let mut entry = NbtCompound::new();
        entry.put_string("Name", name.to_string());
        NbtTag::Compound(entry)
    }

    /// A template of the given size with one block of state 0 at each position of `blocks`, one
    /// palette per entry of `palettes`, and one entity per entry of `entities` (position and
    /// entity NBT).
    fn template(
        size: [i32; 3],
        palettes: &[&str],
        blocks: &[[i32; 3]],
        entities: &[([f64; 3], NbtCompound)],
    ) -> StructureTemplate {
        let mut nbt = NbtCompound::new();
        nbt.put("size", int_list(size));
        nbt.put(
            "palettes",
            NbtTag::List(
                palettes
                    .iter()
                    .map(|name| NbtTag::List(vec![block_entry(name)]))
                    .collect(),
            ),
        );
        nbt.put(
            "blocks",
            NbtTag::List(
                blocks
                    .iter()
                    .map(|pos| {
                        let mut block = NbtCompound::new();
                        block.put("pos", int_list(*pos));
                        block.put_int("state", 0);
                        NbtTag::Compound(block)
                    })
                    .collect(),
            ),
        );
        nbt.put(
            "entities",
            NbtTag::List(
                entities
                    .iter()
                    .map(|(pos, entity_nbt)| {
                        let mut entity = NbtCompound::new();
                        entity.put(
                            "pos",
                            NbtTag::List(pos.iter().copied().map(NbtTag::Double).collect()),
                        );
                        entity.put("blockPos", int_list(pos.map(|v| v.floor() as i32)));
                        entity.put("nbt", NbtTag::Compound(entity_nbt.clone()));
                        NbtTag::Compound(entity)
                    })
                    .collect(),
            ),
        );
        StructureTemplate::from_nbt_compound(&nbt).unwrap()
    }

    fn xoroshiro(seed: u64) -> RandomGenerator {
        RandomGenerator::Xoroshiro(Xoroshiro::from_seed(seed))
    }

    #[test]
    fn places_the_drawn_palette() {
        // Vanilla `StructurePlaceSettings.getRandomPalette`: `nextInt(palettes)` of the feature
        // random picks the palette.
        let template = template(
            [1, 1, 1],
            &["minecraft:stone", "minecraft:dirt"],
            &[[0, 0, 0]],
            &[],
        );
        let mut seen = [false; 2];
        for seed in 0..16 {
            let mut expected_random = xoroshiro(seed);
            let palette_index = expected_random.next_bounded_i32(2) as usize;
            seen[palette_index] = true;

            let mut chunk = empty_chunk();
            let mut random = xoroshiro(seed);
            assert!(place(
                &mut chunk,
                &template,
                Rotation::None,
                &[],
                &mut random,
                ORIGIN
            ));
            let placed = GenerationCache::get_block_state(&chunk, &ORIGIN.0).to_block_id();
            let expected = [BlockId::STONE, BlockId::DIRT][palette_index];
            assert_eq!(placed, expected, "seed {seed}");
            assert_eq!(
                random.next_i32(),
                expected_random.next_i32(),
                "seed {seed}: random"
            );
        }
        assert_eq!(seen, [true, true]);
    }

    #[test]
    fn block_rot_draws_from_the_feature_random() {
        // Vanilla `TemplateFeature.place` calls `setRandom(random)`, so `BlockRotProcessor` takes
        // `nextFloat()` of the feature random for each block, in template block order, after the
        // palette draw.
        let blocks: Vec<[i32; 3]> = (0..4)
            .flat_map(|x| (0..4).map(move |z| [x, 0, z]))
            .collect();
        let template = template([4, 1, 4], &["minecraft:stone"], &blocks, &[]);
        let processors = [StructureProcessor::BlockRot {
            integrity: 0.5,
            rottable_blocks: None,
        }];
        let corner = template_corner(template.size, ORIGIN.0, Rotation::None);
        for seed in 0..4 {
            let mut expected_random = xoroshiro(seed);
            expected_random.next_bounded_i32(1);
            let expected: Vec<(Vector3<i32>, bool)> = template.palettes[0]
                .blocks()
                .iter()
                .map(|block| (corner.add(&block.pos), expected_random.next_f32() <= 0.5))
                .collect();
            assert!(expected.iter().any(|(_, kept)| *kept));
            assert!(expected.iter().any(|(_, kept)| !*kept));

            let mut chunk = empty_chunk();
            let mut random = xoroshiro(seed);
            assert!(place(
                &mut chunk,
                &template,
                Rotation::None,
                &processors,
                &mut random,
                ORIGIN
            ));
            for (pos, kept) in expected {
                let placed = GenerationCache::get_block_state(&chunk, &pos).to_block_id();
                let expected = if kept { BlockId::STONE } else { BlockId::AIR };
                assert_eq!(placed, expected, "seed {seed} {pos:?}");
            }
            assert_eq!(
                random.next_i32(),
                expected_random.next_i32(),
                "seed {seed}: random"
            );
        }
    }

    fn placed_entities(chunk: &mut ProtoChunk) -> Vec<NbtCompound> {
        chunk.take_pending_structure_entities()
    }

    #[test]
    fn places_template_entities() {
        // Vanilla `addEntitiesToWorld`: `Pos` is the placed position plus
        // `StructureTemplate.transform(Vec3)` around pivot zero, `UUID` is removed, and the yaw
        // turns with the rotation.
        let mut entity = NbtCompound::new();
        entity.put_string("id", "minecraft:armor_stand".to_string());
        entity.put("UUID", NbtTag::IntArray(vec![1, 2, 3, 4]));
        entity.put(
            "Rotation",
            NbtTag::List(vec![NbtTag::Float(10.0), NbtTag::Float(0.0)]),
        );
        let local = [1.25, 0.5, 0.75];
        let template = template(
            [3, 2, 2],
            &["minecraft:stone"],
            &[[0, 0, 0]],
            &[(local, entity)],
        );
        for (rotation, yaw_turn) in [
            (Rotation::None, 0.0),
            (Rotation::Clockwise90, 90.0),
            (Rotation::Rotate180, 180.0),
            (Rotation::CounterClockwise90, 270.0),
        ] {
            let (ox, oz) = rotation.rotate_offset(-(template.size.x / 2), -(template.size.z / 2));
            let base = (f64::from(ORIGIN.0.x + ox), f64::from(ORIGIN.0.z + oz));
            let (x, z) = (local[0], local[2]);
            let (rx, rz) = match rotation {
                Rotation::None => (x, z),
                Rotation::Clockwise90 => (1.0 - z, x),
                Rotation::Rotate180 => (1.0 - x, 1.0 - z),
                Rotation::CounterClockwise90 => (z, 1.0 - x),
            };
            let expected = [base.0 + rx, f64::from(ORIGIN.0.y) + local[1], base.1 + rz];

            let mut chunk = empty_chunk();
            let mut random = xoroshiro(1);
            assert!(place(
                &mut chunk,
                &template,
                rotation,
                &[],
                &mut random,
                ORIGIN
            ));
            let entities = placed_entities(&mut chunk);
            assert_eq!(entities.len(), 1, "{rotation:?}");
            let nbt = &entities[0];
            let pos: Vec<f64> = nbt
                .get_list("Pos")
                .unwrap()
                .iter()
                .map(|v| v.extract_double().unwrap())
                .collect();
            assert_eq!(pos, expected, "{rotation:?}");
            assert!(nbt.get("UUID").is_none(), "{rotation:?}");
            let yaw = nbt.get_list("Rotation").unwrap()[0]
                .extract_float()
                .unwrap();
            assert!(
                (yaw - (10.0 + yaw_turn)).abs() < 1e-4,
                "{rotation:?}: yaw {yaw}"
            );
        }
    }

    #[test]
    fn zero_weight_template_is_never_picked() {
        const ROTATIONS: &[Rotation] = &[Rotation::None];
        let templates = [
            TemplateEntry {
                id: "never",
                rotations: ROTATIONS,
                weight: 0,
            },
            TemplateEntry {
                id: "always",
                rotations: ROTATIONS,
                weight: 3,
            },
        ];
        for seed in 0..32 {
            assert_eq!(
                pick_entry(&templates, &mut xoroshiro(seed)).unwrap().id,
                "always"
            );
        }
        assert!(pick_entry(&templates[..1], &mut xoroshiro(0)).is_none());
    }
}
