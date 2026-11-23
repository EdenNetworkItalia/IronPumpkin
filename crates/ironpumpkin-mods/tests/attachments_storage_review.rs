//! Review checks of the attachment storage and wire format against `NeoForge` 26.3.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
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

use ironpumpkin_mods::pumpkin_core::plugin::api::events::server::PacketSentEvent;
use ironpumpkin_mods::{
    ModInit, NativeMod,
    attachments::{
        self, ATTACHMENTS_NBT_KEY, AttachmentAccess, AttachmentType, AttachmentTypeBuilder,
        NbtSerializer, SYNC_ATTACHMENTS_CHANNEL,
    },
    pumpkin_core::{
        entity::{Entity, EntityBase},
        net::java::{
            configuration_tasks::RegistrySyncPayloads,
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
use pumpkin_nbt::{
    compound::NbtCompound,
    nbt_compress::{read_gzip_compound_tag, write_gzip_compound_tag},
    tag::NbtTag,
};
use pumpkin_protocol::{
    codec::var_int::VarInt,
    java::{
        client::play::CCustomPayload,
        neoforge::{FrozenRegistryPayload, NetworkChannel, NetworkPayloadSetup, RegistrySnapshot},
        packet_decoder::TCPNetworkDecoder,
    },
    packet::MultiVersionJavaPacket,
    ser::{NetworkReadExt, NetworkWriteExt},
};
use pumpkin_util::identifier::Identifier;

/// Saved and synced.
static MANA: OnceLock<AttachmentType<i32>> = OnceLock::new();
/// Synced only.
static COMBO: OnceLock<AttachmentType<i32>> = OnceLock::new();

fn mana() -> AttachmentType<i32> {
    *MANA.get().unwrap()
}

fn combo() -> AttachmentType<i32> {
    *COMBO.get().unwrap()
}

fn write_var_int(buf: &mut Vec<u8>, value: i32) {
    buf.write_var_int(&VarInt(value)).unwrap();
}

struct ReviewMod;

impl NativeMod for ReviewMod {
    fn id(&self) -> &'static str {
        "reviewtest"
    }

    fn display_name(&self) -> &'static str {
        "Review Test"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        let mana = cx
            .register_attachment_type(
                "reviewtest:mana",
                AttachmentTypeBuilder::new(|| 0)
                    .serialize(NbtSerializer::new(
                        |nbt: &NbtCompound| nbt.get_int("mana").ok_or_else(|| "no mana".to_owned()),
                        |mana: &i32, nbt: &mut NbtCompound| nbt.put_int("mana", *mana),
                    ))
                    .sync_with(|buf, mana| write_var_int(buf, *mana)),
            )
            .unwrap();
        let combo = cx
            .register_attachment_type(
                "reviewtest:combo",
                AttachmentTypeBuilder::new(|| 0).sync_with(|buf, combo| write_var_int(buf, *combo)),
            )
            .unwrap();
        MANA.set(mana).unwrap();
        COMBO.set(combo).unwrap();
    }
}

register_mod!(ReviewMod);

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

async fn read_until_marker(peer: tokio::net::TcpStream, channel: String) -> Vec<Vec<u8>> {
    let mut decoder = TCPNetworkDecoder::new(tokio::io::BufReader::new(peer));
    let mut found = Vec::new();
    loop {
        let packet = tokio::time::timeout(Duration::from_secs(60), decoder.get_raw_packet())
            .await
            .unwrap()
            .unwrap();
        if packet.id != CCustomPayload::to_id(CURRENT_MC_VERSION) {
            continue;
        }
        let mut body = &packet.payload[..];
        let packet_channel = body.get_str().unwrap();
        if &*packet_channel == "minecraft:brand" && body == b"marker" {
            return found;
        }
        if *packet_channel == channel {
            found.push(body.to_vec());
        }
    }
}

fn payloads_before_marker(
    player: &ironpumpkin_mods::pumpkin_core::entity::player::Player,
    peer: tokio::net::TcpStream,
) -> Vec<Vec<u8>> {
    send_to_player(player, "minecraft:brand", b"marker").unwrap();
    support::RUNTIME.block_on(read_until_marker(peer, SYNC_ATTACHMENTS_CHANNEL.to_owned()))
}

fn var_int(value: i32) -> Vec<u8> {
    let mut buf = Vec::new();
    write_var_int(&mut buf, value);
    buf
}

fn string(value: &str) -> Vec<u8> {
    let mut buf = var_int(value.len() as i32);
    buf.extend_from_slice(value.as_bytes());
    buf
}

/// `FriendlyByteBuf#writeBlockPos`: `BlockPos.asLong`, X 26 bits at 38, Z 26 bits at 12, Y 12
/// bits at 0, big endian.
fn vanilla_block_pos(x: i32, y: i32, z: i32) -> [u8; 8] {
    let packed = ((i64::from(x) & 0x3FF_FFFF) << 38)
        | ((i64::from(z) & 0x3FF_FFFF) << 12)
        | (i64::from(y) & 0xFFF);
    packed.to_be_bytes()
}

/// `FriendlyByteBuf#writeChunkPos`: `ChunkPos.pack`, big endian.
fn vanilla_chunk_pos(x: i32, z: i32) -> [u8; 8] {
    ((i64::from(x) & 0xFFFF_FFFF) | ((i64::from(z) & 0xFFFF_FFFF) << 32)).to_be_bytes()
}

/// `SyncAttachmentsPayload.STREAM_CODEC` with one type, read by hand: target, list of registry
/// ids (`VarInt` count, `VarInt` ids), `UNBOUNDED_BYTE_ARRAY` (`VarInt` length) of
/// `writeBoolean(present)` and the handler's `VarInt`.
fn one_type_payload(target: &[u8], synced_id: i32, value: Option<i32>) -> Vec<u8> {
    let mut data = vec![u8::from(value.is_some())];
    if let Some(value) = value {
        data.extend(var_int(value));
    }
    let mut payload = target.to_vec();
    payload.extend(var_int(1));
    payload.extend(var_int(synced_id));
    payload.extend(var_int(data.len() as i32));
    payload.extend(data);
    payload
}

const MANA_ID: i32 = 0;
const COMBO_ID: i32 = 1;

/// Question 1: an entity compound as `Entity#saveWithoutId` writes it on `NeoForge`, with an
/// unknown type, a type without serializer and an invalid key, as
/// `AttachmentHolder#deserializeAttachments` skips them.
#[test]
fn a_neoforge_entity_compound_loads_and_saves_back() {
    let world = started();
    let mut attachments_nbt = NbtCompound::new();
    let mut mana_nbt = NbtCompound::new();
    mana_nbt.put_int("mana", 7);
    attachments_nbt.put_compound("reviewtest:mana", mana_nbt);
    let mut other = NbtCompound::new();
    other.put_int("x", 1);
    attachments_nbt.put_compound("othermod:thing", other);
    attachments_nbt.put_compound("reviewtest:combo", NbtCompound::new());
    attachments_nbt.put_int("Not A Key", 3);
    let mut nbt = NbtCompound::new();
    nbt.put_compound(ATTACHMENTS_NBT_KEY, attachments_nbt);

    let entity = Entity::new(world, Vector3::new(0.5, 80.0, 0.5), &EntityType::PIG);
    entity.read_nbt_non_mut(&nbt);
    assert_eq!(entity.get_existing_data(mana()), Some(7));
    assert!(!entity.has_data(combo()));

    let mut out = NbtCompound::new();
    entity.write_nbt(&mut out);
    let saved = out.get_compound(ATTACHMENTS_NBT_KEY).unwrap();
    assert_eq!(saved.child_tags.len(), 1, "{saved:?}");
    assert_eq!(
        saved
            .get_compound("reviewtest:mana")
            .unwrap()
            .get_int("mana"),
        Some(7)
    );
}

/// Question 1: the level file as `SavedDataStorage` writes `neoforge:data_attachments`.
#[test]
fn a_neoforge_level_file_loads() {
    let world = started();
    let _serial = support::serial();
    let dim = &world.level.level_folder.dim_folder;
    assert!(
        dim.ends_with("dimensions/minecraft/overworld"),
        "DimensionType.getStorageFolder: {}",
        dim.display()
    );
    let path = dim.join("data/neoforge/data_attachments.dat");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut mana_nbt = NbtCompound::new();
    mana_nbt.put_int("mana", 9);
    let mut data = NbtCompound::new();
    data.put_compound("reviewtest:mana", mana_nbt);
    let mut root = NbtCompound::new();
    root.put_int("DataVersion", 5023);
    root.put_compound("data", data);
    write_gzip_compound_tag(root, std::fs::File::create(&path).unwrap()).unwrap();

    world.attachments.level.replace(None);
    hooks::load_level(&world);
    assert_eq!(world.get_existing_data(mana()), Some(9));

    // `LevelAttachmentsSavedData#isDirty` is always true: the file stays, with an empty data.
    world.remove_data(mana());
    hooks::save_level(&world);
    let root = read_gzip_compound_tag(std::fs::File::open(&path).unwrap()).unwrap();
    assert_eq!(root.get_int("DataVersion"), Some(5023));
    assert!(root.get_compound("data").unwrap().is_empty());
}

/// Question 2: a chunk with only a type without serializer writes the same NBT as a chunk with
/// no attachment, and a chunk with a saved type adds only the top-level key. `NbtCompound` is a
/// `HashMap`, so two writes of one chunk already differ in key order on master: the check
/// compares the decoded compounds.
#[test]
fn a_chunk_without_saved_attachments_writes_the_same_nbt() {
    fn root(bytes: &[u8]) -> NbtCompound {
        pumpkin_nbt::Nbt::read(&mut pumpkin_nbt::deserializer::NbtReadHelperJava::new(
            &mut Cursor::new(bytes.to_vec()),
        ))
        .unwrap()
        .root_tag
    }
    let world = started();
    let pos = Vector2::new(-20, 17);
    load(&world, pos);
    let chunk = world.level.read_chunk_sync(&pos, Arc::clone).unwrap();
    // The live chunk ticks while the test runs: compare detached copies of one snapshot.
    let snapshot = chunk.internal_to_bytes();
    let plain = ChunkData::internal_from_bytes(&snapshot, pos).unwrap();
    let before = root(&plain.internal_to_bytes());
    attachments::chunk(&world, pos)
        .unwrap()
        .set_data(combo(), 3);
    let with_combo = ChunkData::internal_from_bytes(&snapshot, pos).unwrap();
    with_combo.attachments.replace(chunk.attachments.take());
    assert!(with_combo.attachments.has_attachments());
    assert_eq!(root(&with_combo.internal_to_bytes()), before);

    attachments::chunk(&world, pos).unwrap().set_data(mana(), 4);
    let with_mana = ChunkData::internal_from_bytes(&snapshot, pos).unwrap();
    with_mana.attachments.replace(chunk.attachments.take());
    let with = with_mana.internal_to_bytes();
    let mut with_root = root(&with);
    let saved = with_root.child_tags.remove(ATTACHMENTS_NBT_KEY).unwrap();
    assert_eq!(with_root, before, "only the top-level key is added");
    let NbtTag::Compound(saved) = saved else {
        panic!("not a compound");
    };
    assert_eq!(
        saved
            .get_compound("reviewtest:mana")
            .unwrap()
            .get_int("mana"),
        Some(4)
    );
    let loaded = ChunkData::internal_from_bytes(&with, pos).unwrap();
    assert_eq!(
        loaded.attachments.lock().as_ref().unwrap().iter().count(),
        1
    );
}

/// Question 3: the target encodings with negative coordinates, read with the vanilla formulas.
#[test]
fn targets_pack_as_vanilla_with_negative_coordinates() {
    let world = started();
    let _serial = support::serial();
    let chunk_pos = Vector2::new(-9, -1);
    load(&world, chunk_pos);
    let pos = BlockPos(Vector3::new(-130, -60, -7));
    assert_eq!(pos.chunk_position(), chunk_pos);
    world.set_block_state(&pos, Block::CHEST.default_state.id, BlockFlags::empty());
    let chest = world.get_block_entity(&pos).unwrap();
    attachments::block_entity(&world, &chest).set_data(combo(), 4);
    world.attachments.take_block_entity_sync();
    attachments::chunk(&world, chunk_pos)
        .unwrap()
        .set_data(combo(), 5);

    let (player, peer) = support::player_with_peer("ReviewTargets", neoforge_client());
    hooks::sync_chunks(&world, &player, &[chunk_pos]);
    let mut chunk_target = vec![1];
    chunk_target.extend(vanilla_chunk_pos(-9, -1));
    let mut block_target = vec![0];
    block_target.extend(vanilla_block_pos(-130, -60, -7));
    let payloads = payloads_before_marker(&player, peer);
    assert_eq!(
        payloads,
        [
            one_type_payload(&chunk_target, COMBO_ID, Some(5)),
            one_type_payload(&block_target, COMBO_ID, Some(4)),
        ]
    );
    attachments::chunk(&world, chunk_pos)
        .unwrap()
        .remove_data(combo());
    world.remove_block_entity(&pos);
    support::RUNTIME.block_on(world.remove_player(&player, false));
}

/// Question 3: set, change and removal of an entity value, decoded by hand.
#[test]
fn entity_set_change_and_remove_decode_as_neoforge() {
    let world = started();
    let _serial = support::serial();
    let (player, peer) = support::player_with_peer("ReviewEntity", neoforge_client());
    let entity = player.get_entity();
    entity.set_data(combo(), 1);
    entity.set_data(combo(), 300);
    entity.remove_data(combo());
    let mut target = vec![2];
    target.extend(var_int(entity.entity_id));
    assert_eq!(
        payloads_before_marker(&player, peer),
        [
            one_type_payload(&target, COMBO_ID, Some(1)),
            one_type_payload(&target, COMBO_ID, Some(300)),
            one_type_payload(&target, COMBO_ID, None),
        ]
    );
    support::RUNTIME.block_on(world.remove_player(&player, false));
}

/// Question 3: `neoforge:frozen_registry` for the synced types, as `RegistrySnapshot` writes it:
/// name, `VarInt` count, (`VarInt` id, name) per entry, `VarInt` alias count. The empty one is the
/// 35-byte payload of the `NeoForge` 26.3.0.64-beta capture (seq 45 of run-b-neoforge).
#[test]
fn the_synced_types_registry_payload_matches_neoforge() {
    started();
    let payloads =
        RegistrySyncPayloads::encode(ironpumpkin_mods::pumpkin_data::dynamic::tables().unwrap())
            .unwrap();
    let mut expected = string("neoforge:synced_attachment_types");
    expected.extend(var_int(2));
    expected.extend(var_int(0));
    expected.extend(string("reviewtest:mana"));
    expected.extend(var_int(1));
    expected.extend(string("reviewtest:combo"));
    expected.extend(var_int(0));
    assert_eq!(payloads.registries.last().unwrap(), &expected);

    let mut empty = Vec::new();
    FrozenRegistryPayload {
        registry_name: Identifier::parse_static("neoforge:synced_attachment_types"),
        snapshot: RegistrySnapshot::from_ids(Vec::new()),
    }
    .write(&mut empty)
    .unwrap();
    let capture = "206e656f666f7267653a73796e6365645f6174746163686d656e745f74797065730000";
    let capture: Vec<u8> = (0..capture.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&capture[i..i + 2], 16).unwrap())
        .collect();
    assert_eq!(empty, capture);
}

/// Question 4: `PlayerList#placeNewPlayer` sends `syncInitialPlayerAttachments` at the login of
/// a Java player.
#[test]
fn a_java_login_sends_the_initial_player_sync() {
    let world = started();
    let _serial = support::serial();
    let server = support::server();
    let (player, peer) = support::player_with_peer("ReviewLogin", neoforge_client());
    let reader =
        support::RUNTIME.spawn(read_until_marker(peer, SYNC_ATTACHMENTS_CHANNEL.to_owned()));
    // As if read from the player data: the set sends one update to the player itself.
    player.get_entity().set_data(combo(), 77);
    let joined = support::RUNTIME.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(120),
            world.spawn_java_player(&server.basic_config, &player, server),
        )
        .await
    });
    assert!(joined.is_ok(), "spawn_java_player timed out");
    send_to_player(&player, "minecraft:brand", b"marker").unwrap();
    let payloads = support::RUNTIME.block_on(reader).unwrap();
    let mut target = vec![2];
    target.extend(var_int(player.entity_id()));
    support::RUNTIME.block_on(world.remove_player(&player, false));
    assert_eq!(
        payloads,
        [
            one_type_payload(&target, COMBO_ID, Some(77)),
            one_type_payload(&target, COMBO_ID, Some(77)),
        ],
        "the update of the set, then the initial sync of the login"
    );
}

/// Question 4: a block entity read from the chunk data and not woken yet keeps its synced
/// value, which `AttachmentSync.onChunkSent` sends to a new viewer on `NeoForge`.
#[test]
fn a_sleeping_block_entity_gets_its_initial_sync() {
    let world = started();
    let _serial = support::serial();
    let chunk_pos = Vector2::new(-12, 3);
    load(&world, chunk_pos);
    let pos = BlockPos(Vector3::new(-190, 70, 50));
    assert_eq!(pos.chunk_position(), chunk_pos);
    world.set_block_state(&pos, Block::CHEST.default_state.id, BlockFlags::empty());
    let chest = world.get_block_entity(&pos).unwrap();
    attachments::block_entity(&world, &chest).set_data(mana(), 11);
    world.attachments.take_block_entity_sync();
    support::RUNTIME.block_on(world.save());
    // As after a restart: the block entity sleeps in the chunk data.
    world
        .block_entities
        .get_mut(&chunk_pos)
        .unwrap()
        .remove(&pos);
    drop(chest);

    let (player, peer) = support::player_with_peer("ReviewSleeping", neoforge_client());
    hooks::sync_chunks(&world, &player, &[chunk_pos]);
    let mut block_target = vec![0];
    block_target.extend(vanilla_block_pos(-190, 70, 50));
    let payloads = payloads_before_marker(&player, peer);
    let woken = world.get_block_entity(&pos).unwrap();
    assert_eq!(
        attachments::block_entity(&world, &woken).get_existing_data(mana()),
        Some(11),
        "the value is saved"
    );
    world.remove_block_entity(&pos);
    support::RUNTIME.block_on(world.remove_player(&player, false));
    assert_eq!(
        payloads,
        [one_type_payload(&block_target, MANA_ID, Some(11))]
    );
}

struct OnPacketSent<F>(F);

impl<F> ironpumpkin_mods::pumpkin_core::plugin::EventHandler<PacketSentEvent> for OnPacketSent<F>
where
    F: Fn(&mut PacketSentEvent) + Send + Sync,
{
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<ironpumpkin_mods::pumpkin_core::server::Server>,
        event: &'a mut PacketSentEvent,
    ) -> ironpumpkin_mods::pumpkin_core::plugin::BoxFuture<'a, ()> {
        (self.0)(event);
        Box::pin(async {})
    }
}

/// Question 5: `sync_update` sends the payload with the holder's lock held, and
/// `PacketSentEvent` handlers run on the sending thread. A handler that reads the attachments
/// of the synced holder must not deadlock.
#[test]
fn a_packet_handler_can_read_the_synced_holder() {
    let world = started();
    let _serial = support::serial();
    let (player, _peer) = support::player_with_peer("ReviewDeadlock", neoforge_client());
    let ironpumpkin_mods::pumpkin_core::net::ClientPlatform::Java(client) = player.client.as_ref()
    else {
        panic!("not a Java client");
    };
    client.set_player(player.clone());
    client
        .connection_state
        .store(pumpkin_protocol::ConnectionState::Play);
    support::server()
        .plugin_manager
        .register_with::<PacketSentEvent, _>(
            Arc::new(OnPacketSent(|event: &mut PacketSentEvent| {
                if event.player.gameprofile.name == "ReviewDeadlock" {
                    let _ = event.player.get_entity().has_data(combo());
                }
            })),
            ironpumpkin_mods::pumpkin_core::plugin::EventPriority::Normal,
            true,
            false,
        );
    let (done, wait) = std::sync::mpsc::channel();
    let target = player.clone();
    std::thread::spawn(move || {
        target.get_entity().set_data(combo(), 5);
        let _ = done.send(());
    });
    assert!(
        wait.recv_timeout(Duration::from_secs(10)).is_ok(),
        "set_data deadlocked: the PacketSentEvent handler re-locks the entity's attachments"
    );
    support::RUNTIME.block_on(world.remove_player(&player, false));
}

/// Question 5: `queue_block_entity_sync` deduplicates with `Vec::contains`, so N syncs in one
/// tick cost N^2/2 comparisons.
#[test]
fn queueing_block_entity_syncs_is_linear() {
    let attachments =
        ironpumpkin_mods::pumpkin_core::plugin::attachments::WorldAttachments::default();
    let time = |count: i32| {
        let start = std::time::Instant::now();
        for i in 0..count {
            attachments.queue_block_entity_sync(BlockPos(Vector3::new(i, 64, 0)), 0);
        }
        let elapsed = start.elapsed();
        assert_eq!(attachments.take_block_entity_sync().len(), count as usize);
        elapsed
    };
    let small = time(2_500);
    let large = time(10_000);
    assert!(
        large < small * 8,
        "4x the syncs cost {:.1}x the time: {small:?} for 2500, {large:?} for 10000",
        large.as_secs_f64() / small.as_secs_f64()
    );
}
