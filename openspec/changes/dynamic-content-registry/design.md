# Design

## Context

See proposal.md for the motivation. This design carries the analysis of the static data model and the id-space and lifetime decision (#16) under Decisions.

### The static data model (Pumpkin baseline, upstream `f1c0871f4`)

**Registries and generated data.** `tools/pumpkin-codegen` (`block.rs`, `item.rs`, `entity_type.rs`, `registry.rs`, `tag.rs`, ...) turns `assets/` into `crates/pumpkin-data/src/generated/`. The model is static and vanilla-shaped: `Block` (`crates/pumpkin-data/src/blocks.rs`) has `id: BlockId(u16)`, `name: &'static str`, `states: &'static [BlockState]`; lookups `Block::from_registry_key` / `from_id` / `from_state_id` (`generated/block.rs`), `Item::from_registry_key` / `from_id(u16)`, `EntityType::from_name`, all returning `&'static`. `BlockStateId(u16)` is the vanilla network ID written directly into chunk palettes (`crates/pumpkin-world/src/chunk/palette.rs`, `BlockPalette = PalettedContainer<BlockStateId, 16>`). Synced dynamic registries come from `pumpkin_data::registry::Registry::get_synced`. This `&'static` model is the blocker the owner hit: no ID space for content absent from `assets/`.

Consequences for custom content:

- `Block`, `Item` and `EntityType` lookups return `&'static` references into generated tables. A value created at runtime has no `'static` home unless it is leaked or the lookups change their return type.
- Ids are u16 and dense. A custom id must sit above the generated maxima. Chunks, item stacks and entity ids are saved by name, but three save sites store numeric ids today (see the boundary table in the decision). After those sites move to names, ids only have to be stable within one server run.
- `BlockStateId` is the palette key in `crates/pumpkin-world/src/chunk/palette.rs` and the key of `Block::from_state_id`. A custom block needs a state range laid out the way pumpkin-codegen lays out vanilla states, so the same lookups resolve both.
- Saves must use namespaced names plus properties, not numeric ids, so a world stays readable when ids are reassigned or a plugin is missing.
- The existing server-side dynamic registries are the pattern to copy: custom enchantments (`crates/pumpkin-core/src/server/enchantment.rs`), custom recipes, and datapack registries (`crates/pumpkin-core/src/data/datapack/dynamic_registry_loader.rs`, merged into `CRegistryData` in `net/java/login/known_packs.rs`).

Risk from the plan: **Static data model.** Phase 2 touches `pumpkin-data`, palettes and many `&'static` call sites; upstream rebases will conflict there. Keep the registry as a fallthrough layer and never edit generated files.

## Goals / Non-Goals

**Goals:**

- One registry that owns custom blocks, items and entity types and answers the lookups the generated tables cannot.
- Existing callers of the generated lookups keep working unchanged.
- No hand edit to `crates/pumpkin-data/src/generated/`. Lookup changes go into the `tools/pumpkin-codegen` templates and the files are regenerated.

**Non-Goals:**

- Sending custom ids to clients (phase 3 registry sync).
- Client-side assets: models, textures, GUIs.

## Decisions

### Decision: id spaces and lifetimes

Decided in #16. Measured on `master` at `3bff38f75` (upstream base `f1c0871f4`).

**Chosen approach: a frozen hybrid.**

- Generated blocks, block states, items and entity types stay where they are: `&'static` values in the generated tables, unchanged.
- Custom content is collected in a registration phase that ends before the first world loads. At the end of the phase the registry freezes: it builds every custom `Block`, `BlockState`, `Item` and `EntityType` once, leaks them with `Box::leak`, and installs the leaked tables in a `OnceLock` in a new hand-written module `crates/pumpkin-data/src/dynamic.rs`.
- The generated lookups fall through to those tables for ids at or above the generated count, and for names the generated maps do not know. Every lookup keeps its signature and keeps returning `&'static`.
- Custom ids are dense ranges that start at the generated count of the running build. The registry allocates them at freeze time, in lexicographic order of the namespaced name, so they do not depend on plugin load order.
- Numeric ids are not persisted. Saves are name-based; the three save sites that write numeric ids today move to names. The world directory gets a content manifest with names, property schemas and display mappings. The manifest exists to rebuild placeholders for content whose plugin is missing.

#### Measurements

All counts use this command form, summed per crate, over `crates/` and `tools/`, excluding the generated files:

```bash
rg --count-matches -g '*.rs' -g '!crates/pumpkin-data/src/generated/**' -e '<regex>' crates tools
```

Lookups that keep their signature (none of these call sites change):

| Regex | Sites | Files | Per crate |
|:--|--:|--:|:--|
| `\bBlock::from_id\(` | 45 | 15 | world 14, wasm-host-v0_1 12, wasm-host-v0_2 12, core 4, data 3 |
| `\bBlock::from_state_id\(` | 111 | 57 | core 61, world 26, data 8, v0_1 7, v0_2 7, codegen 2 |
| `\bBlock::from_registry_key\(` | 15 | 5 | world 12, data 1, v0_1 1, v0_2 1 |
| `\bBlock::from_name\(` | 33 | 15 | world 18, core 4, v0_1 3, v0_2 3, command 2, data 1, gametest 1, plugin-api 1 |
| `\bBlock::from_item_id\(` | 5 | 4 | core 5 |
| `\bBlockState::from_id\(` | 168 | 82 | world 115, core 36, data 7, v0_1 5, v0_2 5 |
| `\bBlockState::from_id_with_block\(` | 9 | 7 | core 7, world 2 |
| `\bBlockId::from_state_id\(` | 7 | 7 | world 2, codegen 1, core 1, data 1, v0_1 1, v0_2 1 |
| `\bBlockStateId::new\(` | 22 | 7 | v0_1 8, v0_2 8, core 3, world 2, codegen 1 |
| `\bBlockStateId::new_or_air\(` | 36 | 9 | v0_1 11, v0_2 11, world 8, protocol 3, data 2, core 1 |
| `\bBlockId::new\(` | 20 | 6 | v0_1 8, v0_2 8, data 2, codegen 1, world 1 |
| `\bBlockId::new_or_air\(` | 13 | 7 | core 4, v0_1 4, v0_2 4, codegen 1 |
| `\.to_block\(\)` | 45 | 22 | world 22, core 19, codegen 2, data 2 |
| `\.to_state\(\)` | 129 | 56 | world 110, core 14, data 3, v0_1 1, v0_2 1 |
| `\.to_block_id\(\)` | 187 | 69 | world 178, core 9 |
| `\bItem::from_id\(` | 15 | 9 | protocol 10, core 4, data 1 |
| `\bItem::from_registry_key\(` | 40 | 18 | protocol 16, core 12, command 3, data 3, inventory 3, plugin-api 1, v0_1 1, v0_2 1 |
| `\bEntityType::from_raw\(` | 1 | 1 | codegen 1 |
| `\bEntityType::from_name\(` | 12 | 9 | core 7, codegen 1, command 1, protocol 1, v0_1 1, v0_2 1 |

Total: 913 lookup call sites. The three method-name patterns (`.to_block()`, `.to_state()`, `.to_block_id()`) can also match other types and are upper bounds.

`&'static` surface that the Arc option would have to change:

| Regex | Sites | Files |
|:--|--:|--:|
| `&'static Block\b` | 132 | 54 |
| `&'static BlockState\b` | 192 | 64 |
| `&'static Item\b` | 58 | 25 |
| `&'static EntityType\b` | 74 | 32 |
| `&Block::[A-Z][A-Z0-9_]+\b` (generated constants used as `&'static`) | 966 | 182 |
| `&Item::[A-Z][A-Z0-9_]+\b` | 587 | 107 |
| `&EntityType::[A-Z][A-Z0-9_]+\b` | 488 | 158 |
| `\.get_block\(` / `\.get_block_state\(` / `\.get_block_and_state\(` (return `&'static`) | 303 / 393 / 148 | 132 / 167 / 68 |
| `\bItemStack\b` (holds `item: &'static Item`) | 1,907 | 276 |

Union of the `&'static` signatures, the constant references, the lookups and the world getters: 3,973 matches in 569 files, out of 2,742 non-generated `.rs` files.

Other facts measured:

- The generated lookups are `const fn` and index with `unsafe { std::hint::assert_unchecked(id < COUNT) }` (`BlockState::from_id`, `Block::from_id`, `BlockId::from_state_id`, `BlockState::to_be_network_id`; template in `tools/pumpkin-codegen/src/block.rs` lines 1359-1524). A custom id that reaches them unchanged is undefined behaviour, so the fallthrough must go into the template, not around it.
- Functions that lose `const`: 21.
  - Generated, through the templates (9): `BlockState::from_id`, `BlockState::from_id_with_block`, `BlockState::to_be_network_id`, `Block::from_id`, `Block::from_state_id`, `Block::from_item_id`, `BlockId::from_state_id` (`tools/pumpkin-codegen/src/block.rs`), `Item::from_id` (`item.rs` line 2002), `EntityType::from_raw` (`entity_type.rs` line 430).
  - Hand-written (12): `BlockStateId::new`, `new_or_air`, `to_state`, `to_block_id`, `to_block`, `is_solid_render`, `can_occlude`, `has_analog_output_signal` (`crates/pumpkin-data/src/block_state.rs`); `BlockId::new`, `new_or_air`, `to_block` (`crates/pumpkin-data/src/blocks.rs`); `test` (`crates/pumpkin-world/src/generation/rule/block_match.rs`).
  - Stays `const`: the generated `get_potted_item` (template `tools/pumpkin-codegen/src/flower_pot_transformations.rs` lines 25-30) switches from `BlockId::new_or_air` to the vanilla-only const constructor. The `BlockState` methods `can_occlude` and `is_solid_render` read fields of `&BlockState` and keep `const`, so `lighting/engine.rs` and `mushroom_plant.rs` need no change.
  - No `const` or `static` item calls a function that loses `const` (rg for `const|static NAME: ... = ...from_id|to_state()...`: 0 matches), so nothing else breaks.
- `BlockId::new` and `BlockStateId::new` are used 39,628 times in generated const items: 39,581 in `generated/block.rs` and 47 in `generated/fluid.rs`. Two `ToTokens` lines emit all of them (`tools/pumpkin-codegen/src/block.rs` lines 570 and 751).
- `Block::from_properties` ends in `_ => panic!("Invalid props")` and `Block::properties` in `_ => return None`. A custom block with properties would panic on the first palette load.
- Vanilla state layout, checked against `assets/blocks.json` and `assets/properties.json`: block ids are 0..1285 in list order; each block's states are contiguous and follow the previous block's last state; all 613 blocks with more than one property list them in name order. The state index is mixed radix with the last property varying fastest, `true` before `false`, integers ascending, enum values in declaration order (oak_stairs default `facing=north,half=bottom,shape=straight,waterlogged=false` is index 11).

Where ids cross a boundary:

| Boundary | Path | Form today | Change |
|:--|:--|:--|:--|
| Chunk palette in memory | `crates/pumpkin-world/src/chunk/palette.rs` (`BlockPalette = PalettedContainer<BlockStateId, 16>`) | `BlockStateId` | none: custom ids are valid `BlockStateId`s |
| Chunk save | `crates/pumpkin-world/src/chunk/format/mod.rs` line 538-560 | `Name` + `Properties` per palette entry; `Name` forced to `minecraft:` unless it starts with it | write the namespaced name as is |
| Chunk load | same file, `extract_u16_array` line 106 -> `BlockStateResolver::resolve` (`generation/structure/template/block_state_resolver.rs`) | name lookup; unknown name -> air, warning per palette entry | custom names resolve through the fallthrough; unknown names warn once per name |
| Scheduled ticks | `chunk/format/mod.rs` lines 187, 613, 626; `tick/mod.rs` line 121 | `"i"` = resource location via `ToResourceLocation for &'static Block` (`blocks.rs` line 119, hardcodes `minecraft:`) | namespace-aware `to_resource_location` |
| Item stack NBT | `crates/pumpkin-data/src/item_stack/mod.rs` lines 803-833 | `"id"` string, written as `minecraft:{registry_key}` | namespace-aware write; read already falls through |
| Entity NBT | `crates/pumpkin-core/src/world/mod.rs` lines 4281-4290 | `"id"` string; unknown type -> warning, compound dropped | keep the compound for placeholder types |
| Chunk data packet | `crates/pumpkin-core/src/net/java/chunk_data/v1_18.rs` line 96 -> `BlockPalette::convert_network` | raw state ids, direct palette 16 bits (`BLOCK_NETWORK_MAX_BITS`) | map custom ids to the display state |
| Block update packets | `CBlockUpdate::new` (9 sites, 5 files), `CMultiBlockUpdate::new` (1) | raw state id | map to the display state |
| Block-break event 2001 | `ParticlesDestroyBlock` (6 sites, 6 files) | raw state id as event data | map to the display state |
| Bedrock ids | `BlockState::to_be_network_id` (14 sites: core 9, world 5) | generated table, `assert_unchecked` | custom ids use the display state's Bedrock id |
| Item stack on the wire | `crates/pumpkin-protocol/src/codec/item_stack_seralizer.rs` (4 writes of `item.id`), `codec/data_component.rs` line 1321 | raw item id | map custom items to the display item |
| Entity type on the wire | spawn packets, `entity_type.id` | raw id | display type (custom entity types task) |
| Block event packet | `CBlockEvent::new` (`crates/pumpkin-core/src/world/mod.rs` line 949) | raw block id | display block id |
| Entity metadata | block state data of tnt (`entity/tnt.rs` line 148), block display (`entity/decoration/display.rs` line 685), enderman carried block, falling block | raw state id | display state id |
| Block particles | particle data that carries a block state | raw state id | display state id |
| Statistics packet | `CAwardStats` (`entity/player.rs`, 2 sites) | stat ids built from item and block ids | display ids |
| Recipe book | `crates/pumpkin-protocol/src/java/client/play/recipe_book_add.rs` lines 70, 205, 215, 774, 783 | raw `item.id` | display item id |
| Enderman save | `crates/pumpkin-core/src/entity/mob/enderman.rs` lines 404, 409 | `carriedBlockState` as an int state id; vanilla writes a block state compound | block state compound (`Name` + `Properties`) |
| Block display save | `crates/pumpkin-core/src/entity/decoration/display.rs` lines 694, 699 | `block_state` as a raw int | block state compound, as vanilla |
| Statistics save | `crates/pumpkin-core/src/entity/player/statistics.rs` lines 32-50 | keys `"{category}:{numeric id}"`, for example the item id from `net/java/play/use_item_on.rs` line 186 | keys by namespaced name |

Three save sites store numeric ids today: the enderman carried block, the block display state and the player statistics. Their ids already shift when a vanilla update adds states or items, and a custom id would shift with the content set. They move to names before custom content is persisted (task in tasks.md, ordered before the persistence task). Every other save path stores names. The `pump` and `linear` region formats wrap the same chunk NBT (`chunk/format/pump.rs` uses `SingleChunkDataSerializer`).

Egress rule: each id type gets one `to_java_network_id()` (`BlockStateId`, `BlockId`, item, entity type) that maps custom ids to the display id. Packet code writes ids only through those functions, so a grep for raw `as_u16()` or `.id` written into a `VarInt` in packet code finds every violation. The item stack codec is already one choke point for items. Block states have none today, so every block-state row above changes.

Existing server-side dynamic registries, and why none of them is the model for this one:

| Registry | Path | Storage | Ids | Lifetime |
|:--|:--|:--|:--|:--|
| Custom enchantments | `crates/pumpkin-core/src/server/enchantment.rs` | `tokio::sync::RwLock<FxHashMap<String, CustomEnchantmentEntry>>` | none, keyed by name | owned, cloned on every read, async lock |
| Datapack registries | `crates/pumpkin-core/src/data/datapack/dynamic_registry_loader.rs`, `data/datapack/mod.rs` line 83 | `std::sync::RwLock<HashMap<String, HashMap<String, RegistryEntryData>>>` | position after `merge_dynamic_registry_entries`: vanilla order, then custom sorted by name | owned, rebuilt on reload, never persisted |
| Recipes | `crates/pumpkin-core/src/server/recipe.rs` | `std::sync::RwLock<Vec<DynamicRecipe>>` | none | owned, `Vec` cloned on every read |

All three take a lock and clone on read, which is acceptable on cold paths only. Block state lookups run in lighting, palette and physics loops. The datapack merge gives the id rule this decision reuses: custom entries sorted by name, after the vanilla entries.

#### Options

**(a) Leak on each registration call, fallthrough behind the generated tables, registry stays open.** The lookup changes are the same as in the chosen option. Ids follow registration order, so they change with plugin load order. The tables can grow at any time, so every fallthrough read needs a lock or an `ArcSwap` load. A hot reload leaks again and orphans the old ids. Chunks loaded before a late registration keep air where the custom block was. Rejected: same diff as the chosen option with worse ids, a lock on the read path, and unbounded leaks.

**(b) Arc-based registry with a handle type.** Every `&'static Block`, `&'static BlockState`, `&'static Item` and `&'static EntityType` in gameplay becomes a handle. Diff: the 3,973 matches in 569 files measured above, plus `ItemStack` (1,907 mentions in 276 files) losing its `&'static Item` field, plus `Block.default_state` and `Block.states` in pumpkin-data. Runtime: an atomic increment and decrement on every handle copy in the lighting, palette and physics loops (world alone has 115 `BlockState::from_id` and 178 `.to_block_id()` sites), with cache-line contention on hot shared values such as air and stone across Rayon threads. A `Copy` index handle avoids the atomics but turns every field read into a table lookup and has the same diff. Upstream merge friction: almost every upstream gameplay commit would conflict. Rejected: the diff and the merge cost are an order of magnitude above the chosen option, and the gain is the ability to free content, which nothing needs because content is never unloaded.

**(c) Frozen hybrid. Chosen.** Diff: 4 codegen templates, 1 new pumpkin-data module, `const` removed from 21 functions (9 generated, 12 hand-written), 20 namespace sites, 3 numeric save sites, the egress sites in the boundary table, the startup order, and 0 changes to the 913 lookup call sites and to any `&'static` signature. Runtime: on vanilla ids the generated lookups replace `assert_unchecked(id < COUNT)` with a predictable `if id < COUNT` branch to the same table read. On custom ids they call a `#[cold] #[inline(never)]` function in `dynamic.rs` that does one acquire load of the `OnceLock` pointer and one index, so the inline lookups stay small. No lock, no refcount, no allocation after the freeze. Memory: the custom content, allocated once per process and never freed.

**Rejected variant of (c): pin numeric ids in a persisted name -> id map.** The registry would read the map from the world and give each name its previous id. Rejected:

- After the three numeric save sites move to names, no save format stores a numeric id, so pinned ids protect nothing on disk. Those three sites need names anyway, because their ids already shift with vanilla updates.
- Name-sorted allocation already makes ids independent of plugin load order, and identical across restarts while the set of registered content stays the same.
- NeoForge does not persist ids either. On branch 26.3.x, `CommonHooks.writeAdditionalLevelSaveData` writes only `fml.LoadingModList` (mod id and version) to level.dat. `RegistrySnapshot` documents that full snapshots are "never saved to disk nor sent to the client", and the client snapshot is sent per connection (`RegistryManager.generateRegistryPackets`).
- Vanilla grows its state count with each version. Pinned custom ids would collide with new vanilla ids on the next upstream merge and need a remap step. Ranges that start at the generated count move with vanilla.
- Pinning needs a free list and leaves permanent holes when content is removed.

#### Id ranges

Each custom range starts at the generated count of the running build and ends at 65,534. 65,535 (`u16::MAX`) is never allocated, because Pumpkin uses `u16::MAX` as the "none" value of u16 ids (`BlockState.block_entity_type`).

| Registry | Generated count (ids) | Custom range today | Free | Hard limit |
|:--|:--|:--|--:|:--|
| Block states | 35,723 (0..=35,722) | 35,723..=65,534 | 29,812 | `BlockStateId(u16)` and the 16-bit direct palette (`BLOCK_NETWORK_MAX_BITS`, asserted by `proper_network_bits_per_entry` in `crates/pumpkin-world/src/block/mod.rs`) |
| Blocks | 1,286 (0..=1,285) | 1,286..=65,534 | 64,249 | each block takes at least one state, so at most 29,812 custom blocks |
| Items | 1,658 (0..=1,657) | 1,658..=65,534 | 63,877 | `Item.id: u16` |
| Entity types | 161 (0..=160) | 161..=65,534 | 65,374 | `EntityType.id: u16` |

The generated counts come from `assets/blocks.json`, `assets/items.json` and `assets/entities.json` and match `BlockId::BLOCK_COUNT`, `BlockStateId::STATE_COUNT` and the generated `from_id` / `from_raw` tables. If the registered content exceeds a range, the freeze fails and the server stops with one error line that names the registry and the count.

Allocation at freeze, per registry: sort the custom names, give them consecutive ids after the generated count. Block states: walk the custom blocks in block id order and give each block a contiguous state range, so a block's states follow the previous block's last state, as in the generated layout.

State layout of a custom block, identical to the generated one: properties sorted by name, the last property varies fastest, booleans `true` then `false`, integers ascending from the minimum, enum values in declaration order. `Block.states[0].id` is the first state of the range, so the generated property helpers (`from_state_id` computes `id - block.states[0].id`) work unchanged. Custom properties use one hand-written `DynamicProperties` type that implements the generated `BlockProperties` trait and returns its names and values as leaked `&'static str`.

#### Lifetime and thread safety

1. Registration phase. Plugins register content through `pumpkin_data::dynamic::register_block`, `register_item` and `register_entity_type`, into a builder behind a `std::sync::Mutex`. The builder lives in pumpkin-data because its validation needs the generated counts. Duplicate names, names without a namespace or in the `minecraft` namespace, and display entries that are not vanilla are rejected.
2. Freeze. Before the freeze, the startup content phase in pumpkin-core reads the manifest and registers placeholders for missing content. `pumpkin_data::dynamic::freeze` then sorts, allocates ids, builds the values, leaks them and sets the `OnceLock`, once. A failed freeze is terminal: no tables are installed and later calls return `RegistryFrozen`. After the freeze the content phase writes the manifest back (temporary file and rename).
3. Frozen. Readers call `OnceLock::get`: one acquire load, no lock, no refcount. The tables are immutable `&'static` data, so `Sync` holds and Rayon workers read them freely. `BlockStateId::new` and `BlockId::new` accept an id below generated count plus custom count; the `SAFETY` invariant in `block_state.rs` and `blocks.rs` becomes "below the total count", which holds because the tables never shrink.
4. After the freeze, registering a new name returns a `RegistryFrozen` error.
5. Hot reload. The plugin watcher runs `on_load` again, so a reloaded plugin registers again. Re-registering a frozen name with an identical schema (properties, values, display mapping) is a rebind: it attaches the new behaviours to the existing entry. A new name or a changed schema is an error until restart; the rest of the plugin still loads. Unloading a plugin detaches its behaviours and its content acts as a placeholder until restart.

Startup order today does not allow this. `PumpkinServer::new` (`crates/pumpkin/src/main.rs` line 130) builds every world in `Server::new` (`crates/pumpkin-core/src/server/mod.rs` world loop at line 398), and plugins load afterwards in `init_plugins` (`main.rs` line 142, `crates/pumpkin-core/src/lib.rs` line 426). `World::load` reads no chunk, only POI data. The ticker thread starts in `PumpkinServer::new` (`lib.rs` lines 335-345) before `init_plugins`, but loads nothing by itself. The first chunk reader is the `#minecraft:load` function tag (`server/mod.rs` line 447). `WorldInitEvent` and `WorldLoadEvent` (lines 419-434) fire before any plugin exists.

Smallest change: call `plugin_manager.load_plugins(&server)` in `Server::new` between `Arc::new(server)` (line 336) and the world loop (line 398), then freeze. `start_watcher` stays in `init_plugins`. Consequences:

- `on_load` sees an empty `server.worlds`. Plugins that look up a world in `on_load` must move that work to `WorldLoadEvent` or later.
- `WorldInitEvent` and `WorldLoadEvent` start to reach plugins, which they never did at startup.
- Plugin load time counts inside `Server::new`; `init_plugins` keeps only the watcher.

Block behaviours. `default_registry()` builds the `BlockRegistry` at `server/mod.rs` line 187, before `Arc<Server>` exists. `register` takes `&mut self` (`block/registry.rs` line 834) and `block_indices` is `[u16; BlockId::COUNT]` (line 503). The array stays as it is, for vanilla ids. Custom behaviours go into a second table indexed by `id - BlockId::COUNT`, built from the registration phase and installed at the freeze in a `OnceLock` inside `BlockRegistry`. The behaviour lookup checks the id range first, like the data lookups. A hot-reload rebind swaps the behaviour behind the entry's `Arc`, not the table.

#### Persistence format and missing content

Disk is name-based once the three numeric save sites move to names. Custom content uses its full namespaced name in `Block.name`, `Item.registry_key` and `EntityType.resource_name` (for example `mymod:copper_lamp`); generated content keeps the bare vanilla name. One helper in `dynamic.rs` returns the namespaced form of either. `Block::from_name` strips only `minecraft:` and `Block::from_registry_key` strips nothing (template `tools/pumpkin-codegen/src/block.rs` lines 1440-1446), so a namespaced custom name reaches the fallthrough unchanged. `Item::from_registry_key` and `EntityType::from_name` also strip only `minecraft:`.

The manifest is `<world>/ironpumpkin/content_registry.json`, where `<world>` is `basic_config.get_world_path()`, the directory that holds level.dat. A separate file keeps level.dat code (`AnvilLevelInfo`) unchanged for upstream merges. JSON, because admins prune it by hand:

```json
{
  "format": 1,
  "blocks": {
    "mymod:copper_lamp": {
      "properties": { "facing": ["north", "south", "west", "east"], "lit": ["true", "false"] },
      "default": { "facing": "north", "lit": "false" },
      "display": "minecraft:redstone_lamp[lit=false]"
    }
  },
  "items": { "mymod:copper_lamp": { "display": "minecraft:redstone_lamp" } },
  "entity_types": { "mymod:golem": { "display": "minecraft:iron_golem" } }
}
```

Property keys are written in name order and values in state-layout order, so the file fixes the state layout of each block. The file holds no numeric id. Every world loaded under `<world>` shares it, because the registry is process-wide.

When the manifest schema of a name differs from the schema a plugin registers, the registered schema wins and the manifest entry is rewritten. Palette entries with property values the new schema does not know resolve to the block's default state, with one warning per name.

When content in the manifest has no registering plugin:

- Block: a placeholder block with the same name and the same property schema, so it has the same states and its palette entries save back unchanged. It has no behaviour, takes the physical state data of its display state, and drops nothing. Clients see the display state.
- Item: a placeholder item with the same name and the display item; stacks keep their components and save back unchanged.
- Entity type: a placeholder type that never spawns. The entity chunk keeps the raw compound and writes it back.
- One warning per missing name at startup. The entry stays in the manifest until an admin deletes it while the server is stopped; after that, remaining references load as air or are dropped, as in vanilla.

A name that is in neither the generated data, the registry nor the manifest (a world copied from another server) loads as air for blocks and is dropped for items and entities, with one warning per name.

#### What the phase 3 registry sync needs

- One snapshot per registry (`minecraft:block`, `minecraft:item`, `minecraft:entity_type`): int id -> namespaced name for every entry, generated and custom, plus an empty alias map. This is the shape of NeoForge's `RegistrySnapshot` (`Int2ObjectSortedMap<Identifier> ids`, `Map<Identifier, Identifier> aliases`) sent in `FrozenRegistryPayload`. The frozen tables give it directly: generated ids `0..count`, then custom ids in allocation order.
- No block state snapshot. NeoForge rebuilds state ids on the client by iterating blocks in id order and each block's possible states in order (`NeoForgeRegistryCallbacks.BlockCallbacks.onBake`). The state layout rule above produces the same ids, given that the ported mod declares the same properties and values.
- The snapshot is per session. Nothing has to be read from disk.
- Display mapping per connection kind: vanilla clients get display ids, NeoForge clients the real ids. Chunk and block update packets are encoded once and broadcast today, so phase 3 must encode per client kind. Out of scope here.

#### Upstream merge friction

- Generated files: regenerated, never merged by hand.
- Codegen templates: small hunks in `block.rs`, `item.rs`, `entity_type.rs` and `flower_pot_transformations.rs` of `tools/pumpkin-codegen/src/`.
- `&'static` signatures changed: 0. Lookup call sites changed: 0 of 913.
- `BlockStateId::new` and `BlockId::new` keep their names and become custom-aware, so new upstream callers handle custom ids without changes. The generated constants switch to a `const fn new_vanilla`.
- New code lives in `crates/pumpkin-data/src/dynamic.rs` and a new `crates/pumpkin-core/src/content/` module.

#### Paths that must change

| Path | Change | Sites |
|:--|:--|--:|
| `tools/pumpkin-codegen/src/block.rs` | fallthrough in `BlockState::from_id`, `BlockState::to_be_network_id`, `Block::from_id`, `Block::from_state_id` (drops `const`), `Block::from_registry_key`, `Block::from_name`, `Block::from_item_id`, `BlockId::from_state_id`, the `_` arms of `Block::properties` and `Block::from_properties`; const constructor template lines 570 and 751 -> `new_vanilla` | 10 generated functions, 2 template lines |
| `tools/pumpkin-codegen/src/item.rs` | `_` arm of `Item::from_id` and `Item::from_registry_key` | 2 |
| `tools/pumpkin-codegen/src/entity_type.rs` | `_` arm of `EntityType::from_raw` and `EntityType::from_name` | 2 |
| `tools/pumpkin-codegen/src/tag.rs` | none: the generated tag lists hold generated ids only, so custom content is in no tag. A custom-id branch in `BlockId::has_tag` measured +5% on the `noise_generation` bench (#17). Tag membership of custom content is a separate task that does not touch `has_tag` | 0 |
| `crates/pumpkin-data/src/dynamic.rs` (new) | registration builder, `freeze`, frozen tables, namespaced-name helper; `DynamicProperties` (block state task) and total counts (wasm host change) later | new |
| `crates/pumpkin-data/src/block_state.rs` | `BlockStateId::new` / `new_or_air` custom-aware, `new_vanilla`; drop `const` on 6 helpers | 8 |
| `crates/pumpkin-data/src/blocks.rs` | `BlockId::new` / `new_or_air` / `new_vanilla`, `to_block`, `ToResourceLocation` (line 119) | 5 |
| `crates/pumpkin-data/src/item_stack/mod.rs` | namespaced `"id"` (line 805) | 1 |
| `crates/pumpkin-world/src/generation/rule/block_match.rs` | drop `const` on `test` | 1 |
| `tools/pumpkin-codegen/src/flower_pot_transformations.rs` lines 25-30 | `get_potted_item` uses the vanilla-only const constructor | 1 |
| `format!("minecraft:{}", ...)` on a block, item or entity type name | namespaced-name helper: wasm-host v0_1 and v0_2 `world.rs` 443 and `events/mod.rs` 89, 98; `block/registry.rs` 827; `entity/living.rs` 3489; `entity/projectile/fishing_bobber.rs` 77; `pumpkin-protocol/src/codec/recipe.rs` 18, 23; `block/entities/trial_spawner.rs` 153; `entity/mod.rs` 3987; `block/entities/mob_spawner.rs` 66, 289; `pumpkin-protocol/src/codec/data_component.rs` 2371; `pumpkin-command/src/argument_types/resource.rs` 176 (plus `blocks.rs` 119, `item_stack/mod.rs` 805 and `chunk/format/mod.rs` 547 in their own rows) | 17 |
| `crates/pumpkin-world/src/chunk/format/mod.rs` | save `Name` (547), unknown-name handling on load | 2 |
| `crates/pumpkin-world/src/generation/structure/template/block_state_resolver.rs` | one warning per unknown name | 1 |
| `crates/pumpkin-core/src/world/mod.rs` lines 4281-4290 | keep the compound of placeholder entity types | 1 |
| `crates/pumpkin-core/src/block/registry.rs` lines 503, 834 | custom behaviour table installed at the freeze, range check in the behaviour lookup | 2 |
| `crates/pumpkin-core/src/entity/mob/enderman.rs` 404, 409; `entity/decoration/display.rs` 694, 699; `entity/player/statistics.rs` 32-50 | numeric ids on disk -> names | 3 sites |
| `crates/pumpkin-wasm-host-v0_1/src/world.rs`, `crates/pumpkin-wasm-host-v0_2/src/world.rs` lines 468-492 | `BlockId::COUNT` / `BlockStateId::COUNT` -> total counts | 4 + 4 |
| `crates/pumpkin-core/src/entity/type.rs` `from_type` | factory arm for custom types | 1 |
| `crates/pumpkin/src/main.rs`, `crates/pumpkin-core/src/server/mod.rs`, `crates/pumpkin-core/src/lib.rs`, `crates/pumpkin-core/src/plugin/mod.rs` | registration phase and freeze before the world loop | startup |
| Network egress, through `to_java_network_id()` only | `palette.rs` `convert_network` and `convert_be_network` 2, `CBlockUpdate::new` 9, `CMultiBlockUpdate::new` 1, `CBlockEvent::new` 1, `ParticlesDestroyBlock` 6, `to_be_network_id` 14, entity metadata block states 4 (tnt, block display, enderman, falling block), `CAwardStats` 2, item stack codec 5, `recipe_book_add.rs` 5, block particle data (not counted) | 49 + particles |

The wasm hosts map entity types to the WIT enum by index in `EntityType::ALL` (`crates/pumpkin-wasm-host-v0_2/src/entity.rs` lines 898-915). Unknown names already return an error there, so custom entity types need an additive WIT representation in the Wasm API task, not a change here.

#### Not measured

- Runtime cost of the fallthrough branch and of the Arc option: no code exists yet. The registry core task runs the existing benches on both commits.
- Memory of the leaked content: depends on what plugins register.
- What a vanilla client does with an out-of-range state id. The display mapping makes the question moot, so I did not capture it.

## Risks / Trade-offs

- [Upstream rebases conflict on `&'static` call sites] -> keep the registry a fallthrough layer; never edit generated files.
- [A world references custom content whose plugin is missing] -> a placeholder rebuilt from the content manifest, one warning per missing name at startup (see the decision).
- [Registration after the first world loads] -> the registry freezes before the world loop in `Server::new`; later calls fail with `RegistryFrozen`.
- [A vanilla client receives a custom block state or item id] -> every network egress maps custom ids to the display id.
- [Vanilla clients cannot render custom content] -> custom entities map to a vanilla type for display until phase 3.
