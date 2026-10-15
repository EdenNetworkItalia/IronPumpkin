//! Native mod for the boot tests of the root workspace. It registers one block, one item and one
//! entity type, each with a vanilla display entry, the synced config [`SYNCED_CONFIG`] and the
//! startup config [`STARTUP_CONFIG`]. A test binary links it by using one of its items, such as
//! `use ironpumpkin_test_mod::BLOCK;`, and starts the server with `pumpkin::run`.

use std::sync::Arc;

use ironpumpkin_mods::{
    ModInit, NativeMod,
    config::{ConfigError, ConfigSpec, ConfigType, ModConfig},
    content::{BlockBuilder, EntityTypeBuilder, ItemBuilder, RegistryError},
    entity::{Entity, EntityBase},
    pumpkin_data::{Block, entity::EntityType, item::Item},
    register_mod,
    tracing::{error, info},
};

pub const ID: &str = "test-mod";
pub const BLOCK: &str = "test-mod:test_block";
pub const ITEM: &str = "test-mod:test_item";
pub const ENTITY_TYPE: &str = "test-mod:test_entity";
/// The default file name of the synced config, in `config/`.
pub const SYNCED_CONFIG: &str = "test-mod-synced.toml";
/// The default file name of the startup config, in `config/`. Its `greeting` is logged from
/// `init` after [`STARTUP_GREETING`].
pub const STARTUP_CONFIG: &str = "test-mod-startup.toml";
pub const STARTUP_GREETING: &str = "test-mod startup greeting: ";
/// The log line of the reloading hook for the synced config.
pub const SYNCED_RELOADED: &str = "test-mod reloaded test-mod-synced.toml";

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
        if let Err(err) = register_configs(cx) {
            error!("{ID}: {err}");
        }
        // `ModInit` keeps the first content error, and the server stops on it after `init`.
        let _ = register_content(cx);
    }

    fn config_reloading(&self, config: &ModConfig) {
        info!("{ID} reloaded {}", config.file_name());
    }
}

fn register_configs(cx: &mut ModInit) -> Result<(), ConfigError> {
    let mut startup = ConfigSpec::builder();
    startup
        .comment(" Logged from init")
        .define("greeting", "hello".to_owned());
    let startup = cx.register_config(ConfigType::Startup, startup.build()?)?;
    info!("{STARTUP_GREETING}{}", startup.get::<String>("greeting")?);

    let mut synced = ConfigSpec::builder();
    synced.comment(" General settings").push("general");
    synced
        .comment(" Whether the mod is on")
        .define("enabled", true);
    synced.define_in_range("damage", 10i64, 0, 100);
    synced.pop();
    cx.register_config(ConfigType::Synced, synced.build()?)?;
    Ok(())
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
