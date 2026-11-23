# World generation, chunk tickets and issue #135: research for phase 6

Read-only research. Paths are relative to the repository root unless they start with `ref:`.
`ref:` is `target/scratch/ref/` (NeoForge 26.3.0.64-beta in `neoforge-src`, vanilla 26.3 in `minecraft-src`).
No design decision is made here. Each section gives options, cost and a recommendation.

Not verified: the text of issue #135 (the research rules forbid GitHub). Section 5 uses the title
given in the task.

## 1. How Pumpkin generates a world

### 1.1 Summary table

| Part | Where it lives | Data driven at runtime? |
|:--|:--|:--|
| Noise router, noise settings | `BaseNoiseRouters`, `NoiseSettings` (generated enums), `crates/pumpkin-data/src/generated/noise_router.rs` (1.8 MB), `noise_settings.rs` | No. Codegen from `assets/datapack` at build time |
| Biome source | `crates/pumpkin-world/src/biome/mod.rs:342` `ActiveBiomeSupplier::from_biome_source`; multi-noise tree from `assets/multi_noise_biome_tree.json` | No. Only the vanilla presets, `Fixed` and `Simple` of `BiomeSource` (`crates/pumpkin-world/src/world_info/mod.rs:233`) |
| Biomes | `crates/pumpkin-data/src/generated/biome.rs` (14 MB), struct `Biome` | No. Static array, id is `u8` |
| Surface rules | `MaterialRule` (generated, `generated/material_rule.rs`), picked in `VanillaGenerator::new_with_all_settings` (`crates/pumpkin-world/src/generation/generator/mod.rs:250`) | No |
| Carvers | `Biome.carvers: &'static [&'static CarverConfig]` (`generated/biome.rs:19`); code in `crates/pumpkin-world/src/generation/carver/` (`cave.rs`, `canyon.rs`) | No |
| Configured features | `enum ConfiguredFeature` (`crates/pumpkin-world/src/generation/feature/configured_features.rs:101`), table `CONFIGURED_FEATURES` (`:84`), built by generated `configured_features_generated.rs` (5663 lines) | No. Codegen `tools/pumpkin-codegen/src/configured_feature.rs` (2125 lines) turns vanilla JSON into Rust struct literals |
| Placed features, placements | `PLACED_FEATURES` (`feature/placed_features.rs:21`), `enum PlacementModifier` (`:168`), generated `placed_features_generated.rs` (404 KB), codegen `tools/pumpkin-codegen/src/placed_feature.rs` (861 lines) | No |
| Feature order and selection | `crates/pumpkin-world/src/generation/feature_order.rs:121` `select_features` | No. Sorted once per dimension from the static `Biome.features` |
| Feature step | `crates/pumpkin-world/src/generation/proto_chunk.rs:1432` `generate_features_and_structure`; loop at `:1490-1510` | No |
| Structures | `enum StructureKeys` (52 variants), `struct Structure`, `StructureSet` in `generated/structures.rs:9,241,446`; code per structure in `crates/pumpkin-world/src/generation/structure/structures/`; dispatch by `match key` in `structure/mod.rs:~55` | Partly. Jigsaw pools and NBT templates load at runtime (see 1.3) |
| Structure starts on disk | `crates/pumpkin-world/src/generation/proto_chunk_ironpumpkin.rs:20-28` writes the key `minecraft:{structure.to_name()}`; `structure/start.rs:56` reads it with `StructureKeys::from_name` | No |

The vanilla data pack folder `assets/datapack/` is gitignored (`.gitignore:113`). It is an input of
codegen only. The server binary does not read it at run time for generation.

### 1.2 What a data pack in the world or in a mod changes today

- `crates/pumpkin-core/src/data/datapack/dynamic_registry_loader.rs:12-190` lists the registries
  that the loader reads, among them `worldgen/biome`, `worldgen/feature`, `worldgen/placed_feature`,
  `worldgen/carver`, `worldgen/noise_settings`. `json_to_registry_nbt_bytes` (`:257`) converts each
  JSON file to NBT bytes. The only consumer is the registry sync to the client
  (`DynamicRegistry`, `dynamic_registry_manager.rs`). Generation never reads these bytes.
- A mod pack under `data/<ns>/worldgen/...` therefore reaches the client registry and nothing else.
  `crates/ironpumpkin-mods/src/data_maps/registries.rs:20-37` only lists the worldgen registries as
  "not synced" for data maps.
- Consequence: a `neoforge:add_features` JSON, a configured feature JSON and a placed feature JSON
  from a mod have no effect on terrain today.

### 1.3 The parts that already have a runtime table

- Jigsaw template pools: `register_dynamic_template_pool` and `register_dynamic_template_pool_json`
  (`crates/pumpkin-world/src/generation/structure/structures/jigsaw.rs:37-84`), filled by
  `crates/pumpkin-core/src/data/datapack/structure_loader.rs:120`.
- Structure NBT templates: `global_cache()` of `structure/template/cache.rs`, filled by
  `structure_loader.rs:13` `load_structures_from_dir`.
- The block content registry (M2): `crates/pumpkin-data/src/dynamic.rs`, ids appended after the
  vanilla ids. This is the pattern for "a registry that is a generated enum plus runtime entries".

### 1.4 The plugin hook `handle-generate-phase`

- WIT: `crates/pumpkin-plugin-wit/v0.2/plugin.wit:137` and `world.wit:1163-1196`
  (`generation-phase` is `biomes | noise | surface | features`; `chunk-buffer` has block, biome and
  fill setters).
- Host: `crates/pumpkin-wasm-host-v0_2/src/world.rs:1409` `set_chunk_generator` builds a
  `WasmChunkGenerator` and calls `level.set_world_gen(WorldGenerator::Custom(..))`. The generator
  implements `CustomChunkGenerator` (`crates/pumpkin-world/src/generation/generator/mod.rs:36-80`).
- The hook replaces the whole generator of one world (`WorldGenerator::Custom`). It does not add
  a step to the vanilla generator, and it does not add a feature to a vanilla biome. A custom
  generator returns no structures (`has_structure_piece_at` is false for `Custom`,
  `generator/mod.rs:112-123`; `set_structure_starts` defaults to a no-op, `:73`).
- The trait is `pub` in `pumpkin-world`, and `ironpumpkin-mods` re-exports `pumpkin_world`
  (`crates/ironpumpkin-mods/src/lib.rs:77`). A native mod can implement `CustomChunkGenerator`
  without Wasm. No native helper to install it exists (no `ModInit` method names it).

## 2. What the reference modpack needs

Demand (`openspec/changes/neoforge-shaped-plugin-api/`):

- `mapping-table.md:993` section 15 "World generation": 72 rows, `Heightmap` 58 mods,
  `Feature` 24, `GenerationStep` 21. `mapping-table.md:1161-1163` `BiomeModifier` gap.
  `mapping-table.md:1468-1493` per class (`PlacedFeature` 20, `FeatureConfiguration` 18,
  `FeaturePlaceContext` 18, `ConfiguredFeature` 13, `OreConfiguration` 8, `PlacementModifierType` 14,
  `RuleTest` 10, `PlacementFilter` 8, `BlockStateProvider` 5).
- `modpack-usage.md:880` `BiomeModifier` 14 mods; `:885` `TicketController` 13 mods;
  `:921` `RegisterTicketControllersEvent` 10 mods; `:929` `BiomeGenerationSettingsBuilder` 9 mods;
  `:376-377` structure mods (`structureessentials`, `structures_tweaker`, `respawningstructures`).
- `Heightmap` reads are already served by `get-top-block-y` (`mapping-table.md:1468`). The 58 mods
  mostly read heightmaps and do not generate.

What a mod needs, in order of mod count: (a) an ore or tree of a vanilla feature type, placed in
vanilla biomes by `neoforge:add_features`; (b) the removal of a vanilla feature
(`neoforge:remove_features`); (c) spawn changes (`add_spawns`, `remove_spawns`); (d) carvers
(`add_carvers`, rare); (e) a custom `Feature` class (Twilight Forest class: 1976 refs); (f) structure
modifiers (`structureessentials` 18 refs).

### 2.1 Vanilla feature types Pumpkin implements

- `ref:minecraft-src/net/minecraft/world/level/levelgen/feature/FeatureTypes.java:30-70` registers
  58 feature types. A script check (rg of `registry, "<name>"` against the arms
  `"minecraft:<name>"` of `tools/pumpkin-codegen/src/configured_feature.rs`) finds an arm for all 58.
  Examples: `ore` and `scattered_ore` (`:320`), `tree`, `random_patch` (`:416`), `simple_block` (`:305`),
  `sequence` (`:1006`), `template` (`:1171`), `speleothem` (`:534`), `spike` (`:745`). The Rust
  implementations are in `crates/pumpkin-world/src/generation/feature/features/` (65 files).
- Placement modifiers: vanilla registers 18 (`ref:.../placement/PlacementModifierTypes.java`).
  Pumpkin has 16 variants (`placed_features.rs:168-185`). The codegen has no arm for `cuboid`,
  `random_chance` and `randomly_selected`. The default arm of the placement converter is
  `PlacementModifier::Biome(..)` (`tools/pumpkin-codegen/src/placed_feature.rs:2123` region): an
  unknown modifier becomes a biome filter without an error. Whether any vanilla JSON uses the
  three types as placements: not verified.
- Block predicates, rule tests, block state providers, tree trunk, foliage and decorator types:
  covered by codegen arms (`configured_feature.rs:1415` `value_to_rule_test`, `:1204`
  `value_to_block_state_provider`, trunk and foliage near `:1675`, `:1755`). An unknown tree
  placer silently falls back (`configured_feature.rs:1675` straight trunk, `:1766` blob foliage).

### 2.2 Can a mod configured feature of a vanilla type run without new Rust code?

No. The Rust code for the types exists, but there is no path from a mod JSON to it:

1. The only JSON to struct converters are the codegen functions. They emit `TokenStream` Rust
   source, and they read only `assets/datapack/data/minecraft/worldgen/feature`
   (`configured_feature.rs:13-37`). They do not run in the server binary.
2. The tables are keyed by generated enums: `HashMap<pumpkin_data::configured_feature::ConfiguredFeature, ConfiguredFeature>`
   (`configured_features.rs:84`) and `HashMap<pumpkin_data::placed_feature::PlacedFeature, PlacedFeature>`
   (`placed_features.rs:21`). A mod id has no variant.
3. `Biome.features` is `&'static [&'static [PlacedFeature]]` (`generated/biome.rs:20`). A mod
   feature cannot be added to a biome.
4. `Biome` ids are `u8` in the chunk (`flat_biome_map`), and `Biome::from_id` is a static lookup
   (`generated/biome.rs:9580`). A mod biome has no id (a memory of the biome reader says an unknown
   biome name loads as id 0 or plains).

### 2.3 The NeoForge side to follow

`ref:neoforge-src/net/neoforged/neoforge/common/world/BiomeModifiers.java` has 8 record types:
`add_features` (`:47`), `remove_features` (`:79`), `add_spawns` (`:148`), `remove_spawns` (`:191`),
`add_carvers` (`:222`), `remove_carvers` (`:252`), `add_spawn_costs` (`:286`), `remove_spawn_costs`
(`:318`). A modifier runs in `BiomeModifier.Phase` order (`modpack-usage.md:894`, 13 mods) and edits a
`BiomeGenerationSettingsBuilder` copy of each biome. NeoForge also shifts the feature index of
later vanilla features when a modifier adds a feature, because the feature sorter runs on the
modified biomes. Pumpkin has the same index dependence: the decorator seed uses `global_index`
(`proto_chunk.rs:1495`). A mod feature in the sorted list changes the seed of later features in the
same step, as in NeoForge. No parity problem arises while no modifier is active.

## 3. Registration API: options and cost

Shared prerequisite for every option that adds a feature, a placement or a structure: the tables
need a key that is not a generated enum. Without it nothing below works.

### 3.1 Key type for features and structures

| Option | Change | Files | Upstream merge cost | Risk |
|:--|:--|:--|:--|:--|
| K1. Keep the enums, add an overlay table keyed by id string | `PLACED_FEATURES` stays. A second `DashMap<Arc<str>, Arc<PlacedFeature>>` holds mod entries. Lookups in the 3 reader sites try the enum first, then the overlay | `placed_features.rs`, `configured_features.rs`, `feature_order.rs` (3 `.features` readers at `feature_order.rs:153,175,178`, `placed_features.rs:524`), `proto_chunk.rs:1490-1495` | Low: hand files `placed_features.rs` (663 lines), `configured_features.rs` (585), `feature_order.rs` (296) change in a few lines. No generated file changes | Low for vanilla speed: one extra branch per feature, none per block |
| K2. New key type `FeatureKey { Vanilla(PlacedFeature), Mod(u32) }` | Replace `PlacedFeature` in the 17 sites of `placed_features.rs`, 15 of `feature_order.rs` and in the callers of 40 feature files (`feature_name` parameter) | about 25 files | High: `feature_name` is a parameter of nearly every `generate` in `features/` (65 files); every upstream feature change conflicts | Medium |
| K3. Make the codegen emit ids instead of enums | Changes `tools/pumpkin-codegen` and `pumpkin-data` generated output | codegen plus all consumers | Very high: all generated files differ on each merge | High |

Recommendation: K1. The seed of a feature uses its position in the sorted list (`global_index`),
not its name, so an overlay key does not change vanilla seeds. The `feature_name` parameter is
only used by the biome check (`BiomePlacementModifier`, `placed_features.rs:514-531`) and by a few
features for nested lookups; K1 passes an overlay key there through one new small enum or a
`&str`. This must be checked per feature file before a design is final (not measured here).

### 3.2 Options for a mod configured feature, placement and feature type

| Option | What the mod writes | Cost | Upstream merge cost | Risk |
|:--|:--|:--|:--|:--|
| F1. Native Rust values | `cx.register_configured_feature(id, ConfiguredFeature::Ore(OreFeature { .. }))` and `register_placed_feature(id, PlacedFeature { feature, placement: vec![..] })`. All structs and enums are `pub` in `pumpkin-world`. No JSON | Overlay of K1 plus 2 `ModInit` methods. No parser. A mod author must build the structs by hand (an ore has `OreTarget` with `RuleTest` and a block state) | Low (K1) | Low |
| F2. Runtime JSON interpreter for vanilla types | The mod ships `data/<ns>/worldgen/configured_feature/*.json` and `placed_feature/*.json`, as in NeoForge. The server reads them | A second copy of the 2125 + 861 lines of codegen, as a `serde` parser into the same structs, or a refactor of the codegen to share the conversion. Minimum subset for the modpack: `ore`, `scattered_ore`, `simple_block`, `random_patch`, `tree`, `random_selector`, `weighted_random_selector`, plus the placements `count`, `in_square`, `height_range`, `heightmap`, `rarity_filter`, `biome`, `block_predicate_filter`, `environment_scan`, plus `rule_test`, `block_state_provider`, `block_predicate` | New files in `pumpkin-world` (bounded module), no edit to upstream files except the K1 sites. Drift risk: upstream changes to the codegen arms need a matching change in the parser | Medium. A parser bug changes terrain. Unknown type must be an error, not a silent fallback |
| F3. Build-time codegen of mod packs | The pack build runs `pumpkin-codegen` over the mod folders and links the generated `.rs` | `pumpkin-codegen` has hard-coded `../../assets/datapack` paths (`configured_feature.rs:13`) and emits `crate::generation::...` paths, so the output only compiles inside `pumpkin-world`. Needs a parameterized input and an `$crate`-style output | Medium: touches `tools/pumpkin-codegen` (all files conflict when upstream changes the generator) | Medium. A `/reload` cannot change generated code |
| F4. Mod type of feature (a `Feature` subclass) | `cx.register_feature_type(id, impl FeatureType)` with a `trait FeatureType { fn place(&self, ctx: &mut FeaturePlaceContext) -> bool }` | Needs a variant in `enum ConfiguredFeature` such as `Custom(Arc<dyn ModFeature>)`: one new arm in the 100-line `match` of `generate` (`configured_features.rs:~170-480`). The `GenerationCache` generic (`proto_chunk.rs:97`) makes the trait generic over `T`; use a small object-safe facade (`dyn WorldgenAccess`) | One arm in a hand file: low | Medium: a facade over `GenerationCache` costs a virtual call per block set inside the feature |
| F5. Placement modifier type | `register_placement_modifier_type(id, impl PlacementFilter)` | New variant `PlacementModifier::Custom(Arc<dyn ..>)` plus one arm in `get_positions` (`placed_features.rs:187-213`) | Low | Low |

Recommendation: F1 first (it unblocks every native mod and tests K1 alone), then F2 for the
vanilla types the modpack uses (ore, tree, random_patch, simple_block with their placements), as a
new module that is a runtime counterpart of the codegen. Reasons: the modpack data comes as JSON
(NeoForge mods ship it, `ModDataPack` already carries it, `crates/ironpumpkin-mods/src/data_pack.rs:1-60`), and a port should keep its JSON. F3 gives no
reload and ties the pack build to the codegen tool. F4 and F5 follow when a ported mod needs a
custom `Feature` or `PlacementModifierType` (24 and 14 mods in `mapping-table.md:1470,1476`).

### 3.3 Biome modifiers

`Biome` is `&'static` and used by `&'static Biome` in 34 call sites of `Biome::from_id` and
`from_name`. Changing the struct is not an option for the merge (14 MB generated file). Options:

| Option | Change | Merge cost | Risk |
|:--|:--|:--|:--|
| B1. Overlay table per biome id | A `static BIOME_FEATURE_OVERLAY: RwLock<Vec<BiomeOverlay>>` indexed by `u8` biome id, applied at `feature_order.rs:153,175,178` and `placed_features.rs:524` (the only 4 readers of `Biome.features`). `remove_features` is a removal set in the overlay. The data pack loader evaluates the `neoforge:biome_modifier` JSON once at start and fills the overlay | Low: 2 hand files | Low. The sorted order (`OVERWORLD_FEATURES_PER_STEP`, `feature_order.rs:106-117`) is built once with `LazyLock`, so the overlay must be frozen before the first chunk generates |
| B2. Rebuild a modified `Biome` copy | Leak a `Box<Biome>` per modified biome and return it from `Biome::from_id` | `from_id` is generated: high | Medium |
| B3. Source patch of generated `biome.rs` | Per pack, the pack build patches the data | Highest | High |

Spawns and spawn costs (`add_spawns`, `remove_spawns`, `add_spawn_costs`): readers are
`crates/pumpkin-core/src/world/natural_spawner.rs:396,423,459,499,590,600,953` (about 7 sites).
The same overlay pattern fits: one function `effective_spawners(biome, group)`.
Carvers (`add_carvers`, `remove_carvers`): `Biome.carvers` is read in `generator/mod.rs:275-290`
(`uniform_carvers`, an optimization that assumes one carver list per dimension) and in
`proto_chunk.rs:1076` `step_to_carvers`. A mod carver needs a `CarverConfig` value; the type is
`pub` in `generated/carver.rs` and holds `&'static` data, so a mod builds it with leaked statics.
Rare in the modpack: recommend leaving `add_carvers` out of the first wave.

Recommendation: B1. A mod JSON `neoforge:add_features` reads biome ids by name (vanilla names
resolve through `Biome::from_name`; tags `#minecraft:is_overworld` resolve through
`pumpkin_data::tag`, which `get_tag_ids(RegistryKey::WorldgenBiome, ..)` already serves in
`proto_chunk.rs:1565-1570`).

### 3.4 Mod biomes and mod structures (more cost)

- A mod biome needs an id above the vanilla `u8` range, the section biome palette (`BiomePalette`
  holds `u8`, memory note from the biome reader fix), the registry sync entry, `Biome.carvers`,
  `spawners`, and a source (multi-noise parameters). No part of this exists. The biome id is `u8` (`Biome::from_id(u8)`), so mod biomes share the space above the vanilla ids
  (the vanilla count was not measured here). Out of scope for the first wave: the 14 mods that use
  `BiomeModifier` mostly edit vanilla biomes (`modpack-usage.md:880`, no per-mod proof read here).
- A mod structure needs a key for `structure_starts` (`StructureKeys` is a generated enum),
  the save name (`proto_chunk_ironpumpkin.rs:20-28`, writes `minecraft:` always), a placement
  (`StructureSet` is generated too, `structures.rs:241`), and a generator. A jigsaw-only mod
  structure can reuse `JigsawGenerator` (`structure/structures/jigsaw.rs`) with a runtime
  `start_pool`. Options: S1 overlay of `StructureKeys` with `StructureId { Vanilla(StructureKeys), Mod(u32) }`
  across `ProtoChunk.structure_starts`, `structure_references`, `start.rs`, `placement.rs` and
  saves (about 8 files, hot path of `generate_structure_step` at `proto_chunk.rs:1515`), medium
  merge cost; S2 do not support mod structures, and give a source patch route (task 2.1.2).
  Recommendation: S2 for the first wave. The structure mods of the modpack (`structure_expansion`,
  `structureessentials`, `structures_tweaker`) edit vanilla structures (`modpack-usage.md:582-584`),
  which a structure modifier needs only after S1. The data pack structure templates and jigsaw pools
  that a mod ships already load at runtime (1.3).
- Save format risk of S1: a mod structure start in a chunk would write `minecraft:<name>`; use the
  namespace of the mod. A world with a mod structure that loses the mod must keep the start data
  unread, as vanilla does (the reader at `start.rs:56` returns `None` for an unknown name: verify
  that the unknown start is not dropped on the next save; not measured).

### 3.5 The existing hook as a fallback

`CustomChunkGenerator` (1.4) lets a native mod generate a whole world dimension of its own
(void worlds, flat worlds with a mod layer). It does not cover "add an ore to the overworld".
Do not use it for biome modifiers.

## 4. Chunk tickets and forced chunks

### 4.1 What pumpkin-world has

- `crates/pumpkin-world/src/chunk_system/chunk_loading.rs:9-17` `ChunkLoading`: `ticket: HashMapType<ChunkPos, Vec<i8>>`
  with a source comment `TODO lifetime & id` (`:14`). A ticket is a level number per position.
  There is no type, no owner and no expiry.
- Levels: `FULL_CHUNK_LEVEL = 43`, `MAX_LEVEL = 49` (`:21-22`). `add_ticket` and `remove_ticket`
  (`:192`, `:232`) propagate a level with a Chebyshev ring (`pumpkin_data::chunk_view_lut`). The
  stage of a chunk comes from `StagedChunkEnum::level_to_stage` (`chunk_state.rs:105`).
- Producers: players (`crates/pumpkin-core/src/world/chunker.rs:118-131`,
  `entity/player.rs:1532-1535`), `FetchTicket` (`crates/pumpkin-world/src/level.rs:134-160`),
  the Wasm `set-force-loaded` (`crates/pumpkin-wasm-host-v0_2/src/world.rs:783,1961`), the
  `/forceload` command (`crates/pumpkin-core/src/command/commands/forceload.rs`), the gametest
  manager (`server_test_manager.rs:386,419`).
- Forced chunks: `World.forced_chunks: Mutex<FxHashSet<Vector2<i32>>>`
  (`crates/pumpkin-core/src/world/mod.rs:298`). It is a set: two owners that force the same chunk
  collapse to one, and the first `remove` unloads it for both. It feeds `update_active_chunks`
  (`world/mod.rs:476-527`, `active_chunks.rs:117` `sync_forced_chunks`) so the chunk ticks.
- Save: nothing. A search for `forced` in `pumpkin-world` and `world_info` finds no read or write.
  Forced chunks do not survive a restart. This is also true for `/forceload` today.
- WIT: `is-chunk-force-loaded` and `set-force-loaded` (`crates/pumpkin-plugin-wit/v0.2/world.wit:726-731,928-931`).
  No owner id.

### 4.2 What NeoForge needs

- `ref:neoforge-src/net/neoforged/neoforge/common/world/chunk/RegisterTicketControllersEvent.java`:
  a mod-bus event with `register(TicketController)`. The id must be unique
  (`ForcedChunkManager.java:~35-45`, `IllegalArgumentException`).
- `TicketController.java:9-70`: `record(Identifier id, @Nullable LoadingValidationCallback callback)`
  and three `forceChunk` overloads: owner `BlockPos`, owner `Entity` (uses its UUID), owner `UUID`;
  each takes `chunkX, chunkZ, add, forceNaturalSpawning` (the entity variants name the last argument
  `ticking`).
- `LoadingValidationCallback.java`: `validateTickets(ServerLevel, TicketHelper)`. It runs when a
  level loads, with the saved tickets of the controller, so the mod can remove stale ones
  (`TicketHelper`: `getBlockTickets`, `getEntityTickets`, `removeAllTickets(owner)`, `removeTicket(owner, chunk, natural)`).
- `ForcedChunkManager.java`: owners are `TicketOwner(id, owner)`. Two sets per type:
  `normal` (loads and ticks) and `naturalSpawning`. Tickets are saved in the vanilla
  `TicketStorage` (`ref:minecraft-src/net/minecraft/world/level/TicketStorage.java:42`
  `chunk_tickets`) under the extra key `neo_ticket_data` (`ForcedChunkManager.java`
  `defineExtraStorageParams`: list of `{controller, block_chunks, entity_chunks}`). On load,
  tickets are "deactivated" until `activateAllDeactivatedTickets` runs the validation callbacks
  and reinstates the rest.
- `hasForcedChunks(level)` keeps a level ticking with no player.
- Mod demand: `ftbchunks` (`modpack-usage.md:367`) and 12 others use `TicketController`; the
  `ChunkWatchEvent` (11 mods) and `ChunkEvent.Load`/`Unload` are separate rows.

### 4.3 Gaps in Pumpkin

| Need | Pumpkin today | Gap |
|:--|:--|:--|
| Controller registry | none | A `register_ticket_controller(id, callback)` in the `ModInit` and a freeze at the end of init |
| Owner per ticket | none (`Vec<i8>` per chunk) | A store `(controller id, owner key) -> set of (chunk, natural)` per world |
| Block and entity owners | none | Owner key is `BlockPos` or `Uuid` |
| Natural spawning flag | none | `update_active_chunks` has no "spawn only" notion; check `natural_spawner.rs` for the chunk set it uses (not read here) |
| Save of forced chunks | none | A new file in the world folder. Vanilla uses `data/chunk_tickets.dat` (SavedData `chunk_tickets`); IronPumpkin has no `SavedData` layer for it (not searched beyond `forced`) |
| Validation callback at load | none | Run when the world loads, before tickets activate |
| Ticking with no player | `forced_chunks` feeds `update_active_chunks` | Works for forced chunks; verify that the world loop does not pause a world with no players when a forced chunk exists (not read here) |
| Level of a forced ticket | `FULL_CHUNK_LEVEL` (43), same as vanilla `FORCED_TICKET_LEVEL` | None |

### 4.4 Options

| Option | Change | Cost | Merge cost |
|:--|:--|:--|:--|
| T1. Owned-ticket store above `forced_chunks` | New module `crates/pumpkin-core/src/world/ticket_owners.rs` (or in `ironpumpkin-mods`) with `HashMap<ChunkPos, HashSet<(ControllerId, Owner, Natural)>>`. `forced_chunks` becomes the union. The store calls the existing `add_force_ticket`/`remove_force_ticket` only on the first and last owner of a chunk. A new file `forced_chunks.nbt` (or `chunk_tickets.dat` with the vanilla codec plus `neo_ticket_data`) in the dimension folder, written on world save and on a change, read on world load | One new module, 3 edit sites (`world/mod.rs:298,463,481`, the load path, the Wasm host `world.rs:783,1961`, `forceload.rs`) | Low to medium: `world/mod.rs` has 9000+ lines and is an upstream hot file; keep the edits to 3 lines and put the logic in the new module |
| T2. Replace `forced_chunks` with the store | Same as T1 but remove the set | More edits in `world/mod.rs` | Medium |
| T3. Wasm only, owner as plugin id | Add an owner argument to the WIT call | WIT is additive only, so a new function `set-force-loaded-owned` | Low but gives no native `TicketController` |

Recommendation: T1. Save in the vanilla format `chunk_tickets` with the NeoForge extra key, so a
world moves between IronPumpkin and a NeoForge server without loss of the forced set (in the same
spirit as the world data format decisions in `design.md:Decided` "World data format").
Reason to start with save: without it every `ftbchunks` claim is lost on restart, which is the
main use of the API.

Order of work: (1) save of the plain `/forceload` set in vanilla `chunk_tickets` format (an upstream
candidate: Pumpkin loses forced chunks on restart); (2) the owner store; (3) the controller
registration and the validation callback; (4) `TicketHelper`.

Performance guardrail: the store is touched only on force and unforce, never per tick.
`ChunkLoading.ticket` is already per-chunk `Vec<i8>`; do not extend it.

## 5. Issue #135: structure template palette drops the custom properties of mod blocks

Title as given: "structure template palette drops the custom properties of mod blocks".

### 5.1 Cause (read in the code)

- `PaletteEntry.properties` is `Vec<pumpkin_data::Property>`
  (`crates/pumpkin-world/src/generation/structure/template/structure_template.rs:234`). `Property` is
  the generated enum of vanilla property kinds.
- Both parsers build it with `Property::from_str_pair(key, value)` and drop the `None` results:
  `from_string` (`structure_template.rs:305-313`) and `from_nbt_compound` (`:381-396`). A mod block
  property with no `Property` variant (a custom `lit`-like enum, an int `age` above the vanilla
  range, `BlockPropertyDefinition` in `crates/pumpkin-data/src/dynamic.rs:81`) returns `None`
  and is dropped silently.
- `to_nbt_compound` (`:358-368`) writes only the typed list, so a re-saved template also loses them.
- Resolution: `BlockStateResolver::resolve` (`template/block_state_resolver.rs:45-90`) calls
  `block.from_properties(&entry.properties)`. For a registered block there is a string path:
  `Block::from_string_properties` and `Block::with_properties` (`dynamic.rs:1864,1879`) and the
  chunk palette reader `palette_entry_to_state` (`crates/pumpkin-world/src/block/state_nbt.rs:107`),
  which already keeps these properties as strings. The template path does not use them.
- Pre-resolved ids: `PaletteEntry.state_id` (`:236`) is set only for static palettes
  (`from_static`, `:281`) and `air`.

### 5.2 Where the fix fits (options)

| Option | Change | Merge cost | Risk |
|:--|:--|:--|:--|
| P1. Add `string_properties: Vec<(String, String)>` to `PaletteEntry`, filled for every key (typed or not). `resolve` calls `Block::from_string_properties(..)`, which already branches on registered content inside `pumpkin-data` (`is_custom_block` is `pub(crate)` there, so `pumpkin-world` cannot call it), and the typed path stays for blocks whose properties all have a `Property` variant | `structure_template.rs` (3 sites: struct, `from_string`, `from_nbt_compound`, `to_nbt_compound`), `block_state_resolver.rs` (1 branch) | Low to medium: both are hand files that upstream edits rarely (they carry IronPumpkin edits for 26.3 keys already, `:373`) | Low. Vanilla blocks keep the exact old path. Rotation and mirror of a custom property with a vanilla name (`facing`) still go through `transform_block_properties` on the typed list; a custom-only property is not rotated, which matches NeoForge (a mod block rotates through `BlockState.rotate`, not through the template system, for unknown keys; not verified in `ref:`) |
| P2. Resolve to a `BlockStateId` at parse time with `state_nbt::block_state_from_compound` (the chunk reader) and keep the raw compound for save | Moves resolution before rotation; a rotated palette entry needs the typed list anyway | Medium | Higher: rotation and mirror for a mod block would need the registered `Block::rotate` hook |
| P3. Replace `Vec<Property>` by `Vec<(String, String)>` everywhere | `transform_block_properties`, 40+ uses in the tests of the file | High | Medium |

Recommendation: P1. It is the smallest change that keeps every vanilla path identical, and it
follows the same string-property route as the chunk palette (`state_nbt.rs`) and the typed fix
for custom blocks already in the tree
(`crates/pumpkin-data/tests/custom_block_typed_properties.rs`).
Check the order dependence: `from_nbt_compound` iterates `child_tags` of the compound; keep the
compound order in `string_properties` and sort on resolve (the value is set by name, so order has
no effect).

Test to leave behind: a template NBT with a registered mod block that has a bool and an int property
(`BlockPropertyDefinition::bool`, `::int`), parsed by `from_nbt_compound`, placed by `resolve`, and
checked with `Block::string_properties` on the resulting state; also a round trip through
`to_nbt_compound`. The M2 registry helpers in `crates/pumpkin-world/tests/custom_block_int_property.rs`
show how a test registers a block.

Pre-existing note found while reading: `from_string` splits a state string on `=` and drops
every pair without a value; a mod property value that contains `=` would be cut. No such value
exists in vanilla. Mention only.

## 6. Summary of recommendations

| Point | Recommendation | Main reason |
|:--|:--|:--|
| Key for mod features | K1: overlay table beside the generated enums | No generated file, 4 reader sites |
| Mod feature of a vanilla type | F1 native values first, then F2 runtime JSON interpreter for ore, tree, random_patch, simple_block and their placements | The Rust types exist; the missing part is the parser and the key |
| Mod `Feature` and `PlacementModifierType` classes | F4 and F5, after F1 | 24 and 14 mods; need one new enum arm each |
| Biome modifiers | B1: overlay per biome id, frozen before the first chunk | `Biome` is a 14 MB generated static |
| Mod biomes, mod structures | Out of the first wave; source-patch route | `u8` ids, `StructureKeys` enum, save keys |
| Tickets | T1 owner store, save in vanilla `chunk_tickets` format with `neo_ticket_data` | Forced chunks are not saved today; `ftbchunks` needs it |
| #135 | P1: string properties in `PaletteEntry` | Smallest change; same route as the chunk palette |

Open questions that this research did not close:
- Whether `feature_name` (the placed feature enum) is needed in each of the 65 feature files beyond
  the biome check (decides the cost of K1 against K2).
- Whether the world loop ticks a world with no player when only a forced chunk exists.
- Whether an unknown structure start is kept on save (`structure/start.rs:56`).
- The text of issue #135 and any acceptance criteria in it.
