//! The mod lifecycle after construction.
//!
//! The order is that of `NeoForge` 26.3 on a dedicated server. `CommonModLoader.begin` fires
//! `RegisterEvent` once per registry (`GameData.postRegisterEvents`) and loads the local configs,
//! then `CommonModLoader.load` runs common setup, sided setup
//! (`FMLDedicatedServerSetupEvent` on a dedicated server), enqueue IMC, process IMC and load
//! complete, and `ServerLifecycleHooks.handleServerAboutToStart` loads the synced configs. Each
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
    LoadedMod,
    config::{self, ConfigType, ModConfig},
    content::ContentKind,
    event::registries::RegisterEvent,
    imc::{ImcQueue, InterModEnqueue, InterModProcess},
    mod_list::ModList,
};

/// The registries that `RegisterEvent` fires for, in the order of
/// `GameData.getRegistrationOrder`: the vanilla registration order puts `minecraft:block` before
/// `minecraft:entity_type` and `minecraft:item`.
pub(crate) const REGISTRIES: [ContentKind; 3] = [
    ContentKind::Block,
    ContentKind::EntityType,
    ContentKind::Item,
];

/// Runs the steps after the last [`NativeMod::init`](crate::NativeMod::init): `RegisterEvent`
/// for each of [`REGISTRIES`] through `fire_register`, which returns the event after its
/// handlers or `None` when no handler listens, then the lifecycle steps, with the local configs
/// loaded before them and the synced configs after them.
///
/// # Errors
///
/// Returns the reason when a `RegisterEvent` handler fails to register content, after the last
/// fire, or when a config cannot load.
pub(crate) fn run(
    mods: &[LoadedMod<'_>],
    mod_list: &ModList,
    configs: &[Arc<ModConfig>],
    mut fire_register: impl FnMut(ContentKind) -> Option<RegisterEvent>,
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
    config::load(configs, ConfigType::Local)?;
    for loaded in mods {
        loaded.native_mod.common_setup();
    }
    for loaded in mods {
        loaded.native_mod.dedicated_server_setup();
    }
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
    config::load(configs, ConfigType::Synced)
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
        run(&mods, &mod_list, &[], |registry| {
            let event = RegisterEvent::new(registry);
            record(format!("RegisterEvent {}", event.registry_key()));
            Some(event)
        })
        .unwrap();
        assert_eq!(
            *CALLS.lock().unwrap(),
            [
                "RegisterEvent minecraft:block",
                "RegisterEvent minecraft:entity_type",
                "RegisterEvent minecraft:item",
                "common_setup alpha",
                "common_setup beta",
                "dedicated_server_setup alpha",
                "dedicated_server_setup beta",
                "inter_mod_enqueue alpha",
                "inter_mod_enqueue beta",
                "inter_mod_process alpha",
                "inter_mod_process beta got register from alpha to beta: Some(42)",
                "inter_mod_process beta",
                "load_complete alpha",
                "load_complete beta",
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
        run(&mods, &mod_list, &init.configs, no_handler).unwrap();
        assert_eq!(
            *CONFIG_CALLS.lock().unwrap(),
            [
                "config_loading local",
                "common_setup local Ok(true) synced Err(NotLoaded(\"configmod-synced.toml\"))",
                "load_complete",
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
        let error = run(&mods, &mod_list, &[], |registry| {
            fired.push(registry);
            let event = RegisterEvent::new(registry);
            // Only the fire of the block registry runs the registration.
            let result = event.register(ContentKind::Block, "setupmod:late", || {
                Err(RegistryError::RegistryFrozen)
            });
            assert_eq!(result.is_err(), registry == ContentKind::Block);
            Some(event)
        })
        .unwrap_err();
        assert_eq!(
            fired,
            [
                ContentKind::Block,
                ContentKind::EntityType,
                ContentKind::Item
            ]
        );
        assert_eq!(
            error,
            "a RegisterEvent handler cannot register the block \"setupmod:late\": the content \
             registry is frozen"
        );
        assert!(!SETUP_RAN.load(Ordering::Relaxed));
    }
}
