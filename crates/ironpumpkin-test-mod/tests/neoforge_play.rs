//! Boots a server with the test mod in a child process, and runs the play script
//! `scripts/boot.toml` of the headless client as a `NeoForge` client: join, one chat message, one
//! block break in creative mode, leave. The steps must receive the packets of
//! `expected/pumpkin-play.txt`, and the server log must show the join, the chat message and the
//! leave. The child reruns this test binary with `SERVER_DIR` set and starts the server from that
//! directory. The test stops the server with the `stop` console command written to its piped
//! stdin, which works on every platform, unlike a signal.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

use std::fs::File;
use std::io::Write;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
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
// A full server boots, a client joins and loads chunks, on a loaded CI runner with parallel tests.
const SESSION_TIMEOUT: Duration = Duration::from_secs(180);
const STOP_TIMEOUT: Duration = Duration::from_mins(1);

/// The server child process.
struct Server {
    child: Child,
    /// The console of the server. Keep the pipe open until the server exits: an EOF ends the
    /// console reader.
    console: ChildStdin,
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
        let log = dir.join("server.log");
        let out = File::create(&log).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([TEST_NAME, "--exact", "--nocapture", "--test-threads=1"])
            .env(SERVER_DIR, dir)
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(out.try_clone().unwrap())
            .stderr(out)
            .spawn()
            .unwrap();
        let console = child.stdin.take().unwrap();
        let mut server = Self {
            child,
            console,
            log,
        };
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

    /// Stops the server with the `stop` console command and waits for it to exit. It kills the
    /// server only when the exit takes longer than `STOP_TIMEOUT`.
    fn stop(&mut self) {
        self.console.write_all(b"stop\n").unwrap();
        self.console.flush().unwrap();
        let deadline = Instant::now() + STOP_TIMEOUT;
        while self.child.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                let _ = self.child.kill();
                panic!(
                    "the server did not stop within {STOP_TIMEOUT:?}:\n{}",
                    self.log()
                );
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

impl Drop for Server {
    /// A server that is still running here belongs to a test that failed before `stop`.
    fn drop(&mut self) {
        if self.child.try_wait().is_ok_and(|status| status.is_none()) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
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
    let mut server = Server::start(dir.path(), port);
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
    server.stop();
}
