//! Forcing a chunk loads it and holds it, as `/forceload add` and the plugin call do.
#![expect(clippy::unwrap_used, reason = "a test fails on the first error")]

mod support;

use std::{
    thread::sleep,
    time::{Duration, Instant},
};

use pumpkin_core::command::CommandSender;
use pumpkin_util::math::vector2::Vector2;
use pumpkin_world::chunk_system::ChunkLoading;

fn wait_until_loaded(chunk: Vector2<i32>) {
    let world = support::world();
    let deadline = Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&chunk) {
        assert!(Instant::now() < deadline, "the forced chunk did not load");
        sleep(Duration::from_millis(10));
    }
}

#[test]
fn forcing_a_chunk_loads_it() {
    let _serial = support::serial();
    let world = support::world();
    let chunk = Vector2::new(30, 30);

    assert!(world.set_chunk_forced(chunk, true));
    assert!(!world.set_chunk_forced(chunk, true));
    assert!(world.forced_chunks.lock().unwrap().contains(&chunk));
    wait_until_loaded(chunk);

    assert!(world.set_chunk_forced(chunk, false));
    assert!(!world.set_chunk_forced(chunk, false));
    assert!(!world.forced_chunks.lock().unwrap().contains(&chunk));
}

/// A forced chunk that no player is near ticks: entities, block entities, scheduled ticks and
/// random ticks all run in the active chunks.
#[test]
fn a_forced_chunk_without_a_player_is_active() {
    let _serial = support::serial();
    let world = support::world();
    let chunk = Vector2::new(32, 32);

    world.set_chunk_forced(chunk, true);
    wait_until_loaded(chunk);
    world.update_active_chunks();

    assert!(world.active_chunks.read().unwrap().contains(&chunk));
}

#[test]
fn concurrent_changes_of_one_chunk_keep_the_ticket_and_the_set_in_agreement() {
    let _serial = support::serial();
    let world = support::world();
    let chunk = Vector2::new(33, 33);

    std::thread::scope(|scope| {
        for offset in 0..4 {
            let world = &world;
            scope.spawn(move || {
                for step in 0..500 {
                    world.set_chunk_forced(chunk, (step + offset) % 2 == 0);
                }
            });
        }
    });

    let forced = world.forced_chunks.lock().unwrap().contains(&chunk);
    let tickets = world
        .level
        .chunk_loading
        .lock()
        .unwrap()
        .ticket
        .get(&chunk)
        .map_or(0, |levels| {
            levels
                .iter()
                .filter(|level| **level == ChunkLoading::FULL_CHUNK_LEVEL)
                .count()
        });
    assert_eq!(tickets, usize::from(forced));
}

fn run(command: &str) -> Result<i32, String> {
    let server = support::server();
    let source = CommandSender::Console.into_source(server).with_silent();
    server
        .command_dispatcher
        .load()
        .execute_input(command, &source)
        .map_err(|error| format!("{:?}", error.error_type))
}

#[test]
fn forceload_rejects_a_huge_area_and_coordinates_outside_the_world() {
    let _serial = support::serial();

    let huge = run("forceload add 0 0 1048575 1048575").unwrap_err();
    assert!(huge.contains("commands.forceload.toobig"), "{huge}");
    let outside = run("forceload add 30000000 0").unwrap_err();
    assert!(outside.contains("argument.pos.outofworld"), "{outside}");
    assert!(support::world().forced_chunks.lock().unwrap().is_empty());
}
