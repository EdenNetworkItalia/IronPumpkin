use super::*;
use crate::chunk::format::anvil::SingleChunkDataSerializer;
use crate::chunk_system::chunk_state::StagedChunkEnum;
use pumpkin_config::world::LevelConfig;
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use std::task::Poll;
use tempfile::TempDir;

/// A saved Full chunk at `pos`, lit so that it loads without a relight, with `structures`
/// as its `structures` tag.
fn saved_chunk(pos: Vector2<i32>, structures: Option<NbtCompound>) -> (Vector2<i32>, SyncChunk) {
    let mut root = NbtCompound::new();
    root.put_int("DataVersion", 4903);
    root.put_int("xPos", pos.x);
    root.put_int("zPos", pos.y);
    root.put_int("yPos", -4);
    root.put_string("Status", "minecraft:full".to_string());
    root.put_bool("isLightOn", true);
    root.put_list("sections", Vec::new());
    if let Some(structures) = structures {
        root.put("structures", NbtTag::Compound(structures));
    }
    let bytes = pumpkin_nbt::Nbt::new(String::new(), root).write();
    let chunk = ChunkData::from_bytes(&bytes, pos).expect("saved chunk parses");
    chunk.mark_dirty(true);
    (pos, Arc::new(chunk))
}

/// A position whose chunk is saved but not loaded, inside a mineshaft that starts 8 chunks
/// away: the lookup loads the position chunk, misses the start in the cache and loads the
/// start chunk. Every chunk that the two fetches touch is saved as Full, so nothing is
/// generated, and the start chunk lies outside the radius of the position fetch.
#[tokio::test(flavor = "multi_thread")]
async fn structure_starts_at_fetches_unloaded_chunks() {
    use crate::generation::structure::start::pack_chunk_pos;
    use pumpkin_data::structures::StructureKeys;

    let pos = BlockPos::new(5, 20, 5);
    let pos_chunk = pos.chunk_position();
    let start_chunk = Vector2::new(8, 0);

    let mut piece = NbtCompound::new();
    piece.put_string("id", "minecraft:mineshaft_corridor".into());
    piece.put("BB", NbtTag::IntArray(vec![2, 10, 2, 140, 30, 14]));
    let mut start = NbtCompound::new();
    start.put_string("id", "minecraft:mineshaft".into());
    start.put_int("ChunkX", start_chunk.x);
    start.put_int("ChunkZ", start_chunk.y);
    start.put_list("Children", vec![NbtTag::Compound(piece)]);
    let mut starts = NbtCompound::new();
    starts.put("minecraft:mineshaft", NbtTag::Compound(start));
    let mut references = NbtCompound::new();
    references.put(
        "minecraft:mineshaft",
        NbtTag::LongArray(vec![pack_chunk_pos(start_chunk.x, start_chunk.y)]),
    );
    let mut start_structures = NbtCompound::new();
    start_structures.put("starts", NbtTag::Compound(starts));
    start_structures.put("References", NbtTag::Compound(references.clone()));
    let mut pos_structures = NbtCompound::new();
    pos_structures.put("References", NbtTag::Compound(references));

    let radius = StagedChunkEnum::FULL_RADIUS;
    let mut saved = Vec::new();
    for x in pos_chunk.x - radius..=start_chunk.x + radius {
        for z in pos_chunk.y - radius..=pos_chunk.y + radius {
            let chunk = Vector2::new(x, z);
            let structures = if chunk == pos_chunk {
                Some(pos_structures.clone())
            } else if chunk == start_chunk {
                Some(start_structures.clone())
            } else {
                None
            };
            saved.push(saved_chunk(chunk, structures));
        }
    }

    let temp_dir = TempDir::new().unwrap();
    let level = Level::from_root_folder(
        &LevelConfig::default(),
        temp_dir.path().to_path_buf(),
        0,
        Dimension::OVERWORLD,
    );
    level.write_chunks(saved).await;
    assert_eq!(level.loaded_chunk_count(), 0);
    let world_gen = level.world_gen.load_full();
    let cache = world_gen
        .global_structure_cache()
        .expect("the noise generator has a structure cache");

    level.get_or_fetch_chunk(pos_chunk, |_| ()).await;
    assert!(
        cache
            .get_start_for_structure(StructureKeys::Mineshaft, start_chunk)
            .is_none(),
        "the position fetch alone recorded the start"
    );
    assert!(level.read_chunk_sync(&start_chunk, |_| ()).is_none());

    let starts = level.structure_starts_at(&pos).await;
    assert!(
        starts
            .iter()
            .any(|start| start.structure == StructureKeys::Mineshaft
                && start.chunk_pos == start_chunk),
        "no mineshaft at {pos:?}: {starts:?}"
    );
    assert!(
        cache
            .get_start_for_structure(StructureKeys::Mineshaft, start_chunk)
            .is_some()
    );
    level.shutdown().await;
}

/// A level where every chunk within `FULL_RADIUS` of `center` is saved as Full, so a fetch
/// of `center` loads from disk and generates nothing.
async fn level_with_saved_area(center: Vector2<i32>) -> (TempDir, Arc<Level>) {
    let radius = StagedChunkEnum::FULL_RADIUS;
    let saved = (center.x - radius..=center.x + radius)
        .flat_map(|x| (center.y - radius..=center.y + radius).map(move |z| (x, z)))
        .map(|(x, z)| saved_chunk(Vector2::new(x, z), None))
        .collect();
    let temp_dir = TempDir::new().unwrap();
    let level = Level::from_root_folder(
        &LevelConfig::default(),
        temp_dir.path().to_path_buf(),
        0,
        Dimension::OVERWORLD,
    );
    level.write_chunks(saved).await;
    (temp_dir, level)
}

fn has_ticket(level: &Level, pos: Vector2<i32>) -> bool {
    level
        .chunk_loading
        .lock()
        .unwrap()
        .ticket
        .contains_key(&pos)
}

async fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let wait = async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    assert!(
        timeout(Duration::from_secs(10), wait).await.is_ok(),
        "timed out waiting until {what}"
    );
}

/// The fetch completes, its ticket goes and an unload pass drops the chunk before the waiter
/// resumes. The waiter must not put the chunk back into `loaded_chunks` with no holder.
#[tokio::test(flavor = "multi_thread")]
async fn fetch_resumed_after_unload_pass_keeps_chunk_unloaded() {
    let pos = Vector2::new(0, 0);
    let (_temp_dir, level) = level_with_saved_area(pos).await;

    let mut fetch = Box::pin(level.get_or_fetch_chunk(pos, |_| ()));
    let first_poll = std::future::poll_fn(|cx| Poll::Ready(fetch.as_mut().poll(cx))).await;
    assert!(first_poll.is_pending());
    wait_until("the scheduler publishes the chunk", || {
        level.is_chunk_loaded(&pos)
    })
    .await;

    // The waiter does not yield between its ticket removal and its next step, so releasing
    // the ticket from outside is the only way to run an unload pass in that gap.
    {
        let mut loading = level.chunk_loading.lock().unwrap();
        loading.remove_ticket(pos, ChunkLoading::FULL_CHUNK_LEVEL);
        loading.send_change();
    };
    wait_until("an unload pass drops the chunk", || {
        level.should_unload.store(true, Ordering::Relaxed);
        level.level_channel.notify();
        !level.is_chunk_loaded(&pos)
    })
    .await;

    fetch.await;
    assert!(
        !level.is_chunk_loaded(&pos) || has_ticket(&level, pos),
        "chunk {pos:?} is loaded with no ticket"
    );
    level.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn dropped_fetch_releases_its_ticket() {
    let pos = Vector2::new(0, 0);
    let (_temp_dir, level) = level_with_saved_area(pos).await;

    let mut fetch = Box::pin(level.fetch_chunk(pos));
    let first_poll = std::future::poll_fn(|cx| Poll::Ready(fetch.as_mut().poll(cx))).await;
    assert!(first_poll.is_pending());
    assert!(has_ticket(&level, pos));

    drop(fetch);
    assert!(
        !has_ticket(&level, pos),
        "the dropped fetch kept its ticket"
    );
    level.shutdown().await;
}

/// The chunk is published for another holder after the caller's cache miss and before the
/// fetch registers its listener, so the scheduler never notifies the fetch.
#[tokio::test(flavor = "multi_thread")]
async fn fetch_finds_chunk_published_before_its_listener() {
    let pos = Vector2::new(0, 0);
    let (_temp_dir, level) = level_with_saved_area(pos).await;
    {
        let mut loading = level.chunk_loading.lock().unwrap();
        loading.add_ticket(pos, ChunkLoading::FULL_CHUNK_LEVEL);
        loading.send_change();
    };
    wait_until("the scheduler publishes the chunk", || {
        level.is_chunk_loaded(&pos)
    })
    .await;

    let fetched = timeout(Duration::from_secs(5), level.fetch_chunk(pos))
        .await
        .expect("the fetch never woke up");
    assert!(
        level
            .read_chunk_sync(&pos, |chunk| Arc::ptr_eq(chunk, &fetched))
            .unwrap_or(false)
    );
    level.shutdown().await;
}
