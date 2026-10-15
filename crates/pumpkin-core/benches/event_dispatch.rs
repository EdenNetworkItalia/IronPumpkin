//! The cost of a fire site of a per-tick event, with no handler and with one no-op handler. The
//! fire site is the one of `LevelTickEventPre`: `fire_blocking_with` builds the event only when a
//! handler listens.

#[path = "../tests/support/mod.rs"]
mod support;

use std::{hint::black_box, sync::Arc};

use criterion::{Criterion, criterion_group, criterion_main};
use pumpkin_core::{
    plugin::{
        BoxFuture, EventHandler, EventPriority, PluginManager, world::level_tick::LevelTickEventPre,
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
