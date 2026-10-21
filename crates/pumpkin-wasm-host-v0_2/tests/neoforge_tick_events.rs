//! The entity and player tick events of `NeoForge` 26.3 reach v0.2 plugins (spec
//! `neoforge-event-bus`, requirement "Wasm plugins see the same events", #131), one event per
//! entity or player and tick, behind the handler check of the fire site. The cost per event is in
//! `benches/tick_events.rs`.
#![expect(clippy::panic, reason = "a test fails on the first unexpected record")]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

mod guest;

use std::sync::{Arc, Mutex, PoisonError};

use guest::{fire, plugin, resource};
use pumpkin_core::{
    entity::r#type::from_type,
    plugin::level::{
        EntityTickEventPost, EntityTickEventPre, PlayerTickEventPost, PlayerTickEventPre,
    },
};
use pumpkin_data::entity::EntityType;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_wasm_host_v0_2::pumpkin::plugin::event::Event;

#[test]
fn a_plugin_cancels_an_entity_tick() {
    let world = support::world();
    let zombie = from_type(
        &EntityType::ZOMBIE,
        Vector3::new(0.5, 70.0, 0.5),
        &world,
        uuid::Uuid::new_v4(),
    );
    let zombie_id = zombie.get_entity().entity_id;
    let manager = plugin::<EntityTickEventPre, _>(move |event, state| {
        let Event::EntityTickEventPre(data) = event else {
            panic!("expected entity tick pre");
        };
        assert_eq!(
            resource(state, &data.entity).get_entity().entity_id,
            zombie_id
        );
        assert!(!data.cancelled);
        data.cancelled = true;
    });

    let event = fire(&manager, EntityTickEventPre::new(zombie));

    assert!(event.cancelled);
}

#[test]
fn a_plugin_receives_the_entity_and_player_tick_events() {
    let world = support::world();
    let player = support::player("Ticker");
    let cow = from_type(
        &EntityType::COW,
        Vector3::new(2.5, 70.0, 0.5),
        &world,
        uuid::Uuid::new_v4(),
    );
    let cow_id = cow.get_entity().entity_id;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = |seen: &Arc<Mutex<Vec<String>>>, entry: String| {
        seen.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(entry);
    };

    let record = seen.clone();
    let post = plugin::<EntityTickEventPost, _>(move |event, state| {
        if let Event::EntityTickEventPost(data) = event {
            let id = resource(state, &data.entity).get_entity().entity_id;
            log(&record, format!("entity post {}", id == cow_id));
        }
    });
    fire(&post, EntityTickEventPost::new(cow));

    let record = seen.clone();
    let pre = plugin::<PlayerTickEventPre, _>(move |event, state| {
        if let Event::PlayerTickEventPre(data) = event {
            let name = resource(state, &data.player).gameprofile.name.clone();
            log(&record, format!("player pre {name}"));
        }
    });
    fire(&pre, PlayerTickEventPre::new(player.clone()));

    let record = seen.clone();
    let player_post = plugin::<PlayerTickEventPost, _>(move |event, state| {
        if let Event::PlayerTickEventPost(data) = event {
            let name = resource(state, &data.player).gameprofile.name.clone();
            log(&record, format!("player post {name}"));
        }
    });
    fire(&player_post, PlayerTickEventPost::new(player));

    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        [
            "entity post true",
            "player pre Ticker",
            "player post Ticker"
        ]
    );
}
