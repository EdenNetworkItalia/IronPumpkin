//! Forcing a chunk loads it and holds it, as `/forceload add` and the plugin call do.
#![expect(clippy::unwrap_used, reason = "a test fails on the first error")]

mod support;

use std::{
    thread::sleep,
    time::{Duration, Instant},
};

use pumpkin_util::math::vector2::Vector2;

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
