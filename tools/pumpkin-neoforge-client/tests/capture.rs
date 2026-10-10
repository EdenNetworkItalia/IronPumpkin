//! The registry expectation check and the capture comparison, run on run (b) of the
//! `NeoForge` 26.3.0.64-beta capture.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::path::Path;

use pumpkin_neoforge_client::{
    compare::{SYNCED_REGISTRIES, compare_with_reference, recorded_channels, synced_channels},
    record::{Entry, read_jsonl},
    registries::{RegistryExpectations, check_registries, decode_frozen_registry},
};
use pumpkin_protocol::java::neoforge::{
    CommonRegisterPayload, FrozenRegistryPayload, KnownRegistryDataMapsPayload, RegistrySnapshot,
    decode_exact,
};
use pumpkin_util::identifier::Identifier;

fn run_b() -> Vec<Entry> {
    read_jsonl(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("captures/neoforge-26.3.0.64-beta/run-b-neoforge.jsonl.gz"),
    )
    .unwrap()
}

fn expectations(text: &str) -> RegistryExpectations {
    RegistryExpectations::parse(text).unwrap()
}

const PLAIN: &str = include_str!("../expected/pumpkin-neoforge-registries.toml");
const TEST_MOD: &str = include_str!("../expected/pumpkin-neoforge-test-mod-registries.toml");

fn is_registry(entry: &Entry) -> bool {
    entry.channel.as_deref() == Some(FrozenRegistryPayload::CHANNEL)
}

/// Run (b) without the `frozen_registry` payloads of the registries outside `keep`, in the order
/// of `keep`.
fn limited(entries: &[Entry], keep: &[&str]) -> Vec<Entry> {
    let registries: Vec<(String, Entry)> = entries
        .iter()
        .filter(|e| is_registry(e))
        .map(|e| {
            let name = decode_frozen_registry(e).unwrap().registry_name.to_string();
            (name, e.clone())
        })
        .collect();
    let mut out: Vec<Entry> = entries
        .iter()
        .filter(|e| !is_registry(e))
        .cloned()
        .collect();
    let at = out
        .iter()
        .position(|e| e.channel.as_deref() == Some("neoforge:frozen_registry_sync_completed"))
        .unwrap();
    let kept = keep.iter().filter_map(|name| {
        registries
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, e)| e.clone())
    });
    out.splice(at..at, kept);
    out
}

#[test]
fn plain_registry_file_matches_run_b_limited_to_the_synced_registries() {
    let entries = limited(&run_b(), &SYNCED_REGISTRIES);
    let failures = check_registries(&entries, &expectations(PLAIN));
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn registry_check_reports_the_registries_of_a_real_server() {
    let failures = check_registries(&run_b(), &expectations(PLAIN));
    // Only the registry list differs: block, item and entity type have the expected contents.
    assert_eq!(failures.len(), 1, "{failures:#?}");
    assert!(failures[0].contains("got 35"), "{}", failures[0]);
    assert!(
        failures[0].contains("neoforge:fluid_type"),
        "{}",
        failures[0]
    );
}

#[test]
fn registry_check_reports_contents_and_order() {
    let entries = limited(&run_b(), &SYNCED_REGISTRIES);
    let failures = check_registries(&entries, &expectations(TEST_MOD));
    assert_eq!(failures.len(), 6, "{failures:#?}");
    assert!(failures[0].contains("expected 1287 ids, got 1286"));
    assert!(failures[1].contains("expected test-mod:test_block at id 1286, got no entry"));

    let reversed: Vec<&str> = SYNCED_REGISTRIES.iter().rev().copied().collect();
    let entries = limited(&run_b(), &reversed);
    let failures = check_registries(&entries, &expectations(PLAIN));
    assert_eq!(failures.len(), 1, "{failures:#?}");
    assert!(failures[0].contains("missing []"), "{}", failures[0]);
}

fn registry_entry(ids: &[(i32, &'static str)]) -> Entry {
    let payload = FrozenRegistryPayload {
        registry_name: Identifier::vanilla_static("block"),
        snapshot: RegistrySnapshot::from_ids(
            ids.iter()
                .map(|&(id, name)| (id, Identifier::parse_static(name))),
        ),
    };
    let mut data = Vec::new();
    payload.write(&mut data).unwrap();
    let mut entry = run_b().into_iter().find(is_registry).unwrap();
    entry.data = hex::encode(data);
    entry
}

#[test]
fn registry_check_reports_gaps() {
    let expected = expectations(
        "[[registry]]\nname = \"minecraft:block\"\nids = 2\nfirst = \"minecraft:air\"\n\
         last = \"minecraft:stone\"\n",
    );
    let entry = registry_entry(&[(0, "minecraft:air"), (1, "minecraft:stone")]);
    assert!(check_registries(&[entry], &expected).is_empty());
    let entry = registry_entry(&[(0, "minecraft:air"), (2, "minecraft:stone")]);
    let failures = check_registries(&[entry], &expected);
    assert_eq!(failures.len(), 2, "{failures:#?}");
    assert!(failures[0].contains("not 0..2"));
}

#[test]
fn registry_file_rejects_bad_ids_and_fields() {
    let bad_id = r#"
[[registry]]
name = "a:b"
ids = 1
first = "a:c"
last = "a:c"
at = { x = "a:d" }
"#;
    assert!(RegistryExpectations::parse(bad_id).is_err());
    let unknown_field = "[[registry]]\nname = \"a:b\"\ncount = 1\n";
    assert!(RegistryExpectations::parse(unknown_field).is_err());
}

#[test]
fn run_b_matches_itself_and_reports_differences() {
    let reference = run_b();
    // The recording of IronPumpkin has the three synced registries. The reference keeps all 35.
    let recording = limited(&reference, &SYNCED_REGISTRIES);
    assert!(compare_with_reference(&recording, &reference).is_empty());

    let channels = synced_channels(&reference).unwrap();
    assert_eq!(channels.len(), 11, "{channels:?}");
    assert_eq!(channels[0], "neoforge:frozen_registry_sync_start");
    assert_eq!(channels[10], "neoforge:feature_flags");

    // Only block, item and entity type of the reference count, in any order.
    let synced = limited(
        &reference,
        &["minecraft:entity_type", "minecraft:block", "minecraft:item"],
    );
    assert!(compare_with_reference(&synced, &reference).is_empty());

    let two = limited(&reference, &["minecraft:block", "minecraft:item"]);
    let failures = compare_with_reference(&two, &reference);
    assert_eq!(failures.len(), 1, "{failures:#?}");
    assert!(failures[0].starts_with("channel order differs"));

    // A fourth registry in the recording is a difference: the filter applies to the reference only.
    let extra = limited(
        &reference,
        &[
            "minecraft:block",
            "minecraft:item",
            "minecraft:entity_type",
            "minecraft:mob_effect",
        ],
    );
    assert_eq!(
        extra.iter().filter(|e| is_registry(e)).count(),
        4,
        "run (b) must have minecraft:mob_effect"
    );
    let failures = compare_with_reference(&extra, &reference);
    assert_eq!(failures.len(), 1, "{failures:#?}");
    assert!(failures[0].starts_with("channel order differs"));
    assert_eq!(recorded_channels(&extra).unwrap().len(), channels.len() + 1);

    let mut changed = recording;
    let register = changed
        .iter_mut()
        .find(|e| e.channel.as_deref() == Some(CommonRegisterPayload::CHANNEL))
        .unwrap();
    register.data.push_str("00");
    let failures = compare_with_reference(&changed, &reference);
    assert_eq!(failures.len(), 1, "{failures:#?}");
    assert!(failures[0].starts_with("c:register bodies differ"));
}

/// A second start of the same `NeoForge` server writes the data map registries in another order.
#[test]
fn data_maps_compare_without_order() {
    let reference = run_b();
    let mut changed = limited(&reference, &SYNCED_REGISTRIES);
    let at = changed
        .iter()
        .position(|e| e.channel.as_deref() == Some(KnownRegistryDataMapsPayload::CHANNEL))
        .unwrap();
    let mut payload = decode_exact(
        &hex::decode(&changed[at].data).unwrap(),
        KnownRegistryDataMapsPayload::read,
    )
    .unwrap();
    payload.data_maps.reverse();
    let write = |payload: &KnownRegistryDataMapsPayload| {
        let mut data = Vec::new();
        payload.write(&mut data).unwrap();
        hex::encode(data)
    };
    changed[at].data = write(&payload);
    assert!(compare_with_reference(&changed, &reference).is_empty());

    let maps = payload
        .data_maps
        .values_mut()
        .find(|m| !m.is_empty())
        .unwrap();
    maps[0].mandatory = true;
    changed[at].data = write(&payload);
    let failures = compare_with_reference(&changed, &reference);
    assert_eq!(failures.len(), 1, "{failures:#?}");
}

/// A data map body that does not decode is a failure, also when both sides are broken the same
/// way.
#[test]
fn undecodable_data_maps_are_a_failure() {
    let reference = run_b();
    let recording = limited(&reference, &SYNCED_REGISTRIES);
    let at = |entries: &[Entry]| {
        entries
            .iter()
            .position(|e| e.channel.as_deref() == Some(KnownRegistryDataMapsPayload::CHANNEL))
            .unwrap()
    };

    // Cut the body in the middle of the payload: valid hex, truncated data.
    let mut broken = recording.clone();
    let i = at(&broken);
    let half = broken[i].data.len() / 4 * 2;
    broken[i].data.truncate(half);
    let failures = compare_with_reference(&broken, &reference);
    assert_eq!(failures.len(), 1, "{failures:#?}");
    assert!(
        failures[0].starts_with("recording: neoforge:known_registry_data_maps body"),
        "{failures:#?}"
    );

    // The same cut in both bodies must not compare as equal.
    let mut broken_reference = reference.clone();
    let j = at(&broken_reference);
    broken_reference[j].data = broken[i].data.clone();
    let failures = compare_with_reference(&broken, &broken_reference);
    assert_eq!(failures.len(), 2, "{failures:#?}");
    assert!(failures[1].starts_with("reference: neoforge:known_registry_data_maps body"));

    // A body that is not hex.
    let mut not_hex = recording;
    let i = at(&not_hex);
    not_hex[i].data = "zz".to_owned();
    let failures = compare_with_reference(&not_hex, &reference);
    assert_eq!(failures.len(), 1, "{failures:#?}");
}
