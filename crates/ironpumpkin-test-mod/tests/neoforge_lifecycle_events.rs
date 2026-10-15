//! Boots a server with the test mod in a child process, stops it with the `stop` console command
//! written to its piped stdin, and checks that the test mod's lifecycle methods, config loading
//! hooks and handlers of the server lifecycle and tick events ran, in `NeoForge` order. The
//! child reruns this test binary with `SERVER_DIR` set and starts the server from that directory.
//! The console command works on every platform, unlike a signal.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

use std::fs::File;
use std::io::Write;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use ironpumpkin_test_mod::EVENT_LOG_PREFIX;

const SERVER_DIR: &str = "SERVER_DIR";
const TEST_NAME: &str = "neoforge_lifecycle_events";
const BOOT_TIMEOUT: Duration = Duration::from_mins(3);
const STOP_TIMEOUT: Duration = Duration::from_mins(1);

/// The events in the order `NeoForge` 26.3 fires them: the local configs load before the common
/// setup, the synced configs after the load complete and before `ServerAboutToStartEvent`,
/// `ServerStartedEvent` before the first tick, and the level tick inside the server tick.
const EVENTS: [&str; 15] = [
    "FMLConstructModEvent",
    "ModConfigEvent.Loading STARTUP",
    "ModConfigEvent.Loading LOCAL",
    "FMLCommonSetupEvent",
    "FMLLoadCompleteEvent",
    "ModConfigEvent.Loading SYNCED",
    "ServerAboutToStartEvent",
    "ServerStartingEvent",
    "ServerStartedEvent",
    "ServerTickEventPre",
    "LevelTickEventPre",
    "LevelTickEventPost",
    "ServerTickEventPost",
    "ServerStoppingEvent",
    "ServerStoppedEvent",
];

fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn start(dir: &Path, port: u16) -> Child {
    std::fs::write(
        dir.join("pumpkin.toml"),
        format!(
            r#"[logging]
color = false

[commands]
use_console = true
use_tty = false

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
    let out = File::create(dir.join("server.log")).unwrap();
    Command::new(std::env::current_exe().unwrap())
        .args([TEST_NAME, "--exact", "--nocapture", "--test-threads=1"])
        .env(SERVER_DIR, dir)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(out.try_clone().unwrap())
        .stderr(out)
        .spawn()
        .unwrap()
}

fn log(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("server.log")).unwrap_or_default()
}

/// Waits until the server listens on `port` and a tick ran.
fn wait_for_boot(dir: &Path, child: &mut Child, port: u16) {
    let start = Instant::now();
    loop {
        let log = log(dir);
        if log.contains(&format!("{EVENT_LOG_PREFIX}ServerTickEventPost"))
            && TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_ok()
        {
            return;
        }
        assert!(
            start.elapsed() < BOOT_TIMEOUT,
            "the server did not boot within {BOOT_TIMEOUT:?}:\n{log}"
        );
        if let Some(status) = child.try_wait().unwrap() {
            panic!("the server exited with {status}:\n{log}");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Where the log has the line of the handler of `event`.
fn position(log: &str, event: &str) -> usize {
    log.find(&format!("{EVENT_LOG_PREFIX}{event}\n"))
        .unwrap_or_else(|| panic!("no handler of {event} ran:\n{log}"))
}

#[test]
fn neoforge_lifecycle_events() {
    if std::env::var_os(SERVER_DIR).is_some() {
        // The server reads pumpkin.toml from the working directory and exits the process.
        pumpkin::run();
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let mut child = start(dir.path(), port);
    wait_for_boot(dir.path(), &mut child, port);

    // Keep the pipe open until the server exits: an EOF ends the console reader.
    let mut console = child.stdin.take().unwrap();
    console.write_all(b"stop\n").unwrap();
    console.flush().unwrap();
    let deadline = Instant::now() + STOP_TIMEOUT;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!(
                "the server did not stop within {STOP_TIMEOUT:?}:\n{}",
                log(dir.path())
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    let log = log(dir.path());
    let positions = EVENTS.map(|event| position(&log, event));
    assert!(positions.is_sorted(), "{EVENTS:?} at {positions:?}:\n{log}");
}
