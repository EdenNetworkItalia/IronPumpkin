//! `register_neoforge_event` registers a v0.2 handler for the native event that each `EventType`
//! names, and for no other (#131). This file is its own test binary, so no other test registers
//! handlers on the shared server.
#![expect(clippy::expect_used, reason = "a test fails on the first setup error")]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

use std::sync::Arc;

use pumpkin_core::plugin::{
    EventPriority, Payload, PluginManager, PluginMetadata,
    level::{
        BlockEventBlockToolModificationEvent, BlockEventEntityPlaceEvent, ChunkWatchEventUnWatch,
        CreateFluidSourceEvent, CropGrowEventPost, EntityTickEventPost, EntityTickEventPre,
        ExplosionEventDetonate, ExplosionKnockbackEvent, GameRuleChangedEvent,
        LevelEventPotentialSpawns, NoteBlockEventChange, PistonEventPost, PlayerTickEventPost,
        PlayerTickEventPre,
    },
    player::use_item_on_block::UseItemOnBlockEvent,
    startup::create_context,
};

use pumpkin_core::plugin::level::{
    EndermanAngerEvent, EntityInvulnerabilityCheckEvent, EntityLeaveLevelEvent,
    EntityMobGriefingEvent, EntityStruckByLightningEvent, EntityTeleportEventSpreadPlayersCommand,
    MobDespawnEvent, MobSpawnEventPositionCheck, MobSpawnEventSpawnPlacementCheck, MobSplitEvent,
    ProjectileImpactEvent, SpawnClusterSizeEvent,
};
use pumpkin_wasm_host_common::{
    concurrent_store::{LegacySyncReentry, TokioSpawner, start_legacy_store},
    plugin::{PluginApiVersion, WasmPlugin},
    scheduler::TaskScheduler,
    state::PluginHostState,
};
use pumpkin_wasm_host_v0_2::{
    events::{
        WasmPluginEventHandler, neoforge::register_neoforge_event,
        neoforge_entity::register_neoforge_entity_event,
    },
    pumpkin::plugin::event::EventType,
};
use wasmtime::{Config, Engine, Store};

/// A v0.2 plugin with a store and no component: enough to register a handler.
fn handler() -> Arc<WasmPluginEventHandler> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    let engine = Engine::new(&config).expect("engine");
    let store = support::RUNTIME.block_on(async {
        start_legacy_store(
            Store::new(&engine, PluginHostState::new()),
            LegacySyncReentry::new(),
            Arc::new(TokioSpawner::new(tokio::runtime::Handle::current())),
        )
        .await
        .expect("plugin store")
    });
    Arc::new(WasmPluginEventHandler {
        handler_id: 0,
        plugin: Arc::new(WasmPlugin {
            plugin_instance: Arc::new(()),
            api_version: PluginApiVersion::V0_2,
            store,
            task_scheduler: Arc::new(TaskScheduler::new()),
        }),
    })
}

/// Whether the manager has a handler of one event type.
type Listens = fn(&PluginManager) -> bool;

fn listens<E: Payload + 'static>(manager: &PluginManager) -> bool {
    manager.has_handlers::<E>()
}

#[test]
fn each_event_type_registers_its_own_event() {
    let server = support::server();
    let context = create_context(
        server,
        PluginMetadata {
            name: "v0.2-routing-plugin".to_owned(),
            version: "1.0.0".to_owned(),
            authors: Vec::new(),
            description: "v0.2 routing test plugin".to_owned(),
            dependencies: Vec::new(),
            permissions: Vec::new(),
        },
    );
    let handler = handler();
    let cases: [(EventType, Listens); 16] = [
        (
            EventType::UseItemOnBlockEvent,
            listens::<UseItemOnBlockEvent>,
        ),
        (
            EventType::BlockEventEntityPlaceEvent,
            listens::<BlockEventEntityPlaceEvent>,
        ),
        (
            EventType::BlockEventBlockToolModificationEvent,
            listens::<BlockEventBlockToolModificationEvent>,
        ),
        (EventType::CropGrowEventPost, listens::<CropGrowEventPost>),
        (
            EventType::CreateFluidSourceEvent,
            listens::<CreateFluidSourceEvent>,
        ),
        (
            EventType::NoteBlockEventChange,
            listens::<NoteBlockEventChange>,
        ),
        (EventType::PistonEventPost, listens::<PistonEventPost>),
        (
            EventType::ExplosionEventDetonate,
            listens::<ExplosionEventDetonate>,
        ),
        (
            EventType::ExplosionKnockbackEvent,
            listens::<ExplosionKnockbackEvent>,
        ),
        (
            EventType::ChunkWatchEventUnWatch,
            listens::<ChunkWatchEventUnWatch>,
        ),
        (
            EventType::GameRuleChangedEvent,
            listens::<GameRuleChangedEvent>,
        ),
        (
            EventType::LevelEventPotentialSpawns,
            listens::<LevelEventPotentialSpawns>,
        ),
        (EventType::EntityTickEventPre, listens::<EntityTickEventPre>),
        (
            EventType::EntityTickEventPost,
            listens::<EntityTickEventPost>,
        ),
        (EventType::PlayerTickEventPre, listens::<PlayerTickEventPre>),
        (
            EventType::PlayerTickEventPost,
            listens::<PlayerTickEventPost>,
        ),
    ];
    let manager = &server.plugin_manager;
    assert!(cases.iter().all(|(_, listens)| !listens(manager)));

    for (registered, (event_type, _)) in cases.iter().enumerate() {
        register_neoforge_event(&context, &handler, EventPriority::Normal, true, *event_type);
        for (index, (other, listens)) in cases.iter().enumerate() {
            assert_eq!(
                listens(manager),
                index <= registered,
                "after {event_type:?}, the handlers of {other:?}"
            );
        }
    }
}

/// The entity events of #138 route through `register_neoforge_entity_event` the same way.
#[test]
fn each_entity_event_type_registers_its_own_event() {
    let server = support::server();
    let context = create_context(
        server,
        PluginMetadata {
            name: "v0.2-entity-routing-plugin".to_owned(),
            version: "1.0.0".to_owned(),
            authors: Vec::new(),
            description: "v0.2 entity routing test plugin".to_owned(),
            dependencies: Vec::new(),
            permissions: Vec::new(),
        },
    );
    let handler = handler();
    let cases: [(EventType, Listens); 12] = [
        (
            EventType::EntityLeaveLevelEvent,
            listens::<EntityLeaveLevelEvent>,
        ),
        (
            EventType::EntityMobGriefingEvent,
            listens::<EntityMobGriefingEvent>,
        ),
        (
            EventType::EntityStruckByLightningEvent,
            listens::<EntityStruckByLightningEvent>,
        ),
        (
            EventType::EntityTeleportEventSpreadPlayersCommand,
            listens::<EntityTeleportEventSpreadPlayersCommand>,
        ),
        (
            EventType::ProjectileImpactEvent,
            listens::<ProjectileImpactEvent>,
        ),
        (EventType::EndermanAngerEvent, listens::<EndermanAngerEvent>),
        (EventType::MobDespawnEvent, listens::<MobDespawnEvent>),
        (
            EventType::MobSpawnEventPositionCheck,
            listens::<MobSpawnEventPositionCheck>,
        ),
        (
            EventType::MobSpawnEventSpawnPlacementCheck,
            listens::<MobSpawnEventSpawnPlacementCheck>,
        ),
        (EventType::MobSplitEvent, listens::<MobSplitEvent>),
        (
            EventType::SpawnClusterSizeEvent,
            listens::<SpawnClusterSizeEvent>,
        ),
        (
            EventType::EntityInvulnerabilityCheckEvent,
            listens::<EntityInvulnerabilityCheckEvent>,
        ),
    ];
    let manager = &server.plugin_manager;
    assert!(cases.iter().all(|(_, listens)| !listens(manager)));

    for (registered, (event_type, _)) in cases.iter().enumerate() {
        register_neoforge_entity_event(
            &context,
            &handler,
            EventPriority::Normal,
            true,
            *event_type,
        );
        for (index, (other, listens)) in cases.iter().enumerate() {
            assert_eq!(
                listens(manager),
                index <= registered,
                "after {event_type:?}, the handlers of {other:?}"
            );
        }
    }
}
