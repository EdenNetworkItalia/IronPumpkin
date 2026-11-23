//! The review probes of issue #168, with the fixed behaviour: the locks of one holder, mod
//! callbacks that panic, the copy rules of a respawn, the End exit and a mob conversion, and the
//! dirty flag after a change in place.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value, and a panicking callback is the case under test"
)]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

use std::{
    collections::BTreeMap,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, OnceLock, Weak, mpsc},
    time::Duration,
};

use arc_swap::ArcSwap;

use ironpumpkin_mods::{
    ModInit, NativeMod,
    attachments::{
        self, AttachmentAccess, AttachmentHolder, AttachmentType, AttachmentTypeBuilder,
        NbtSerializer, SYNC_ATTACHMENTS_CHANNEL,
    },
    pumpkin_core::{
        entity::{
            Entity, EntityBase,
            mob::conversion::{ConversionParams, convert_to},
            r#type::from_type,
        },
        net::java::neoforge::{ConnectionType, NegotiatedState},
        plugin::{
            BoxFuture, EventHandler, EventPriority, Payload,
            api::events::network::ConnectionProtocol,
            player::player_spawn_location::PlayerSpawnLocationEvent, startup::STARTUP_HOOK,
        },
        server::Server,
        world::{World, portal::PortalType},
    },
    pumpkin_data::{dimension::Dimension, entity::EntityType},
    pumpkin_util::math::{vector2::Vector2, vector3::Vector3},
    pumpkin_world::chunk::io::Dirtiable,
    register_mod,
};
use pumpkin_config::world::LevelConfig;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::java::neoforge::{NetworkChannel, NetworkPayloadSetup};
use pumpkin_util::identifier::Identifier;
use pumpkin_world::{dimension::into_level, world_info::LevelData};

static A: OnceLock<AttachmentType<i32>> = OnceLock::new();
static B: OnceLock<AttachmentType<i32>> = OnceLock::new();
/// Synced; its `send_to_player` reads `B` of the same holder, as a `NeoForge` predicate may.
static OWNER_GATED: OnceLock<AttachmentType<i32>> = OnceLock::new();
/// Serialized; its read panics on a value of 13, its write panics on 666.
static FRAGILE: OnceLock<AttachmentType<i32>> = OnceLock::new();
/// Serialized, copy on death.
static SOUL: OnceLock<AttachmentType<i32>> = OnceLock::new();
/// Serialized, copy on death, with a copy handler that panics.
static CURSED: OnceLock<AttachmentType<i32>> = OnceLock::new();

fn a() -> AttachmentType<i32> {
    *A.get().unwrap()
}
fn b() -> AttachmentType<i32> {
    *B.get().unwrap()
}
fn owner_gated() -> AttachmentType<i32> {
    *OWNER_GATED.get().unwrap()
}
fn fragile() -> AttachmentType<i32> {
    *FRAGILE.get().unwrap()
}
fn soul() -> AttachmentType<i32> {
    *SOUL.get().unwrap()
}
fn cursed() -> AttachmentType<i32> {
    *CURSED.get().unwrap()
}

fn int_serializer(
    key: &'static str,
) -> NbtSerializer<
    impl Fn(&NbtCompound) -> Result<i32, String> + Send + Sync + 'static,
    impl Fn(&i32, &mut NbtCompound) + Send + Sync + 'static,
> {
    NbtSerializer::new(
        move |nbt: &NbtCompound| {
            let v = nbt.get_int(key).ok_or_else(|| "missing".to_owned())?;
            assert_ne!(v, 13, "a mod serializer panics on read");
            Ok(v)
        },
        move |v: &i32, nbt: &mut NbtCompound| {
            assert_ne!(*v, 666, "a mod serializer panics on write");
            nbt.put_int(key, *v);
        },
    )
}

struct ReviewMod;

impl NativeMod for ReviewMod {
    fn id(&self) -> &'static str {
        "review"
    }
    fn display_name(&self) -> &'static str {
        "Review"
    }
    fn version(&self) -> &'static str {
        "1.0.0"
    }
    fn init(&self, cx: &mut ModInit) {
        let reg = |cx: &mut ModInit, name, builder| cx.register_attachment_type(name, builder);
        A.set(reg(cx, "review:a", AttachmentTypeBuilder::new(|| 0)).unwrap())
            .unwrap();
        B.set(reg(cx, "review:b", AttachmentTypeBuilder::new(|| 0)).unwrap())
            .unwrap();
        OWNER_GATED
            .set(
                cx.register_attachment_type(
                    "review:owner_gated",
                    AttachmentTypeBuilder::new(|| 0).sync_to(
                        |holder: &AttachmentHolder<'_>, _to| match holder {
                            AttachmentHolder::Entity(entity) => {
                                entity.get_existing_data(*B.get().unwrap()).is_some()
                            }
                            _ => true,
                        },
                        |buf, v: &i32| buf.extend_from_slice(&v.to_be_bytes()),
                    ),
                )
                .unwrap(),
            )
            .unwrap();
        FRAGILE
            .set(
                cx.register_attachment_type(
                    "review:fragile",
                    AttachmentTypeBuilder::new(|| 0).serialize(int_serializer("f")),
                )
                .unwrap(),
            )
            .unwrap();
        SOUL.set(
            cx.register_attachment_type(
                "review:soul",
                AttachmentTypeBuilder::new(|| 0)
                    .serialize(int_serializer("s"))
                    .copy_on_death(),
            )
            .unwrap(),
        )
        .unwrap();
        CURSED
            .set(
                cx.register_attachment_type(
                    "review:cursed",
                    AttachmentTypeBuilder::new(|| 0)
                        .serialize(int_serializer("c"))
                        .copy_on_death()
                        .copy_handler(|_: &i32, _: &AttachmentHolder<'_>| -> Option<i32> {
                            panic!("a mod copy handler panics")
                        }),
                )
                .unwrap(),
            )
            .unwrap();
    }
}

register_mod!(ReviewMod);

fn started() -> Arc<World> {
    let _ = STARTUP_HOOK.set(ironpumpkin_mods::init_mods);
    support::world()
}

/// Runs `f` on a thread and reports whether it returned within two seconds.
fn finishes(f: impl FnOnce() + Send + 'static) -> bool {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        f();
        let _ = tx.send(());
    });
    rx.recv_timeout(Duration::from_secs(2)).is_ok()
}

fn neoforge_client() -> NegotiatedState {
    let id = Identifier::parse_static(SYNC_ATTACHMENTS_CHANNEL);
    let channel = NetworkChannel {
        id: id.clone(),
        chosen_version: "1".to_owned(),
    };
    NegotiatedState {
        connection_type: ConnectionType::NeoForge,
        payload_setup: NetworkPayloadSetup {
            channels: BTreeMap::from([(ConnectionProtocol::Play, BTreeMap::from([(id, channel)]))]),
        },
        ..NegotiatedState::default()
    }
}

/// `with_data` on one type that reads another type of the same holder: a common port of
/// `holder.getData(A).modify(holder.getData(B))`.
#[test]
fn with_data_reads_another_type_of_the_same_holder() {
    let world = started();
    let entity = Arc::new(Entity::new(
        world,
        Vector3::new(0.5, 80.0, 0.5),
        &EntityType::PIG,
    ));
    entity.set_data(b(), 2);
    let e = entity.clone();
    let done = finishes(move || {
        e.with_data(a(), |value| *value += e.get_existing_data(b()).unwrap_or(0));
        e.with_data(a(), |value| *value += e.set_data(b(), 5).unwrap_or(0));
    });
    assert!(done, "the lock of one type does not block another type");
    assert_eq!(entity.get_existing_data(a()), Some(4));
    assert_eq!(entity.get_existing_data(b()), Some(5));
}

/// A use of the type of the running `with_data` panics with the name of the type.
#[test]
fn with_data_that_reenters_its_own_type_panics() {
    let world = started();
    let entity = Arc::new(Entity::new(
        world,
        Vector3::new(0.5, 80.0, 0.5),
        &EntityType::PIG,
    ));
    let e = entity.clone();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = catch_unwind(AssertUnwindSafe(|| {
            e.with_data(a(), |_| e.get_existing_data(a()));
        }));
        let message = result
            .err()
            .and_then(|payload| payload.downcast_ref::<String>().cloned());
        let _ = tx.send(message);
    });
    let message = rx.recv_timeout(Duration::from_secs(2)).unwrap().unwrap();
    assert!(message.contains("review:a"), "{message}");
    // The lock is free after the panic.
    entity.set_data(a(), 1);
    assert_eq!(entity.get_existing_data(a()), Some(1));
}

/// A sync predicate that reads the same holder, on a `set_data` with a `NeoForge` client watching.
#[test]
fn a_sync_predicate_reads_the_holder() {
    let world = started();
    let _serial = support::serial();
    let (player, _peer) = support::player_with_peer("ReviewSync", neoforge_client());
    let p = player.clone();
    let done = finishes(move || {
        p.get_entity().set_data(b(), 1);
        p.get_entity().set_data(owner_gated(), 1);
        p.get_entity().with_data(owner_gated(), |value| *value += 1);
        p.get_entity().sync_data(owner_gated());
    });
    assert!(done, "the sync predicate runs with no lock held");
    support::RUNTIME.block_on(world.remove_player(&player, false));
}

/// `NeoForge` catches a failing serializer read and loads the other attachments.
#[test]
fn a_panicking_serializer_read_skips_only_its_attachment() {
    let world = started();
    let mut saved = NbtCompound::new();
    let mut attachments = NbtCompound::new();
    let mut f = NbtCompound::new();
    f.put_int("f", 13);
    attachments.put_compound("review:fragile", f);
    let mut s = NbtCompound::new();
    s.put_int("s", 5);
    attachments.put_compound("review:soul", s);
    saved.put_compound("neoforge:attachments", attachments);

    let entity = Entity::new(world, Vector3::new(0.5, 80.0, 0.5), &EntityType::PIG);
    let result = catch_unwind(AssertUnwindSafe(|| entity.read_nbt_non_mut(&saved)));
    assert!(result.is_ok(), "the panic stays in the attachment read");
    assert_eq!(entity.get_existing_data(soul()), Some(5));
    assert!(!entity.has_data(fragile()));
}

/// `NeoForge` catches a failing serializer write and saves the other attachments.
#[test]
fn a_panicking_serializer_write_skips_only_its_attachment() {
    let world = started();
    let entity = Entity::new(world, Vector3::new(0.5, 80.0, 0.5), &EntityType::PIG);
    entity.set_data(soul(), 5);
    entity.set_data(fragile(), 666);
    let mut nbt = NbtCompound::new();
    let result = catch_unwind(AssertUnwindSafe(|| entity.write_nbt(&mut nbt)));
    assert!(result.is_ok(), "the panic stays in the attachment write");
    let saved = nbt.get_compound("neoforge:attachments").unwrap();
    assert_eq!(
        saved.get_compound("review:soul").unwrap().get_int("s"),
        Some(5)
    );
    assert!(saved.get_compound("review:fragile").is_none());
}

/// `AttachmentInternals.onLivingConvert`: `NeoForge` copies the copy-on-death types to the
/// converted mob.
#[test]
fn a_zombie_that_becomes_a_drowned_keeps_its_copy_on_death_types() {
    let world = started();
    let chunk = Vector2::new(0, 0);
    {
        let mut loading = world.level.chunk_loading.lock().unwrap();
        loading.add_force_ticket(chunk);
        loading.send_change();
    };
    let deadline = std::time::Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&chunk) {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let zombie = from_type(
        &EntityType::ZOMBIE,
        Vector3::new(0.5, 80.0, 0.5),
        &world,
        uuid::Uuid::new_v4(),
    );
    world.spawn_entity(zombie.clone());
    zombie.get_entity().set_data(soul(), 9);
    zombie.get_entity().set_data(fragile(), 4);
    zombie.get_entity().set_data(a(), 1);
    let drowned = convert_to(
        zombie.get_mob().unwrap(),
        &EntityType::DROWNED,
        ConversionParams::single(true, true),
        |_| {},
    )
    .unwrap();
    let drowned = drowned.get_entity();
    assert_eq!(drowned.get_existing_data(soul()), Some(9));
    assert!(!drowned.has_data(fragile()), "no copy on death");
    assert!(!drowned.has_data(a()), "no serializer");
}

/// `NeoForge` mods mark a chunk unsaved after an in-place change; `with_data` marks it.
#[test]
fn an_in_place_chunk_change_marks_the_chunk_unsaved() {
    let world = started();
    let pos = Vector2::new(3, 3);
    {
        let mut loading = world.level.chunk_loading.lock().unwrap();
        loading.add_force_ticket(pos);
        loading.send_change();
    };
    let deadline = std::time::Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&pos) {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let raw = world.level.read_chunk_sync(&pos, Arc::clone).unwrap();
    let holder = attachments::chunk(&world, pos).unwrap();
    holder.set_data(soul(), 1);
    raw.mark_dirty(false);
    holder.with_data(soul(), |v| *v += 1);
    assert!(raw.is_dirty());
    assert_eq!(holder.get_existing_data(soul()), Some(2));
}

/// A blocking handler that runs a closure on the event.
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

/// `PlayerList.respawn`: `PlayerRespawnPositionEvent` sees the player with all its data, and
/// the clone runs after it. A copy handler that panics drops only its value.
#[test]
fn the_respawn_position_event_sees_every_type_and_a_failed_copy_drops_one() {
    let world = started();
    let _serial = support::serial();
    let seen = Arc::new(Mutex::new(None));
    let record = seen.clone();
    support::server()
        .plugin_manager
        .register_with::<PlayerSpawnLocationEvent, _>(
            Arc::new(Blocking(
                move |event: &mut PlayerSpawnLocationEvent| {
                    if event.player.gameprofile.name == "ReviewRespawn" {
                        *record.lock().unwrap() =
                            Some(event.player.get_entity().get_existing_data(fragile()));
                    }
                },
                PhantomData,
            )),
            EventPriority::Normal,
            true,
            false,
        );
    let player = support::player_with_writer("ReviewRespawn");
    let entity = player.get_entity();
    entity.set_data(fragile(), 7);
    entity.set_data(soul(), 3);
    entity.set_data(cursed(), 1);

    support::RUNTIME.block_on(world.respawn_player(&player, false));
    assert_eq!(*seen.lock().unwrap(), Some(Some(7)), "before the clone");
    assert!(!entity.has_data(fragile()), "no copy on death");
    assert_eq!(entity.get_existing_data(soul()), Some(3));
    assert!(!entity.has_data(cursed()), "the copy handler panicked");
    support::RUNTIME.block_on(world.remove_player(&player, false));
}

/// The End exit of Pumpkin (`PortalType::End` from the End): the copy rules of
/// `PlayerList.respawn(player, true)`, every type with a serializer and nothing else.
#[test]
fn the_end_exit_keeps_every_serialized_type() {
    let overworld = started();
    let _serial = support::serial();
    let root = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    // The chunk system of a level starts on the Tokio runtime.
    let _runtime = support::RUNTIME.enter();
    let end = Arc::new(World::load(
        into_level(
            Dimension::THE_END,
            &LevelConfig::default(),
            root.path().to_path_buf(),
            0,
        ),
        Arc::new(ArcSwap::from_pointee(LevelData::default(
            pumpkin_util::world_seed::Seed(0),
        ))),
        Dimension::THE_END,
        ironpumpkin_mods::pumpkin_core::block::registry::default_registry(),
        Weak::new(),
    ));
    let player = support::player_with_writer("ReviewEndExit");
    end.players.rcu(|players| {
        let mut players = (**players).clone();
        players.push(player.clone());
        players
    });
    let entity = player.get_entity();
    entity.set_data(fragile(), 7);
    entity.set_data(soul(), 3);
    entity.set_data(a(), 1);

    let transition = support::RUNTIME.block_on(PortalType::End.get_portal_destination(
        &end,
        overworld.clone(),
        player.as_ref(),
        None,
    ));
    assert!(Arc::ptr_eq(&transition.unwrap().new_world, &overworld));
    assert_eq!(entity.get_existing_data(fragile()), Some(7));
    assert_eq!(entity.get_existing_data(soul()), Some(3));
    assert!(!entity.has_data(a()), "no serializer");
    end.players.store(Arc::new(Vec::new()));
    support::RUNTIME.block_on(overworld.remove_player(&player, false));
    support::RUNTIME.block_on(end.level.shutdown());
}
