# Porting a NeoForge mod to IronPumpkin

A NeoForge mod is a Java archive. The NeoForge loader reads it when the server starts. An IronPumpkin mod is a Rust crate. Cargo compiles it into the server binary of one modpack.

IronPumpkin takes the semantics and the catalogue of NeoForge and gives them a Rust shape. There is no Java, no mod bus object, no `DeferredRegister` and no mixin. This guide shows what replaces each of them. Some of the API has landed. The rest is planned. A planned item names its task in [`tasks.md`](../../openspec/changes/neoforge-shaped-plugin-api/tasks.md).

---

The table below contrasts a NeoForge 26.3 mod with an IronPumpkin native mod:

| Concept | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Language** | Java (JVM) | Rust |
| **Binary output** | `.jar` file in the `mods/` folder | One server binary per modpack. The mod crate is compiled into it. |
| **Mod descriptor** | `neoforge.mods.toml` and a `@Mod` class | The `NativeMod` trait (`id`, `display_name`, `version`, `client_required`) and `register_mod!`. An optional `neoforge.mods.toml` through `NativeMod::mods_toml()` is planned (task 2.2). |
| **Lifecycle hooks** | `FMLCommonSetupEvent` and the other FML events | `NativeMod::init`, called once before the first world loads. The other lifecycle events as `NativeMod` methods are planned (task 2.2). |
| **Event bus** | `IEventBus#addListener`, `@SubscribeEvent` | `ModInit::register_event` with a Pumpkin event handler. NeoForge event names, fire sites and fields are planned (tasks 2.3, 2.12, 2.13 and 2.15). |
| **Registration** | `DeferredRegister`, `DeferredHolder` | `ModInit::register_block`, `register_item`, `register_entity_type` and `register_tag` with builders. See [Registration](./registration.md). |
| **Config** | `ModConfigSpec` | Planned (task 2.8). Today a mod reads its own files in `ModInit::get_data_folder()` and sends a synced file with `ModInit::synced_config`. |
| **Network** | `PayloadRegistrar` | Payload registration is planned (task 2.11). The server already negotiates the NeoForge channels with the client. |
| **Mixins and reflection** | Mixin configs, access transformers, reflection | Source patches that the pack build applies. See [Mixins, reflection and access transformers](./mixins-and-patches.md). |
| **Build and distribution** | Gradle, a jar on a mod site | `cargo xtask build` and a GitHub Action. A release has one binary per platform. See [Setup and build](./setup-and-build.md). |
| **Client side** | The same jar runs on the client | A NeoForge client needs a Java mod with the same registry names. See [What is unsupported](./unsupported.md). |
| **Sandbox** | None | None for a native mod. Wasm plugins keep their sandbox. |
| **Licence** | The mod author chooses | A GPL-3.0-compatible licence. The `ironpumpkin-mods` API crate is MIT OR Apache-2.0. |

---

## Porting order

1. Create a pack from the template and build it once. See [Setup and build](./setup-and-build.md).
2. Port the content: blocks, items, entity types and tags. See [Registration](./registration.md).
3. Port each mixin, access transformer and reflection call. Use a NeoForge event or an `ironpumpkin-mods` function when one exists. Use a source patch when none exists. See [Mixins, reflection and access transformers](./mixins-and-patches.md).
4. Check the rest of the mod against [What is unsupported](./unsupported.md).
5. Keep the Java client half of the mod, with the same registry names, for NeoForge clients.

## Topic pages

- [Setup and build](./setup-and-build.md): from a Gradle project to a Rust crate in a pack, and from the template repository to a running binary.
- [Registration](./registration.md): `DeferredRegister` and `DeferredHolder` against the `ModInit::register_*` builders.
- [Mixins, reflection and access transformers](./mixins-and-patches.md): when a source patch is allowed, and how a pack applies it.
- [What is unsupported](./unsupported.md): what a mod cannot do today, and where to look.

## Planned pages

A page for an API that does not exist yet is written when its task lands.

| Page | NeoForge subject | Task |
| :--- | :--- | :--- |
| Events | `@SubscribeEvent`, `IEventBus`, event priority and cancel | 2.3, 2.12, 2.13, 2.15 |
| Lifecycle | `FMLCommonSetupEvent`, `neoforge.mods.toml`, `ModList` | 2.2 |
| Configuration | `ModConfigSpec`, config types Local, Synced and Startup | 2.8 |
| Network payloads | `PayloadRegistrar`, `RegisterPayloadHandlersEvent` | 2.11 |

## Look up a NeoForge class

[`mapping-table.md`](../../openspec/changes/neoforge-shaped-plugin-api/mapping-table.md) lists the NeoForge class or member and its counterpart in `ironpumpkin-mods` or Pumpkin. Its status column says `supported`, `planned` or `not supported`. The inventory in [`design.md`](../../openspec/changes/neoforge-shaped-plugin-api/design.md) is the contract: where the two files differ, `design.md` is right.

## Worked example

[`ironpumpkin-pack-test`](https://github.com/EdenNetworkItalia/ironpumpkin-pack-test) is a pack with one ported mod: WITS (What Is This Structure) by TelepathicGrunt. This guide refers to it where a page needs a real mod.
