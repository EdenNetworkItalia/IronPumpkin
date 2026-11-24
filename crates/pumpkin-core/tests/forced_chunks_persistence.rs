//! The forced chunks of a world survive a save and a reload of the world.
#![expect(clippy::unwrap_used, reason = "a test fails on the first error")]

mod support;

use std::sync::Arc;

use pumpkin_config::world::LevelConfig;
use pumpkin_core::world::World;
use pumpkin_data::dimension::Dimension;
use pumpkin_util::math::vector2::Vector2;
use pumpkin_world::world_info::chunk_tickets::chunk_tickets_path;

fn reload(world: &World) -> World {
    let _runtime = support::RUNTIME.enter();
    let level = pumpkin_world::level::Level::from_root_folder(
        &LevelConfig::default(),
        world.level.level_folder.root_folder.clone(),
        0,
        Dimension::OVERWORLD,
    );
    World::load(
        level,
        world.level_info.clone(),
        Dimension::OVERWORLD,
        world.block_registry.clone(),
        Arc::downgrade(support::server()),
    )
}

#[test]
fn forced_chunks_survive_a_save_and_a_reload() {
    let _serial = support::serial();
    let world = support::world();
    let forced = [Vector2::new(40, -12), Vector2::new(-7, 3)];
    world.forced_chunks.lock().unwrap().extend(forced);

    support::RUNTIME.block_on(world.save());

    assert!(chunk_tickets_path(&world.level.level_folder.dim_folder).exists());
    let reloaded = reload(&world).forced_chunks.lock().unwrap().clone();
    assert!(forced.iter().all(|pos| reloaded.contains(pos)));

    world.forced_chunks.lock().unwrap().remove(&forced[0]);
    support::RUNTIME.block_on(world.save());
    let reloaded = reload(&world).forced_chunks.lock().unwrap().clone();
    assert!(!reloaded.contains(&forced[0]));
    assert!(reloaded.contains(&forced[1]));
}

#[test]
fn a_forced_chunk_of_the_save_gets_loaded() {
    let _serial = support::serial();
    let world = support::world();
    let chunk = Vector2::new(25, 25);
    world.forced_chunks.lock().unwrap().insert(chunk);

    world.load_forced_chunks();

    let deadline = std::time::Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&chunk) {
        assert!(
            std::time::Instant::now() < deadline,
            "the forced chunk did not load"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
