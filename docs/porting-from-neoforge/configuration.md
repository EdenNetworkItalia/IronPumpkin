# Porting mod configuration from NeoForge

In NeoForge, a mod builds a `ModConfigSpec` with `ModConfigSpec.Builder`. It keeps the typed values (`BooleanValue`, `IntValue`) in static fields. It registers the spec on its `ModContainer` with a `ModConfig.Type`. FML loads the file, corrects it against the spec and watches it for edits. FML fires `ModConfigEvent.Loading` and `ModConfigEvent.Reloading` on the mod bus. A `SYNCED` config also goes to the clients.

In IronPumpkin, a mod builds a `ConfigSpec` with a `ConfigSpecBuilder`. It registers the spec with `ModInit::register_config` in `NativeMod::init`. The call returns an `Arc<ModConfig>`. The mod reads a value with `ModConfig::get::<T>(path)`. There are no typed value handles. The file, the correction and the sync follow NeoForge. The server polls the file instead of watching it.

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Spec** | `ModConfigSpec` from `ModConfigSpec.Builder` | `ConfigSpec` from `ConfigSpec::builder()`. `build()` returns a `Result`. |
| **Values** | A typed handle: `ModConfigSpec.BooleanValue` and others | No handle. `ModConfig::get::<T>("section.key")` returns a `Result<T, ConfigError>`. |
| **Value types** | Boolean, int, long, double, string, enum, list | `bool`, `i32`, `i64`, `f64`, `String`, a `ConfigEnum` type, and `Vec` of these |
| **Registration** | `ModContainer#registerConfig(type, spec)` | `ModInit::register_config(type, spec)` in `init` |
| **Types** | `ModConfig.Type.LOCAL`, `SYNCED`, `STARTUP` and `CLIENT` | `ConfigType::Local`, `Synced` and `Startup`. `CLIENT` never loads on a server. |
| **File** | `config/<modid>-<type>.toml` | The same path. `register_config_with_file_name` sets another name. |
| **Load time** | Startup at registration. Local after the load complete step. Synced when the server is about to start. | Startup in `register_config`. Local and Synced after `load_complete`, before the server starts. |
| **Correction** | `ModConfigSpec.correct` on load | The same rules, and the same `.bak` backup of the old file |
| **Reload** | `ConfigWatcher` watches the file | The server checks the files every `mod_config_reload_ticks` ticks |
| **Events** | `ModConfigEvent.Loading` and `Reloading` on the mod bus | `NativeMod::config_loading` and `NativeMod::config_reloading` |
| **Sync** | `SYNCED` configs go to the clients as `neoforge:config_file` | The same payload, in the configuration phase |
| **Change a value** | `ConfigValue#set`, then `ModConfigSpec#save` | `ModConfig::set`, then `ModConfig::save` |

---

## Code Comparison: a synced config with two values

### 1. NeoForge implementation (Java)

```java
public final class HelloConfig {
    public static final ModConfigSpec SPEC;
    public static final ModConfigSpec.BooleanValue ENABLED;
    public static final ModConfigSpec.IntValue DAMAGE;

    static {
        ModConfigSpec.Builder builder = new ModConfigSpec.Builder();
        builder.comment("General settings").push("general");
        ENABLED = builder.comment("Whether the mod is on").define("enabled", true);
        DAMAGE = builder.defineInRange("damage", 10, 0, 100);
        builder.pop();
        SPEC = builder.build();
    }
}

@Mod(HelloMod.MOD_ID)
public final class HelloMod {
    public static final String MOD_ID = "hello_mod";

    public HelloMod(IEventBus modBus, ModContainer container) {
        container.registerConfig(ModConfig.Type.SYNCED, HelloConfig.SPEC);
        modBus.addListener(this::onLoad);
    }

    private void onLoad(ModConfigEvent.Loading event) {
        LOGGER.info("[{}] enabled = {}", MOD_ID, HelloConfig.ENABLED.get());
    }
}
```

The file is `config/hello_mod-synced.toml`.

### 2. IronPumpkin implementation (Rust)

The snippet is a doc-test of `ironpumpkin-mods`. The mod keeps the `Arc<ModConfig>` in a static, as the Java mod keeps its `SPEC`.

```rust
use std::sync::{Arc, OnceLock};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    config::{ConfigError, ConfigSpec, ConfigType, ModConfig},
    register_mod,
    tracing::{error, info},
};

const MOD_ID: &str = "hello_mod";

static CONFIG: OnceLock<Arc<ModConfig>> = OnceLock::new();

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

    fn init(&self, cx: &mut ModInit) {
        if let Err(err) = register_config(cx) {
            error!("[{MOD_ID}] cannot register the config: {err}");
        }
    }

    fn config_loading(&self, config: &ModConfig) {
        if let Ok(enabled) = config.get::<bool>("general.enabled") {
            info!("[{MOD_ID}] enabled = {enabled}");
        }
    }
}

fn register_config(cx: &mut ModInit) -> Result<(), ConfigError> {
    let mut builder = ConfigSpec::builder();
    builder.comment(" General settings").push("general");
    builder
        .comment(" Whether the mod is on")
        .define("enabled", true);
    builder.define_in_range("damage", 10i64, 0, 100);
    builder.pop();
    let config = cx.register_config(ConfigType::Synced, builder.build()?)?;
    let _ = CONFIG.set(config);
    Ok(())
}

/// The value of `general.damage`, or `None` before the config loads.
fn damage() -> Option<i64> {
    CONFIG.get()?.get("general.damage").ok()
}

/// Doubles the damage inside its range and writes the file.
fn double_damage() -> Result<(), ConfigError> {
    let Some(config) = CONFIG.get() else {
        return Ok(());
    };
    let damage: i64 = config.get("general.damage")?;
    config.set("general.damage", damage * 2)?;
    config.save()
}

register_mod!(HelloMod);
```

The mapping from the Java code to the Rust code:

| Java | Rust |
| :--- | :--- |
| `new ModConfigSpec.Builder()` | `ConfigSpec::builder()` |
| `builder.comment(..).push("general")` and `builder.pop()` | The same calls, on `&mut ConfigSpecBuilder` |
| `builder.define("enabled", true)` | `builder.define("enabled", true)` |
| `builder.define(path, default, validator)` | `builder.define_validated(path, default, validator)` |
| `builder.defineInRange("damage", 10, 0, 100)` | `builder.define_in_range("damage", 10i64, 0, 100)`. The type is `i32`, `i64` or `f64`. |
| `builder.defineEnum(path, default)` | `builder.define_enum(path, default)`. The type implements `ConfigEnum`. |
| `builder.defineList(path, default, validator)` | `builder.define_list(path, default, validator)` |
| `builder.worldRestart()` and `gameRestart()` | `builder.world_restart()` and `game_restart()` |
| `builder.translation(key)` | `builder.translation(key)` |
| `builder.build()` | `builder.build()?` |
| `container.registerConfig(type, spec)` | `cx.register_config(type, spec)?` |
| `ENABLED.get()` | `config.get::<bool>("general.enabled")?` |
| `ENABLED.set(false)` and `SPEC.save()` | `config.set("general.enabled", false)?` and `config.save()?` |
| `ModConfigEvent.Loading` | `NativeMod::config_loading` |
| `ModConfigEvent.Reloading` | `NativeMod::config_reloading` |

---

## Config types and load time

| Type | File | Loads | Clients |
| :--- | :--- | :--- | :--- |
| `Startup` | `config/<modid>-startup.toml` | In `register_config`, so `init` reads it before it registers content | Never get it. A value that changes content must agree with the client by other means. |
| `Local` | `config/<modid>-local.toml` | After `load_complete`, before the server starts | Never get it |
| `Synced` | `config/<modid>-synced.toml` | After `load_complete`, before the server starts | Get it in the configuration phase, as `neoforge:config_file` |

`ModConfig::get` returns `ConfigError::NotLoaded` for a Local or Synced config until it loads. The hook `config_loading` runs for every config of every mod, after `load_complete`. A Startup config loads earlier, but its hook runs with the others.

The file name uses the mod id. With a `neoforge.mods.toml`, that is the `modId` of the file. A client applies a synced file only when its own copy of the mod registers a config with the same file name.

A second config with the same file name fails with `ConfigError::Duplicate`.

## Correction

The server reads the file on load and checks each value against the spec:

- A missing value gets its default.
- A number out of its range gets the nearest bound.
- A value that fails its kind, allowed values or validator gets its default.
- A list keeps the elements that pass. It falls back to its default when none pass.
- A key that the spec does not define is removed.
- A file that is not valid TOML is recreated with the defaults.

When the corrected text differs from the file, the server copies the old file to `<name>-1.toml.bak`. It then writes the corrected text with the comments of the spec. It keeps five backups.

## Reload and the hooks

The server checks the config files every `mod_config_reload_ticks` ticks. The key is in `pumpkin.toml`. The default is 100. The value 0 turns the check off. A changed file reloads, and the server calls `NativeMod::config_reloading`.

`ModConfig::save` writes the values to the file and calls `config_reloading` too, as `ModConfigSpec#save` does.

A client that connects after a reload gets the new text of a synced file. The server sends the file in the configuration phase of each connection.

## Restart flags

A value with `world_restart` or `game_restart` keeps the value that it loaded with. `get` returns that value until the server restarts. The file changes at once. A dedicated server closes its world only when it stops, so `world_restart` acts like `game_restart`. A synced config cannot have a `game_restart` value: `register_config` fails with `ConfigError::InvalidSpec`.

## Errors

| Error | Cause |
| :--- | :--- |
| `UnknownPath` | The spec has no value with this path. |
| `InvalidValue` | A `set` value fails the spec, or `get::<T>` asks for another type. |
| `InvalidSpec` | The builder has an empty or double path, a dangling comment or flag, or a default that fails its own validator. |
| `Duplicate` | Another config uses the file. |
| `NotLoaded` | The Local or Synced config has not loaded yet. |
| `Io` | The server cannot read or write the file. |

A config error never stops the server, except a Local or Synced config that cannot load. That error stops the startup. Log the other errors and continue.

---

## Do and don't

**Do**

- Build the spec and register it in `NativeMod::init`.
- Keep the `Arc<ModConfig>` that `register_config` returns.
- Write the path of a value as the sections and the key joined with `.`, for example `"general.damage"`.
- Read a Local or Synced value after it loads, for example in `config_loading` or in an event handler.
- Use a Startup config for a value that `init` needs before it registers content.
- Handle the `Result` of `get` and `set`.

**Don't**

- Do not read a Local or Synced config in `init`. It has not loaded, and `get` returns `NotLoaded`.
- Do not change a Startup value and expect clients to follow. Clients never get a Startup config.
- Do not use `game_restart` in a synced config.
- Do not edit the file and expect an instant reload. The server checks every `mod_config_reload_ticks` ticks.
- Do not register two configs with one file name.
- Do not unwrap a `ConfigError`. The panic hook of the server writes a crash report and exits at once.
