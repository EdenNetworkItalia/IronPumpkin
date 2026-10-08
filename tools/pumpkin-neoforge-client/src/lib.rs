//! Headless Minecraft Java client that logs in offline, runs the configuration phase like a
//! vanilla or `NeoForge` client and records the configuration payloads it receives.
#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod channels;
pub mod record;
pub mod session;

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

/// Everything that fails an `assert` run. An empty list means the run passed.
#[must_use]
pub fn check(entries: &[Entry], outcome: &Outcome, expected: &[String]) -> Vec<String> {
    let mut failures = Vec::new();
    if let Outcome::Disconnected(reason) = outcome {
        failures.push(format!(
            "configuration did not finish, disconnected: {reason}"
        ));
    }
    let actual = clientbound_channels(entries);
    if actual != expected {
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

        let failures = check(&entries, &Outcome::Finished, &expected);
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("neoforge:network"));

        let reversed: Vec<String> = expected.iter().rev().cloned().collect();
        let failures = check(
            &entries,
            &Outcome::Disconnected("bye".to_owned()),
            &reversed,
        );
        assert_eq!(failures.len(), 3, "{failures:?}");
        assert!(failures[1].contains("index 0"));
    }
}
