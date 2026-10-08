# Proposal

## Why

The project goal is to make porting a NeoForge mod to IronPumpkin a guided translation. Today a port has no NeoForge-shaped API to target: events, registries, capabilities, config and metadata all have different names and shapes in Pumpkin, and some do not exist. The API must be designed against the whole NeoForge 26.3.x surface, not grown behind the first ported mod, so that every mod author knows in advance what is supported, planned, client-only or never supported.

Phase goal (pinned "Orchestration status" issue #29, roadmap, the NeoForge-shaped plugin API phase): a crate `ironpumpkin-neo` (Rust, Wasm-compatible, MIT OR Apache-2.0) with `DeferredRegister<Block>`, an `IEventBus` facade that maps NeoForge events to Pumpkin events (block breaking, NeoForge `BreakBlockEvent`, maps to Pumpkin `BlockBreakEvent`), `neoforge.mods.toml` as the metadata source, a `ModConfigSpec`-like config that feeds the configuration sync of the NeoForge handshake phase, and capabilities as `Context::register_service` services.

Decision (2026-10-07, recorded in #29): the target is a NeoForge-shaped Wasm plugin API for ported mods. Loading real Java mod jars is out of scope, because a mod jar needs the whole `net.minecraft` Java API surface, which Pumpkin does not have. The Java feasibility spike stays optional and runs only on the owner's request.

## What Changes

- An inventory of the NeoForge 26.3.x API surface, generated from the sources, with a side and a status for every row (design.md, #34). It is the contract of this phase.
- An inventory of what the reference modpack (FTB StoneBlock 4, 414 mods) uses (`modpack-usage.md`, #46). It sets the order of the work.
- The design (#36): one contract, the WIT package `ironpumpkin:neo` in `crates/pumpkin-plugin-wit/neo` (MIT OR Apache-2.0), with one interface per API family and its own host crate `pumpkin-wasm-host-neo` (GPL-3.0). A ported mod is a Wasm component of the `neo-plugin` world, which includes the `pumpkin:plugin@0.2.0` world. The host starts as a stub whose imports trap with "not implemented"; the phase implements it one interface at a time.
- `ironpumpkin-neo`, the guest SDK over that package (MIT OR Apache-2.0, no code copied from the GPL server crates or from NeoForge, which is LGPL-2.1): Rust types and macros with the NeoForge names (`DeferredRegister`, `@SubscribeEvent`-style listener registration, `ModConfigSpec`) that call the `ironpumpkin:neo` imports.
- A mapping table (`mapping-table.md`, NeoForge class -> WIT counterpart and status) where each supported row gets a test and each unsupported row an explicit reason.

Out of scope: loading Java mod jars and client-side content (models, textures, screens, key mappings). Block entities, mod menu types, attachments and data maps are implemented in the modded gameplay parity phase; this phase only defines their WIT contract.

## Capabilities

### New Capabilities

One delta spec per interface of the `ironpumpkin:neo` WIT package (#36):

- `neoforge-wit-contract`: the package, its worlds and shared types, the exported callbacks, validation, and the stub host crate.
- `neoforge-event-bus`: listener registration with NeoForge event names, priorities, cancellation and results, mapped to Pumpkin events, delivered to every Wasm mod, and one event pipeline that v0.2 plugins and neo mods share.
- `neoforge-registration`: `DeferredRegister` and holders over the content registry.
- `neoforge-capabilities`: capability keys, providers and lookups as services.
- `neoforge-attachments`: attachment types and holders.
- `neoforge-data-maps`: data map types and their data pack values.
- `neoforge-mod-config`: config specs, config types and config sync.
- `neoforge-network`: payload registration, handling, sending and configuration tasks.
- `neoforge-lifecycle`: `neoforge.mods.toml` metadata, entry points, lifecycle order and the mod list.
- `neoforge-loot`: global loot modifiers and loot conditions.
- `neoforge-menus`: container menus opened for players, with slots, data slots and the menu logic as callbacks.
- `neoforge-shapes`: block shape reads, collision tests and the declared shapes of mod blocks.

### Modified Capabilities

None.

## Impact

- New WIT package `crates/pumpkin-plugin-wit/neo` and new crates `pumpkin-wasm-host-neo` (GPL-3.0, not linked into the server binary yet) and `ironpumpkin-neo` (guest SDK, MIT OR Apache-2.0), all in bounded locations so `git merge upstream/master` stays cheap. The `neo` package is not mirrored: `sync-wit.yml` publishes the upstream repository only.
- `pumpkin-wasm-host-v0_2` and the v0.2 WIT are reused, not changed: the neo host binds the v0.2 types through `with`.
- Depends on the dynamic content registry phase for registration, and on the NeoForge handshake and registry sync phase for config sync and for NeoForge clients that see the ported content.

## Verification

- Port one small NeoForge mod end to end.
- The mapping table: every row has a test or an explicit "unsupported".
- `openspec validate --all` passes.
