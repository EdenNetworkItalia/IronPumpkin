//! The mod lifecycle after construction.
//!
//! The order is that of `NeoForge` 26.3 `CommonModLoader.load`: common setup, sided setup (`FMLDedicatedServerSetupEvent` on a
//! dedicated server), enqueue IMC, process IMC, load complete. Each step calls the method of
//! every mod in load order before the next step starts, and each method runs on its own mod
//! only, as each `NeoForge` mod has its own mod bus.
//!
//! The server lifecycle events (`ServerAboutToStartEvent`, `ServerStartingEvent` and the others)
//! are game bus events: a mod registers them as Pumpkin events with
//! [`ModInit::register_event`](crate::ModInit::register_event).

use crate::{
    LoadedMod,
    imc::{ImcQueue, InterModEnqueue, InterModProcess},
    mod_list::ModList,
};

/// Runs the lifecycle steps after the last [`NativeMod::init`](crate::NativeMod::init).
pub(crate) fn run(mods: &[LoadedMod<'_>], mod_list: &ModList) {
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
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::run;
    use crate::{
        LoadedMod, ModInit, NativeMod,
        imc::{InterModEnqueue, InterModProcess},
        mod_list::{ModList, system_mods},
        mods_toml::parse,
    };

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
        let mut infos = system_mods();
        for id in ["alpha", "beta"] {
            infos.push(parse(&format!("[[mods]]\nmodId = \"{id}\"\n"), id, "1").unwrap());
        }
        let mod_list = ModList::new(infos);
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
        run(&mods, &mod_list);
        assert_eq!(
            *CALLS.lock().unwrap(),
            [
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
}
