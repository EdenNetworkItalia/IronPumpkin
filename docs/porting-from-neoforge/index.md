# Porting a NeoForge mod to IronPumpkin

A NeoForge mod is a Java archive. The NeoForge loader reads it when the server starts. An IronPumpkin mod is a Rust crate. Cargo compiles it into the server binary of one modpack.

IronPumpkin takes the semantics and the catalogue of NeoForge and gives them a Rust shape. There is no Java, no mod bus object, no `DeferredRegister` and no mixin. This guide shows what replaces each of them. Some of the API has landed. The rest is planned. A planned item names its task in `tasks.md` of the change folder (see [Look up a NeoForge class](#look-up-a-neoforge-class)).

---

The table below contrasts a NeoForge 26.3 mod with an IronPumpkin native mod:

| Concept | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Language** | Java (JVM) | Rust |
| **Binary output** | `.jar` file in the `mods/` folder | One server binary per modpack. The mod crate is compiled into it. |
| **Mod descriptor** | `neoforge.mods.toml` and a `@Mod` class | The `NativeMod` trait (`id`, `display_name`, `version`, `client_required`) and `register_mod!`. An optional `neoforge.mods.toml` through `NativeMod::mods_toml()` gives the mod id, the namespace and the dependencies. See [Lifecycle](./lifecycle.md). |
| **Lifecycle hooks** | `FMLCommonSetupEvent` and the other FML events | `NativeMod::init`, then `common_setup`, `dedicated_server_setup`, `inter_mod_enqueue`, `inter_mod_process` and `load_complete`, in load order. See [Lifecycle](./lifecycle.md). |
| **Event bus** | `IEventBus#addListener`, `@SubscribeEvent` | `ModInit::register_event` with a Pumpkin event handler, a priority, and the blocking and receive-canceled flags. The server, tick, chat, statistic, anvil, grindstone, brewing, enchanting, player, level and block events have NeoForge fire sites and fields. Some player and level rows and the abstract classes stay planned, and the entity family is planned (task 2.15). See [Events](./events.md) and [What is unsupported](./unsupported.md). |
| **Registration** | `DeferredRegister`, `DeferredHolder` | `ModInit::register_block`, `register_item`, `register_entity_type` and `register_tag` with builders. See [Registration](./registration.md). |
| **Config** | `ModConfigSpec` | `ModInit::register_config` with a `ConfigSpec` and a `ConfigType`: `Local`, `Synced` or `Startup`. The file is `config/<modid>-<type>.toml`. A `Synced` config goes to NeoForge clients as `neoforge:config_file`. See [Configuration](./configuration.md). |
| **Network** | `PayloadRegistrar`, `PacketDistributor`, `RegisterConfigurationTasksEvent` | `RegisterPayloadHandlersEvent` and `RegisterConfigurationTasksEvent` as Pumpkin events, a `PayloadRegistrar` per channel id with raw bytes, and `network::send_to_player` and the broadcasts. The server negotiates the channels with a NeoForge client. See [Network payloads](./network.md). |
| **Mixins and reflection** | Mixin configs, access transformers, reflection | Source patches that the pack build applies. See [Mixins, reflection and access transformers](./mixins-and-patches.md). |
| **Build and distribution** | Gradle, a jar on a mod site | `cargo xtask build` and a GitHub Action. A release has one binary per platform. See [Setup and build](./setup-and-build.md). |
| **Client side** | The same jar runs on the client | A NeoForge client needs a Java mod with the same registry names. See [What is unsupported](./unsupported.md). |
| **Sandbox** | None | None for a native mod. Wasm plugins keep their sandbox. |
| **Licence** | The mod author chooses | A GPL-3.0-compatible licence. The `ironpumpkin-mods` API crate is MIT OR Apache-2.0. |

---

## Porting order

1. Create a pack from the template and build it once. See [Setup and build](./setup-and-build.md).
2. Port the content: blocks, items, entity types and tags. See [Registration](./registration.md).
3. Port the lifecycle, the config, the event listeners and the network payloads. See [Lifecycle](./lifecycle.md), [Configuration](./configuration.md), [Events](./events.md) and [Network payloads](./network.md).
4. Port each mixin, access transformer and reflection call. Use a NeoForge event or an `ironpumpkin-mods` function when one exists. Use a source patch when none exists. See [Mixins, reflection and access transformers](./mixins-and-patches.md).
5. Check the rest of the mod against [What is unsupported](./unsupported.md).
6. Keep the Java client half of the mod, with the same registry names, for NeoForge clients.

## Topic pages

- [Setup and build](./setup-and-build.md): from a Gradle project to a Rust crate in a pack, and from the template repository to a running binary.
- [Registration](./registration.md): `DeferredRegister` and `DeferredHolder` against the `ModInit::register_*` builders.
- [Lifecycle](./lifecycle.md): `@Mod`, the FML lifecycle events, `neoforge.mods.toml` and `ModList` against `NativeMod`.
- [Configuration](./configuration.md): `ModConfigSpec` and the config types against `ConfigSpec` and `ConfigType`.
- [Events](./events.md): `@SubscribeEvent` and `IEventBus` against `ModInit::register_event`.
- [Network payloads](./network.md): `PayloadRegistrar`, `IPayloadContext`, `PacketDistributor` and the configuration tasks against the `network` module.
- [Mixins, reflection and access transformers](./mixins-and-patches.md): when a source patch is allowed, and how a pack applies it.
- [What is unsupported](./unsupported.md): what a mod cannot do today, and where to look.

## Planned pages

A page for an API that does not exist yet is written when its task lands.

| Page | NeoForge subject | Task |
| :--- | :--- | :--- |
| Entity events | `EntityJoinLevelEvent`, the living and item entity events | 2.15 |

## Look up a NeoForge class

The folder [`openspec/changes/neoforge-shaped-plugin-api/`](../../openspec/changes/neoforge-shaped-plugin-api/) holds the contract of the API. When the change is archived, the folder moves to `openspec/changes/archive/`. The files are:

- `mapping-table.md` lists the NeoForge class or member and its counterpart in `ironpumpkin-mods` or Pumpkin. Its status column says `supported`, `planned` or `not supported`.
- `design.md` has the inventory that is the contract. Where the two files differ, `design.md` is right.
- `tasks.md` names the task of each planned item.

## Worked example

[`ironpumpkin-pack-test`](https://github.com/EdenNetworkItalia/ironpumpkin-pack-test) is a pack with one ported mod: WITS (What Is This Structure) by TelepathicGrunt. This guide refers to it where a page needs a real mod.
