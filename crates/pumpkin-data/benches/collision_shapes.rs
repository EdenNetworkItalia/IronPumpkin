//! The shape reads of the entity collision loop (`World::get_block_collisions`) and of the
//! outline ray trace, over a fixed patch of vanilla terrain.
#![allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::print_stdout
)]

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use pumpkin_data::{Block, BlockState};
use pumpkin_util::math::{boundingbox::BoundingBox, position::BlockPos, vector3::Vector3};

const SIZE: i32 = 6;

/// A 6x6x6 patch: three layers of ground, one layer of plants and half blocks, two of air.
fn patch() -> Vec<(BlockPos, &'static BlockState)> {
    let ground = [
        Block::STONE.default_state,
        Block::DIRT.default_state,
        Block::GRASS_BLOCK.default_state,
    ];
    let surface = [
        Block::OAK_SLAB.default_state,
        Block::OAK_STAIRS.default_state,
        Block::OAK_FENCE.default_state,
        Block::SHORT_GRASS.default_state,
        Block::BAMBOO.default_state,
        Block::WATER.default_state,
        Block::AIR.default_state,
        Block::WHITE_CARPET.default_state,
    ];
    let mut blocks = Vec::new();
    for x in 0..SIZE {
        for y in 0..SIZE {
            for z in 0..SIZE {
                let state = match y {
                    0..=2 => ground[((x + z) % 3) as usize],
                    3 => surface[((x * SIZE + z) % surface.len() as i32) as usize],
                    _ => Block::AIR.default_state,
                };
                blocks.push((BlockPos::new(x, y, z), state));
            }
        }
    }
    blocks
}

#[inline(never)]
fn collisions(blocks: &[(BlockPos, &'static BlockState)], entity: &BoundingBox) -> usize {
    let mut collisions = Vec::new();
    for (pos, state) in blocks {
        if state.is_air() {
            continue;
        }
        for shape in state.get_block_collision_shapes_at(pos) {
            let shape = shape.at_pos(*pos);
            if shape.intersects(entity) {
                collisions.push(shape);
            }
        }
    }
    collisions.len()
}

#[inline(never)]
fn outlines(blocks: &[(BlockPos, &'static BlockState)], ray: &BoundingBox) -> usize {
    let mut hits = 0;
    for (pos, state) in blocks {
        for shape in state.get_block_outline_shapes_at(pos) {
            if shape.at_pos(*pos).intersects(ray) {
                hits += 1;
            }
        }
    }
    hits
}

pub fn bench_collision_shapes(c: &mut Criterion) {
    let blocks = patch();
    let size = f64::from(SIZE);
    // Covers the whole patch, so every shape is read and tested.
    let probe = BoundingBox::new(
        Vector3::new(0.1, 0.1, 0.1),
        Vector3::new(size - 0.1, 3.9, size - 0.1),
    );
    assert!(collisions(&blocks, &probe) > 0);

    c.bench_function("collision_shapes/get_block_collisions_loop", |b| {
        b.iter(|| collisions(black_box(&blocks), black_box(&probe)))
    });
    c.bench_function("collision_shapes/outline_loop", |b| {
        b.iter(|| outlines(black_box(&blocks), black_box(&probe)))
    });
}

criterion_group!(benches, bench_collision_shapes);
criterion_main!(benches);
