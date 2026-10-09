<div align="center">

# IronPumpkin

[![CI](https://github.com/EdenNetworkItalia/IronPumpkin/actions/workflows/rust.yml/badge.svg)](https://github.com/EdenNetworkItalia/IronPumpkin/actions/workflows/rust.yml)
[![License: GPL](https://img.shields.io/badge/License-GPLv3-yellow.svg)](https://opensource.org/licenses/gpl-3-0)

</div>

IronPumpkin is a Minecraft server maintained by [EdenNetwork Italia](https://github.com/EdenNetworkItalia).
It is a derivative of [Pumpkin](https://github.com/Pumpkin-MC/Pumpkin), a Minecraft server written in Rust.
The goal is a server that NeoForge clients can join and that runs NeoForge mods ported to Rust.

Bedrock clients are not a target. IronPumpkin targets NeoForge clients. The Bedrock code from
Pumpkin stays in the tree to keep upstream merges cheap, but IronPumpkin does not test it and does
not keep it working with custom content. This may change later.

> [!IMPORTANT]
> IronPumpkin is at an early stage. The roadmap below is planned work.

## How mods work

A ported mod is a Rust crate. A modpack is a Cargo workspace that depends on IronPumpkin and links
one crate per mod; the build produces one server binary with the mods baked in. There is no Java,
no JVM and no mod loading at runtime: a mod runs in-process, like it did on NeoForge, at native
speed.

A mod gets two primitives, and nothing else:

- **The NeoForge-shaped API.** The planned `ironpumpkin-neo` crate gives mods the shape a NeoForge
  mod sees, with the NeoForge names: event bus, deferred registers, capabilities, attachments, mod
  config, network payloads, loot modifiers, data maps and `neoforge.mods.toml` metadata. A port
  keeps the structure of the Java mod and translates it to Rust class by class. A hook point for a
  mod is a NeoForge event: there are no IronPumpkin-only hooks.
- **Source patches.** Mixins, reflection and access transformers have no bytecode to patch in a
  Rust server. What a mod did with them, and no NeoForge event does, becomes a patch to the
  IronPumpkin source. The mod crate ships `patches/<name>.patch` with a justification file
  `patches/<name>.md` and names the IronPumpkin commit in `[package.metadata.ironpumpkin] commit`.
  The modpack build applies the patches of every mod with `git apply` (no fuzz, no three-way
  merge), in the byte order of the `modpack.toml` keys, before it compiles. A patch for another
  commit fails the build unless `modpack.toml` sets `allow-drift = true`; a conflict between two
  mods, or a patch that touches generated files or `Cargo.lock`, always fails it.

A patch is accepted in a mod only with a justification file next to it that says what the patch
does, why a NeoForge event or API is not enough, and that the patch is bound to the IronPumpkin
commit it names. Every accepted patch is a candidate NeoForge-shaped event for a later version, so
the API grows from real patches.

The modpack blueprint in `blueprint/` has a GitHub Action that applies the patches and builds the
binaries for each pack, so operators and players download a binary and never compile. Wasm plugins stay
what Pumpkin offers today: sandboxed, hot-reloadable, for administration and integrations.

A mod linked into the server binary and a source patch are derivative works of a GPLv3 program, and
a mod must use a GPLv3-compatible licence. The planned `ironpumpkin-neo` API crate is MIT OR
Apache-2.0.

## Relationship with Pumpkin

IronPumpkin builds on the work of the [Pumpkin project](https://pumpkinmc.org/) and its
[authors and contributors](https://github.com/Pumpkin-MC/Pumpkin/graphs/contributors).
Pumpkin is licensed under the GPLv3, and so is IronPumpkin.

- IronPumpkin is not a GitHub fork. It is a derivative that merges upstream changes regularly.
- IronPumpkin tracks the `master` branch of Pumpkin.
- Gameplay fixes and general improvements go to Pumpkin, not to this repository.
- Only the NeoForge compatibility layer lives here.

## Roadmap

Work is tracked as [milestones](https://github.com/EdenNetworkItalia/IronPumpkin/milestones) and issues.

- [x] **NeoForge-aware networking.** The server recognises NeoForge clients, negotiates channels and
  exposes the configuration phase to plugins.
- [ ] **M1: Native mod channel.** The server as a library crate, mod registration, the modpack
  blueprint with its GitHub Action, and source patches that the pack build applies.
- [ ] **M2: Content registry.** Native mods add blocks, items and entity types that survive a world
  round trip.
- [ ] **M3: NeoForge handshake and registry sync.** A NeoForge client with a test mod joins and sees
  its content.
- [ ] **M4: NeoForge-shaped native API.** The `ironpumpkin-neo` crate, family by family, sized by a scan
  of a large reference modpack.
- [ ] **M5: Modded gameplay parity.** Block entities and menus, attachments, data maps, dimensions.

## About Pumpkin

[Pumpkin](https://pumpkinmc.org/) is a Minecraft server built entirely in Rust, offering a fast, efficient,
and customizable experience. It prioritizes performance and player enjoyment while adhering to the core mechanics of the game.
<div align="center">

![Pumpkin Chunk Loading](./assets/pumpkin-chunk-loading.webp)

</div>

### Goals

- **Performance**: Uses multi-threading for speed and efficiency.
- **Compatibility**: Supports the latest Java & Bedrock Minecraft server version while adhering to Vanilla game mechanics.
- **Security**: Prioritizes security by preventing known security exploits.
- **Flexibility**: Highly configurable, with the ability to disable unnecessary features.
- **Extensibility**: Provides a foundation for plugin development.

> [!NOTE]
> Pumpkin is under heavy development.
>
> [See what needs to be done before the Pumpkin 1.0.0 Release](https://github.com/Pumpkin-MC/Pumpkin/issues/449)

### Features

- [x] Configuration (toml)
- [Tracking: Protocol](https://github.com/Pumpkin-MC/Pumpkin/issues/1401)
  - [x] Server Status/Ping
  - [x] Encryption
  - [x] Packet Compression
  - [x] Java Edition
  - [x] Bedrock Edition (W.I.P)
  - ...
- [Tracking: World](https://github.com/Pumpkin-MC/Pumpkin/issues/1403)
  - [x] Player Tab-list
  - [x] Scoreboard
  - [x] World Loading
  - [x] World Time
  - [x] World Borders
  - [x] World Saving
  - [x] Lighting
  - [x] Entity Spawning
  - [x] Bossbar
  - [x] Chunk Loading (Vanilla, Linear, Pump)
  - [Chunk Generation](https://github.com/Pumpkin-MC/Pumpkin/issues/36)
  - [x] Chunk Saving (Vanilla, Linear, Pump)
  - [Redstone](https://github.com/Pumpkin-MC/Pumpkin/issues/1402)
  - [x] Liquid Physics
  - ...
- [Tracking: Player](https://github.com/Pumpkin-MC/Pumpkin/issues/1405)
  - [x] Skins
  - [x] Teleport
  - [x] Movement
  - [x] Animation
  - [x] Inventory
  - [Combat](https://github.com/Pumpkin-MC/Pumpkin/issues/1404)
  - [x] Experience
  - [x] Hunger
  - [X] Off Hand
  - [X] Advancements (W.I.P)
  - [x] Eating
  - ...
- Entities
  - [x] Non-Living (Minecart, Eggs...) (W.I.P)
  - [x] Entity Effects
  - [x] Players
  - [x] Mobs (W.I.P)
  - [x] Animals (W.I.P)
  - [Entity AI](https://github.com/Pumpkin-MC/Pumpkin/issues/1406)
  - [x] Boss (W.I.P)
  - [x] Villagers (W.I.P)
  - [X] Entity Saving
- Server
  - [Plugins](https://github.com/Pumpkin-MC/Pumpkin/issues/1407)
  - [x] Query
  - [x] [Minecraft Server Management Protocol (MSMP)](https://minecraft.wiki/w/Minecraft_Server_Management_Protocol)
  - [x] Inventories
  - [x] Particles
  - [x] Chat
  - [Commands](https://github.com/Pumpkin-MC/Pumpkin/issues/15)
  - [x] Permissions
  - [x] Translations
- Proxy
  - [x] [BungeeCord](https://github.com/SpigotMC/BungeeCord)
  - [x] [BungeeGuard](https://github.com/lucko/BungeeGuard)
  - [x] [Velocity](https://github.com/PaperMC/Velocity)

## How to run

Without mods, IronPumpkin runs like Pumpkin: see the [Quick Start](https://docs.pumpkinmc.org/#quick-start) guide.
With mods, create a modpack repository from the
[ironpumpkin-pack-template](https://github.com/EdenNetworkItalia/ironpumpkin-pack-template) template
repository and build the server there: see [blueprint/README.md](blueprint/README.md).

## Contributions

Contributions are welcome. Open issues and pull requests in this repository. See [CONTRIBUTING.md](CONTRIBUTING.md).

CI does not run on every push. It runs on pull requests and every night on `master`.
To start it on demand, run `gh workflow run rust.yml --ref <branch>`.

A fix that is relevant to Pumpkin goes to [Pumpkin](https://github.com/Pumpkin-MC/Pumpkin) first.
It reaches IronPumpkin when we merge upstream.

## Docs

Pumpkin's documentation is at <https://pumpkinmc.org/>.

## Upstream

For Pumpkin itself, join the [Pumpkin community](https://discord.gg/wT8XjrjKkf).

## License & Attribution

* **Server**: Licensed under the [GNU General Public License v3.0 (GPLv3)](LICENSE), the same license as Pumpkin.
* **API crates (`ironpumpkin-mods`, `ironpumpkin-neo` (planned), `pumpkin-plugin-api`, `pumpkin-plugin-wit` & `pumpkin-plugin-utils`)**: Dual-licensed under MIT OR Apache-2.0, so mod and plugin authors can choose either license. A mod linked into the server binary is still bound by the GPLv3 of the server.
  See [LICENSE-MIT](crates/pumpkin-plugin-api/LICENSE-MIT) and [LICENSE-APACHE](crates/pumpkin-plugin-api/LICENSE-APACHE).
* **Third-Party Assets & Data**: Bedrock mappings, protocol conversion data, and Minecraft assets are subject to their respective licenses and attribution terms. See [assets/NOTICE.md](assets/NOTICE.md) for full details.
