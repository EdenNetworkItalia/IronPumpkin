## Why

M4 gives a ported NeoForge mod the NeoForge events and the NeoForge APIs over the content that Pumpkin already has: blocks, items, entity types, data components, sound events, mob effects and attributes. Most mods of the reference modpack also register content of a kind that Pumpkin cannot hold, because the engine has a closed, generated set for it: 90 of the 98 mods that register blocks also register a block entity type, 78 register a menu type, 74 register a recipe serializer, 41 a particle type, 16 a fluid, and 72 add world generation. The M4 inventory marks these 140 rows `planned` with the prefix `M5 engine extension:`. This change adds the extension points, so that a mod that needs them can be ported.

## What Changes

- A generic table for mod entries of the static registries that need only an id, or an id and a server behaviour that a Rust trait object carries (particle types, potions, custom stats, stat types, villager types, map decorations, loot functions, conditions and entry types, triggers, consume effects and the others), with the registry sync of M3 built at run time from the registries that have mod entries.
- Mod block entity types: a registry entry, a factory, load and save, ticking, client sync and the type id on the wire.
- Mod menu types: a registry entry, `neoforge:advanced_open_screen` with extra data, and `neoforge:advanced_container_set_data` for 32-bit data slots.
- Item abilities, special plantables and the farmland water hook; mod-defined registries (`NewRegistryEvent`, `RegistryBuilder`) as a server-only `CustomRegistry<T>`.
- The `neoforge:conditions` key of data pack files, evaluated once for every loader that NeoForge covers.
- Mod recipe types and serializers, custom ingredient types, and the `neoforge:recipe_content` sync.
- Mod game rules beside the generated vanilla rules.
- Mod fluids: the fluid pair, `FluidType`, the flow, buckets, cauldron content and the entity interaction of water-like fluids.
- World generation: mod configured and placed features of the vanilla feature types, mod feature and placement modifier types, NeoForge biome modifiers and spawns; mod biomes and structures last.
- Chunk tickets per mod (`RegisterTicketControllersEvent`, `TicketController`) saved with the world.
- The extension interface hooks that #184 marks `M5 engine extension:`, ordered by the demand of the modpack scan.
- The remaining M5 rows: chunk data events, entity size, default components, cauldron interactions and the others, each implemented or marked `not supported` with the reason.

## Capabilities

### New Capabilities
- `neoforge-static-registries`: the generic mod registry table, the run-time registry sync rows, item abilities and mod-defined registries.
- `neoforge-block-entity-types`: mod block entity types.
- `neoforge-menu-types`: mod menu types, the advanced open screen payload and the 32-bit data slots.
- `neoforge-data-conditions`: `neoforge:conditions` in the data pack loaders.
- `neoforge-recipes`: mod recipe types, serializers, ingredient types and recipe sync.
- `neoforge-game-rules`: mod game rules.
- `neoforge-fluids`: mod fluids and `FluidType`.
- `neoforge-worldgen`: mod features, placement modifiers, biome modifiers, mod biomes and structures.
- `neoforge-chunk-tickets`: chunk tickets per mod.
- `neoforge-extension-hooks`: the hooks of the NeoForge extension interfaces that the engine calls.

### Modified Capabilities
- None in `openspec/specs/`. The M4 capabilities (`neoforge-registration`, `neoforge-menus`, `neoforge-capabilities`) are still in the change `neoforge-shaped-plugin-api`; this change adds new capabilities next to them and does not change their requirements.

## Impact

- `crates/pumpkin-data` (the dynamic content registry: new kinds and the generic table; no generated file changes), `tools/pumpkin-codegen` (the fluid template, the placement modifiers), `crates/pumpkin-core` (block entities, screens, data pack loaders, recipes, game rules, fluids, entity fluid interaction, world generation, chunk tickets, the configuration tasks), `crates/pumpkin-world`, `crates/pumpkin-inventory`, `crates/pumpkin-protocol`, `crates/ironpumpkin-mods` (the mod API of every new kind), the porting guide `docs/porting-from-neoforge/`, the inventory of `openspec/changes/neoforge-shaped-plugin-api/design.md`.
- Wire: new rows in the registry sync for NeoForge clients, and the NeoForge payloads `advanced_open_screen`, `advanced_container_set_data` and `recipe_content`. A vanilla client sees no change on a server without mod content.
- Saves: mod block entities, fluids, game rules and tickets save by name; a world keeps the data of a mod that is absent.
- Upstream merge cost: every extension follows the M2 pattern (a fallthrough after the generated lookup, one added arm or one call in an upstream file); the design lists the upstream files that each extension touches.
