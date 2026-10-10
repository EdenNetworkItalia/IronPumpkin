//! The startup content phase writes the content manifest and restores missing content as
//! placeholders, with their explicit tags and the mod tags they list. The content registry is
//! process-wide and freezes once, so each server start is a child process that reruns this test
//! binary with `CONTENT_PHASE` set.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use pumpkin_core::content::{self, ContentError, manifest_path};
use pumpkin_core::entity::custom::{factory, register_entity_type};
use pumpkin_core::entity::{Entity, EntityBase};
use pumpkin_data::dynamic::{
    self, BlockDefinition, BlockPropertyDefinition, ContentKind, DynamicTaggable,
    EntityTypeDefinition, ItemDefinition, RegistryError, TagDefinition,
};
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::tag::RegistryKey;
use pumpkin_data::{Block, BlockStateId};
use pumpkin_nbt::compound::NbtCompound;
use serde_json::{Value, json};
use uuid::Uuid;

const PHASE: &str = "CONTENT_PHASE";
const WORLD: &str = "CONTENT_WORLD";
const TEST_NAME: &str = "content_phase_writes_the_manifest_and_restores_placeholders";

fn spawn(entity: Entity) -> Arc<dyn EntityBase> {
    Arc::new(entity)
}

fn golem() -> EntityTypeDefinition {
    EntityTypeDefinition {
        name: "test:golem".to_string(),
        display: &EntityType::IRON_GOLEM,
        dimensions: None,
        eye_height: None,
        tags: vec!["skeletons".to_string()],
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

/// The block lamp lists the block tag, and the item tag lists the item lamp.
fn register_tags() {
    dynamic::register_tag(TagDefinition {
        kind: ContentKind::Block,
        name: "test:lamps".to_string(),
        values: strings(&["redstone_lamp"]),
    })
    .unwrap();
    dynamic::register_tag(TagDefinition {
        kind: ContentKind::Item,
        name: "test:lamps".to_string(),
        values: strings(&["test:lamp", "minecraft:glowstone"]),
    })
    .unwrap();
}

fn lamp_item() -> ItemDefinition {
    ItemDefinition {
        name: "test:lamp".to_string(),
        display: &Item::REDSTONE_LAMP,
        block: Some("test:lamp".to_string()),
        tags: strings(&["minecraft:piglin_loved"]),
    }
}

const LAMP_TAGS: [&str; 2] = ["#test:lamps", "mineable/pickaxe"];

/// Reruns this test in a child process for one server start and returns its log.
fn start(phase: &str, world: &Path) -> String {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([TEST_NAME, "--exact", "--nocapture", "--test-threads=1"])
        .env(PHASE, phase)
        .env(WORLD, world)
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&output.stderr).into_owned()
        + &String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{phase} start failed:\n{log}");
    log
}

#[test]
fn content_phase_writes_the_manifest_and_restores_placeholders() {
    if let Ok(phase) = std::env::var(PHASE) {
        tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(std::io::stderr)
            .init();
        let world = std::env::var(WORLD).unwrap();
        match phase.as_str() {
            "register" => start_with_mod(Path::new(&world)),
            "missing" => start_without_mod(Path::new(&world)),
            "vanilla" => start_vanilla(Path::new(&world)),
            "changed" => start_with_changed_mod(Path::new(&world)),
            "invalid" => start_with_invalid_manifests(Path::new(&world)),
            _ => panic!("unknown phase {phase}"),
        }
        return;
    }

    let world = tempfile::tempdir().unwrap();
    let log = start("register", world.path());
    assert!(!log.contains("no mod registers it"), "{log}");
    let written = std::fs::read_to_string(manifest_path(world.path())).unwrap();

    let log = start("missing", world.path());
    for name in [
        "block \"test:lamp\"",
        "item \"test:lamp\"",
        "entity type \"test:golem\"",
        "block tag \"test:lamps\"",
        "item tag \"test:lamps\"",
    ] {
        let warning = format!("{name} is in the content manifest but no mod registers it");
        assert_eq!(log.matches(&warning).count(), 1, "{warning}\n{log}");
    }
    // The placeholders keep every entry, so the rewritten manifest is identical.
    let rewritten = std::fs::read_to_string(manifest_path(world.path())).unwrap();
    assert_eq!(rewritten, written);

    let vanilla = tempfile::tempdir().unwrap();
    start("vanilla", vanilla.path());
    assert_eq!(std::fs::read_dir(vanilla.path()).unwrap().count(), 0);

    let log = start("changed", world.path());
    assert!(!log.contains("no mod registers it"), "{log}");
    assert!(!log.contains("cannot restore"), "{log}");
    let warning =
        "the block \"test:lamp\" in the content manifest differs from what its mod registers";
    assert_eq!(log.matches(warning).count(), 1, "{log}");
    assert_eq!(
        log.matches("differs from what its mod registers").count(),
        1,
        "{log}"
    );

    let invalid = tempfile::tempdir().unwrap();
    start("invalid", invalid.path());
}

fn start_with_mod(world: &Path) {
    dynamic::register_block(BlockDefinition {
        name: "test:lamp".to_string(),
        display: Block::REDSTONE_LAMP.default_state.id,
        properties: vec![
            BlockPropertyDefinition::enumeration(
                "facing",
                &["north", "south", "west", "east"],
                "west",
            ),
            BlockPropertyDefinition::bool("lit", false),
        ],
        tags: strings(&LAMP_TAGS),
    })
    .unwrap();
    dynamic::register_item(lamp_item()).unwrap();
    register_entity_type(golem(), spawn).unwrap();
    register_tags();

    assert_eq!(content::run(world).unwrap(), []);
    assert_eq!(
        register_entity_type(golem(), spawn),
        Err(RegistryError::RegistryFrozen)
    );

    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(manifest_path(world)).unwrap()).unwrap();
    assert_eq!(
        manifest,
        json!({
            "format": 1,
            "blocks": {
                "test:lamp": {
                    "properties": {
                        "facing": ["north", "south", "west", "east"],
                        "lit": ["true", "false"]
                    },
                    "default": { "facing": "west", "lit": "false" },
                    "display": "minecraft:redstone_lamp[lit=false]",
                    "tags": ["minecraft:mineable/pickaxe", "test:lamps"]
                }
            },
            "items": { "test:lamp": {
                "display": "minecraft:redstone_lamp",
                "tags": ["minecraft:piglin_loved"]
            } },
            "entity_types": { "test:golem": {
                "display": "minecraft:iron_golem",
                "tags": ["minecraft:skeletons"]
            } },
            "block_tags": { "test:lamps": ["minecraft:redstone_lamp"] },
            "item_tags": { "test:lamps": ["minecraft:glowstone", "test:lamp"] }
        })
    );
    assert_tags();
}

/// The tags of the content of `start_with_mod`, with the mod or with its placeholders.
fn assert_tags() {
    let lamp = Block::from_name("test:lamp").unwrap();
    // Explicit, not inherited from the display block.
    assert!(!Block::REDSTONE_LAMP.has_tag_dynamic("minecraft:mineable/pickaxe"));
    assert!(lamp.has_tag_dynamic("minecraft:mineable/pickaxe"));
    assert!(lamp.has_tag_dynamic("test:lamps"));
    assert_eq!(
        dynamic::tag_ids(RegistryKey::Block, "test:lamps"),
        Some((
            &[Block::REDSTONE_LAMP.id.as_u16()][..],
            &[lamp.id.as_u16()][..]
        ))
    );

    let item = Item::from_registry_key("test:lamp").unwrap();
    assert!(!Item::REDSTONE_LAMP.has_tag_dynamic("minecraft:piglin_loved"));
    assert!(item.has_tag_dynamic("minecraft:piglin_loved"));
    assert_eq!(
        dynamic::tag_ids(RegistryKey::Item, "test:lamps"),
        Some((&[Item::GLOWSTONE.id][..], &[item.id][..]))
    );

    let golem = EntityType::from_name("test:golem").unwrap();
    assert!(!EntityType::IRON_GOLEM.has_tag_dynamic("minecraft:skeletons"));
    assert!(golem.has_tag_dynamic("minecraft:skeletons"));
}

fn start_without_mod(world: &Path) {
    assert_eq!(
        content::run(world).unwrap(),
        [
            (ContentKind::Block, "test:lamp".to_string()),
            (ContentKind::Item, "test:lamp".to_string()),
            (ContentKind::EntityType, "test:golem".to_string()),
        ]
    );

    let lamp = Block::from_name("test:lamp").unwrap();
    assert!(dynamic::is_placeholder(ContentKind::Block, "test:lamp"));
    assert_eq!(lamp.states.len(), 8);
    assert_eq!(
        lamp.properties(lamp.default_state.id).unwrap().to_props(),
        [("facing", "west"), ("lit", "false")]
    );
    assert_eq!(
        lamp.default_state.id.display_state(),
        Block::REDSTONE_LAMP.default_state.id
    );
    assert!(BlockStateId::new(lamp.default_state.id.as_u16()).is_some());

    let item = Item::from_registry_key("test:lamp").unwrap();
    assert!(dynamic::is_placeholder(ContentKind::Item, "test:lamp"));
    assert_eq!(
        item.to_java_network_id(pumpkin_data::dynamic::ContentIds::Display),
        Item::REDSTONE_LAMP.id
    );

    assert_tags();

    let golem = EntityType::from_name("test:golem").unwrap();
    assert!(golem.is_placeholder());
    assert!(!golem.summonable);
    assert!(factory(golem).is_none());
    assert_eq!(
        golem.to_java_network_id(pumpkin_data::dynamic::ContentIds::Display),
        EntityType::IRON_GOLEM.id
    );
    assert!(!EntityType::IRON_GOLEM.is_placeholder());

    // A chunk goes live, then its live entities are saved: the placeholder record never spawns
    // and is written back unchanged.
    let mut placeholder = NbtCompound::new();
    placeholder.put_string("id", "test:golem".to_string());
    placeholder.put_uuid("UUID", Uuid::from_u128(1));
    placeholder.put_int("charge", 7);
    let mut zombie = NbtCompound::new();
    zombie.put_string("id", "minecraft:zombie".to_string());
    zombie.put_uuid("UUID", Uuid::from_u128(2));
    let mut data = vec![placeholder.clone(), zombie.clone()];

    let spawnable = content::take_spawnable_records(&mut data);
    assert_eq!(spawnable, [zombie.clone()]);
    assert_eq!(data, [placeholder.clone()]);

    content::rebuild_live_records(&mut data, vec![zombie.clone()]);
    assert_eq!(data, [placeholder, zombie]);
}

/// A mod update drops the `facing` property; the item and the entity type stay the same.
fn start_with_changed_mod(world: &Path) {
    dynamic::register_block(BlockDefinition {
        name: "test:lamp".to_string(),
        display: Block::REDSTONE_LAMP.default_state.id,
        properties: vec![BlockPropertyDefinition::bool("lit", false)],
        tags: strings(&LAMP_TAGS),
    })
    .unwrap();
    dynamic::register_item(lamp_item()).unwrap();
    register_entity_type(golem(), spawn).unwrap();
    register_tags();

    assert_eq!(content::run(world).unwrap(), []);
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(manifest_path(world)).unwrap()).unwrap();
    assert_eq!(
        manifest["blocks"]["test:lamp"]["properties"],
        json!({ "lit": ["true", "false"] })
    );
}

/// An entry that cannot be restored stops the run and leaves the manifest as it is. The run
/// fails before the freeze, so one process checks several manifests.
fn start_with_invalid_manifests(root: &Path) {
    let entries = [
        json!({ "blocks": { "test:gone": { "display": "minecraft:no_such_block" } } }),
        json!({ "blocks": { "test:one": {
            "properties": { "lit": ["true"] },
            "default": { "lit": "true" },
            "display": "minecraft:stone"
        } } }),
        json!({ "items": { "test:gone": { "display": "minecraft:no_such_item" } } }),
        json!({ "block_tags": { "minecraft:ores": [] } }),
        json!({ "item_tags": { "test:gems": ["minecraft:no_such_item"] } }),
    ];
    for (index, mut manifest) in entries.into_iter().enumerate() {
        manifest["format"] = json!(1);
        let world = root.join(index.to_string());
        let path = manifest_path(&world);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text = manifest.to_string();
        std::fs::write(&path, &text).unwrap();

        let error = content::run(&world).unwrap_err();
        assert!(matches!(error, ContentError::Entry { .. }), "{error}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }

    // A mod tag resolves at the freeze, which fails and also leaves the manifest as it is. The
    // failed freeze is terminal, so this case comes last.
    let world = root.join("unknown_tag");
    let path = manifest_path(&world);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let text = json!({
        "format": 1,
        "blocks": { "test:tagged": { "display": "minecraft:stone", "tags": ["test:no_such_tag"] } }
    })
    .to_string();
    std::fs::write(&path, &text).unwrap();
    let error = content::run(&world).unwrap_err();
    assert!(
        matches!(
            error,
            ContentError::Freeze(RegistryError::UnknownTag { .. })
        ),
        "{error}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
}

fn start_vanilla(world: &Path) {
    assert_eq!(content::run(world).unwrap(), []);
    assert!(!manifest_path(world).exists());
}
