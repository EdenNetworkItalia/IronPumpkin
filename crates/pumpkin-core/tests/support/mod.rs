//! A server for the event tests and the dispatch bench: the vanilla data files and the world
//! live in a directory of the process under `CARGO_TARGET_TMPDIR`, and the server runs no ticker
//! and binds no port. The first call creates it and changes the working directory of the process.
#![allow(dead_code, reason = "each test binary uses a part of the support")]
#![expect(
    clippy::unwrap_used,
    reason = "the support fails the test on the first error"
)]

use std::{
    fs::File,
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex, MutexGuard, OnceLock, PoisonError},
    time::Duration,
};

use arc_swap::ArcSwap;
use pumpkin_config::{AdvancedConfiguration, BasicConfiguration, TelemetryConfig};
use pumpkin_core::{
    data::VanillaData,
    entity::player::Player,
    net::{
        ClientPlatform, GameProfile, PlayerConfig,
        java::{JavaClient, neoforge::NegotiatedState, pending::PendingConnection},
        packet_limiter::PacketRateLimiter,
    },
    server::Server,
    world::World,
};
use pumpkin_util::GameMode;
use tokio::runtime::Runtime;

/// Deadline for waits on chunk generation or a server round trip: debug build, parallel tests, CI.
pub const SLOW_RUNNER: Duration = Duration::from_secs(180);

pub static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
});

/// The lock that marks the directory of this process as in use. The OS releases it when the
/// process ends, however it ends.
static DIR_LOCK: OnceLock<File> = OnceLock::new();

/// Creates the directory of this process and removes the directories of ended processes of the
/// same binary. A test runner with one process per test (`cargo nextest`) starts many processes
/// of one binary at once, so they cannot share a directory.
fn process_dir() -> PathBuf {
    let binary = std::env::current_exe().unwrap();
    let name = binary.file_stem().unwrap().to_string_lossy().into_owned();
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(&root).unwrap();
    remove_ended_dirs(&root, &name);

    let id = format!("{name}-{}", std::process::id());
    // The lock comes before the directory, so the scan never finds a directory in creation.
    let lock = File::create(root.join(format!("{id}.lock"))).unwrap();
    lock.lock().unwrap();
    DIR_LOCK.set(lock).unwrap();
    let dir = root.join(id);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Removes each `<name>-<pid>` directory in `root` whose lock nobody holds.
fn remove_ended_dirs(root: &Path, name: &str) {
    let prefix = format!("{name}-");
    for entry in std::fs::read_dir(root).unwrap().flatten() {
        let lock = entry.path();
        let Some(id) = lock
            .file_name()
            .and_then(|file| file.to_str())
            .and_then(|file| file.strip_suffix(".lock"))
        else {
            continue;
        };
        let Some(pid) = id.strip_prefix(&prefix) else {
            continue;
        };
        if pid.is_empty() || !pid.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let Ok(file) = File::options().write(true).open(&lock) else {
            continue;
        };
        if file.try_lock().is_err() {
            continue;
        }
        // Windows refuses to delete a file that is open.
        drop(file);
        let _ = std::fs::remove_dir_all(root.join(id));
        let _ = std::fs::remove_file(&lock);
    }
}

static SERVER: LazyLock<Arc<Server>> = LazyLock::new(|| {
    let dir = process_dir();
    // `VanillaData::load` and the plugin data folders use the working directory.
    std::env::set_current_dir(&dir).unwrap();

    let basic = BasicConfiguration {
        default_level_name: dir.join("world").to_string_lossy().into_owned(),
        allow_nether: false,
        allow_end: false,
        allow_chat_reports: false,
        ..BasicConfiguration::default()
    };
    let mut advanced = AdvancedConfiguration::default();
    advanced.networking.bedrock.enabled = false;
    RUNTIME.block_on(async {
        Server::new(
            basic,
            advanced,
            TelemetryConfig::default(),
            VanillaData::load(),
            Vec::new(),
        )
        .await
        .unwrap()
    })
});

/// The shared server.
pub fn server() -> &'static Arc<Server> {
    &SERVER
}

static SERIAL: Mutex<()> = Mutex::new(());

/// Runs the tests that hold it one at a time: they share the players of one world, and a
/// broadcast of one test reaches the players of another.
pub fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The overworld of the shared server.
pub fn world() -> Arc<World> {
    server().worlds.load()[0].clone()
}

/// Joins a player named `name` to the overworld, over a loopback socket that nobody reads.
pub fn player(name: &str) -> Arc<Player> {
    join(name, false, NegotiatedState::default()).0
}

/// Joins a player like [`player`], with a packet writer, so a packet that waits for the writer
/// returns once the write to the closed socket fails.
pub fn player_with_writer(name: &str) -> Arc<Player> {
    join(name, true, NegotiatedState::default()).0
}

/// Joins a player like [`player_with_writer`], whose connection negotiated `negotiated`, and
/// returns the client end of its socket, where the packets of the player arrive.
pub fn player_with_peer(
    name: &str,
    negotiated: NegotiatedState,
) -> (Arc<Player>, tokio::net::TcpStream) {
    join(name, true, negotiated)
}

fn join(
    name: &str,
    writer: bool,
    negotiated: NegotiatedState,
) -> (Arc<Player>, tokio::net::TcpStream) {
    let server = server();
    RUNTIME.block_on(async {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address: SocketAddr = listener.local_addr().unwrap();
        let (stream, (peer, _)) = tokio::join!(tokio::net::TcpStream::connect(address), async {
            listener.accept().await.unwrap()
        });
        let stream = stream.unwrap();
        let pending = PendingConnection::new(
            stream,
            address,
            0,
            PacketRateLimiter::new(false, 0.0, 0.0),
            Arc::downgrade(server),
        );
        let profile = GameProfile {
            id: uuid::Uuid::new_v4(),
            name: name.to_owned(),
            properties: ArcSwap::from_pointee(Vec::new()),
            profile_actions: None,
        };
        let mut client = JavaClient::from_pending(
            pending,
            profile.clone(),
            PlayerConfig::default(),
            negotiated,
        );
        if writer {
            client.start_outgoing_packet_task();
        }
        let world = world();
        let player = Arc::new(Player::new(
            Arc::new(ClientPlatform::Java(client)),
            profile,
            PlayerConfig::default(),
            &world,
            GameMode::Survival,
        ));
        world.add_player(&player).unwrap();
        (player, peer)
    })
}
