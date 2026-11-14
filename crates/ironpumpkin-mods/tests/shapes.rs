//! Scenarios of the spec `neoforge-shapes` (`openspec/changes/neoforge-shaped-plugin-api`) on a
//! real server: the shape reads of vanilla blocks, the collision tests, and the declared shapes of
//! mod blocks in entity collision, ray traces and support checks. The content registry freezes
//! once per process, so this file is its own test binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

use std::sync::Arc;

use ironpumpkin_mods::{
    ModInit, NativeMod,
    content::BlockBuilder,
    pumpkin_core::{
        entity::{Entity, EntityBase},
        plugin::startup::STARTUP_HOOK,
        world::World,
    },
    pumpkin_data::{Block, BlockDirection, BlockState, entity::EntityType},
    pumpkin_util::math::{boundingbox::BoundingBox, position::BlockPos, vector3::Vector3},
    pumpkin_world::world::BlockFlags,
    register_mod,
    shapes::{
        BooleanOp, ShapeKind, SupportType, VoxelShape, get_block_shape, get_state_shape,
        is_face_sturdy, no_collision,
    },
};

struct ShapeMod;

impl NativeMod for ShapeMod {
    fn id(&self) -> &'static str {
        "shapemod"
    }

    fn display_name(&self) -> &'static str {
        "Shape Mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        // Half a block high, as a slab.
        cx.register_block(
            BlockBuilder::new("shapemod:half", Block::STONE.default_state.id)
                .shape(|_| VoxelShape::pixels(0.0, 0.0, 0.0, 16.0, 8.0, 16.0)),
        )
        .unwrap();
        // A full cube: no declared shape and no block type.
        cx.register_block(BlockBuilder::new(
            "shapemod:cube",
            Block::TORCH.default_state.id,
        ))
        .unwrap();
        // A step whose high half faces `facing`, with no collision when `open`, and a bowl as
        // its outline: the state picks the shape.
        cx.register_block(
            BlockBuilder::new("shapemod:step", Block::STONE.default_state.id)
                .enum_property("facing", &["north", "east", "south", "west"], "north")
                .bool_property("open", false)
                .shape(|_| {
                    VoxelShape::block().join(
                        &VoxelShape::pixels(2.0, 4.0, 2.0, 14.0, 16.0, 14.0),
                        BooleanOp::OnlyFirst,
                    )
                })
                .collision_shape(|state| {
                    if state.get_bool("open") == Some(true) {
                        return VoxelShape::empty();
                    }
                    VoxelShape::pixels(0.0, 0.0, 0.0, 16.0, 8.0, 16.0)
                        .or(VoxelShape::pixels(0.0, 8.0, 0.0, 16.0, 16.0, 8.0))
                        .rotate_horizontal(state.get_direction("facing").unwrap())
                }),
        )
        .unwrap();
        // A stair by type, with no declared shape: the stair shapes.
        cx.register_block(
            BlockBuilder::new("shapemod:stairs", Block::OAK_STAIRS.default_state.id)
                .block_type("minecraft:oak_stairs"),
        )
        .unwrap();
    }
}

register_mod!(ShapeMod);

fn corners(shape: &VoxelShape) -> Vec<[f64; 6]> {
    let mut boxes: Vec<[f64; 6]> = shape
        .boxes()
        .iter()
        .map(|b| [b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z])
        .collect();
    boxes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    boxes
}

fn load(world: &World, pos: &BlockPos) {
    {
        let chunk = pos.chunk_position();
        let mut loading = world.level.chunk_loading.lock().unwrap();
        loading.add_force_ticket(chunk);
        loading.send_change();
    };
    let deadline = std::time::Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&pos.chunk_position()) {
        assert!(
            std::time::Instant::now() < deadline,
            "the chunk did not load"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// A clear 3x3 column of air above `pos`, so nothing but the tested block is in the way.
fn clear_above(world: &Arc<World>, pos: &BlockPos) {
    for dx in -1..=1 {
        for dy in 0..=4 {
            for dz in -1..=1 {
                world.set_block_state(
                    &pos.offset(Vector3::new(dx, dy, dz)),
                    Block::AIR.default_state.id,
                    BlockFlags::empty(),
                );
            }
        }
    }
}

fn state_of(name: &str, properties: &[(&str, &str)]) -> &'static BlockState {
    let block = Block::from_name(name).unwrap();
    let id = block
        .states
        .iter()
        .find(|state| {
            let values = block.string_properties(state.id).unwrap();
            properties.iter().all(|property| values.contains(property))
        })
        .unwrap()
        .id;
    BlockState::from_id(id)
}

#[test]
fn shapes_of_vanilla_and_mod_blocks() {
    STARTUP_HOOK.set(ironpumpkin_mods::init_mods).unwrap();
    let world = support::world();

    a_vanilla_slab();
    the_shape_getters_of_a_mod_block();
    free_space_for_a_spawn(&world);
    a_half_height_mod_block(&world);
}

/// Scenario "A vanilla slab".
fn a_vanilla_slab() {
    let slab = get_state_shape(Block::STONE_SLAB.default_state, ShapeKind::Collision);
    assert_eq!(corners(&slab), [[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]);
}

fn the_shape_getters_of_a_mod_block() {
    let cube = Block::from_name("shapemod:cube").unwrap().default_state;
    assert!(get_state_shape(cube, ShapeKind::Collision).is_full_block());
    assert!(get_state_shape(cube, ShapeKind::Outline).is_full_block());
    assert!(cube.is_full_cube() && cube.is_solid());
    for face in BlockDirection::all() {
        assert!(cube.is_side_solid(face), "{face:?}");
    }
    assert!(
        !get_state_shape(cube, ShapeKind::Collision)
            .join_is_not_empty(&VoxelShape::block(), BooleanOp::NotSame)
    );

    let step = state_of("shapemod:step", &[("facing", "east"), ("open", "false")]);
    let collision = get_state_shape(step, ShapeKind::Collision);
    assert_eq!(
        corners(&collision),
        [
            [0.0, 0.0, 0.0, 1.0, 0.5, 1.0],
            [0.5, 0.5, 0.0, 1.0, 1.0, 1.0]
        ]
    );
    assert!(step.is_side_solid(BlockDirection::East));
    assert!(!step.is_side_solid(BlockDirection::West));
    assert!(step.is_face_sturdy(BlockDirection::Down, SupportType::Rigid));
    assert!(!step.is_face_sturdy(BlockDirection::Up, SupportType::Rigid));
    assert!(!step.is_face_sturdy(BlockDirection::Up, SupportType::Full));
    let outline = get_state_shape(step, ShapeKind::Outline);
    assert!(!outline.is_face_full(BlockDirection::Up));
    assert!(outline.is_face_full(BlockDirection::Down));
    let open = state_of("shapemod:step", &[("facing", "east"), ("open", "true")]);
    assert!(get_state_shape(open, ShapeKind::Collision).is_empty());
    assert!(!open.is_solid() && !open.is_side_solid(BlockDirection::Up));

    let stairs = Block::from_name("shapemod:stairs").unwrap();
    let oak = Block::OAK_STAIRS;
    assert_eq!(
        corners(&get_state_shape(stairs.default_state, ShapeKind::Collision)),
        corners(&get_state_shape(oak.default_state, ShapeKind::Collision))
    );
}

/// Scenario "Free space for a spawn".
fn free_space_for_a_spawn(world: &Arc<World>) {
    let pos = BlockPos::new(24, 100, 24);
    load(world, &pos);
    clear_above(world, &pos);
    world.set_block_state(&pos, Block::STONE.default_state.id, BlockFlags::empty());
    let zombie = Entity::new(
        world.clone(),
        Vector3::new(24.5, 104.0, 24.5),
        &EntityType::ZOMBIE,
    );
    let inside_the_wall = BoundingBox::new(
        Vector3::new(24.0, 100.0, 24.0),
        Vector3::new(25.0, 101.0, 25.0),
    );
    assert!(!no_collision(world, &zombie, inside_the_wall));
    assert!(no_collision(
        world,
        &zombie,
        inside_the_wall.shift(Vector3::new(0.0, 1.0, 0.0))
    ));
    assert!(is_face_sturdy(
        world,
        &pos,
        BlockDirection::Up,
        SupportType::Full
    ));
}

/// Scenario "A half-height mod block", for an entity that the server moves and for the box of a
/// player that stands on the block.
fn a_half_height_mod_block(world: &Arc<World>) {
    let pos = BlockPos::new(40, 100, 40);
    load(world, &pos);
    clear_above(world, &pos);
    let half = Block::from_name("shapemod:half").unwrap().default_state;
    world.set_block_state(&pos, half.id, BlockFlags::empty());

    assert_eq!(
        corners(&get_state_shape(half, ShapeKind::Collision)),
        [[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]
    );
    assert_eq!(
        corners(&get_block_shape(world, &pos, ShapeKind::Outline, None)),
        [[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]
    );

    // Entity collision: a zombie that falls onto the block stops half a block above it.
    let zombie = Entity::new(
        world.clone(),
        Vector3::new(40.5, 102.0, 40.5),
        &EntityType::ZOMBIE,
    );
    zombie.move_entity(&zombie, Vector3::new(0.0, -3.0, 0.0));
    assert!(
        (zombie.pos.load().y - 100.5).abs() < 1e-9,
        "{}",
        zombie.pos.load().y
    );
    assert!(zombie.on_ground.load(std::sync::atomic::Ordering::Relaxed));

    // A player box that stands on the block is free, one that sinks into it is not.
    let player = support::player("HalfStander");
    let standing = BoundingBox::new(
        Vector3::new(40.2, 100.5, 40.2),
        Vector3::new(40.8, 102.3, 40.8),
    );
    assert!(no_collision(
        world,
        player.as_ref() as &dyn EntityBase,
        standing
    ));
    assert!(world.is_space_empty(standing));
    let sinking = standing.shift(Vector3::new(0.0, -0.1, 0.0));
    assert!(!no_collision(
        world,
        player.as_ref() as &dyn EntityBase,
        sinking
    ));

    // Ray traces: a ray from above hits the top of the shape.
    let hit = world
        .ray_trace_block(
            Vector3::new(40.5, 103.0, 40.5),
            Vector3::new(40.5, 99.0, 40.5),
            false,
        )
        .unwrap();
    assert_eq!(hit.0, pos);
    assert_eq!(hit.1, BlockDirection::Up);
    assert!((hit.2.y - 100.5).abs() < 1e-9, "{}", hit.2.y);

    // Support checks: the bottom face is full, the top face holds nothing that needs a full face.
    assert!(is_face_sturdy(
        world,
        &pos,
        BlockDirection::Down,
        SupportType::Full
    ));
    assert!(!is_face_sturdy(
        world,
        &pos,
        BlockDirection::Up,
        SupportType::Full
    ));
    assert!(!is_face_sturdy(
        world,
        &pos,
        BlockDirection::Up,
        SupportType::Center
    ));
    assert!(!half.is_full_cube());
}
