use super::*;
use crate::chunk::format::anvil::SingleChunkDataSerializer;
use crate::chunk_system::chunk_state::StagedChunkEnum;
use crate::generation::generator::{CustomChunkGenerator, WorldGenerator};
use crate::generation::proto_chunk::ProtoChunk;
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
    save_chunks(saved).await
}

/// A level that holds `saved` on disk.
async fn save_chunks(saved: Vec<(Vector2<i32>, SyncChunk)>) -> (TempDir, Arc<Level>) {
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

/// The step of the tick pass for the chunk `pos` that does block tick.
fn step(level: &Level, pos: Vector2<i32>) -> TickData {
    let ticking = FxHashSet::from_iter([pos]);
    level.get_tick_data(&FxHashSet::default(), &ticking, 0)
}

/// A chunk that loads from disk with scheduled block and fluid ticks joins the set that the tick
/// step walks, as vanilla `LevelChunk.registerTickContainerInLevel` adds its tick containers.
/// The ticks run after their saved delay, with no other tick scheduled in the chunk.
#[tokio::test(flavor = "multi_thread")]
async fn a_chunk_loaded_from_disk_steps_its_saved_ticks() {
    let pos = Vector2::new(0, 0);
    let radius = StagedChunkEnum::FULL_RADIUS;
    let at = BlockPos::new(3, 70, 4);
    let mut saved = Vec::new();
    for x in -radius..=radius {
        for z in -radius..=radius {
            let (chunk_pos, chunk) = saved_chunk(Vector2::new(x, z), None);
            if chunk_pos == pos {
                chunk.block_ticks.schedule_tick(
                    &ScheduledTick {
                        delay: 5,
                        priority: TickPriority::Normal,
                        position: at,
                        value: &Block::REDSTONE_LAMP,
                    },
                    0,
                );
                chunk.fluid_ticks.schedule_tick(
                    &ScheduledTick {
                        delay: 3,
                        priority: TickPriority::Normal,
                        position: at.up(),
                        value: &Fluid::FLOWING_WATER,
                    },
                    1,
                );
            }
            saved.push((chunk_pos, chunk));
        }
    }
    let (_temp_dir, level) = save_chunks(saved).await;
    assert!(level.chunks_with_scheduled_ticks.is_empty());

    level.get_or_fetch_chunk(pos, |_| ()).await;
    assert!(
        level.chunks_with_scheduled_ticks.contains(&pos),
        "the loaded chunk with ticks is not in the stepped set"
    );
    assert!(
        level.chunks_with_scheduled_ticks.len() == 1,
        "a loaded chunk without ticks joined the stepped set"
    );

    for tick in 0..6 {
        let due = step(&level, pos);
        assert_eq!(
            due.fluid_ticks.len(),
            usize::from(tick == 3),
            "fluid ticks at step {tick}"
        );
        assert_eq!(
            due.block_ticks.len(),
            usize::from(tick == 5),
            "block ticks at step {tick}"
        );
    }
    assert!(!level.chunks_with_scheduled_ticks.contains(&pos));
    level.shutdown().await;
}

/// A generator that schedules a fluid tick in each chunk at the carvers stage, as the aquifer
/// and the carvers of the noise generator do. It makes empty chunks and runs fast in a debug build.
struct TickingGenerator;

const GENERATED_TICK: (i32, i32, i32) = (1, 40, 2);

impl CustomChunkGenerator for TickingGenerator {
    fn dimension(&self) -> &Dimension {
        &Dimension::OVERWORLD
    }

    fn seed(&self) -> u64 {
        0
    }

    fn step_to_carvers(&self, chunk: &mut ProtoChunk) {
        chunk.snapshot_wg_heightmaps();
        chunk.schedule_fluid_tick(
            GENERATED_TICK.0,
            GENERATED_TICK.1,
            GENERATED_TICK.2,
            &Fluid::WATER,
        );
        chunk.stage = StagedChunkEnum::Carvers;
    }
}

/// The portal that generation needs: no entity spawns and every block places.
struct NoPortal;

impl WorldPortalExt for NoPortal {
    fn can_place_at(
        &self,
        _block: &Block,
        _state: &pumpkin_data::BlockState,
        _block_accessor: &dyn crate::world::BlockAccessor,
        _block_pos: &BlockPos,
    ) -> bool {
        true
    }

    fn mirror(
        &self,
        block: &Block,
        state_id: BlockStateId,
        mirror: pumpkin_data::Mirror,
    ) -> &'static pumpkin_data::BlockState {
        block.mirror(state_id, mirror)
    }

    fn rotate(
        &self,
        block: &Block,
        state_id: BlockStateId,
        rotation: pumpkin_data::Rotation,
    ) -> &'static pumpkin_data::BlockState {
        block.rotate(state_id, rotation)
    }

    fn spawn_mobs_for_chunk_generation(
        &self,
        _cache: &mut dyn crate::generation::proto_chunk::GenerationCache,
        _biome: &'static Biome,
        _chunk_x: i32,
        _chunk_z: i32,
    ) {
    }
}

/// A chunk that generation makes with fluid ticks joins the stepped set, and its ticks step with
/// no other tick scheduled.
#[tokio::test(flavor = "multi_thread")]
async fn a_generated_chunk_steps_its_fluid_ticks() {
    let temp_dir = TempDir::new().unwrap();
    let level = Level::from_root_folder(
        &LevelConfig::default(),
        temp_dir.path().to_path_buf(),
        0,
        Dimension::OVERWORLD,
    );
    level.set_world_gen(Arc::new(WorldGenerator::Custom(Arc::new(TickingGenerator))));
    level.world_portal.store(Arc::new(Some(Arc::new(NoPortal))));

    let pos = Vector2::new(0, 0);
    assert!(
        level
            .get_or_fetch_chunk(pos, |chunk| chunk.fluid_ticks.has_ticks())
            .await,
        "the generated chunk lost its fluid tick"
    );
    assert!(
        level.chunks_with_scheduled_ticks.contains(&pos),
        "the generated chunk with fluid ticks is not in the stepped set"
    );

    let due = step(&level, pos);
    assert_eq!(
        due.fluid_ticks.len(),
        1,
        "the generated fluid tick did not step"
    );
    let at = due.fluid_ticks[0].position.0;
    assert_eq!((at.x, at.y, at.z), GENERATED_TICK);
    level.shutdown().await;
}
