# Porting the mod lifecycle from NeoForge

In NeoForge, FML builds the `@Mod` class of each mod when it loads the jar. The constructor gets the mod bus. The mod listens on that bus to `FMLCommonSetupEvent`, `InterModEnqueueEvent` and the other lifecycle events. `neoforge.mods.toml` gives the id, the version and the dependencies. FML sorts the mods with the `ordering` of their dependencies.

In IronPumpkin, a mod is a type that implements `NativeMod`. `NativeMod::init` replaces the constructor. Each lifecycle event is a `NativeMod` method with an empty default. The server calls each method on its own mod only, as each NeoForge mod has its own bus. An optional `neoforge.mods.toml` comes from `NativeMod::mods_toml()`. The server reads the metadata of all mods before it calls the first `init`.

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Entry point** | The constructor of the `@Mod` class | `NativeMod::init(&self, cx: &mut ModInit)` |
| **Mod bus** | One `IEventBus` for each mod | No object. Each lifecycle event is a `NativeMod` method. |
| **`FMLCommonSetupEvent`** | A listener on the mod bus | `NativeMod::common_setup` |
| **`FMLDedicatedServerSetupEvent`** | A listener on the mod bus | `NativeMod::dedicated_server_setup` |
| **`InterModEnqueueEvent`, `InterModProcessEvent`** | Listeners that call `InterModComms` | `NativeMod::inter_mod_enqueue` and `inter_mod_process` |
| **`FMLLoadCompleteEvent`** | A listener on the mod bus | `NativeMod::load_complete` |
| **`RegisterEvent`** | One event for each registry | A Pumpkin event, once for each of the block, entity type and item registries, after `init`. Content also registers at once in `init`. See [Registration](./registration.md). |
| **Client events** | `FMLClientSetupEvent` and the other client dist events | Never fire. The server has no client dist. |
| **Mod descriptor** | `neoforge.mods.toml` in the jar | `NativeMod::mods_toml()`: optional text, usually `include_str!` of the file |
| **Mod list** | `ModList.get()` | `ModList::get()` in `ironpumpkin_mods::mod_list` |
| **Load order** | The `ordering` of the dependencies. The discovery order breaks a tie. | The `ordering` of the dependencies. The mod id breaks a tie. |
| **Dependencies** | `type`, `versionRange` and `side` of `[[dependencies.<modId>]]` | The same keys. The server refuses to start on an unmet dependency. |
| **Dist** | `FMLEnvironment.getDist()` and `@Mod(dist = ..)` | `mod_list::dist()` is always `Dist::DedicatedServer` |
| **Server lifecycle events** | `ServerStartingEvent` and the others, on the game bus | Pumpkin events. See [Events](./events.md). |
| **`ServerAboutToStartEvent`** | Fires before the worlds load, on the game bus | A Pumpkin event with the same position. Only native mods receive it. |
| **Registry freeze** | Right after `RegisterEvent`, before `FMLCommonSetupEvent` | After `load_complete` and the Synced configs. See "The registry freeze". |
| **Config loading** | A Local config loads before common setup. A Synced config loads when the server is about to start. | A Local config loads after every `init` and before `common_setup`. A Synced config loads after `load_complete`. See [Configuration](./configuration.md). |

---

## Code Comparison: a mod with a setup step and a message

### 1. NeoForge implementation (Java)

The mod listens to the common setup and sends a message to `other_mod`. It reads its own messages in the process step.

```java
@Mod(HelloMod.MOD_ID)
public final class HelloMod {
    public static final String MOD_ID = "hello_mod";

    public HelloMod(IEventBus modBus) {
        modBus.addListener(this::commonSetup);
        modBus.addListener(this::enqueueMessages);
        modBus.addListener(this::processMessages);
        modBus.addListener(this::loadComplete);
    }

    private void commonSetup(FMLCommonSetupEvent event) {
        List<String> mods = ModList.get().getMods().stream().map(IModInfo::getModId).toList();
        LOGGER.info("[{}] common setup, mod list: {}", MOD_ID, String.join(", ", mods));
    }

    private void enqueueMessages(InterModEnqueueEvent event) {
        InterModComms.sendTo("other_mod", "register", () -> 42);
    }

    private void processMessages(InterModProcessEvent event) {
        InterModComms.getMessages(MOD_ID).forEach(message ->
                LOGGER.info("[{}] {} from {}", MOD_ID, message.method(), message.senderModId()));
    }

    private void loadComplete(FMLLoadCompleteEvent event) {
        LOGGER.info("[{}] other_mod loaded: {}", MOD_ID, ModList.get().isLoaded("other_mod"));
    }
}
```

File `neoforge.mods.toml`:

```toml
license = "GPL-3.0"

[[mods]]
modId = "hello_mod"
namespace = "hello-mod"
version = "${file.jarVersion}"
displayName = "Hello Mod"

[[dependencies.hello_mod]]
modId = "other_mod"
type = "optional"
versionRange = "[1.0,2.0)"
ordering = "AFTER"
side = "BOTH"
```

### 2. IronPumpkin implementation (Rust)

The snippet is a doc-test of `ironpumpkin-mods`. It keeps the text of `neoforge.mods.toml` in a constant. A mod crate writes `Some(include_str!("../neoforge.mods.toml"))` instead.

```rust
use ironpumpkin_mods::{
    ModInit, NativeMod,
    imc::{InterModEnqueue, InterModProcess},
    mod_list::ModList,
    register_mod,
    tracing::info,
};

const MOD_ID: &str = "hello_mod";
const MODS_TOML: &str = r#"
[[mods]]
modId = "hello_mod"
namespace = "hello-mod"
version = "${file.jarVersion}"
displayName = "Hello Mod"

[[dependencies.hello_mod]]
modId = "other_mod"
type = "optional"
versionRange = "[1.0,2.0)"
ordering = "AFTER"
side = "BOTH"
"#;

struct HelloMod;

impl NativeMod for HelloMod {
    fn id(&self) -> &'static str {
        MOD_ID
    }

    fn display_name(&self) -> &'static str {
        "Hello Mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn mods_toml(&self) -> Option<&'static str> {
        Some(MODS_TOML)
    }

    fn init(&self, _cx: &mut ModInit) {}

    fn common_setup(&self) {
        let mods: Vec<&str> = ModList::get()
            .get_mods()
            .iter()
            .map(|info| info.mod_id.as_str())
            .collect();
        info!("[{MOD_ID}] common setup, mod list: {}", mods.join(", "));
    }

    fn inter_mod_enqueue(&self, imc: &mut InterModEnqueue) {
        // `send` returns false and drops the message when `other_mod` is not loaded.
        imc.send("other_mod", "register", 42u32);
    }

    fn inter_mod_process(&self, imc: &mut InterModProcess) {
        for message in imc.get_messages() {
            info!(
                "[{MOD_ID}] {} from {}: {:?}",
                message.method(),
                message.sender_mod_id(),
                message.payload::<u32>()
            );
        }
    }

    fn load_complete(&self) {
        info!(
            "[{MOD_ID}] other_mod loaded: {}",
            ModList::get().is_loaded("other_mod")
        );
    }
}

register_mod!(HelloMod);
```

The mapping from the Java code to the Rust code:

| Java | Rust |
| :--- | :--- |
| `@Mod("hello_mod")` and the constructor | `NativeMod::id` returns `"hello_mod"`. `init` is the constructor. |
| `modBus.addListener(this::commonSetup)` | Override `NativeMod::common_setup`. |
| `ModList.get().getMods()` | `ModList::get().get_mods()`, a slice of `ModInfo` in load order |
| `ModList.get().isLoaded(id)` | `ModList::get().is_loaded(id)` |
| `InterModComms.sendTo(id, method, supplier)` | `imc.send(id, method, payload)`. The payload is any value. |
| `InterModComms.getMessages(MOD_ID)` | `imc.get_messages()`. It returns the messages of the mod itself. |
| `message.messageSupplier().get()` | `message.payload::<T>()`, an `Option<&T>` |
| `neoforge.mods.toml` in the jar | `NativeMod::mods_toml()` |

---

## The order of the steps

The server runs these steps once at startup. Each step calls the method of every mod, in load order, before the next step starts.

1. It reads the metadata of every mod and checks the ids and the dependencies. It stops the server on an error.
2. It calls `NativeMod::init` of each mod. A Startup config loads inside `register_config`, and its `config_loading` runs there.
3. It fires `RegisterEvent` for `minecraft:block`, `minecraft:entity_type` and `minecraft:item`, in that order. A registration error stops the server after the last fire.
4. It loads the Local configs and calls `config_loading` for each.
5. It calls `common_setup`.
6. It calls `dedicated_server_setup`.
7. It calls `inter_mod_enqueue`.
8. It calls `inter_mod_process`.
9. It calls `load_complete`.
10. It loads the Synced configs and calls `config_loading` for each.
11. It freezes the content registry.
12. It fires `ServerAboutToStartEvent`. Only native mods receive it. The datapack recipes and tags are not loaded yet.
13. It loads the worlds. The other server events, such as `ServerStartingEvent`, fire after this step.

The order of steps 2 to 9 is the order of `CommonModLoader.begin` and `CommonModLoader.load` in NeoForge 26.3. A mod has no `ModInit` after `init` returns, so it registers content, commands and handlers in `init`.

## The registry freeze

NeoForge freezes the registries right after `RegisterEvent`. IronPumpkin freezes the content registry after `load_complete` and the Synced configs. So a registration after `RegisterEvent` has a different result on the two servers. A `register_block` call from `common_setup` throws on NeoForge, and it succeeds here until the freeze. `ironpumpkin-mods` gives `common_setup` no `ModInit`, so only a direct call of a `pumpkin_core` registry function can make that registration. Do not rely on it: register in `init` or in a `RegisterEvent` handler.

## Load order

The load order is the order of `ModList::get_sorted_mods`. The server computes it in three rules:

- With `ordering = "AFTER"`, the mod loads after the dependency. With `BEFORE`, the mod loads before the dependency. `NONE` adds no rule.
- A dependency that is not loaded adds no rule.
- Among mods that no rule constrains, the mod id decides. FML uses the discovery order there, which a binary does not have.

A cycle of rules stops the server. The error names the mods of the cycle.

The mod list holds the system mods `minecraft` and `neoforge`, as on NeoForge. A mod can depend on them. Their versions are the Minecraft version of the server and the NeoForge version that the server emulates for clients.

## The `neoforge.mods.toml` of a native mod

`mods_toml()` is optional. With it, the `[[mods]]` entry decides the metadata of the mod. Without it, the mod is known by the methods of `NativeMod` and has no dependencies.

| Key | Use |
| :--- | :--- |
| `modId` | The mod id. `NativeMod::id` picks the `[[mods]]` entry with this `modId`, or the only entry. |
| `namespace` | The registry namespace. The default is the mod id. |
| `version` | The version. `${file.jarVersion}` becomes `NativeMod::version()`. The version starts with a digit. |
| `displayName`, `description`, `authors` | The display fields of `ModInfo` |
| `[[dependencies.<modId>]]` | The dependencies of the mod: `modId`, `type`, `versionRange`, `ordering`, `side` and `reason` |

The other keys (`modLoader`, `loaderVersion`, `mixins`, `accessTransformers` and the client display keys) are not read.

A dependency has one of four types. The `type` key is case-insensitive:

| Type | The server |
| :--- | :--- |
| `required` | Stops when the dependency is not loaded in its `versionRange` |
| `optional` | Stops only when the dependency is loaded outside its `versionRange` |
| `incompatible` | Stops when the dependency is loaded in its `versionRange` |
| `discouraged` | Logs a warning when the dependency is loaded in its `versionRange` |

A dependency with `side = "CLIENT"` is ignored on the server.

## The mod id

With a `neoforge.mods.toml`, the `modId` must match the FML 12.0.8 pattern `^(?=.{2,64}$)[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`. The `namespace` must match `^[a-z][a-z0-9_.-]{1,63}$`. A Java mod id has no `-`, so a registry namespace such as `hello-mod` goes in the `namespace` key.

Without a `neoforge.mods.toml`, the id is `NativeMod::id`. It is not empty and uses only `a-z`, `0-9`, `_`, `.` and `-`.

Two mods with the same id stop the server. A mod with the id `minecraft` or `neoforge` stops it too.

The mod id is the namespace of the commands and permissions of the mod. It names the data folder `plugins/data/<mod id>` and the config files. The registry namespace is the `namespace` key.

---

## Do and don't

**Do**

- Register content, commands, permissions, event handlers and configs in `NativeMod::init`.
- Use `common_setup` for work that needs the content of every mod. All `init` calls are done by then.
- Write a `neoforge.mods.toml` for a mod that depends on another mod. Use `ordering` when the load order matters.
- Use the mod id of the Java mod as the `modId`, and its registry namespace as the `namespace`.
- Call `ModList::get().is_loaded(..)` to find out if an optional mod is present.
- Send messages in `inter_mod_enqueue`. Read them in `inter_mod_process`.

**Don't**

- Do not call `ModList::get()` from a static initializer. The list is empty until the server sets it.
- Do not register content in `common_setup` or in a later step. Those methods have no `ModInit`.
- Do not read a Local config in `init`, and do not read a Synced config in a lifecycle step. A Local config is ready from `common_setup` on. A Synced config loads after `load_complete`. Use `config_loading` for the earliest access.
- Do not rely on the order of two mods that have no `ordering` rule. Only the mod id decides there.
- Do not port a client setup event. It never fires.
- Do not panic in a lifecycle method. The panic hook of the server writes a crash report and exits at once.
