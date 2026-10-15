//! A server for the event tests and the dispatch bench: the vanilla data files and the world
//! live in a directory under `CARGO_TARGET_TMPDIR`, and the server runs no ticker and binds no
//! port. The first call creates it and changes the working directory of the process.
#![allow(dead_code, reason = "each test binary uses a part of the support")]
#![expect(
    clippy::unwrap_used,
    reason = "the support fails the test on the first error"
)]

use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError},
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

pub static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
});

static SERVER: LazyLock<Arc<Server>> = LazyLock::new(|| {
    // One directory per test binary: the next run of the same binary replaces it.
    let binary = std::env::current_exe().unwrap();
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(binary.file_stem().unwrap());
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
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
    let server = server();
    RUNTIME.block_on(async {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address: SocketAddr = listener.local_addr().unwrap();
        let (stream, _) = tokio::join!(tokio::net::TcpStream::connect(address), async {
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
        let client = JavaClient::from_pending(
            pending,
            profile.clone(),
            PlayerConfig::default(),
            NegotiatedState::default(),
        );
        let world = world();
        let player = Arc::new(Player::new(
            Arc::new(ClientPlatform::Java(client)),
            profile,
            PlayerConfig::default(),
            &world,
            GameMode::Survival,
        ));
        world.add_player(&player).unwrap();
        player
    })
}
