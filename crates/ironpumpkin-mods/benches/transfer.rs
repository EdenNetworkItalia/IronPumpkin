//! The cost of transactional transfers: one thread on one mod storage, eight threads on one
//! shared storage (the journals of the storage are contended), and eight threads on eight
//! independent storages (no shared journal). Each iteration is one root transaction that inserts
//! one item and extracts it again, then commits.

use std::{
    hint::black_box,
    sync::{Arc, Barrier},
    thread,
    time::{Duration, Instant},
};

use criterion::{Criterion, criterion_group, criterion_main};
use ironpumpkin_mods::transfer::{
    ItemResource, ItemStacksResourceHandler, ResourceHandler, Transaction,
};
use pumpkin_data::item::Item;

const THREADS: usize = 8;

fn transfer(handler: &ItemStacksResourceHandler, stone: &ItemResource) -> u32 {
    let Ok(mut transaction) = Transaction::open_root() else {
        return 0;
    };
    let inserted = handler.insert(stone, 1, &mut transaction);
    let extracted = handler.extract(stone, inserted, &mut transaction);
    transaction.commit();
    inserted + extracted
}

/// Runs `iters` transfers on each of `THREADS` threads; thread `t` uses `handlers[t % len]`.
fn parallel(handlers: &[Arc<ItemStacksResourceHandler>], iters: u64) -> Duration {
    let barrier = Arc::new(Barrier::new(THREADS + 1));
    let workers: Vec<_> = (0..THREADS)
        .map(|t| {
            let handler = handlers[t % handlers.len()].clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                let stone = ItemResource::of_item(&Item::STONE);
                barrier.wait();
                for _ in 0..iters {
                    black_box(transfer(&handler, &stone));
                }
            })
        })
        .collect();
    barrier.wait();
    let start = Instant::now();
    for worker in workers {
        let _ = worker.join();
    }
    start.elapsed()
}

fn storage() -> Arc<ItemStacksResourceHandler> {
    Arc::new(ItemStacksResourceHandler::new(9, 64))
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("transfer");
    let one = storage();
    let stone = ItemResource::of_item(&Item::STONE);
    group.bench_function("uncontended", |b| {
        b.iter(|| black_box(transfer(&one, &stone)));
    });
    let shared = [storage()];
    group.bench_function("parallel_one_storage", |b| {
        b.iter_custom(|iters| parallel(&shared, iters));
    });
    let independent: Vec<_> = (0..THREADS).map(|_| storage()).collect();
    group.bench_function("parallel_independent_storages", |b| {
        b.iter_custom(|iters| parallel(&independent, iters));
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
