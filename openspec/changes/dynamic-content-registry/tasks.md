# Tasks

Each item mirrors one GitHub issue on EdenNetworkItalia/IronPumpkin. All issues carry the `later` label until phase 1 lands; remove it when the issue enters a wave. Items marked "issue to create" come from the id-space decision (#16) and have no issue yet.

Order after the id-space decision: 2.1 and 2.3 in parallel, then 3.1, 4.1 and 6.1 in parallel, then 2.2, then 3.2 and 3.3, then 5.1, then 5.2. The palette namespace fix (3.4) lands before 3.2. The startup content phase (2.2) comes before the plugin API because plugins load after the worlds today, and registration must end before the first world loads. The numeric save sites (2.3) move to names before custom content is persisted (3.2).

## 1. Decision

- [x] 1.1 Decision record on id spaces and lifetimes for custom content (#16); verify: the Decisions section of this design.md has the approach, the rg call-site counts and the id ranges

## 2. Registry core

- [x] 2.1 ContentRegistry core (#17, depends on #16): frozen tables in `crates/pumpkin-data/src/dynamic.rs`, fallthrough in the codegen templates of `block.rs`, `item.rs`, `entity_type.rs` and `flower_pot_transformations.rs`, custom-aware `BlockStateId::new` and `BlockId::new`, name-sorted id allocation, freeze; verify: unit tests for allocation that does not depend on registration order, duplicate and `minecraft:` rejection, fallthrough of every lookup in the decision, the frozen state; `cargo run --locked -p pumpkin-codegen` leaves a clean tree; the existing benches on master and on the branch
- [x] 2.2 Startup content phase, content manifest and placeholders (issue to create, depends on #17 and #18): `load_plugins` and the freeze between `Arc::new(server)` and the world loop in `Server::new`, `RegistryFrozen` for new names after it, the hot-reload rebind rule, `<world>/ironpumpkin/content_registry.json` read and written, the manifest schema rule, placeholder blocks, items and entity types for missing content, raw compounds of placeholder entity types kept in the entity chunk; verify: a unit test or gametest with a test-only registration writes the manifest, a second run without that registration logs one warning per missing name, registers the placeholders and keeps a placeholder entity compound through a save
- [x] 2.3 Convert the three numeric-id save sites to names (#38, no dependency): enderman `carriedBlockState` and block display `block_state` as block state compounds like vanilla, player statistics keyed by namespaced name; verify: NBT round-trip unit tests per site and a vanilla-written enderman and block display load correctly. The enderman and block display sites differ from vanilla, so they are upstream bugs: own commit and an upstream PR draft in the issue
- [x] 2.4 Tag membership for custom content without a hot-path branch (#45, depends on #17): at the freeze, build a separate per-registry custom tag table (a map from generated tag name to the sorted custom ids), filled from the display entry's tags plus explicit tags; only non-hot paths read it (a `has_tag_dynamic` and the datapack and command tag registry); `BlockId::has_tag` and the generated `Taggable` methods stay unchanged, because a custom-id branch there cost +5% on `noise_generation`; verify: unit test of inherited and explicit tags, benches unchanged

## 3. Blocks

- [x] 3.1 Block state space for custom blocks (#18, depends on #17): the state layout rule of the decision (properties in name order, last property fastest) and `DynamicProperties`; verify: unit tests for a block with no properties, one boolean, and two mixed properties, with state ids checked against the layout rule
- [x] 3.2 Chunk palette and Anvil persistence for custom blocks (#19, depends on #18, the startup content phase and the numeric save sites task): namespaced `Name` on save, one warning per unknown name on load, placeholder blocks round-trip; verify: gametest save and reload of a custom block, save and reload with the plugin missing keeps the palette entry, byte comparison of a vanilla chunk before and after
- [ ] 3.3 Display mapping of custom block states for vanilla clients (issue to create, depends on #18): one `to_java_network_id()` per id type as the only egress for block states and block ids (chunk data palette, block update and block event packets, block-break event 2001, entity metadata, block particles, statistics, Bedrock ids); verify: a grep finds no raw block id written to a packet outside those functions, and a bot or client loads a chunk with a custom block and sees the display block without a decode error. It touches `crates/pumpkin-core/src/net` and `crates/pumpkin-protocol`: schedule it after the network work in progress there lands

- [x] 3.4 Chunk palette writer keeps the namespace of non-`minecraft:` names (#40, bug, before 3.2): the writer in `crates/pumpkin-world/src/chunk/format/mod.rs` checks for any namespace separator, as `crates/pumpkin-core/src/block/state_nbt.rs` does, in every format that reuses the serializer and on the reader side; verify: a palette entry with a `mymod:` namespace is written and read back unchanged

## 4. Items

- [x] 4.1 Item stacks with custom items (#20, depends on #17): namespaced `"id"` in NBT, display item through the item `to_java_network_id()` in the item stack codec and `recipe_book_add.rs`; verify: NBT round-trip unit test and a relog with a custom item in the inventory

## 5. Plugin API

- [x] 5.1 Native plugin Context API for content registration (#21, depends on #17, #20 and the startup content phase): registration runs in the content phase; verify: a sample native plugin registers a block and an item and the server boots
- [ ] 5.2 Behaviour hooks for custom blocks and items (#23, depends on #21); verify: boot test where placing, breaking and using run the attached behaviour

## 6. Entities

- [x] 6.1 Custom entity types (#24, depends on #17): factory arm in `from_type`, display type on the wire; verify: a custom entity spawns, ticks, saves and reloads in a boot or gametest check

## Workflow follow-up

- Write the delta specs listed in proposal.md after #16 lands, then remove `skip_specs` from `.openspec.yaml`.
- Archive this change with `openspec archive dynamic-content-registry` when M2 lands.
