//! The cost of one tick event fire with a v0.2 listener, per entity or player and tick: with no
//! handler, with a v0.2 handler that converts the event to its WIT record and back, and with that
//! conversion inside a guest call of the plugin `StoreExecutor`, as `WasmPluginEventHandler`
//! does it. The store has no component, so the last case leaves out only the lowering of the
//! record into the guest and the code of the guest.
#![expect(clippy::expect_used, reason = "the bench stops on a setup error")]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

#[path = "../tests/guest/mod.rs"]
mod guest;

use std::{hint::black_box, marker::PhantomData, sync::Arc};

use criterion::{Criterion, criterion_group, criterion_main};
use pumpkin_core::{
    entity::r#type::from_type,
    plugin::{
        BoxFuture, EventHandler, EventPriority, Payload, PluginManager,
        level::{EntityTickEventPre, PlayerTickEventPre},
    },
    server::Server,
};
use pumpkin_data::entity::EntityType;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_wasm_host_common::{
    concurrent_store::{LegacyStore, LegacySyncReentry, TokioSpawner, start_legacy_store},
    state::PluginHostState,
};
use pumpkin_wasm_host_v0_2::events::ToFromWasmEvent;
use wasmtime::{Config, Engine, Store};

/// A v0.2 handler whose guest returns the record unchanged, run through the plugin store.
struct StoreGuest<E> {
    store: LegacyStore,
    _event: PhantomData<fn() -> E>,
}

impl<E> EventHandler<E> for StoreGuest<E>
where
    E: Payload + ToFromWasmEvent + Clone + Send + Sync + 'static,
{
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut E,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let mut pending = event.clone();
            let updated = self
                .store
                .call_guest(move |mut guest| {
                    Box::pin(async move {
                        Ok(guest.with(|mut store| {
                            let state = store.data_mut();
                            let wasm = pending.to_wasm_event(state);
                            pending.apply_wasm_event(wasm, state);
                            pending
                        }))
                    })
                })
                .await;
            if let Ok(updated) = updated {
                *event = updated;
            }
        })
    }
}

fn plugin_store() -> LegacyStore {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    let engine = Engine::new(&config).expect("engine");
    support::RUNTIME.block_on(async {
        start_legacy_store(
            Store::new(&engine, PluginHostState::new()),
            LegacySyncReentry::new(),
            Arc::new(TokioSpawner::new(tokio::runtime::Handle::current())),
        )
        .await
        .expect("plugin store")
    })
}

fn store_plugin<E>() -> PluginManager
where
    E: Payload + ToFromWasmEvent + Clone + Send + Sync + 'static,
{
    let manager = PluginManager::new(Vec::new());
    manager.register_with::<E, _>(
        Arc::new(StoreGuest {
            store: plugin_store(),
            _event: PhantomData,
        }),
        EventPriority::Normal,
        true,
        true,
    );
    manager
}

fn bench_tick_events(c: &mut Criterion) {
    let server = support::server();
    let world = support::world();
    let zombie = from_type(
        &EntityType::ZOMBIE,
        Vector3::new(0.5, 70.0, 0.5),
        &world,
        uuid::Uuid::new_v4(),
    );
    let player = support::player("BenchTicker");
    let mut group = c.benchmark_group("tick_events");

    let none = PluginManager::new(Vec::new());
    group.bench_function("entity_tick_pre_zero_handlers", |b| {
        b.iter(|| {
            black_box(none.fire_blocking_with(server, || EntityTickEventPre::new(zombie.clone())))
        });
    });

    let conversion = guest::plugin::<EntityTickEventPre, _>(|_event, _state| {});
    group.bench_function("entity_tick_pre_v0_2_conversion", |b| {
        b.iter(|| {
            black_box(
                conversion.fire_blocking_with(server, || EntityTickEventPre::new(zombie.clone())),
            )
        });
    });

    let store = store_plugin::<EntityTickEventPre>();
    group.bench_function("entity_tick_pre_v0_2_store_round_trip", |b| {
        b.iter(|| {
            black_box(store.fire_blocking_with(server, || EntityTickEventPre::new(zombie.clone())))
        });
    });

    let player_store = store_plugin::<PlayerTickEventPre>();
    group.bench_function("player_tick_pre_v0_2_store_round_trip", |b| {
        b.iter(|| {
            black_box(
                player_store.fire_blocking_with(server, || PlayerTickEventPre::new(player.clone())),
            )
        });
    });
    group.finish();
}

criterion_group!(benches, bench_tick_events);
criterion_main!(benches);
