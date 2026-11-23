#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value, and a panicking callback is the case under test"
)]

use std::collections::HashMap;

use pumpkin_core::plugin::attachments::{AttachmentCell, new_cell};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::math::{position::BlockPos, vector2::Vector2, vector3::Vector3};
use pumpkin_world::chunk::ChunkData;

use super::{
    AttachmentError, AttachmentHolder, AttachmentSerializer, AttachmentSyncHandler, AttachmentType,
    AttachmentTypeBuilder, NbtSerializable, NbtSerializer, leak_entry,
    sync::{Target, encode_payload, initial_data},
    types::{ErasedType, Types},
    validate_name,
};

fn int_serializer() -> NbtSerializer<
    impl Fn(&NbtCompound) -> Result<i32, String> + Send + Sync + 'static,
    impl Fn(&i32, &mut NbtCompound) + Send + Sync + 'static,
> {
    NbtSerializer::new(
        |nbt: &NbtCompound| nbt.get_int("v").ok_or_else(|| "no v".to_owned()),
        |value: &i32, nbt: &mut NbtCompound| nbt.put_int("v", *value),
    )
}

fn types(entries: &[&'static dyn ErasedType]) -> Types {
    Types::new(entries.to_vec())
}

type Cells = Vec<(u32, AttachmentCell)>;

fn map_of(values: Vec<(u32, Box<dyn std::any::Any + Send + Sync>)>) -> Cells {
    values
        .into_iter()
        .map(|(index, value)| (index, new_cell(value)))
        .collect()
}

fn value_of<T: Clone + 'static>(cells: &Cells, index: u32) -> Option<T> {
    let (_, cell) = cells.iter().find(|(i, _)| *i == index)?;
    cell.lock().unwrap().downcast_ref::<T>().cloned()
}

fn int(cells: &Cells, ty: AttachmentType<i32>) -> Option<i32> {
    value_of(cells, ty.index())
}

#[test]
fn names_follow_the_resource_location_rules() {
    assert_eq!(validate_name("examplemod:mana"), Ok(()));
    assert_eq!(validate_name("examplemod:a/b.c-d_e"), Ok(()));
    assert_eq!(
        validate_name("mana"),
        Err(AttachmentError::ReservedNamespace("mana".to_owned()))
    );
    assert_eq!(
        validate_name("minecraft:mana"),
        Err(AttachmentError::ReservedNamespace(
            "minecraft:mana".to_owned()
        ))
    );
    assert_eq!(
        validate_name("Example:mana"),
        Err(AttachmentError::InvalidName("Example:mana".to_owned()))
    );
    assert_eq!(
        validate_name("examplemod:"),
        Err(AttachmentError::InvalidName("examplemod:".to_owned()))
    );
}

#[test]
fn a_second_registration_of_a_name_fails() {
    let mut cx = crate::ModInit::new("attachtest");
    let first =
        cx.register_attachment_type("attachtest:twice", AttachmentTypeBuilder::new(|| 0i32));
    assert!(first.is_ok());
    let second =
        cx.register_attachment_type("attachtest:twice", AttachmentTypeBuilder::new(String::new));
    assert_eq!(
        second.unwrap_err(),
        AttachmentError::Duplicate("attachtest:twice".to_owned())
    );
    assert_eq!(first.unwrap().name(), "attachtest:twice");
}

#[test]
fn only_types_with_a_serializer_are_saved_and_read_back() {
    let saved = leak_entry(
        0,
        "test:saved",
        None,
        AttachmentTypeBuilder::new(|| 0i32).serialize(int_serializer()),
    );
    let transient = leak_entry(
        1,
        "test:transient",
        None,
        AttachmentTypeBuilder::new(|| 0i32),
    );
    let types = types(&[saved.erased(), transient.erased()]);
    let map = map_of(vec![(0, Box::new(7i32)), (1, Box::new(9i32))]);

    let nbt = types.write_map(&map).unwrap();
    assert_eq!(nbt.child_tags.len(), 1);
    assert_eq!(
        nbt.get_compound("test:saved").unwrap().get_int("v"),
        Some(7)
    );

    let chunk = ChunkData::empty(0, 0);
    let holder = AttachmentHolder::Chunk(&chunk);
    let read = map_of(types.read_map(&holder, &nbt));
    assert_eq!(int(&read, saved), Some(7));
    assert_eq!(int(&read, transient), None);
}

#[test]
fn a_holder_without_saved_types_writes_nothing() {
    let transient = leak_entry(
        0,
        "test:only_transient",
        None,
        AttachmentTypeBuilder::new(|| 0i32),
    );
    let types = types(&[transient.erased()]);
    assert!(types.write_map(&[]).is_none());
    assert!(
        types
            .write_map(&map_of(vec![(0, Box::new(1i32))]))
            .is_none()
    );
}

#[test]
fn unknown_invalid_and_broken_entries_are_skipped_on_read() {
    let saved = leak_entry(
        0,
        "test:read",
        None,
        AttachmentTypeBuilder::new(|| 0i32).serialize(int_serializer()),
    );
    let transient = leak_entry(
        1,
        "test:read_transient",
        None,
        AttachmentTypeBuilder::new(|| 0i32),
    );
    let types = types(&[saved.erased(), transient.erased()]);
    let mut nbt = NbtCompound::new();
    nbt.put_compound("Not A Key", NbtCompound::new());
    nbt.put_compound("othermod:unknown", NbtCompound::new());
    nbt.put_compound("test:read_transient", NbtCompound::new());
    // `rawChildOrEmpty`: a value that is not a compound reads as an empty compound, and the
    // serializer fails on it.
    nbt.put("test:read", NbtTag::Int(3));
    let chunk = ChunkData::empty(0, 0);
    assert!(
        types
            .read_map(&AttachmentHolder::Chunk(&chunk), &nbt)
            .is_empty()
    );
}

#[test]
fn a_serializer_that_skips_a_value_drops_its_key() {
    let ty = leak_entry(
        0,
        "test:codec",
        None,
        AttachmentTypeBuilder::new(HashMap::<String, i32>::new)
            .serialize_codec_if(|value| !value.is_empty()),
    );
    let types = types(&[ty.erased()]);
    let empty = map_of(vec![(0, Box::new(HashMap::<String, i32>::new()))]);
    assert!(types.write_map(&empty).is_none());

    let full = map_of(vec![(
        0,
        Box::new(HashMap::from([("level".to_owned(), 4i32)])),
    )]);
    let nbt = types.write_map(&full).unwrap();
    let chunk = ChunkData::empty(0, 0);
    let read = map_of(types.read_map(&AttachmentHolder::Chunk(&chunk), &nbt));
    let value = value_of::<HashMap<String, i32>>(&read, 0).unwrap();
    assert_eq!(value.get("level"), Some(&4));
}

#[derive(Clone)]
struct Counter(i32);

impl NbtSerializable for Counter {
    fn serialize(&self, nbt: &mut NbtCompound) {
        nbt.put_int("count", self.0);
    }

    fn deserialize(&mut self, nbt: &NbtCompound) {
        self.0 += nbt.get_int("count").unwrap_or(0);
    }
}

#[test]
fn a_serializable_type_reads_into_its_default() {
    let ty = leak_entry(
        0,
        "test:serializable",
        None,
        AttachmentTypeBuilder::serializable(|| Counter(100)),
    );
    let types = types(&[ty.erased()]);
    let nbt = types
        .write_map(&map_of(vec![(0, Box::new(Counter(5)))]))
        .unwrap();
    let chunk = ChunkData::empty(0, 0);
    let read = map_of(types.read_map(&AttachmentHolder::Chunk(&chunk), &nbt));
    // `AttachmentType.serializable`: the read starts from the default value.
    assert_eq!(value_of::<Counter>(&read, 0).unwrap().0, 105);
}

/// The copy rules of `AttachmentInternals.copyAttachments` on a death and on the End exit.
#[test]
fn copy_rules_follow_restore_from() {
    let kept = leak_entry(
        0,
        "test:kept",
        None,
        AttachmentTypeBuilder::new(|| 0i32)
            .serialize(int_serializer())
            .copy_on_death()
            .copy_handler(|value: &i32, _: &AttachmentHolder<'_>| Some(value + 1)),
    );
    let saved = leak_entry(
        1,
        "test:saved_only",
        None,
        AttachmentTypeBuilder::new(|| 0i32).serialize(int_serializer()),
    );
    let transient = leak_entry(
        2,
        "test:transient_only",
        None,
        AttachmentTypeBuilder::new(|| 0i32),
    );
    let dropped = leak_entry(
        3,
        "test:dropped",
        None,
        AttachmentTypeBuilder::new(|| 0i32)
            .serialize(int_serializer())
            .copy_on_death()
            .copy_handler(|_: &i32, _: &AttachmentHolder<'_>| None),
    );
    let types = types(&[
        kept.erased(),
        saved.erased(),
        transient.erased(),
        dropped.erased(),
    ]);
    let old = map_of(vec![
        (0, Box::new(10i32)),
        (1, Box::new(20i32)),
        (2, Box::new(30i32)),
        (3, Box::new(40i32)),
    ]);
    let chunk = ChunkData::empty(0, 0);
    let holder = AttachmentHolder::Chunk(&chunk);

    let death = map_of(types.copy_map(&old, &holder, |entry| entry.copy_on_death()));
    assert_eq!(int(&death, kept), Some(11), "through the copy handler");
    assert_eq!(int(&death, saved), None, "no copy on death");
    assert_eq!(int(&death, transient), None, "no serializer");
    assert_eq!(int(&death, dropped), None, "the copy handler dropped it");

    let end_exit = map_of(types.copy_map(&old, &holder, |_| true));
    assert_eq!(int(&end_exit, kept), Some(11));
    assert_eq!(int(&end_exit, saved), Some(20), "a write and a read");
    assert_eq!(int(&end_exit, transient), None, "no serializer");
    assert_eq!(int(&end_exit, dropped), None);
}

/// `BlockPos.asLong` of vanilla.
fn vanilla_block_pos(x: i64, y: i64, z: i64) -> i64 {
    ((x & 0x3FF_FFFF) << 38) | ((z & 0x3FF_FFFF) << 12) | (y & 0xFFF)
}

/// `SyncAttachmentsPayload.STREAM_CODEC`: the target (`Target.STREAM_CODEC`), the varint list of
/// ids in `neoforge:synced_attachment_types`, then the data as a varint-prefixed byte array.
#[test]
fn payload_matches_the_neoforge_codec() {
    let data = [1, 0xAA];
    let tail = |ids: &[u8]| {
        let mut tail = ids.to_vec();
        tail.extend_from_slice(&[2, 1, 0xAA]);
        tail
    };

    let mut expected = vec![0];
    expected.extend_from_slice(&vanilla_block_pos(-3, -60, 70_000).to_be_bytes());
    expected.extend(tail(&[1, 5]));
    assert_eq!(
        encode_payload(
            Target::BlockEntity(BlockPos(Vector3::new(-3, -60, 70_000))),
            &[5],
            &data
        ),
        expected
    );

    // `ChunkPos.pack`: x in the low 32 bits, z in the high 32 bits.
    let mut expected = vec![1];
    expected.extend_from_slice(&0x0000_0002_FFFF_FFFFi64.to_be_bytes());
    expected.extend(tail(&[2, 0, 1]));
    assert_eq!(
        encode_payload(Target::Chunk(Vector2::new(-1, 2)), &[0, 1], &data),
        expected
    );

    let mut expected = vec![2, 0xAC, 0x02];
    expected.extend(tail(&[1, 0x80, 0x01]));
    assert_eq!(encode_payload(Target::Entity(300), &[128], &data), expected);

    let mut expected = vec![3];
    expected.extend(tail(&[0]));
    assert_eq!(encode_payload(Target::Level, &[], &data), expected);
}

struct Bytes;

impl AttachmentSyncHandler<i32> for Bytes {
    fn write(&self, buf: &mut Vec<u8>, value: &i32, initial_sync: bool) {
        // Writes nothing on the initial sync of a zero, as a handler may.
        if !(initial_sync && *value == 0) {
            buf.push(*value as u8);
        }
    }
}

/// `AttachmentSync.syncInitialAttachments`: a boolean and the handler bytes per synced type,
/// and a type whose handler writes nothing is left out of the list.
#[test]
fn initial_data_leaves_out_types_that_write_nothing() {
    let first = leak_entry(
        0,
        "test:first",
        Some(0),
        AttachmentTypeBuilder::new(|| 0i32).sync(Bytes),
    );
    let silent = leak_entry(
        1,
        "test:silent",
        Some(1),
        AttachmentTypeBuilder::new(|| 0i32).sync(Bytes),
    );
    let unsynced = leak_entry(
        2,
        "test:unsynced",
        None,
        AttachmentTypeBuilder::new(|| 0i32),
    );
    let types = types(&[first.erased(), silent.erased(), unsynced.erased()]);

    let map = map_of(vec![
        (0, Box::new(7i32)),
        (1, Box::new(0i32)),
        (2, Box::new(9i32)),
    ]);
    let (ids, data) = initial_data(&map, &types).unwrap();
    assert_eq!(ids, vec![0]);
    assert_eq!(data, vec![1, 7]);

    assert!(initial_data(&map_of(vec![(2, Box::new(9i32))]), &types).is_none());
}

#[test]
fn synced_names_follow_the_synced_ids() {
    let a = leak_entry(
        0,
        "test:a",
        Some(1),
        AttachmentTypeBuilder::new(|| 0i32).sync(Bytes),
    );
    let b = leak_entry(1, "test:b", None, AttachmentTypeBuilder::new(|| 0i32));
    let c = leak_entry(
        2,
        "test:c",
        Some(0),
        AttachmentTypeBuilder::new(|| 0i32).sync(Bytes),
    );
    let names: Vec<String> = types(&[a.erased(), b.erased(), c.erased()])
        .synced_names()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(names, ["test:c", "test:a"]);
}

struct Panics;

impl AttachmentSerializer<i32> for Panics {
    fn read(&self, _: &AttachmentHolder<'_>, _: &NbtCompound) -> Result<i32, String> {
        panic!("read")
    }

    fn write(&self, _: &i32, _: &mut NbtCompound) -> bool {
        panic!("write")
    }
}

impl AttachmentSyncHandler<i32> for Panics {
    fn send_to_player(
        &self,
        _: &AttachmentHolder<'_>,
        _: &pumpkin_core::entity::player::Player,
    ) -> bool {
        panic!("send_to_player")
    }

    fn write(&self, _: &mut Vec<u8>, _: &i32, _: bool) {
        panic!("sync write")
    }
}

/// `AttachmentHolder#serializeAttachments` and `deserializeAttachments` catch the exception of
/// one attachment and go on: a panic of a mod callback skips only its value.
#[test]
fn a_panicking_callback_skips_only_its_value() {
    let good = leak_entry(
        0,
        "test:good",
        Some(0),
        AttachmentTypeBuilder::new(|| 0i32)
            .serialize(int_serializer())
            .sync(Bytes),
    );
    let bad = leak_entry(
        1,
        "test:bad",
        Some(1),
        AttachmentTypeBuilder::new(|| 0i32)
            .serialize(Panics)
            .copy_on_death()
            .sync(Panics),
    );
    let types = types(&[good.erased(), bad.erased()]);
    let cells = map_of(vec![(0, Box::new(7i32)), (1, Box::new(8i32))]);

    let nbt = types.write_map(&cells).unwrap();
    assert_eq!(nbt.child_tags.len(), 1);
    assert!(nbt.get_compound("test:good").is_some());

    let mut saved = nbt;
    saved.put_compound("test:bad", NbtCompound::new());
    let chunk = ChunkData::empty(0, 0);
    let holder = AttachmentHolder::Chunk(&chunk);
    let read = map_of(types.read_map(&holder, &saved));
    assert_eq!(int(&read, good), Some(7));
    assert_eq!(int(&read, bad), None);

    let copies = map_of(types.copy_map(&cells, &holder, |_| true));
    assert_eq!(int(&copies, good), Some(7));
    assert_eq!(int(&copies, bad), None);

    let (ids, data) = initial_data(&cells, &types).unwrap();
    assert_eq!(ids, vec![0]);
    assert_eq!(data, vec![1, 7]);
}
