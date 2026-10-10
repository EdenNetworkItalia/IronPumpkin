//! Runs the configuration phase against a live `IronPumpkin` server without native mods, as a
//! vanilla and as a `NeoForge` client, and asserts the clientbound payload sequences. It does
//! nothing unless `PUMPKIN_NEOFORGE_CLIENT_SERVER` names the server as `host:port`, so a plain
//! `cargo test` or `cargo nextest run` stays offline. The `NeoForge` case needs the server to run
//! with `detect_neoforge_clients = true`.

use std::path::Path;
use std::time::Duration;

use pumpkin_neoforge_client::{
    Error,
    channels::ChannelMap,
    check,
    compare::compare_with_reference,
    parse_expected,
    record::{Entry, read_jsonl},
    registries::{RegistryExpectations, check_registries},
    session::{self, Outcome},
};

const SERVER_VAR: &str = "PUMPKIN_NEOFORGE_CLIENT_SERVER";

/// What a vanilla client receives before the brand when `detect_neoforge_clients` is on: the
/// `NeoForge` probe, then the `minecraft:register` of `initializeOtherConnection`, as in run (a)
/// of the capture.
const DETECTION: [&str; 4] = [
    "minecraft:unregister",
    "minecraft:register",
    "neoforge:register",
    "minecraft:register",
];

async fn record(channels: Option<ChannelMap>) -> Result<Option<(Vec<Entry>, Outcome)>, Error> {
    let Ok(server) = std::env::var(SERVER_VAR) else {
        return Ok(None);
    };
    let (host, port) = server
        .rsplit_once(':')
        .ok_or_else(|| format!("{SERVER_VAR} must be host:port, got {server:?}"))?;
    let brand = if channels.is_some() {
        "neoforge"
    } else {
        "vanilla"
    };
    let options = session::Options {
        host: host.to_owned(),
        port: port.parse()?,
        username: session::random_username(),
        brand: brand.to_owned(),
        channels,
        timeout: Duration::from_secs(60),
    };
    let mut entries = Vec::new();
    let outcome = session::run(&options, &mut |entry| entries.push(entry.clone())).await?;
    Ok(Some((entries, outcome)))
}

fn manifest_path(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

#[tokio::test]
async fn vanilla_configuration_matches_expected_sequence() -> Result<(), Error> {
    let Some((entries, outcome)) = record(None).await? else {
        return Ok(());
    };
    let mut expected = parse_expected(include_str!("../expected/pumpkin-vanilla.txt"));
    if entries
        .iter()
        .any(|e| e.channel.as_deref() == Some(DETECTION[2]))
    {
        expected.splice(0..0, DETECTION.map(str::to_owned));
    }
    let failures = check(&entries, &outcome, Some(&expected), None);
    assert!(failures.is_empty(), "{failures:#?}");
    Ok(())
}

/// The sequence of `expected/pumpkin-neoforge.txt`, the registries of
/// `expected/pumpkin-neoforge-registries.toml`, and the channel order and modded task bodies of
/// run (b) of the `NeoForge` 26.3.0.64-beta capture.
#[tokio::test]
async fn neoforge_configuration_matches_expected_sequence_and_capture() -> Result<(), Error> {
    let channels = ChannelMap::load(&manifest_path("channels/neoforge-26.3.toml"))?;
    let Some((entries, outcome)) = record(Some(channels)).await? else {
        return Ok(());
    };
    let expected = parse_expected(include_str!("../expected/pumpkin-neoforge.txt"));
    let mut failures = check(&entries, &outcome, Some(&expected), None);
    let registries =
        RegistryExpectations::parse(include_str!("../expected/pumpkin-neoforge-registries.toml"))?;
    failures.extend(check_registries(&entries, &registries));
    let reference = read_jsonl(&manifest_path(
        "captures/neoforge-26.3.0.64-beta/run-b-neoforge.jsonl.gz",
    ))?;
    failures.extend(compare_with_reference(&entries, &reference));
    assert!(failures.is_empty(), "{failures:#?}");
    Ok(())
}
