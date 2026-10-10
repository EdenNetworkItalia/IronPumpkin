//! The encoded registry sync payloads decode to the snapshots of the frozen content tables. The
//! content registry is process-wide and freezes once, so each case is a child process that reruns
//! this test binary with `SYNC_CASE` set.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

use std::path::Path;
use std::process::Command;

use pumpkin_core::content;
use pumpkin_core::net::java::configuration_tasks::RegistrySyncPayloads;
use pumpkin_data::Block;
use pumpkin_data::BlockId;
use pumpkin_data::dynamic::{self, BlockDefinition};
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_protocol::java::neoforge::{
    FrozenRegistryPayload, FrozenRegistrySyncStartPayload, decode_exact,
};

const CASE: &str = "SYNC_CASE";
const TEST_NAME: &str = "registry_sync_payloads_decode_to_the_frozen_snapshots";

/// Decodes the payloads and checks the parts that every case shares: the start payload lists the
/// registries of the snapshot payloads, every name is namespaced and no snapshot has aliases.
fn decode(payloads: &RegistrySyncPayloads) -> Vec<FrozenRegistryPayload> {
    let start = decode_exact(&payloads.start, FrozenRegistrySyncStartPayload::read).unwrap();
    let registries: Vec<FrozenRegistryPayload> = payloads
        .registries
        .iter()
        .map(|data| decode_exact(data, FrozenRegistryPayload::read).unwrap())
        .collect();
    let names: Vec<String> = registries
        .iter()
        .map(|registry| registry.registry_name.to_string())
        .collect();
    assert_eq!(
        names,
        ["minecraft:block", "minecraft:item", "minecraft:entity_type"]
    );
    let listed: Vec<String> = start.to_access.iter().map(ToString::to_string).collect();
    assert_eq!(listed, names);
    for registry in &registries {
        assert!(registry.snapshot.aliases.is_empty());
        assert!(
            registry
                .snapshot
                .ids
                .values()
                .all(|name| name.to_string().contains(':'))
        );
    }
    registries
}

/// The id count, the first and the last entry of a snapshot.
fn shape(registry: &FrozenRegistryPayload) -> (usize, (i32, String), (i32, String)) {
    let ids = &registry.snapshot.ids;
    let entry = |(id, name): (&i32, &pumpkin_util::identifier::Identifier)| (*id, name.to_string());
    (
        ids.len(),
        entry(ids.first_key_value().unwrap()),
        entry(ids.last_key_value().unwrap()),
    )
}

fn entry(id: u16, name: &str) -> (i32, String) {
    (i32::from(id), name.to_owned())
}

/// No mod registers content: the snapshots equal those of a `NeoForge` 26.3.0.64-beta server
/// without mods (run (b) of `tools/pumpkin-neoforge-client/captures/neoforge-26.3.0.64-beta/`).
fn without_content() {
    assert!(RegistrySyncPayloads::get().is_none(), "before the freeze");
    dynamic::freeze().unwrap();
    let payloads = RegistrySyncPayloads::get().unwrap();
    // Encoded once: every call returns the same payloads.
    assert!(std::ptr::eq(payloads, RegistrySyncPayloads::get().unwrap()));
    let registries = decode(payloads);
    assert_eq!(
        shape(&registries[0]),
        (
            1286,
            entry(0, "minecraft:air"),
            entry(BlockId::COUNT - 1, "minecraft:firefly_bush")
        )
    );
    assert_eq!(
        shape(&registries[1]),
        (
            1658,
            entry(0, "minecraft:air"),
            entry(Item::COUNT - 1, "minecraft:ominous_bottle")
        )
    );
    assert_eq!(
        shape(&registries[2]),
        (
            161,
            entry(0, "minecraft:acacia_boat"),
            entry(EntityType::COUNT - 1, "minecraft:fishing_bobber")
        )
    );
    for registry in &registries {
        let ids: Vec<i32> = registry.snapshot.ids.keys().copied().collect();
        let dense: Vec<i32> = (0..i32::try_from(ids.len()).unwrap()).collect();
        assert_eq!(ids, dense, "{}", registry.registry_name);
    }
}

/// The manifest names a block whose mod is missing. Its placeholder id is a gap between the ids
/// of two registered blocks.
fn with_placeholder(world: &Path) {
    for name in ["alpha:block", "zeta:block"] {
        dynamic::register_block(BlockDefinition {
            name: name.to_owned(),
            display: Block::STONE.default_state.id,
            properties: Vec::new(),
            tags: Vec::new(),
        })
        .unwrap();
    }
    let manifest = content::manifest_path(world);
    std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    std::fs::write(
        &manifest,
        r#"{"format":1,"blocks":{"gone:block":{"display":"minecraft:stone"}}}"#,
    )
    .unwrap();
    let placeholders = content::run(world).unwrap();
    assert_eq!(placeholders.len(), 1, "{placeholders:?}");

    let registries = decode(RegistrySyncPayloads::get().unwrap());
    let blocks = &registries[0].snapshot.ids;
    let count = i32::from(BlockId::COUNT);
    assert_eq!(blocks.len(), usize::from(BlockId::COUNT) + 2);
    assert_eq!(
        blocks.get(&(count - 1)).map(ToString::to_string).as_deref(),
        Some("minecraft:firefly_bush")
    );
    assert_eq!(
        blocks.get(&count).map(ToString::to_string).as_deref(),
        Some("alpha:block")
    );
    assert_eq!(blocks.get(&(count + 1)), None);
    assert_eq!(
        blocks.get(&(count + 2)).map(ToString::to_string).as_deref(),
        Some("zeta:block")
    );
    assert_eq!(registries[1].snapshot.ids.len(), usize::from(Item::COUNT));
    assert_eq!(
        registries[2].snapshot.ids.len(),
        usize::from(EntityType::COUNT)
    );
}

fn run_case(case: &str) {
    let world = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([TEST_NAME, "--exact", "--nocapture", "--test-threads=1"])
        .env(CASE, case)
        .current_dir(world.path())
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&output.stderr).into_owned()
        + &String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "case {case} failed:\n{log}");
}

#[test]
fn registry_sync_payloads_decode_to_the_frozen_snapshots() {
    if let Ok(case) = std::env::var(CASE) {
        match case.as_str() {
            "without_content" => without_content(),
            "with_placeholder" => with_placeholder(&std::env::current_dir().unwrap()),
            _ => panic!("unknown case {case}"),
        }
        return;
    }
    run_case("without_content");
    run_case("with_placeholder");
}
