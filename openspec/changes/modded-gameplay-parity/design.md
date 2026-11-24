# Design

## Context

IronPumpkin is a fork of the Pumpkin Minecraft server. It merges upstream Pumpkin often, so each change to an upstream file costs merge work. This change is the milestone M5 "Modded gameplay parity". The roadmap of the pinned status issue #29 calls it phase 6.

Terms:
- A native mod is a Rust crate that links against `ironpumpkin-mods`. A modpack includes it.
- A ported mod is a NeoForge mod. A native mod replaces its server half.
- The client half of a ported mod is the original NeoForge jar on a NeoForge 26.3 client.
- A Wasm plugin is a v0.2 Pumpkin plugin. It is not a mod.
- The freeze is the one point at startup where the mod tables close and get their ids. It runs after the startup hook in which mods register their content, and before the first world loads (`crates/pumpkin-core/src/server/mod.rs:457`). A data pack reload does not freeze again.
- The inventory is the set of tables in `openspec/changes/neoforge-shaped-plugin-api/design.md`. Its section "How the generator classifies a row" defines the statuses `supported`, `planned`, `not supported` and `client-only`.
- The demand of a row is the number of mods of the reference modpack scan that use it on the server. The scan is `openspec/changes/neoforge-shaped-plugin-api/modpack-usage.md`, and the Mods column of `mapping-table.md` in the same folder gives the demand per row.
- Group (a): a static registry whose entries hold an id and data, with no server behaviour.
- Group (b): a static registry whose entries hold an id and a server behaviour in a Rust trait object.
- Group (c): a static registry that needs an engine change (`research/generic-registries.md` sections 1.1 to 1.3).
- Registry sync sends the ids of a registry to the client in the configuration phase. Recipe content sync sends recipes to the client in the play phase.
- The performance guardrail is the rule of `openspec/config.yaml`. The benches that it names must not report a regression that Criterion marks as significant.

M4 gives native mods the NeoForge events and APIs over the content kinds that the M2 content registry backs. These kinds are blocks, items, entity types, tags, data component types, sound events, mob effects and attributes. The inventory marks 140 rows `planned` with the prefix `M5 engine extension:`. Each of these rows needs an extension point that the engine lacks. Task 2.41 of M4 (#184) adds one row per extension interface hook. The hooks with no engine call site get the same prefix. They are in addition to the 140 rows.

M4 wave 10 owns four vanilla parity fixes that parts of this change need (M4 tasks.md, tasks 2.48 to 2.51):
- #189 adds the vanilla placement modifiers `cuboid`, `random_chance` and `randomly_selected` to world generation.
- #190 saves the vanilla forced chunks, and it fixes the positions and the block of the fluid spread event.
- #191 sends the vanilla recipe sync, cooks data pack cooking recipes, and reads `"#tag"` ingredient strings.
- #192 sends the vanilla update tag of each block entity in the chunk packet, not its saved NBT.

The research for this change is in `research/` of this folder, one file per area. Its `path:line` pointers are valid at master `db6227715`. An implementer checks them against the current tree. The labels "option A", "K1", "T1" and "S1" name the options of the research file that a decision cites.

| File | Area |
|:--|:--|
| `research/block-entities-menus.md` | block entity types, menu types, data slots |
| `research/generic-registries.md` | the 76 static registries, item abilities, mod-defined registries |
| `research/recipes-conditions.md` | recipes, ingredients, `neoforge:conditions`, game rules |
| `research/fluids.md` | fluids, `FluidType`, flow, buckets, cauldrons, entity interaction |
| `research/worldgen-tickets.md` | world generation, biome modifiers, chunk tickets, #135 |

The 140 rows map to the decisions below. The `fluid` and `game_rule` rows of the static registry table belong to the Fluids and Game rules decisions.

| Decision | Rows |
|:--|--:|
| Shapes of the static registries | 76 |
| Data conditions | 17 |
| Mod-defined registries and holder sets | 9 |
| Fluids | 7 |
| Item abilities, special plantables, farmland water, minecart collision | 6 |
| World generation | 6 |
| Chunk tickets | 4 |
| Recipes | 2 |
| The remaining rows | 13 |
| Total | 140 |

## Goals / Non-Goals

**Goals:**
- Each `M5 engine extension:` row of the inventory ends `supported`, with a test, or `not supported`, with the reason.
- A NeoForge 26.3 client with the client half of a ported mod sees the mod content of each new kind. The registry sync carries the ids, and the payloads have the NeoForge 26.3 wire shape.
- A vanilla client gets a defined result for each new kind. A NeoForge client needs the client half of every mod with synced content, as on NeoForge.
- A world keeps the data of a mod that the pack no longer has, as M2 does for blocks.
- A server with no mod content runs at most one compare or one empty check at each call site that this change adds to an upstream file, and at most two at a hook call site. The performance guardrail holds.

**Non-Goals:**
- Client rendering, client-only registries and display-only data.
- Java mod jars (phase 5 of the roadmap, an optional spike with no milestone).
- Bedrock clients (`openspec/config.yaml`).

## Decisions

### Scope follows the modpack demand

This change implements a row when its demand is at least one. It also implements a row that a mod of `examples/modpack` needs. For a registry, a mod uses it when it registers an entry into it, through the registry or through a helper (for example `GameRules.register`). Every other row becomes `not supported` with the reason "no mod of the reference modpack uses it". The row returns to `planned` when a ported mod needs it.

Reason: a row with no demand costs code, tests and upstream merge work, and no mod gets a feature from it. The owner chose this rule in the decision log of #29.

Rejected alternatives:
- Full NeoForge parity: about 30 to 40 percent more work in this change, for rows that no mod of the scan uses.
- A threshold of 5 mods: it leaves out rows that 1 to 4 mods need, for example `FarmlandWaterManager`.

Task 6.1 applies the rule to the inventory before the first implementation task. Under the rule, 38 of the 76 static registries become `not supported`. The other 38 are 5 in group (a), 12 in group (b) and 21 in group (c): 16 for world generation and 5 others.

### Extensions stay in bounded modules

A mod registers each entry before the freeze. Its id is the generated count plus its position in name order. The generated lookups fall through to the frozen mod table. No generated file changes by hand. The extension code lives in new IronPumpkin modules. An upstream file gets only the call sites that the table "Files each decision touches" lists.

Rejected alternative: data-driven tables in place of the generated enums. They touch every upstream file that changes often, on each merge (`research/block-entities-menus.md` option C, `research/recipes-conditions.md` option C).

### Each registry has a policy for vanilla clients

M2 maps every mod entry to a vanilla display entry for a connection in the `ContentIds::Display` mode (`crates/pumpkin-data/src/dynamic.rs:207-216`, `crates/pumpkin-core/src/net/java/configuration_tasks.rs:647`). A connection is in the `Display` mode unless it completed the NeoForge registry sync of M3. A NeoForge client completes that sync only when it has every mod whose entries are in the rows, as on NeoForge (`ClientPayloadHandler.java:103-108`). The Pumpkin client-required check (`crates/pumpkin-core/src/net/java/neoforge.rs:862-873`) disconnects a client without NeoForge when a client-required mod exists.

Each registry spec of this change states one policy for a `Display` connection. The policies are:
- map to a vanilla display entry. A particle maps to an option-less vanilla particle, so the client never decodes foreign option bytes. Fluids and potions map to a vanilla entry.
- leave the data out. The chunk packet and the block entity data component leave out the data of a mod block entity type. The tags packet leaves out the mod ids of a tag. The statistics packet leaves out the mod custom stats. The command tree leaves out the nodes of a mod argument type, as the NeoForge `CommandTreeCleaner` does.
- refuse the action with a defined error. Opening a mod menu type fails with the error of the menu spec.

The encoders apply the policy. The code that builds a packet does not decide it. The encoders are: the particle, potion and map decoration packets, the chunk packet, the block entity data packet, the `block_entity_data` item component, the tags packet, the statistics packet, the command tree, the open screen packets and the game rule values packet.

Rejected alternatives:
- Refuse a vanilla client when a pack has mod content. M2 and M3 already serve vanilla clients with display entries, and many server-only mods need no client half.
- Keep the entries of a client-optional mod out of the rows. It does not match the M2 rows, and a NeoForge client with the client half then loses the content.

### Shapes of the static registries

A synced registry gets an entry in one runtime id table, keyed by registry id. The table holds the group (a) registries, the synced group (b) registries, `command_argument_type`, the menu type registry and `neoforge:ingredient_serializer`. It lives in `pumpkin-data` next to the M2 content tables, because `pumpkin-world` (`PoiRecord`) and `pumpkin-core` both read it. Each registry has one spec with its vanilla count and its vanilla names, and a count test against the generated lookups. The generated types (`Particle`, `PoiType`, `Potion`, `MapDecorationType`) stay. An id-based function next to each type accepts a mod id. A mod names the display entry of each entry at registration, as the M2 builders do, for a registry whose policy maps to a display entry. `PoiRecord` stores the POI type as an id of the id table, and the POI save keeps the name, so a mod POI type travels through the POI storage and queries.

A group (b) registry that NeoForge does not sync needs no numeric id. It gets a decoder registry next to its use site: a map from a name to a decode function that returns a trait object. The map rejects a duplicate name and the `minecraft` namespace, and it closes at the freeze. `LootModifierTypes` (`crates/pumpkin-core/src/world/loot/modifiers.rs:98-162`) already has this shape. A generic `DecoderRegistry<T>` in `pumpkin-util` serves the loot, trigger, predicate, condition, world generation and holder set type registries; `pumpkin-core` and `pumpkin-world` both depend on `pumpkin-util`. `LootModifierTypes` moves onto it. The registries themselves live in `pumpkin-core` next to their use sites, with the traits that mods implement. Each trait takes the full `pumpkin-core` context: the loot context of a roll, the `ItemStack`, the entity and the world. The loot use-site enums live in `pumpkin-util` (`loot_table.rs`), which cannot name these types. So a `Custom` arm of `DynamicLootCondition`, `LootFunctionKind` or `DynamicLootEntryKind` holds a type-erased value: the type name and an `Arc<dyn Any + Send + Sync>`, with a manual `PartialEq` (the same name and the same `Arc`). The `pumpkin-util` parser gets a decode hook from `pumpkin-core` for a type that it does not know. The use site downcasts the value to the trait object of its registry. `AdvancementTrigger` and the predicates live in `pumpkin-core` and hold the trait object directly.

Rejected alternatives:
- One `ContentKind` variant per registry: about 44 new variants in `pumpkin-data`, with ids that no packet uses (`research/generic-registries.md` section 2.2).
- A change to the generator for each registry: it conflicts with each upstream change of the generator.
- Server-only registries in the numeric id table: a count spec and a count test for each, for ids that never reach the wire.
- The trait of a loot `Custom` arm in `pumpkin-util`, with a context trait that `pumpkin-core` implements: `pumpkin-util` cannot name `ItemStack` or the entities, so a mod loot function cannot change a stack through it, and `Any` cannot carry the borrowed loot context.

Under the scope rule, these registries remain:
- group (a): `particle_type` (41 mods), `point_of_interest_type` (9), `potion` (5), `custom_stat` (5), `map_decoration_type` (2);
- group (b), synced: `recipe_serializer` (74), `recipe_type` (49);
- group (b), not synced: `trigger_type` (19), `loot_function_type` (16), `loot_pool_entry_type` (7), `loot_condition_type` (7), `data_component_predicate_type` (5), `enchantment_effect_component_type` (3), `height_provider_type` (2), `int_provider_type` (1), `entity_sub_predicate_type` (1), `enchantment_entity_effect_type` (1).

`point_of_interest_type` builds on the POI extension of M4 (`ExtendPoiTypesEvent`, fired at `crates/ironpumpkin-mods/src/lifecycle.rs:109`). The two enchantment registries wait for wave 6. Today a data pack enchantment loads as a raw registry entry and its effects do not run, because generated code holds the vanilla effects (`crates/pumpkin-core/src/data/datapack/dynamic_registry_loader.rs:30`). Wave 6 first makes data pack enchantment effects run, as vanilla does, and then adds the mod effect types. `height_provider_type` and `int_provider_type` decode inside placements, so their use site is the runtime JSON reader of the World generation decision.

Under the scope rule, the group (c) registries outside world generation are `command_argument_type` (21 mods), `fluid` (16), `villager_profession` (6), `memory_module_type` (2) and `sensor_type` (2). `fluid` belongs to the Fluids decision. The others get one task each, in descending order of demand. `command_argument_type` goes in wave 2, because the client parses the command tree. NeoForge syncs it for the command tree packet (`NeoForgeRegistriesSetup.java:42`), and that packet carries argument type ids, so it gets a registry sync row. The 16 world generation registries belong to the World generation decision.

### The freeze builds the registry sync rows

`SYNCED_REGISTRIES` (`crates/pumpkin-core/src/net/java/configuration_tasks.rs`) becomes a list that the freeze builds. It holds the 7 current rows. It gets one more row for each registry that matches two conditions:
- NeoForge syncs the registry;
- a server packet carries a mod id of the registry. The tags packet counts: it carries the ids of `fluid`, `point_of_interest_type`, `potion` and `game_event` (`RegistryKey::NETWORK_KEYS`). `game_event` has no demand, so it gets no row.

A row and the packet encoders use the same id function. The tags packet leaves out mod `point_of_interest_type` ids on every connection, so that registry needs no row. A row lists the vanilla names, or the NeoForge built-in names for a registry that NeoForge owns, then the mod names in name order. The order of the built-in names comes from the generated lookups for a vanilla registry, and from a NeoForge 26.3 capture for a NeoForge registry. A capture test against a NeoForge 26.3 server checks the built-in names and their order before a row ships.

Reason: the client takes the ids from the row (`RegistryManager.java:177-188`). A row whose ids differ from the ids that the encoders write breaks the client without an error.

Rejected alternative: a row for every synced registry with mod entries. A row that no packet needs adds a risk of a wrong order and no information.

### Mod block entity types

The `block_entity_type` and `menu` registries are separate rows of the inventory, outside the 76. A mod block entity type is a new content kind in `pumpkin-data` with a factory in `pumpkin-core`, as `EntityFactory` does for entity types (`research/block-entities-menus.md` option A). The registration takes:
- the type name;
- the blocks that create the type;
- the blocks for which the type is valid;
- a factory that makes a new block entity from a position and a block state;
- a loader that makes a saved block entity from a position, a block state and its NBT.

The `BlockEntity` trait of upstream does not change. A mod type that does not tick does not override `tick`. The tick loop then makes one virtual call, the same as for a vanilla type. The `u16` type index of a mod type is the generated count plus its position, so the block state field and `u16::MAX` as "none" stay.

Each site that resolves a type through the generated list calls one helper (`research/block-entities-menus.md` section 1). #192 adds the helper for the chunk packet: it matches the full name, and the generated list answers only for the `minecraft` namespace. This change extends it to the other sites and to the mod table. Today `get_id` drops the namespace, so `examplemod:chest` resolves to the vanilla chest. A test with that name checks the helper. A name that resolves to no type stays out of every packet. The NBT load path has no block state. This change adds a block state parameter to it and to its callers.

The creation set fills the block state field of mod blocks at the freeze. It uses the store of the M4 `BlockEntityTypeAddBlocksEvent` (`dynamic_types::add_block_entity_block`), and the same rule: when two types claim one mod block, the first wins with a warning. The event adds a block to the valid set of a type. For a mod block with no creation type, it also sets the creation type, as M4 does today, so an addon block can create the block entity of its base mod. The valid set only answers the validity check, as on NeoForge. A vanilla block in a valid set does not change what the vanilla block creates. The capability registration and `BlockEntityTypeAddBlocksEvent` accept mod type names.

Rejected alternatives:
- A freeze error for two claims on one block: it changes the M4 event, and NeoForge has no such conflict.
- A per-type `ticks` flag: it needs a flag read or a second tick set in the tick loop, for a cost that no bench showed.
- A change of the `BlockEntity` trait for the type id: it touches every vanilla block entity file.

### Mod menu types

A menu type is a name only. It is an entry of the id table with the shape of a group (a) entry, synced as `minecraft:menu`. `ScreenHandlerBehaviour` keeps `window_type` for the vanilla types. It gets a second field for a mod menu type id. `open_menu` of `ironpumpkin-mods` takes an extra data writer.

The open path follows `ServerPlayer.openMenu` (`ServerPlayer.java:1478-1499` of the patched vanilla sources). It sends `neoforge:advanced_open_screen` when the extra data is not empty. Otherwise it sends the vanilla open packet, also for a mod menu type. A `Display` connection gets the vanilla open packet without the extra data for a vanilla menu type. For a mod menu type it gets a new `MenuError` variant that names the menu type; today the `window_type()?` of `open_handled_screen` (`player.rs:7025`, `:7194`) turns it into `MenuError::Cancelled`. This replaces the planned requirement of the M4 menus spec for opening a mod menu type.

The data slot path carries the `i32` value from the sync handler (`sync_handler.rs:115-123`, `:202-208`) to the send (`player.rs:9394`), which today cuts it to `i16` first. A data slot value goes out as `neoforge:advanced_container_set_data` to a client that has that channel, for every value. It goes out as the vanilla 16-bit packet to the other clients. The NeoForge patch of `ServerPlayer` does the same (`ServerPlayer.java:324-329`). M4 checks that a menu keeps the slot layout of its vanilla menu type. That check applies to vanilla menu types only.

Rejected alternatives:
- A new content kind for menu types: a second mechanism for a name-only registry.
- The vanilla data slot packet when the value fits in 16 bits: it differs from NeoForge, and a client mod that listens for the payload misses values.
- A change of the type of `window_type`: about 220 lines in upstream screen handlers.

### Item abilities, special plantables and farmland water

`ItemAbility` is a named value. A mod can add new values. One function `can_perform_action(stack, ability)` replaces the identity and tag checks of the vanilla abilities at their call sites (`research/generic-registries.md` section 3.1). The function carries the vanilla answers. An item behaviour can answer for a mod item. `SpecialPlantable` (8 mods) is a trait object on the item. The villager farming code and the villager pickup check call it (`HarvestFarmland.java:123`, `Villager.java:772`), with the `neoforge:villager_farmlands` tag of M4 #183. Sheep and snow golems keep their own shears check, as NeoForge 26.3 does (`Sheep.java:149`). `FarmlandWaterManager` (2 mods) is a ticket store that `is_water_nearby` reads. `IMinecartCollisionHandler` (no mod) is `not supported` under the scope rule.

Rejected alternative: an ability check inside each item behaviour. The vanilla checks stay spread over about 12 sites, and a mod item cannot answer for a vanilla ability.

### Mod-defined registries and holder sets

`NewRegistryEvent`, `RegistryBuilder` and `DeferredRegister#makeRegistry` map to a server-only `CustomRegistry<T>` in `ironpumpkin-mods` (`research/generic-registries.md` section 4, option B). It freezes with the other mod registries, and `pumpkin-data` does not change. A registration with `sync(true)` fails at the freeze with a "not supported" error. That row returns to `planned` under the scope rule when a ported mod needs it.

Custom holder set types (`neoforge:holder_set_type`) get a decoder registry next to the tag reader, under the scope rule. NeoForge syncs that registry, and a custom holder set travels on the wire as `-1 - id` (`ByteBufCodecs.java:724`). Pumpkin writes a custom holder set as the direct list of its members instead, so the registry needs no row.

`ModifyRegistriesEvent` (2 mods) and the registry callbacks (7 mods) attach to the mod tables: an add callback runs per entry at registration, and a bake callback runs once after the freeze. A clear callback never runs, because the tables never clear. `NewDatapackRegistryEvent` follows the scope rule in task 6.1.

Rejected alternatives:
- A sync row for a registry with `sync(true)` (research option C): the scan does not show a mod that needs it.
- A generic registry in `pumpkin-data`: a change in a crate that every upstream merge touches, for registries that only mods read.

### Data pack tags

Vanilla loads the tags of every static registry from the data packs (`TagLoader`), in pack order: each file adds its entries to the tag, `replace` clears the entries of the earlier packs, an entry `{"id": .., "required": false}` is optional, and a tag that names a missing required entry does not load and is logged (`TagLoader.java:104-114`). NeoForge adds the `remove` list (`TagFile.java:7`, `TagLoader.java:68`). The bound tags answer every tag check, and the server sends them in the tags packet at the join and after a `/reload` (`PlayerList.java:881`). Pumpkin loads the tag files into `DatapackManager` (`data/datapack/tag_loader.rs`), but `Taggable::has_tag` reads the generated tags and the tags that mods register in code, and the tags packet sends the same (`update_tags.rs`). Only the item tags reach the recipes (`dynamic::set_data_pack_item_tags`, #191). So a data pack, or the data folder of a mod, that adds a block to `minecraft:mineable/pickaxe` changes nothing.

At each data pack load and reload, `pumpkin-data` builds one effective tag table for each static registry, in the vanilla order: the generated tags (the vanilla pack), the mod tags registered in code (the mod packs sit after the vanilla pack, as NeoForge places `mod_data`), then the tag files of the enabled packs with the vanilla merge rules and the NeoForge `remove` list. The table holds the tags that a data pack changed or added, and every generated tag that reaches one of them through `#` references, rebuilt in dependency order as `TagLoader.build` does; the codegen emits the direct `#` references of each generated tag for this, because the generated members are flattened. The table swaps in one atomic step. `Taggable::has_tag` keeps the generated lookup when the table has no entry for the tag, so a server whose packs change no tag pays one load of an atomic pointer per check. The tags packet sends the effective members to a `Real` connection and the vanilla ids of the same tags to a `Display` connection, by the policy for vanilla clients. `TagsUpdatedEvent.ServerDataLoad` fires after the swap. Tags of the dynamic registries (biomes, structures and the other worldgen rows) load at startup with their registry, as in vanilla.

Rejected alternatives:
- Runtime maps for every generated tag: every tag check pays a hash lookup, and every upstream call site of the generated statics changes.
- Data pack tags for items only (today): blocks, entity types, fluids and the other registries ignore the packs.

### Data conditions

One module evaluates `neoforge:conditions` (`research/recipes-conditions.md` section 2.4, option A). Each loader for which NeoForge evaluates conditions calls it once after the JSON parse. The loaders run in two passes: tags first, so `tag_empty` sees the tags of every pack. The condition types are the NeoForge 26.3 set: `always`, `never`, `registered`, `mod_loaded`, `tag_empty`, `feature_flags_enabled`, `and`, `or`, `not`. `mod_loaded` reads the native mod list (`crates/pumpkin-core/src/plugin/startup.rs:46`) and does not see Wasm plugins, because the NeoForge `ModList` holds mods only. `registered` answers from the Pumpkin lookup of the named registry; for a registry without a lookup it is false, with a warning. `feature_flags_enabled` reads the enabled feature flags of the server in every loader, because Pumpkin has one fixed flag set. A file whose condition fails does not load. The data map and loot modifier loaders call the same function. A mod condition type registers in a decoder registry.

Rejected alternatives:
- A condition check inside each loader: about 15 copies of the same logic.
- One pass: `tag_empty` then depends on the pack order.

### Recipes

The tag ingredient string fix of #191 lands before the mod recipe types. The other #191 fixes do not block them. One immutable recipe snapshot holds the data pack recipes and a per-type index of the mod recipes. The single `set_recipes` call swaps the snapshot, so a reload replaces the vanilla and the mod recipes in one step. `RecipeManager` (`crates/pumpkin-core/src/server/recipe.rs`) reads through the snapshot and keeps its public API. `DynamicRecipe` does not change. A mod recipe type and its serializer are entries of the id table, because NeoForge has a registry sync for both registries.

A serializer registers a JSON decoder and a stream encoder. The stream encoder must write the bytes that the `StreamCodec` of the client half reads. A custom ingredient is a `Custom` arm of the ingredient type with a trait object and a stream encoder. Its flattened item list is the display form for every client.

The recipes of a mod recipe type get no recipe content sync by default. At registration, a mod lists the mod recipe types to send with `neoforge:recipe_content`, as `OnDatapackSyncEvent.sendRecipes` does. A vanilla recipe type cannot be listed, because Pumpkin has no stream encoders for vanilla recipes. The freeze decides the registry sync rows of `recipe_type`, `recipe_serializer` and `neoforge:ingredient_serializer` from the listed types and the registered serializers. A reload does not change them.

Rejected alternatives:
- A separate `ModRecipeRegistry` next to `RecipeManager` (research option B): two stores with two writes, so a reload is not one step.
- An extension of `DynamicRecipe` for mod types: it changes an upstream enum and every match on it.
- Rows from the loaded recipes: a reload that adds a serializer would send an id that the client cannot map.

### Game rules

A hand-written side table holds the mod rules next to the generated vanilla rules (`research/recipes-conditions.md` section 3.3, option A). The registration returns a typed handle. The rules save under `modid:rule` keys. The server writes unknown keys back unchanged, so a world keeps the rule values of a removed mod. The `/gamerule` command and the client values packet include the mod rules. A `Display` connection gets no mod rule in the values packet. The native `GameRuleChangedEvent` gets the mod rule id next to the vanilla rule. The v0.2 host does not send a mod rule change to Wasm plugins, because the WIT `game-rule` enum is positional and no plugin needs mod rules.

Rejected alternatives:
- Mod rules in the generated enum: the Wasm hosts convert the enum by position, and 65 sites read its fields.
- A new variant of the WIT `game-rule` enum per mod rule: the enum is fixed at build time.
- A new WIT event record for mod rule changes: a permanent public interface that no plugin needs.

### Fluids

A new content kind holds the still and flowing pair, with leaked `Fluid` values and fallthrough lookups (`research/fluids.md` section 4.1, option A). The cost is one change to the codegen template of `fluid.rs`. One generic flowing fluid runs the existing `FlowingFluid` code with the parameters of the mod (`BaseFlowingFluid.Properties`). Task 6.17 first fixes the shared flow code (`research/fluids.md` section 4.3): the fluid registration takes its id from the fluid, not from a fixed list, and the replace rule stops treating every fluid as water (`flowing_trait.rs:59`, `physics.rs:14-22`). #190 fixes the flow event. The contact rules between a mod fluid and water or lava follow the NeoForge `FluidInteractionRegistry` under the scope rule.

`FluidType` is a trait object with the members that the engine reads: viscosity, the swim, drown and extinguish rules, the motion scale and the bucket item. A mod that needs another member gets it when a task adds the engine read.

The vanilla empty bucket asks the fluid of the target block for its bucket, through one helper call after its vanilla arms in `bucket.rs`. A filled mod bucket is an item behaviour. `cauldron.rs` reads the cauldron content table through one helper. The fluid handler of a cauldron block reads the same table; task 6.22 adds that handler to the M4 capabilities if they have none. The NeoForge transfer handlers of the `transfer.fluid` package (bucket and cauldron wrappers) build on this table in wave 3. The freeze rejects a declared bucket or cauldron link whose target is missing or wrong. A fluid can have neither link, as on NeoForge.

Entity interaction comes in two steps. In step one, a water-like mod fluid (`isWaterLike`) runs the water branches with the values of its `FluidType`. Step two is a per-fluid scan, as vanilla `EntityFluidInteraction` does. It starts only when a ported mod needs a fluid that is not water-like. The entity scan keeps a fast path when it sees only generated fluids. The entity tick bench measures it.

The `fluid` registry gets a sync row when a mod fluid exists, because the tags packet carries fluid ids. `neoforge:fluid_type` gets no row, because no packet carries its id.

Rejected alternatives:
- Mod arms in `bucket.rs` for each mod fluid: a change in an upstream file per mod.
- The full `FluidType` surface: members that no engine code reads.
- A per-fluid entity scan in step one: it changes upstream hot paths for a need that no ported mod has shown.

### World generation

The vanilla placement modifiers of #189 land first. Mod configured and placed features get keys in an overlay table next to the generated enums. The overlay keys a feature by its full name. The four reader sites of the biome feature lists read both (`research/worldgen-tickets.md` section 3.1, K1).

The mod world generation data loads before the worlds exist, as vanilla loads the world generation registries before the level. The one `load_all` call of the startup (`crates/pumpkin-core/src/server/mod.rs:507-511`) moves before the world loop (`:464`); `load_all` does not read the worlds. The overlay freezes after that first load. A reload skips the mod features and the biome modifiers. The template pools and processor lists of upstream reload as they do today.

The public registration contract is the NeoForge JSON of the configured and placed features, in mod data packs or as JSON values in code. Reason: ported mods ship this JSON, and the internal feature structs of `pumpkin-world` change with upstream. They stay out of the public API of `ironpumpkin-mods`.

The runtime JSON reader covers the feature types of the research minimum subset: `ore`, `scattered_ore`, `tree`, `random_patch`, `simple_block`, `random_selector` and `weighted_random_selector`, with their placements (`research/worldgen-tickets.md` section 3.2). It lives in an IronPumpkin module of `pumpkin-world` and extends the runtime parsers that exist there (`jigsaw.rs:51-81`, `processor.rs:799-960`). A differential test parses every vanilla configured and placed feature of the supported types from `assets/datapack` at run time. The test compares each result with the generated values. The feature structs derive `PartialEq` and `Debug` for it, one line per struct in the upstream feature files. A codegen arm that no longer matches the runtime reader then fails the test before it changes the terrain.

A mod type of these registries gets one enum arm that calls a trait object. The traits see the world through an object-safe facade over `GenerationCache`, not the internal structs. The registries are: `feature_type`, `placement_modifier_type`, `block_state_provider_type`, `trunk_placer_type`, `foliage_placer_type`, `tree_decorator_type`, `rule_test`, `height_provider_type` and `int_provider_type`. NeoForge biome modifiers (`add_features`, `remove_features`, `add_spawns`, `remove_spawns`) apply as an overlay per biome, frozen before the first chunk.

`add_carvers`, `carver_type`, `chunk_generator`, `biome_source`, `density_function_type`, mod biomes and mod structures come last, in one design pass. The generator computes one carver list per dimension (`uniform_carvers`), the biome id is `u8`, and the structure keys and their save keys are generated. That pass decides between two routes. Option S1 of `research/worldgen-tickets.md` section 3.4 is an engine change: an overlay of the structure keys across about 8 files. The other route is the source patch route of M4 task 2.1.2. A modpack then patches the server source itself.

The structure template palette keeps the custom properties of mod blocks (#135). A palette entry keeps string properties and resolves them through `Block::from_string_properties` (`research/worldgen-tickets.md` section 5).

Rejected alternatives:
- Native Rust feature values as the public contract (research F1): an upstream change to a feature struct breaks every mod.
- Replacing the generated enum keys in about 25 files (research K2): a conflict with every upstream change to features.
- Mod structures in the first wave: the overlay of the structure keys touches the structure hot path before any ported mod needs a mod structure.

### Chunk tickets

#190 saves the vanilla forced chunks first. Then an owner store records the mod that owns each forced chunk of a world. A ticket with the NeoForge `forceTicks` flag also makes its chunk count for natural spawning: `count_natural_spawn_chunks` (`world/mod.rs:570`) adds those chunks to the player chunks (`research/worldgen-tickets.md` section 4.4, T1). It saves in the vanilla format with the NeoForge `neo_ticket_data` key. `RegisterTicketControllersEvent` registers a controller per mod. The controller validates the saved tickets on load, as NeoForge does.

Rejected alternative: replace the forced chunk set with the owner store (research T2). It needs more edits in `world/mod.rs`, an upstream file that changes often.

### Extension interface hooks

NeoForge adds each hook as a method with a default implementation to a vanilla class. It overrides the method in some vanilla subclasses (for example `TntBlock.onCaughtFire`). Pumpkin hard-codes those vanilla answers at the call sites.

A mod registers a hook implementation per hook and per target with the builder of the target. The targets are blocks, items, entity types, block entity types, fluids and mob effects, as the rows of #184 need. The registration replaces a separate override declaration. At the freeze, each hook that at least one mod implements gets a table indexed by the target id, with an empty slot for a target without an implementation. This change adds no method to the upstream behaviour traits.

The hard-coded vanilla answer of a hook moves into a function, as `can_perform_action` does. The function first resolves the vanilla type of an M2 typed mod block (`dynamic_types::block_type`), so a mod block of type `minecraft:tnt` gets the TNT answer. The call site reads the slot. A filled slot calls the mod implementation, and an empty slot calls the vanilla function. Each hook has a test with a mod target, a typed mod block and a vanilla target. The hooks land in descending order of demand, one task per group of related hooks.

Rejected alternatives:
- Default methods on the upstream behaviour traits: each hook adds a method to an upstream trait, and each call is a virtual call for every target.
- A bitset with a separate declaration per mod: a missing declaration silently disables an override.

### The remaining rows

These rows have no decision above: `ChunkDataEvent.Load` and `.Save`, `EntityEvent.Size`, `ModifyDefaultComponentsEvent`, `DefaultDataComponentsBoundEvent`, the three `RegisterCauldronInteractionEvent` rows (item interactions with a cauldron, not the fluid content of the Fluids decision), `neoforge:auxiliary_light_data`, `neoforge:entity_data_serializers`, `NeoForgeLootContextParams`, `common.advancements.critereon` and `common.command`. Each row gets a task after the scope rule, or becomes `not supported` with the reason in the inventory.

## Files each decision touches

The table lists upstream files unless it marks a file as IronPumpkin. The research files list the exact sites.

| Decision | Files with call sites |
|:--|:--|
| Policy for vanilla clients | the encoders that the decision lists |
| Shapes of the static registries | `configuration_tasks.rs` (IronPumpkin), `world/loot/modifiers.rs` (IronPumpkin), a new module of `pumpkin-util`, the loot condition and function enums, `advancement/trigger.rs`, `advancement/predicate.rs`, `pumpkin-world` `poi/record.rs` and `poi/mod.rs` |
| `command_argument_type` | the command tree encoder of `pumpkin-core` (also the policy for vanilla clients) |
| Mod block entity types | `block/entities/mod.rs`, the chunk packet `v1_18.rs`, `data_component.rs`, `capabilities/mod.rs` (IronPumpkin), `registration.rs` (IronPumpkin), the callers of `block_entity_from_nbt` (`world/mod.rs`, `block/registry.rs`, `entity/falling.rs`, `world/block_placer.rs`, `world/generation_cache.rs`, `server/server_test_manager.rs`, the tests in `tests/vanilla_26_3_chunk.rs`) |
| Mod menu types | the screen handler behaviour of `pumpkin-inventory`, `sync_handler.rs`, `player.rs` (`open_handled_screen`, `enqueue_property_packet`), `menus/mod.rs` (IronPumpkin) |
| Item abilities, plantables, farmland water | `world/loot.rs`, `world/loot/generation.rs`, `entity/mod.rs`, `entity/player.rs`, `tripwire.rs`, `campfire.rs`, `ignition.rs`, `shears.rs`, `beehive.rs`, `pumpkin.rs`, `armadillo.rs`, `harvest_farmland.rs`, `villager/mod.rs`, `farmland.rs` |
| Mod-defined registries and holder sets | the tag reader |
| Data pack tags | `data/datapack/tag_loader.rs`, `data/datapack/mod.rs`, the `Taggable` codegen template of `tag.rs`, `dynamic.rs` (IronPumpkin), `update_tags.rs` (config and play) |
| Data conditions | the loaders under `data/datapack/` and `dynamic_registry_loader.rs` |
| Recipes | `server/recipe.rs`, `recipe_loader.rs` |
| Game rules | `data_files.rs`, the `gamerule` command, the game rule packet, `game_rule.rs` of the events, the v0.2 host conversion |
| Fluids | the codegen template of `fluid.rs`, `block/registry.rs` (the fluid registration), `flowing_trait.rs`, `physics.rs`, `world/mod.rs`, `entity/mod.rs`, `living.rs`, `player.rs`, `item.rs`, `bucket.rs`, `cauldron.rs` |
| World generation | `server/mod.rs` (the startup order of `load_all`), `feature_order.rs`, `placed_features.rs`, `natural_spawner.rs`, `jigsaw.rs`, `processor.rs`, the codegen of placed features, the derives of the feature structs in `configured_features.rs`, `placed_features.rs` and the feature config files; for the last design pass also `generator/mod.rs`, `carver/mod.rs`, `proto_chunk.rs` |
| #135 | `structure_template.rs`, reusing `state_nbt::block_state_from_compound`; the processor rule outputs (`processor.rs`) keep mod properties the same way |
| Chunk tickets | `world/mod.rs`, the level data save |
| Extension interface hooks | one call site per hook, listed by #184 |
| The remaining rows | named by the task of each row |

## Order of work

tasks.md gives the tasks that each task needs. A task starts when they are done, so tasks of different waves can run at the same time. The column below gives the main needs of each wave.

| Wave | Content | Main needs |
|:--|:--|:--|
| 1 | Scope rule applied to the inventory; id table and sync rows; decoder registries; block entity types; menu types and data slots; item abilities, plantables and farmland water; #135 | M4 closed; #192 for the block entity types |
| 2 | Data conditions; recipes; `command_argument_type`; mod-defined registries; the group (b) use sites outside world generation; game rules; chunk tickets | the id table and the decoder registries; #191 for recipes; #190 for the tickets |
| 3 | Fluids: kind, flow, buckets, cauldrons, entity interaction step one, transfer handlers | the id table and the sync rows; #190 for the flow |
| 4 | World generation: startup order, overlay, runtime JSON reader, biome modifiers, the world generation registries with a trait object | #189; data conditions for the JSON reader |
| 5 | Extension interface hooks in descending order of demand | #184 |
| 6 | The last world generation design pass; the remaining rows; the other group (c) registries; the enchantment registries; entity fluid scan step two if a ported mod needs it; the ported mod; the close | the tasks that tasks.md names |

## Migration

This change adds save data. It writes no new data into a world without mod content, so a vanilla world stays a vanilla world.
- Mod block entities save under the full type name. A world that loses the mod keeps the NBT and writes it back.
- Mod POI records and mod custom stats save under their names. A world that loses the mod keeps a POI record of an unknown type as a raw record that no query matches, and writes it back; today the POI load drops it (`poi/mod.rs:474`). The stats file keeps unknown stat keys.
- Mod game rules save under `modid:rule` keys in the game rule data. Unknown keys stay.
- Mod fluids save under the full fluid name in the scheduled fluid ticks. A world that loses the mod drops those ticks on load (`crates/pumpkin-world/src/chunk/format/mod.rs:229-231`), as vanilla drops a tick of an unknown fluid.
- Forced chunks with owners save the NeoForge `neo_ticket_data` key next to the vanilla forced chunk data.

## Wave planning

When a wave is planned, the orchestrator writes one issue per task. The issue fixes the public API of the task in `ironpumpkin-mods` (names, signatures, error variants), the files the task creates or changes, and the acceptance criteria from the specs. The design and the research are its sources. An agent reports `question` on a contract that its issue does not fix. A task of wave 6 gets a design note in its issue first, because the remaining rows have no decision here.

## Verification

- Each kind has a test mod entry in `crates/ironpumpkin-test-mod`. Its test registers, uses, saves and reloads the entry.
- Each new registry sync row has a test through the headless client `tools/pumpkin-neoforge-client`, and a capture from a NeoForge 26.3.0.64-beta dedicated server checks its names and order.
- Each new play-phase payload and the chunk packet have an in-process byte test against the stream codec of the decompiled NeoForge or vanilla source. The headless client does not decode chunk data or open menus.
- Each new kind has a test with a `Display` connection.
- Task 6.39 ports one mod of the reference modpack into `examples/modpack`. It is the mod with the fewest server classes whose kinds this change supports. The pack builds and boots from the modpack blueprint, the template crate that the unit CI builds.
- The performance guardrail runs in every wave that touches a tick, generation or lookup path. Wave 2 adds a recipe lookup bench in `pumpkin-core`, and wave 3 adds an entity tick bench. Each first records a baseline on master with the vanilla content, then runs again with the mod content of the test mod.
- The reviews follow `openspec/config.yaml`. The change closes with all levels of the CLAUDE.md section "Test levels" and `openspec archive modded-gameplay-parity`.

## Risks / Trade-offs

- [The ids of a new sync row differ from the ids that the encoders write.] -> The row and the encoders use one id function. The capture test checks the built-in names and their order.
- [Entity fluid interaction sits in upstream files that change often (`entity/mod.rs`, `living.rs`).] -> The fluid logic lives in a new file. These files get call sites only. Step two waits for a ported mod that needs it.
- [World generation has the largest upstream merge cost.] -> The overlay keeps the generated enums. The last design pass handles carvers, biomes and structures.
- [The runtime JSON reader and the codegen convert the same JSON twice and can drift.] -> The differential test compares both results for every vanilla feature of the supported types.
- [A mod removed from a pack leaves block entities, fluids, rules and tickets in the world.] -> Saves use names, and the server writes unknown data back unchanged.
- [A hook table costs memory over the whole id range.] -> A slot is one trait object reference (16 bytes on 64-bit targets). A table exists only for a hook that at least one mod implements.
- [A mod decoder parses data pack JSON, and open screen extra data comes from mod code.] -> Data packs are trusted server content, as in vanilla. Decoders return errors, not panics, and a decode error skips the file with a log line. The decoder registries reject the `minecraft` namespace.
- [The `u16` block entity type index (`u16::MAX - 1` types, because `u16::MAX` means none) and the `u8` biome id limit the number of mod entries.] -> The freeze fails with `RegistryError::RangeExceeded` when a kind exceeds its range. The last world generation design pass handles the biome id.
- [The freeze rejects a wrong fluid link or a duplicate block entity creation set, so the server does not start.] -> The error names the mod and the entry. A test checks each error.
- [A ported mod outside the reference modpack meets a `not supported` row.] -> The row returns to `planned` when a ported mod needs it, and the scope rule implements it.

## Open Questions

None blocks wave 1. Each question names the task that closes it.
- How the 65 feature files of `pumpkin-world` use `feature_name` beyond the biome check, before the overlay keys are final (task 6.25).
- Whether an unknown structure start survives a save (task 6.25).
- Whether vanilla `GameRuleMap.CODEC` accepts an unknown key, and which `LevelInfo` holds the rules of a second world (task 6.15).
- Whether a world with only a forced chunk ticks with no player (task 6.16).
- The last world generation design pass decides between option S1 and the source patch route (task 6.31).
