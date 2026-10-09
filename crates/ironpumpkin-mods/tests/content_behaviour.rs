//! A native mod attaches behaviours to its blocks and items through the builders, and the server
//! registries run them for the custom ids. The content registry freezes once per process, so this
//! file is its own test binary. It uses `ironpumpkin-mods` only, like a mod crate does.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::sync::{Arc, Once};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    content::{BlockBehaviour, BlockBuilder, ItemBuilder, RegistryError},
    mods,
    pumpkin_core::{
        block::{PathComputationType, blocks::anvil::AnvilBlock, registry},
        item::items::{self, spyglass::SpyglassItem},
    },
    pumpkin_data::{Block, BlockState, dynamic, item::Item},
    register_mod,
};

/// A behaviour of the mod: a full cube that mobs path through.
struct Walkable;

impl BlockBehaviour for Walkable {
    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        true
    }
}

struct BehaviourMod;

impl NativeMod for BehaviourMod {
    fn id(&self) -> &'static str {
        "behaviour-mod"
    }

    fn display_name(&self) -> &'static str {
        "Behaviour Mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        // A behaviour of the mod on a full cube.
        cx.register_block(
            BlockBuilder::new("behaviour-mod:lamp", Block::REDSTONE_LAMP.default_state.id)
                .behaviour(Arc::new(Walkable)),
        )
        .unwrap();
        // A server behaviour on a block that is not a full cube. Only `is_pathfindable` runs:
        // `AnvilBlock::on_place` reads block properties and would panic on this block.
        cx.register_block(
            BlockBuilder::new("behaviour-mod:heavy_torch", Block::TORCH.default_state.id)
                .behaviour(Arc::new(AnvilBlock)),
        )
        .unwrap();
        cx.register_block(BlockBuilder::new(
            "behaviour-mod:plain_torch",
            Block::TORCH.default_state.id,
        ))
        .unwrap();
        cx.register_item(
            ItemBuilder::new("behaviour-mod:lens", &Item::STICK).behaviour(Arc::new(SpyglassItem)),
        )
        .unwrap();
        cx.register_item(ItemBuilder::new("behaviour-mod:plain", &Item::STICK))
            .unwrap();
        // A rejected registration drops its behaviour: the first lamp keeps its own.
        assert!(matches!(
            cx.register_block(
                BlockBuilder::new("behaviour-mod:lamp", Block::TORCH.default_state.id)
                    .behaviour(Arc::new(AnvilBlock)),
            ),
            Err(RegistryError::Duplicate { .. })
        ));
    }
}

register_mod!(BehaviourMod);

/// Runs the init of every linked mod and freezes the registry, as the server does at startup.
fn boot() {
    static BOOT: Once = Once::new();
    BOOT.call_once(|| {
        let mods = mods().unwrap();
        assert_eq!(mods.len(), 1);
        for native_mod in mods {
            native_mod.init(&mut ModInit::new(native_mod.id()));
        }
        dynamic::freeze().unwrap();
    });
}

fn land_pathfindable(name: &str) -> bool {
    let block = Block::from_name(name).unwrap();
    registry::default_registry().is_pathfindable(
        block,
        block.default_state,
        PathComputationType::Land,
    )
}

#[test]
fn the_block_registry_runs_the_behaviour_of_a_custom_block() {
    boot();
    // Without a behaviour the registry falls back to the shape: a torch is not a full cube.
    assert!(land_pathfindable("behaviour-mod:plain_torch"));
    // The anvil behaviour of the server says no on a torch shape.
    assert!(!land_pathfindable("behaviour-mod:heavy_torch"));
    // The mod's behaviour says yes on a full cube.
    assert!(land_pathfindable("behaviour-mod:lamp"));
    // Vanilla ids keep their own behaviour.
    assert!(!land_pathfindable("minecraft:redstone_lamp"));
    let blocks = registry::default_registry();
    assert!(blocks.get_pumpkin_block(Block::ANVIL.id).is_some());
    assert!(blocks.get_pumpkin_block(Block::STONE.id).is_none());
}

#[test]
fn the_item_registry_runs_the_behaviour_of_a_custom_item() {
    boot();
    let items = items::default_registry();
    let lens = Item::from_registry_key("behaviour-mod:lens").unwrap();
    let plain = Item::from_registry_key("behaviour-mod:plain").unwrap();
    let spyglass = items.get_use_duration(Item::SPYGLASS.id);
    assert!(spyglass.is_some());
    assert_eq!(items.get_use_duration(lens.id), spyglass);
    assert!(
        items
            .get_pumpkin_item(lens.id)
            .unwrap()
            .as_any()
            .is::<SpyglassItem>()
    );
    assert!(items.get_pumpkin_item(plain.id).is_none());
    assert!(items.get_pumpkin_item(Item::STICK.id).is_none());
    // An id past the custom range has no behaviour and does not panic.
    assert!(items.get_pumpkin_item(u16::MAX - 1).is_none());
}
