//! Comparison of an `IronPumpkin` recording with a capture of a real `NeoForge` server.

use pumpkin_protocol::java::neoforge::{
    CommonRegisterPayload, CommonVersionPayload, ConfigFilePayload, ExtensibleEnumDataPayload,
    FeatureFlagDataPayload, FrozenRegistryPayload, FrozenRegistrySyncStartPayload,
    KnownRegistryDataMapsPayload, decode_payload,
};

use crate::record::{Direction, Entry};
use crate::registries::decode_frozen_registry;

/// The registries that `IronPumpkin` syncs.
///
/// The other `frozen_registry` payloads of the reference capture are left out of the comparison. A
/// recording is never filtered: a registry that `IronPumpkin` sends outside this list is a
/// difference.
pub const SYNCED_REGISTRIES: [&str; 3] =
    ["minecraft:block", "minecraft:item", "minecraft:entity_type"];

/// The channels whose clientbound bodies must equal the capture. See [`comparable_body`].
pub const SAME_BODY_CHANNELS: [&str; 6] = [
    CommonVersionPayload::CHANNEL,
    CommonRegisterPayload::CHANNEL,
    ConfigFilePayload::CHANNEL,
    KnownRegistryDataMapsPayload::CHANNEL,
    ExtensibleEnumDataPayload::CHANNEL,
    FeatureFlagDataPayload::CHANNEL,
];

/// The clientbound custom payload channels of a reference capture.
///
/// This is the window of [`recorded_channels`] without the `frozen_registry` payloads of registries
/// outside [`SYNCED_REGISTRIES`].
pub fn synced_channels(entries: &[Entry]) -> Result<Vec<String>, crate::Error> {
    window_channels(entries, true)
}

/// The clientbound custom payload channels of a recording, from
/// `neoforge:frozen_registry_sync_start` to `finish_configuration`. Every `frozen_registry`
/// payload is kept.
pub fn recorded_channels(entries: &[Entry]) -> Result<Vec<String>, crate::Error> {
    window_channels(entries, false)
}

fn window_channels(entries: &[Entry], synced_only: bool) -> Result<Vec<String>, crate::Error> {
    let clientbound: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.direction == Direction::Clientbound)
        .collect();
    let start = clientbound
        .iter()
        .position(|e| e.channel.as_deref() == Some(FrozenRegistrySyncStartPayload::CHANNEL))
        .ok_or("no neoforge:frozen_registry_sync_start in the recording")?;
    let end = clientbound
        .iter()
        .position(|e| e.packet == "finish_configuration")
        .ok_or("no finish_configuration in the recording")?;
    let window = clientbound
        .get(start..end)
        .ok_or("finish_configuration comes before neoforge:frozen_registry_sync_start")?;
    let mut channels = Vec::new();
    for entry in window {
        let Some(channel) = &entry.channel else {
            continue;
        };
        if synced_only && channel == FrozenRegistryPayload::CHANNEL {
            let registry = decode_frozen_registry(entry)?.registry_name.to_string();
            if !SYNCED_REGISTRIES.contains(&registry.as_str()) {
                continue;
            }
        }
        channels.push(channel.clone());
    }
    Ok(channels)
}

/// The body of a payload in the form the comparison uses: the hex bytes, except for
/// `neoforge:known_registry_data_maps`. `NeoForge` writes that map in the hash order of its
/// registry keys, which changes from one server start to the next, so its registries and data
/// maps are compared decoded and sorted. A body that does not decode is an error: two equal
/// broken bodies must not pass as equal.
fn comparable_body(entry: &Entry) -> Result<String, crate::Error> {
    if entry.channel.as_deref() != Some(KnownRegistryDataMapsPayload::CHANNEL) {
        return Ok(entry.data.clone());
    }
    let data = hex::decode(&entry.data)?;
    let payload = decode_payload(&data, entry.reassembled, KnownRegistryDataMapsPayload::read)?;
    let mut registries: Vec<String> = payload
        .data_maps
        .iter()
        .map(|(registry, maps)| {
            let mut maps: Vec<String> = maps
                .iter()
                .map(|m| format!("{}{}", m.id, if m.mandatory { "!" } else { "" }))
                .collect();
            maps.sort();
            format!("{registry}={}", maps.join(","))
        })
        .collect();
    registries.sort();
    Ok(registries.join(" "))
}

/// The clientbound bodies on `channel`, in order, as [`comparable_body`] shows them.
fn bodies(entries: &[Entry], channel: &str) -> Result<Vec<String>, crate::Error> {
    entries
        .iter()
        .filter(|e| e.direction == Direction::Clientbound && e.channel.as_deref() == Some(channel))
        .map(comparable_body)
        .collect()
}

/// Everything that differs between a recording and a reference capture.
///
/// That is the channel order of [`recorded_channels`] against [`synced_channels`] and the bodies of
/// [`SAME_BODY_CHANNELS`]. A body that does not decode is a failure too. An empty list means they
/// match.
#[must_use]
pub fn compare_with_reference(actual: &[Entry], reference: &[Entry]) -> Vec<String> {
    let mut failures = Vec::new();
    match (recorded_channels(actual), synced_channels(reference)) {
        (Ok(actual), Ok(reference)) if actual != reference => failures.push(format!(
            "channel order differs: reference {reference:?}, recording {actual:?}"
        )),
        (Ok(_), Ok(_)) => {}
        (actual, reference) => {
            if let Err(e) = actual {
                failures.push(format!("recording: {e}"));
            }
            if let Err(e) = reference {
                failures.push(format!("reference: {e}"));
            }
        }
    }
    for channel in SAME_BODY_CHANNELS {
        match (bodies(actual, channel), bodies(reference, channel)) {
            (Ok(actual), Ok(reference)) if actual != reference => failures.push(format!(
                "{channel} bodies differ: reference {reference:?}, recording {actual:?}"
            )),
            (Ok(_), Ok(_)) => {}
            (actual, reference) => {
                if let Err(e) = actual {
                    failures.push(format!("recording: {channel} body: {e}"));
                }
                if let Err(e) = reference {
                    failures.push(format!("reference: {channel} body: {e}"));
                }
            }
        }
    }
    failures
}
