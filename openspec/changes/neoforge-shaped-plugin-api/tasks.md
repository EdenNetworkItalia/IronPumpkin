# Tasks

Each item mirrors one GitHub issue of this repository. Items without an issue number get one when the wave that implements them is planned.

Milestones named below: M1 Native mod channel, M2 Content registry, M3 NeoForge handshake and registry sync, M4 NeoForge-shaped native API, M5 Modded gameplay parity.

## 1. Inventories and contract

- [x] 1.1 Complete inventory of the NeoForge 26.3.x API surface as the M4 contract (#34); verify: design.md lists every family with counts, every row has a side and a status, `openspec validate --all` passes
- [x] 1.2 Scan the reference modpack (FTB StoneBlock 4) for the NeoForge and Minecraft classes its mods use (#46); verify: `modpack-usage.md` in this change folder
- [x] 1.3 Mapping table of the NeoForge API to its counterparts (#36); verify: `mapping-table.md` in this change folder, every server-side inventory row has a counterpart or a reason
- [x] 1.4 Update the contract against the modpack scan (#49): mods column and gap verdicts in `mapping-table.md`, the event propagation and one event pipeline requirements, the task order; verify: `openspec validate neoforge-shaped-plugin-api --strict`
- [x] 1.5 Native channel proposal and design (#50): the section "Native channel" of design.md, the spec `neoforge-native-channel`, this task order; verify: `openspec validate neoforge-shaped-plugin-api --strict`, `openspec validate --all`, `uvx typos` on this change folder
- [x] 1.6 Method-level mixin target scan of the reference modpack (#51), the demand by target method; verify: the mixin targets by method in `modpack-usage.md`, `openspec validate --all`
- [ ] 1.7 Two native primitives (#53): the NeoForge-shaped API and compile-time source patches in the section "Native channel" of design.md, the spec `neoforge-native-channel`, the mapping table verdicts, the README and `openspec/config.yaml`; verify: `openspec validate neoforge-shaped-plugin-api --strict`, `openspec validate --all`, `uvx typos` on the changed files

## 2. Native channel

The two primitives, in order: the NeoForge-shaped API `ironpumpkin-neo` and the source patches that the modpack build applies. The foundation (2.1) runs first because every later task tests through a mod linked into an example pack binary. The family tasks (2.2 to 2.18) are ordered by the number of reference modpack mods that need their largest row (in brackets, from `mapping-table.md`), except that a task follows the task it builds on. Each family task implements its delta spec for native mods.

### 2.1 Foundation

- [x] 2.1.1 `pumpkin` library entry point and native mod registration (#52): `pumpkin::run()`, `ironpumpkin-mods` with `NativeMod` and `register_mod!` over `inventory`, an example mod and the example modpack workspace `examples/modpack`; verify: the example pack boots and logs its mod, `cargo test -p ironpumpkin-mods`
- [ ] 2.1.2 Patch application in the modpack blueprint build (#54; follows 2.1.1): `modpack.toml`, the build tool that checks out IronPumpkin at the pinned commit and applies the source patches of the mods as design.md says, and the GitHub Action that builds the binaries per pack; verify: the patch scenarios of `neoforge-native-channel`, the example pack builds and boots from the blueprint

### 2.2 to 2.18 NeoForge families

- [ ] 2.2 `lifecycle` (411 mods, `@Mod`): `neoforge.mods.toml` metadata from the mod registration, mod container and mod list, mod bus event order, IMC, pack finders and reload listeners; verify: the `neoforge-lifecycle` scenarios as tests
- [ ] 2.3 `event-bus` core with the server, tick and root package events (263 mods, `IEventBus#addListener`): `add-listener`, `post`, dispatch to every mod, write-back, cancellation and results, for `event`, `event.server`, `event.tick`, `event.brewing`, `event.enchanting` and the other packages; verify: the `neoforge-event-bus` scenarios, one test per supported event row of these packages in `mapping-table.md`, and the cancel and write-back tests of this family
- [ ] 2.4 One event pipeline, Pumpkin side (follows 2.3): `level-access` over the Pumpkin world with the events of its table, projectile causes resolved to their owner; verify: a v0.2 plugin sees and cancels each `level-access` mutation that a player or an entity causes, including a mod projectile that breaks a block, and a cause-less `set-block` fires no event
- [ ] 2.5 `registration` (224 mods, `DeferredHolder`): deferred registers, builders and holders over the M2 content registry for blocks, items and entity types, mod behaviours as `BlockBehaviour` and `ItemBehaviour` implementations, and the registration events; verify: the `neoforge-registration` scenarios, a boot test that registers and places a block
- [ ] 2.6 Data component types, sound events, mob effects and attributes (247 mods, `DataComponentType`; follows 2.5): back the four registries, `data-components`, `mob-effects` and `entity-attributes`, and the NeoForge attributes `neoforge:swim_speed` and `neoforge:creative_flight`; verify: the task 2.6 scenarios of `neoforge-registration`
- [ ] 2.7 `capabilities` (189 mods, the `transfer` packages): capability registry, mod providers, Pumpkin inventories as item providers, fluid and energy handlers, transactions and caches; verify: the `neoforge-capabilities` scenarios
- [ ] 2.8 `mod-config` (182 mods, `ModConfigSpec`): config specs, TOML files, value access, config events and the `synced` type over the M3 `config_file` sync; verify: the `neoforge-mod-config` scenarios
- [ ] 2.9 `menus` (176 mods, `AbstractContainerMenu`; follows 2.5 and 2.7): open menus of vanilla menu types over the Pumpkin screen handlers, slots over the four slot sources, data slots and the menu callbacks; mod menu types wait for M5 `advanced_open_screen`; verify: the `neoforge-menus` scenarios
- [ ] 2.10 `shapes` (170 mods, `VoxelShape`): shape reads and collision tests over the generated block shapes, and the declared shapes of mod blocks in Pumpkin collision; verify: the `neoforge-shapes` scenarios
- [ ] 2.11 `network` (170 mods, `RegisterPayloadHandlersEvent`): payload registrar, payload dispatch and sending, payload context and configuration tasks over the M3 negotiation; verify: the `neoforge-network` scenarios with `tools/pumpkin-neoforge-client`
- [ ] 2.12 `event-bus` player events (96 mods, `PlayerEvent`; `event.entity.player`); verify: one test per supported row of the package, and the cancel and write-back tests of this family
- [ ] 2.13 `event-bus` level and block events (65 mods, `BlockEvent`; `event.level`, `event.level.block`); verify: one test per supported row of these packages, the cancel and write-back tests of this family, and the cross-mod `break-block-event` scenario of `neoforge-event-bus`
- [ ] 2.14 `attachments` (57 mods, `IAttachmentHolder#getData`; also the `@Unique` state of mixins): attachment storage, saving, copy on death and `neoforge:sync_attachments` (needs M5 storage); verify: the `neoforge-attachments` scenarios
- [ ] 2.15 `event-bus` entity, living and item entity events (49 mods, `EntityJoinLevelEvent`; `event.entity`, `event.entity.living`, `event.entity.item`), with an entity tick event per entity, as on NeoForge; verify: one test per supported row of these packages, and the cancel and write-back tests of this family
- [ ] 2.16 `loot` (29 mods, `IGlobalLootModifier`): global loot modifier types, the `global_loot_modifiers.json` loader and the built-in `neoforge:add_table` and `neoforge:loot_table_id`; verify: the `neoforge-loot` scenarios
- [ ] 2.17 `data-maps` (27 mods, `DataMapType`): data map registration, the data map loader, mergers, removers and sync (needs M5 loader); verify: the `neoforge-data-maps` scenarios
- [ ] 2.18 One event pipeline, neo side (follows 2.13 and 2.15, which wire the neo block, entity and damage events): the neo bus as the second view of the Pumpkin dispatch for every `level-access` event; verify: all "One event pipeline" scenarios of `neoforge-event-bus`, in both directions (a v0.2 plugin cancels and a neo listener observes, a neo listener cancels and a v0.2 plugin observes)

## 3. Wasm projection

Future work, generated from `ironpumpkin-neo`. Task 3.1 builds the generator and the loading; task 3.k for k from 2 to 18 projects the native task 2.k and depends on it. Source patches are native only and are not projected.

- [ ] 3.1 Projection generator and loading (depends on 2.1.1 and 2.2): generate the WIT package and the host adapter from `ironpumpkin-neo`, load a Wasm mod component next to native mods in one mod list, and allow reentrant callbacks through the `pumpkin-plugin-runtime` reentry pump; verify: a sample Wasm mod boots, its `neoforge.mods.toml` is read, and regenerating the projection leaves a clean tree
- [ ] 3.2 `lifecycle` projection (depends on 2.2); verify: the `neoforge-lifecycle` scenarios with a Wasm mod
- [ ] 3.3 `event-bus` core projection (depends on 2.3): Wasm listeners as adapters on the native bus, write-back and cancellation; verify: the "Every supported event reaches Wasm plugins" scenarios of `neoforge-event-bus` for this family
- [ ] 3.4 One event pipeline, Pumpkin side, projection (depends on 2.4): the v0.2 mutators of a Wasm mod routed through the same Pumpkin events; verify: the v0.2 world call scenario of `neoforge-event-bus`
- [ ] 3.5 `registration` projection (depends on 2.5); verify: the `neoforge-registration` scenarios with a Wasm mod
- [ ] 3.6 Data components, sound events, mob effects and attributes projection (depends on 2.6); verify: the task 2.6 scenarios with a Wasm mod
- [ ] 3.7 `capabilities` projection (depends on 2.7); verify: a Wasm provider answers a lookup from a native mod
- [ ] 3.8 `mod-config` projection (depends on 2.8); verify: the `neoforge-mod-config` scenarios with a Wasm mod
- [ ] 3.9 `menus` projection (depends on 2.9); verify: the `neoforge-menus` scenarios with a Wasm mod
- [ ] 3.10 `shapes` projection (depends on 2.10); verify: the `neoforge-shapes` scenarios with a Wasm mod
- [ ] 3.11 `network` projection (depends on 2.11); verify: the `neoforge-network` scenarios with a Wasm mod
- [ ] 3.12 `event-bus` player events projection (depends on 2.12); verify: the cancel and write-back tests of this family from a Wasm listener
- [ ] 3.13 `event-bus` level and block events projection (depends on 2.13); verify: the cross-mod `break-block-event` scenario with a Wasm listener and a native mod
- [ ] 3.14 `attachments` projection (depends on 2.14); verify: the `neoforge-attachments` scenarios with a Wasm mod
- [ ] 3.15 `event-bus` entity, living and item entity events projection (depends on 2.15), hot events in batches only; verify: the cancel and write-back tests of this family from a Wasm listener
- [ ] 3.16 `loot` projection (depends on 2.16); verify: the `neoforge-loot` scenarios with a Wasm mod
- [ ] 3.17 `data-maps` projection (depends on 2.17); verify: the `neoforge-data-maps` scenarios with a Wasm mod
- [ ] 3.18 One event pipeline, neo side, projection (depends on 2.18); verify: the cross-channel scenarios of `neoforge-event-bus` (a native listener cancels and a Wasm listener observes, and the reverse)

## Workflow follow-up

- Plan the implementation waves from design.md, `modpack-usage.md` and `mapping-table.md`, then give each section 2 item its issue number.
- Archive this change with `openspec archive neoforge-shaped-plugin-api` when the phase lands.
