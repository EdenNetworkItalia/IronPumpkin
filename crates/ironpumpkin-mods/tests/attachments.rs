//! The scenarios of the spec `neoforge-attachments` (`openspec/changes/neoforge-shaped-plugin-api`)
//! on a real server: saving on every holder, copy on death and the sync to `NeoForge` clients.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

use std::{
    collections::BTreeMap,
    io::Cursor,
    sync::{Arc, OnceLock},
    time::Duration,
};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    attachments::{
        self, ATTACHMENTS_NBT_KEY, AttachmentAccess, AttachmentError, AttachmentHolder,
        AttachmentType, AttachmentTypeBuilder, NbtSerializer, SYNC_ATTACHMENTS_CHANNEL,
    },
    pumpkin_core::{
        entity::{Entity, EntityBase, player::Player},
        net::java::{
            configuration_tasks::SYNCED_REGISTRIES,
            neoforge::{ConnectionType, NegotiatedState},
        },
        plugin::{
            api::events::network::ConnectionProtocol, attachments as hooks,
            network_events::send_to_player, startup::STARTUP_HOOK,
        },
        world::World,
    },
    pumpkin_data::{Block, entity::EntityType, packet::CURRENT_MC_VERSION},
    pumpkin_util::math::{position::BlockPos, vector2::Vector2, vector3::Vector3},
    pumpkin_world::{chunk::ChunkData, world::BlockFlags},
    register_mod,
};
use pumpkin_nbt::{compound::NbtCompound, nbt_compress::read_gzip_compound_tag};
use pumpkin_protocol::{
    codec::var_int::VarInt,
    java::{
        client::play::CCustomPayload,
        neoforge::{NetworkChannel, NetworkPayloadSetup},
        packet_decoder::TCPNetworkDecoder,
    },
    packet::MultiVersionJavaPacket,
    ser::{NetworkReadExt, NetworkWriteExt},
};
use pumpkin_util::identifier::Identifier;

/// A copy-on-death counter with a copy handler that adds one, saved and synced.
static MANA: OnceLock<AttachmentType<i32>> = OnceLock::new();
/// Saved, without copy on death and without sync.
static NAME: OnceLock<AttachmentType<String>> = OnceLock::new();
/// Neither saved nor copied, synced.
static COMBO: OnceLock<AttachmentType<i32>> = OnceLock::new();

fn mana() -> AttachmentType<i32> {
    *MANA.get().unwrap()
}

fn name() -> AttachmentType<String> {
    *NAME.get().unwrap()
}

fn combo() -> AttachmentType<i32> {
    *COMBO.get().unwrap()
}

fn write_var_int(buf: &mut Vec<u8>, value: i32) {
    buf.write_var_int(&VarInt(value)).unwrap();
}

struct AttachMod;

impl NativeMod for AttachMod {
    fn id(&self) -> &'static str {
        "attachtest"
    }

    fn display_name(&self) -> &'static str {
        "Attachment Test"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        let mana = cx
            .register_attachment_type(
                "attachtest:mana",
                AttachmentTypeBuilder::new(|| 0)
                    .serialize(NbtSerializer::new(
                        |nbt: &NbtCompound| nbt.get_int("mana").ok_or_else(|| "no mana".to_owned()),
                        |mana: &i32, nbt: &mut NbtCompound| nbt.put_int("mana", *mana),
                    ))
                    .copy_on_death()
                    .copy_handler(|mana: &i32, _: &AttachmentHolder<'_>| Some(mana + 1))
                    .sync_with(|buf, mana| write_var_int(buf, *mana)),
            )
            .unwrap();
        let name = cx
            .register_attachment_type(
                "attachtest:name",
                AttachmentTypeBuilder::new(String::new).serialize(NbtSerializer::new(
                    |nbt: &NbtCompound| {
                        nbt.get_string("name")
                            .map(ToString::to_string)
                            .ok_or_else(|| "no name".to_owned())
                    },
                    |name: &String, nbt: &mut NbtCompound| nbt.put_string("name", name.clone()),
                )),
            )
            .unwrap();
        let combo = cx
            .register_attachment_type(
                "attachtest:combo",
                AttachmentTypeBuilder::new(|| 0).sync_with(|buf, combo| write_var_int(buf, *combo)),
            )
            .unwrap();
        MANA.set(mana).unwrap();
        NAME.set(name).unwrap();
        COMBO.set(combo).unwrap();
    }
}

register_mod!(AttachMod);

fn started() -> Arc<World> {
    let _ = STARTUP_HOOK.set(ironpumpkin_mods::init_mods);
    support::world()
}

fn load(world: &World, chunk: Vector2<i32>) {
    {
        let mut loading = world.level.chunk_loading.lock().unwrap();
        loading.add_force_ticket(chunk);
        loading.send_change();
    };
    let deadline = std::time::Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&chunk) {
        assert!(
            std::time::Instant::now() < deadline,
            "the chunk did not load"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
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

/// The payloads on `channel` that reach the client before the `minecraft:brand` marker that
/// the test sends last.
fn payloads_before_marker(
    player: &Player,
    peer: tokio::net::TcpStream,
    channel: &str,
) -> Vec<Vec<u8>> {
    send_to_player(player, "minecraft:brand", b"marker").unwrap();
    support::RUNTIME.block_on(async {
        let mut decoder = TCPNetworkDecoder::new(tokio::io::BufReader::new(peer));
        let mut found = Vec::new();
        loop {
            let packet = tokio::time::timeout(Duration::from_secs(10), decoder.get_raw_packet())
                .await
                .unwrap()
                .unwrap();
            if packet.id != CCustomPayload::to_id(CURRENT_MC_VERSION) {
                continue;
            }
            let mut body = &packet.payload[..];
            let packet_channel = body.get_str().unwrap();
            if &*packet_channel == "minecraft:brand" {
                return found;
            }
            if &*packet_channel == channel {
                found.push(body.to_vec());
            }
        }
    })
}

/// The id of `ty` in `neoforge:synced_attachment_types`.
fn synced_id(ty_name: &str) -> u8 {
    let snapshot =
        hooks::synced_types_snapshot(ironpumpkin_mods::pumpkin_data::dynamic::tables().unwrap());
    let (id, _) = snapshot
        .iter()
        .find(|(_, name)| name.to_string() == ty_name)
        .unwrap();
    u8::try_from(*id).unwrap()
}

fn var_int(value: i32) -> Vec<u8> {
    let mut buf = Vec::new();
    write_var_int(&mut buf, value);
    buf
}

/// The synced types travel in a registry of the `NeoForge` registry sync, by registration
/// order of the types with a sync handler.
#[test]
fn synced_types_join_the_registry_sync() {
    started();
    let registries = SYNCED_REGISTRIES;
    let (name, snapshot) = registries.last().unwrap();
    assert_eq!(name.to_string(), "neoforge:synced_attachment_types");
    let names: Vec<(i32, String)> =
        snapshot(ironpumpkin_mods::pumpkin_data::dynamic::tables().unwrap())
            .into_iter()
            .map(|(id, name)| (id, name.to_string()))
            .collect();
    assert_eq!(
        names,
        [
            (0, "attachtest:mana".to_owned()),
            (1, "attachtest:combo".to_owned())
        ]
    );
}

#[test]
fn a_registration_after_the_freeze_fails() {
    started();
    let mut cx = ModInit::new("attachtest");
    assert_eq!(
        cx.register_attachment_type("attachtest:late", AttachmentTypeBuilder::new(|| 0))
            .unwrap_err(),
        AttachmentError::RegistryFrozen("attachtest:late".to_owned())
    );
}

/// Scenarios "First read", "No cost" and the saving of an entity.
#[test]
fn an_entity_saves_its_serialized_attachments_only() {
    let world = started();
    let entity = Entity::new(
        world.clone(),
        Vector3::new(0.5, 80.0, 0.5),
        &EntityType::PIG,
    );

    // A holder without attachments allocates nothing and writes no key.
    assert!(!entity.has_data(mana()));
    assert_eq!(entity.get_existing_data(mana()), None);
    assert!(entity.attachments.lock().is_none());
    let mut nbt = NbtCompound::new();
    entity.write_nbt(&mut nbt);
    assert!(nbt.get_compound(ATTACHMENTS_NBT_KEY).is_none());

    // `getData` creates and stores the default.
    assert_eq!(entity.get_data(name()), "");
    assert!(entity.has_data(name()));
    assert_eq!(entity.set_data(mana(), 4), None);
    assert_eq!(entity.set_data(mana(), 5), Some(4));
    entity.set_data(combo(), 9);
    entity.with_data(name(), |name| name.push_str("Bob"));

    let mut nbt = NbtCompound::new();
    entity.write_nbt(&mut nbt);
    let saved = nbt.get_compound(ATTACHMENTS_NBT_KEY).unwrap();
    assert_eq!(saved.child_tags.len(), 2, "the combo has no serializer");
    assert_eq!(
        saved
            .get_compound("attachtest:mana")
            .unwrap()
            .get_int("mana"),
        Some(5)
    );
    assert_eq!(
        saved
            .get_compound("attachtest:name")
            .unwrap()
            .get_string("name")
            .map(ToString::to_string),
        Some("Bob".to_owned())
    );

    let loaded = Entity::new(world, Vector3::new(0.5, 80.0, 0.5), &EntityType::PIG);
    loaded.read_nbt_non_mut(&nbt);
    assert_eq!(loaded.get_existing_data(mana()), Some(5));
    assert_eq!(loaded.get_existing_data(name()), Some("Bob".to_owned()));
    assert!(!loaded.has_data(combo()));

    assert_eq!(entity.remove_data(mana()), Some(5));
    assert!(!entity.has_data(mana()));
}

/// Scenario "Round trip": a player logs out and back in.
#[test]
fn a_player_keeps_its_attachments_across_a_relog() {
    let world = started();
    let _serial = support::serial();
    let player = support::player("AttachRelog");
    player.get_entity().set_data(mana(), 12);
    player.get_entity().set_data(combo(), 3);
    let server = support::server();
    server
        .player_data_storage
        .handle_player_leave(&player)
        .unwrap();
    support::RUNTIME.block_on(world.remove_player(&player, false));
    let data = server
        .player_data_storage
        .load_data(&player.gameprofile.id)
        .unwrap()
        .unwrap();

    let back = support::player("AttachRelog2");
    back.read_nbt_non_mut(&data);
    assert_eq!(back.get_entity().get_existing_data(mana()), Some(12));
    assert!(!back.get_entity().has_data(combo()));
    support::RUNTIME.block_on(world.remove_player(&back, false));
}

#[test]
fn a_block_entity_saves_with_its_chunk_data() {
    let world = started();
    let pos = BlockPos(Vector3::new(40, 70, 40));
    load(&world, pos.chunk_position());
    world.set_block_state(&pos, Block::CHEST.default_state.id, BlockFlags::empty());
    let chest = world.get_block_entity(&pos).unwrap();
    let holder = attachments::block_entity(&world, &chest);
    assert!(!holder.has_attachments());
    holder.set_data(name(), "Loot".to_owned());
    holder.set_data(combo(), 2);
    // The sync waits for the start of the next world tick.
    let queued = world.attachments.take_block_entity_sync();
    assert_eq!(
        queued.len(),
        1,
        "only the synced type is queued: {queued:?}"
    );

    support::RUNTIME.block_on(world.save());
    let saved = world
        .level
        .read_chunk_sync(&pos.chunk_position(), |chunk| {
            chunk
                .pending_block_entities
                .lock()
                .unwrap()
                .get(&pos)
                .cloned()
        })
        .flatten()
        .unwrap();
    let attachments = saved.get_compound(ATTACHMENTS_NBT_KEY).unwrap();
    assert_eq!(attachments.child_tags.len(), 1);

    // A wake-up from the saved data reads the attachments back.
    world
        .block_entities
        .get_mut(&pos.chunk_position())
        .unwrap()
        .remove(&pos);
    let woken = world.get_block_entity(&pos).unwrap();
    assert!(!Arc::ptr_eq(&woken, &chest));
    let holder = attachments::block_entity(&world, &woken);
    assert_eq!(holder.get_existing_data(name()), Some("Loot".to_owned()));
    assert!(!holder.has_data(combo()));

    // A new block entity at the same position starts empty.
    world.remove_block_entity(&pos);
    world.set_block_state(&pos, Block::AIR.default_state.id, BlockFlags::empty());
    world.set_block_state(&pos, Block::CHEST.default_state.id, BlockFlags::empty());
    let fresh = world.get_block_entity(&pos).unwrap();
    assert!(!attachments::block_entity(&world, &fresh).has_attachments());
}

#[test]
fn a_chunk_saves_its_attachments_at_the_top_level() {
    let world = started();
    let pos = Vector2::new(5, 5);
    load(&world, pos);
    let holder = attachments::chunk(&world, pos).unwrap();
    holder.set_data(mana(), 77);
    holder.set_data(combo(), 1);
    let chunk = world.level.read_chunk_sync(&pos, Arc::clone).unwrap();
    let bytes = chunk.internal_to_bytes();

    let root = pumpkin_nbt::Nbt::read(&mut pumpkin_nbt::deserializer::NbtReadHelperJava::new(
        &mut Cursor::new(bytes.to_vec()),
    ))
    .unwrap()
    .root_tag;
    let attachments = root.get_compound(ATTACHMENTS_NBT_KEY).unwrap();
    assert_eq!(attachments.child_tags.len(), 1);
    assert_eq!(
        attachments
            .get_compound("attachtest:mana")
            .unwrap()
            .get_int("mana"),
        Some(77)
    );

    let loaded = ChunkData::internal_from_bytes(&bytes, pos).unwrap();
    let guard = loaded.attachments.lock();
    let map = guard.as_ref().unwrap();
    assert_eq!(map.iter().count(), 1);

    let empty = ChunkData::empty(9, 9).internal_to_bytes();
    let root = pumpkin_nbt::Nbt::read(&mut pumpkin_nbt::deserializer::NbtReadHelperJava::new(
        &mut Cursor::new(empty.to_vec()),
    ))
    .unwrap()
    .root_tag;
    assert!(root.get_compound(ATTACHMENTS_NBT_KEY).is_none());
}

#[test]
fn a_level_saves_to_its_saved_data_file() {
    let world = started();
    world.set_data(mana(), 31);
    world.set_data(combo(), 8);
    hooks::save_level(&world);
    let path = world
        .level
        .level_folder
        .dim_folder
        .join("data/neoforge/data_attachments.dat");
    let root = read_gzip_compound_tag(std::fs::File::open(&path).unwrap()).unwrap();
    assert!(root.get_int("DataVersion").is_some());
    let data = root.get_compound("data").unwrap();
    assert_eq!(data.child_tags.len(), 1);
    assert_eq!(
        data.get_compound("attachtest:mana")
            .unwrap()
            .get_int("mana"),
        Some(31)
    );

    world.attachments.level.replace(None);
    hooks::load_level(&world);
    assert_eq!(world.get_existing_data(mana()), Some(31));
    assert!(!world.has_data(combo()));
    world.remove_data(mana());
}

/// Scenario "Respawn", and the copy rules of `ServerPlayer#restoreFrom` on the End exit.
#[test]
fn copy_on_death_keeps_only_its_types_through_the_copy_handler() {
    let world = started();
    let _serial = support::serial();
    let player = support::player_with_writer("AttachDeath");
    let entity = player.get_entity();
    entity.set_data(mana(), 5);
    entity.set_data(name(), "kept on the End exit".to_owned());
    entity.set_data(combo(), 3);

    support::RUNTIME.block_on(world.respawn_player(&player, false));
    assert_eq!(
        entity.get_existing_data(mana()),
        Some(6),
        "through the copy handler"
    );
    assert!(!entity.has_data(name()), "no copy on death");
    assert!(!entity.has_data(combo()), "no serializer");

    entity.set_data(name(), "Steve".to_owned());
    entity.set_data(combo(), 3);
    support::RUNTIME.block_on(world.respawn_player(&player, true));
    assert_eq!(entity.get_existing_data(mana()), Some(7));
    assert_eq!(entity.get_existing_data(name()), Some("Steve".to_owned()));
    assert!(!entity.has_data(combo()), "no serializer");
    support::RUNTIME.block_on(world.remove_player(&player, false));
}

/// Scenario "Tracking player", and the other sync rules.
#[test]
fn a_synced_change_reaches_neoforge_clients_only() {
    let world = started();
    let _serial = support::serial();
    let (player, peer) = support::player_with_peer("AttachSync", neoforge_client());
    let entity = player.get_entity();
    let id = entity.entity_id;

    entity.set_data(mana(), 300);
    entity.set_data(name(), "not synced".to_owned());
    entity.sync_data(mana());
    // `AttachmentSync.syncUpdate` returns for a type without a sync handler: nothing is sent.
    entity.sync_data(name());
    entity.remove_data(mana());
    hooks::sync_player(&player);

    let mut target = vec![2];
    target.extend(var_int(id));
    let update = |present: bool, value: i32| {
        let mut data = vec![u8::from(present)];
        if present {
            data.extend(var_int(value));
        }
        let mut payload = target.clone();
        payload.extend([1, synced_id("attachtest:mana")]);
        payload.extend(var_int(data.len() as i32));
        payload.extend(data);
        payload
    };
    let payloads = payloads_before_marker(&player, peer, SYNC_ATTACHMENTS_CHANNEL);
    // The set, the explicit sync and the removal; the initial sync of a holder with no synced
    // value sends nothing.
    assert_eq!(
        payloads,
        [update(true, 300), update(true, 300), update(false, 0)]
    );
    support::RUNTIME.block_on(world.remove_player(&player, false));

    let (vanilla, peer) = support::player_with_peer("AttachVanilla", NegotiatedState::default());
    vanilla.get_entity().set_data(mana(), 1);
    hooks::sync_player(&vanilla);
    assert!(payloads_before_marker(&vanilla, peer, SYNC_ATTACHMENTS_CHANNEL).is_empty());
    support::RUNTIME.block_on(world.remove_player(&vanilla, false));
}

/// The initial syncs of a level, a chunk and its block entities.
#[test]
fn a_new_viewer_gets_the_synced_values() {
    let world = started();
    let _serial = support::serial();
    let chunk_pos = Vector2::new(8, 8);
    load(&world, chunk_pos);
    let pos = BlockPos(Vector3::new(130, 70, 130));
    world.set_block_state(&pos, Block::CHEST.default_state.id, BlockFlags::empty());
    let chest = world.get_block_entity(&pos).unwrap();
    attachments::block_entity(&world, &chest).set_data(combo(), 4);
    world.attachments.take_block_entity_sync();
    attachments::chunk(&world, chunk_pos)
        .unwrap()
        .set_data(combo(), 5);
    world.set_data(combo(), 6);

    let (player, peer) = support::player_with_peer("AttachViewer", neoforge_client());
    hooks::sync_level(&world, &player);
    hooks::sync_chunks(&world, &player, &[chunk_pos]);

    let combo_id = synced_id("attachtest:combo");
    let payload = |target: Vec<u8>, value: i32| {
        let mut payload = target;
        payload.extend([1, combo_id]);
        let mut data = vec![1];
        data.extend(var_int(value));
        payload.extend(var_int(data.len() as i32));
        payload.extend(data);
        payload
    };
    let mut chunk_target = vec![1];
    chunk_target.extend_from_slice(&(8i64 | (8i64 << 32)).to_be_bytes());
    let mut block_target = vec![0];
    block_target.extend_from_slice(&pos.as_long().to_be_bytes());

    let payloads = payloads_before_marker(&player, peer, SYNC_ATTACHMENTS_CHANNEL);
    // The level payload arrives first; the level of the test server may hold the values of
    // the other tests, so only the payloads of this test count.
    assert!(payloads.contains(&payload(vec![3], 6)), "{payloads:?}");
    assert!(payloads.contains(&payload(chunk_target, 5)), "{payloads:?}");
    assert!(payloads.contains(&payload(block_target, 4)), "{payloads:?}");
    world.remove_data(combo());
    support::RUNTIME.block_on(world.remove_player(&player, false));
}
