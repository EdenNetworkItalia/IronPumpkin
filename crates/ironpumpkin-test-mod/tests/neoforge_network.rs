//! Boots a server with the test mod in a child process and connects the headless client as a
//! `NeoForge` client that has the channels of the test mod (`channels/test-mod.toml` of the
//! client) and sends back each payload on `test-mod:licence` and `test-mod:echo`:
//!
//! - The licence task and the greeting task run after the tasks of `NeoForge`, in registration
//!   order, and the configuration ends only after the client answered the licence challenge
//!   (scenarios "Licence handshake", "Two mod tasks" and "Reply to a mod task" of
//!   `neoforge-network`).
//! - A player whose name starts with `Stray` gets a task that sends the challenge and never
//!   finishes, so the answer arrives while no licence task waits, and the server disconnects
//!   the client (scenario "Unexpected reply").
//! - In play, the mod sends a payload on `test-mod:echo` to the joining player, the client sends
//!   it back, and the handler of the mod logs the player and replies on `test-mod:echo_reply`
//!   (scenarios "Handler runs" and "Reply").
//!
//! The child reruns this test binary with `SERVER_DIR` set and starts the server from that
//! directory. The test stops the server with the `stop` console command.
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

use ironpumpkin_mods::{pumpkin_data::translation, text::TextComponent};
use ironpumpkin_test_mod::{
    ECHO_CHANNEL, ECHO_LOG, ECHO_PING, ECHO_REPLY_CHANNEL, GREETING, GREETING_CHANNEL,
    LICENCE_CHALLENGE, LICENCE_CHANNEL, STRAY_PLAYER_PREFIX,
};
use pumpkin_neoforge_client::{
    channels::ChannelMap,
    record::{Direction, Entry, State},
    script::Script,
    session::{self, Options, Outcome},
};

const SERVER_DIR: &str = "SERVER_DIR";
const TEST_NAME: &str = "neoforge_network_with_the_headless_client";
const BOOT_TIMEOUT: Duration = Duration::from_mins(3);
// A full server boots, a client joins and loads chunks, on a loaded CI runner with parallel tests.
const SESSION_TIMEOUT: Duration = Duration::from_secs(180);
const STOP_TIMEOUT: Duration = Duration::from_mins(1);

struct Server {
    child: Child,
    /// Keep the pipe open until the server exits: an EOF ends the console reader.
    console: ChildStdin,
    log: std::path::PathBuf,
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

/// The channel map of a `NeoForge` 26.3 client with the test mod.
fn channels(dir: &Path) -> ChannelMap {
    let client = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/pumpkin-neoforge-client");
    let mut text = std::fs::read_to_string(client.join("channels/neoforge-26.3.toml")).unwrap();
    text.push_str(&std::fs::read_to_string(client.join("channels/test-mod.toml")).unwrap());
    let path = dir.join("channels.toml");
    std::fs::write(&path, text).unwrap();
    ChannelMap::load(&path).unwrap()
}

fn options(port: u16, username: &str, channels: &ChannelMap) -> Options {
    Options {
        host: Ipv4Addr::LOCALHOST.to_string(),
        port,
        username: username.to_owned(),
        brand: "neoforge".to_owned(),
        channels: Some(channels.clone()),
        timeout: SESSION_TIMEOUT,
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Where the recording has the custom payload `channel` in `direction`, with its body.
fn payload(
    entries: &[Entry],
    direction: Direction,
    state: State,
    channel: &str,
) -> (usize, Vec<u8>) {
    let index = entries
        .iter()
        .position(|entry| {
            entry.direction == direction
                && entry.state == state
                && entry.channel.as_deref() == Some(channel)
        })
        .unwrap_or_else(|| panic!("no {direction:?} {channel} in the recording:\n{entries:#?}"));
    (index, hex::decode(&entries[index].data).unwrap())
}

fn packet(entries: &[Entry], direction: Direction, name: &str) -> usize {
    entries
        .iter()
        .position(|entry| entry.direction == direction && entry.packet == name)
        .unwrap_or_else(|| panic!("no {direction:?} {name} in the recording"))
}

fn check_licence(port: u16, channels: &ChannelMap, server: &Server) {
    let mut entries = Vec::new();
    let outcome = runtime()
        .block_on(session::run(
            &options(port, "LicenceProbe", channels),
            &mut |entry| entries.push(entry.clone()),
        ))
        .unwrap();
    assert_eq!(outcome, Outcome::Finished, "{entries:#?}");
    let configuration = State::Configuration;
    let (challenge, body) = payload(
        &entries,
        Direction::Clientbound,
        configuration,
        LICENCE_CHANNEL,
    );
    assert_eq!(body, LICENCE_CHALLENGE);
    let (answer, body) = payload(
        &entries,
        Direction::Serverbound,
        configuration,
        LICENCE_CHANNEL,
    );
    assert_eq!(body, LICENCE_CHALLENGE);
    let (greeting, body) = payload(
        &entries,
        Direction::Clientbound,
        configuration,
        GREETING_CHANNEL,
    );
    assert_eq!(body, GREETING);
    let (feature_flags, _) = payload(
        &entries,
        Direction::Clientbound,
        configuration,
        "neoforge:feature_flags",
    );
    let finish = packet(&entries, Direction::Clientbound, "finish_configuration");
    // The mod tasks follow the NeoForge tasks, the greeting task starts only after the answer
    // finished the licence task, and the configuration ends after both.
    let order = [feature_flags, challenge, answer, greeting, finish];
    assert!(order.is_sorted(), "{order:?}:\n{entries:#?}");
    server.wait_for_log("test-mod licence answered");
}

fn check_stray(port: u16, channels: &ChannelMap) {
    let name = format!("{STRAY_PLAYER_PREFIX}Probe");
    let mut entries = Vec::new();
    let outcome = runtime()
        .block_on(session::run(
            &options(port, &name, channels),
            &mut |entry| entries.push(entry.clone()),
        ))
        .unwrap();
    let Outcome::Kicked(reason) = outcome else {
        panic!("the stray answer did not kick: {outcome:?}\n{entries:#?}");
    };
    // The exception of `finishCurrentTask`, as `Connection.exceptionCaught` reports it.
    #[expect(
        deprecated,
        reason = "the server builds the reason with `TextComponent::translate`, not the cross macro"
    )]
    let expected = TextComponent::translate(
        translation::java::DISCONNECT_GENERICREASON,
        [TextComponent::text(
            "Internal Exception: java.lang.IllegalStateException: Unexpected request for task \
             finish, current task: test-mod:stray, requested: test-mod:licence",
        )],
    );
    assert_eq!(reason, expected);
    assert!(
        !entries
            .iter()
            .any(|entry| entry.packet == "finish_configuration")
    );
}

fn check_echo(port: u16, channels: &ChannelMap, server: &Server) {
    // The client leaves once the reply arrived, or fails the run after `SESSION_TIMEOUT`.
    let steps = format!("steps = [\"join\", \"await_payload {ECHO_REPLY_CHANNEL}\", \"leave\"]");
    let script = Script::parse(&steps, false).unwrap();
    let mut entries = Vec::new();
    runtime()
        .block_on(session::play(
            &options(port, "EchoProbe", channels),
            &script,
            &mut |entry| entries.push(entry.clone()),
        ))
        .unwrap();
    let (ping, body) = payload(&entries, Direction::Clientbound, State::Play, ECHO_CHANNEL);
    assert_eq!(body, ECHO_PING);
    let (reply, body) = payload(
        &entries,
        Direction::Clientbound,
        State::Play,
        ECHO_REPLY_CHANNEL,
    );
    assert_eq!(body, ECHO_PING);
    assert!(ping < reply);
    server.wait_for_log(&format!("{ECHO_LOG}EchoProbe"));
}

#[test]
fn neoforge_network_with_the_headless_client() {
    if std::env::var_os(SERVER_DIR).is_some() {
        // The server reads pumpkin.toml from the working directory and exits the process.
        pumpkin::run();
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let mut server = Server::start(dir.path(), port);
    let channels = channels(dir.path());
    check_licence(port, &channels, &server);
    check_stray(port, &channels);
    check_echo(port, &channels, &server);
    server.stop();
}
