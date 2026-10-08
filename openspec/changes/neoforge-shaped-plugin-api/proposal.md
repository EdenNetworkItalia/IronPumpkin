# Proposal

## Why

The project goal is to make porting a NeoForge mod to IronPumpkin a guided translation. Today a port has no NeoForge-shaped API to target: events, registries, capabilities, config and metadata all have different names and shapes in Pumpkin, and some do not exist. The API must be designed against the whole NeoForge 26.3.x surface, not grown behind the first ported mod, so that every mod author knows in advance what is supported, planned, client-only or never supported.

Ported mods also need what NeoForge mods get from mixins and reflection: per-entity and per-tick code inside vanilla logic. The reference modpack (FTB StoneBlock 4, `modpack-usage.md`) ships 3508 mixin classes in 259 of 414 mods, and its most targeted vanilla classes are `Entity` (43 mods), `LivingEntity` (42), `Player` (30) and `ItemStack` (23). A Wasm call cannot carry that load: each hook into a Wasm plugin is a cross-task round trip through the per-plugin `StoreExecutor` (`crates/pumpkin-plugin-runtime/src/executor.rs`, started from `crates/pumpkin-wasm-host-common/src/concurrent_store.rs`), and every world that calls the same plugin waits on that one executor.

Decisions, recorded in the pinned "Orchestration status" issue (#29):

- Ported NeoForge mods are native Rust crates compiled into the server binary at build time (the xcaddy model). A modpack is a Cargo workspace that links the server library and one crate per mod; CI builds one binary per pack. Java mods also ran in-process with no sandbox, so the trust model does not change.
- Wasm stays for plugins: sandbox, hot reload and coarse services. A Wasm API for mods is future work: a projection generated from the native API, kept where a Wasm call per use is cheap enough.
- Loading real Java mod jars is out of scope. An embedded JVM and a NeoForge sidecar are rejected: a mod jar needs the whole `net.minecraft` Java API surface, which Pumpkin does not have, and the owner dropped the Java feasibility work on 2026-10-08.

Phase goal: a native Rust crate `ironpumpkin-neo` (MIT OR Apache-2.0) with the NeoForge names and structure: an `IEventBus` facade that maps NeoForge events to Pumpkin events (block breaking, NeoForge `BreakBlockEvent`, maps to Pumpkin `BlockBreakEvent`), deferred registers (`DeferredRegister.Blocks` and the other typed registers), capabilities, attachments, a `ModConfigSpec`-like config that feeds the configuration sync of the NeoForge handshake phase, and `neoforge.mods.toml` as the metadata source. A future Wasm projection is generated from this API.

## What Changes

- The native channel, which is the target. `crates/pumpkin` becomes a library with a thin binary, and `ironpumpkin-mods` (#52) collects the mods linked into a pack binary at build time. `ironpumpkin-neo` is the native API with the NeoForge names. Three primitives replace mixins and reflection: build-time accessors, hook points through a `#[hook]` attribute macro, and service seams (replaceable and decorable trait objects, like `BlockBehaviour` and `ItemBehaviour` in `crates/pumpkin-core`). Attachments hold the state that mixins add with `@Unique`. A blueprint repository with a GitHub Action produces the binaries per pack. design.md, section "Native channel", has the build model, the primitives, the performance rules, the API shape rules and the licence consequence.
- The Wasm projection, which is secondary and future work. A generator derives a WIT package and its host adapter from `ironpumpkin-neo`, so the two never drift. The projection keeps what a Wasm call per use can carry: registration, lifecycle, config, network, capability lookups, menus, shapes, attachments, loot modifiers, data maps and the events; per-entity and per-tick events reach Wasm plugins only in batches. Hook points, accessors and fine-grained service seams are native only. It has no issue yet.
- One event dispatch for everything: every world mutation through the API fires the Pumpkin event of the vanilla action, so v0.2 plugins and native mods see and can cancel each other's actions, and a Wasm listener of the projection joins the same dispatch.
- An inventory of the NeoForge 26.3.x API surface, generated from the sources, with a side and a status for every row (design.md, #34). It is the contract of this phase.
- An inventory of what the reference modpack uses (`modpack-usage.md`, #46). It sets the order of the work. A method-level scan of the mixin targets (#51) sizes the catalogue of hook points, accessors and service seams.
- A mapping table (`mapping-table.md`, NeoForge class -> counterpart and status) where each supported row gets a test and each unsupported row an explicit reason.

What changes for each existing artifact:

| Artifact | Change |
|:--|:--|
| `mapping-table.md` | Stays the contract source: the inventory of rows, counterparts and verdicts. The counterpart column names the API item in interface notation (`registration.deferred-register.register`); the `ironpumpkin-neo` item has the same name in Rust case. The verdicts of the gaps that the native channel closes change from out of scope to a plan: mixins, access transformers, world generation and the world generation registry types. |
| `modpack-usage.md` | Stays the usage inventory. Its mixin section sizes the three primitives; #51 adds the mixin targets by method. |
| WIT package `crates/pumpkin-plugin-wit/neo` | Removed. The Wasm projection is generated from `ironpumpkin-neo` when its work starts. |
| `pumpkin-wasm-host-neo` | Removed. The generated host adapter of the Wasm projection takes its place. |
| Delta specs | They name API items in interface notation and describe `ironpumpkin-neo`. The new spec `neoforge-native-channel` holds the build model, the primitives, the performance rules, the API shape and the licence. The event bus spec states the two event constraints for native mods and for the Wasm projection. |
| `tasks.md` | Section 2 is the native channel; section 3 is the Wasm projection, and each of its tasks depends on its native task. |

Out of scope: loading Java mod jars, client-side content (models, textures, screens, key mappings), client mixins, and mixins that change data structures or data formats. Block entities, mod menu types, attachment storage and data maps are implemented in the modded gameplay parity phase; this phase defines their API.

## Capabilities

### New Capabilities

- `neoforge-native-channel`: native mods compiled into the server binary, their registration, the three primitives that replace mixins and reflection, the performance rules, the API shape rules and the licence of linked mods.

One delta spec per interface of `ironpumpkin-neo`:

- `neoforge-event-bus`: listener registration with NeoForge event names, priorities, cancellation and results, mapped to Pumpkin events, delivered to every mod, and one event pipeline that v0.2 plugins and mods share.
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

- `crates/pumpkin` gets a library entry point; `src/main.rs` calls it (#52). New crates: `ironpumpkin-mods` (mod registration, #52) and `ironpumpkin-neo` (the native API) with its macro crate for `#[hook]` and the accessors, all MIT OR Apache-2.0, with no code copied from the GPL server crates or from NeoForge (LGPL-2.1).
- Hook points, accessors and service seams touch upstream Pumpkin code. Each hook point is one attribute on an upstream function, the accessors are generated into one module per owning crate, and each service seam replaces one direct call with a call through a trait object. The catalogue grows only on demand, so `git merge upstream/master` stays cheap.
- A native mod linked into a pack binary is a derivative work of the GPL-3.0 server and must have a GPL-3.0-compatible licence. Wasm plugins have no such constraint.
- The v0.2 WIT and `pumpkin-wasm-host-v0_2` do not change. The future Wasm projection reuses the v0.2 types for players, entities, levels and item stacks.
- Depends on the dynamic content registry phase for registration, and on the NeoForge handshake and registry sync phase for config sync and for NeoForge clients that see the ported content.

## Verification

- Port one small NeoForge mod end to end as a native crate in an example modpack workspace.
- The bench of the `#[hook]` task gives the cost of one hook point with no listener, with a native listener and with a call into a Wasm plugin through the v0.2 executor.
- The mapping table: every row has a test or an explicit "unsupported".
- `openspec validate --all` passes.
