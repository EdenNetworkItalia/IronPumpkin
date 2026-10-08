# Tasks

Each item mirrors one GitHub issue of this repository. Section 2 items get their issue number when the wave that implements them is planned.

## 1. Inventories and contract

- [ ] 1.1 Complete inventory of the NeoForge 26.3.x API surface as the phase 4 contract (#34); verify: design.md lists every family with counts, every row has a side and a status, `openspec validate --all` passes
- [ ] 1.2 Scan the reference modpack (FTB StoneBlock 4) for the NeoForge and Minecraft classes its mods use (#46); verify: `modpack-usage.md` in this change folder
- [ ] 1.3 Formal WIT contract `ironpumpkin:neo` with the mapping table and a stub host (#36); verify: `wasm-tools component wit crates/pumpkin-plugin-wit/neo` passes, `cargo run --locked -p pumpkin-codegen -- wit` leaves a clean tree, `cargo check -p pumpkin-wasm-host-neo` and its clippy pass, `openspec validate --all` passes
- [ ] 1.4 Update the contract against the modpack scan (#49): mods column and gap verdicts in `mapping-table.md`, the WIT additions for the gaps marked "add now", the event propagation and one event pipeline requirements, this task order; verify: `wasm-tools component wit` and `wit-bindgen` for both worlds, `cargo check -p pumpkin-wasm-host-neo` with its linker test and clippy, `openspec validate neoforge-shaped-plugin-api --strict`

## 2. Interface implementations

One task per interface of `crates/pumpkin-plugin-wit/neo`, with the event bus split by event family. Each replaces the stub of its interface in `crates/pumpkin-wasm-host-neo` and satisfies its delta spec. Loading runs first because every later task tests through a loaded `neo-plugin` component. The other tasks are ordered by the number of reference modpack mods that need their largest row (in brackets, from `mapping-table.md`), except that a task follows the task it builds on.

- [ ] 2.1 `neo-plugin` loading: load a `neo-plugin` component in `pumpkin-wasm-host` with `pumpkin_wasm_host_neo::add_to_linker` in place of the v0.2 linker, call the `callbacks` exports, and allow reentrant callbacks through the `pumpkin-plugin-runtime` reentry pump; verify: a sample `neo-plugin` component boots and its `get-mods-toml` is read
- [ ] 2.2 `lifecycle` (411 mods, `@Mod`): `neoforge.mods.toml` metadata, mod container and mod list, mod bus event order, IMC, pack finders and reload listeners; verify: the `neoforge-lifecycle` scenarios as tests
- [ ] 2.3 `event-bus` core with the server, tick and root package events (263 mods, `IEventBus#addListener`): `add-listener`, `post`, dispatch to every Wasm mod through `handle-event`, write-back, cancellation and results, for `event`, `event.server`, `event.tick`, `event.brewing`, `event.enchanting` and the other packages; verify: the `neoforge-event-bus` scenarios, one test per supported event row of these packages in `mapping-table.md`, and the cancel and write-back tests of this family
- [ ] 2.4 One event pipeline, Pumpkin side (follows 2.3): `level-access` over the Pumpkin world with the events of its doc table, projectile causes resolved to their owner, and the v0.2 mutators of a `neo-plugin` component routed through the same Pumpkin events; verify: a v0.2 plugin sees and cancels each `level-access` mutation that a player or an entity causes, including a mod projectile that breaks a block, and a cause-less `set-block` fires no event
- [ ] 2.5 `registration` (224 mods, `DeferredHolder`): deferred registers, builders and holders over the phase 2 content registry for blocks, items and entity types, and the registration events; verify: the `neoforge-registration` scenarios, a boot test that registers and places a block
- [ ] 2.6 Data component types, sound events, mob effects and attributes (247 mods, `DataComponentType`; follows 2.5): back the four registries, the interfaces `data-components`, `mob-effects` and `entity-attributes`, and the NeoForge attributes `neoforge:swim_speed` and `neoforge:creative_flight`; verify: the task 2.6 scenarios of `neoforge-registration`
- [ ] 2.7 `capabilities` (189 mods, the `transfer` packages): capability registry, mod providers, Pumpkin inventories as item providers, fluid and energy handlers, transactions and caches; verify: the `neoforge-capabilities` scenarios
- [ ] 2.8 `mod-config` (182 mods, `ModConfigSpec`): config specs, TOML files, value access, config events and the `synced` type over the phase 3 `config_file` sync; verify: the `neoforge-mod-config` scenarios
- [ ] 2.9 `menus` (176 mods, `AbstractContainerMenu`; follows 2.5 and 2.7): open menus of vanilla menu types over the Pumpkin screen handlers, slots over the four slot sources, data slots and the menu callbacks; mod menu types wait for phase 6 `advanced_open_screen`; verify: the `neoforge-menus` scenarios
- [ ] 2.10 `shapes` (170 mods, `VoxelShape`): shape reads and collision tests over the generated block shapes, and the declared shapes of mod blocks in Pumpkin collision; verify: the `neoforge-shapes` scenarios
- [ ] 2.11 `network` (170 mods, `RegisterPayloadHandlersEvent`): payload registrar, payload dispatch and sending, payload context and configuration tasks over the phase 3 negotiation; verify: the `neoforge-network` scenarios with `tools/pumpkin-neoforge-client`
- [ ] 2.12 `event-bus` player events (96 mods, `PlayerEvent`; `event.entity.player`); verify: one test per supported row of the package, and the cancel and write-back tests of this family
- [ ] 2.13 `event-bus` level and block events (65 mods, `BlockEvent`; `event.level`, `event.level.block`); verify: one test per supported row of these packages, the cancel and write-back tests of this family, and the cross-mod `break-block-event` scenario of `neoforge-event-bus`
- [ ] 2.14 `attachments` (57 mods, `IAttachmentHolder#getData`): attachment storage, saving, copy on death and `neoforge:sync_attachments` (needs phase 6 storage); verify: the `neoforge-attachments` scenarios
- [ ] 2.15 `event-bus` entity, living and item entity events (49 mods, `EntityJoinLevelEvent`; `event.entity`, `event.entity.living`, `event.entity.item`); verify: one test per supported row of these packages, and the cancel and write-back tests of this family
- [ ] 2.16 `loot` (29 mods, `IGlobalLootModifier`): global loot modifier types, the `global_loot_modifiers.json` loader and the built-in `neoforge:add_table` and `neoforge:loot_table_id`; verify: the `neoforge-loot` scenarios
- [ ] 2.17 `data-maps` (27 mods, `DataMapType`): data map registration, the data map loader, mergers, removers and sync (needs phase 6 loader); verify: the `neoforge-data-maps` scenarios
- [ ] 2.18 One event pipeline, neo side (follows 2.13 and 2.15, which wire the neo block, entity and damage events): the neo bus as the second view of the Pumpkin dispatch for every `level-access` event; verify: all "One event pipeline" scenarios of `neoforge-event-bus`, in both directions (a v0.2 plugin cancels and a neo listener observes, a neo listener cancels and a v0.2 plugin observes)

## Workflow follow-up

- Plan the implementation waves from design.md, `modpack-usage.md` and `mapping-table.md`, then give each section 2 item its issue number.
- Archive this change with `openspec archive neoforge-shaped-plugin-api` when the phase lands.
