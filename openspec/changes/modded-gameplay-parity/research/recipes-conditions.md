# Phase 6 research: recipes, data conditions, game rules

Scope: questions 1 to 4 of the brief. Read-only research. Nothing was run (no cargo): every
"measured" claim is a code read, and I say so where a run would be needed.

Path prefixes: `NF` = `target/scratch/ref/neoforge-src/net/neoforged/neoforge`,
`MC` = `target/scratch/ref/minecraft-src/net/minecraft`. Other paths are repository paths.

## 0. Findings that change the plan

1. The task text names the conditions `true`, `false` and `item exists`. These are the 21.1 names.
   NeoForge 26.3 registers `neoforge:and`, `never`, `registered`, `mod_loaded`, `not`, `or`,
   `tag_empty`, `always` and `feature_flags_enabled` (`NF/common/NeoForgeMod.java:381-389`). A file
   with `neoforge:true` fails to decode on 26.3 and is logged and dropped. Two Pumpkin tests use the
   old names (`crates/ironpumpkin-mods/src/data_maps/tests.rs:596`,
   `crates/pumpkin-core/src/world/loot/modifiers/tests.rs:122`). The modpack jars are 1.21.1, so the
   JSON of ported mods can carry the old names: a port step must rewrite them.
2. Pumpkin never sends `UPDATE_RECIPES`. The packet struct exists
   (`crates/pumpkin-protocol/src/java/client/play/update_recipes.rs:9`) and no code builds it
   (`rg CUpdateRecipes` finds only generated Wasm host files). The recipe property sets and the
   stonecutter list of vanilla 26.3 are therefore not synced today.
3. Dynamic furnace, smoker, blast furnace and campfire recipes are parsed and shown in the recipe
   book, but no cooking logic reads them. Cooking reads only the generated table
   (`crates/pumpkin-data/src/generated/recipes.rs:25113`, `furnace_like_screen_handler.rs:32`).
4. A tag ingredient written as a string (`"#c:ingots"`, the 26.3 form) is read as an item id and
   never matches (`recipe_loader.rs:57-60`). Only the old object form `{"tag": ..}` gives
   `Tagged`. Code read, not run.
5. Mod game rules cannot fit the generated `GameRule` enum (section 3). The Wasm hosts transmute
   the enum to a WIT enum by position (`crates/pumpkin-wasm-host-v0_2/src/world.rs:84-92`).

## 1. Recipes

### 1.1 How Pumpkin loads and matches recipes today

Load path:

- `DatapackManager::load_all` (`crates/pumpkin-core/src/data/datapack/mod.rs:146`) calls
  `load_pack_contents` per pack. The recipe step reads `data/<ns>/recipe` and `recipes`
  (`mod.rs:1773`), walks every `.json` (`mod.rs:2118`) and calls `recipe_loader::parse_recipe`
  (`recipe_loader.rs:9`).
- `parse_recipe` strips `minecraft:` from `type` and matches 14 fixed names: shaped, shapeless,
  transmute, dye, decorated pot, imbue, stonecutting, smithing transform and trim,
  `crafting_special_*`, smelting, blasting, smoking, campfire cooking, brewing. Any other `type`
  returns `None` with no log (`recipe_loader.rs:45`). A mod type `mymod:crush` is dropped silently.
- The result is `DynamicRecipe`, a closed 7-variant enum in `pumpkin-protocol`
  (`crates/pumpkin-protocol/src/codec/recipe.rs:119`). A recipe of a later pack replaces an earlier
  one with the same id (`mod.rs:2145-2157`). `set_recipes` stores the list in `RecipeManager`
  (`mod.rs:250`, `crates/pumpkin-core/src/server/recipe.rs:7`), one `RwLock<Vec<DynamicRecipe>>`.
  `get_dynamic_recipes()` clones the whole vector on every call (`recipe.rs:65`).
- Vanilla recipes are not loaded. They are `static` generated tables: crafting
  (`generated/recipes.rs:202`), cooking (`:20056`), stonecutting (`:21652`), smithing trim and
  transform (`:24813`, `:24923`). The file is 25148 lines. A pack recipe cannot replace or remove a
  vanilla recipe: the static table is matched first (`crafting_screen_handler.rs:1034`, dynamic
  after it at `:1048`).
- Mod packs reach the same path: `mod_packs.rs` writes the embedded files to `mod_data/<id>/` and
  runs the same loaders (`crates/pumpkin-core/src/data/datapack/mod_packs.rs:1-30`).

Consumers of the recipes (all do a linear scan, static table first, then `get_dynamic_recipes()`):

| Consumer | Static | Dynamic | Pointer |
|:--|:--|:--|:--|
| Crafting table, 2x2 grid, crafter | yes | `Crafting` only | `crafting_screen_handler.rs:1006-1065` |
| Furnace, smoker, blast furnace, campfire (logic) | yes | no | `generated/recipes.rs:25113`, `ironpumpkin-mods/src/transfer/inventory.rs:353` |
| Furnace slot check | yes | no | `furnace_like_screen_handler.rs:23-40` |
| Stonecutter | yes | `Stonecutting` | `stonecutter_screen_handler.rs:60-80` |
| Smithing table | yes | transform, trim | `smithing_table_screen_handler.rs:37-85`, `:249-290` |
| Brewing stand | brewing table | `Brewing` | `crates/pumpkin-core/src/block/entities/brewing_stand.rs:78-235` |
| Recipe book add on join | yes | `Crafting`, `Cooking` | `world/mod.rs:4281-4291`, `recipe_book_add.rs:520-660` |
| Recipe book re-add on `/reload` | yes | same | `server/mod.rs:624-641` |
| `place_recipe` (click in book) | yes | same | `net/java/play/place_recipe.rs:48-98` |

Matching code: `GenericIngredient` and `IngredientRef` are two enums that wrap the generated
`RecipeIngredientTypes` and `OwnedRecipeIngredient` (`net/java/recipe_helper.rs:7-20`,
`recipe_provider.rs:11-27`). `OwnedRecipeIngredient` has 3 variants: `Simple(id)`, `Tagged(tag)`,
`OneOf(ids)` and matches by item only (`codec/recipe.rs:7-27`). It has no count, no component test
and no custom type. `Simple` compares strings through `namespaced_name` on every call.

The recipe book writes entries with id = position in the concatenation `static crafting + static
cooking + dynamic` (`recipe_book_add.rs:534-640`). A `/reload` that changes the dynamic list
shifts the ids of later entries. The `replace = true` flag on re-add resets the client list.

Plugin side: v0.1 and v0.2 Wasm hosts can add `DynamicRecipe` values
(`crates/pumpkin-wasm-host-v0_2/src/recipe.rs:128-137`). `ironpumpkin-mods` has no recipe module;
its only recipe code is the brewing and furnace read for transfer
(`crates/ironpumpkin-mods/src/transfer/inventory.rs:65-76,353`).

### 1.2 What vanilla 26.3 and NeoForge do (reference)

- `RECIPE` is a reloadable data registry, not a plain reload listener
  (`MC/resources/RegistryDataLoader.java:154,162`). `RECIPE_TYPE` and `RECIPE_SERIALIZER` are
  built-in registries (`MC/core/registries/BuiltInRegistries.java:211-212`). The loader dispatches
  on the `type` field through `RECIPE_SERIALIZER`. Recipe types are object identity
  (`MC/world/item/crafting/RecipeType.java:7-14`, `simple(Identifier)` makes a mod type).
- `RecipeManager` indexes by type (`RecipeMap`), keeps property sets and stonecutter recipes, and
  builds display entries in `finalizeRecipeLoading` (`MC/world/item/crafting/RecipeManager.java:65-95`).
  NeoForge adds recipe priorities (`RecipeMap.order`, `RecipeMap.java:48`) and mod property sets
  (`RegisterRecipePropertiesEvent`, `NF/common/crafting/RecipePropertySetManager.java:29`).
- What vanilla syncs to the client (server to client):
  1. `UPDATE_RECIPES`: a map `property set key -> list of item ids` (9 vanilla keys: smithing
     base, template, addition, furnace, blast furnace, smoker, campfire, brewing input, brewing
     reagent) and the stonecutter list (`Ingredient` + display only, no recipe)
     (`MC/network/protocol/game/ClientboundUpdateRecipesPacket.java:17-23`,
     `MC/world/item/crafting/SelectableRecipe.java:12-41`,
     `MC/world/item/crafting/RecipePropertySet.java:11-28`).
  2. `RECIPE_BOOK_ADD`: `RecipeDisplayEntry` = display id, `RecipeDisplay`, optional group,
     `RecipeBookCategory` (registry id), optional list of `Ingredient` as crafting requirements
     (`MC/world/item/crafting/display/RecipeDisplayEntry.java:19-31`).
  Full recipes never travel in vanilla. A machine recipe of a mod does not reach the client unless
  it has a display in the recipe book.
- NeoForge opt-in sync: `OnDatapackSyncEvent.sendRecipes(types)` (`NF/event/OnDatapackSyncEvent.java:68`)
  then `CommonHooks.sendRecipes` sends `neoforge:recipe_content` to NeoForge clients only
  (`NF/common/CommonHooks.java:1723-1727`, `NF/network/payload/RecipeContentPayload.java:30-46`).
  Called from `PlayerList` on join and on reload (`MC/server/players/PlayerList.java:193-198,880-890`).
  The payload holds the `RecipeType` registry ids and `RecipeHolder.STREAM_CODEC`, which writes the
  serializer registry id and the recipe body. Both registries are synced registries
  (`NF/registries/NeoForgeRegistriesSetup.java:47,60,82`).
- Pumpkin already lists `neoforge:recipe_content` as a known play channel but has no sender
  (`crates/pumpkin-core/src/net/java/neoforge.rs:261`). `SYNCED_REGISTRIES` has 7 rows and none is
  `recipe_type` or `recipe_serializer` (`crates/pumpkin-core/src/net/java/configuration_tasks.rs:59-88`).

### 1.3 What a `recipe_type` and `recipe_serializer` API needs

Goal: a mod machine recipe in a data pack JSON loads, syncs where vanilla syncs, and a block entity
of the mod can query it. Needed parts:

1. Registration: `ModInit::register_recipe_type(id)` and
   `register_recipe_serializer(id, decode)` next to `register_block`
   (`crates/ironpumpkin-mods/src/lib.rs:496-568`). Host table: a frozen name table like
   `dynamic_registries.rs` (`crates/pumpkin-data/src/dynamic_registries.rs:456-551`, one `register_*`
   per kind, frozen by `dynamic::freeze`). The types need no id on the wire unless `recipe_content`
   is sent (see 1.5).
2. Loader: `parse_recipe` falls through to the serializer table for an unknown `type`. The decode
   callback gets the whole JSON object (as the loot modifier callback does,
   `crates/ironpumpkin-mods/src/loot.rs:1-30`) and returns a mod recipe value. A decode error is
   logged with the recipe id (today it is silent).
3. Storage: `RecipeManager` needs a per-type index so a block entity does not scan every recipe.
   Today there is one vector and one clone per read (`server/recipe.rs:56-69`). A type-keyed
   `HashMap<TypeId, Arc<[RecipeEntry]>>` behind `ArcSwap` (the server already uses `ArcSwap` for
   `level_info`) gives lock-free reads and no clone.
4. Query API for a mod: `recipes.get_recipe_for(type, input, world)` and `by_id`, with the
   NeoForge semantics of `RecipeManager.getRecipeFor` (hint recipe first, then type index in
   priority order) (`MC/world/item/crafting/RecipeManager.java:97-130`). The value type is a
   `Box<dyn Any + Send + Sync>` in the pack binary (the mod crate downcasts to its recipe struct):
   a mod crate is compiled into the binary, so a generic `ModRecipe` trait object works and needs no
   WIT.
5. Sync: where vanilla syncs. A mod recipe with a vanilla-shaped result is not synced in vanilla
   either. Syncing needs one of: (a) an optional display, (b) property-set items, (c)
   `recipe_content`. See options in 1.5.
6. Conditions: a recipe file passes the condition step of section 2 before decode.
7. Reload: `set_recipes` must also replace the mod table in the same write, so a block entity never
   sees a mix of old and new recipes.

### 1.4 Ingredient types and the wire format

Reference (26.3):

- `Ingredient` is a `HolderSet<Item>` or a custom `ICustomIngredient`
  (`MC/world/item/crafting/Ingredient.java:19-40`). JSON: `"ns:item"`, `"#ns:tag"` or a list for the
  vanilla form (`Ingredient.java:33-34`). A JSON object with `neoforge:ingredient_type` selects a
  custom type from the registry `neoforge:ingredient_serializer`, key `INGREDIENT_TYPES`
  (`NF/common/crafting/IngredientCodecs.java:20-33`, `NF/registries/NeoForgeRegistries.java:40,55`).
  The registry is synced (`.sync(true)`).
- Built-in types (`NF/common/NeoForgeMod.java:365-370`): `compound` (field `children`, alias
  `ingredients`), `components`, `difference`, `intersection`, `block_tag`, `custom_display`.
  `BlockTagIngredient` takes a block tag and tests the item of each block
  (`NF/common/crafting/BlockTagIngredient.java:34-79`). `CompoundIngredient` tests any child
  (`CompoundIngredient.java:44-50`). `SizedIngredient` is not an ingredient type: it is an
  ingredient plus a count, nested JSON `{"ingredient":..,"count":..}`, test = ingredient and
  `stack.getCount() >= count` (`NF/common/crafting/SizedIngredient.java:39-83`).
- Wire, vanilla and every non-NeoForge connection: `ByteBufCodecs.holderSet(ITEM)`: `VarInt 0` +
  tag identifier, or `VarInt n+1` + n item registry ids (`Ingredient.java:26`). A custom ingredient
  is flattened to its `items()` list (`Ingredient.java:getValuesForSync`).
- Wire, NeoForge connection and custom ingredient: `VarInt -1000`, then the registry id of the
  `IngredientType` as a VarInt, then the type stream codec; default = the map codec as NBT with
  registry ops (`NF/common/crafting/IngredientCodecs.java:35,55`, `IngredientType.java`). A reader
  detects the marker by reading the first VarInt and rewinding (`IngredientCodecs.java:41-48`).
- `Optional<Ingredient>` uses the same marker (`IngredientCodecs.java:81`). The recipe book
  `craftingRequirements` list and the stonecutter list use `CONTENTS_STREAM_CODEC`
  (`RecipeDisplayEntry.java:28`, `SelectableRecipe.java:21`).

Pumpkin today: `write_ingredient_holderset` writes exactly the vanilla form, with a tag name when
the tag has no known items (`recipe_book_add.rs:207-262`). It never writes `-1000`. A custom
ingredient therefore needs a flatten step to an item list for any client, and the marker only for
a NeoForge client with the ingredient type registry synced.

### 1.5 Options for recipes

| Option | Files and crates | Upstream merge cost | Performance | Wire and save risk |
|:--|:--|:--|:--|:--|
| A. Extend `DynamicRecipe` with a `Mod(Box<dyn ModRecipe>)` variant and a serializer table | `pumpkin-protocol/codec/recipe.rs`, `recipe_loader.rs`, `server/recipe.rs`, `ironpumpkin-mods` | Low to medium. `DynamicRecipe::` appears in 13 source and test files (`rg -c`): each exhaustive match needs one arm, among them the hot `recipe_book_add.rs` | A scan over `Vec` stays. Add the per-type index to avoid it | None for vanilla clients: the variant is skipped in the book writer. No save change |
| B. New `ModRecipeRegistry` beside `RecipeManager`, `DynamicRecipe` untouched | new module in `pumpkin-core` (bounded), `mod_packs.rs` hook, `ironpumpkin-mods` | Lowest. Only the load call site (`mod.rs:250`) and one `set` call change | Per-type `ArcSwap` index, no clone | None. Recipe book display is a separate step (below) |
| C. Replace `RecipeManager` by a typed map and make the 7 vanilla variants entries of it | `pumpkin-core`, `pumpkin-inventory`, `pumpkin-protocol`, 12 call sites | High: touches hot upstream files in every merge | Best | Display ids in the book shift: the book id scheme must be pinned first |

Recommendation: B for mod types, plus fixing the vanilla gaps in A style where they are small:
parse `"#tag"` strings (finding 4), use dynamic `Cooking` in the furnace logic if a mod needs it
(finding 3). B keeps the upstream diff to two call sites. The decision to give C a later task stays
open: C only pays off if the number of vanilla-type consumers grows.

Sync sub-options for B (one choice per mod recipe type, all additive):

- S1: no sync. The block entity reads the server table. Correct for any machine recipe that has no
  client-side use. Zero wire risk. A JEI-style client display needs more.
- S2: `neoforge:recipe_content` for the types a mod lists, to NeoForge clients only
  (`CommonHooks.java:1723`). Needs: add `recipe_type` and `recipe_serializer` to `SYNCED_REGISTRIES`
  (`configuration_tasks.rs:59`, 2 rows, snapshot fns like `ContentTables::block_snapshot`), a body
  encoder that serializes the mod recipe (NBT of the recipe JSON is the simplest wire for a
  mod-defined codec, because `RecipeHolder.STREAM_CODEC` calls the serializer stream codec), and the
  client mod must decode it. The mod owns both ends: its Rust server code writes bytes, its Java
  client (the real NeoForge client mod) reads them. Medium wire risk. Needs a wire capture from a
  NeoForge 26.3 server (not done here).
- S3: recipe book display with a vanilla display variant (3 exist, `recipe_book_add.rs:16-18`).
  A new display type needs `recipe_display` registry sync: out of scope for a machine recipe.
- S4: property sets (`UPDATE_RECIPES`), 9 item sets, for client slot checks. Needs the vanilla
  sender first (finding 2). It also affects smithing and furnace slot checks on a vanilla client;
  not tested whether they work today (needs a client).

Ingredient types (for B and S2):

- Add `CustomIngredient` as a trait object in `OwnedRecipeIngredient` (`Custom(Arc<dyn ...>)`):
  `test(stack)`, `items()`, `is_simple()`, `type_id()`. Built-ins `compound`, `difference`,
  `intersection`, `block_tag`, `components`, `custom_display` as Rust types. `components` needs the
  data component matcher of `pumpkin-data` (patch compare). `block_tag` needs the dynamic block tag
  table (`pumpkin-data/src/dynamic.rs` tag tables, items-of-block map).
- `neoforge:ingredient_serializer` registration API: `ModInit::register_ingredient_type(id,
  decode)`, frozen like the recipe types, synced registry (`SYNCED_REGISTRIES` row 3 of this section).
  Wire marker `-1000` only when the client has the type; else flatten. Cost: one writer function in
  `recipe_book_add.rs` (hot file, +1 arm), one row in `SYNCED_REGISTRIES`.
- Upstream merge: `OwnedRecipeIngredient` has 3 variants and 8 source files use it; adding `Custom` adds an arm to each match. Alternative: keep the enum and store a mod ingredient as `OneOf(resolved ids)` at load time.
  This works only when the item set is static between reloads, which holds for item, tag and compound
  ingredients but not for `components` (stack test) or `SizedIngredient` counts. Recommended: `Custom`
  variant for tests, `OneOf` as the flattened display form.

Risks: the recipe id of a `Mod` variant must be unique across vanilla and dynamic ids (book ids use
position); StoneBlock 4 has 33018 recipe files in 188 mods (`modpack-usage.md:1587`). A linear scan per grid change over that set is likely too slow, so the per-type index is needed. Measure first: no recipe lookup bench exists.

## 2. Data conditions (`neoforge:conditions`)

### 2.1 How NeoForge applies them

- Format: a `neoforge:conditions` list inside the JSON object of the file; a non-object value uses
  `neoforge:value` (`NF/common/conditions/ConditionalOps.java:CONDITIONS_KEY, CONDITIONAL_VALUE_KEY`).
  A failed condition makes the codec return `Optional.empty()`; the loader logs at debug and skips
  the file (`MC/server/packs/resources/SimpleJsonResourceReloadListener.java:35-55`,
  `MC/resources/RegistryLoadTask.java:117-122`).
- Condition ids, 26.3 (`NF/common/NeoForgeMod.java:381-389`): `neoforge:and`, `or`, `not` (children),
  `always`, `never`, `mod_loaded` (`modid`), `registered` (`registry` default `minecraft:item`,
  `value`), `tag_empty` (`registry` default item, `tag`), `feature_flags_enabled` (`flags`, non-empty).
  The condition registry is server-side only (`NeoForgeRegistries.java:42`, no `.sync`).
- Evaluation context `ICondition.IContext`: `isTagLoaded`, `getTag`, `registries()`,
  `enabledFeatures()` (`NF/common/conditions/ICondition.java`, `ConditionContext.java`). Tags are
  the pending tags of the same reload, so `tag_empty` sees tags that no registry has bound yet
  (`ConditionContext.java:getTag`, `MC/resources/ResourceManagerRegistryLoadTask.java:53-54`).
- The 26.3 registry loader uses `FeatureFlags.VANILLA_SET`, not the world flags
  (`ResourceManagerRegistryLoadTask.java:53`). `ReloadableServerResources` uses the world flags
  (`MC/server/ReloadableServerResources.java:56`). So `feature_flags_enabled` differs by loader.
- Loaders that NeoForge covers (patched sites): every `SimpleJsonResourceReloadListener`
  (`scanDirectory`), every data registry through `RegistryLoadTask`, data maps
  (`NF/registries/DataMapLoader.java:140`, entry level `WithConditions`), global loot modifiers
  (`NF/common/loot/IGlobalLootModifier.java:28`), sprite sources, overlays in `pack.mcmeta`
  (`MC/server/packs/OverlayMetadataSection.java:57`), client animation and model loaders.
  Not covered: tag files. `TagLoader` has no condition code; tags use `required` and `remove`
  (`MC/tags/TagFile.java:7-12`). Functions (`.mcfunction`) are not JSON.
- 26.3 recipes are in the `RECIPE` data registry, so recipes follow `RegistryLoadTask`; the
  optional advancement of a recipe is conditional through the same ops (`ConditionalRecipeOutput`).

### 2.2 Where Pumpkin reads pack JSON

All pack JSON passes `load_pack_contents` (`mod.rs:1755`) per pack, in order: embedded packs, mod
packs (`mod_packs.rs`), world packs. A file read is `fs::read_to_string` + `serde_json::from_str`
in each loader. Loaders:

Files read per loader, none reads the key today unless noted:

- recipes `recipe_loader.rs:9`, `mod.rs:2118`; advancements `advancement_loader.rs:56`; loot tables
  `loot_table_loader.rs:51`; predicates, item modifiers, slot sources `predicate_loader.rs:43`,
  `item_modifier_loader.rs:43`, `slot_source_loader.rs:43`; context providers
  `context_provider_loader.rs:55-121`; damage types `damage_type_loader.rs:225`; dynamic registries
  `dynamic_registry_loader.rs:299` (exclusion list `:314`); structures, pools, processor lists
  `structure_loader.rs:55,118,189`; trades `trade_loader.rs:503,553,603`; test instances
  `test_loader.rs:89`; function tags `function_loader.rs:92`; tags `tag_loader.rs:99` (NeoForge has
  no conditions there).
- Loot modifiers `loot_modifier_loader.rs:51`: the key is ignored on purpose
  (`ironpumpkin-mods/src/loot.rs:12`, test `modifiers/tests.rs:122`).
- Data maps use their own file walk (`ironpumpkin-mods/src/data_maps/loader.rs:55,67`): a conditional
  entry never loads (warn).

One hook point per loader: about 15 call sites in `pumpkin-core` plus the data map loader.

### 2.3 What one evaluation point needs

1. A function `conditions::passes(&mut serde_json::Value, &Context) -> Result<bool, String>`:
   reads `neoforge:conditions`, evaluates the list (all must hold), removes the key so the
   typed decode of the loader does not see it, and handles `neoforge:value` (unwrap). A file
   without the key costs one `Value::get`. A list with an unknown `type` is a parse error: the file
   is dropped and logged, as NeoForge does (`ICondition.CODEC` fails on an unknown id).
2. A `Context`: mod list (`ModList.isLoaded`), item/block/other registry existence, tag contents,
   enabled features. Facts for each:
   - `mod_loaded`: `ModList` lives in `ironpumpkin-mods` (`crates/ironpumpkin-mods/src/mod_list.rs:62-64`)
     and `pumpkin-core` cannot depend on it (the dependency goes the other way). Same inversion as
     `mod_packs::set_mod_data_packs` (`mod_packs.rs:56-65`): a `OnceLock` of mod ids that the pack
     build sets. Wasm plugins are not NeoForge mods: say whether their ids count (open question).
   - `registered`: items, blocks, entity types, mob effects, sound events, attributes and data
     component types have name lookups that include mod content after `dynamic::freeze`
     (`pumpkin-data/src/dynamic.rs`, `dynamic_registries.rs`). Other registries (data registries,
     recipe type) need a case each or a "unknown registry: false" rule.
   - `tag_empty`: needs the merged tag set of all packs. Today tags merge while packs load
     (`PackContentAccumulators.tags`, `mod.rs:1599`), so a tag of a later pack is not yet visible when
     an earlier pack's recipe loads. NeoForge loads tags first for the reload. Pumpkin needs a
     two-pass load: tags of all packs, then every other file with the condition context. Cost: tag
     pass reads `data/*/tags` of every pack twice (cheap) or keeps the first-pass result.
     A mod-owned tag defined with `register_tag` is "unknown to the datapack tag loader"
     (`dynamic.rs` header), so `tag_empty` for a mod tag reads the custom tag table, not the loader.
   - `feature_flags_enabled`: `DatapackManager::resolve_enabled_features` returns a fixed list of
     `&'static str` from the enabled packs (`mod.rs:972-1004`). Needs the flag set before
     `load_all` finishes: derive it from `enabled_packs` (already an argument, `mod.rs:146`).
3. The built-in condition set is 9 types; a mod type registry (`neoforge:condition_codecs`, 16 mods
   register one, `mapping-table.md:358`) is out of scope for the first step. Interface to allow it
   later: a `HashMap<Identifier, fn(&Value, &Context) -> Result<bool>>` that the built-ins fill.

### 2.4 Options

| Option | Changes | Upstream merge cost | Performance | Risk |
|:--|:--|:--|:--|:--|
| A. One `pumpkin-core/src/data/datapack/conditions.rs` module, called by each loader after `from_str` | 1 new file, about 15 one-line calls in loaders, 1 `OnceLock` for mod ids, context built once per `load_all` | Low per file, but about 15 files that upstream edits. Each call is one line near `from_str`, so conflicts stay small | Zero when no key; one `Value::get` per file | Tags must be loaded first (two-pass), else `tag_empty` is wrong |
| B. Wrap file reads: one `read_pack_json(path) -> Option<Value>` helper that every loader uses, with the condition check inside | 1 new helper, about 15 loaders change `fs::read_to_string + from_str` to the helper | Medium: touches the read line of each loader (more conflict-prone than A) | Same | Single place for future needs (overlays, hash) |
| C. Pre-filter at the pack boundary: before `load_pack_contents`, walk the pack, evaluate conditions on every `.json`, write a filtered copy (as `mod_packs.rs` already copies) | 1 module, 0 loader changes | Lowest for loaders | One extra read of every file; mod packs already pass through disk | A filtered copy for world packs means writing into the world folder: not acceptable; needs an in-memory overlay instead |

Recommendation: A, with B's helper only if the loader count grows. A matches "one evaluation point"
and keeps each loader diff at one line. C does not fit world packs. The two-pass tag order is the
real design cost and is needed under every option.

Data maps need no extra code beyond replacing the "never loads" branch (`loader.rs:55-67`) by the
shared function. The loot modifier test `neoforge_conditions_are_ignored` would change meaning:
a modifier with `neoforge:false` must not load.

## 3. Game rules

### 3.1 Pumpkin today

- A game rule is a row of `assets/game_rules.json` (a bool or `{default, min, max}`), 59 rules.
  `tools/pumpkin-codegen/src/game_rules.rs` generates the `GameRule` enum
  (`crates/pumpkin-data/src/generated/game_rules.rs:5`), the `GameRuleRegistry` struct with one typed
  `pub` field per rule (`:222`) and `GameRuleValue<I, B>` (`:460`). Code reads fields directly:
  `level_info.game_rules.mob_griefing` (`crates/pumpkin-core/tests/neoforge_entity_events.rs:318`,
  `world/mod.rs:4443`, 65 matches of `game_rules.<field>`). `get(rule)` and `get_mut(rule)` match by enum.
- State: `LevelInfo.game_rules: GameRuleRegistry` (`crates/pumpkin-world/src/world_info/mod.rs:83-85`)
  in an `ArcSwap`. `World::set_game_rule` clones the whole `LevelInfo`, mutates, stores
  (`world/mod.rs:994-1018`). `/gamerule` does the same inline (`commands/gamerule.rs:49-110`) and
  fires `GameRuleChangedEvent` (`plugin/level_events.rs:349`).
- Save: `data/minecraft/game_rules.dat`, a NBT `data` compound with `minecraft:<rule>` keys: byte for
  bool, int for int (`crates/pumpkin-world/src/world_info/data_files.rs:417-490`). Keys that the loop
  over `GameRule::all()` does not know are dropped on the next write: a mod rule value would be lost.
- Client: `REQUEST_GAMERULE_VALUES` is answered with `(name, value-string)` pairs of
  `GameRule::all()` only (`net/java/play/client_command.rs:56-103`, `game_rule_values.rs:19-30`).
  The key is the bare name `advance_time`.
- Events: `GameRuleChangedEvent { rule: GameRule, value }`
  (`crates/pumpkin-core/src/plugin/api/events/level/game_rule.rs:13`). It cannot name a mod rule.
- Wasm: WIT `game-rule` enum is mirrored by position with an unsafe transmute
  (`crates/pumpkin-wasm-host-v0_2/src/world.rs:84-92`, test `:2892`). Adding a variant in the middle
  of the generated enum breaks the plugins; the WIT is mirrored publicly and additive only.

### 3.2 Reference (26.3)

- `GAME_RULE` is a built-in registry (`MC/core/registries/BuiltInRegistries.java:306`) of
  `GameRule<T>`: category, bool or int type, argument type, codec, default, required features
  (`MC/world/level/gamerules/GameRule.java:15-40`). Registration helpers `registerBoolean` and
  `registerInteger` are public (`MC/world/level/gamerules/GameRules.java:130-175`), so a mod
  registers a rule with a plain call. NeoForge adds categories only
  (`RegisterGameRuleCategoryEvent`, `NF/event/RegisterGameRuleCategoryEvent.java`,
  `GameRuleCategory.registerModdedCategories` at `GameData.java:112`). The registry is not in
  `VANILLA_SYNC_REGISTRIES` (`NeoForgeRegistriesSetup.java:30-60`): the rule id never travels as a
  registry id.
- `/gamerule` is built from the registry: one literal per rule, short and qualified form
  (`MC/server/commands/GameRuleCommand.java:21-33`). A mod rule shows up by registration alone.
- Save: `GameRuleMap` is a `SavedData` `game_rules`, a dispatched map keyed by registry name
  (`MC/world/level/gamerules/GameRuleMap.java:10-14`). Rules of disabled features are removed on
  load (`GameRules.java:97-109`).
- Client sync: `ClientboundGameRuleValuesPacket` = `Map<ResourceKey<GameRule>, String>`
  (`MC/network/protocol/game/ClientboundGameRuleValuesPacket.java:10-14`), sent on request. Keys are
  identifiers, so `mymod:rule` works. Some rules have extra effects in `onGameRuleChanged`
  (`MC/server/MinecraftServer.java:2154-2184`); a mod effect uses `GameRuleChangedEvent`.
- `GameRuleChangedEvent` carries the `GameRule<?>` and the new value (`NF/event/level/GameRuleChangedEvent.java:22-48`).

### 3.3 What a mod rule needs and options

Needs: (1) registration of id, category, kind, default, range, features; (2) the `/gamerule`
command; (3) save and load in `game_rules.dat` under the key `modid:rule`; (4) client values packet
with the identifier; (5) a typed read for the mod (`get(rule)`); (6) the change event for mods and
Wasm plugins; (7) a custom category (name only, `gamerule.category.<ns>.<path>` language key lives
on the client).

| Option | Changes | Upstream merge cost | Performance | Wire and save risk |
|:--|:--|:--|:--|:--|
| A. Side table: `GameRuleRegistry` stays generated and untouched; add `mod_rules: BTreeMap<Identifier, ModRuleValue>` to `LevelInfo` and a frozen `ModRuleDef` table in `pumpkin-data` (hand-written, not generated) | `world_info/mod.rs` (+1 field), `data_files.rs` (write and read the extra keys), `gamerule.rs` (add literals from the table), `client_command.rs` (append rows), `ironpumpkin-mods` (`register_game_rule`), `GameRuleChangedEvent` (add `ModRuleChanged` as a new event struct) | Low. The generator and the enum stay as upstream. Touch points are 4 small hand-written files | Zero for vanilla rules. A mod rule is a map lookup; mod code caches a handle | `game_rules.dat` gets `modid:rule` keys; vanilla reads them if registered, else drops them (vanilla `GameRuleMap.CODEC` fails on an unknown key: not measured, check before shipping). Unknown keys must be kept, not lost, when the mod is absent |
| B. Generate mod rules into the enum at pack build time | codegen runs per modpack, enum and WIT change | High: generated file edited per pack, WIT transmute breaks | None | Breaks the rule "never edit `generated`"; patches that touch `generated` are rejected by the pack build (`openspec/config.yaml` context) |
| C. Replace the struct by a registry map for all rules | 65 read sites, codegen, WIT, wasm hosts | Very high, every merge | A map lookup on `mob_griefing` and `random_tick_speed` paths (tick hot path) | Large |

Recommendation: A. It leaves the vanilla rule set and the WIT enum intact. A mod reads its own
rule through a typed handle (`GameRuleHandle<bool>`) that `ModInit::register_game_rule` returns,
resolved to a slot index at freeze time so the read is one array index on an `Arc<LevelInfo>` it
already holds. Keep unknown `modid:rule` keys of `game_rules.dat` and write them back, so a world
survives a pack change.

Open point: Pumpkin keeps rules in the per-server `LevelInfo` and also reads a `world.level_info`;
which one is authoritative for a second world was not traced (`world/mod.rs:980-1018` uses the
world's field, `commands/gamerule.rs:55` uses the server's). A mod rule needs one answer.

## 4. Demand (from `mapping-table.md` and `modpack-usage.md`)

Modpack: FTB StoneBlock 4, 414 mods (`modpack-usage.md:39`). "Mods" below is the column
"Mods (srv)" of the tables unless noted.

| Item | Demand | Source | Status in the mapping table |
|:--|:--|:--|:--|
| `recipe_serializer` registrations | 74 mods register (98 touch), 343 call sites | `modpack-usage.md:1492` | `planned`, M5 (`mapping-table.md:337`) |
| `recipe_type` registrations | 49 mods register (72 touch), 192 call sites | `modpack-usage.md:1495` | `planned` (`:342`) |
| Recipe JSON files | 33018 in 188 mods (+23 in 10 mods, old path) | `modpack-usage.md:1587-1588` | datapack loader exists |
| `Ingredient` class | 141 mods (1516 refs) | `modpack-usage.md:1082` | `supported` as mapping (`mapping-table.md:1289`), tests with item id and tag |
| `Recipe`, `RecipeHolder`, `RecipeInput` | 118, 118, 118 mods | `modpack-usage.md:1110-1112` | machine code, not mapped |
| `RecipeType`, `RecipeSerializer`, `RecipeManager` | 116, 112, 109 mods | `modpack-usage.md:1114-1124` | planned |
| `SizedIngredient`, `DataComponentIngredient`, `CompoundIngredient` | 16, 10, 7 mods | `modpack-usage.md:865,918,961` | out of scope in the table (`mapping-table.md:1000`) |
| `IngredientType`, `ICustomIngredient` | 22, 27 mods; 18 mods register an ingredient type (14 via `DeferredRegister`) | `modpack-usage.md:810,834,1506` | `neoforge:ingredient_serializer` `planned` (`:418`) |
| Mixins into `RecipeManager` | 15 mods, top member `apply` 11 mods | `modpack-usage.md:1783,2202` | source patch case |
| `ICondition` | 42 mods; `ICondition.IContext` 32; `ConditionalOps` 12; `WithConditions` 8 | `modpack-usage.md:765-949`, gap row 19 at `mapping-table.md:997` | `planned`, M5 (`:788-793`) |
| Built-in condition classes | `ModLoadedCondition` 7, `AndCondition` 2, `FalseCondition` 2, `NotCondition` 1, `OrCondition` 1, `TagEmptyCondition` 1, `TrueCondition` 1, `ItemExistsCondition` 1 | `modpack-usage.md:961` | `planned` |
| `condition_codec` registrations | 16 mods register, 31 call sites; 11 mods use `condition_serializer` | `modpack-usage.md:1505,1513` | out of scope (`mapping-table.md:358`, `:1004`) |
| Custom game rules | 5 mods, 9 call sites (`GameRules.register`) | `modpack-usage.md:1632` | `planned`, M5, no task (`mapping-table.md:445,996`) |
| `GameRules` class (read) | 53 mods; `GameRules$Key` 52, `$Value` 17, `$BooleanValue` 16, `$Type` 10, `$Category` 6 | `modpack-usage.md:2953,3033-3095` | read: supported; custom: gap (`mapping-table.md:1444-1453`) |

Reading the numbers:

- The jars use 21.1 names (`world.level.GameRules` is `gamerules.GameRules` in 26.3,
  `modpack-usage.md:2953`). The port step maps them.
- The condition demand (42 mods) is 6 times larger than the sum of the built-in condition class
  uses (15): most mods call the `ICondition` interface in their own datagen or custom codec. The
  JSON `neoforge:conditions` key in the 33018 recipes and other data is not counted in the scan.
  Count it with a script over the unpacked jars before sizing the work (not done here: needs
  the jars).
- Custom game rules: 5 mods. This is the smallest demand of the three and the cheapest option (A)
  already covers it.

## 5. Not measured or open

- `neoforge:conditions` key counts in the data of the 414 jars: not scanned (jars not read).
- Wire capture of `neoforge:recipe_content` from a NeoForge 26.3 server: none. S2 is from source.
- Vanilla `GameRuleMap.CODEC` on an unknown key in `game_rules.dat`: from source only, not run.
- Whether Wasm plugin ids count for `mod_loaded`: owner decision.
