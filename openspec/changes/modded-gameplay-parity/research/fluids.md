# Fluids: extension points for mod fluids

Scope: custom fluids (`FluidType`, `BaseFlowingFluid`, `FluidStack`, `FluidUtil`, cauldron content) for
ported NeoForge mods. Paths are relative to the repository root. `NF:` is
`target/scratch/ref/neoforge-src/net/neoforged/neoforge`. `MC:` is `target/scratch/ref/minecraft-src`.
All statements were read in the sources named. Nothing here was run.

## 1. Summary

- Pumpkin has exactly five fluids (empty, water, flowing_water, lava, flowing_lava). They are a
  generated `const` set in `crates/pumpkin-data/src/generated/fluid.rs` from `assets/fluids.json`. All
  lookups are `match` arms on those five names, ids or block state ranges. A mod fluid has no home
  today.
- The flow code is reusable. `FlowingFluid` (`crates/pumpkin-core/src/block/fluid/flowing_trait.rs:14`) already has
  the four parameters of `BaseFlowingFluid.Properties`. Three small water assumptions in shared code
  need a fix first (section 3.4).
- The entity code is the hard part. Swimming, drowning, motion, pushing and extinguishing are written for
  two fluids with two flags (`touching_water`, `touching_lava`). NeoForge 26.3 reads all of them from
  the `FluidType` of each fluid the entity touches (section 3.5).
- The wire side needs two more synced registries: `fluid` and `neoforge:fluid_type`. Pumpkin syncs
  neither (`crates/pumpkin-core/src/net/java/configuration_tasks.rs:59`).
- Demand: 16 of 414 mods register fluids, but 155 mods reference `Fluid`, 108 `FluidStack`, 75
  `FluidType`, 41 `FluidUtil` (section 5). Most of that demand is `FluidStack` and transfer, which
  need no custom fluid. Custom fluids are a narrow, deep change.

## 2. How Pumpkin models fluids today

### 2.1 The generated fluid set

- `assets/fluids.json`: 5 entries. `flowing_water` and `flowing_lava` have 16 states each (`falling` x
  `level` 1..8). `water` and `lava` have 2 states (`falling`).
- Codegen: `tools/pumpkin-codegen/src/fluid.rs` (908 lines), run from
  `tools/pumpkin-codegen/src/main.rs:202`. It groups fluids that share a property set into structs
  (`WaterLikeFluidProperties`, `FlowingWaterLikeFluidProperties`) and emits the enums `Falling` and `Level`.
- `Fluid` (`generated/fluid.rs:34-43`): `id: u16`, `name`, `properties`, `states: &'static
  [FluidState]`, `default_state_index`, `flow_speed`, `flow_distance`, `can_convert_to_source`. The last three
  fields exist in the data, but the behaviour code does not read them: `water.rs:21` and `water.rs:62-76`
  hard-code the same numbers.
- Lookups, all closed matches: `from_registry_key` (`:620`), `from_id` (`:630`), `from_state_id` (`:641`, ranges of
  the vanilla water and lava block state ids), `ident_to_fluid_id` (`:651`), `properties` / `from_properties`
  (`:663`, `:680`, panic on an unknown name), `same_fluid_type` and `to_flowing` (`:695`, `:705`, hard-coded
  still/flowing pairs).
- Names: `ToResourceLocation` always writes `minecraft:{name}` (`:729`). `FromResourceLocation` strips
  `minecraft:` only (`:734`).
- Tags: `Taggable for Fluid` (`:758`) uses `RegistryKey::Fluid` and `id`. There are 20 generated fluid tags
  (`generated/tag.rs:20766`), for example `MINECRAFT_WATER = ids [2, 1]`.
- Reference count: 157 uses of `Fluid::WATER/LAVA/FLOWING_*` in 71 files (`rg`), 305 uses of `Fluid::` in
  all. Types that carry `&'static Fluid`: the chunk fluid tick scheduler
  (`crates/pumpkin-world/src/chunk/mod.rs:76`), `World::schedule_fluid_tick` (`crates/pumpkin-core/src/world/mod.rs:6519`),
  `FluidResource` (`crates/ironpumpkin-mods/src/transfer/mod.rs:139`).

### 2.2 Fluid state in block states

- A fluid is not stored apart from blocks. The fluid at a position is derived from the block state:
  - `World::get_fluid_from_state_id` (`world/mod.rs:6843`): `Fluid::from_state_id` (water and lava blocks), else
    water when the block is waterlogged, kelp, seagrass or a bubble column.
  - `World::fluid_state_from_block_state` (`world/mod.rs:6863`): copies the first state of `WATER` or `LAVA`, then
    for the water and lava blocks reads the `level` property 0..15 as vanilla `LiquidBlock.getFluidState` does
    (source at 0, amount `8 - level`, falling at 8 and above).
- `has_random_ticking_fluid` (`crates/pumpkin-world/src/chunk/palette.rs:27`) is true for lava only.
- A mod liquid block can have the same layout today: `BlockBuilder::block_type("minecraft:water")`
  (`crates/ironpumpkin-mods/src/content.rs:550`) gives a custom block the properties of water, and the
  generated property structs accept it (`crates/pumpkin-data/src/dynamic_types.rs:1-12`). But no lookup maps
  such a block to a fluid.

### 2.3 Flowing: spread, source creation, ticks

- Tick storage: per chunk `fluid_ticks: ChunkTickScheduler<&'static Fluid>`. Saved as a list under
  `fluid_ticks` with `i` = `to_resource_location()` (`crates/pumpkin-world/src/chunk/format/mod.rs:698-709`),
  read back with `parse_scheduled_tick::<&'static Fluid>` (`:471-477`).
- Tick dispatch: `world/mod.rs:2304-2313` takes the fluid at the position and calls
  `block_registry.get_pumpkin_fluid(fluid.id).on_scheduled_tick`. Random ticks: `:2347-2356`.
- Behaviour registry: `FxHashMap<u16, Arc<dyn FluidBehaviour>>` (`crates/pumpkin-core/src/block/registry.rs:519`). `register_fluid<T:
  FluidBehaviour + FluidMetadata>` (`:937`) takes ids from the static `T::ids()`, so one type cannot serve
  an id that is only known at run time. Water and lava register at `:484-485`.
- `FluidBehaviour` (`block/fluid/mod.rs:24`): `placed`, `on_scheduled_tick`, `on_neighbor_update`,
  `on_entity_collision`, `random_tick`, `use_with_item` and others.
- `FlowingFluid` (`flowing_trait.rs:14`) holds the shared algorithm: `get_new_liquid`, `try_flow`, `spread_to`,
  `apply_spread`, `check_infinite_source_formation`, `flow_to_sides` (uses `pathfinder.rs`). Parameters a
  fluid supplies: `get_level_decrease_per_block`, `get_flow_speed`, `get_max_flow_distance`,
  `can_convert_to_source` (`:15-17`, `:41`). `FlowingWater` (`water.rs:21-76`) sets 1, 5, 4 and the game rule.
  `FlowingLava` (`lava.rs`, 297 lines) adds fire spread and water-to-stone logic.
- Source creation: `can_convert_to_source` plus the plugin event `CreateFluidSourceEvent`
  (`crates/pumpkin-core/src/plugin/level_events.rs:480-508`).
- Events fired by flow: `BlockFromToEvent` (`flowing_trait.rs:355`).

### 2.4 Buckets and cauldrons

- Buckets are matched by item id, not by fluid: `FilledBucketItem::ids()` lists water, lava, powder snow and the mob
  buckets (`crates/pumpkin-core/src/item/items/bucket.rs:41-56`). `try_pickup_fluid_at` (`:277`) and
  `try_place_liquid` (`:370`) test `Item::LAVA_BUCKET` and `Block::WATER/LAVA` directly. There is no
  `Fluid -> bucket item` link and no `bucket item -> Fluid` link.
- Cauldrons: one behaviour for `CAULDRON`, `WATER_CAULDRON`, `LAVA_CAULDRON`, `POWDER_SNOW_CAULDRON`
  (`crates/pumpkin-core/src/block/blocks/cauldron.rs:39-49`). Fill and empty are `match` arms on those block ids and
  on bucket items (`:248-330`). Dripstone: `enum CauldronDripFluid { Water, Lava }`
  (`crates/pumpkin-core/src/block/blocks/dripstone.rs:44`, `get_fluid_above_stalactite` `:460`).
- There is no fluid capability provider for cauldrons or tanks: `register_vanilla_providers`
  (`crates/ironpumpkin-mods/src/capabilities/vanilla.rs:~90-110`) registers item containers only.

### 2.5 Entity interaction

- `Entity::update_fluid_interaction` (`crates/pumpkin-core/src/entity/mod.rs:3087`) copies the vanilla
  `EntityFluidInteraction.update` scan, but keeps two slots: `fluid_push`, `fluid_n`, `in_fluid`,
  `fluid_height` are `[_; 2]`, and `i = usize::from(fluid is lava)` (`:3131`). Push speeds: `WATER_PUSH_SPEED`
  (`:138`) and `lava_push_speed` (`:148`).
- Flags: `touching_water` and `touching_lava` (`:1623`, set at `:3202`). `is_in_water` `:4115`.
  `is_submerged_in_water` `:4138` matches `Fluid::WATER` only.
- Swimming: `Player::update_swimming` (`crates/pumpkin-core/src/entity/player.rs:2637`, water check `:2653`).
- Drowning: `LivingEntity::tick_air_supply` (`crates/pumpkin-core/src/entity/living.rs:3729`) tests
  `is_submerged_in_water`.
- Motion: `travel_in_fluid`, `travel_in_water`, `apply_fluid_moving_speed` (`living.rs:2389`, `:2330`, `:2452`)
  use the water or lava branch. Items: `crates/pumpkin-core/src/entity/item.rs:475`.
- Tags: `World::fluid_state_has_tag` (`world/mod.rs:6924`) collapses water and lava, then uses the tag.

### 2.6 Content registry and sync today

- `ContentKind` has Block, Item, EntityType, SoundEvent, MobEffect, Attribute, DataComponentType
  (`crates/pumpkin-data/src/dynamic.rs:171-181`). Only Block, Item and EntityType have tags (`:183`). No fluid.
- Dynamic entries are leaked `&'static` data allocated at `freeze` (`dynamic.rs:1205`, `:1612`). Ids follow
  the generated counts. Generated lookups fall through to the dynamic tables for ids above the counts.
- `SYNCED_REGISTRIES` (`configuration_tasks.rs:59`): block, item, entity_type, mob_effect, attribute,
  data_component_type, `neoforge:synced_attachment_types`. No `fluid`, no `neoforge:fluid_type`.
- The data map registry list knows `fluid` as a vanilla-only static registry
  (`crates/ironpumpkin-mods/src/data_maps/registries.rs:229`).
- Config-phase tags use `dynamic::network_tag_ids` (`crates/pumpkin-protocol/src/java/client/config/update_tags.rs:58`),
  so custom members already flow there for the kinds that have tags.

### 2.7 Transfer API (#153)

- `FluidResource(&'static Fluid)` has no data components (`transfer/mod.rs:137-177`), `BUCKET_VOLUME = 1000`
  (`:28`), `FluidStacksResourceHandler` (`:405`). Amounts are `u32`. The fluid capability keys exist in
  `capabilities` (`fluid::BLOCK`, `ENTITY`, `ITEM`).
- Missing against `NF:transfer/fluid/`: `FluidUtil` (`FluidUtil.java:102-337`), `BucketResourceHandler`,
  `CauldronWrapper`, `DispenseFluidContainer`, `ItemAccessFluidHandler`, and the `FluidStack` type
  (`NF:fluids/FluidStack.java`, a record-like holder with data components, `:49-60`).

## 3. What NeoForge 26.3 gives a mod

### 3.1 `FluidType` (`NF:fluids/FluidType.java`) and the `neoforge:fluid_type` registry

- Registry: `NeoForgeRegistries.FLUID_TYPES`, `sync(true)` (`NF:registries/NeoForgeRegistries.java:38`,
  registered `NeoForgeRegistriesSetup.java:71`). Built-in entries `minecraft:empty`, `water`, `lava`
  (`NF:common/NeoForgeMod.java:408-470`). `milk` registers only when a mod enables it (`:649`).
- `Properties` (`FluidType.java:860-1158`), defaults in `:860-885`: `descriptionId`, `motionScale` (0.014),
  `canPushEntity` (true), `canSwim` (true), `canDrown` (true), `fallDistanceModifier` (0.5), `canExtinguish`
  (false), `canConvertToSource` (false), `supportsBoating` (false), `pathType` (WATER), `adjacentPathType`
  (WATER_BORDER), `sound(action, event)`, `canHydrate` (false), `lightLevel` (0), `density` (1000),
  `temperature` (300), `viscosity` (1000), `rarity`, `addDripstoneDripping(...)`, `isWaterLike` (false).
- Overridable hooks (class is open): entity-based (`motionScale(Entity)`, `canPushEntity`, `canSwim`,
  `getFallDistanceModifier`, `canExtinguish`, `canDrownIn`, `canHydrate`, `setItemMovement`,
  `supportsBoating`, `canRideVehicleUnder`, `:249-386`), `move(LivingEntity, Vec3, gravity)` (`:306`, custom
  movement, default none, water-like uses water movement), state-based (`canConvertToSource(state, reader,
  pos)` `:412`, path types `:429`, `:447`, `getSound`, `getLightLevel`, `getDensity`, `getTemperature`,
  `getViscosity`, `:462-546`), `handleCauldronDrip` (`:565`), stack-based (`:596-711`), placement
  (`getBucket`, `getBlockForFluidState`, `getStateForPlacement`, `isVaporizedOnPlacement`, `onVaporize`,
  `:741-840`).
- `SoundAction` is an open string-keyed class. Built-ins: `BUCKET_FILL`, `BUCKET_EMPTY`, `FLUID_VAPORIZE`,
  `CAULDRON_DRIP` (`NF:common/SoundActions.java:19-34`).
- Where vanilla (patched) calls the hooks, all through `Fluid.getFluidType()`:
  - `EntityFluidInteraction.update` filters pushing by `FluidType` and tracks height per fluid
    (`MC:world/entity/EntityFluidInteraction.java:42-114`).
  - `Entity.java:1661`, `:1680` (swim), `:1689` (update), `:2835` (crawl); `LivingEntity.java:2598` (calls
    `moveInFluid` for each non-vanilla type), `:3191`; `ItemEntity.java:130` (float in a custom fluid);
    `Player.java:290`, `:317` (drown, swim); `AbstractArrow.java:187`; `Fluid.java:70` (extinguish).
  - `BucketItem.java:130,171`; `CauldronBlock.java:54`; `PointedDripstoneBlock.java:95`.
- `FluidInteractionRegistry` (`NF:fluids/FluidInteractionRegistry.java:34-171`): per source type, a predicate and a
  result block state (lava + water -> stone style). Used by `LiquidBlock` neighbour checks.

### 3.2 `BaseFlowingFluid` (`NF:fluids/BaseFlowingFluid.java`)

- A source and a flowing class sharing `Properties(fluidType, still, flowing)` plus `bucket`, `block`,
  `slopeFindDistance` (4), `levelDecreasePerBlock` (1), `explosionResistance` (1), `tickRate` (5)
  (`:30-222`). Suppliers are lazy, so the four objects can refer to each other.
- Behaviour: `canConvertToSource` comes from the `FluidType` (`:~90`). `canBeReplacedWith` is "from above
  only, and not the same fluid" (the water rule). `createLegacyBlock` returns the `LiquidBlock` of the
  properties with the `LEVEL` property (`:~140`). `isSame` is true for still or flowing of the pair.
  Pickup sound comes from `SoundActions.BUCKET_FILL`.
- A mod fluid that needs lava rules (fire, water interaction) extends `FlowingFluid` itself. Only the
  `BaseFlowingFluid` users (30 mods) are served by parameters alone.

### 3.3 `FluidStack`, `FluidUtil`, handlers

- `FluidStack` holds a `Holder<Fluid>`, `amount` and data components (`NF:fluids/FluidStack.java:49-60`),
  `FluidInstance` has the codecs (`FluidInstance.java:20-49`). The registry stream codec needs the `fluid`
  registry ids on the client.
- `FluidUtil` moved to `neoforge.transfer.fluid` (`NF:transfer/fluid/FluidUtil.java`): `interactWithFluidHandler`
  (`:102`, `:122`), `tryPickupFluid` (`:196`, `:224`), `tryPlaceFluid` (`:292`, `:333`), sound and game event
  helpers (`:134-190`). `tryPlaceFluid` and `tryPickupFluid` call `FluidType` hooks and `BucketPickup`.
- Bucket transfer: `BucketResourceHandler` (`NF:transfer/fluid/BucketResourceHandler.java`) reads the fluid of a
  `BucketItem.content` and returns `FluidType.getBucket(stack)`. Items, tanks, `DispenseFluidContainer`
  and `ItemAccessFluidHandler` sit on the same handler API as #153.

### 3.4 Cauldron content

- `RegisterCauldronFluidContentEvent` (a mod-bus event, `NF:fluids/RegisterCauldronFluidContentEvent.java:~28`):
  `register(Block, Fluid, totalAmount, @Nullable IntegerProperty levelProperty)`. One call links a cauldron
  block to a fluid and a volume. Rules in `CauldronFluidContent.register`: no duplicate block or fluid,
  amount > 0, level property must start at 1 (`NF:fluids/CauldronFluidContent.java:~100-150`).
- `CauldronFluidContent.registerCapabilities` registers `Capabilities.Fluid.BLOCK` for every registered cauldron
  block through `CauldronWrapper` (`:~160-174`, `NF:transfer/fluid/CauldronWrapper.java`). This is what
  lets pipes fill a cauldron. Vanilla entries: empty, water (level 1..3), lava (`:~70-90`).
- `FluidType.addDripstoneDripping` ties a fluid to a stalactite drip and a filled cauldron block
  (`FluidType.java:1114`, `:565`).
- Only 1 mod subscribes to this event (section 5).

### 3.5 Client sync

- `VANILLA_SYNC_REGISTRIES` includes `BuiltInRegistries.FLUID` with the comment "Required for Fluid/FluidStack
  packets" (`NF:registries/NeoForgeRegistriesSetup.java:38`). `neoforge:fluid_type` is its own synced
  registry (`NeoForgeRegistries.java:38`). Both travel in `neoforge:frozen_registry` payloads.
- The client builds the `FluidState` of a chunk block from the `LiquidBlock` class of the mod, so the client
  needs the same mod. A server-only display fallback exists only through the display state of the block.

## 4. What IronPumpkin needs, with options

Rules from `common.txt`: options and costs only. A recommendation is given per point. No decision is taken.

### 4.1 Fluid registry entries and the `Fluid` type

Need: a fluid pair (still, flowing) per mod fluid, a `FluidType` per pair, ids allocated at the freeze, tag
membership, names in saves and in the registry sync.

- Option A: add `ContentKind::Fluid` to the dynamic content registry. A `FluidDefinition` (name, `still`
  and `flowing` pair, `FluidType` name, flow parameters, block name, bucket name) creates two leaked `Fluid`
  values with the layout of `flowing_water` (16 states) and `water` (2 states). Generated lookups
  fall through for ids above `5`.
  - Files: `crates/pumpkin-data/src/dynamic.rs` (+ a `dynamic_fluids.rs`), `tools/pumpkin-codegen/src/fluid.rs`
    (template for `from_id`, `from_registry_key`, `from_state_id`, `properties`, `from_properties`,
    `same_fluid_type`, `to_flowing`, `ToResourceLocation`, `FromResourceLocation`), `ironpumpkin-mods`
    (`FluidBuilder`, `ModInit::register_fluid`), `crates/pumpkin-core/src/block/registry.rs`.
  - Merge cost: the generated `fluid.rs` is `keep-ours` and regenerated, so the cost is the codegen template
    only (908 lines, upstream edits it rarely). `dynamic.rs` is already an IronPumpkin file. Medium.
  - Performance: `Fluid::from_state_id` runs on every fluid query (entity scan, flow, `contains_any_liquid`).
    A branch "state id above the generated count" costs one compare, as for blocks.
  - Wire and save: ids are u16 and dense. Saves must use names (fluid ticks, section 4.6).
- Option B: keep the five `Fluid` values and give mod fluids a second type (`ModFluid`) beside them. Needs a
  second variant everywhere that holds `&'static Fluid`: chunk ticks, `schedule_fluid_tick`, `FluidResource`,
  71 files of water/lava logic. More files touched than A, and no generated file saved.
- Option C: map every mod fluid onto water or lava ids with a side table (a "water with a type" overlay).
  No new ids and no sync. It cannot make two mod fluids differ in tags, buckets or `FluidType`, and it breaks
  `FluidStack` equality. Not enough for 16 registering mods.
- Recommendation: A. It follows the pattern of the M2 content registry (leak at freeze, fall-through lookups)
  and keeps `&'static Fluid` signatures. Prerequisite check before design: that every `match` on the five
  fluids in non-generated code (157 uses in 71 files) is either a vanilla-only rule (stay) or reads a
  property that `FluidType` supplies (move).

`FluidType` itself: a Rust trait object in `ironpumpkin-mods` (`FluidType` with `Properties` builder and
the NeoForge default values of section 3.1). Hooks that take `Entity`, `FluidState` or `LevelReader` need the
mod API types for those. Cost: one module; the hooks only matter once section 4.5 reads them.

### 4.2 Fluid states of the fluid block

- The block of a mod fluid is a custom block with `block_type("minecraft:water")` (layout: `level` 0..15).
  Needed: a table "block id -> fluid pair" in the dynamic registry, and
  `get_fluid_from_state_id` / `fluid_state_from_block_state` (`world/mod.rs:6843`, `:6863`) must use it for
  ids above the generated count. Today they test `BlockId::WATER | LAVA` and copy `Fluid::WATER`/`LAVA` states.
- The `block` lookups and `FluidState` creation stay inside `World`, so the change is local to two private
  functions, plus `Fluid::from_state_id`. Upstream hot path: `world/mod.rs` is a large file upstream edits
  often. Keep the new code in a small function in a new module and make the two functions call it.
- Display state for NeoForge clients: not needed (the client needs the mod). For vanilla clients: the
  `display` of the block is a water state, as for any custom block (`BlockDefinition.display`,
  `dynamic.rs:53`). The project target is NeoForge clients, so this is the fallback only.
- Waterlogging stays a water-only rule. NeoForge 26.3 lets other fluids log blocks only through
  `canFluidLog` checks in path types. Not part of this extension point.

### 4.3 Flow behaviour: can a mod fluid reuse the water or lava code?

Yes for the algorithm, with three fixes. Evidence: `FlowingFluid` is a trait with default methods and four
required parameters (`flowing_trait.rs:14-41`), the same four as `BaseFlowingFluid.Properties`
(`slopeFindDistance` = `get_max_flow_distance`, `levelDecreasePerBlock`, `tickRate` = `get_flow_speed`,
`canConvertToSource`).

- Option A: one generic `ModFlowingFluid` struct that implements `FluidBehaviour` and `FlowingFluid`
  from a parameter record (the four values plus the `FluidType` source rule). Registered per id.
  - Needed in shared code:
    1. `register_fluid` takes ids from a static `T::ids()` (`registry.rs:937`). Add a way to register an
       `Arc<dyn FluidBehaviour>` for one id (a ~5 line addition next to it).
    2. `apply_spread` fires `BlockFromToEvent` with `Block::WATER` for every fluid (`flowing_trait.rs:355-358`).
       This is wrong for lava today too. It must use the block of the fluid.
    3. `get_effective_props` and `physics::can_be_replaced` have a water special case and a rule "an
       other fluid is always replaced" (`flowing_trait.rs:59`, `physics.rs:14-22`). `BaseFlowingFluid`
       replaces only from above and never the same fluid (section 3.2). A mod fluid needs
       `FluidInteractionRegistry` style rules for contact with water and lava, as lava has in `lava.rs`.
  - Files: `crates/pumpkin-core/src/block/fluid/` (new `mod_fluid.rs`, edits to `flowing_trait.rs`,
    `physics.rs`), `registry.rs`, `ironpumpkin-mods`.
  - Merge cost: low to medium. Fixes 2 and 3 edit upstream files (small hunks) and are candidates for upstream
    PRs (fix 2 is an upstream bug). New behaviour lives in a new file.
  - Performance: none if the tick path stays `get_pumpkin_fluid(id)`. The new struct adds one `Arc` per fluid.
  - Save risk: none beyond section 4.6.
- Option B: a mod writes its own `FlowingFluid` implementation for every fluid. Same shared-code fixes, plus
  more code per mod and the 30 `BaseFlowingFluid` users all re-implement the same parameters. Only for
  fluids that need lava-like rules.
- Recommendation: A for the `BaseFlowingFluid` shape, with B open through the public `FlowingFluid` and
  `FluidBehaviour` traits for custom rules. Fixes 1 and 2 are small and independent of the rest, and
  fix 2 is a bug even today.
- `FluidInteractionRegistry` (9 mods): lava has its own code in `lava.rs`. A table-driven version needs
  one hook in the neighbour update of the liquid block. It is separate work; do it with fix 3 or later.

### 4.4 Buckets and the bucket item

- Need: a `Fluid <-> bucket item` link, so `try_pickup_fluid_at` and `try_place_liquid` serve any fluid, and so
  `BucketResourceHandler` can read `BucketItem.content`.
- Option A: add the link as data in the fluid definition (`bucket` item name) and in the bucket item (`content`
  fluid), checked at the freeze. Make `bucket.rs` ask the registry for the fluid of the held item and the
  item of the fluid. Edits upstream file `item/items/bucket.rs` (734 lines): the `match` arms on
  `Item::LAVA_BUCKET` etc. at `:41-56`, `:89`, `:277`, `:370`. Medium merge cost, contained.
- Option B: a mod bucket is an `ItemBuilder` with `.item_type("minecraft:bucket")`
  (`content.rs:562`) plus its own `ItemBehaviour` (`content.rs:317`) that calls `try_place_liquid`-like
  helpers through the transfer API. No edit of `bucket.rs`, but pickup of a mod fluid by an empty vanilla
  bucket does not work (the vanilla bucket does not know the fluid), and the code is copied per mod.
- Recommendation: A for the lookup in `bucket.rs` (pickup with an empty bucket is common to every fluid
  mod), with `FluidType.getBucket` and `SoundActions.BUCKET_*` read from the type. Use B only for exotic
  buckets.
- `LiquidBlock` also needs the pickup contract (`IBucketPickupExtension`, 7 mods) and `BucketPickup`
  sound: part of the same lookup.

### 4.5 Entity interaction: swimming, drowning, motion

This is the largest cost and the one place where option choice changes performance.

- Today: two slots (water, lava), two flags, water-only drowning and swim checks (section 2.5). A mod fluid
  needs: height and push per fluid, `canSwim`, `canDrown`, `canExtinguish`, `motionScale`,
  `fallDistanceModifier`, `canPushEntity`, `move` hook, and item floating.
- Option A: generalise `update_fluid_interaction` to a small per-entity list keyed by fluid id (vanilla
  `EntityFluidInteraction` does the same, `MC:EntityFluidInteraction.java:27-114`), and keep `touching_water`
  and `touching_lava` as derived flags for the many existing readers. Add `FluidType` lookups in
  `is_submerged_in_water`, `update_swimming`, `tick_air_supply`, `travel_in_fluid`, item drag.
  - Files: `entity/mod.rs`, `entity/living.rs`, `entity/player.rs`, `entity/item.rs`, plus the entity AI
    code with water checks (the `rg` list in 2.1 shows `squid`, `dolphin`, `turtle`, `axolotl` and others, which
    can stay water-only because vanilla keeps them water-only).
  - Merge cost: high. `entity/mod.rs` and `living.rs` are upstream hot files and are edited on most merges.
    Keep the edit to the hook points, with the fluid-type logic in a new file.
  - Performance: this scan runs for every entity every tick. Today it allocates a `BTreeMap` and a `Vec` per
    call (`entity/mod.rs:3087-3140`). A mod fluid must not add cost when none exists: take the fast path
    when the scan sees only fluids with id below the generated count. Run the entity tick benchmark before and
    after (`config` rule: no evident impact).
  - Wire: none. Save: none.
- Option B: add only the flags a mod fluid most often uses (swim, drown, viscosity-like slowdown) and leave the
  hooks `move`, `canRideVehicleUnder`, `supportsBoating` out. Smaller change, partial parity. The mapping
  table counts 75 mods on `FluidType`, but most of these only read `FluidType.getDescriptionId` or compare types
  (not measured per member; the table has "Members n/u 37/8"). Measure member use first.
- Option C: treat a mod fluid as "water-like" when `isWaterLike` is true (use the water branches with the
  type's values) and as "no interaction" otherwise. Cheapest. `isWaterLike` exists in 26.3 for exactly this
  reason (`FluidType.java:584`, `move` doc `:306`).
- Recommendation: C first (water-like branches read `FluidType` values: viscosity, swim, drown, extinguish,
  motion scale), then A for the per-fluid scan if the member census shows demand for `move` or for several
  mod fluids at once. State this order in the design; the census is a one-off `rg` over `modpack-usage.md`
  section 9 and the member lists, which this research did not run.

### 4.6 Save and wire formats

- Fluid ticks: written as `minecraft:{name}` and read by stripping `minecraft:` (`generated/fluid.rs:729-740`,
  `chunk/format/mod.rs:471-477`, `:698-709`). A mod fluid needs its full name in `i`. A chunk with a tick of
  an unregistered fluid must not fail the chunk: check `parse_scheduled_tick` behaviour (not read here).
  Block states of the liquid block already save by name through the M2 palette.
- Registry sync: add `fluid` (all fluids, vanilla first, then mod fluids in name order) and
  `neoforge:fluid_type` (vanilla `empty`, `water`, `lava`, then mod types) to `SYNCED_REGISTRIES`
  (`configuration_tasks.rs:59`, needs `ContentTables::fluid_snapshot` and a type snapshot beside
  `block_snapshot` at `dynamic.rs:478`). The array length 7 becomes 9. This changes the handshake for every
  NeoForge client, so a vanilla-only server must still send identical bytes as before for a modpack with no
  mod fluid: decide whether `fluid` is always synced (as NeoForge does) or only with mod fluids. Option
  "always": matches NeoForge, adds 2 payloads for all clients. Option "only with mod fluids": smaller, but a
  client that expects `fluid` in the list sees a difference.
- Tags: add `ContentKind::Fluid` to `tag_key` (`dynamic.rs:183`) so mod fluids join `minecraft:water`, `c:*`
  tags and mod tags. The config-phase packet already merges custom members (`config/update_tags.rs:58`).
- `FluidStack` on the wire: it carries the `fluid` registry id. It is needed only by mod packets and menus
  that ship fluid slots (menus task). The sync above covers it.

### 4.7 Transfer API (#153) and `FluidUtil`

- `FluidResource(&'static Fluid)` already works with leaked dynamic fluids, so option 4.1-A needs no change in
  the handler types (`transfer/mod.rs:137`).
- Gaps against `NF:transfer/fluid/`: `FluidStack` (with components: Pumpkin `FluidResource` drops them by
  design, `transfer/mod.rs:137`), `FluidUtil`, `BucketResourceHandler`, `CauldronWrapper`,
  `DispenseFluidContainer`, `ItemAccessFluidHandler`. Each is a Rust module in `ironpumpkin-mods/src/transfer/`.
  They depend on 4.4 (bucket link) and 4.8 (cauldron).
- `FluidUtil.tryPlaceFluid` reads `FluidType.canBePlacedInLevel`, `isVaporizedOnPlacement`, and the sound
  actions (sections 3.1, 3.3). So `FluidType` must exist before `FluidUtil`.
- Vanilla fluid providers: none exist for tanks or cauldrons (section 2.4). Pipes mods read
  `Capabilities.Fluid.BLOCK` on cauldrons, so item 4.8 also gives the first vanilla fluid provider.

### 4.8 Cauldron content

- Need: `RegisterCauldronFluidContentEvent` (1 mod), a `CauldronFluidContent` table, and the capability provider.
- Option A: add the table and the event, and make `cauldron.rs` read the table instead of its `match` on four
  block ids (`:39-49`, `:248-330`). Edit of an upstream file (864 lines). Adds `CauldronWrapper` as the
  first fluid provider.
- Option B: keep `cauldron.rs` as is. Add the table and the event and the `CauldronWrapper` provider only
  (capability side, no change of cauldron interaction). Mod cauldrons (a custom block with its own
  `BlockBehaviour`) then work through the capability but vanilla buckets do not fill them.
- Recommendation: B first, because 1 of 414 mods needs the event and the capability side is where pipes
  mods act. Choose A when a mod fluid with a cauldron must work with a hand bucket.
- Dripstone: `CauldronDripFluid` is a two-value enum (`dripstone.rs:44`). A mod `addDripstoneDripping` needs
  it to carry a fluid and a cauldron block. Low demand, and it edits upstream `dripstone.rs`: park it.

## 5. Demand

Source: `openspec/changes/neoforge-shaped-plugin-api/modpack-usage.md` (414 mods; "Mods (srv)" counts
mods with a server or mixed class that references the class) and `mapping-table.md` section "Modpack coverage".

| Item | Mods (srv) | Refs | Source line |
|:--|--:|--:|:--|
| Package `neoforge.fluids` (39 classes) | - | 4170 | `modpack-usage.md:82` |
| `net.minecraft...material.Fluid` | 155 | 1307 | `modpack-usage.md:1074` |
| `...material.FluidState` | 141 | 953 | `:1084` |
| `...material.Fluids` | 118 | 502 | `:1113` |
| `...material.FlowingFluid` | 103 | 476 | `:1127` |
| `neoforge.fluids.FluidStack` | 108 | 1440 | `:707` |
| `FluidType` | 75 | 398 | `:730` |
| `FluidUtil` (moved in 26.3) | 41 | 101 | `:769` |
| `FluidType$Properties` | 32 | 80 | `:789` |
| `BaseFlowingFluid` | 30 | 101 | `:795` |
| `BaseFlowingFluid$Properties` | 26 | 76 | `:814` |
| `BaseFlowingFluid$Flowing` / `$Source` | 23 / 21 | 34 / 25 | `:830`, `:839` |
| `SoundAction` / `SoundActions` | 23 / 20 | 44 / 41 | `:828`, `:841` |
| `FluidIngredient` / `SizedFluidIngredient` | 23 / 18 | 96 / 160 | `:827`, `:852` |
| `FluidInteractionRegistry` | 9 | 12 | `:934` |
| `RegisterCauldronFluidContentEvent` | 1 | 1 | `:1470` |
| `IFluidExtension` (mapping `planned`) | 49 | - | `mapping-table.md:830` |
| `IFluidStateExtension` | 47 | - | `mapping-table.md:832` |
| `IBucketPickupExtension` | 7 | - | `mapping-table.md` (extension table) |
| `world.item.BucketItem` | 38 | - | `modpack-usage.md:4333` |
| `world.level.block.LiquidBlock` | 49 | - | `:4392` |
| Registry `fluid` (mods registering) | 16 | 63 calls | `:1502` |
| Registry `neoforge:fluid_type` | 16 | 56 calls | `:1503` |
| Mixins on `FlowingFluid` / `BucketItem` / `LiquidBlock` | 5 / 3 / 2 | - | `:2427`, `:2518`, `:2541` |

GONE or moved in 26.3 (the old handler API, so these need the #153 transfer API, not a port of the old
classes): `IFluidHandler` 89, `FluidAction` 77, `IFluidHandlerItem` 45, `FluidTank` 37, `FluidActionResult` 16,
`IFluidTank` 13 (`modpack-usage.md:714`, `:725`, `:760`, `:780`, `:871`, `:883`). `FluidUtil` moved to
`neoforge.transfer.fluid` (`:769`, `:2956`).

Mapping-table status of the fluid rows today (all `planned` or `not supported`): `neoforge:fluid_type` and `fluid`
registries `planned` (`mapping-table.md:357`, `:359`), `FluidType`, `BaseFlowingFluid` and the `Flowing` /
`Source` pair "no phase yet" (`:1129-1134`), `RegisterCauldronFluidContentEvent` `planned` (`:257`),
`transfer.fluid` `planned` (`:886`), `fluids` and `fluids.crafting` "not supported" and `planned` (`:887`,
`:898`). `NeoForgeMod` item 28 says the milk fluid follows custom fluids (`:1006`).

Reading of the demand:
- Pure `FluidStack` and handler use (108, 89 mods) is served by the transfer API without a custom fluid. Its
  gaps are `FluidStack` itself and the missing `neoforge.transfer.fluid` classes (section 4.7).
- 16 mods add a fluid. They need sections 4.1 to 4.5. Of those, 30 mods use `BaseFlowingFluid` classes, and
  only a few override lava-like rules (5 mixin mods on `FlowingFluid`: the mixin rows ride the source patch
  channel, not this API).
- `fluids.crafting` (28 classes, 23 mods on `FluidIngredient`) is the recipe side. It belongs to the recipe
  extension point, not to this file.

## 6. Order and cost summary

| Step | Depends on | Touches (new / upstream-edited) | Merge cost | Perf risk |
|:--|:--|:--|:--|:--|
| 1 `ContentKind::Fluid`, leaked `Fluid` values, codegen fall-through | M2 registry | new `dynamic_fluids.rs`; codegen template; `dynamic.rs` | medium (template) | none |
| 2 block -> fluid table, `world/mod.rs` lookups | 1 | two private fns in `world/mod.rs` | low | one compare per query |
| 3 `ModFlowingFluid` + three flow fixes | 1, 2 | new file; `flowing_trait.rs`, `physics.rs`, `registry.rs` | low to medium | none |
| 4 bucket link, `BucketResourceHandler`, `FluidUtil` | 1, 3 | `bucket.rs` match arms; new transfer modules | medium | none |
| 5 `FluidType` hooks in entity code (water-like first) | 1, 2 | `entity/mod.rs`, `living.rs`, `player.rs`, `item.rs` | high | entity tick scan: benchmark |
| 6 registry sync (`fluid`, `neoforge:fluid_type`) | 1 | `configuration_tasks.rs`, `dynamic.rs` snapshots | low | none; handshake change |
| 7 fluid tags, fluid tick save names | 1 | `dynamic.rs` `tag_key`; `fluid.rs` codegen template; chunk format | low | none |
| 8 cauldron table, event, `CauldronWrapper` | 4 | new modules (B); `cauldron.rs` (A) | low (B) / medium (A) | none |

Steps 1, 2, 3, 6, 7 give a mod fluid that flows, ticks, saves, tags and syncs. Step 5 gives swimming and drowning.
Steps 4 and 8 give buckets and cauldrons.

## 7. Open points for the owner (not decided here)

1. Always sync `fluid` and `neoforge:fluid_type` (NeoForge behaviour) or only when a mod fluid exists
   (section 4.6)?
2. Entity interaction scope: water-like branches only, or the per-fluid scan with the `move` hook
   (section 4.5)? The per-member census of `FluidType` use was not run.
3. Pickup with a vanilla empty bucket for mod fluids: edit `bucket.rs` (A) or per-mod behaviour (B)?
4. Cauldron: capability side only, or also the hand-bucket interaction (section 4.8)?
5. `ModFlowingFluid` fix 2 (`BlockFromToEvent` with `Block::WATER` for all fluids) is an upstream bug:
   report it as its own commit and upstream PR draft, as the project rules say.
6. Not read: `parse_scheduled_tick` handling of an unknown fluid name, the `Aquifer`/world generation use
   of fluids (`levelgen` fluid pickers are generated data), and the Bedrock paths (out of scope by the config).
