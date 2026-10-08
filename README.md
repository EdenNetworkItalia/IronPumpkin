<div align="center">

# IronPumpkin

[![CI](https://github.com/EdenNetworkItalia/IronPumpkin/actions/workflows/rust.yml/badge.svg)](https://github.com/EdenNetworkItalia/IronPumpkin/actions/workflows/rust.yml)
[![License: GPL](https://img.shields.io/badge/License-GPLv3-yellow.svg)](https://opensource.org/licenses/gpl-3-0)

</div>

IronPumpkin is a Minecraft server maintained by [EdenNetwork Italia](https://github.com/EdenNetworkItalia).
It is a derivative of [Pumpkin](https://github.com/Pumpkin-MC/Pumpkin), a Minecraft server written in Rust.
The goal is NeoForge compatibility on top of Pumpkin: custom content, NeoForge clients, and ported NeoForge mods.

> [!IMPORTANT]
> IronPumpkin is at an early stage. The roadmap below is planned work. Nothing on it is done yet.

## Relationship with Pumpkin

IronPumpkin builds on the work of the [Pumpkin project](https://pumpkinmc.org/) and its
[authors and contributors](https://github.com/Pumpkin-MC/Pumpkin/graphs/contributors).
Pumpkin is licensed under the GPLv3, and so is IronPumpkin.

- IronPumpkin is not a GitHub fork. It is a derivative that merges upstream changes regularly.
- IronPumpkin tracks the `master` branch of Pumpkin.
- Gameplay fixes and general improvements go to Pumpkin, not to this repository.
- Only the NeoForge compatibility layer lives here.

## Roadmap

The four goals are staged. Each goal depends on the ones before it.

- [ ] **Content registration API.** Plugins add custom blocks, items and entities.
  The server syncs dynamic registries to clients.
- [ ] **NeoForge network handshake.** NeoForge clients can join the server.
  This covers channel registration, registry sync and config sync.
- [ ] **NeoForge-shaped plugin API.** An event bus, deferred registers, capabilities and mod metadata.
  Custom NeoForge mods can be ported to IronPumpkin without a JVM.
- [ ] **JVM bridge feasibility study.** A bounded study of running real Java NeoForge server mods
  through a JVM bridge. It is a study, not a commitment to ship a bridge.

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

See Pumpkin's [Quick Start](https://docs.pumpkinmc.org/#quick-start) guide to get the server running.
IronPumpkin has no separate release or documentation yet.

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
* **Plugin API crates (`pumpkin-plugin-api`, `pumpkin-plugin-wit` & `pumpkin-plugin-utils`)**: Dual-licensed under MIT OR Apache-2.0, so plugin authors can choose either license.
  See [LICENSE-MIT](crates/pumpkin-plugin-api/LICENSE-MIT) and [LICENSE-APACHE](crates/pumpkin-plugin-api/LICENSE-APACHE).
* **Third-Party Assets & Data**: Bedrock mappings, protocol conversion data, and Minecraft assets are subject to their respective licenses and attribution terms. See [assets/NOTICE.md](assets/NOTICE.md) for full details.
