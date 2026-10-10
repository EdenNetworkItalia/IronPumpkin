//! Runs the vanilla configuration phase against a live server and asserts the clientbound payload
//! sequence. It does nothing unless `PUMPKIN_NEOFORGE_CLIENT_SERVER` names the server as
//! `host:port`, so a plain `cargo test` or `cargo nextest run` stays offline.

use std::time::Duration;

use pumpkin_neoforge_client::{Error, check, parse_expected, session};

const SERVER_VAR: &str = "PUMPKIN_NEOFORGE_CLIENT_SERVER";

#[tokio::test]
async fn vanilla_configuration_matches_expected_sequence() -> Result<(), Error> {
    let Ok(server) = std::env::var(SERVER_VAR) else {
        return Ok(());
    };
    let (host, port) = server
        .rsplit_once(':')
        .ok_or_else(|| format!("{SERVER_VAR} must be host:port, got {server:?}"))?;
    let options = session::Options {
        host: host.to_owned(),
        port: port.parse()?,
        username: session::random_username(),
        brand: "vanilla".to_owned(),
        channels: None,
        timeout: Duration::from_secs(60),
    };
    let mut entries = Vec::new();
    let outcome = session::run(&options, &mut |entry| entries.push(entry.clone())).await?;
    let expected = parse_expected(include_str!("../expected/pumpkin-vanilla.txt"));
    let failures = check(&entries, &outcome, Some(&expected), None);
    assert!(failures.is_empty(), "{failures:#?}");
    Ok(())
}
