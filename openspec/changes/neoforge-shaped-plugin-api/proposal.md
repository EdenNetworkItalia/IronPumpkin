# Proposal

## Why

The project goal is to make porting a NeoForge mod to IronPumpkin a guided translation. A port needs the NeoForge behaviour in Pumpkin form: the events, registries, capabilities, config and metadata of NeoForge have different names and shapes in Pumpkin, and some do not exist. The API must be designed against the whole NeoForge 26.3.x surface, not grown behind the first ported mod, so that every mod author knows in advance what is supported, planned, client-only or never supported.

Ported mods also need what NeoForge mods get from mixins, reflection and access transformers: per-entity and per-tick code inside vanilla logic. The reference modpack (FTB StoneBlock 4, `modpack-usage.md`) ships 3508 mixin classes in 259 of 414 mods, and its most targeted vanilla classes are `Entity` (43 mods), `LivingEntity` (42), `Player` (30) and `ItemStack` (23). A Wasm call cannot carry that load: each hook into a Wasm plugin is a cross-task round trip through the per-plugin `StoreExecutor` (`crates/pumpkin-plugin-runtime/src/executor.rs`, started from `crates/pumpkin-wasm-host-common/src/concurrent_store.rs`), and every world that calls the same plugin waits on that one executor.

Decisions, recorded in the pinned "Orchestration status" issue (#29):

- Ported NeoForge mods are native Rust crates compiled into the server binary at build time (the xcaddy model). A modpack is a Cargo workspace that links the server library and one crate per mod; CI builds one binary per pack. Java mods also ran in-process with no sandbox, so the trust model does not change.
- A mod gets two primitives: the native API `ironpumpkin-mods`, and source patches to the server that the mod carries and the modpack build applies before it compiles. There is no hook catalogue, no generated accessor API and no service-seam API of IronPumpkin's own. The method-level mixin scan (#51) shows flat demand: 2674 server-side mixin members from 157 mods hit 1931 distinct target members, and the 200 most targeted ones cover 26% of the members and 11% of the mods.
- NeoForge semantics, Pumpkin shape (decided on 2026-10-15). The glue of a mod (event bus, deferred registers, mod bus) is a small part of a mod. The value is the catalogue of NeoForge and its behaviour on the wire. IronPumpkin takes the semantics and the catalogue, not the shape: the NeoForge events are Pumpkin events with the same fire sites, fields, cancel and result rules and event names, and the API is idiomatic Rust in `ironpumpkin-mods`. A Java-shaped facade crate and a Mojang-shaped `net.minecraft` facade are rejected.
- Wasm stays for plugins: sandbox, hot reload and coarse services. The existing v0.2 Wasm plugins reach the new events through the existing Pumpkin event pipeline, in the same dispatch.
- Loading real Java mod jars is out of scope. An embedded JVM and a NeoForge sidecar are rejected: a mod jar needs the whole `net.minecraft` Java API surface, which Pumpkin does not have, and the owner dropped the Java feasibility work on 2026-10-08.

Phase goal: the native API `ironpumpkin-mods` (MIT OR Apache-2.0) gives a mod the behaviour of NeoForge in Rust: the NeoForge events as Pumpkin events (block breaking, NeoForge `BreakBlockEvent`, is Pumpkin `BlockBreakEvent` with the missing fields added), lifecycle methods on the `NativeMod` trait, content registration with the `ModInit::register_*` builders, capabilities, attachments, a Rust TOML config spec that feeds the configuration sync of the NeoForge handshake phase, payload registration over the M3 negotiation, and optional `neoforge.mods.toml` metadata. The phase has a parity harness that compares a Java probe mod on a NeoForge server with a Rust probe mod on IronPumpkin, and a porting guide for mod authors.

## What Changes

- The native channel, which is the target. `crates/pumpkin` becomes a library with a thin binary, and `ironpumpkin-mods` (#52) collects the mods linked into a pack binary at build time and is the native API for mods. Its surface is the NeoForge public API of the inventory and nothing more: a hook point for a mod is a NeoForge event. What a NeoForge mod does with a mixin, reflection or an access transformer becomes a source patch: a mod crate ships `patches/<name>.patch` files with a justification file each and the IronPumpkin commit they are written for, and the modpack build applies them strictly, in the byte order of the `modpack.toml` keys (the crate package names), before it compiles. Attachments hold the state that mixins add with `@Unique`. A blueprint repository with a GitHub Action applies the patches and produces the binaries per pack. design.md, section "Native channel", has the build model, the two primitives, the patch contract and its governance, the coverage of the reference modpack, the API shape rules and the licence consequence.
- The NeoForge events as Pumpkin events. Each concrete NeoForge class is one Pumpkin event struct with the NeoForge name, fields, fire site, cancel and result rules (a nested class is flattened with the parent prefix, for example `ServerTickEventPost`). Where Pumpkin already has an event for a fire site, the mapping table maps the NeoForge event to it and the missing fields are added to it. A new struct exists only where Pumpkin has no event for that fire site. There is no second dispatch layer. The Pumpkin dispatch honours `EventPriority`.
- One event dispatch for everything: every world mutation through the API fires the Pumpkin event of the vanilla action, so v0.2 plugins and native mods see and can cancel each other's actions. The existing v0.2 Wasm plugins reach the new events through the existing Pumpkin event pipeline. Per-tick and per-entity events reach them one event per call, behind the handler check.
- A performance guardrail: no evident impact on Pumpkin performance. A fire site checks for registered handlers before it builds the event, and every wave that adds fire sites runs the world benches and the dispatch bench with zero listeners.
- An inventory of the NeoForge 26.3.x API surface, generated from the sources, with a side and a status for every row (design.md, #34). It is the contract of this phase. Rows that need an engine extension point (a registration API, a fixed set, a missing loader step) are `planned` with the prefix `M5 engine extension:`.
- An inventory of what the reference modpack uses (`modpack-usage.md`, #46). It sets the order of the work. A method-level scan of the mixin targets (#51) measures how many mixin members a NeoForge event covers and how many need a source patch.
- A mapping table (`mapping-table.md`, NeoForge class -> counterpart and status), the porting guide of the API, where each supported row gets a test and each unsupported row an explicit reason.
- A differential parity harness and a porting guide (`docs/porting-from-neoforge/`).

What changes for each existing artifact:

| Artifact | Change |
|:--|:--|
| `mapping-table.md` | Stays the porting guide and the contract source: the inventory of rows, counterparts and verdicts. The counterpart column names the `ironpumpkin-mods` item (a Rust path such as `ModInit::register_block`) or the Pumpkin event (such as `pumpkin_core::plugin::block::BlockBreakEvent`), in interface notation until the task that implements the row lands. The verdicts of the gaps that the native channel closes are a plan: mixins, reflection and access transformers become source patches; rows that need an engine extension point wait for M5. |
| `modpack-usage.md` | Stays the usage inventory. Its mixin sections count the mixin classes and the mixin targets by method (#51). |
| `ironpumpkin-mods` | Becomes the native API for mods: builders, lifecycle methods on `NativeMod`, config, network, capabilities, attachments and data maps as modules. |
| `PluginManager::fire_dyn` | Honours `EventPriority`: blocking handlers sorted by priority, then non-blocking handlers. |
| `ConfigurationTask` (`crates/pumpkin-core/src/net/java/configuration_tasks.rs`) | Gets a dynamic variant for mod tasks. |
| `startup::set_synced_configs` | The `OnceLock` of synced configs becomes a live source that the `SyncConfig` task reads. |
| Delta specs | They name the Pumpkin mechanism (`ModInit::register_event`, `NativeMod` methods, `ModInit::register_*`, Pumpkin event structs with NeoForge names). The spec `neoforge-native-channel` holds the build model, the API surface, the patch contract and its governance, the API shape, the performance guardrail and the licence. The event bus spec states the single dispatch and the priority rule. |
| `tasks.md` | Section 2 is the native channel, including the parity harness, the porting guide, the play state in the headless client and the branding. |

Out of scope: loading Java mod jars, client-side content (models, textures, screens, key mappings), client mixins, and mixins that change generated data (the tables under `crates/pumpkin-data/src/generated`); a mixin that changes a packet field or the save format becomes a source patch. Block entities, mod menu types, attachment storage and data maps are implemented in the modded gameplay parity phase; this phase defines their API. The engine extension points of M5 (registration APIs for the other registries, world generation types, the `neoforge:conditions` loader and the fixed sets) are the next phase.

## Capabilities

### New Capabilities

- `neoforge-native-channel`: native mods compiled into the server binary, their registration, the two primitives (the native API and compile-time source patches), the patch contract and its governance, the API shape rules, the performance guardrail and the licence of linked mods and patches.

One delta spec per area of `ironpumpkin-mods`:

- `neoforge-event-bus`: the NeoForge events as Pumpkin events, handler registration with priorities, cancellation and results, delivered to every mod, and one dispatch that v0.2 plugins and mods share.
- `neoforge-registration`: the `ModInit` builders over the content registry.
- `neoforge-capabilities`: capability keys, providers and lookups as services.
- `neoforge-attachments`: attachment types and holders.
- `neoforge-data-maps`: data map types and their data pack values.
- `neoforge-mod-config`: config specs, config types and config sync.
- `neoforge-network`: payload registration, handling, sending and configuration tasks.
- `neoforge-lifecycle`: optional `neoforge.mods.toml` metadata, lifecycle methods, init order and the mod list.
- `neoforge-loot`: global loot modifiers and loot conditions.
- `neoforge-menus`: container menus opened for players, with slots, data slots and the menu logic as callbacks.
- `neoforge-shapes`: block shape reads, collision tests and the declared shapes of mod blocks.

### Modified Capabilities

None.

## Impact

- `crates/pumpkin` gets a library entry point; `src/main.rs` calls it (#52). `ironpumpkin-mods` (mod registration, #52, and the native API) is MIT OR Apache-2.0, with no code copied from the GPL server crates or from NeoForge (LGPL-2.1).
- The internal hooks that fire NeoForge events touch upstream Pumpkin code in bounded modules. Source patches live in the mods and are bound to one IronPumpkin commit, so `git merge upstream/master` stays cheap for the server, and the mod author writes a patch again after an update.
- A native mod linked into a pack binary is a derivative work of the GPL-3.0 server and must have a GPL-3.0-compatible licence; a source patch is a derivative work in the same way. Wasm plugins have no such constraint.
- The v0.2 WIT and `pumpkin-wasm-host-v0_2` do not change.
- Depends on the dynamic content registry phase for registration, and on the NeoForge handshake and registry sync phase for config sync and for NeoForge clients that see the ported content.

## Verification

- Port one small NeoForge mod end to end as a native crate in an example modpack workspace.
- The blueprint build applies a valid patch and builds, and fails with a message on drift (the mod, the patch and both commits), on a conflict (the mods and the file) and on a patch that touches a generated file or `Cargo.lock` (the mod, the patch and the file).
- The parity harness (task 2.19): a Java probe mod on a NeoForge 26.3.0.64-beta dedicated server and a Rust probe native mod on IronPumpkin write a normalized trace for the same scripted actions, and `tools/parity-probe/diff.py` reports no difference for the supported events.
- The performance gate: each wave that adds fire sites runs `cargo bench -p pumpkin-world` and the dispatch bench with zero listeners, and shows no regression outside the bench noise.
- The porting guide (task 2.20): `docs/porting-from-neoforge/` takes a reader from the template repository to a booting pack binary, and its Rust snippets compile against `ironpumpkin-mods`.
- The mapping table: every row has a test or an explicit "unsupported".
- `openspec validate --all` passes.
