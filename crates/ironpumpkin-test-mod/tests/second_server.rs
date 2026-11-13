//! Starts an embedded server with the test mod, stops it, and starts a second one in the same
//! process, as an integrated server opens one world after another. The mod loads once, with the
//! first server, as `NeoForge` loads the mods once per process; the synced configs and the server
//! lifecycle events run for each server, as `ServerLifecycleHooks` does for each
//! `MinecraftServer`. On both servers, the mod's command, permission, service, event handlers,
//! config poller and payload handler work, each handler once per fire. A `world_restart` value
//! keeps its value until its server stops, and the next server reads the edited file.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

use std::{
    net::{Ipv4Addr, SocketAddr, TcpListener},
    path::Path,
    time::Duration,
};

use ironpumpkin_mods::{
    network::ConnectionProtocol,
    pumpkin_core::{
        PumpkinServer, STOP_INTERRUPT,
        command::CommandSender,
        data::VanillaData,
        plugin::{PluginMetadata, startup},
        server::Server,
    },
};
use ironpumpkin_test_mod::{
    COMMAND, COMMAND_PERMISSION, ECHO_CHANNEL, ECHO_REPLY_CHANNEL, SERVICE, SYNCED_CONFIG,
    TestModService, event_count, synced_damage,
};
use pumpkin_config::{AdvancedConfiguration, BasicConfiguration, TelemetryConfig};
use pumpkin_neoforge_client::{
    channels::ChannelMap,
    record::{Direction, State},
    script::Script,
    session::{self, Options},
};

const TIMEOUT: Duration = Duration::from_mins(3);
const RELOADED: &str = "ModConfigEvent.Reloading SYNCED";

/// The steps of the mod loading, which run with the first server only.
const ONCE: [&str; 7] = [
    "FMLConstructModEvent",
    "ModConfigEvent.Loading STARTUP",
    "EntityAttributeCreationEvent",
    "ModConfigEvent.Loading LOCAL",
    "FMLCommonSetupEvent",
    "FMLLoadCompleteEvent",
    "RegisterPayloadHandlersEvent",
];

/// The events of each server, a run of the mod's command, an echo payload and a reload of the
/// synced config on each server.
const PER_SERVER: [&str; 9] = [
    "ModConfigEvent.Loading SYNCED",
    "ServerAboutToStartEvent",
    "ServerStartingEvent",
    "ServerStartedEvent",
    "ServerStoppingEvent",
    "ServerStoppedEvent",
    "command",
    "echo",
    RELOADED,
];

fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn configs(dir: &Path, port: u16) -> (BasicConfiguration, AdvancedConfiguration) {
    let basic = BasicConfiguration {
        default_level_name: dir.join("world").to_string_lossy().into_owned(),
        allow_nether: false,
        allow_end: false,
        allow_chat_reports: false,
        detect_neoforge_clients: true,
        mod_config_reload_ticks: 2,
        ..BasicConfiguration::default()
    };
    let mut advanced = AdvancedConfiguration::default();
    advanced.networking.java.address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    advanced.networking.java.online_mode = false;
    advanced.networking.bedrock.enabled = false;
    advanced.networking.query.enabled = false;
    advanced.networking.lan_broadcast.enabled = false;
    advanced.update_checker.enabled = false;
    (basic, advanced)
}

/// The channel map of a `NeoForge` 26.3 client with the test mod.
fn channels(dir: &Path) -> ChannelMap {
    let client = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/pumpkin-neoforge-client");
    let mut text = std::fs::read_to_string(client.join("channels/neoforge-26.3.toml")).unwrap();
    text.push_str(&std::fs::read_to_string(client.join("channels/test-mod.toml")).unwrap());
    let path = dir.join("channels.toml");
    std::fs::write(&path, text).unwrap();
    ChannelMap::load(&path).unwrap()
}

/// Sets `general.damage` in the synced config file. The rename keeps the poller from reading a
/// half-written file.
fn set_damage(dir: &Path, from: i64, to: i64) {
    let path = dir.join("config").join(SYNCED_CONFIG);
    let text = std::fs::read_to_string(&path).unwrap();
    let edited = text.replace(&format!("damage = {from}\n"), &format!("damage = {to}\n"));
    assert_ne!(edited, text, "{}", path.display());
    let temporary = dir.join("synced.toml.tmp");
    std::fs::write(&temporary, edited).unwrap();
    std::fs::rename(temporary, path).unwrap();
}

async fn wait_until(what: &str, done: impl Fn() -> bool) {
    tokio::time::timeout(TIMEOUT, async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no {what} within {TIMEOUT:?}"));
}

/// Joins `server` with the headless client, which echoes the mod's ping on [`ECHO_CHANNEL`].
/// The reply on [`ECHO_REPLY_CHANNEL`] comes from the mod's payload handler.
async fn echo(port: u16, channels: &ChannelMap) {
    let options = Options {
        host: Ipv4Addr::LOCALHOST.to_string(),
        port,
        username: "EchoProbe".to_owned(),
        brand: "neoforge".to_owned(),
        channels: Some(channels.clone()),
        timeout: TIMEOUT,
    };
    let steps = format!("steps = [\"join\", \"await_payload {ECHO_REPLY_CHANNEL}\", \"leave\"]");
    let script = Script::parse(&steps, false).unwrap();
    let mut replies = 0;
    session::play(&options, &script, &mut |entry| {
        if entry.direction == Direction::Clientbound
            && entry.state == State::Play
            && entry.channel.as_deref() == Some(ECHO_REPLY_CHANNEL)
        {
            replies += 1;
        }
    })
    .await
    .unwrap();
    assert_eq!(replies, 1);
}

async fn has_service(server: &std::sync::Arc<Server>) -> bool {
    let metadata = PluginMetadata {
        name: "second-server-test".to_owned(),
        version: "0".to_owned(),
        authors: Vec::new(),
        description: String::new(),
        dependencies: Vec::new(),
        permissions: Vec::new(),
    };
    startup::create_context(server, metadata)
        .get_service::<TestModService>(SERVICE)
        .await
        .is_some()
}

/// Starts server number `number` on the world of `dir`, checks the mod on it, and stops it.
async fn run_server(dir: &Path, number: usize, damage: i64) {
    let port = free_port();
    let (basic, advanced) = configs(dir, port);
    let telemetry = TelemetryConfig {
        enabled: false,
        ..TelemetryConfig::default()
    };
    let pumpkin = PumpkinServer::new(
        basic,
        advanced,
        telemetry,
        VanillaData::default(),
        Vec::new(),
        false,
    )
    .await
    .unwrap();
    let server = pumpkin.server.clone();

    let channels_offered = server
        .network_registry
        .registrations(ConnectionProtocol::Play)
        .await;
    assert!(
        channels_offered
            .iter()
            .any(|channel| channel.id.to_string() == ECHO_CHANNEL),
        "server {number} does not offer {ECHO_CHANNEL}"
    );
    assert!(
        server
            .permission_manager
            .get_permission(COMMAND_PERMISSION)
            .is_some(),
        "server {number} has no {COMMAND_PERMISSION}"
    );
    assert!(
        has_service(&server).await,
        "server {number} has no {SERVICE}"
    );
    let source = CommandSender::Console.into_source(&server);
    assert_eq!(
        server
            .command_dispatcher
            .load()
            .execute_input(COMMAND, &source)
            .map_err(|err| err.message.get_text()),
        Ok(1)
    );
    assert_eq!(synced_damage(), Some(damage), "server {number}");

    let session = async {
        // The listener binds in `start`.
        wait_until("listener", || {
            std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_ok()
        })
        .await;
        echo(port, &channels(dir)).await;
        assert_eq!(event_count("echo"), number);

        // The poller of this server reloads the edited file, and the World value keeps its value.
        let reloads = event_count(RELOADED);
        set_damage(dir, damage, damage * 2);
        wait_until("synced config reload", || event_count(RELOADED) > reloads).await;
        assert_eq!(synced_damage(), Some(damage), "server {number}");
        server.stop();
    };
    tokio::time::timeout(TIMEOUT, async { tokio::join!(pumpkin.start(), session) })
        .await
        .unwrap();
}

#[test]
fn a_second_server_reuses_the_mods_loaded_by_the_first() {
    let dir = tempfile::tempdir().unwrap();
    // The mod configs and the plugin data folders are in the working directory.
    std::env::set_current_dir(dir.path()).unwrap();
    startup::STARTUP_HOOK
        .set(ironpumpkin_mods::init_mods)
        .unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    // Each server doubles `general.damage` in the file while it runs, and the next server
    // reads the doubled value.
    let mut damage = 10;
    for number in 1..=2 {
        runtime.block_on(run_server(dir.path(), number, damage));
        damage *= 2;
        for event in ONCE {
            assert_eq!(event_count(event), 1, "{event} after server {number}");
        }
        for event in PER_SERVER {
            assert_eq!(event_count(event), number, "{event} after server {number}");
        }
        assert!(!STOP_INTERRUPT.is_cancelled());
    }
}
