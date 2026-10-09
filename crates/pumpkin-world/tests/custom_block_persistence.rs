//! Custom block states survive an Anvil save and a restart by name. The content registry is
//! process-wide and freezes once, so each server start is a child process that reruns this test
//! binary with `PERSISTENCE_PHASE` set.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use pumpkin_config::chunk::AnvilChunkConfig;
use pumpkin_data::dynamic::{self, BlockDefinition, BlockPropertyDefinition, ContentKind};
use pumpkin_data::{Block, BlockStateId};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::math::vector2::Vector2;
use pumpkin_world::chunk::ChunkData;
use pumpkin_world::chunk::format::anvil::{AnvilChunkFile, SingleChunkDataSerializer};
use pumpkin_world::chunk::io::{Dirtiable, FileIO, LoadedData, file_manager::ChunkFileManager};
use pumpkin_world::level::LevelFolder;

const PHASE: &str = "PERSISTENCE_PHASE";
const WORLD: &str = "PERSISTENCE_WORLD";
const TEST_NAME: &str = "custom_blocks_persist_by_name_across_restarts";
const LAMP: &str = "test:lamp";
const CHUNKS: [Vector2<i32>; 2] = [Vector2::new(0, 0), Vector2::new(1, 0)];
/// Two sections per chunk hold the lamp, so a per-section warning would show up twice per chunk.
const LAMP_YS: [i32; 2] = [64, 0];
const UNKNOWN_WARNING: &str = "is not registered: it loads as air";
const PROPERTY_WARNING: &str = "has properties its schema does not know";

/// Reruns this test in a child process for one server start and returns its log.
fn start(phase: &str, world: &Path) -> String {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([TEST_NAME, "--exact", "--nocapture", "--test-threads=1"])
        .env(PHASE, phase)
        .env(WORLD, world)
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&output.stderr).into_owned()
        + &String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{phase} start failed:\n{log}");
    log
}

#[test]
fn custom_blocks_persist_by_name_across_restarts() {
    if let Ok(phase) = std::env::var(PHASE) {
        tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(std::io::stderr)
            .init();
        let world = PathBuf::from(std::env::var(WORLD).unwrap());
        match phase.as_str() {
            "save" => start_and_place(&world),
            "same" => start_and_reload(&world, Registration::Same),
            "shifted" => start_and_reload(&world, Registration::Shifted),
            "placeholder" => start_and_reload(&world, Registration::Placeholder),
            "changed" => start_with_changed_schema(&world),
            "unknown" => start_without_the_block(&world),
            _ => panic!("unknown phase {phase}"),
        }
        return;
    }

    let world = tempfile::tempdir().unwrap();
    let log = start("save", world.path());
    assert!(!log.contains("in a chunk palette"), "{log}");
    let saved = std::fs::read_to_string(world.path().join("save.palette")).unwrap();

    for phase in ["same", "shifted", "placeholder", "same"] {
        let log = start(phase, world.path());
        assert!(!log.contains("in a chunk palette"), "{phase}:\n{log}");
        let palette =
            std::fs::read_to_string(world.path().join(format!("{phase}.palette"))).unwrap();
        assert_eq!(palette, saved, "{phase} saves a different palette");
    }

    let log = start("changed", world.path());
    assert_eq!(log.matches(PROPERTY_WARNING).count(), 1, "{log}");
    assert!(log.contains("facing="), "{log}");
    assert!(!log.contains(UNKNOWN_WARNING), "{log}");

    let log = start("unknown", world.path());
    assert_eq!(log.matches(UNKNOWN_WARNING).count(), 1, "{log}");
    assert!(log.contains("Block test:lamp in a chunk palette"), "{log}");
}

fn lamp_definition() -> BlockDefinition {
    BlockDefinition {
        name: LAMP.to_string(),
        display: Block::REDSTONE_LAMP.default_state.id,
        properties: vec![
            BlockPropertyDefinition::enumeration(
                "facing",
                &["north", "south", "west", "east"],
                "west",
            ),
            BlockPropertyDefinition::bool("lit", false),
        ],
        tags: Vec::new(),
    }
}

fn level_folder(world: &Path) -> LevelFolder {
    let region_folder = world.join("region");
    std::fs::create_dir_all(&region_folder).unwrap();
    LevelFolder {
        root_folder: world.to_path_buf(),
        dim_folder: world.to_path_buf(),
        region_folder,
        entities_folder: world.join("entities"),
        poi_folder: world.join("poi"),
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn save(world: &Path, chunks: Vec<Arc<ChunkData>>) {
    let folder = level_folder(world);
    let manager = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(AnvilChunkConfig::default());
    let data = chunks
        .into_iter()
        .map(|chunk| {
            chunk.mark_dirty(true);
            (Vector2::new(chunk.x, chunk.z), chunk)
        })
        .collect();
    runtime()
        .block_on(manager.save_chunks(&folder, data))
        .unwrap();
}

fn load(world: &Path) -> Vec<Arc<ChunkData>> {
    let folder = level_folder(world);
    let manager = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(AnvilChunkConfig::default());
    let (send, mut receive) = tokio::sync::mpsc::channel(CHUNKS.len());
    let mut chunks = Vec::new();
    runtime().block_on(async {
        let fetch = manager.fetch_chunks(&folder, &CHUNKS, send);
        let collect = async {
            while let Some(data) = receive.recv().await {
                match data {
                    LoadedData::Loaded(chunk) => chunks.push(chunk),
                    LoadedData::Missing(position) => panic!("chunk {position:?} is missing"),
                    LoadedData::Error((position, error)) => {
                        panic!("chunk {position:?} failed: {error:?}")
                    }
                }
            }
        };
        tokio::join!(fetch, collect);
    });
    chunks.sort_by_key(|chunk| chunk.x);
    assert_eq!(chunks.len(), CHUNKS.len());
    chunks
}

fn sorted_props(id: BlockStateId) -> String {
    let mut props: Vec<String> = Block::from_state_id(id)
        .properties(id)
        .map(|props| props.to_props())
        .unwrap_or_default()
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    props.sort();
    props.join(",")
}

fn describe(id: BlockStateId) -> String {
    let block = Block::from_state_id(id);
    format!(
        "{} {} [{}]",
        id.as_u16(),
        dynamic::namespaced_name(block.name),
        sorted_props(id)
    )
}

/// One line per placed block: chunk x, block x, block y, raw state id, name and properties.
fn placed_blocks(chunks: &[Arc<ChunkData>]) -> String {
    let mut lines = String::new();
    for chunk in chunks {
        for y in LAMP_YS {
            for x in 0..10 {
                let id = chunk.section.get_block_absolute_y(x, y, 0).unwrap();
                writeln!(lines, "{} {x} {y} {}", chunk.x, describe(id)).unwrap();
            }
        }
    }
    lines
}

fn canonical(tag: &NbtTag) -> String {
    match tag {
        NbtTag::Compound(compound) => {
            let mut entries: Vec<String> = compound
                .child_tags
                .iter()
                .map(|(key, value)| format!("{key}:{}", canonical(value)))
                .collect();
            entries.sort();
            format!("{{{}}}", entries.join(","))
        }
        NbtTag::List(list) => {
            format!(
                "[{}]",
                list.iter().map(canonical).collect::<Vec<_>>().join(",")
            )
        }
        other => format!("{other:?}"),
    }
}

/// The block palette entries of every section, as written by the chunk serializer.
fn palette_entries(chunk: &ChunkData) -> Vec<NbtCompound> {
    let bytes = chunk.to_bytes().unwrap();
    let mut cursor = std::io::Cursor::new(&bytes[..]);
    let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(
        pumpkin_nbt::deserializer::NbtStreamReader(&mut cursor),
    );
    let root = pumpkin_nbt::Nbt::read(&mut reader).unwrap().root_tag;
    root.get_list("sections")
        .unwrap()
        .iter()
        .filter_map(NbtTag::extract_compound)
        .flat_map(|section| {
            section
                .get_compound("block_states")
                .and_then(|states| states.get_list("palette"))
                .into_iter()
                .flatten()
                .filter_map(NbtTag::extract_compound)
                .cloned()
        })
        .collect()
}

/// The canonical palette of each chunk, with entries sorted, so that palette order does not count.
fn palette_dump(chunks: &[Arc<ChunkData>]) -> String {
    let mut dump = String::new();
    for chunk in chunks {
        let mut entries: Vec<String> = palette_entries(chunk)
            .iter()
            .map(|entry| canonical(&NbtTag::Compound(entry.clone())))
            .collect();
        entries.sort();
        writeln!(dump, "chunk {}: {}", chunk.x, entries.join(" ")).unwrap();
    }
    dump
}

fn start_and_place(world: &Path) {
    dynamic::register_block(lamp_definition()).unwrap();
    dynamic::freeze().unwrap();
    let lamp = Block::from_name(LAMP).unwrap();
    assert_eq!(lamp.states.len(), 8);

    let chunks: Vec<Arc<ChunkData>> = CHUNKS
        .iter()
        .map(|position| {
            let chunk = ChunkData::empty(position.x, position.y);
            for y in LAMP_YS {
                for (x, state) in lamp.states.iter().enumerate() {
                    chunk.set_block_absolute_y(x, y, 0, state.id);
                }
                chunk.set_block_absolute_y(8, y, 0, Block::STONE.default_state.id);
                chunk.set_block_absolute_y(9, y, 0, Block::REPEATER.states[5].id);
            }
            Arc::new(chunk)
        })
        .collect();

    // Every lamp state is written as its namespaced name with all its properties.
    for chunk in &chunks {
        let entries = palette_entries(chunk);
        for state in lamp.states {
            let mut properties = NbtCompound::new();
            for (name, value) in lamp.properties(state.id).unwrap().to_props() {
                properties.put_string(name, value.to_string());
            }
            let mut expected = NbtCompound::new();
            expected.put_string("Name", LAMP.to_string());
            expected.put_compound("Properties", properties);
            assert_eq!(
                entries.iter().filter(|entry| **entry == expected).count(),
                LAMP_YS.len(),
                "{expected:?} in {entries:?}"
            );
        }
        assert!(!entries.iter().any(|entry| {
            entry
                .get_string("Name")
                .is_some_and(|name| name.starts_with("minecraft:test"))
        }));
    }

    std::fs::write(world.join("placed.txt"), placed_blocks(&chunks)).unwrap();
    std::fs::write(world.join("save.palette"), palette_dump(&chunks)).unwrap();
    save(world, chunks);
}

#[derive(Clone, Copy)]
enum Registration {
    /// The mod registers the block as before: every state keeps its numeric id.
    Same,
    /// Another mod registers a block first, so the lamp's numeric ids move.
    Shifted,
    /// The mod is gone and the content manifest restores the block as a placeholder.
    Placeholder,
}

fn start_and_reload(world: &Path, registration: Registration) {
    let phase = match registration {
        Registration::Same => {
            dynamic::register_block(lamp_definition()).unwrap();
            "same"
        }
        Registration::Shifted => {
            dynamic::register_block(BlockDefinition {
                name: "test:aaa_first".to_string(),
                display: Block::STONE.default_state.id,
                properties: vec![BlockPropertyDefinition::int("level", 0, 2, 0)],
                tags: Vec::new(),
            })
            .unwrap();
            dynamic::register_block(lamp_definition()).unwrap();
            "shifted"
        }
        Registration::Placeholder => {
            dynamic::register_placeholder_block(lamp_definition()).unwrap();
            "placeholder"
        }
    };
    dynamic::freeze().unwrap();
    assert_eq!(
        dynamic::is_placeholder(ContentKind::Block, LAMP),
        matches!(registration, Registration::Placeholder)
    );

    let placed = std::fs::read_to_string(world.join("placed.txt")).unwrap();
    let chunks = load(world);
    let loaded = placed_blocks(&chunks);
    match registration {
        Registration::Same | Registration::Placeholder => assert_eq!(loaded, placed),
        Registration::Shifted => {
            assert_ne!(loaded, placed, "the lamp's ids did not move");
            assert_eq!(without_ids(&loaded), without_ids(&placed));
        }
    }

    std::fs::write(
        world.join(format!("{phase}.palette")),
        palette_dump(&chunks),
    )
    .unwrap();
    if matches!(registration, Registration::Placeholder) {
        save(world, chunks);
    }
}

fn without_ids(lines: &str) -> String {
    lines
        .lines()
        .map(|line| {
            let mut fields: Vec<&str> = line.split(' ').collect();
            fields.remove(3);
            fields.join(" ") + "\n"
        })
        .collect()
}

/// A mod update drops the `facing` property: `lit` keeps its saved value.
fn start_with_changed_schema(world: &Path) {
    dynamic::register_block(BlockDefinition {
        name: LAMP.to_string(),
        display: Block::REDSTONE_LAMP.default_state.id,
        properties: vec![BlockPropertyDefinition::bool("lit", false)],
        tags: Vec::new(),
    })
    .unwrap();
    dynamic::freeze().unwrap();

    let placed = std::fs::read_to_string(world.join("placed.txt")).unwrap();
    let loaded = placed_blocks(&load(world));
    for (before, after) in placed.lines().zip(loaded.lines()) {
        if !before.contains(LAMP) {
            assert_eq!(without_ids(before), without_ids(after));
            continue;
        }
        let lit = |line: &str| line.contains("lit=true");
        assert!(after.contains(LAMP), "{after}");
        assert!(!after.contains("facing="), "{after}");
        assert_eq!(lit(before), lit(after), "{before} -> {after}");
    }
}

/// No mod and no manifest entry: the lamp loads as air and the vanilla blocks stay.
fn start_without_the_block(world: &Path) {
    dynamic::freeze().unwrap();
    assert!(Block::from_name(LAMP).is_none());

    let placed = std::fs::read_to_string(world.join("placed.txt")).unwrap();
    let loaded = placed_blocks(&load(world));
    let air = format!("{} minecraft:air []", BlockStateId::AIR.as_u16());
    for (before, after) in placed.lines().zip(loaded.lines()) {
        if before.contains(LAMP) {
            assert!(after.ends_with(&air), "{after}");
        } else {
            assert_eq!(before, after);
        }
    }
}
