//! Boots a server with the test mod in a child process and runs the headless client against it.
//! A `NeoForge` client gets the registry sync with the test mod's content at the first free ids,
//! then the modded configuration tasks with the test mod's synced config. A vanilla client gets
//! neither: the client-required test mod gets it kicked. The child reruns this test binary with
//! `SERVER_DIR` set and starts the server from that directory.
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

use ironpumpkin_test_mod::{BLOCK, ENTITY_TYPE, ITEM, SYNCED_CONFIG, SYNCED_CONFIG_CONTENTS};
use pumpkin_neoforge_client::{
    channels::ChannelMap,
    check, parse_expected,
    record::{Direction, Entry},
    registries::{RegistryExpectations, check_registries},
    session::{self, Outcome},
};
use pumpkin_protocol::java::neoforge::{
    CommonRegisterPayload, CommonVersionPayload, ConfigFilePayload, ExtensibleEnumDataPayload,
    FeatureFlagDataPayload, FrozenRegistryPayload, FrozenRegistrySyncCompletedPayload,
    FrozenRegistrySyncStartPayload, KnownRegistryDataMapsPayload, decode_exact,
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

fn check_neoforge_client(server: &Server, port: u16) {
    let channels = ChannelMap::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/pumpkin-neoforge-client/channels/neoforge-26.3.toml"),
    )
    .unwrap();
    let (entries, outcome) = record(port, NEOFORGE_PLAYER, Some(channels));
    let expected = parse_expected(include_str!(
        "../../../tools/pumpkin-neoforge-client/expected/pumpkin-neoforge-test-mod.txt"
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
    let expectations = RegistryExpectations::parse(include_str!(
        "../../../tools/pumpkin-neoforge-client/expected/pumpkin-neoforge-test-mod-registries.toml"
    ))
    .unwrap();
    let custom: Vec<&str> = expectations.0.iter().map(|r| r.last.as_str()).collect();
    assert_eq!(custom, [BLOCK, ITEM, ENTITY_TYPE]);
    let failures = check_registries(&entries, &expectations);
    assert!(failures.is_empty(), "{failures:#?}");

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

    check_modded_tasks(&entries);

    server.wait_for_log(&format!(
        "{NEOFORGE_PLAYER} entered play as a NeoForge client"
    ));
}

/// The clientbound channels of the modded configuration tasks, in task order.
const MODDED_TASK_CHANNELS: [&str; 6] = [
    CommonVersionPayload::CHANNEL,
    CommonRegisterPayload::CHANNEL,
    ConfigFilePayload::CHANNEL,
    KnownRegistryDataMapsPayload::CHANNEL,
    ExtensibleEnumDataPayload::CHANNEL,
    FeatureFlagDataPayload::CHANNEL,
];

/// The modded tasks run between `update_tags` and `finish_configuration`, the bodies of
/// `c:register` and the first `neoforge:config_file` equal run (b), and the test mod's config
/// follows.
fn check_modded_tasks(entries: &[Entry]) {
    let tags = position(entries, Direction::Clientbound, "update_tags", None);
    let finish = position(
        entries,
        Direction::Clientbound,
        "finish_configuration",
        None,
    );
    let positions = MODDED_TASK_CHANNELS.map(|channel| {
        position(
            entries,
            Direction::Clientbound,
            "custom_payload",
            Some(channel),
        )
    });
    assert!(
        tags < positions[0] && positions.is_sorted() && positions[5] < finish,
        "{tags} {positions:?} {finish}"
    );

    // Run (b), seq 88: version 1, protocol play, the channel neoforge:split.
    let register = &entries[positions[1]];
    assert_eq!(
        register.data,
        "0104706c6179010e6e656f666f7267653a73706c6974"
    );
    let configs: Vec<ConfigFilePayload> = entries
        .iter()
        .filter(|entry| {
            entry.direction == Direction::Clientbound
                && entry.channel.as_deref() == Some(ConfigFilePayload::CHANNEL)
        })
        .map(|entry| decode_exact(&body(entry), ConfigFilePayload::read).unwrap())
        .collect();
    assert_eq!(configs.len(), 2);
    assert_eq!(configs[0].file_name, "neoforge-synced.toml");
    assert_eq!(
        &*configs[0].contents,
        include_bytes!("../../../assets/neoforge/neoforge-synced.toml")
    );
    assert_eq!(configs[1].file_name, SYNCED_CONFIG);
    assert_eq!(&*configs[1].contents, SYNCED_CONFIG_CONTENTS);
}

/// `NetworkRegistry.initializeOtherConnection` refuses a vanilla client while a client-required
/// mod is loaded.
fn check_vanilla_client(port: u16) {
    let (entries, outcome) = record(port, VANILLA_PLAYER, None);
    let failures = check(
        &entries,
        &outcome,
        None,
        Some("neoforge.network.negotiation.failure.vanilla.client.not_supported"),
    );
    assert!(failures.is_empty(), "{failures:#?}");
    let modded: Vec<&str> = entries
        .iter()
        .filter_map(|entry| entry.channel.as_deref())
        .filter(|channel| {
            channel.starts_with("neoforge:frozen_registry")
                || MODDED_TASK_CHANNELS.contains(channel)
        })
        .collect();
    assert!(modded.is_empty(), "{outcome}: {modded:?}");
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
