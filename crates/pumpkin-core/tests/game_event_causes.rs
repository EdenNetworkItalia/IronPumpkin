//! The context of a vanilla game event (`GameEvent.Context`): `VanillaGameEvent.getCause()` is the
//! entity of the context, and the context also holds the affected block state. One test for each
//! group of emit sites: an entity that emits for itself, a block, a block entity, a dispenser and
//! a cauldron. A last test covers an entity that emits on its removal path.
//!
//! Each test works at its own x position and filters its handler by it, because the handlers stay
//! registered on the shared server.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use pumpkin_core::{
    block::{
        BlockBehaviour, OnScheduledTickArgs,
        blocks::{cauldron::CauldronBlock, redstone::dispenser::DispenserBlock, tnt::TNTBlock},
        entities::{BlockEntity, jukebox::JukeboxBlockEntity},
    },
    entity::{EntityBase, r#type::from_type},
    plugin::{BoxFuture, EventHandler, EventPriority, Payload, world::GenericGameEvent},
    server::Server,
    world::World,
};
use pumpkin_data::{
    Block, BlockId, BlockStateId,
    block_properties::{JukeboxLikeProperties, WaterCauldronLikeProperties},
    damage::DamageType,
    entity::EntityType,
    game_event::GameEvent,
    item::Item,
    item_stack::ItemStack,
};
use pumpkin_inventory::Inventory;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use pumpkin_world::world::BlockFlags;

struct Blocking<E, F>(F, PhantomData<fn() -> E>);

impl<E, F> EventHandler<E> for Blocking<E, F>
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&mut E) + Send + Sync,
{
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut E,
    ) -> BoxFuture<'a, ()> {
        (self.0)(event);
        Box::pin(async {})
    }
}

/// A game event as the handler sees it: the key, the id of the cause and the affected state.
type Seen = (String, Option<i32>, Option<BlockStateId>);

/// Records the game events at `x`, the position of the test.
fn watch(x: f64) -> Arc<Mutex<Vec<Seen>>> {
    let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
    let record = seen.clone();
    support::server()
        .plugin_manager
        .register_with::<GenericGameEvent, _>(
            Arc::new(Blocking(
                move |event: &mut GenericGameEvent| {
                    if event.position().x == x {
                        record.lock().unwrap().push((
                            event.event_key().to_owned(),
                            event
                                .cause
                                .as_ref()
                                .map(|cause| cause.get_entity().entity_id),
                            event.affected_state(),
                        ));
                    }
                },
                PhantomData,
            )),
            EventPriority::Normal,
            true,
            false,
        );
    seen
}

fn taken(seen: &Mutex<Vec<Seen>>) -> Vec<Seen> {
    std::mem::take(&mut *seen.lock().unwrap())
}

/// Loads the chunk of `pos` for the rest of the process and places `block` there.
fn place(world: &Arc<World>, pos: BlockPos, block: &Block) {
    let chunk = pos.chunk_position();
    {
        let mut loading = world.level.chunk_loading.lock().unwrap();
        loading.add_force_ticket(chunk);
        loading.send_change();
    };
    let deadline = Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&chunk) {
        assert!(Instant::now() < deadline, "the chunk of {pos} did not load");
        std::thread::sleep(Duration::from_millis(10));
    }
    world.set_block_state(&pos, block.default_state.id, BlockFlags::NOTIFY_ALL);
    assert_eq!(world.get_block(&pos).id, block.id);
}

/// An entity emits for itself, as `Entity.gameEvent(event)`: the cause is the entity.
#[test]
fn an_entity_self_emit_has_the_entity_as_its_cause() {
    let _serial = support::serial();
    let x = 31_000.0;
    let player = support::player("SelfEmit");
    player
        .living_entity
        .entity
        .set_pos(Vector3::new(x, 100.0, 0.5));
    let id = player.living_entity.entity.entity_id;
    let seen = watch(x);

    player
        .living_entity
        .cause_use_vibration(&ItemStack::new(1, &Item::APPLE), GameEvent::Eat);

    assert_eq!(taken(&seen), [("eat".to_owned(), Some(id), None)]);
}

/// A block break has the breaker as its cause and the broken state as its affected state, as
/// `Level.destroyBlock`. A primed TNT has the igniter as its cause, as `TntBlock.prime`.
#[test]
fn a_block_event_has_the_acting_entity_and_the_state() {
    let _serial = support::serial();
    let world = support::world();
    let player = support::player("BlockEmit");
    let id = player.living_entity.entity.entity_id;

    let x = 31_100;
    let pos = BlockPos::new(x, 100, 0);
    place(&world, pos, &Block::STONE);
    let seen = watch(f64::from(x) + 0.5);
    assert!(world.destroy_block(&pos, false, Some(player.as_ref() as &dyn EntityBase)));
    assert_eq!(
        taken(&seen),
        [(
            "block_destroy".to_owned(),
            Some(id),
            Some(Block::STONE.default_state.id)
        )]
    );

    let x = 31_102;
    let pos = BlockPos::new(x, 100, 0);
    place(&world, pos, &Block::TNT);
    let seen = watch(f64::from(x) + 0.5);
    assert!(TNTBlock::prime_by(
        &world,
        &pos,
        Some(&player.living_entity.entity)
    ));
    assert_eq!(taken(&seen), [("prime_fuse".to_owned(), Some(id), None)]);
}

/// A jukebox block entity emits `Context.of(blockState)`: no cause, and the state of the jukebox
/// after the record change.
#[test]
fn a_block_entity_event_has_the_block_state() {
    let _serial = support::serial();
    let world = support::world();
    let x = 31_200;
    let pos = BlockPos::new(x, 100, 0);
    place(&world, pos, &Block::JUKEBOX);
    let jukebox = JukeboxBlockEntity::new(pos);
    jukebox.tick(&world);
    let seen = watch(f64::from(x) + 0.5);

    jukebox.set_stack(0, ItemStack::new(1, &Item::MUSIC_DISC_13));

    let with_record = JukeboxLikeProperties { has_record: true }.to_state_id(&Block::JUKEBOX);
    assert_eq!(
        taken(&seen),
        [("block_change".to_owned(), None, Some(with_record))]
    );
    assert_eq!(world.get_block_state_id(&pos), with_record);
}

/// A dispenser with nothing to dispense emits `block_activate` with its own state.
#[test]
fn a_dispenser_event_has_the_block_state() {
    let _serial = support::serial();
    let world = support::world();
    let x = 31_300;
    let pos = BlockPos::new(x, 100, 0);
    place(&world, pos, &Block::DISPENSER);
    assert!(world.get_block_entity(&pos).is_some());
    let seen = watch(f64::from(x) + 0.5);

    DispenserBlock.on_scheduled_tick(OnScheduledTickArgs {
        world: &world,
        block: &Block::DISPENSER,
        position: &pos,
    });

    assert_eq!(
        taken(&seen),
        [(
            "block_activate".to_owned(),
            None,
            Some(world.get_block_state_id(&pos))
        )]
    );
}

/// Rain fills a cauldron: `LayeredCauldronBlock.handlePrecipitation` emits `Context.of(newState)`.
/// The fill has a chance, so the test repeats it.
#[test]
fn a_cauldron_event_has_the_new_state() {
    let _serial = support::serial();
    let world = support::world();
    let x = 31_400;
    let pos = BlockPos::new(x, 100, 0);
    place(&world, pos, &Block::WATER_CAULDRON);
    let level = |level: u8| {
        let mut props =
            WaterCauldronLikeProperties::from_state_id(Block::WATER_CAULDRON.default_state.id);
        props.level = level;
        props.to_state_id(&Block::WATER_CAULDRON)
    };
    let seen = watch(f64::from(x) + 0.5);

    let mut fills = 0;
    for _ in 0..20_000 {
        world.set_block_state(&pos, level(1), BlockFlags::NOTIFY_ALL);
        CauldronBlock::handle_precipitation(&world, &pos, BlockId::WATER_CAULDRON, level(1), true);
        if world.get_block_state_id(&pos) == level(2) {
            fills += 1;
            break;
        }
    }

    assert_eq!(fills, 1, "the rain never filled the cauldron");
    assert_eq!(
        taken(&seen),
        [("block_change".to_owned(), None, Some(level(2)))]
    );
}

/// A shulker bullet emits `entity_damage` after the world removed it, as
/// `ShulkerBullet.destroy` does (`discard`, then `gameEvent(ENTITY_DAMAGE, Context.of(this))`).
/// The cause is the bullet, although the world no longer lists it.
#[test]
fn the_cause_of_an_entity_that_emits_after_its_removal_is_the_entity() {
    let _serial = support::serial();
    let world = support::world();
    let x = 31_500.0;
    let bullet = from_type(
        &EntityType::SHULKER_BULLET,
        Vector3::new(x, 100.0, 0.5),
        &world,
        uuid::Uuid::new_v4(),
    );
    let id = bullet.get_entity().entity_id;
    world.spawn_entity(bullet.clone());
    assert!(world.get_entity_by_id(id).is_some());
    let seen = watch(x);

    bullet.damage(bullet.as_ref(), 1.0, DamageType::GENERIC);

    assert!(world.get_entity_by_id(id).is_none());
    assert_eq!(taken(&seen), [("entity_damage".to_owned(), Some(id), None)]);
}
