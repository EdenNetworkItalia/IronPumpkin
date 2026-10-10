//! Boots a server with the test mod in a child process and runs the headless client against it.
//! A `NeoForge` client gets the registry sync with the test mod's content at the first free ids,
//! and a vanilla client gets no sync. The child reruns this test binary with `SERVER_DIR` set and
//! starts the server from that directory.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

use std::fs::File;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use ironpumpkin_test_mod::{BLOCK, ENTITY_TYPE, ITEM};
use pumpkin_neoforge_client::{
    channels::ChannelMap,
    check, parse_expected,
    record::{Direction, Entry},
    session::{self, Outcome},
};
use pumpkin_protocol::java::neoforge::{
    FrozenRegistryPayload, FrozenRegistrySyncCompletedPayload, FrozenRegistrySyncStartPayload,
    decode_exact,
};

const SERVER_DIR: &str = "SERVER_DIR";
const TEST_NAME: &str = "neoforge_client_syncs_custom_content";
const NEOFORGE_PLAYER: &str = "SyncProbe";
const VANILLA_PLAYER: &str = "VanillaProbe";
const BOOT_TIMEOUT: Duration = Duration::from_mins(3);
const SESSION_TIMEOUT: Duration = Duration::from_secs(60);

/// The server child process. Dropping it kills the server.
struct Server {
    child: Child,
    log: PathBuf,
}

impl Server {
    fn start(dir: &Path, port: u16) -> Self {
        std::fs::write(
            dir.join("pumpkin.toml"),
            format!(
                r#"detect_neoforge_clients = true

[logging]
color = false

[commands]
use_console = false

[networking.java]
address = "127.0.0.1:{port}"
online_mode = false

[networking.bedrock]
enabled = false

[networking.query]
enabled = false

[networking.lan_broadcast]
enabled = false

[telemetry]
enabled = false
"#
            ),
        )
        .unwrap();
        let log = dir.join("server.log");
        let out = File::create(&log).unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args([TEST_NAME, "--exact", "--nocapture", "--test-threads=1"])
            .env(SERVER_DIR, dir)
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(out.try_clone().unwrap())
            .stderr(out)
            .spawn()
            .unwrap();
        let mut server = Self { child, log };
        server.wait_for_port(port);
        server
    }

    fn log(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn wait_for_port(&mut self, port: u16) {
        let start = Instant::now();
        while TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err() {
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!("the server exited with {status}:\n{}", self.log());
            }
            assert!(
                start.elapsed() < BOOT_TIMEOUT,
                "the server did not listen within {BOOT_TIMEOUT:?}:\n{}",
                self.log()
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Waits until the log has `line`, which the server writes after the client is gone.
    fn wait_for_log(&self, line: &str) {
        let start = Instant::now();
        while !self.log().contains(line) {
            assert!(
                start.elapsed() < SESSION_TIMEOUT,
                "the server log has no {line:?}:\n{}",
                self.log()
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Runs the configuration phase as `username` and returns the recording, like the headless
/// client's `--out`.
fn record(port: u16, username: &str, channels: Option<ChannelMap>) -> (Vec<Entry>, Outcome) {
    let brand = if channels.is_some() {
        "neoforge"
    } else {
        "vanilla"
    };
    let options = session::Options {
        host: Ipv4Addr::LOCALHOST.to_string(),
        port,
        username: username.to_owned(),
        brand: brand.to_owned(),
        channels,
        timeout: SESSION_TIMEOUT,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut entries = Vec::new();
    let outcome = runtime
        .block_on(session::run(&options, &mut |entry| {
            entries.push(entry.clone());
        }))
        .unwrap();
    (entries, outcome)
}

fn position(entries: &[Entry], direction: Direction, packet: &str, channel: Option<&str>) -> usize {
    entries
        .iter()
        .position(|entry| {
            entry.direction == direction
                && entry.packet == packet
                && entry.channel.as_deref() == channel
        })
        .unwrap_or_else(|| panic!("no {direction:?} {packet} {channel:?} in the recording"))
}

fn body(entry: &Entry) -> Vec<u8> {
    hex::decode(&entry.data).unwrap()
}

/// The id count, then the names at `ids`, of a snapshot.
fn names_at(registry: &FrozenRegistryPayload, ids: [i32; 3]) -> (usize, [String; 3]) {
    let snapshot = &registry.snapshot.ids;
    let name = |id| {
        snapshot
            .get(&id)
            .map(ToString::to_string)
            .unwrap_or_default()
    };
    (snapshot.len(), ids.map(name))
}

fn check_neoforge_client(server: &Server, port: u16) {
    let channels = ChannelMap::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/pumpkin-neoforge-client/channels/neoforge-26.3.toml"),
    )
    .unwrap();
    let (entries, outcome) = record(port, NEOFORGE_PLAYER, Some(channels));
    let expected = parse_expected(include_str!(
        "../../../tools/pumpkin-neoforge-client/expected/pumpkin-neoforge.txt"
    ));
    let failures = check(&entries, &outcome, Some(&expected), None);
    assert!(failures.is_empty(), "{failures:#?}");

    let payloads = |channel| {
        entries.iter().filter(move |entry| {
            entry.direction == Direction::Clientbound && entry.channel.as_deref() == Some(channel)
        })
    };
    let start: Vec<FrozenRegistrySyncStartPayload> =
        payloads(FrozenRegistrySyncStartPayload::CHANNEL)
            .map(|entry| decode_exact(&body(entry), FrozenRegistrySyncStartPayload::read).unwrap())
            .collect();
    let registries: Vec<FrozenRegistryPayload> = payloads(FrozenRegistryPayload::CHANNEL)
        .map(|entry| decode_exact(&body(entry), FrozenRegistryPayload::read).unwrap())
        .collect();
    let names: Vec<String> = registries
        .iter()
        .map(|registry| registry.registry_name.to_string())
        .collect();
    assert_eq!(
        names,
        ["minecraft:block", "minecraft:item", "minecraft:entity_type"]
    );
    assert_eq!(start.len(), 1);
    let listed: Vec<String> = start[0].to_access.iter().map(ToString::to_string).collect();
    assert_eq!(listed, names);
    assert!(
        registries
            .iter()
            .all(|registry| registry.snapshot.aliases.is_empty())
    );

    // The generated entries keep their ids and the custom entry takes the first free id.
    assert_eq!(
        names_at(&registries[0], [0, 1285, 1286]),
        (
            1287,
            [
                "minecraft:air".to_owned(),
                "minecraft:firefly_bush".to_owned(),
                BLOCK.to_owned()
            ]
        )
    );
    assert_eq!(
        names_at(&registries[1], [0, 1657, 1658]),
        (
            1659,
            [
                "minecraft:air".to_owned(),
                "minecraft:ominous_bottle".to_owned(),
                ITEM.to_owned()
            ]
        )
    );
    assert_eq!(
        names_at(&registries[2], [0, 160, 161]),
        (
            162,
            [
                "minecraft:acacia_boat".to_owned(),
                "minecraft:fishing_bobber".to_owned(),
                ENTITY_TYPE.to_owned()
            ]
        )
    );

    // The known packs wait for the client's echo of the completed payload.
    let completed = Some(FrozenRegistrySyncCompletedPayload::CHANNEL);
    let sent = position(
        &entries,
        Direction::Clientbound,
        "custom_payload",
        completed,
    );
    let echo = position(
        &entries,
        Direction::Serverbound,
        "custom_payload",
        completed,
    );
    let known_packs = position(&entries, Direction::Clientbound, "select_known_packs", None);
    assert!(
        sent < echo && echo < known_packs,
        "{sent} {echo} {known_packs}"
    );

    server.wait_for_log(&format!(
        "{NEOFORGE_PLAYER} entered play as a NeoForge client"
    ));
}

fn check_vanilla_client(port: u16) {
    let (entries, outcome) = record(port, VANILLA_PLAYER, None);
    let synced: Vec<&str> = entries
        .iter()
        .filter_map(|entry| entry.channel.as_deref())
        .filter(|channel| channel.starts_with("neoforge:frozen_registry"))
        .collect();
    assert!(synced.is_empty(), "{outcome}: {synced:?}");
}

#[test]
fn neoforge_client_syncs_custom_content() {
    if std::env::var_os(SERVER_DIR).is_some() {
        // The server reads pumpkin.toml from the working directory and exits the process.
        pumpkin::run();
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let server = Server::start(dir.path(), port);
    check_neoforge_client(&server, port);
    check_vanilla_client(port);
}
