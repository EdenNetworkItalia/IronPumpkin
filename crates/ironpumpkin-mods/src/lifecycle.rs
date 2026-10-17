//! The mod lifecycle after construction.
//!
//! The order is that of `NeoForge` 26.3 on a dedicated server. `CommonModLoader.begin` fires
//! `RegisterEvent` once per registry, then `EntityAttributeCreationEvent`,
//! `EntityAttributeModificationEvent`, `RegisterSpawnPlacementsEvent` and
//! `BlockEntityTypeAddBlocksEvent` (`GameData.postRegisterEvents`), and loads the local configs.
//! Then `CommonModLoader.load` runs common setup, sided setup (`FMLDedicatedServerSetupEvent` on
//! a dedicated server), `ExtendPoiTypesEvent` (`RegistrationEvents.init`), enqueue IMC, process
//! IMC and load complete. The registration events are methods of `NativeMod` too: see
//! [`registration`](crate::registration). `NetworkRegistry.setup` then posts `RegisterPayloadHandlersEvent`, and
//! `ServerLifecycleHooks.handleServerAboutToStart` loads the synced configs. Each
//! step calls the method of every mod in load order before the next step starts, and each method
//! runs on its own mod only, as each `NeoForge` mod has its own mod bus.
//!
//! `RegisterEvent` is a Pumpkin event, because a `NeoForge` mod registers its listener on the mod
//! bus: a mod registers a handler with [`ModInit::register_event`](crate::ModInit::register_event)
//! and registers content through [`RegisterEventExt`](crate::content::RegisterEventExt). The
//! server lifecycle events (`ServerAboutToStartEvent`, `ServerStartingEvent` and the others) are
//! game bus events, registered the same way.

use std::sync::Arc;

use crate::{
    LoadedMod, NativeMod,
    config::{self, ConfigType, ModConfig},
    content::ContentKind,
    event::registries::RegisterEvent,
    imc::{ImcQueue, InterModEnqueue, InterModProcess},
    mod_list::ModList,
    registration::RegistrationEvent,
};

/// The registries that `RegisterEvent` fires for, in the order of
/// `GameData.getRegistrationOrder`: `minecraft:attribute` and `minecraft:data_component_type`
/// first, then the vanilla registration order, which puts `minecraft:sound_event` and
/// `minecraft:mob_effect` before `minecraft:block`, `minecraft:entity_type` and `minecraft:item`.
pub(crate) const REGISTRIES: [ContentKind; 7] = [
    ContentKind::Attribute,
    ContentKind::DataComponentType,
    ContentKind::SoundEvent,
    ContentKind::MobEffect,
    ContentKind::Block,
    ContentKind::EntityType,
    ContentKind::Item,
];

/// Runs the steps after the last [`NativeMod::init`](crate::NativeMod::init): `RegisterEvent`
/// for each of [`REGISTRIES`] through `fire_register`, which returns the event after its
/// handlers or `None` when no handler listens, then the registration events and the lifecycle
/// steps, with the local configs loaded before common setup, then the payload registration step `register_payload_handlers`, and the
/// synced configs last.
///
/// # Errors
///
/// Returns the reason when a `RegisterEvent` handler fails to register content, after the last
/// fire, when a registration event keeps an error, after the step, or when a config cannot load.
pub(crate) fn run(
    mods: &[LoadedMod<'_>],
    mod_list: &ModList,
    configs: &[Arc<ModConfig>],
    mut fire_register: impl FnMut(ContentKind) -> Option<RegisterEvent>,
    register_payload_handlers: impl FnOnce(),
) -> Result<(), String> {
    let mut register_error = None;
    for registry in REGISTRIES {
        if let Some(error) = fire_register(registry).and_then(|mut event| event.take_error()) {
            register_error.get_or_insert(error);
        }
    }
    if let Some(error) = register_error {
        return Err(error.to_string());
    }
    // `GameData.postRegisterEvents` posts these after the last `RegisterEvent`.
    post(mods, |m, event| m.entity_attribute_creation(event))?;
    post(mods, |m, event| m.entity_attribute_modification(event))?;
    post(mods, |m, event| m.register_spawn_placements(event))?;
    post(mods, |m, event| m.block_entity_type_add_blocks(event))?;
    config::load(configs, ConfigType::Local)?;
    for loaded in mods {
        loaded.native_mod.common_setup();
    }
    for loaded in mods {
        loaded.native_mod.dedicated_server_setup();
    }
    // `RegistrationEvents.init`, the "Registration events" task after the sided setup.
    post(mods, |m, event| m.extend_poi_types(event))?;
    let mut queue = ImcQueue::default();
    for loaded in mods {
        loaded.native_mod.inter_mod_enqueue(&mut InterModEnqueue {
            sender_mod_id: loaded.mod_id,
            mod_list,
            queue: &mut queue,
        });
    }
    for loaded in mods {
        loaded.native_mod.inter_mod_process(&mut InterModProcess {
            mod_id: loaded.mod_id,
            queue: &mut queue,
        });
    }
    for loaded in mods {
        loaded.native_mod.load_complete();
    }
    register_payload_handlers();
    config::load(configs, ConfigType::Synced)
}

/// Calls the method of a registration event on every mod in load order, as `ModLoader.postEvent`
/// does, and returns the first error of a mod after the last one.
fn post<E: RegistrationEvent>(
    mods: &[LoadedMod<'_>],
    method: impl Fn(&dyn NativeMod, &mut E),
) -> Result<(), String> {
    let mut first_error = None;
    for loaded in mods {
        let mut event = E::new();
        method(loaded.native_mod, &mut event);
        if let Some(error) = event.take_error() {
            first_error.get_or_insert_with(|| {
                format!("the {} of mod {}: {error}", E::NAME, loaded.mod_id)
            });
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    };

    use super::run;
    use crate::{
        LoadedMod, ModInit, NativeMod,
        config::{ConfigSpec, ConfigType, ModConfig},
        content::{ContentKind, RegistryError},
        event::registries::RegisterEvent,
        imc::{InterModEnqueue, InterModProcess},
        mod_list::{ModList, system_mods},
        mods_toml::parse,
        registration::{
            BlockEntityTypeAddBlocksEvent, EntityAttributeCreationEvent,
            EntityAttributeModificationEvent, ExtendPoiTypesEvent, RegisterSpawnPlacementsEvent,
        },
    };

    /// Fires nothing: no handler listens to `RegisterEvent`.
    fn no_handler(_: ContentKind) -> Option<RegisterEvent> {
        None
    }

    fn mod_list(ids: &[&str]) -> ModList {
        let mut infos = system_mods();
        for id in ids {
            let text = format!("license = \"MIT\"\n[[mods]]\nmodId = \"{id}\"\n");
            infos.push(parse(&text, id, "1").unwrap());
        }
        ModList::new(infos)
    }

    static CALLS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn record(call: String) {
        CALLS.lock().unwrap().push(call);
    }

    /// Records each lifecycle call with the id of the mod it runs on. `alpha` sends a message to
    /// `beta` and to a mod that is not loaded.
    struct LifecycleMod(&'static str);

    impl NativeMod for LifecycleMod {
        fn id(&self) -> &'static str {
            self.0
        }

        fn display_name(&self) -> &'static str {
            self.0
        }

        fn version(&self) -> &'static str {
            "1.0"
        }

        fn init(&self, _cx: &mut ModInit) {}

        fn common_setup(&self) {
            record(format!("common_setup {}", self.0));
        }

        fn dedicated_server_setup(&self) {
            record(format!("dedicated_server_setup {}", self.0));
        }

        fn inter_mod_enqueue(&self, imc: &mut InterModEnqueue) {
            record(format!("inter_mod_enqueue {}", self.0));
            if self.0 == "alpha" {
                assert!(imc.send("beta", "register", 42u32));
                assert!(!imc.send("jei", "register", 1u32));
            }
        }

        fn inter_mod_process(&self, imc: &mut InterModProcess) {
            for message in imc.get_messages() {
                record(format!(
                    "inter_mod_process {} got {} from {} to {}: {:?}",
                    self.0,
                    message.method(),
                    message.sender_mod_id(),
                    message.mod_id(),
                    message.payload::<u32>()
                ));
            }
            record(format!("inter_mod_process {}", self.0));
        }

        fn load_complete(&self) {
            record(format!("load_complete {}", self.0));
        }

        fn entity_attribute_creation(&self, _event: &mut EntityAttributeCreationEvent) {
            record(format!("EntityAttributeCreationEvent {}", self.0));
        }

        fn entity_attribute_modification(&self, _event: &mut EntityAttributeModificationEvent) {
            record(format!("EntityAttributeModificationEvent {}", self.0));
        }

        fn register_spawn_placements(&self, _event: &mut RegisterSpawnPlacementsEvent) {
            record(format!("RegisterSpawnPlacementsEvent {}", self.0));
        }

        fn block_entity_type_add_blocks(&self, _event: &mut BlockEntityTypeAddBlocksEvent) {
            record(format!("BlockEntityTypeAddBlocksEvent {}", self.0));
        }

        fn extend_poi_types(&self, _event: &mut ExtendPoiTypesEvent) {
            record(format!("ExtendPoiTypesEvent {}", self.0));
        }
    }

    static ALPHA: LifecycleMod = LifecycleMod("alpha");
    static BETA: LifecycleMod = LifecycleMod("beta");

    #[test]
    fn lifecycle_runs_in_neoforge_order_on_each_mod_and_delivers_imc() {
        let mod_list = mod_list(&["alpha", "beta"]);
        let mods = [
            LoadedMod {
                native_mod: &ALPHA,
                mod_id: "alpha",
            },
            LoadedMod {
                native_mod: &BETA,
                mod_id: "beta",
            },
        ];
        run(
            &mods,
            &mod_list,
            &[],
            |registry| {
                let event = RegisterEvent::new(registry);
                record(format!("RegisterEvent {}", event.registry_key()));
                Some(event)
            },
            || record("RegisterPayloadHandlersEvent".to_owned()),
        )
        .unwrap();
        assert_eq!(
            *CALLS.lock().unwrap(),
            [
                "RegisterEvent minecraft:attribute",
                "RegisterEvent minecraft:data_component_type",
                "RegisterEvent minecraft:sound_event",
                "RegisterEvent minecraft:mob_effect",
                "RegisterEvent minecraft:block",
                "RegisterEvent minecraft:entity_type",
                "RegisterEvent minecraft:item",
                "EntityAttributeCreationEvent alpha",
                "EntityAttributeCreationEvent beta",
                "EntityAttributeModificationEvent alpha",
                "EntityAttributeModificationEvent beta",
                "RegisterSpawnPlacementsEvent alpha",
                "RegisterSpawnPlacementsEvent beta",
                "BlockEntityTypeAddBlocksEvent alpha",
                "BlockEntityTypeAddBlocksEvent beta",
                "common_setup alpha",
                "common_setup beta",
                "dedicated_server_setup alpha",
                "dedicated_server_setup beta",
                "ExtendPoiTypesEvent alpha",
                "ExtendPoiTypesEvent beta",
                "inter_mod_enqueue alpha",
                "inter_mod_enqueue beta",
                "inter_mod_process alpha",
                "inter_mod_process beta got register from alpha to beta: Some(42)",
                "inter_mod_process beta",
                "load_complete alpha",
                "load_complete beta",
                "RegisterPayloadHandlersEvent",
            ]
        );
    }

    static CONFIG_CALLS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static LOCAL: OnceLock<Arc<ModConfig>> = OnceLock::new();
    static SYNCED: OnceLock<Arc<ModConfig>> = OnceLock::new();

    /// Registers a local and a synced config and records what `common_setup` reads from them.
    struct ConfigMod;

    fn flag_spec() -> ConfigSpec {
        let mut builder = ConfigSpec::builder();
        builder.define("flag", true);
        builder.build().unwrap()
    }

    impl NativeMod for ConfigMod {
        fn id(&self) -> &'static str {
            "configmod"
        }

        fn display_name(&self) -> &'static str {
            "Config mod"
        }

        fn version(&self) -> &'static str {
            "1.0"
        }

        fn init(&self, cx: &mut ModInit) {
            let local = cx.register_config(ConfigType::Local, flag_spec()).unwrap();
            let synced = cx.register_config(ConfigType::Synced, flag_spec()).unwrap();
            LOCAL.set(local).unwrap();
            SYNCED.set(synced).unwrap();
        }

        fn common_setup(&self) {
            let local = LOCAL.get().unwrap().get::<bool>("flag");
            let synced = SYNCED.get().unwrap().get::<bool>("flag");
            CONFIG_CALLS
                .lock()
                .unwrap()
                .push(format!("common_setup local {local:?} synced {synced:?}"));
        }

        fn load_complete(&self) {
            CONFIG_CALLS
                .lock()
                .unwrap()
                .push("load_complete".to_owned());
        }

        fn config_loading(&self, config: &ModConfig) {
            CONFIG_CALLS
                .lock()
                .unwrap()
                .push(format!("config_loading {}", config.config_type()));
        }
    }

    static CONFIG_MOD: ConfigMod = ConfigMod;

    /// `CommonModLoader.begin` loads `LOCAL` before `FMLCommonSetupEvent`, and
    /// `ServerLifecycleHooks.handleServerAboutToStart` loads `SYNCED` after `FMLLoadCompleteEvent`.
    #[test]
    fn local_configs_load_before_common_setup_and_synced_after_load_complete() {
        let dir = tempfile::tempdir().unwrap();
        let mut init = ModInit::new("configmod");
        init.config_dir = dir.path().join("config");
        init.owner = Some(&CONFIG_MOD);
        CONFIG_MOD.init(&mut init);
        let mod_list = mod_list(&["configmod"]);
        let mods = [LoadedMod {
            native_mod: &CONFIG_MOD,
            mod_id: "configmod",
        }];
        run(&mods, &mod_list, &init.configs, no_handler, || {
            CONFIG_CALLS
                .lock()
                .unwrap()
                .push("RegisterPayloadHandlersEvent".to_owned());
        })
        .unwrap();
        assert_eq!(
            *CONFIG_CALLS.lock().unwrap(),
            [
                "config_loading local",
                "common_setup local Ok(true) synced Err(NotLoaded(\"configmod-synced.toml\"))",
                "load_complete",
                "RegisterPayloadHandlersEvent",
                "config_loading synced",
            ]
        );
    }

    static SETUP_RAN: AtomicBool = AtomicBool::new(false);

    struct SetupMod;

    impl NativeMod for SetupMod {
        fn id(&self) -> &'static str {
            "setupmod"
        }

        fn display_name(&self) -> &'static str {
            "Setup mod"
        }

        fn version(&self) -> &'static str {
            "1.0"
        }

        fn init(&self, _cx: &mut ModInit) {}

        fn common_setup(&self) {
            SETUP_RAN.store(true, Ordering::Relaxed);
        }
    }

    static SETUP_MOD: SetupMod = SetupMod;

    /// `GameData.postRegisterEvents` posts every registry, then throws the collected errors before
    /// `FMLCommonSetupEvent`.
    #[test]
    fn a_failed_register_event_registration_stops_before_common_setup() {
        let mod_list = mod_list(&["setupmod"]);
        let mods = [LoadedMod {
            native_mod: &SETUP_MOD,
            mod_id: "setupmod",
        }];
        let mut fired = Vec::new();
        let error = run(
            &mods,
            &mod_list,
            &[],
            |registry| {
                fired.push(registry);
                let event = RegisterEvent::new(registry);
                // Only the fire of the block registry runs the registration.
                let result = event.register(ContentKind::Block, "setupmod:late", || {
                    Err(RegistryError::RegistryFrozen)
                });
                assert_eq!(result.is_err(), registry == ContentKind::Block);
                Some(event)
            },
            || {},
        )
        .unwrap_err();
        assert_eq!(fired, super::REGISTRIES);
        assert_eq!(
            error,
            "a RegisterEvent handler cannot register the block \"setupmod:late\": the content \
             registry is frozen"
        );
        assert!(!SETUP_RAN.load(Ordering::Relaxed));
    }

    static POI_NEXT_STEP_RAN: AtomicBool = AtomicBool::new(false);

    /// Adds a block to a POI type that does not exist.
    struct PoiMod;

    impl NativeMod for PoiMod {
        fn id(&self) -> &'static str {
            "poimod"
        }

        fn display_name(&self) -> &'static str {
            "POI mod"
        }

        fn version(&self) -> &'static str {
            "1.0"
        }

        fn init(&self, _cx: &mut ModInit) {}

        fn extend_poi_types(&self, event: &mut ExtendPoiTypesEvent) {
            // The handle keeps the error, so the mod can ignore the `Result`.
            let _ = event.add_block_to_poi("minecraft:alchemist", "minecraft:stone");
        }

        fn inter_mod_enqueue(&self, _imc: &mut InterModEnqueue) {
            POI_NEXT_STEP_RAN.store(true, Ordering::Relaxed);
        }
    }

    static POI_MOD: PoiMod = PoiMod;

    /// `ModLoader.postEvent` throws the exception of a listener, so the next step never runs.
    #[test]
    fn an_error_of_a_registration_event_stops_before_the_next_step() {
        let mod_list = mod_list(&["poimod"]);
        let mods = [LoadedMod {
            native_mod: &POI_MOD,
            mod_id: "poimod",
        }];
        let error = run(&mods, &mod_list, &[], no_handler, || {}).unwrap_err();
        assert_eq!(
            error,
            "the ExtendPoiTypesEvent of mod poimod: unknown POI type \"minecraft:alchemist\""
        );
        assert!(!POI_NEXT_STEP_RAN.load(Ordering::Relaxed));
    }
}
