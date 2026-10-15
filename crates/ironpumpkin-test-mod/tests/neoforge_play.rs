//! Boots a server with the test mod in a child process, and runs the play script
//! `scripts/boot.toml` of the headless client as a `NeoForge` client: join, one chat message, one
//! block break in creative mode, leave. The steps must receive the packets of
//! `expected/pumpkin-play.txt`, and the server log must show the join, the chat message and the
//! leave. The child reruns this test binary with `SERVER_DIR` set and starts the server from that
//! directory.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

use std::fs::File;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use ironpumpkin_test_mod::ID;
use pumpkin_neoforge_client::{
    channels::ChannelMap,
    check_play, parse_play_expected,
    script::Script,
    session::{self, Options},
};

const SERVER_DIR: &str = "SERVER_DIR";
const TEST_NAME: &str = "neoforge_client_plays_a_script";
const PLAYER: &str = "PlayProbe";
const BOOT_TIMEOUT: Duration = Duration::from_mins(3);
const SESSION_TIMEOUT: Duration = Duration::from_secs(60);

/// The server child process. Dropping it kills the server.
struct Server {
    child: Child,
    log: std::path::PathBuf,
}

impl Server {
    fn start(dir: &Path, port: u16) -> Self {
        std::fs::write(
            dir.join("pumpkin.toml"),
            format!(
                r#"seed = "parity"
default_gamemode = "Creative"
spawn_protection = 0

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

    /// Waits until the log has `line`.
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

fn client_path(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/pumpkin-neoforge-client")
        .join(path)
}

#[test]
fn neoforge_client_plays_a_script() {
    if std::env::var_os(SERVER_DIR).is_some() {
        // The server reads pumpkin.toml from the working directory and exits the process.
        pumpkin::run();
        return;
    }

    let script = Script::load(&client_path("scripts/boot.toml")).unwrap();
    let expected = parse_play_expected(include_str!(
        "../../../tools/pumpkin-neoforge-client/expected/pumpkin-play.txt"
    ))
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let server = Server::start(dir.path(), port);
    // The test mod is client-required, so only a NeoForge client gets into play.
    server.wait_for_log(&format!("{ID} (client_required = true)"));

    let options = Options {
        host: Ipv4Addr::LOCALHOST.to_string(),
        port,
        username: PLAYER.to_owned(),
        brand: "neoforge".to_owned(),
        channels: Some(ChannelMap::load(&client_path("channels/neoforge-26.3.toml")).unwrap()),
        timeout: SESSION_TIMEOUT,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut entries = Vec::new();
    let report = runtime
        .block_on(session::play(&options, &script, &mut |entry| {
            entries.push(entry.clone());
        }))
        .unwrap();
    let failures = check_play(&entries, &report, &expected);
    assert!(failures.is_empty(), "{failures:#?}");

    server.wait_for_log(&format!("{PLAYER} joined the game"));
    server.wait_for_log(&format!("<chat> {PLAYER}: hello from the boot test"));
    server.wait_for_log(&format!("{PLAYER} left the game"));
}
