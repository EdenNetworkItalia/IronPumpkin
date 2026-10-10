//! The registry expectation check: the expected contents of the `neoforge:frozen_registry`
//! payloads of a recording.

use std::collections::BTreeMap;
use std::path::Path;

use pumpkin_protocol::java::neoforge::{FrozenRegistryPayload, decode_payload};
use pumpkin_util::identifier::Identifier;
use serde::Deserialize;

use crate::Error;
use crate::record::{Direction, Entry};

/// The expected `neoforge:frozen_registry` payloads, in the order the server sends them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryExpectations(pub Vec<RegistryExpectation>);

/// The expected snapshot of one registry: ids `0..ids` without gaps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryExpectation {
    pub name: String,
    pub ids: usize,
    /// The name at id 0.
    pub first: String,
    /// The name at id `ids - 1`.
    pub last: String,
    /// More names at given ids.
    pub at: BTreeMap<i32, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    registry: Vec<Registry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    name: String,
    ids: usize,
    first: String,
    last: String,
    // TOML keys are strings, so the ids are parsed after the deserialization.
    #[serde(default)]
    at: BTreeMap<String, String>,
}

impl RegistryExpectations {
    /// Reads a TOML registry expectation file.
    pub fn load(path: &Path) -> Result<Self, Error> {
        Self::parse(&std::fs::read_to_string(path)?)
            .map_err(|e| format!("{}: {e}", path.display()).into())
    }

    pub fn parse(text: &str) -> Result<Self, Error> {
        let file: File = toml::from_str(text)?;
        let registries = file
            .registry
            .into_iter()
            .map(|registry| {
                let at = registry
                    .at
                    .into_iter()
                    .map(|(id, name)| {
                        let id = id.parse().map_err(|_| {
                            format!("registry {}: the id {id:?} is not a number", registry.name)
                        })?;
                        Ok((id, name))
                    })
                    .collect::<Result<_, Error>>()?;
                Ok(RegistryExpectation {
                    name: registry.name,
                    ids: registry.ids,
                    first: registry.first,
                    last: registry.last,
                    at,
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self(registries))
    }
}

impl RegistryExpectation {
    fn check(&self, ids: &BTreeMap<i32, Identifier>, failures: &mut Vec<String>) {
        let name = &self.name;
        if ids.len() != self.ids {
            failures.push(format!(
                "registry {name}: expected {} ids, got {}",
                self.ids,
                ids.len()
            ));
        }
        if !ids
            .keys()
            .copied()
            .eq(0..ids.len().try_into().unwrap_or(i32::MAX))
        {
            failures.push(format!("registry {name}: the ids are not 0..{}", ids.len()));
        }
        let last = i32::try_from(self.ids).unwrap_or(i32::MAX) - 1;
        let named = [(0, &self.first), (last, &self.last)]
            .into_iter()
            .chain(self.at.iter().map(|(id, expected)| (*id, expected)));
        for (id, expected) in named {
            let actual = ids.get(&id).map(ToString::to_string);
            if actual.as_deref() != Some(expected.as_str()) {
                failures.push(format!(
                    "registry {name}: expected {expected} at id {id}, got {}",
                    actual.as_deref().unwrap_or("no entry")
                ));
            }
        }
    }
}

/// Decodes the body of a recorded `neoforge:frozen_registry` payload.
pub fn decode_frozen_registry(entry: &Entry) -> Result<FrozenRegistryPayload, Error> {
    let data = hex::decode(&entry.data)?;
    decode_payload(&data, entry.reassembled, FrozenRegistryPayload::read)
        .map_err(|e| format!("frozen_registry payload (seq {}): {e}", entry.seq).into())
}

/// Decodes the clientbound `neoforge:frozen_registry` payloads of a recording.
pub fn frozen_registries(entries: &[Entry]) -> Result<Vec<FrozenRegistryPayload>, Error> {
    entries
        .iter()
        .filter(|e| {
            e.direction == Direction::Clientbound
                && e.channel.as_deref() == Some(FrozenRegistryPayload::CHANNEL)
        })
        .map(decode_frozen_registry)
        .collect()
}

/// Everything that differs between the `neoforge:frozen_registry` payloads of a recording and
/// the expectations. An empty list means the check passed.
///
/// It compares the registry names in order, then the snapshot of each expected registry that
/// arrived.
#[must_use]
pub fn check_registries(entries: &[Entry], expected: &RegistryExpectations) -> Vec<String> {
    let payloads = match frozen_registries(entries) {
        Ok(payloads) => payloads,
        Err(e) => return vec![e.to_string()],
    };
    let mut failures = Vec::new();
    let actual: Vec<String> = payloads
        .iter()
        .map(|p| p.registry_name.to_string())
        .collect();
    let wanted: Vec<&str> = expected.0.iter().map(|r| r.name.as_str()).collect();
    if actual != wanted {
        let unexpected: Vec<&str> = actual
            .iter()
            .map(String::as_str)
            .filter(|name| !wanted.contains(name))
            .collect();
        let missing: Vec<&str> = wanted
            .iter()
            .copied()
            .filter(|name| !actual.iter().any(|a| a == name))
            .collect();
        failures.push(format!(
            "frozen_registry payloads differ: expected {} registries {wanted:?}, got {} \
             {actual:?}; unexpected {unexpected:?}, missing {missing:?}",
            wanted.len(),
            actual.len()
        ));
    }
    for expectation in &expected.0 {
        if let Some(payload) = payloads
            .iter()
            .find(|p| p.registry_name.to_string() == expectation.name)
        {
            expectation.check(&payload.snapshot.ids, &mut failures);
        }
    }
    failures
}
