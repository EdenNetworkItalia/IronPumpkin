# Generic static registries, item abilities and mod-defined registries (phase 6 research)

Scope: the 76 static registries of `design.md` (table "Static registries a `DeferredRegister` can target", rows with `M5 engine extension:`, lines 992-1083), item abilities, three small `common` classes, and mod-defined registries.

Demand numbers come from section 6 of `openspec/changes/neoforge-shaped-plugin-api/modpack-usage.md` (lines 1480-1590). "Registering" is the number of mods that register into the registry (`DeferredRegister`, `Registry.register`, `RegisterEvent`). "Touching" is the number of mods that name it at all. A row with 0 and 0 is absent from the scan. Two scan names are my mapping and are not verified: `item_sub_predicate_type` -> `data_component_predicate_type`, and `blockstate_provider_type` + `block_state_provider_type` -> `worldgen/block_state_provider_type`. The scan row `loot_number_provider_type` (3 registering, 8 touching) has no row among the 76. It is probably `context_float_provider_type` and `context_int_provider_type` (26.3 names, `minecraft-src/net/minecraft/core/registries/BuiltInRegistries.java:236-240`). I did not verify this.

Verified: the M2 registry code, the M3 sync code, the NeoForge client sync handler, the loot, trigger, consume effect, recipe loader and command argument sites, and the item-ability sites of section 3. Not read: the use sites of the enchantment, environment attribute, spawn condition, permission, slot source, provider and worldgen registries. Their group is a judgement from the registry name and the generated file names, marked "(judgement)".

## 1. Groups by what a mod entry needs

- (a) The entry is a name, an id and plain data. The server needs no code of the mod. Where the id travels in a packet, the client needs it in the registry sync.
- (b) The entry is an id plus server behaviour that a Rust trait object carries: a decoder, a predicate, an effect, a writer. The engine has a hand-written place that runs the behaviour, or a small one can be added.
- (c) The entry needs an engine change beyond a table: a subsystem, a generator, AI, or a wire parser.

| Group | Registries | Synced in NeoForge | Largest registering demand (mods) |
|:--|--:|--:|:--|
| (a) id and data | 13 | 11 | particle_type 41, point_of_interest_type 9, potion 5, custom_stat 5, map_decoration_type 2 |
| (b) id and trait object | 31 | 5 | recipe_serializer 74, recipe_type 49, trigger_type 19, loot_function_type 16, loot_pool_entry_type 7, loot_condition_type 7, data_component_predicate_type 5, enchantment_effect_component_type 3 |
| (c) engine change | 32 | 3 | command_argument_type 21, fluid 16, worldgen/feature_type 13, worldgen/structure_type 8, worldgen/placement_modifier_type 8, worldgen/structure_processor 7, villager_profession 6, worldgen/structure_piece 6 |
| Total | 76 | 19 | |

A mod registers in several registries, so the demand column lists the largest rows. It is not a sum of mods. 38 of the 76 registries have no registering mod in the scan (counted from `groups.txt`, produced by `gen_groups.py` in this folder).

### 1.1 Group (a): 13 registries

| Registry | Synced | Mods registering | Mods touching |
|:--|:--|--:|--:|
| `particle_type` | yes | 41 | 49 |
| `game_event` | yes | 0 | 6 |
| `potion` | yes | 5 | 29 |
| `custom_stat` | yes | 5 | 16 |
| `stat_type` | yes | 0 | 7 |
| `villager_type` | yes | 0 | 7 |
| `position_source_type` | yes | 0 | 0 |
| `number_format_type` | yes | 0 | 0 |
| `map_decoration_type` | yes | 2 | 5 |
| `recipe_book_category` | yes | 0 | 0 |
| `point_of_interest_type` | yes | 9 | 19 |
| `context_key_set` | no | 0 | 0 |
| `input_control_type` | no | 0 | 0 |

Why (a):

- The generated type of 7 of the 13 is an enum: `Particle` (`crates/pumpkin-data/src/generated/particle.rs:3`, 128 entries), `GameEvent` and `PositionSourceType` (`game_event.rs:4,15`), `PoiType` (`poi_type.rs:5`), `VillagerType` (`villager.rs:7232`), `CustomStatistic` (`statistic.rs:57`). A mod entry cannot be a value of a generated enum, so the packet path must carry an id. `CParticle` already does: `particle_id: VarInt` and raw `data` bytes (`crates/pumpkin-protocol/src/java/client/play/particle.rs:34-38`). A mod particle needs no protocol change, only an id from the registry and the option bytes from the mod.
- `potion` and `map_decoration_type` are structs (`potion.rs:4`, `map_decoration.rs:2`). A mod entry can be a leaked struct, as `StatusEffect` is for mob effects (`dynamic_registries.rs`, `Registry<StatusEffect>`).
- `point_of_interest_type`: native mods already add blocks to a vanilla job site POI type through `extend_poi_types` (`crates/pumpkin-core/src/content/poi.rs:1-6`; fired in `crates/ironpumpkin-mods/src/lifecycle.rs`, after `data_maps::post`). A new POI type is a new value of the enum `PoiType`, which 14 files use (`rg "PoiType::"`). It is the hardest of (a).
- `game_event`: 96 files use `GameEvent::` (`rg "GameEvent::"`). A mod event needs an id-based emit path next to the enum. Demand: 6 mods touch it, 0 register.
- `context_key_set`, `input_control_type` (judgement): the server forwards data to the client or reads a result.

### 1.2 Group (b): 31 registries

| Registry | Synced | Mods registering | Mods touching |
|:--|:--|--:|--:|
| `loot_pool_entry_type` | no | 7 | 13 |
| `loot_function_type` | no | 16 | 29 |
| `loot_condition_type` | no | 7 | 16 |
| `loot_nbt_provider_type` | no | 0 | 0 |
| `loot_score_provider_type` | no | 0 | 0 |
| `context_float_provider_type` | no | 0 | 0 |
| `context_int_provider_type` | no | 0 | 0 |
| `float_provider_type` | no | 0 | 0 |
| `int_provider_type` | no | 1 | 4 |
| `height_provider_type` | no | 2 | 5 |
| `block_predicate_type` | no | 0 | 0 |
| `trigger_type` | no | 19 | 28 |
| `entity_sub_predicate_type` | no | 1 | 4 |
| `data_component_predicate_type` | no | 5 | 11 |
| `consume_effect_type` | yes | 0 | 0 |
| `enchantment_effect_component_type` | no | 3 | 9 |
| `enchantment_level_based_value_type` | no | 0 | 0 |
| `enchantment_entity_effect_type` | no | 1 | 3 |
| `enchantment_location_based_effect_type` | no | 0 | 0 |
| `enchantment_value_effect_type` | no | 0 | 0 |
| `enchantment_provider_type` | no | 0 | 0 |
| `recipe_display` | yes | 0 | 0 |
| `slot_display` | yes | 0 | 0 |
| `slot_source_type` | no | 0 | 0 |
| `recipe_type` | yes | 49 | 72 |
| `recipe_serializer` | yes | 74 | 98 |
| `spawn_condition_type` | no | 0 | 0 |
| `environment_attribute` | no | 0 | 0 |
| `attribute_type` | no | 0 | 0 |
| `permission_type` | no | 0 | 0 |
| `permission_check_type` | no | 0 | 0 |

Use sites that I verified, and what a mod entry needs there:

- Loot (`loot_pool_entry_type`, `loot_function_type`, `loot_condition_type`, `loot_nbt_provider_type`, `loot_score_provider_type`): the datapack path is `parse_loot_table_with_predicates` (`crates/pumpkin-util/src/loot_table/parser.rs:34`) into the `Dynamic*` enums `DynamicLootCondition` (`crates/pumpkin-util/src/loot_table.rs:172`) and `DynamicLootEntryKind` (`:350`). The static tables use the `Copy` enums `LootCondition` (`:7`) and `LootFunctionKind` (`:103`). A mod type needs a `Custom` variant in the `Dynamic*` enums only, holding an `Arc<dyn ..>`. The `Copy` enums and `generated/loot_table.rs` stay as they are. The precedent is `crates/ironpumpkin-mods/src/loot.rs:1-30`: `register_loot_modifier_type` takes an id and a decode callback, and `close_registration` runs before the attachments freeze (`lifecycle.rs:81`).
- `trigger_type`: `AdvancementTrigger` is a closed enum with fixed fire sites (`crates/pumpkin-core/src/entity/player/advancement/trigger.rs:7`). Conditions decode in `advancement/predicate.rs`. A mod trigger needs a `Custom { id, .. }` variant, a decode callback for the `conditions` object, and a `fire(player, trigger, context)` call that the mod makes (NeoForge `CriterionTrigger#trigger`).
- `consume_effect_type`: `ConsumeEffect` is a closed enum with a manual `Hash`, `to_str`, `registry_id` and NBT writer (`crates/pumpkin-data/src/data_component_impl/food.rs:175-260`). It runs in `crates/pumpkin-core/src/entity/living.rs:5224-5297`. A mod effect needs a `Custom(Arc<dyn ConsumeEffectBehaviour>)` variant. The file is hand-written, not generated, but `pumpkin-data` is shared with upstream. Demand 0 in the scan.
- `recipe_type`, `recipe_serializer`: `parse_recipe` matches a fixed list of `minecraft:` types (`crates/pumpkin-core/src/data/datapack/recipe_loader.rs:9-40`) into the closed `DynamicRecipe` (`pumpkin-protocol/src/codec/recipe.rs`). This is the largest demand (74 and 49 mods). The mapping table says mod machine recipes are datapack JSON that the mod reads itself (`mapping-table.md:990`). Then the engine needs: a serializer id that decodes a JSON object into a mod value, a store by type for `get_recipes_for(type)`, and `/reload`. It needs no crafting-grid logic. A mod recipe that must join the crafting grid, the furnace or the recipe book is group (c) for that recipe type.
- `recipe_display`, `slot_display`: the server writes the display body bytes in the recipe book packet, so a mod needs a writer for its own display type.
- `entity_sub_predicate_type`, `data_component_predicate_type`, `block_predicate_type` (judgement): predicates read by loot and advancement code (`advancement/predicate.rs`). They fit the trait-object slot.
- Enchantment (6 registries), `environment_attribute`, `attribute_type`, `spawn_condition_type`, `permission_type`, `permission_check_type`, `slot_source_type`, `float_provider_type`, `int_provider_type`, `height_provider_type`, `context_float_provider_type`, `context_int_provider_type` (judgement): the data that uses them (`generated/enchantment.rs`, `generated/environment_attribute.rs`) is generated. A trait-object slot works only where a hand-written path decodes the datapack form. For enchantments the codegen decodes the effects, so a mod effect needs a parallel dynamic path or a generator change. That makes 7 registries (6 enchantment, `environment_attribute`) whose cost sits at the use site. They stay in (b) because the entry is a trait object.

### 1.3 Group (c): 32 registries

| Registry | Synced | Mods registering | Mods touching |
|:--|:--|--:|--:|
| `fluid` | yes | 16 | 81 |
| `chunk_status` | no | 0 | 0 |
| `rule_test` | no | 1 | 4 |
| `rule_block_entity_modifier` | no | 0 | 0 |
| `pos_rule_test` | no | 0 | 0 |
| `command_argument_type` | yes | 21 | 28 |
| `villager_profession` | yes | 6 | 19 |
| `memory_module_type` | no | 2 | 5 |
| `sensor_type` | no | 2 | 5 |
| `activity` | no | 0 | 0 |
| `worldgen/carver_type` | no | 1 | 3 |
| `worldgen/feature_type` | no | 13 | 17 |
| `worldgen/structure_placement` | no | 3 | 7 |
| `worldgen/structure_piece` | no | 6 | 9 |
| `worldgen/structure_type` | no | 8 | 12 |
| `worldgen/placement_modifier_type` | no | 8 | 13 |
| `worldgen/block_state_provider_type` | no | 2 | 5 |
| `worldgen/foliage_placer_type` | no | 1 | 4 |
| `worldgen/trunk_placer_type` | no | 2 | 5 |
| `worldgen/root_placer_type` | no | 0 | 0 |
| `worldgen/tree_decorator_type` | no | 1 | 4 |
| `worldgen/feature_size_type` | no | 0 | 0 |
| `worldgen/biome_source` | no | 1 | 2 |
| `worldgen/chunk_generator` | no | 4 | 5 |
| `worldgen/material_condition_type` | no | 0 | 0 |
| `worldgen/material_rule_type` | no | 0 | 0 |
| `worldgen/density_function_type` | no | 1 | 1 |
| `worldgen/structure_processor` | no | 7 | 14 |
| `worldgen/structure_pool_element` | no | 2 | 5 |
| `worldgen/pool_alias_binding` | no | 0 | 0 |
| `game_rule` | no | 0 | 0 |
| `ticket_type` | no | 0 | 0 |

Split of the 32:

- 24 are world generation: 20 `worldgen/*` registries, `chunk_status`, `rule_test`, `rule_block_entity_modifier`, `pos_rule_test`. The design says the generator is Rust code in `pumpkin-world` and plugins hook phases only (`design.md:1031-1050`). The 20 `worldgen/*` rows add up to 60 registering-mod entries (a mod counts once per row). Mapping row 15 (World generation, 72 mods) already says: plan an M5 registration API for world generation types, and generator changes as source patches.
- 8 are other:
  - `fluid` (16 registering, 81 touching): fluid states and flow are engine code, and the generated `crates/pumpkin-data/src/generated/fluid.rs` (1049 lines) holds block state links. It is a subsystem, not a table.
  - `command_argument_type` (21 registering): `ArgumentType` is a closed enum with `repr(u32)` ids (`crates/pumpkin-protocol/src/java/client/play/commands.rs:187`). The wire cost is small: a variant `Custom { id, bytes }` and the type id from the registry sync. The server parse side is the mod command code that `ModInit::register_command` already hosts. It is the lightest (c) and a candidate to move to (b). I did not read the command tree builder, so this is a judgement.
  - `villager_profession` (6): `VillagerProfession` is a generated enum with `from_i32` (`crates/pumpkin-core/src/entity/passive/villager/data.rs:186`), 19 files use it, and a profession needs a POI type, trades and brain behaviour. `villager_type` stays in (a) (7 files).
  - `memory_module_type`, `sensor_type`, `activity` (2, 2, 0 registering): the brain has a closed `SensorType` enum (`crates/pumpkin-core/src/entity/ai/brain/sensing/mod.rs:39`), a `MemoryModuleType<T>` with a raw `u8` id (`brain/memory/mod.rs:44`) and an `ActivitySet(u32)` bitmask (`brain/activity.rs:25`). A mod sensor or activity changes the sensor dispatch and may exceed the bitmask.
  - `game_rule` (53 mods touch the `GameRules` class, 0 register in the scan table): `GameRule` is a generated enum with 34 uses in 15 files (`rg "GameRule::"`). The level data, the command and the client sync read it.
  - `ticket_type` (0 registering): `pumpkin-world` exposes no chunk tickets (`mapping-table.md` row 24, "Packages common.world").

Counts: (a) 13, (b) 31, (c) 32. Of (c), 24 are world generation and 8 are other.

## 2. How far the M2 registry reaches

### 2.1 What the M2 and M3 code does

- `crates/pumpkin-data/src/dynamic_registries.rs` holds four registries (sound event, mob effect, attribute, data component type). The pattern: a `Pending` struct with a `BTreeMap<String, Definition>` per registry (`:224-232`), `register_*` functions behind `with_pending` (`:378`), and `freeze()` (`:575`) that builds one generic container `Registry<T>` with a name index (`:336-361`). An id is the first id plus the index in name order. The first id is the generated count (`first_sound_event_id`, `first_data_component_type_id`, `:608-629`).
- `crates/pumpkin-data/src/dynamic.rs:171` has a closed `ContentKind` enum with 7 kinds, used in errors, `is_registered`, `tag_key` and `Display`. `dynamic::freeze()` (`:1612`) calls the module freeze, then builds the block, item and entity tables.
- The M3 sync is `SYNCED_REGISTRIES: [(Identifier, RegistrySnapshotFn); 7]` (`crates/pumpkin-core/src/net/java/configuration_tasks.rs:56-81`). A row is a registry name and a function `fn(&ContentTables) -> Vec<(i32, Identifier)>`. `RegistrySyncPayloads::encode` writes one start payload and one `neoforge:frozen_registry` payload per row, once, and caches the bytes (`:114-150`). The row `neoforge:synced_attachment_types` already points at `crate::plugin::attachments::synced_types_snapshot`, a registry that lives outside `ContentTables`.
- NeoForge side: the server syncs only registries with `setSync(true)`. These are 28 vanilla ones (`NeoForgeRegistriesSetup.java:32-63`) and the mod registries built with `RegistryBuilder.sync(true)` (`RegistryBuilder.java:92`). The client keeps its own ids for a registry that is not in the start payload. It disconnects when the server lists a registry and does not send it (`client/network/ClientPayloadHandler.java:96-98`). It disconnects when a snapshot has a key that it does not know (`:103-108`, `registries/RegistryManager.java:165-181`).

### 2.2 Can one mechanism back (a) and (b)?

Yes for the id table of all 44 registries of (a) and (b), without a change to a generated file. Conditions and costs:

1. Do not add 44 `ContentKind` variants. Add a runtime table keyed by registry id (`&'static str`) next to `Pending` and `Registry<T>`. A spec per registry gives: the registry id, the vanilla entry count, the name of vanilla id `n`, and the `synced` flag. A new module file keeps the diff out of `dynamic.rs` (3432 lines).
2. The vanilla count and names come from the generated lookups at freeze, with no generated change: probe `Particle::from_id(n)` until `None`, use `MapDecorationType::ALL.len()` (`map_decoration.rs:373`), `PoiType::ALL` (`poi_type.rs:30`), `Potion::from_id(n: u8)` (`potion.rs:672`). The APIs differ per file, so each spec has a small hand-written function. A unit test per registry that compares the count with the generated one catches an upstream merge that adds an entry.
3. Per-registry trait: the id table lives in `pumpkin-data`. The trait object lives where the use site lives (`pumpkin-core` for loot, trigger, consume effect and recipe; `pumpkin-protocol` for display writers), because `pumpkin-data` cannot name those types. The mod API registers `(registry, name, entry)`. The host stores the trait object next to its use site, joined by name.
4. The use site decides the cost, not the table. For (a) the use site is a packet writer that takes an id, and `CParticle` already does. For (b) the cost is the closed enum at the use site (section 1.2): `Custom` variants in `DynamicLootCondition`, `DynamicLootEntryKind`, `AdvancementTrigger`, `ConsumeEffect`, `DynamicRecipe`. None is under `generated/`.
5. Where the generated type is an enum and an API takes the enum (`Particle`, `PoiType`, `GameEvent`, `GameRule`, `VillagerProfession`), a mod entry needs a parallel id-based entry point. This is the real cost of (a) for `game_event` (96 files) and `point_of_interest_type` (14 files).

| Option | Files and crates | Upstream merge cost | Performance risk | Wire or save risk |
|:--|:--|:--|:--|:--|
| 1. One runtime generic table with a spec per registry (recommended) | new module in `pumpkin-data`, one spec fn per registry, row list in `configuration_tasks.rs`, `register_*` in `ironpumpkin-mods` | low: new files and one line per registry in `SYNCED_REGISTRIES` | none on hot paths: lookups read a frozen `OnceLock` table, as M2 does | names in saves, ids never persist (as M2) |
| 2. One `ContentKind` variant and one `Pending` field per registry (copy M2) | `dynamic.rs` and `dynamic_registries.rs` grow per registry | high: both files grow and every `match` on `ContentKind` changes | none | same |
| 3. Generator change that emits counts and name accessors for all registries | `pumpkin-codegen` and every `generated/*.rs` | very high: generated files conflict on every merge | none | none |

I recommend option 1. It leaves the generated files and the large files unchanged, and one test per registry covers drift.

### 2.3 What changes in the M3 sync, per registry

Rule: a registry needs a snapshot row if the registry is synced in NeoForge and the server writes a mod id of it into a packet. A registry whose entries travel by name needs no row (the sound event precedent, `configuration_tasks.rs:52-54`). A row lists every vanilla id in order, then the mod entries, with exact vanilla names. A missing or reordered vanilla name breaks the client apply (`RegistryManager.java:165-181`).

| Registry (synced) | Mod id in a Pumpkin packet | Snapshot row |
|:--|:--|:--|
| `particle_type` | yes: `CParticle.particle_id` | add |
| `potion` | yes: potion contents component (judgement) | add |
| `custom_stat`, `stat_type` | yes: award stats packet (`NeoForgeRegistriesSetup.java:43,53`) | add |
| `map_decoration_type` | yes: map data (judgement) | add |
| `villager_type`, `villager_profession` | yes: entity data (`NeoForgeRegistriesSetup.java:44-45`) | add |
| `game_event`, `point_of_interest_type` | debug packets only (`NeoForgeRegistriesSetup.java:61-62`) | optional: absence is safe while Pumpkin writes no mod id |
| `position_source_type`, `number_format_type`, `recipe_book_category` | only if the server writes the type id; 0 in the scan | add when used |
| `consume_effect_type`, `recipe_display`, `slot_display` | yes: item component and recipe book packets | add |
| `recipe_type`, `recipe_serializer` | yes: recipe sync payload; demand 49 and 74 | add |
| `command_argument_type` | yes: command tree packet | add (group c) |
| `fluid` | yes: fluid and block state packets | add (group c) |
| the other 57 | no | none |

Cost: up to 19 new rows against 7 today. `SYNCED_REGISTRIES` is a `const` array of fixed length 7, so it becomes a `Vec` or a slice built at freeze. The encoded bodies are cached once (`RegistrySyncPayloads::get`, `:141-150`). The cost is a boot-time encode and one more payload per registry in the configuration phase of each connection. I did not measure the byte size. `particle_type` has 128 vanilla entries and most others are smaller.

## 3. Item abilities, `SpecialPlantable`, `IMinecartCollisionHandler`, `FarmlandWaterManager`

### 3.1 Item abilities (45 mods name `ItemAbility`, 41 name `ItemAbilities`, 114 uses)

NeoForge: `ItemAbility` is an interned name (`neoforge-src/.../common/ItemAbility.java:12-30`). `canPerformAction(stack, ability)` is false by default, except `SWORD_SWEEP` (tag `SWORDS`) and `SHOVEL_DOUSE` (tag `DOUSES_CAMPFIRES`) (`extensions/IItemExtension.java:343-350`). Vanilla item classes override it with `DEFAULT_*_ACTIONS` sets (`TridentItem.java:151`, `ShearsItem.java:95`, `FlintAndSteelItem.java:62`, `FireChargeItem.java:91`, `BrushItem.java:146`, `SpyglassItem.java:53`, `FishingRodItem.java:70` in the patched vanilla source). There are 13 stock abilities (`ItemAbilities.java:24-94`). Callers ask the stack: `PumpkinBlock.java:32` (`SHEARS_CARVE`), `TripWireBlock.java:105` (`SHEARS_DISARM`), `CampfireBlock.java:91` (`SHOVEL_DOUSE`), the loot condition `CanItemPerformAbility` (`CanItemPerformAbility.java:48`), and `getToolModifiedState` (`FlintAndSteelItem.java:31`).

Pumpkin hard-codes the answer by item identity or tag at each site:

| Ability | Pumpkin site |
|:--|:--|
| `shears_dig`, `shears_harvest`, `shears_carve`, `shears_disarm`, `shears_trim` | `Item::SHEARS` compares in `world/loot.rs:353,547`, `world/loot/modifiers.rs:425`, `world/loot/generation.rs:244`, `entity/mod.rs:6681`, `entity/passive/snow_golem.rs:289`, `entity/passive/sheep.rs:366`, `block/blocks/redstone/tripwire.rs:95`; the item behaviour `item/items/shears.rs:23` (all under `crates/pumpkin-core/src`) |
| `shovel_douse` | tag test `MINECRAFT_DOUSES_CAMPFIRES` in `block/blocks/campfire.rs:123` |
| `sword_sweep` | the attack type in `entity/player.rs:1848`; no stack test; `SweepAttackEvent` can force it (`plugin/player_events.rs:198-238`) |
| `firestarter_light` | `item/items/ignite/ignition.rs:42`; the event `BlockEventBlockToolModificationEvent` carries the name as `&'static str` (`plugin/api/events/level/block.rs:209-261`) |
| `trident_throw`, `spyglass_scope`, `brush_brush`, `fishing_rod_cast` | item behaviours registered by id: `item/items/trident.rs:25`, `spyglass.rs:14`, `brush.rs:27`, `fishing_rod.rs:21`; `net/java/play/use_item.rs:171` |

What a hook needs:

1. One function `can_perform_action(stack, ability: &str) -> bool` in `pumpkin-core`. It has a default set per vanilla item (the table above, or the tags that NeoForge uses) and a registration call for mod items: `ModInit::register_item_ability(item, ability)`, plus a closure form for a stack-dependent answer. The ability type is a `&'static str`, as the event already uses.
2. Each site above replaces its identity compare by the function. That is about 12 call sites. Each is hand-written code in `pumpkin-core`, none is generated. The merge cost is moderate: `entity/mod.rs` and `entity/player.rs` are large upstream files, and each edit is one line.
3. Mods mostly ask whether a stack can do a vanilla action. A few declare new abilities (`ItemAbility.get("name")` is open). The cheapest correct hook is the function plus the registration. The event `BlockEventBlockToolModificationEvent` stays unchanged: axe, hoe and shovel transforms are data in Pumpkin (`generated/block_transformer.rs`), and NeoForge 26.3 fires the event for no tool of those (`block.rs:218-221`).
4. Performance: an item set or tag lookup at the sites above. The only frequent path is the sweep test, once per attack.

### 3.2 `SpecialPlantable` (8 mods), `IMinecartCollisionHandler` (0 mods), `FarmlandWaterManager` (2 mods)

- `SpecialPlantable` (`common/SpecialPlantable.java`): an item that places a plant at a position (`canPlacePlantAtPosition`, `spawnPlantAtPosition`) and says whether a villager may plant it (`villagerCanPlantItem`). NeoForge uses it in the villager pick-up and farm behaviour (`Villager.java:771`, `HarvestFarmland.java:123` of the patched source). Pumpkin has a `HarvestFarmland` behaviour (`crates/pumpkin-core/src/entity/ai/brain/behavior/work/harvest_farmland.rs`). A hook needs a trait object registered per item (`register_special_plantable(item, impl)`), two calls inside `harvest_farmland.rs` and the villager item pick-up filter, and the player placement path for a non-block plant. I did not read `harvest_farmland.rs`, so "two call sites" is a judgement.
- `IMinecartCollisionHandler` (`common/IMinecartCollisionHandler.java`, 0 mods in the scan, `mapping-table.md:958`): four methods that replace minecart collision and box code. Pumpkin: `entity/vehicle/minecart.rs:571-576` has `is_pushable` and `push`, and `entity/mod.rs:1067-1180` has the push logic. A hook is a global handler slot read by these functions. No mod uses it: skip.
- `FarmlandWaterManager` (`common/FarmlandWaterManager.java`): water tickets per chunk (`addAABBTicket`, `addCustomTicket`, `hasBlockWaterTicket` at `:113`). Patched `FarmlandBlock.java:127` calls it. Pumpkin: `block/blocks/farmland.rs:94` calls `is_water_nearby` (`:169-187`, a 9 x 2 x 9 scan of fluid tags per random tick). A hook is a per-world ticket store (boxes with an owner) that `is_water_nearby` checks after the fluid scan. Cost: one extra check in a path that already scans 162 positions. Check for an empty store first. Tickets leave at chunk unload (`NeoForgeEventHandler.java:84`), so the store is keyed by chunk. Demand is 2 mods.

Recommendation: item abilities first (45 mods), `SpecialPlantable` second (8 mods), `FarmlandWaterManager` only if one of its 2 mods is in the target list, and skip the minecart handler (0 mods).

## 4. `NewRegistryEvent`, `RegistryBuilder`, `DeferredRegister#makeRegistry` (48 mods)

Demand: `RegistryBuilder` 47 mods, `NewRegistryEvent` 31, `makeRegistry` 12, `ModifyRegistriesEvent` 2 (`modpack-usage.md:756,794`; `mapping-table.md` gap row 20, 48 mods). The mapping table marks it out of scope: "a mod keeps its own lookup tables in its own code".

What the NeoForge API gives a mod registry (`RegistryBuilder.java:23-130`): a key, a `defaultKey` (defaulted registry), `withIntrusiveHolders`, callbacks (`onAdd`, `onBake`, `onClear`), `maxId`, `sync(boolean)` (default false), and `create()`. `NewRegistryEvent` fires on the mod bus on both sides, before `RegisterEvent`. Each new registry is then a `DeferredRegister` target.

Side: the registry exists on the server for every mod that defines it. It is synced only when the builder sets `sync(true)`. The scan does not record the flag, so I cannot say how many of the 48 sync. NeoForge builds 11 registries of its own with this event (`NeoForgeRegistriesSetup.java:66-77`). Pumpkin already has two of that kind in `ironpumpkin-mods`, not in `pumpkin-data`: `neoforge:attachment_types` (frozen at `lifecycle.rs:83-86`, synced through `neoforge:synced_attachment_types`) and the loot modifier serializers (`loot.rs`).

| Option | What it is | Files and crates | Upstream merge cost | Risk |
|:--|:--|:--|:--|:--|
| A. Status quo | A mod keeps a Rust `static` table. An addon mod calls a function of the base mod crate | none | none | An addon that registers through `RegisterEvent` into a registry of another mod has no common API. Each mod pair needs glue. The addon side of the 48 mods is the loss |
| B. Server-only `CustomRegistry<T>` in `ironpumpkin-mods` (recommended) | A generic typed table. `ModInit::register_registry(key, builder)` runs before the `RegisterEvent` loop (`lifecycle.rs:71`). Entries come through `RegisterEvent`. It freezes after the last event, in the slot of `attachments::freeze` (`lifecycle.rs:83-86`). Features: default key, `onAdd` and `onBake` callbacks, name lookup, ids in name order, a `DeferredRegister`-style helper. No `pumpkin-data` change | `ironpumpkin-mods` only: new `custom_registry.rs`, edits in `lifecycle.rs` and `registration.rs` | none: new files in an MIT/Apache crate | none on the wire; saves hold names |
| C. B plus `sync(true)` | The frozen registry gets a snapshot row. The row list (`configuration_tasks.rs:56-81`) must become runtime built; `synced_types_snapshot` is the precedent | B plus `configuration_tasks.rs` | one edit in a file that already carries fork code | wire risk: the client throws for a non-empty snapshot of a registry it does not define (`RegistryManager.java:136-142`) and disconnects on an unknown key (`ClientPayloadHandler.java:103-108`) |
| D. Datapack-driven registries with codecs | A mod registry that loads JSON entries from `data/<ns>/<registry>/` | datapack loader, registry codecs | high | not what the 48 mods use (`RegistryBuilder`); do not mix |

I recommend B now and C only when a target mod needs `sync(true)`. The scan shows no such mod, so C now is speculative. B adds what the mapping table lacks: a cross-mod API to register into a registry that another mod owns. It needs no generated file and no change in `pumpkin-core`.

## 5. Recommendations in order

1. Build the generic id table (option 1 of 2.2) with `particle_type`, `potion`, `custom_stat`, `stat_type`, `map_decoration_type` and `villager_type` as the first (a) registries. Each has a snapshot row and a count test. Demand: 41 registering mods for `particle_type`, 5 for `potion` and `custom_stat`.
2. Build the `Custom` use-site variants for the highest-demand (b) registries: `recipe_serializer` and `recipe_type` (74, 49), `trigger_type` (19), `loot_function_type` (16), then `loot_pool_entry_type` and `loot_condition_type` (7, 7).
3. Add the item ability function (3.1) and `CustomRegistry<T>` (option B of section 4). Both avoid generated files. They touch large upstream files only with one-line call-site edits.
4. Keep (c) as separate chapters: world generation (24 registries, plan row 15), `fluid`, `command_argument_type` (candidate for (b)), `villager_profession`, brain types, `game_rule`, `ticket_type`.

## 6. Not measured

- The byte size of the extra registry payloads per connection.
- Whether any of the 48 mods builds its registry with `sync(true)`.
- The use sites of the registries marked "(judgement)".
- The command tree builder in `pumpkin-command` and `harvest_farmland.rs` (known by name only).
- The scan mapping of three registry names (see the top of this file).
