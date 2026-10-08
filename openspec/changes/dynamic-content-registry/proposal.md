# Proposal

## Why

Blocks, items and entity types in Pumpkin are static tables generated from `assets/`, with `&'static` lookups and u16 ids. A plugin cannot add content, so a ported NeoForge mod has nowhere to register its blocks, items and entities. The project target is a NeoForge-shaped Wasm plugin API for ported mods, and its DeferredRegister needs a server-side registry; the NeoForge registry sync needs the same registry to send ids to modded clients.

Phase goal: blocks, items and entity types absent from `assets/` can be registered by plugins at startup and survive a world round trip.

## What Changes

- A content registry in pumpkin-core that allocates ids above the generated ranges and answers lookups the generated tables cannot, with the generated lookups falling through to it.
- A block state space for custom blocks.
- Chunk palette and Anvil persistence of custom blocks by namespaced name, with a placeholder for blocks whose plugin is missing.
- Item stacks with custom items in inventories, drops and NBT.
- Content registration in the native plugin Context and, additively, in the Wasm plugin API (a new `registry.wit` interface).
- Behaviour hooks that attach existing BlockBehaviour and ItemBehaviour implementations to custom content.
- Custom entity types with a spawn factory and a display mapping to a vanilla type.

Out of scope: sending custom ids to clients (vanilla clients cannot see custom content; NeoForge clients need the registry sync of phase 3), models, GUIs.

The id-space and lifetime decision (#16) comes first: it decides the shape of every other change in this phase.

## Capabilities

### New Capabilities

Planned, written as delta specs after the id-space decision (#16) lands:

- `content-registry`: registration, id allocation, lookup fallthrough and the frozen state.
- `custom-block-persistence`: custom block states in chunk palettes and Anvil saves.
- `custom-item-stacks`: custom items in inventories, drops and NBT.
- `plugin-content-registration`: the native and Wasm registration API and behaviour hooks.
- `custom-entity-types`: registration, spawning, saving and display mapping of custom entities.

### Modified Capabilities

None.

## Impact

- Crates: pumpkin-data (lookups only, never the generated files), pumpkin-core, pumpkin-world (palettes, Anvil), pumpkin-plugin-wit v0.2 (additive), pumpkin-wasm-host-v0_2, pumpkin-plugin-api.
- Upstream rebases will conflict around `&'static` call sites. The registry stays a fallthrough layer to keep the conflicts small.
- Dependency: phase 1 only for the shared test harness (`tools/pumpkin-neoforge-client`).

## Verification

- Unit tests for allocation and lookup.
- pumpkin-gametest: place, save, reload and read back a custom block.
- Boot test with a sample plugin that registers content.
