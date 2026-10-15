//! The cost of a fire site of a per-tick event: with no handler at all, with a handler on another
//! event only (a server with any mod hashes the event name), and with one no-op handler. The fire
//! site is the one of `LevelTickEventPre`: `fire_blocking_with` builds the event only when a
//! handler listens.

#[path = "../tests/support/mod.rs"]
mod support;

use std::{hint::black_box, sync::Arc};

use criterion::{Criterion, criterion_group, criterion_main};
use pumpkin_core::{
    plugin::{
        BoxFuture, EventHandler, EventPriority, PluginManager,
        world::level_tick::{LevelTickEventPost, LevelTickEventPre},
    },
    server::Server,
};

struct NoOp;

impl EventHandler<LevelTickEventPre> for NoOp {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        _event: &'a mut LevelTickEventPre,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}

impl EventHandler<LevelTickEventPost> for NoOp {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        _event: &'a mut LevelTickEventPost,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}

fn bench_event_dispatch(c: &mut Criterion) {
    let server = support::server();
    let world = support::world();
    let manager = PluginManager::new(Vec::new());
    let mut group = c.benchmark_group("event_dispatch");

    group.bench_function("zero_handlers", |b| {
        b.iter(|| {
            black_box(manager.fire_blocking_with(server, || LevelTickEventPre::new(world.clone())))
        });
    });

    let other_event = PluginManager::new(Vec::new());
    other_event.register_with::<LevelTickEventPost, _>(
        Arc::new(NoOp),
        EventPriority::Normal,
        true,
        false,
    );
    assert!(!other_event.has_handlers::<LevelTickEventPre>());
    group.bench_function("zero_handlers_other_event_registered", |b| {
        b.iter(|| {
            black_box(
                other_event.fire_blocking_with(server, || LevelTickEventPre::new(world.clone())),
            )
        });
    });

    manager.register_with::<LevelTickEventPre, _>(
        Arc::new(NoOp),
        EventPriority::Normal,
        true,
        false,
    );
    assert!(manager.has_handlers::<LevelTickEventPre>());
    group.bench_function("one_noop_handler", |b| {
        b.iter(|| {
            black_box(manager.fire_blocking_with(server, || LevelTickEventPre::new(world.clone())))
        });
    });
    group.finish();
}

criterion_group!(benches, bench_event_dispatch);
criterion_main!(benches);
