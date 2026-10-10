//! Headless Minecraft Java client that logs in offline, runs the configuration phase like a
//! vanilla or `NeoForge` client and records the configuration payloads it receives.
#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod channels;
pub mod compare;
pub mod record;
pub mod registries;
pub mod session;

use std::fmt;

use pumpkin_util::text::{TextComponent, TextContent};
use record::{Decode, Direction, Entry};
use session::Outcome;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// Channels of the clientbound custom payloads, in the order they arrived.
#[must_use]
pub fn clientbound_channels(entries: &[Entry]) -> Vec<&str> {
    entries
        .iter()
        .filter(|e| e.direction == Direction::Clientbound)
        .filter_map(|e| e.channel.as_deref())
        .collect()
}

/// Reads an expected channel list: one channel per line, `#` starts a comment.
#[must_use]
pub fn parse_expected(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The translation key of a translate component.
#[must_use]
pub fn translate_key(component: &TextComponent) -> Option<&str> {
    match &*component.0.content {
        TextContent::Translate { translate, .. } => Some(translate),
        _ => None,
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Finished => f.write_str("finished"),
            Self::Disconnected(reason) => write!(f, "disconnected: {reason}"),
            Self::Kicked(reason) => {
                f.write_str("configuration disconnect")?;
                if let TextContent::Translate {
                    translate, with, ..
                } = &*reason.0.content
                {
                    let args: Vec<String> = with
                        .iter()
                        .map(|arg| TextComponent(arg.clone()).get_text())
                        .collect();
                    write!(f, " {translate} {args:?}")?;
                }
                write!(f, ": {:?}", reason.clone().get_text())
            }
        }
    }
}

/// Everything that fails an `assert` run. An empty list means the run passed.
///
/// `expected` is the clientbound channel sequence; `None` skips that check. Without
/// `expect_disconnect` the configuration must finish. With it, the server must end the
/// configuration with a `disconnect` whose reason has that translation key.
#[must_use]
pub fn check(
    entries: &[Entry],
    outcome: &Outcome,
    expected: Option<&[String]>,
    expect_disconnect: Option<&str>,
) -> Vec<String> {
    let mut failures = Vec::new();
    match (outcome, expect_disconnect) {
        (Outcome::Finished, None) => {}
        (Outcome::Kicked(reason), Some(key)) if translate_key(reason) == Some(key) => {}
        (_, None) => failures.push(format!("configuration did not finish: {outcome}")),
        (_, Some(key)) => failures.push(format!(
            "expected a configuration disconnect with {key}, got: {outcome}"
        )),
    }
    let actual = clientbound_channels(entries);
    if let Some(expected) = expected
        && actual != expected
    {
        let at = actual
            .iter()
            .zip(expected)
            .position(|(a, e)| a != e)
            .unwrap_or_else(|| actual.len().min(expected.len()));
        failures.push(format!(
            "channel sequence differs at index {at}: expected {expected:?}, got {actual:?}"
        ));
    }
    for entry in entries.iter().filter(|e| e.decode == Decode::Error) {
        failures.push(format!(
            "payload {} (seq {}) failed to decode: {}",
            entry.channel.as_deref().unwrap_or_default(),
            entry.seq,
            entry.error.as_deref().unwrap_or_default()
        ));
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;
    use record::State;

    fn payload(seq: usize, channel: &str, decode: Decode) -> Entry {
        Entry {
            seq,
            direction: Direction::Clientbound,
            state: State::Configuration,
            packet: "custom_payload".to_owned(),
            channel: Some(channel.to_owned()),
            length: 0,
            reassembled: false,
            decode,
            summary: None,
            error: None,
            data: String::new(),
        }
    }

    #[test]
    fn check_reports_order_decode_errors_and_disconnects() {
        let entries = [
            payload(0, "minecraft:brand", Decode::Ok),
            payload(1, "neoforge:network", Decode::Error),
        ];
        let expected = parse_expected("minecraft:brand # vanilla\n\nneoforge:network\n");
        assert_eq!(expected, ["minecraft:brand", "neoforge:network"]);

        let failures = check(&entries, &Outcome::Finished, Some(&expected), None);
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("neoforge:network"));

        let reversed: Vec<String> = expected.iter().rev().cloned().collect();
        let failures = check(
            &entries,
            &Outcome::Disconnected("bye".to_owned()),
            Some(&reversed),
            None,
        );
        assert_eq!(failures.len(), 3, "{failures:?}");
        assert!(failures[1].contains("index 0"));
    }

    // The configuration disconnect body of a NeoForge 26.3.0.64-beta server for a client with a
    // required channel that the server does not have (run c).
    const INCOMPATIBLE: &str = "0a09000477697468080000000100174e656f466f7267652032362e332e302e36342d626574610800097472616e736c61746500236d756c7469706c617965722e646973636f6e6e6563742e696e636f6d70617469626c6500";
    const INCOMPATIBLE_KEY: &str = "multiplayer.disconnect.incompatible";

    fn incompatible() -> Outcome {
        Outcome::Kicked(
            record::read_disconnect_reason(&hex::decode(INCOMPATIBLE).unwrap()).unwrap(),
        )
    }

    #[test]
    fn reads_neoforge_incompatible_reason() {
        let mut body = hex::decode(INCOMPATIBLE).unwrap();
        let reason = record::read_disconnect_reason(&body).unwrap();
        assert_eq!(translate_key(&reason), Some(INCOMPATIBLE_KEY));
        body.push(0);
        assert!(record::read_disconnect_reason(&body).is_err());
        assert!(record::read_disconnect_reason(&body[..10]).is_err());
        let shown = Outcome::Kicked(reason).to_string();
        assert!(
            shown.contains(r#"multiplayer.disconnect.incompatible ["NeoForge 26.3.0.64-beta"]"#),
            "{shown}"
        );
    }

    #[test]
    fn expect_disconnect_passes_only_on_a_matching_key() {
        assert!(check(&[], &incompatible(), None, Some(INCOMPATIBLE_KEY)).is_empty());
        assert_eq!(check(&[], &incompatible(), None, None).len(), 1);
        for outcome in [
            Outcome::Finished,
            Outcome::Disconnected("bye".to_owned()),
            Outcome::Kicked(TextComponent::text("bye")),
        ] {
            let failures = check(&[], &outcome, None, Some(INCOMPATIBLE_KEY));
            assert_eq!(failures.len(), 1, "{outcome}: {failures:?}");
        }
    }
}
