//! Native mod for the boot tests of the root workspace. It registers one block, one item and one
//! entity type, each with a vanilla display entry. A test binary links it with
//! `use ironpumpkin_test_mod as _;` and starts the server with `pumpkin::run`.

use std::sync::Arc;

use ironpumpkin_mods::{
    ModInit, NativeMod,
    content::{BlockBuilder, EntityTypeBuilder, ItemBuilder, RegistryError},
    entity::{Entity, EntityBase},
    pumpkin_data::{Block, entity::EntityType, item::Item},
    register_mod,
};

pub const ID: &str = "test-mod";
pub const BLOCK: &str = "test-mod:test_block";
pub const ITEM: &str = "test-mod:test_item";
pub const ENTITY_TYPE: &str = "test-mod:test_entity";

struct TestMod;

impl NativeMod for TestMod {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        "Test Mod"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn init(&self, cx: &mut ModInit) {
        // `ModInit` keeps the first content error, and the server stops on it after `init`.
        let _ = register_content(cx);
    }
}

fn register_content(cx: &mut ModInit) -> Result<(), RegistryError> {
    cx.register_block(BlockBuilder::new(
        BLOCK,
        Block::REDSTONE_LAMP.default_state.id,
    ))?;
    cx.register_item(ItemBuilder::new(ITEM, &Item::STICK))?;
    cx.register_entity_type(EntityTypeBuilder::new(ENTITY_TYPE, &EntityType::PIG), spawn)
}

fn spawn(entity: Entity) -> Arc<dyn EntityBase> {
    Arc::new(entity)
}

register_mod!(TestMod);
