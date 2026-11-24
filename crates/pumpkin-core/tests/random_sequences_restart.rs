//! The random sequences of a server go on after a stop and a start (issue #193): vanilla saves
//! `random_sequences.dat` on stop, on autosave and on `/save-all`.
#![expect(clippy::unwrap_used, reason = "a test fails on the first error")]

use std::{path::Path, sync::Mutex};

use pumpkin_config::{AdvancedConfiguration, BasicConfiguration, TelemetryConfig};
use pumpkin_core::{data::VanillaData, server::Server, world::random_sequences::RandomSequences};
use pumpkin_util::{identifier::Identifier, random::RandomImpl, world_seed::Seed};
use pumpkin_world::world_info::data_files::read_random_sequences;

const KEY: &str = "minecraft:chests/simple_dungeon";

async fn start(dir: &Path) -> std::sync::Arc<Server> {
    let basic = BasicConfiguration {
        seed: Seed(0),
        default_level_name: dir.join("world").to_string_lossy().into_owned(),
        allow_nether: false,
        allow_end: false,
        allow_chat_reports: false,
        ..BasicConfiguration::default()
    };
    let mut advanced = AdvancedConfiguration::default();
    advanced.networking.bedrock.enabled = false;
    Server::new(
        basic,
        advanced,
        TelemetryConfig::default(),
        VanillaData::load(),
        Vec::new(),
    )
    .await
    .unwrap()
}

/// Draws `count` values of the sequence `KEY` of `sequences` the way a roll does.
fn draw(sequences: &Mutex<RandomSequences>, world_seed: i64, count: usize) -> Vec<i64> {
    let id = Identifier::parse(KEY).unwrap();
    let handle = sequences
        .lock()
        .unwrap()
        .get_or_create(&id, world_seed)
        .handle();
    let mut sequence = handle.check_out();
    (0..count).map(|_| sequence.rng().next_i64()).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_saves_the_sequences_and_a_start_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    // `VanillaData::load` and the plugin data folders use the working directory.
    std::env::set_current_dir(dir.path()).unwrap();

    let first = start(dir.path()).await;
    let world_seed = first.level_info.load().world_gen_settings.seed;
    let stream = draw(&Mutex::new(RandomSequences::new()), world_seed, 5);
    assert_eq!(draw(&first.random_sequences, world_seed, 2), stream[..2]);
    first.shutdown().await;
    let saved = read_random_sequences(&dir.path().join("world"));
    assert!(
        saved.sequences.contains_key(KEY),
        "the stop saved the sequence"
    );
    drop(first);

    let second = start(dir.path()).await;
    assert_eq!(draw(&second.random_sequences, world_seed, 3), stream[2..]);
}
