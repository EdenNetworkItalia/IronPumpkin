# What a NeoForge mod cannot do on IronPumpkin

IronPumpkin runs the server side of a mod, as Rust code in the server process. It does not run Java, it has no client code, and it has the parts of Minecraft that Pumpkin has. A mod that needs more must wait for a task of the plan, use a source patch, or leave that part out.

The inventory in `design.md` of the change folder has one row for each element of the NeoForge 26.3.x API. Each row has a status and a reason. `mapping-table.md` in the same folder gives the counterpart of each row. Read both before you port a class. The [index](./index.md) names the folder.

---

## Key Differences

| Subject | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Java code** | Runs in the JVM | Does not run. Port the logic to Rust. |
| **Client code** | Renderers, screens, models, tooltips and key bindings run in the same jar | Not in the pack. A NeoForge client uses a Java mod with the same registry names. |
| **Mod loading** | The loader scans `mods/` and loads each jar | No loading. Cargo compiles the mod into the binary. A change needs a new build. |
| **Mixins and access transformers** | Mixin configs and `accesstransformer.cfg` | Source patches. See [Mixins, reflection and access transformers](./mixins-and-patches.md). |
| **Registries** | Any registry, with a `DeferredRegister` | Block, item, entity type and tag. Sound events, mob effects, data components and attributes are planned (task 2.6). The rest waits for M5 or is not supported. |
| **Events** | The NeoForge event catalogue | Pumpkin events. The server, tick, chat, statistic, anvil, grindstone, brewing, enchanting and player events have NeoForge fire sites and fields. See [Events](./events.md). Five player rows are planned, as listed below. The level and entity families are planned (tasks 2.13 and 2.15). |
| **Data generation and game tests** | `runData`, `RegisterGameTestsEvent` | Not supported |
| **Mod version records of a world** | `ModMismatchEvent` | Not supported. Pumpkin does not keep them. |
| **Creative tabs** | `creative_mode_tab` registry, `BuildCreativeModeTabContentsEvent` | The server never reads them. `ModInit::register_creative_mode_tab` accepts a tab and ignores it. |

---

## The three kinds of gap

**Planned, with a task.** The API has a task in `tasks.md` of the change. The page of this guide for that API is written when the task lands. These are the tasks for the families that mods use most:

- 2.12: the player rows that stay planned, listed below.
- 2.13 and 2.15: the level and block, and the entity families of events.
- 2.25: the other registration events of NeoForge and the behaviour by block type.
- 2.6: sound events, mob effects, data components and attributes.
- 2.7: capabilities.
- 2.9: menus.
- 2.10: shapes.
- 2.11: network payloads.
- 2.14: attachments.
- 2.16: loot modifiers.
- 2.17: data maps.

The player events landed with task 2.12. These player rows stay planned, each for its own reason:

- `PlayerEvent.Clone`: Pumpkin keeps the same `Player` across a respawn, so there is no old player to copy from. The copy on death belongs to attachments (task 2.14).
- `PermissionsChangedEvent`: the op and deop commands change the op list before the permission level of the player changes. A cancel needs a check in each caller before the list changes.
- `CanContinueSleepingEvent`: Pumpkin has no check that wakes a sleeping player, so the event has no decision to change.
- `PlayerNegotiationEvent`: it delays a login until async work ends. The configuration tasks are a fixed list, so no hook can add work to a login. Task 2.11 makes them dynamic.
- `AdvancementEvent.AdvancementProgressEvent`: Pumpkin awards and revokes a criterion under the advancement lock and passes no criterion name outside it. The event needs a step after the lock for each grant and revoke.
- The abstract classes, such as `PlayerEvent`: they have no struct, because the NeoForge event bus rejects a listener on an abstract event. Register a handler for a subclass.

**Planned for M5.** Many rows need an extension point that Pumpkin lacks. M5, Modded gameplay parity, adds it. The inventory marks these rows `planned` with the reason prefix `M5 engine extension:`. They cover registries that have no registration API, world generation types and hooks, and the datapack loader step for `neoforge:conditions`. They cover fixed sets of types too: enchantment effects, loot functions, recipe types, ingredient types, command argument types and attributes. The last group is cauldron interactions, item abilities, fluids, chunk tickets, farmland water sources and default components.

**Not supported.** A row stays `not supported` when it is one of these:

- Java loading: Mixin configs, access transformers, jar and mod file scanning, Java file helpers and `ServerModLoader`.
- Java tools: game tests, data generation, the settings of NeoForge itself, the `/neoforge` command tree and JUnit.
- Client only or display only: tooltips, pack listings and dialog codecs.
- A vanilla feature that Pumpkin lacks and no extension point would add: the phantom spawner, village sieges and the custom spawner list.
- A design difference: Pumpkin has one permission manager. It keeps no mod version record for a world. It has no event for the empty clicks that only the client knows.

---

## Code Comparison: the client half of a mod

### 1. NeoForge implementation (Java)

A NeoForge mod puts the client code in a class with `dist = Dist.CLIENT`. The same jar holds the server code.

```java
@Mod(value = HelloMod.MOD_ID, dist = Dist.CLIENT)
public final class HelloModClient {
    public HelloModClient(IEventBus modBus) {
        modBus.addListener(HelloModClient::registerRenderers);
    }

    private static void registerRenderers(EntityRenderersEvent.RegisterRenderers event) {
        event.registerEntityRenderer(HelloMod.GREETER.get(), PigRenderer::new);
    }
}
```

### 2. IronPumpkin implementation (Rust)

The pack has no client code. The Rust crate is the server half. The Java mod above stays as the client half and registers the same names. `examples/modpack/client-mod` is a complete client half for `hello-mod`.

`NativeMod::client_required` tells the server whether a client needs the mod. The default is `true`: the server disconnects a client that cannot have the mod, at the configuration handshake. A mod with only server logic returns `false`.

```rust
use ironpumpkin_mods::{ModInit, NativeMod, register_mod};

const MODS_TOML: &str = r#"
[[mods]]
modId = "hello_mod"
namespace = "hello-mod"
displayName = "Hello Mod"
"#;

struct HelloMod;

impl NativeMod for HelloMod {
    fn id(&self) -> &'static str {
        "hello_mod"
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

    // The mod registers content, so a client needs the Java half. This is the default.
    fn client_required(&self) -> bool {
        true
    }

    fn init(&self, _cx: &mut ModInit) {}
}

register_mod!(HelloMod);
```

A NeoForge client without the Java half is disconnected by its own registry check. The mod id of the Java half must match the FML 12.0.8 pattern `^(?=.{2,64}$)[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`, so `hello-mod` becomes `hello_mod`. The `namespace` key of the `neoforge.mods.toml` of the Rust mod keeps the registry namespace `hello-mod`, so the registry names match the server.

### A registry without a function

A NeoForge mod that registers a recipe type has no IronPumpkin counterpart today:

```java
public static final DeferredRegister<RecipeType<?>> RECIPE_TYPES =
        DeferredRegister.create(Registries.RECIPE_TYPE, MOD_ID);
```

`ModInit` has a register function only for a registry that a phase backs. There is no function for recipe types, and no way to fake one. The inventory row of `recipe_type` says why.

---

## Do and don't

**Do**

- Look up each class of the mod in `mapping-table.md` before you port it. Read the status and the task id.
- Split the mod in two: a Rust server half and a Java client half. Keep the registry names equal.
- Set `client_required` to `false` for a mod that has only server logic.
- Read the reason column of an inventory row. A `planned` row with the reason prefix `M5 engine extension:` waits for M5. A `not supported` row has no plan.
- Open an issue in `EdenNetworkItalia/IronPumpkin` for a gap that blocks a port. An accepted patch is a candidate for an event.

**Don't**

- Do not move renderers, screens, models, tooltips or key bindings into the pack. The server never runs them.
- Do not expect the server to load a Java mod jar. It cannot.
- Do not treat `planned` as available. Check the task id and whether the task has landed.
- Do not hot-load a native mod. Build the pack again. Wasm plugins are the hot-reload path for administration and integrations.
