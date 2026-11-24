//! The per-tick cost of the ticking chunk sets and of the chunk tick: `update_active_chunks` when
//! nothing changed, and `tick_chunks` over loaded empty chunks. 441 forced chunks (21 x 21) stand
//! for the area of one player at simulation distance 10.
#![expect(clippy::unwrap_used, reason = "a bench stops on a setup failure")]

use std::sync::{Arc, Weak};

use arc_swap::ArcSwap;
use criterion::{Criterion, criterion_group, criterion_main};
use pumpkin_config::world::LevelConfig;
use pumpkin_core::{block::registry::default_registry, world::World};
use pumpkin_data::dimension::Dimension;
use pumpkin_util::{math::vector2::Vector2, world_seed::Seed};
use pumpkin_world::{chunk::ChunkData, level::Level, world_info::LevelData};

const RADIUS: i32 = 10;

fn bench(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let folder = tempfile::tempdir().unwrap();
    let world = {
        let _guard = runtime.enter();
        let dimension = Dimension::OVERWORLD.clone();
        Arc::new(World::load(
            Level::from_root_folder(
                &LevelConfig::default(),
                folder.path().to_path_buf(),
                0,
                dimension.clone(),
            ),
            Arc::new(ArcSwap::from_pointee(LevelData::default(Seed(0)))),
            dimension,
            default_registry(),
            Weak::new(),
        ))
    };
    let entered = runtime.enter();

    let mut group = c.benchmark_group("chunk_tick");
    group.bench_function("update_active_chunks_empty", |b| {
        b.iter(|| world.update_active_chunks());
    });

    // Loaded chunks one ring past the area, so the block ticking ring is loaded too.
    for x in -RADIUS - 1..=RADIUS + 1 {
        for z in -RADIUS - 1..=RADIUS + 1 {
            world
                .level
                .loaded_chunks
                .insert(Vector2::new(x, z), ChunkData::empty_sync(x, z));
        }
    }
    {
        let mut forced = world.forced_chunks.lock().unwrap();
        for x in -RADIUS..=RADIUS {
            for z in -RADIUS..=RADIUS {
                forced.insert(Vector2::new(x, z));
            }
        }
    }
    world.update_active_chunks();

    group.bench_function("update_active_chunks_441_forced", |b| {
        b.iter(|| world.update_active_chunks());
    });
    group.bench_function("tick_chunks_441_forced", |b| {
        b.iter(|| world.tick_chunks());
    });
    group.finish();
    drop(entered);
    runtime.block_on(world.level.shutdown());
}

criterion_group!(benches, bench);
criterion_main!(benches);
