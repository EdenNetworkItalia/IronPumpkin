# Design

The inventory tables are hand-written. A task edits the rows of its issue, recounts the counts in the `####` headings, the family rows and the Total row, and runs `python3 -I tools/openspec-sync/mapping_status.py` (`tools/openspec-sync/README.md`). A generator wrote the first version of the tables; it is not in the repository. The one exception is the family "Extension interfaces": `tools/openspec-sync/extension_table.py` writes its tables and its rows in `mapping-table.md` from the NeoForge sources, `tools/openspec-sync/extension_rows.tsv` (the hand-written statuses) and `tools/modpack-scan/extension_demand.tsv` (the demand per method). The mapping tables of the generator follow the decision "Engine extensions are the next phase": a row whose reason is a missing engine extension point is `planned` with the reason prefix `M5 engine extension:`. The arguments are: the NeoForge clone (`git clone --filter=blob:none --sparse -b 26.3.x https://github.com/neoforged/NeoForge`, then `git sparse-checkout add src/main/java src/client/java src/generated src/main/resources patches`), the unpacked FancyModLoader sources (`https://maven.neoforged.net/releases/net/neoforged/fancymodloader/loader/12.0.8/loader-12.0.8-sources.jar`), the unpacked eventbus sources (`https://maven.neoforged.net/releases/net/neoforged/bus/8.0.5/bus-8.0.5-sources.jar`), this checkout, the prose before and after the tables, and the output file.

## Context

See proposal.md for the motivation. This design holds two things. The section "Native channel" fixes how ported mods run: as native Rust crates compiled into the server binary, against the native API `ironpumpkin-mods`, which takes the semantics and the catalogue of NeoForge in Pumpkin shape, and with source patches that the modpack build applies. The inventory of the NeoForge 26.3.x API surface (#34) is the contract of M4 Native API with NeoForge semantics: every row says what a mod author gets when a mod is ported to IronPumpkin. The mapping table (`mapping-table.md`, NeoForge class -> `ironpumpkin-mods` item or Pumpkin event) is built from these rows and is the porting guide. The sibling document `modpack-usage.md` (#46) lists what the reference modpack (FTB StoneBlock 4) uses.

### Sources

| Source | Version | Used for |
|:--|:--|:--|
| neoforged/NeoForge, branch 26.3.x | commit `89f2d71c9ff34aee172c1523653805e64a2fd4ef` | `src/main/java`, `src/client/java`, `patches`, `src/main/resources/META-INF/injected-interfaces.json`, `src/generated/resources/reports/registry_order.json`, `gradle.properties` |
| FancyModLoader sources jar (maven.neoforged.net) | 12.0.8 | FML lifecycle and config events, `@Mod`, `@EventBusSubscriber`, `ModContainer`, `ModList`, `ModConfig`, `neoforge.mods.toml` keys |
| NeoForge eventbus sources jar (maven.neoforged.net) | 8.0.5 | `Event`, `ICancellableEvent`, `IEventBus`, `EventPriority`, `@SubscribeEvent`, the abstract event rule |
| IronPumpkin working tree | this checkout | Pumpkin events (`crates/pumpkin-core/src/plugin/api/events`), their fire sites, WIT v0.2 `event.wit`, `metadata.wit`, `context.rs`, the phase 1 codecs (`crates/pumpkin-protocol/src/java/neoforge/`), the generated tags |

### How the generator classifies a row

| Column | Rule |
|:--|:--|
| Event | A class whose superclass chain reaches `net.neoforged.bus.api.Event`. Names resolve through nested classes, imports and the same package. |
| Bus | `mod` when the class or a supertype implements `IModBusEvent`, else `game`. |
| Side | `client` for the client source set, a `.client.` package, `FMLClientSetupEvent`, or an event that only client code constructs (`src/client`, patches under `net/minecraft/client` and `com/mojang/blaze3d`). `server` for `event.server`, the permission events, `FMLDedicatedServerSetupEvent`, or a constructor or field of a server-only type (`ServerPlayer`, `ServerLevel`, `MinecraftServer` and similar). `both` otherwise. NeoForge 26.3 has no `@OnlyIn` on API classes. |
| Cancel | The class or a supertype implements `ICancellableEvent`. |
| Result | Setters in the class or its superclasses that take a `Result` enum, a `TriState` or an `InteractionResult`. |
| Abstract | The eventbus rejects listeners on abstract event classes, so an abstract row has no handler of its own and takes its status from its subclass rows. It is `supported` when each subclass row is `supported`, `client-only` or `not supported` and at least one is `supported`. It stays `planned` while a subclass row is `planned`. It is `not supported` when each subclass row is `not supported`. The Reason cell lists the subclass statuses, counted over the concrete subclasses at any depth. |
| Holder | A class with no members of its own, such as `Tags` or `NeoForgeRegistries`, follows its nested rows or constants, as an abstract row follows its subclasses. It is `supported` when each nested row is `supported`, and `planned` while one is `planned`. The Reason cell gives the counts. |
| Status | `supported`: a Pumpkin event or API exists today. For events, the generator checks that the struct exists, that code outside the events folder constructs it, and names the files and the WIT v0.2 exposure. A mapped Pumpkin event that nothing fires becomes `planned`. A non-event API claim is checked by a string search in the named file. `planned`: server-side and mappable to the named Pumpkin concept, or server-side with an engine extension point that Pumpkin lacks (the reason has the prefix `M5 engine extension:`). `client-only`: never runs on a dedicated server. `not supported`: Java-only, client-only or display-only, or a vanilla feature that Pumpkin lacks; the reason says why. |
| Method rows | One row per public method name; overloads are counted in the Kind column. |

The status of each row comes from hand-written mapping tables next to the generator. The generator stops when a class has no mapping, a reason is empty, or a claimed Pumpkin event or API is missing from this checkout.

## Goals / Non-Goals

**Goals:**

- One row for every public NeoForge API element a server-side port touches, with side and status.
- Counts per family, so the size of each gap is visible.
- A row status that a later run can recheck against the Pumpkin tree.
- The build model, the two primitives a mod gets (the native API and compile-time source patches), and the rules that keep the native API fast, stable across upstream merges and free of cost for a server with no listeners.

**Non-Goals:**

- What the reference modpack uses. That is `modpack-usage.md` (#46).
- The patches of a mod. They live in the mod crate, and the modpack build applies them; this phase defines the contract of the build (#54).
- Precompiled drop-in mods. Mods compile from source into the pack binary; this phase does not build precompiled mods.

## Decisions

### Ported mods are native crates

Decided on 2026-10-08 and recorded in the pinned "Orchestration status" issue (#29). A ported mod is a Rust crate compiled into the server binary of its modpack, against `ironpumpkin-mods`. Wasm keeps what it is good at: sandbox, hot reload and coarse services. Loading real Java mod jars is out of scope, because a mod jar needs the whole `net.minecraft` Java API surface and Pumpkin does not have it. The section "Native channel" gives the reasons and the rejected alternatives.

### NeoForge semantics, Pumpkin shape

Decided on 2026-10-15. IronPumpkin takes the semantics and the catalogue of NeoForge, not its shape. The native API for mods is `ironpumpkin-mods`: idiomatic Rust that re-exports the Pumpkin server types a mod needs. The NeoForge events of the inventory are Pumpkin events with the same fire sites, fields, cancel and result rules and event names. The section "The native API" lists what that means for each family.

Reasons:

- The glue of a mod (event bus, registers, mod bus) is a small part of a mod. Most of a port is gameplay code and content, and it does not change with the shape of the glue.
- A Java-shaped facade is a maintenance layer. It must follow NeoForge and Pumpkin, and it lies where Pumpkin differs.
- The value is the catalogue (which events exist, where they fire, which fields and results they have) and the wire behaviour for NeoForge clients (config sync, payload negotiation, registry sync). Both survive a change of shape.

### Engine extensions are the next phase

Decided on 2026-10-15. Many inventory rows need an extension point that Pumpkin lacks: a registration API for a registry, a fixed set of types (recipes, enchantment effects, loot functions, attributes), a loader step (`neoforge:conditions`) or a world generation hook. These rows are a gap of the engine, not of the NeoForge API. They are `planned` with the reason prefix `M5 engine extension:`, and M5 Modded gameplay parity adds the extension points. The M5 rows are:

- the registries without a registration API (the "no registration API" rows), the new registry and datapack registry events, and the fixed datapack registry list;
- world generation types, biome and structure modifiers, tree decorators and generation stages;
- the datapack loader step for `neoforge:conditions` and the condition types;
- registry aliases and registry callbacks (`addAlias`, `resolve`, `getMaxId`, `addCallback`), the spawn data hook of a mod entity type (`neoforge:advanced_add_entity`), the mod menu types and the extra data of `openMenu`, and the shears hook for a mod block or entity (`IShearable`);
- fixed sets: enchantment effect types, loot functions, conditions and entry types, recipe types and serializers, ingredient types, command argument types, attributes, cauldron interactions, item abilities, fluids and fluid sound actions, chunk ticket levels and tickets, farmland water sources, default components, and the minecart collision and special plantable hooks.

A row stays `not supported` in these cases:

- Java-only: Mixin configuration, access transformers, `@ApiStatus.Internal` classes (with one exception, below), jar and mod file scanning, Java file helpers, game tests, data generation, `ServerModLoader`, the own settings of NeoForge, the `/neoforge` command tree and JUnit.
- Exception to the Internal rule: an `@ApiStatus.Internal` configuration task or payload whose wire behaviour IronPumpkin reproduces is `supported`, with the test of the wire behaviour. Other Internal classes are `not supported` (Java-only). `RegistryDataMapNegotiation` and the configuration tasks `CheckExtensibleEnums`, `CheckFeatureFlags`, `CommonRegisterTask`, `CommonVersionTask`, `SyncConfig` and `SyncRegistries` follow it.
- Client-only or display-only: tooltips, pack listing and dialog codecs.
- A vanilla feature that Pumpkin lacks and no extension point would add: the phantom spawner, village sieges and the custom spawner list.
- A design difference: one permission manager, no per-world mod version records, and the empty-click events that only the client knows.

### The statuses are hand-written and checked

The class lists, buses, sides, cancel flags, results, method counts, channel ids, registry lists and `neoforge.mods.toml` keys of the tables come from the NeoForge sources. The statuses and the Pumpkin concepts are judgement. Each task that changes a status names the test that proves it, and `tools/openspec-sync/mapping_status.py --check` keeps `mapping-table.md` in sync with the tables. A NeoForge update needs a new pass over the sources, because no generator is in the repository. The family "Extension interfaces" is the exception: `tools/openspec-sync/extension_table.py` reads the method lists from the sources again, and `tools/openspec-sync/extension_rows.tsv` holds its statuses.

### `supported` means a ported mod gets the behaviour today

A Pumpkin event that exists but fires only for a subset of NeoForge's cases is `planned`, with the gap in the reason (for example `ArrowLooseEvent`: Pumpkin fires `EntityShootBowEvent` only from mob goals). Smaller gaps that keep the event usable stay `supported` and are named in the reason (for example `CropGrowEvent.Pre` sets a result in NeoForge and cancels in Pumpkin). The mapping table verifies each supported row with a test.

### Names follow NeoForge 26.3, not older guides

Some NeoForge 26.3 names differ from older porting guides and from the roadmap text in #29. Block breaking is `net.neoforged.neoforge.event.level.block.BreakBlockEvent`, not `BlockEvent.BreakEvent`. Config types are `LOCAL`, `CLIENT`, `SYNCED` and `STARTUP` in FancyModLoader 12.0.8. The event names of the Pumpkin events follow the 26.3 names.

### World data format

IronPumpkin loads vanilla worlds. A world that IronPumpkin wrote is not meant to go back to vanilla: mod blocks and entities would be lost there. The readers are the primary target. The writers produce the 26.3 shape, so IronPumpkin stays aligned with upstream Pumpkin.

- Chunks, entity chunks, POI and player data declare DataVersion 5023 (`WORLD_DATA_VERSION`). `level.dat` keeps the version it read. At 5023 vanilla runs no data fixer, so every writer must produce the 26.3 shape. The audit of the 16 vanilla fixes between 4903 and 5023 is on #144.
- Block states use the keys of 26.3 `StateHolder.codec`: `id` and `properties`. The readers also take `Name`/`Properties` (26.2 and older) and a bare name (a default state).
- Pot decorations are the map of 26.3 `PotDecorations.CODEC`, in the item component and in the decorated pot `sherds`: one optional item per side, and only the present sides are written. The readers also take the old list and pad it with bricks, as `PotDecorationsComponentUnflatteningFix` does.
- The item reader drops `minecraft:map_color` and splits `minecraft:swing_animation` into `minecraft:attack_animation` and `minecraft:interact_animation`, as `MapColorComponentRemovalFix` and `SwingAnimationComponentSplitFix` do. 26.3 has neither key, and an unknown component name fails the whole stack.
- POI records use the `pos` int array of `PoiRecord.Packed.CODEC`. The reader also takes the `x`/`y`/`z` ints that Pumpkin wrote before.
- A sign or hanging sign read from an Anvil or pump chunk below 5002, or with no DataVersion, gets `allow_op_features=true`, as `SignsOpFlagFix` does. This applies to every such chunk, also to chunks that IronPumpkin wrote at 4903: the owner chose vanilla parity over a heuristic on the missing `LastUpdate` key.
- The code follows the upstream PR branches on the fork, so the upstream merge stays cheap: `fix/chunk-palette-26-3-keys`, `fix/pot-decorations-map` and `fix/sign-op-flag-old-data` (`fix_signs_op_flag` in `ChunkData::internal_from_bytes`). IronPumpkin diverges in one place: its palette goes through `BlockStateCodec::state_to_nbt` and `block/state_nbt.rs`, which the custom content registry needs for mod blocks and placeholders. Upstream writes the palette entry inline. The POI, armor stand and item component reader fixes change upstream files and have no upstream branch yet: they are candidates for upstream PRs.

Open rows, raw NBT that Pumpkin does not convert:

- A block entity stays raw NBT in the chunk until the world touches it, and an entity record of an unknown type stays raw. Pumpkin writes this NBT back unchanged under 5023. Data from a world below 5023 keeps its old shape there: a decorated pot that never woke up keeps the `sherds` list. IronPumpkin reads both shapes, vanilla 26.3 does not.
- `SignsOpFlagFix` also acts on a sign in item `block_entity_data` (a sign item in a chest or an inventory). IronPumpkin applies it only to chunk block entities: the item reader has no data version.
- Fixes for worlds between 4435 (the oldest `level.dat` Pumpkin accepts) and 4903 that IronPumpkin does not apply: `TridentAnimationFix` (a `minecraft:consumable` animation `spear` becomes `trident`; Pumpkin fails the stack) and `ExplorerMapItemFix` (a `minecraft:filled_map` with a `+` decoration stays a filled map instead of the 26.3 explorer map item). For worlds below 5013, `MergeTerrainChunkStatusFix` maps the chunk status `noise` and `surface` to `biomes` and drops the blocks. IronPumpkin maps them to `terrain`, so a proto-chunk that had no surface yet skips the surface step.

Reader follow-ups from the audit of a vanilla 26.3 world (the losses on load, with the vanilla codec as reference):

- Block entities: `CustomName` of chests and furnaces; `LootTable`/`LootTableSeed` of barrels (an unopened barrel loses its loot); `SpawnPotentials` of spawners; `RecipesUsed` and `speed_multiplier` of furnaces, smokers and blast furnaces; `total_fuel`, `total_brew_time` and `speed_multiplier` of brewing stands; `UpdateLastExecution` of command blocks (read as false); `listener` of sculk sensors and shrieker; `cursors` of sculk catalysts; `countdown` of potent sulfur; `align_items_to_bottom` of shelves; `resolved` of written books.
- Entities: `BlockState` and the fall fields of falling blocks; `block_state` of primed TNT; `DisplayState` of minecarts; `Inventory` and `Gossips` of villagers; `color` and `block_pos` of cushions; `block_pos` of item frames; `PickupDelay` of items above 255 (an `AtomicU8`: 32767, never pick up, becomes 255); `glow_color_override` of displays.
- Pot decorations: a 26.2 decorated pot without `sherds` gets four bricks from `PotDecorationsBlockEntityUnflatteningFix` (`orElseEmptyList`); IronPumpkin keeps it empty. It matters only for the sherd drop of a broken pot, which Pumpkin does not have yet.
- Writer that IronPumpkin reads back but vanilla does not: `potion_contents.potion` as an integer id (26.3 `Potion.CODEC` takes the registry name).

## Native channel

### Why native

A Wasm hook is too slow for the code that mods put inside vanilla logic. Each call into a Wasm plugin goes through the plugin's `StoreExecutor` (`crates/pumpkin-plugin-runtime/src/executor.rs`, started by `start_legacy_store` in `crates/pumpkin-wasm-host-common/src/concurrent_store.rs`). The caller sends the call to the store driver task and waits for its answer, so a hook costs a cross-task round trip. There is one store per plugin, so every world that calls the same plugin waits on one executor. This rules out per-entity and per-tick hooks, and those are the hooks mods need most: the most mixin-targeted vanilla server classes of the reference modpack are `Entity` (43 mods, 63 mixin classes), `LivingEntity` (42, 66), `Player` (30, 38) and `ItemStack` (23, 26), from the mixins section of `modpack-usage.md`. A native call costs a function call. Java mods ran in the server process with no sandbox, so native mods keep the trust model a modpack already has.

### Build model

- `crates/pumpkin` is a library with a small entry point (`pumpkin::run()`, #52), and `src/main.rs` calls it. The server binary of upstream Pumpkin is the same program with no mods linked.
- A modpack is a Cargo workspace. Its bin crate depends on the `pumpkin` library crate and on one crate per mod, and references each mod crate so that the linker keeps it.
- A mod registers itself at link time through `inventory` (`linkme` where `inventory` fails on a CI target), with the `register_mod!` macro of `ironpumpkin-mods` (#52). The server collects the mods and calls each mod's `init` once in the startup content phase before the first world loads. The call order is the load order: the `ordering` of the dependencies in `neoforge.mods.toml`, with the mod id as tie-break. `ironpumpkin-mods` is both the registration and the native API of the mod.
- A mod can return a `neoforge.mods.toml` from `NativeMod::mods_toml() -> Option<&'static str>`, and the host then reads its id, version, dependencies and ordering from it, as for a Java mod. A mod that returns `None` uses the trait methods `id`, `display_name` and `version`.
- The workspace pins IronPumpkin by git commit and each mod crate by version or revision; its `Cargo.lock` makes the pack build reproducible.
- The pack build checks out IronPumpkin at the pinned commit, applies the source patches of the mods (section "Source patches" below), and then compiles the server with the mods.
- A blueprint holds the workspace template (`modpack.toml` with the IronPumpkin commit and the mod list, the bin crate, the build tool `cargo xtask check|build` that applies the patches) and a GitHub Action that builds Linux and Windows binaries per pack on a tag, with a cached `target/`. It lives under `blueprint/` in this repository (#54), and the template repository `EdenNetworkItalia/ironpumpkin-pack-template` holds it with an empty mod list. The build points the server crates that the binary links at the pinned checkout and patches only those crates (`316f16246`). `examples/modpack` is the minimal example pack.

### Two primitives

A mod gets two primitives from IronPumpkin, and nothing else:

1. The native API of `ironpumpkin-mods` with NeoForge semantics. Events, lifecycle, registration, config, network, capabilities and the other families behave as on NeoForge, in the form of Pumpkin and Rust.
2. Source patches. A mod carries patches to the IronPumpkin source tree, and the modpack build applies them before it compiles. Everything that a NeoForge mod does with a mixin, with reflection or with an access transformer, and that no NeoForge event or API of the inventory does, becomes a source patch. Mixins, reflection and access transformers stay compile-time source patches.

There is no hook catalogue, no generated accessor API and no service-seam API of IronPumpkin's own. A hook point for a mod is a NeoForge event.

### The native API

`ironpumpkin-mods` (`crates/ironpumpkin-mods`, MIT OR Apache-2.0) is the native API for mods. The `net.minecraft` side stays the types of Pumpkin: there is no Mojang-shaped facade. Each family has its delta spec in this change.

- Surface. The surface for NeoForge behaviour is the NeoForge public API of the inventory in this document, and nothing more. No hook, accessor or service seam of IronPumpkin's own is exposed to mods. The one IronPumpkin-only extension is the vanilla `display` entry of block, item and entity type properties, documented as such. The implementation can put internal hooks on Pumpkin methods to fire the events. These hooks are host code, not API: a mod cannot name them, and they can move on any upstream merge.
- Events. The NeoForge events of the inventory are Pumpkin events (the plugin event system in `crates/pumpkin-core/src/plugin/`: `Payload`, `ModInit::register_event`, `EventPriority`, blocking handlers and cancel) with the same fire sites, fields, cancel and result rules and event names as NeoForge 26.3. A concrete NeoForge class is one struct, and a nested class is flattened with the parent prefix (`ServerTickEventPost`). One event per fire site: where Pumpkin has an event for the fire site, the mapping table maps the NeoForge event to it and the missing fields are added to it (`BlockBreakEvent` for `BreakBlockEvent`), as an upstream parity candidate. A new struct with the NeoForge name exists only where Pumpkin has no event for that fire site. There is no second dispatch layer. The fire sites have these properties, which a port relies on:
  - `LevelTickEvent` fires while the game is frozen too. In a normal tick the worlds tick in parallel, so handlers of two worlds can run at the same time.
  - `ServerStoppingEvent` fires after the ticker thread exits, with a 30 s bound. A tick that runs longer logs a warning and the shutdown continues.
  - `PrepareAnvilEvent` and `PrepareGrindstoneEvent` fire outside the screen-handler lock on every result update. The result applies only if the inputs did not change while the event fired.
  - `PlayerEnchantItemEvent` fires before the lapis is consumed, and the table writes the item back. `EnchantmentLevelSetEvent` has `pos` and a getter for the original levels. Both events and `EnchantItemEvent` fire outside the enchanting-table screen-handler lock. The click goes on only when the table did not change while an event fired.
  - `GrindstoneEvent.OnTakeItem` fires outside the screen-handler lock. The grindstone defers the vanilla experience and the input change to it only when a handler listens.
  - A cancel of `PotionBrewEvent.Pre` takes the stacks only if the stand inventory did not change meanwhile. `PotionBrewEvent.Post` takes changed stacks under the same rule.
  - `ServerChatEvent` fills `raw_text` only when a handler was registered when the event is created.
  - No player event fires under a screen-handler lock or a shard lock of the entity tracker. `AnvilCraftEventPre`, `AnvilCraftEventPost` and `TradeWithVillagerEvent` fire after the slot click released the screen-handler lock of the anvil or the trade screen. `PlayerEvent.StartTracking` and `StopTracking` fire after the iteration of the entity map (`player_events::Deferral`). The tracker updates the entities in parallel on rayon workers, and each worker fires its own events after its update: handlers of two entities can run at the same time, and their order between entities is not fixed. `PlayerEventNameFormat` and `PlayerEventTabListNameFormat` are not cached and fire on every read of the name. `PlayerEvent` and the other abstract classes have no struct: EventBus 8.0.5 rejects a listener on an abstract event. The concrete parent `PlayerContainerEvent` is named by `#[event_parents(..)]` on `InventoryOpenEvent` and `InventoryCloseEvent`.
  - `StatAwardEvent` fires on `increment_stat` and `set_stat` with `value`, the new total, resets to 0 included, as NeoForge fires on `setValue`.
  - `ServerAboutToStartEvent` fires before the worlds load, after the synced configs load. Only native mods receive it.
- Priority. `PluginManager` (`crates/pumpkin-core/src/plugin/mod.rs`, `dispatch.rs`) keeps each handler list in dispatch order at registration: blocking handlers before non-blocking handlers, each from `Highest` to `Lowest`, in registration order inside one priority. A non-blocking handler runs after the handler before it finished, as NeoForge posts to one listener at a time; upstream Pumpkin runs the non-blocking handlers of an event at the same time (`join_all`). Upstream Pumpkin stores `EventPriority` and never reads it, so this is an IronPumpkin change and an upstream parity candidate. The registration has the NeoForge `receiveCanceled` flag: a handler does not run for a cancelled event unless it sets the flag. `Context::register_event` (Wasm hosts) always receives cancelled events. Priority between v0.2 Wasm plugins and native mods follows from the single dispatch. The reentry depth of 64 of `pumpkin-plugin-runtime` is a limit of Wasm plugins and is not part of the native API.
- Parent events. A concrete NeoForge class with concrete subclasses is a struct, and each subclass struct names it in `Payload::get_parent_names_static`. A handler on the parent is a `ParentEventHandler` that a mod registers with `ModInit::register_parent_event`: it gets the parent struct and every subclass struct as `dyn Payload` and downcasts it. The dispatch merges the handlers of the subclass and of its parents in one priority order, the subclass handlers first inside one rank. An abstract NeoForge class has no struct, so nobody can listen to it.
- Guardrail helpers. `PluginManager::fire_with` and `fire_blocking_with` take a closure that builds the event, and call it only when `has_handlers` finds a handler of the event or of a parent.
- Synchronous handlers. Handlers are synchronous inside the async host of Pumpkin. A handler that needs a world call uses `block_on(unconstrained(..))`, as `ModInit::register_service` does (`crates/ironpumpkin-mods/src/lib.rs`), and never holds a lock across a fire.
- Lifecycle. The lifecycle events (`FMLCommonSetupEvent`, `FMLLoadCompleteEvent` and the others) are methods of the `NativeMod` trait with a default no-op. `init_mods` calls `init` of every mod in load order and fires `RegisterEvent` per registry. Then the Local configs load, and `common_setup`, `dedicated_server_setup`, `inter_mod_enqueue`, `inter_mod_process` and `load_complete` each run on every mod in turn, in NeoForge order. Inter-mod messages are a queue on `ironpumpkin-mods`. `neoforge.mods.toml` is optional through `NativeMod::mods_toml()`, and the trait methods are the fallback. A mod with a mods.toml uses the FML mod id pattern (FancyModLoader `ModInfo.VALID_MODID`, `^(?=.{2,64}$)[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`), and its registry namespace can differ from its mod id. The `license` key is required: a missing or blank license stops the server with "Missing license", as in FancyModLoader `ModFileInfo`. `init` runs in the load order of FancyModLoader `ModSorter`: the `BEFORE` and `AFTER` orderings of the dependencies, with the mod id as tie-break. The mod list API (`is_loaded`, `get_mods`, the sorted mods and the container by id) lives in `ironpumpkin-mods`. The mod list holds the system mods `minecraft` and `neoforge`, so a ported descriptor that requires them loads. A process that embeds several servers loads the mods once, with the first server, as NeoForge does: `init`, content, `RegisterEvent`, the registration events, the lifecycle methods, the Local configs and `RegisterPayloadHandlersEvent` (`NetworkRegistry.setup` runs once). Each later server gets the commands, permissions, event handlers and services that `init` registered, on its own event bus, and the payload channels of the first fire. Each server loads the Synced configs before `ServerAboutToStartEvent` (`ServerLifecycleHooks.handleServerAboutToStart`), starts its own config poller and fires the server lifecycle events. Differences from NeoForge: the mod loading runs inside the first `Server::new`, because `RegisterEvent` fires through the event bus of a server; there is no world `syncedconfig` override. Test `a_second_server_reuses_the_mods_loaded_by_the_first` (`crates/ironpumpkin-test-mod/tests/second_server.rs`).
- Registration. The M2 builders are the API: `ModInit::register_block`, `register_item`, `register_entity_type` and `register_tag` with `content::BlockBuilder` and the other builders (`crates/ironpumpkin-mods/src/content.rs`, backed by `crates/pumpkin-data/src/dynamic.rs`). Entries apply at init and bind at the single freeze. The mapping table maps the typed registers of NeoForge to these functions. `RegisterEvent` is a Pumpkin event that fires once for `minecraft:block`, `minecraft:entity_type` and `minecraft:item`, in that order, after the last `init` and before the Local configs load; a handler registers through `content::RegisterEventExt`, which routes to the same builders and registers only on the fire of its own registry. Lookups after the freeze go through the frozen tables: `ironpumpkin_mods::registry` has `contains`, `id_of`, `key_of`, `keys`, `is_in_tag` and `tag_entries`, and a placeholder is not an entry. `RegistryError::RegistryFrozen` is `registry-frozen` and `RegistryError::Duplicate` is `duplicate`. `ModInit::register_creative_mode_tab` accepts a creative tab, logs one line and ignores it. The registration events `EntityAttributeCreationEvent`, `EntityAttributeModificationEvent`, `RegisterSpawnPlacementsEvent` and `BlockEntityTypeAddBlocksEvent` (after the last `RegisterEvent`) and `ExtendPoiTypesEvent` (after the sided setup) are `NativeMod` methods with a handle of `ironpumpkin_mods::registration`; the handles take names, because they fire before the freeze, and keep the first error, which stops the server after the step. Behaviour by type: `BlockBuilder::block_type` names a vanilla block (26.3 has no `minecraft:block_type` registry), whose server behaviour, properties, per-state physical data and generated tags the block takes, and `ItemBuilder::item_type` names a vanilla item whose server behaviour the item runs; a behaviour of the mod wins over the type. The generated property structs accept a typed block where they accept its type, and `Taggable::has_tag` answers for a custom entry through its block type or display entry (`pumpkin_data::dynamic_types`).
- Mod config. A Rust TOML config spec in `ironpumpkin-mods` has a builder (comments, sections, typed values with ranges, enums and lists), correction, write with comments, `get`, `set` and `save`, and reload by polling every N ticks. It adds no dependency. The types are Local, Synced and Startup, and the files are `config/<modid>-<type>.toml` like NeoForge, so client-side and server-side files stay parallel. The Synced type feeds the `neoforge:config_file` sync through a live source: each synced `ModConfig` is a `startup::SyncedConfigSource`, and the `SyncConfig` configuration task reads the current file for each connection, so a reloaded file reaches every later client. Startup configs load inside `ModInit::register_config` during `init`, so `init` reads them before it registers content. Local configs load after every `init` and before `common_setup`. Synced configs load after `load_complete` and before the server starts. The parity harness trace of a NeoForge 26.3.0.64-beta dedicated server (`tools/parity-probe/captures/neoforge-26.3.0.64-beta/probe-trace.jsonl`) has the same order: construct, `RegisterEvent`, `FMLCommonSetupEvent`, `FMLLoadCompleteEvent`, `RegisterPayloadHandlersEvent`, `ModConfigEvent.Loading` of the synced config, then `ServerAboutToStartEvent`. Loading and reloading are the `NativeMod` methods `config_loading` and `config_reloading`. The hooks of all mods run one at a time. A `save` inside a hook of the same config writes the file and does not call the hook again. The host reads at most 1 MiB of a config file, and a larger file fails to load. A boolean value accepts the strings "true" and "false" in any case, as the NeoForge validator does. The poll interval is `mod_config_reload_ticks` in `pumpkin.toml`, and the poll is a host handler of `ServerTickEndEvent`. The module `ironpumpkin_mods::config` documents the correction rules.
- Network. `ironpumpkin_mods::network` registers payload channels over the M3 negotiation (`NegotiatedState`, `NetworkRegistry::register`). `RegisterPayloadHandlersEvent` is a Pumpkin event that fires once after `load_complete` and before the synced configs load, and its `PayloadRegistrar` registers a channel by id with raw bytes; the handlers live in `NetworkRegistry` (`plugin/network_events.rs`) and are fixed after the event. A payload reaches the handler of its channel: in play on the world tick of the sender (`HandlerThread::Main`) or on the connection task (`HandlerThread::Network`), in configuration on the connection task. A required channel that the client did not negotiate disconnects it with `multiplayer.disconnect.incompatible`, an optional one is readable ad hoc, a client-bound one is dropped. `send_to_player` fails with `ChannelNotNegotiated` where `checkPacket` throws, and the broadcasts skip such clients. `RegisterConfigurationTasksEvent` fires once per Java connection after the negotiation; its `CustomConfigurationTask`s become `ConfigurationTask::Mod` entries after the tasks of NeoForge and before `JoinWorldTask`, and a finish request for a task that is not current disconnects with the message of `finishCurrentTask`.
- Capabilities. `ironpumpkin_mods::capabilities` has the capability keys (`BlockCapability`, `EntityCapability`, `ItemCapability`, created by name with a type check), the built-in keys in `item`, `fluid` and `energy`, lookups, `BlockCapabilityCache` and `invalidate_capabilities`. `NativeMod::register_capabilities` is `RegisterCapabilitiesEvent`: the host calls it once per `EventPriority` phase for every mod in load order, and acts as the `neoforge` mod first in each phase (proxyable marks at `High`, vanilla providers at `Normal`), so a fallback at `Low` sees every `Normal` provider whatever the load order. `ironpumpkin_mods::transfer` has the handlers and transactions: a transaction is per thread and `!Send`, and a nested one borrows its parent. Worlds tick in parallel, so a journal belongs to the root that touched it first until that root closes, and a transaction of another thread transfers nothing in it meanwhile (insert and extract return 0; owner decision on #153). A rollback of a Pumpkin inventory slot gives back only the amount that the transaction moved, because code outside the transaction can write the slot in between. A transactional write defers the vanilla `setItem` side effects to the root commit (furnace cooking reset, hopper cooldown), and the wrappers apply the vanilla `canPlaceItem` and `getMaxStackSize` rules of each container. The face rules of `WorldlyContainer` are a table in `transfer::inventory`, so `pumpkin-inventory` stays unchanged. The engine reaches the module through `pumpkin_core::plugin::capabilities`: block entity placement and removal, chest type changes and chunk load and unload invalidate caches, and hoppers, droppers, crafters and hopper minecarts ask `item::BLOCK` of a block without a vanilla inventory after the block container and before the container entities (`HopperBlockEntity.getContainerOrHandlerAt`, `VanillaInventoryCodeHooks`).
- Data maps. `ironpumpkin_mods::data_maps` has `DataMapType` and `AdvancedDataMapType` with their builders, the mergers, the removers and `RegisterDataMapTypesEvent` (`NativeMod::register_data_map_types`, after the capabilities). The value type is the codec of the map: `Decode` reads the files and `Encode` writes the form of the network codec, so a synced map with a different network codec encodes that form. The host registers the eleven `NeoForgeDataMaps` types. After `DatapackManager::load_all`, at startup and on `/reload`, the loader of the module reads the data map files of the enabled data packs, as `DataMapLoader`, and fires `DataMapsUpdatedEvent`. The engine reaches the module through `pumpkin_core::plugin::data_maps`: the known maps of the configuration task, the refusal of a vanilla client when a map is mandatory, and `neoforge:registry_data_map_sync` after `CSetSelectedSlot` on join and to every player after `/reload`. A data map with `mandatorySync` turns on the `NeoForge` client detection, as a client-required mod does. A synced value whose JSON exceeds the 32767 characters of `writeUtf` is dropped at load. A block transform is checked against `BlockTransformData.CODEC` at load (block predicates and typed int providers by their `type` only), so a value that a `NeoForge` client cannot decode never syncs. IronPumpkin evaluates no `neoforge:conditions`, so a conditional value does not load. The server gives the loader the NeoForge default values as a first pack in memory, built from the vanilla tables and data of the server crates (no NeoForge file is copied), so a data pack overrides them. After each load `ironpumpkin_mods` hands the values of the eleven maps to `pumpkin_core::plugin::data_maps::builtin`, and the vanilla behaviours read them where NeoForge does: see the rows of the built-in data maps.
- Menus. `ironpumpkin_mods::menus` opens a menu of a vanilla menu type over a Pumpkin screen handler: `open_menu` takes a `MenuDefinition` (the type, the title, the slots, a container of the menu and the data slots) and a `MenuLogic` with the overridable methods of `AbstractContainerMenu` and `Slot`, closes the open menu and fires `InventoryOpenEvent`. The vanilla client builds the menu of a vanilla type itself, so `open_menu` fails with `MenuError::Layout` when the slot or data slot count differs from the vanilla 26.3 class (owner decision on #154). `MenuDefinition::vanilla` builds the storage menus (`generic_9x1` to `generic_9x6`, `generic_3x3`, `hopper`, `shulker_box`) with the vanilla slots and shift-click. A slot reads one of four sources: the player inventory, the container of the menu, a Pumpkin inventory or a resource handler; a click, a button, a close and a `Menu::with` call each run in one root transaction, a handler slot writes in a nested transaction of it, and a handler that moves less than a write needs rolls the whole action back and restores the cursor and the other slots, so the menu keeps the journal ownership of the transfer API under parallel ticks (owner decision on #154). Data slots go out at the broadcast as 16-bit values, as in vanilla. A `MenuAccess::close` inside a click, a button or a `Menu::with` call closes the menu when the screen handler lock drops, through the `Deferral` of the click, of the button and of `Menu::with` (owner decision on #154). Mod menu types, the extra data of `openMenu` (`neoforge:advanced_open_screen`) and the 32-bit `neoforge:advanced_container_set_data` wait for M5.
- Shapes. `ironpumpkin_mods::shapes` reads the shapes of states and blocks (`get_state_shape`, `get_block_shape` with `ShapeKind`) and tests collisions (`is_face_sturdy`, `no_collision`). `VoxelShape` (`crates/pumpkin-data/src/voxel_shape.rs`) is a union of boxes, the format that Pumpkin collision reads; `join` with `OnlyFirst` subtracts boxes. A mod block declares its outline and its collision shape per state with `BlockBuilder::shape` and `collision_shape`. The freeze calls them once per state and stores the boxes after the generated boxes (`crates/pumpkin-data/src/dynamic_shapes.rs`), as the vanilla per-state shape cache does, so the collision loop with no mod blocks reads the generated table as before and a mod block costs no hook call. Shapes depend on the state only: `dynamicShape` and context-dependent mod shapes are not supported, because a per-block hook in the collision loop costs a call per block and a world call from inside it can deadlock. A block without declared shapes is a full cube, or has the shapes of its block type, as on NeoForge; vanilla clients never join a server with mods, so the display state does not decide the shapes. The derived data of a mod state (sturdy faces, full cube, legacy solid, solid render, redstone conductor, light opacity) follow its shapes. Pumpkin extracts only the outline and the collision shape of vanilla states, with the empty context. The block support getter has the shapes of its vanilla overrides in `voxel_shape.rs`; the interaction, occlusion and visual getters give the vanilla defaults, and their 16 vanilla overrides are a documented gap that an Extractor change can close. `RIGID` and the center of a side face are computed from the block support shape; the full faces and the vertical centers read the extracted flags. Block collision tests scan as `BlockCollisions` (one more cell on each side, for shapes taller than a block) and take the collision context of the entity for powder snow and scaffolding (`crates/pumpkin-core/src/world/collision_context.rs`).
- Other families. Attachments, data maps and loot modifiers behave as on NeoForge and are modules of `ironpumpkin-mods` with Rust names. Attachment storage and the data map loader are part of M4 (owner decision of 2026-11-23); mod menu types wait for the engine phase (M5).
- State of mixins. The state that a mixin adds to a vanilla class with `@Unique` (443 members in 61 mods) becomes an attachment of the entity, block entity, chunk or level (spec `neoforge-attachments`).
- Growth. The API grows only from real patches. Every accepted patch is a candidate event with NeoForge semantics for a later version of `ironpumpkin-mods`: when an event can do what the patch does, the event enters the API and the inventory, and the patch leaves the mod.
- Mods that other mods extend. A mod that other mods extend exposes its own events and API, as NeoForge mods do. Source patches apply to the IronPumpkin tree only, so the 881 mixin members that target other mods (73 mods into 46 mods, section "Mixins into other mods" of `modpack-usage.md`) become events or API of the target mod, or a change that the two mod authors agree on.

### Contract changes to existing Pumpkin events

These Pumpkin events that upstream already has get a new contract here. A plugin or mod that handles them sees the new behaviour.

- `PrepareAnvilEvent` fires on every anvil result update when the left input is not empty: a click on the input or result slots, a quick move or a rename. It fires after the screen-handler lock is released. It carries the vanilla result, cost and material cost. `output`, `repair_cost` and `material_cost` apply. A cancel clears the output and the cost. The result applies only if the inputs, the name and the result did not change while the event fired.
- `PrepareGrindstoneEvent` fires on every grindstone result update (an input change or a take), outside the lock. `result_item` is the id of the vanilla result. A set `output` replaces the result, with `xp` used when it is 0 or more. A cancel clears the result. The same guard on unchanged inputs applies.
- `PlayerStatisticIncrementEvent` fires on `increment_stat` and on `set_stat`. `value` is the new total, which the server stores in the stat that `category` and `stat` name after the handlers. `amount` is the change.
- `PrepareItemEnchantEvent` and `EnchantItemEvent` fire after the enchanting-table screen-handler lock is released. The rows and the enchantments apply only if the table did not change while the event fired.
- `PlayerSpawnLocationEvent` (NeoForge `PlayerRespawnPositionEvent`) has new fields: `yaw`, `pitch`, `dimension`, `original_spawn_pos`, `original_dimension`, `from_end_fight` and `copy_original_spawn_position`. `new()` takes the player, the position, the yaw, the pitch, the dimension, `from_end_fight` and `copy_original_spawn_position`. The respawn writes back the position, the yaw, the pitch and the dimension. `copy_original_spawn_position` set to `false` clears the respawn point of the player. A cancel keeps the vanilla position and the respawn point.
- `InventoryOpenEvent` and `InventoryCloseEvent` also reach the handlers of their parent `PlayerContainerEvent`.
- `EntitySpawnEvent` also fires for an entity that loads from its chunk and for each rider that a spawned mob brings, as NeoForge fires `EntityJoinLevelEvent` from `PersistentEntitySectionManager.addEntity` and for each passenger of `addFreshEntityWithPassengers`. It has the new field `loaded_from_disk`, true only for a loaded entity, so a handler that cancels a spawn can leave loaded entities alone. A cancel keeps the entity out of the world; a cancelled loaded entity is not saved again.
- `EntitySpawnEvent` also fires from `World::spawn_entity_non_save`, as NeoForge fires `EntityJoinLevelEvent` from every `addFreshEntity`: a falling block, a split slime, a ghast fireball, a shulker bullet, a hatched turtle, a mob from a nether portal, the drops and eggs of an armadillo or a sniffer, a mob of the natural spawner, the lightning bolt and the skeleton horse of a thunderstorm, the raiders of a raid wave and their ravager riders, the ender dragon and the end crystals of the dragon fight, and the iron golem that villagers summon. A cancel spawns nothing. A cancelled raider stays in its raid, and a cancelled dragon still sets the dragon of the fight, as on NeoForge, which ignores the result of `addFreshEntity` there.
- `BrewEvent` (NeoForge `PotionBrewEvent.Pre`): a cancel takes the changed `stacks` if the stand inventory did not change while the event fired, and `PotionBrewEventPost` fires after it.
- `PlayerCustomPayloadEvent` and `ConfigCustomPayloadEvent` no longer fire for a payload on a channel that a native mod registered with `PayloadRegistrar`: the payload goes to the handler of its channel only, as NeoForge hands a modded payload to its handler. Every other channel still reaches them. The server also disconnects a client in two cases. A payload on a required mod channel that the client did not negotiate disconnects it with `multiplayer.disconnect.incompatible` ("No Channel for <id>"). A payload on any mod channel before the negotiation disconnects it too ("No Payload Setup"). Upstream Pumpkin hands such a payload only to `ConfigCustomPayloadEvent` or `PlayerCustomPayloadEvent`.
- `PlayerInteractEvent` has new fields: `hand`, `face`, `hit_vector`, `left_click_action`, `use_block`, `use_item` and `cancellation_result`. `new()` keeps its arguments and gives the defaults; `left_click_block`, `right_click_block` and `right_click_item` build the NeoForge subclasses.
- `PlayerInteractEvent` with the action left click block fires for each dig action in `player_action.rs` (start, stop and abort), as NeoForge does. An arm swing into a block fires nothing, so a plugin that listened on the swing sees different behaviour. A cancel stops the action and sends the block back. It no longer suppresses the swing broadcast, which always plays. In adventure mode nothing fires, because the vanilla client sends no dig action.
- `PlayerInteractEvent` on an item use (`use_item.rs`) fires with the action right click air whatever the ray of the player hits, before the item use and not while the item is on cooldown, and no longer with the action right click block. A cancel also stops the equip and the start of the use.
- `PlayerItemHeldEvent` fires only when the slot changes (Java `set_held_item.rs`, Bedrock `mob_equipment.rs`). A vanilla Java client sends the packet only on a change, so the difference shows mostly on Bedrock.
- The villager resends its offers to the trading player from its own tick, one tick after the trade, as vanilla 26.3 does (`Villager.onReputationEventFrom` calls `updateSpecialPrices`). `resend_offers_to_player` is removed.
- `BlockBreakEvent` has the new field `state`. `exp` now sets the experience that the break drops; upstream Pumpkin keeps it at 0 and never reads it.
- `BlockPlaceEvent` has the new fields `placed_state` and `replaced_state`, and `BlockMultiPlaceEvent` has the new field `replaced_blocks`.
- `EntityExplodeEvent` has the new field `world`.
- The damage of an item frame, a painting and an end crystal reads `Entity::is_invulnerable_to`, as vanilla reads `isInvulnerableToBase` there: an invulnerable or removed painting no longer breaks, and an end crystal, which is fire immune, no longer explodes from fire damage. `EndCrystalEntity::set_invulnerable` sets the base invulnerable flag of the entity, and a fixed item frame reads `is_invulnerable_to` too. `check_spawn_rules` takes the world as `&Arc<World>` and the spawn reason, as vanilla `SpawnPlacements.checkSpawnRules`, and `spawn_util::try_spawn_mob` takes the spawn reason, as vanilla `SpawnUtil.trySpawnMob`.
- `ChunkLoad` carries the chunk as `Arc<ChunkData>`. `ChunkLoad` and `ChunkUnloadEvent` now fire from the world tick; upstream Pumpkin never fires them.
- `LivingEntity::visibility_percent` takes the targeting entity, as vanilla `getVisibilityPercent(Entity)` does, in place of its entity type.

The event layouts, the signature of `LivingEntity::visibility_percent` (#137) and the signatures of `check_spawn_rules` and `spawn_util::try_spawn_mob` (#138) changed, and the merge of upstream Pumpkin (#140) changed more layouts: `EntityBreedEvent` has `experience`, `CreatureSpawnEvent` has the `Bucket` reason, and `plugin::level::EntitySpawnReason` is the upstream `entity::mob::spawn::EntitySpawnReason`. So `PLUGIN_API_VERSION` is 7 and a native plugin built for an older version does not load. Upstream Pumpkin is at 5. The WIT v0.2 records of these events carry the new fields, and `neoforge-events.wit` holds the records of the events that IronPumpkin adds (#131, #136, #137, #138). A v0.2 handler of a `neoforge-events.wit` event writes back only the fields that NeoForge can set; the host keeps the others. The merged Pumpkin events (`BlockBreakEvent`, `PlayerInteractEvent`, `EntityExplodeEvent` and the others of `event.wit`) keep the upstream write-back of their old fields, such as the block, the position and the player, and keep the new fields that NeoForge cannot set. The change is source-compatible, but the new record fields and the new cases of `event-type` and `event` change the component type, so a prebuilt v0.2 guest must be rebuilt. The package stays `pumpkin:plugin@0.2.0` because v0.2 is unreleased behind the `plugin-v0_2` feature.

### Source patches

A source patch is the compile-time equivalent of a mixin: it changes the server source of one pack build, and it is bound to one IronPumpkin commit.

Mechanism:

- A mod crate ships `patches/<name>.patch` files against the IronPumpkin source tree (unified diff, paths from the repository root, as `git diff` writes them). It declares the IronPumpkin commit that the patches are written for in its `Cargo.toml`:

  ```toml
  [package.metadata.ironpumpkin]
  commit = "<full 40-character commit hash>"
  ```

- The modpack build checks out IronPumpkin at the commit that `modpack.toml` pins.
- It applies the patches of every mod in the order of the keys of `modpack.toml`, which are the crate package names, in byte order, then in file name order within a mod, with `git apply`: no fuzz, no whitespace leniency and no three-way merge.
- It fails the build on any conflict, and on a patch written for another commit unless the pack opts in with `allow-drift = true` in `modpack.toml`. With `allow-drift`, a patch for another commit applies with a warning, and a conflict still fails the build.
- Then it builds the server with the mods.

Errors of the build, each one a failed build with a message:

| Case | The message names |
|:--|:--|
| A patch written for another commit, with no `allow-drift` | the mod, the patch, the commit of the patch and the commit of the pack |
| A patch that does not apply | the mod, the patch and the file; when an earlier mod changed the same file, that mod too |
| A patch that touches a generated file or a `Cargo.lock` | the mod, the patch and the file |
| A patch with no justification file | the mod and the patch |
| A mod with patches and no `[package.metadata.ironpumpkin] commit` | the mod and the patch |

Limits:

- A patch touches the server source only. It never touches the generated files under `crates/pumpkin-data/src/generated` (pumpkin-codegen writes them) and never a `Cargo.lock`, and the build rejects a patch that does.
- Two mods that patch the same hunk fail the build. The pack maintainer resolves the conflict in one of three ways: by ordering (the later mod gets a patch written against the tree with the earlier mod's patch applied), by a shared patch that both mods use, or by proposing an event that replaces both patches.
- A patch is version-bound. After an IronPumpkin update, the mod author writes the patch again against the new commit. The cost of an upstream merge stays on the patches, not on the server.

Governance:

> A patch is accepted in a mod only with a justification file next to it (`patches/<name>.md`) that says what the patch does, why a NeoForge event or API is not enough, and that the patch is bound to the IronPumpkin commit it names. Every accepted patch is a candidate event with NeoForge semantics for a later version, so the API grows from real patches.

Licence: a patch is a derivative work of the GPLv3 server, like a linked mod. A distributed pack binary comes with the source of every linked mod and every applied patch.

### Coverage of the reference modpack

The method-level mixin scan of the reference modpack (section "Mixin targets by method" of `modpack-usage.md`, #51) shows flat demand: 2674 server-side mixin members with a vanilla target, in 157 of 414 mods, hit 1931 distinct target members of 519 vanilla classes. The 200 most targeted members cover 26% of the mixin members and 11% of the mods. A catalogue of IronPumpkin hook points, accessors and service seams sized by that demand would be the largest API of the project and still leave most mods without a port.

How the two primitives split the 2674 members:

| Primitive | Members | Share | How the share is counted |
|:--|--:|--:|:--|
| A NeoForge event of the inventory | 94 | 3.5% | the member is in the hook class (pre, post or cancel: an injection at the start or the end of the method, or a cancellable injection), and every vanilla method that it targets fires a server-side or common NeoForge event of the inventory whose status is not `not supported` |
| Source patches | 2580 | 96.5% | all other members: 1086 accessors, 589 call-site and override members (value, mid and overwrite: a change inside the method body needs a patch even where an event fires), and 905 members of the hook class whose methods fire no such event |

Of the 1588 members that are not accessors, 999 are in the hook class, and 94 of them (9%) can use an event. They hit 58 of the 1931 target members, and 40 of the 157 mods have at least one of them. 3 mods have only such members, so the API alone ports their mixins. 8 of the matched events have the status `not supported` in the inventory (for example `VillageSiegeEvent` and `EntityEvent.Size`); with them, the count is 98 members (3.7%).

Method of the count: the fire sites are the calls in the server-side NeoForge source patches (`patches/net/minecraft`, without the client packages) that post an event, directly or through `EventHooks` and `CommonHooks`. Each fire site is mapped to its enclosing vanilla method in a Vineflower decompile of the Mojang-mapped server: 322 fire sites, 295 of them in a vanilla method, 231 vanilla methods, 161 events of the inventory, 153 of them with a status other than `not supported`. The count uses the patches of NeoForge branch 1.21.1, because the reference modpack runs NeoForge 21.1 and its mixins name 1.21.1 methods. The patches of the inventory commit of branch 26.3.x give 67 members (2.5%), because some methods have other names in 26.3 (`LivingEntity.hurt` is `hurtServer`). The count is an estimate in both directions: an event in the target method can fire at another point than the one the mixin changes, and an event that fires in a called method does not count.

Performance consequence: a native mod and a patch run in the server process, at the cost of a function call. The cost of the Wasm executor (a cross-task round trip per call, one store per plugin) does not apply to native mods; it applies to Wasm plugins. A native handler gets an event per object, as on NeoForge. The guardrail below keeps the cost of a fire site at zero when no mod listens.

What has no equivalent:

- Client mixins (1539 mixin classes in the `client` array of their config; 1137 vanilla client targets). A dedicated server never runs that code.
- Mixin plugins that apply mixins conditionally (108 mixin configs name one). A native mod uses Cargo features or its config instead.
- Mixins that change generated data (block states, registries and other tables under `crates/pumpkin-data/src/generated`). A patch cannot change generated files; such a change goes into IronPumpkin itself, through pumpkin-codegen.

### Performance guardrail

The condition of the owner: no evident impact on Pumpkin performance.

- Every new fire site checks for registered handlers before it constructs the event. When no mod listens, the site allocates nothing and clones no stack or position. `fire_dyn` already returns early on an empty handler map, but the call site builds the event first, so the check moves in front of the construction. `PluginManager::has_handlers::<E>()` is the check.
- Per-entity and per-tick events sit behind the same check.
- A v0.2 Wasm listener of a tick event costs one guest call per entity or player and tick. `cargo bench -p pumpkin-wasm-host-v0_2 --bench tick_events` (criterion, bench profile, AMD Ryzen 9 5950X) measures one fire of `EntityTickEventPre` from one caller: about 10 ns with no handler, about 175 ns with a v0.2 handler that converts the event to its WIT record and back, and about 9.3 µs with that conversion inside a guest call of the plugin `StoreExecutor`, as `WasmPluginEventHandler` runs it. `PlayerTickEventPre` costs 9 to 10 µs the same way (9.2 and 10.2 µs in two runs). The bench store has no component, so the figure leaves out the lowering of the record into the guest and the code of the guest: it is a lower bound for one event. The real tick fires from the parallel entity loop, and several callers queue on the one store of the plugin, so the total per tick was not measured. Without a listener the fire site is unchanged (#131).
- Every wave that adds fire sites runs `cargo bench -p pumpkin-world` (`noise_generation`, `full_chunk_generation`) and a criterion bench of event dispatch with zero listeners in `pumpkin-core`. The first event task adds the dispatch bench. A regression outside the bench noise is a must-fix in the wave review.

### Mutators and their events

A mod changes the world through the Pumpkin types that `ironpumpkin-mods` re-exports (`world`, `entity`), and a v0.2 Wasm plugin through the `pumpkin:plugin/world` imports. Each mutator of the table fires the Pumpkin event of the vanilla action before the change, and makes no change when a handler cancels it (spec `neoforge-event-bus`, requirement "One event pipeline"). There is no second path: native mods and v0.2 plugins get the same event from the same fire site. Each fire site calls `PluginManager::fire_blocking_with`, so no event is built when no handler listens.

Cause resolution. A mutator with a cause takes the entity that acts. A player is its own cause. Another entity resolves to its owner through `EntityBase::get_owner_entity` (vanilla `Projectile.getOwner`), so a projectile resolves to its shooter. `BlockBreakEvent` has only a `player` field, so the cause is the owner when the owner is a player, and none otherwise. A block entity has no entity: a machine calls the mutator without a cause, and the event carries no player, as for a piston. This replaces the `FakePlayer` of NeoForge (row 16 of `mapping-table.md`).

| Mutator | Event fired | Cause resolution | Test |
|:--|:--|:--|:--|
| `World::set_block_state`, `World::set_block_state_if` | no event, as vanilla `Level.setBlock` | none | `a_cause_less_set_block_state_fires_no_event` |
| `World::break_block(pos, cause, flags)` | `BlockBreakEvent`; `exp` and `drop` are settable | `cause` is the player | `a_player_breaks_a_block_through_the_world` |
| `World::destroy_block(pos, drop, breaker)` (vanilla `Level.destroyBlock`: the block becomes its fluid, the break effect goes to both editions) | `BlockBreakEvent`; `exp` and `drop` are settable | the breaker, or the player that owns it | `a_projectile_of_a_mod_breaks_a_block`, `destroy_block_takes_the_experience_and_the_drop_of_the_event`, `destroy_block_leaves_the_fluid_of_the_block` |
| `World::place_block(pos, state, placer)` (NeoForge `EventHooks.onBlockPlace`, then `Level.setBlock`) | `BlockPlaceEvent` (NeoForge `BlockEvent.EntityPlaceEvent`) for a player, `BlockEventEntityPlaceEvent` for another entity; no `BlockMultiPlaceEvent`, the call sets one block | the placer, or the entity that owns it; `placed_against` is the block below; a placer that is not in the world and has no owner in it fires nothing | `a_mod_places_a_block_through_the_world` |
| `World::spawn_entity` | `EntitySpawnEvent` (NeoForge `EntityJoinLevelEvent`); `false` when cancelled | no cause field | `a_mod_spawns_damages_and_kills_an_entity` |
| `World::spawn_entity_non_save` | `EntitySpawnEvent` | no cause field | `a_mod_spawns_damages_and_kills_an_entity` |
| `World::spawn_creature(entity, reason, player)` | `CreatureSpawnEvent`, then `EntitySpawnEvent` | `player` | - |
| `World::drop_stack`, `World::drop_stack_from_face` | `ItemSpawnEvent`, then `EntitySpawnEvent` | no cause field | `a_mod_drops_an_item` |
| `World::scatter_stack`, `World::scatter_inventory` | `EntitySpawnEvent` for each stack | no cause field | - |
| `World::strike_lightning` | `LightningStrikeEvent`, then `EntitySpawnEvent` | no cause field | - |
| `World::explode`, `World::explode_with_calculator` | `EntityExplodeEvent` (NeoForge `ExplosionEvent.Start`), then `ExplosionEventDetonate` and an `ExplosionKnockbackEvent` for each pushed entity | none: `World::explode` has no source entity, so `entity_id` is 0 (gap of the row `ExplosionEvent.Start`) | `cancel_explosion_event_start` |
| `EntityBase::damage` and `EntityBase::damage_with_context` of a `LivingEntity` (mobs and players) or an `ItemEntity`, `LivingEntity::damage`, `Player::damage` | `EntityInvulnerabilityCheckEvent` (at the end of `Entity::is_invulnerable_to`), then `EntityDamageEvent`, then `EntityDamageByEntityEvent` or `EntityDamageByBlockEvent`; a non-living entity fires no damage event, as NeoForge posts the damage events for living entities only: an item frame, a painting and an end crystal fire `EntityInvulnerabilityCheckEvent` only, as vanilla reads `isInvulnerableToBase` there, and displays, interactions, markers and eyes of ender take no damage, as in vanilla; gap: the armor stand, a living entity in vanilla, fires `EntityInvulnerabilityCheckEvent` and no damage event, the shulker bullet fires nothing, and a vehicle fires `VehicleDamageEvent` only; a mob or a player then fires `LivingShieldBlockEvent` when it holds up a shield, `LivingKnockBackEvent` for the knockback, `LivingDamageEventPre` before the absorption and `LivingDamageEventPost` after the health changed | the caller passes `source` (the direct entity) and `cause` (its owner); `damager_id` is `source`, else `cause` | `a_mod_spawns_damages_and_kills_an_entity` |
| the damage that kills | `EntityDeathEvent`; `PlayerDeathEvent` for a player; `LivingExperienceDropEvent` before the experience of a mob that a player killed | as the damage | `a_mod_spawns_damages_and_kills_an_entity` |
| `EntityBase::kill`, `Player::kill` | the damage events and `EntityDeathEvent` of a kill damage | none | `a_mod_spawns_damages_and_kills_an_entity` |
| `LivingEntity::heal`, `Player::heal` | `EntityRegainHealthEvent` | none | - |
| `LivingEntity::set_health`, `Player::set_health` | no event, as vanilla `setHealth` | none | - |
| `Entity::remove`, `World::remove_entity` | `EntityLeaveLevelEvent` after the entity left the entity list, as vanilla `onTrackingEnd`; the unload of a chunk and a player that leaves fire it too | none | `one_event_per_fire_site_entity_leave_level_event` |
| `World::add_entity_silent` | no event itself: it is the host path behind the spawn mutators; `EntitySpawnEvent` (NeoForge `EntityJoinLevelEvent`) fires before it for an entity that loads from its chunk and for each rider that a spawned mob brings, and a cancel keeps the entity out | none | `cancel_entity_join_level_event_of_a_rider` |
| `World::add_block_entity`, `World::remove_block_entity`, `World::update_block_entity` | no event, as vanilla `Level.setBlockEntity` | none | - |
| `LivingEntity::set_active_hand` (vanilla `startUsingItem`) | `LivingEntityUseItemEventStart`; `false` when no use starts | none | `cancel_living_entity_use_item_event_start` |
| `Player::release_using_item` (vanilla `releaseUsingItem`) | `LivingEntityUseItemEventStop`; a cancel skips the release of the item; then `ArrowLooseEvent` for a bow, where `charge` is settable and a cancel shoots nothing | none | `cancel_living_entity_use_item_event_stop`, `arrow_loose_event_of_release_using_item` |
| `LivingEntity::add_effect` | `MobEffectEventApplicable`, then `EntityPotionEffectEvent` (NeoForge `MobEffectEvent.Added`) | none: Pumpkin knows no source entity of an effect | `change_a_settable_field_mob_effect_event_applicable` |

A projectile hit is not a mutator: every projectile fires `ProjectileImpactEvent` before it handles a hit, and a cancel lets it fly on; arrows also fire `ProjectileHitEvent`. A mod projectile acts through the mutators above with itself as the cause.

### API shape

- Mods compile from source into the pack binary (the xcaddy model), so the API has no ABI constraint. It may use generic types and methods: `ModInit::register_event<E, H>` is generic. There is no `repr(C)` rule.
- Listener and callback traits are synchronous inside the async host of Pumpkin, like `BlockBehaviour` and `ItemBehaviour`. Where Pumpkin needs async work, the host runs it and the API offers a synchronous call or a completion callback. A handler that needs a world call uses `block_on(unconstrained(..))`, as `ModInit::register_service` does, and never holds a lock across a fire.
- `ironpumpkin-mods` is versioned with semver. Mods depend on `ironpumpkin-mods`, which re-exports the server types a mod needs, and not on the server crates directly. The host side maps the API to Pumpkin internals, so an upstream merge that changes internals changes the host side and not the mods. A breaking API change is a new major version. Source patches are outside this guarantee: they are bound to one commit.

### Licence

The server crates are GPL-3.0. A mod linked into a pack binary is a derivative work of the server, so its licence must be GPL-3.0-compatible, and a distributed pack binary comes with the source of every linked mod. A source patch is a derivative work of the server in the same way. A port is also a derivative of the original Java mod, so that licence must allow the port. `ironpumpkin-mods` stays MIT OR Apache-2.0, like `pumpkin-plugin-api`, and contains no server or NeoForge (LGPL-2.1) source. The Wasm channel has no such constraint: a Wasm plugin talks to the server only through the WIT boundary, as v0.2 plugins do.

### Names and Wasm plugins

- The delta specs and `mapping-table.md` name API items in interface notation, kebab-case (`level-access.destroy-block`), until the task that implements the item lands. That task replaces the notation with the Rust item of `ironpumpkin-mods` (`ModInit::register_block`) or the Pumpkin event (`pumpkin_core::plugin::block::BlockBreakEvent`). A callback that NeoForge takes as a Java interface or lambda is a method of a trait object or a closure that the mod registers.
- The existing v0.2 Wasm plugins reach the new events through the existing Pumpkin event pipeline, in the same dispatch. Per-tick and per-entity events reach Wasm plugins one event per call, behind the same handler check as for native mods, so a world with no Wasm listener pays nothing; the cost of a listener is in the section "Performance guardrail". Source patches are native only.

### Rejected alternatives

- A catalogue of IronPumpkin hook points, generated accessors and service seams: demand is flat (section "Coverage of the reference modpack" above), so the catalogue would be the largest API of the project and still miss most mods. A patch per mod is the honest compile-time equivalent of a mixin, and the events that many patches need enter the API with NeoForge semantics.
- A Java-shaped facade crate that copies the NeoForge event bus, mod bus, registers and holders in Rust: it is a maintenance layer that must follow NeoForge and Pumpkin, and it lies where Pumpkin differs. The Pumpkin events with NeoForge semantics give the same fire sites and fields with no second dispatch.
- A Mojang-shaped `net.minecraft` facade: the owner rejected it. It would be a giant API that nobody can maintain, and the Mojang types of a port map to the Pumpkin types.
- Wasm-only mods: every hook is a cross-task round trip through the per-plugin `StoreExecutor`, and one executor serializes every world that calls the plugin.
- A server rewrite in Go: Go's `plugin` package needs the identical toolchain and dependency sources for host and plugin, the internal ABI is not stable across releases, and Go reflection covers only field and method access.
- An embedded JVM or a NeoForge sidecar: a mod jar needs the whole `net.minecraft` Java API surface, which Pumpkin does not have. The owner dropped the Java feasibility work on 2026-10-08.

## Inventory

### Summary per family

| Family | Rows | Supported | Planned | Client-only | Not supported |
|:--|:--|:--|:--|:--|:--|
| Events, server and common | 283 | 216 | 46 | 1 | 20 |
| Events, client | 189 | 0 | 0 | 189 | 0 |
| Registries | 150 | 37 | 99 | 0 | 14 |
| Capabilities | 35 | 33 | 0 | 0 | 2 |
| Attachments | 26 | 23 | 0 | 0 | 3 |
| Data maps | 24 | 24 | 0 | 0 | 0 |
| Config | 37 | 30 | 0 | 1 | 6 |
| Networking | 88 | 58 | 5 | 0 | 25 |
| Lifecycle and metadata | 82 | 42 | 0 | 10 | 30 |
| Loot and conditions | 25 | 5 | 18 | 0 | 2 |
| Extension interfaces | 373 | 12 | 253 | 27 | 81 |
| Client-only subsystems | 9 | 0 | 0 | 9 | 0 |
| Other server-side packages | 87 | 12 | 34 | 0 | 41 |
| Total | 1408 | 492 | 455 | 237 | 224 |

Method: each family row counts the Status cells of the tables in one section of this inventory. The section "Events" gives two families: the table "Client events" and all other event tables. Every other section gives one family. The Total row is the sum of the family rows. `python3 -I tools/openspec-sync/mapping_status.py` compares the Total row with the Status cells of all inventory tables.

### Events

Event classes found: 471 (93 on the mod bus, 378 on the game bus). Server or common: 282. Client: 189. Abstract classes: 71.

#### `event` (38: 16 supported, 12 planned, 1 client-only, 9 not supported)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| AddAttributeTooltipsEvent | game | both | no | - | not supported | - | tooltip text is built for display; the client renders tooltips from item components |
| AddPackFindersEvent | mod | both | no | - | supported | `NativeMod::add_pack_finders` with `ironpumpkin_mods::registration::AddPackFindersEvent` | fires once after `load_complete` and the payload handler step, as `ResourcePackLoader.populatePackRepository` runs after `ServerModLoader.load`; `add_pack` adds a pack with an id and the files of `include_data_pack!`; the added packs load after the packs of the mods and before the packs of the world; an invalid id, a mod id or a duplicate id stops the start; test `add_pack_finders_collects_the_added_packs_and_rejects_bad_ids` |
| AddServerReloadListenersEvent | game | server | no | - | supported | `AddServerReloadListenersEvent` (`pumpkin_core::plugin::server`) | fired by `plugin::server_events::server_data_loaded` at each load; `add_listener` adds a `ReloadListener` under a key (a duplicate key is an error), `add_dependency` orders two listeners; the vanilla loaders of Pumpkin are not listeners, so every listener runs after them, in registration order, with the loaded packs; a cycle logs an error and runs none of the listeners of that load; native only: a listener is host code; tests `add_server_reload_listeners_event_runs_the_listeners_in_order`, `a_cycle_of_reload_listener_dependencies_fails_the_sort` |
| AnvilUpdateEvent | game | both | yes | - | supported | `PrepareAnvilEvent` | fires on every anvil result update with a non-empty left input (slot click, quick move, rename), after the screen-handler lock is released, with the vanilla result, cost and material cost; `output`, `repair_cost` and `material_cost` apply and a cancel clears the output and the cost, only if the inputs, the name and the result did not change meanwhile; tests `change_a_settable_field_anvil_update_event`, `anvil_update_event_handler_closes_the_screen`, `update_result_without_handler_is_vanilla`; WIT v0.2 |
| BlockEntityTypeAddBlocksEvent | mod | both | no | - | supported | `NativeMod::block_entity_type_add_blocks` with `ironpumpkin_mods::registration::BlockEntityTypeAddBlocksEvent` | fires after `RegisterSpawnPlacementsEvent`; a mod block joins a vanilla block entity type, its states carry that type and placing it creates the block entity; a vanilla block keeps its own type; tests `each_registration_event_fires_once_at_its_neoforge_position`, `a_block_entity_type_accepts_the_mod_block` |
| BuildCreativeModeTabContentsEvent | mod | both | no | - | not supported | - | creative tab contents are client UI; the server never reads them |
| CommandEvent | game | server | yes | - | supported | `CommandEvent` (`pumpkin_core::plugin::server`) | fired by the interceptor of the server dispatcher (`CommandDispatcher::handle_command`, `plugin/server_events.rs`) for each command that a sender performs: a player, the console, a command block, a sign; a function line (`handle_function_command`) fires nothing, as `performCommand` is not on the function path of NeoForge; it fires after the slash is removed and before the parse, with `command` the input of the parse: Pumpkin has no parse result that a handler can keep, so a set `command` replaces it with a new parse; a cancel stops the command, and `exception` sends a failure message to the sender and logs it; `PlayerCommandPreprocessEvent` now fires for a player command before `PlayerCommandSendEvent` (NeoForge has no event there), and `ServerCommandEvent` keeps firing first for the console; tests `cancel_command_event`, `replace_the_command_of_a_command_event`, `command_event_from_a_player_with_an_exception`, `a_function_line_fires_no_command_event`, `player_command_preprocess_event_replaces_and_cancels`; WIT v0.2, tests `a_plugin_replaces_the_command_and_cancels_with_an_exception`, `the_console_sender_of_a_command_has_no_player` |
| DefaultDataComponentsBoundEvent | game | both | no | - | planned | engine extension point (M5) | M5 engine extension: default item components come from generated pumpkin-data and are never rebound |
| DifficultyChangeEvent | game | both | no | - | client-only | - | NeoForge 26.3 posts it only from `ClientLevel.ClientLevelData.setDifficulty` (`CommonHooks.onDifficultyChange` has no server caller), so a dedicated server never fires it |
| GameShuttingDownEvent | game | both | no | - | supported | `GameShuttingDownEvent` (`pumpkin_core::plugin::server`) | fired once in `PumpkinServer::run` after the ticker stopped, before `ServerStoppingEvent` and before the plugins and mods unload, as `DedicatedServer.stopServer` posts it before `MinecraftServer.stopServer`; tests `neoforge_lifecycle_events` and `second_server` (`crates/ironpumpkin-test-mod/tests`); WIT v0.2, test `a_plugin_receives_the_tags_update_and_the_shutdown` |
| GatherSkippedAttributeTooltipsEvent | game | both | no | - | not supported | - | tooltip display only, see AddAttributeTooltipsEvent |
| GrindstoneEvent (abstract) | game | both | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported |
| GrindstoneEvent.OnPlaceItem | game | both | yes | - | supported | `PrepareGrindstoneEvent` | fires on every grindstone result update (input change or take), after the screen-handler lock is released; `result_item` is the id of the vanilla result; a set `output` replaces the result and `xp` applies when it is 0 or more; a cancel clears the result; the result applies only if the inputs did not change meanwhile; tests `one_event_per_fire_site_grindstone_event_on_place_item`, `update_result_without_handler_is_vanilla`; WIT v0.2 |
| GrindstoneEvent.OnTakeItem | game | both | yes | - | supported | `GrindstoneEventOnTakeItem` | fired on every take of the grindstone result, after the screen-handler lock is released (`plugin/container_events.rs`); `xp` is the amount of a place handler or the vanilla amount; the player keeps the result; a cancel gives no experience, puts the inputs back and leaves the result empty; otherwise the grindstone gives `xp` and its inputs become `new_top` and `new_bottom`; with no known grindstone position, `pos` is the zero position and no orbs or sound spawn; a stack that cannot go back into a closed grindstone (a closed flag set by `on_closed`, read under the screen-handler lock, so a close on another thread during the event counts) or onto new inputs goes to the player; WIT v0.2, test `a_plugin_changes_the_inputs_and_the_experience_of_a_grindstone_take` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_grindstone_event_on_take_item` |
| ItemAttributeModifierEvent | game | both | no | - | planned | attribute modifiers data component | needs a hook where Pumpkin reads item attribute modifiers |
| ItemStackedOnOtherEvent | game | both | yes | - | planned | slot click handling (pumpkin-inventory) | no event for stacking one item onto another in a slot |
| LootTableLoadEvent | game | both | yes | - | supported | `LootTableLoadEvent` (`pumpkin_core::plugin::server`) | fired by `plugin::server_events::server_data_loaded` for each loot table that the data packs of the mods and of the world loaded, at the first load and after each reload, before the data maps load; the compiled vanilla tables that no pack replaces fire it too, built from `pumpkin_data::loot_table::all_loot_tables()` only when a handler listens, and a changed or cancelled vanilla table is stored as a loaded table that `DatapackManager::get_loot_table` prefers; a set `table` (a new `Arc`) replaces the table, a cancel loads an empty table; tests `loot_table_load_event_replaces_and_cancels`, `loot_table_load_event_changes_a_vanilla_table`; WIT v0.2 has `name` and `cancelled` only, test `a_plugin_cancels_a_loot_table_load_and_the_host_keeps_the_table` |
| ModMismatchEvent | mod | both | no | - | not supported | - | needs per-world mod version records, which Pumpkin does not keep |
| ModifyDefaultComponentsEvent | mod | both | no | - | planned | engine extension point (M5) | M5 engine extension: vanilla default components are generated constants; phase 2 adds components only for new items |
| OnDatapackSyncEvent | game | server | no | - | supported | `OnDatapackSyncEvent` (`pumpkin_core::plugin::server`) | fired in `World::spawn_java_player` for the joining player, before the data maps go out, and in `Server::reload_datapacks` with no player for all players; `relevant_players` gives the players to sync; gap: `sendRecipes` is not supported, the recipe content payload has no sender; tests `tags_updated_and_datapack_sync_events_after_a_reload`, `datapack_sync_event_of_a_join`; WIT v0.2, test `a_plugin_receives_the_player_of_a_datapack_sync` |
| PlayLevelSoundEvent | game | both | yes | - | planned | `World::play_sound` | concrete parent of AtEntity and AtPosition |
| PlayLevelSoundEvent.AtEntity | game | both | yes | - | planned | `World::play_sound` | no sound event; Pumpkin sends sound packets directly |
| PlayLevelSoundEvent.AtPosition | game | both | yes | - | planned | `World::play_sound` | no sound event; Pumpkin sends sound packets directly |
| RegisterCauldronInteractionEvent (abstract) | mod | both | no | - | planned | shared fields of 2 subclasses | M5 engine extension: abstract: the bus rejects listeners on abstract events; subclasses: 2 planned |
| RegisterCauldronInteractionEvent.Dispatcher | mod | both | no | - | planned | engine extension point (M5) | M5 engine extension: cauldron interactions are hard-coded in the cauldron BlockBehaviour |
| RegisterCauldronInteractionEvent.Interaction | mod | both | no | - | planned | engine extension point (M5) | M5 engine extension: cauldron interactions are hard-coded in the cauldron BlockBehaviour |
| RegisterCommandsEvent | game | server | no | - | supported | `ModInit::register_command` (API, no event) | API call instead of an event; commands are not rebuilt on reload; test `one_event_per_fire_site_register_commands_event` |
| RegisterGameRuleCategoryEvent | mod | both | no | - | not supported | - | game rule categories group rules in the client UI only |
| RegisterGameTestsEvent | mod | both | no | - | not supported | - | NeoForge game tests run inside the Java server; pumpkin-gametest is a separate harness |
| RegisterRecipePropertiesEvent | mod | both | no | - | not supported | - | recipe property sets feed client recipe displays; Pumpkin derives them from generated data |
| RegisterStructureConversionsEvent | game | both | no | - | not supported | - | converts pre-1.18.2 structure ids; Pumpkin does not upgrade old worlds |
| RegisterTooltipAppendersEvent | mod | both | no | - | not supported | - | tooltip display only |
| ServerChatEvent | game | server | yes | - | supported | `PlayerChatEvent` | fired in `net/bedrock/play/chat_message.rs`, `net/java/play/chat_message.rs`; `message` is plain text, not a component; `raw_text` is filled only when a chat handler was registered when the event is created; test `cancel_server_chat_event`; WIT v0.2 |
| SortedReloadListenerEvent (abstract) | game | both | no | - | planned | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 1 planned, 1 client-only |
| StatAwardEvent | game | both | yes | - | supported | `PlayerStatisticIncrementEvent` | fired by `increment_stat` and `set_stat` (`plugin/stat_events.rs`), as `StatsCounter.setValue`, with `value` the new total, resets to 0 included; the server stores `value` in the stat that `category` and `stat` name after the handlers, and a cancel stores nothing; `amount` is the change; a Wasm handler that changes `amount` moves `value` by the same step; gap: the stats lock is not held during the event, so the stored `value` overwrites a change to the same statistic that another thread made while the event fired; test `one_event_per_fire_site_stat_award_event`; WIT v0.2 |
| TagsUpdatedEvent | game | both | no | - | supported | `TagsUpdatedEvent` (`pumpkin_core::plugin::server`) | concrete parent that the server never fires: a `ParentEventHandler` gets `TagsUpdatedEventServerDataLoad`; test `tags_updated_and_datapack_sync_events_after_a_reload` |
| TagsUpdatedEvent.ServerDataLoad | game | server | no | - | supported | `TagsUpdatedEventServerDataLoad` | fired by `plugin::server_events::server_data_loaded` after the first data pack load and after each reload, after the data maps load (`DataMapHost::load`, which fires `DataMapsUpdatedEvent`, as NeoForge loads them in its own handler of this event) and after the loot table and reload listener events, so a handler sees the new data maps; the data maps go out to the players after `OnDatapackSyncEvent`; it has no fields: Pumpkin has no registry access or server resources object; tests `tags_updated_and_datapack_sync_events_after_a_reload`, `the_data_maps_load_before_the_tags_and_the_datapack_sync_events`; WIT v0.2 without a payload, test `a_plugin_receives_the_tags_update_and_the_shutdown` |
| VanillaGameEvent | game | both | yes | - | planned | `GenericGameEvent` | Pumpkin has no vanilla game event dispatch: `World::emit_game_event` fires it from 10 call sites for 8 of the 61 vanilla game events (`prime_fuse` of TNT, `shriek`, `entity_place` of the dispenser, `block_activate` and `block_deactivate` of the lever, `block_change` of the daylight detector, `teleport` of a living entity, `entity_damage` of an item entity) and the datapack events of 3 enchantment effects, and no vibration listener receives a game event, so a cancel stops nothing; missing sites: `block_place`, `block_destroy`, `step`, `hit_ground`, `entity_die`, `entity_damage` of living entities, `entity_action`, `container_open`, `container_close`, `block_open`, `block_close`, `block_attach`, `block_detach`, `projectile_shoot`, `projectile_land`, `eat`, `drink`, `explode`, `fluid_pickup`, `fluid_place`, `equip`, `unequip`, `swim`, `splash`, `flap`, `bounce`, `elytra_glide`, `entity_mount`, `entity_dismount`, `entity_interact`, `item_interact_start`, `item_interact_finish`, `jukebox_play`, `jukebox_stop_play`, `note_block_play`, `instrument_play`, `lightning_strike`, `shear`, `sculk_sensor_tendrils_clicking` and `resonate_1` to `resonate_15`; test `one_event_per_fire_site_vanilla_game_event`; WIT v0.2 |

#### `event.brewing` (4: 3 supported, 1 planned)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| PlayerBrewedPotionEvent | game | both | no | - | planned | brewing stand screen handler | no event when the potion is taken |
| PotionBrewEvent (abstract) | game | both | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported |
| PotionBrewEvent.Post | game | both | no | - | supported | `PotionBrewEventPost` | fired in `plugin/brewing_events.rs` after the vanilla brew and after a cancelled `PotionBrewEvent.Pre`; `stacks` is a copy of the stand inventory, and the stand takes changed stacks when its inventory did not change meanwhile, as NeoForge hands out the live stacks; after the vanilla brew it fires before the ingredient is consumed, as NeoForge fires before the shrink, and the stand consumes one ingredient after the handlers; gap: it fires after every cancelled Pre (NeoForge skips it when the Pre handler changed all five stacks); WIT v0.2, test `a_plugin_changes_the_stacks_of_a_brew_and_reads_an_enchant` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_potion_brew_event_post` |
| PotionBrewEvent.Pre | game | both | yes | - | supported | `BrewEvent` | fired in `plugin/brewing_events.rs`; a cancel applies `stacks` only if the stand inventory did not change meanwhile, then `PotionBrewEvent.Post` fires; test `one_event_per_fire_site_potion_brew_event_pre`; WIT v0.2 |

#### `event.enchanting` (4: 1 supported, 3 planned)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| EnchantedBlockLootEvent | game | server | no | - | planned | loot table evaluation (`data/datapack/loot_table_loader.rs`) | no hook on enchantment level queries |
| EnchantedEntityLootEvent | game | both | no | - | planned | loot table evaluation (`data/datapack/loot_table_loader.rs`) | no hook on enchantment level queries |
| EnchantmentLevelSetEvent | game | both | no | - | supported | `PrepareItemEnchantEvent` | fired once for the three rows, after the clues are chosen, after the enchanting-table screen-handler lock is released (`plugin/container_events.rs`); the rows apply only if the item and the rows did not change meanwhile, and when the rows changed while the event fired it fires once more for the new rows; `pos` is the open container position (the zero position with no container) and `original_level_requirements()` returns the vanilla levels; tests `one_event_per_fire_site_enchantment_events`, `enchanting_events_handler_closes_the_screen`; WIT v0.2 |
| GetEnchantmentLevelEvent | game | both | no | - | planned | enchantment level lookup on `ItemStack` | no hook |

#### `event.entity` (21: 17 supported, 4 planned)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| EntityAttributeCreationEvent | mod | both | no | - | supported | `NativeMod::entity_attribute_creation` with `ironpumpkin_mods::registration::EntityAttributeCreationEvent` | fires after the last `RegisterEvent`; `put` sets the default attributes of a mod entity type and rejects a type that has them; tests `each_registration_event_fires_once_at_its_neoforge_position`, `a_spawned_mod_entity_has_the_attributes_of_the_creation_event` |
| EntityAttributeModificationEvent | mod | both | no | - | supported | `NativeMod::entity_attribute_modification` with `ironpumpkin_mods::registration::EntityAttributeModificationEvent` | fires after `EntityAttributeCreationEvent`; `add` adds or replaces an attribute of any type, vanilla ones included; test `the_modification_event_adds_an_attribute_to_a_vanilla_entity` |
| EntityEvent (abstract) | game | both | no | - | planned | shared fields of 122 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 104 supported, 11 planned, 5 not supported, 2 client-only |
| EntityEvent.EnteringSection | game | both | no | - | planned | entity section tracking in `World` | no event |
| EntityEvent.EntityConstructing | game | both | no | - | planned | attachments (phase 6) on `Entity` | mods use it to attach data at construction |
| EntityEvent.Size | game | both | no | - | planned | engine extension point (M5) | M5 engine extension: entity dimensions come from generated pumpkin-data; no dynamic size hook |
| EntityInvulnerabilityCheckEvent | game | both | no | - | supported | `EntityInvulnerabilityCheckEvent` | fired in `entity/mod.rs` at the end of `Entity::is_invulnerable_to` (vanilla `isInvulnerableToBase`), which the damage of living entities, item entities, armor stands, item frames (fixed ones too), paintings and end crystals reads; the invulnerable flag of an end crystal is the base flag of the entity, as in vanilla; `invulnerable` is settable and starts at the Pumpkin result, which also counts the damage immunities of plugins; a removed entity returns before the event, where NeoForge fires it with the vanilla result `true`; `damage_type` and `attacker` stand for the `DamageSource`; tests `change_a_settable_field_entity_invulnerability_check_event`, `an_item_frame_fires_the_invulnerability_check`, `an_invulnerable_painting_does_not_break`, `fire_does_not_destroy_an_end_crystal`; WIT v0.2 |
| EntityJoinLevelEvent | game | both | yes | - | supported | `EntitySpawnEvent` | fired in `world/mod.rs` for a spawn, for each rider that a spawned mob brings (vanilla `addFreshEntityWithPassengers`) and for an entity that loads from its chunk; `loaded_from_disk` (NeoForge `loadedFromDisk`) is true only for the chunk load (`World::load_chunk_entity`); a cancel keeps the entity out of the world, and a cancelled loaded entity is gone with the next save of its chunk, as on NeoForge; tests `cancel_entity_join_level_event_of_a_rider`, `one_event_per_fire_site_entity_join_level_event_on_load`, `cancel_entity_join_level_event_on_load`; WIT v0.2; gap: the upstream v0.2 record `entity-spawn-event-data` has no `loaded-from-disk`, so a Wasm handler cannot tell a load from a spawn, and the host keeps the native flag; CreatureSpawnEvent carries the spawn reason |
| EntityLeaveLevelEvent | game | both | no | - | supported | `EntityLeaveLevelEvent` | fired in `world/mod.rs` after an entity left the entity list: `World::remove_entity` (a kill, a discard, a despawn), the unload of its chunk, and a player that leaves; Pumpkin's `EntityRemoveEvent` still fires nowhere; the dragon fight removes the summoning crystals after it released its lock; test `one_event_per_fire_site_entity_leave_level_event` (removal, chunk unload, player leave); WIT v0.2 |
| EntityMobGriefingEvent | game | server | no | - | supported | `EntityMobGriefingEvent` | fired by `entity_events::can_entity_grief` where vanilla reads `mobGriefing` for an entity and NeoForge posts `canEntityGrief`: the turtle egg trample, the enderman block pick-up and place goals, the remove block goal, the door break goal, the sheep grass eating, the wither block breaking, and the piglin item pick-up and wanted item search (Pumpkin checks `can_pick_up_loot` before the event there, vanilla after it); `can_grief` is settable; gap: the mob explosion interaction fires nothing, because `World::explode` has no source entity, and the other NeoForge sites (fox, rabbit, snow golem, ravager, evoker, silverfish, allay, villager farming, farmland trample, small fireball, projectile `mayInteract`, wither rose) have no `mobGriefing` check in Pumpkin; test `change_a_settable_field_entity_mob_griefing_event`; WIT v0.2 |
| EntityMountEvent | game | both | yes | - | supported | `EntityMountEvent` | fired in `entity/mod.rs`; WIT v0.2; EntityDismountEvent for dismounting |
| EntityStruckByLightningEvent | game | both | yes | - | supported | `EntityStruckByLightningEvent` | fired in `entity/lightning.rs` before `on_lightning_strike` of each struck entity, outside the lock of the struck set; a cancel spares the entity from the bolt; Pumpkin strikes an entity once per bolt, vanilla on every tick of the bolt; test `cancel_entity_struck_by_lightning_event`; WIT v0.2 |
| EntityTeleportEvent | game | server | yes | - | supported | `EntityTeleportEvent` | fired in `entity/mob/enderman.rs`; WIT v0.2; fired from the enderman only |
| EntityTeleportEvent.EnderEntity | game | both | yes | - | supported | `EntityTeleportEvent` | fired in `entity/mob/enderman.rs`; WIT v0.2; fired from the enderman only; NeoForge also covers the shulker |
| EntityTeleportEvent.EnderPearl | game | server | yes | - | supported | `PlayerTeleportEvent` | fired in `entity/player.rs`; WIT v0.2 |
| EntityTeleportEvent.ItemConsumption | game | both | yes | - | supported | `PlayerTeleportEvent` | fired in `entity/player.rs`; WIT v0.2; chorus fruit |
| EntityTeleportEvent.SpreadPlayersCommand | game | server | yes | - | supported | `EntityTeleportEventSpreadPlayersCommand` | fired in `command/commands/spreadplayers.rs` before each teleport, for players and other entities; `target` is settable; a cancel leaves the entity where it is; a handler of Pumpkin's `EntityTeleportEvent` does not get it; tests `change_a_settable_field_entity_teleport_event_spread_players_command`, `cancel_entity_teleport_event_spread_players_command`; WIT v0.2 |
| EntityTeleportEvent.TeleportCommand | game | server | yes | - | supported | `PlayerTeleportEvent` | fired in `entity/player.rs`; WIT v0.2; EntityTeleportEvent for non-players |
| EntityTravelToDimensionEvent | game | both | yes | - | supported | `EntityPortalEvent` | fired in `entity/mod.rs`; WIT v0.2; PlayerPortalEvent for players |
| ProjectileImpactEvent | game | both | yes | - | supported | `ProjectileImpactEvent` | fired before a projectile handles a hit: the shared tick of thrown projectiles in `entity/projectile/mod.rs` (snowballs, eggs, ender pearls, potions, experience bottles, fireballs, wither skulls, wind charges, llama spit, firework rockets), arrows, tridents, shulker bullets and the entity hook of a fishing bobber; `ray` is the block hit (position, face, point) or the entity hit; a cancel skips the hit and the projectile flies on, as the event class says (the 26.3 patch of `ThrowableProjectile` keeps the hit); gap: the block stop of a fishing bobber fires nothing; on a cancel, a shulker bullet and a fishing bobber fire the event once for each entity they overlap in the tick, where vanilla has one hit result per tick, and a shulker bullet still checks the entities after a cancelled block hit; arrows also fire `ProjectileHitEvent`; tests `cancel_projectile_impact_event`, `cancel_projectile_impact_event_of_an_arrow`; WIT v0.2 |
| RegisterSpawnPlacementsEvent | mod | both | no | - | supported | `NativeMod::register_spawn_placements` with `ironpumpkin_mods::registration::RegisterSpawnPlacementsEvent`; `check_spawn_rules` and the natural spawner read the merged placement | fires after `EntityAttributeModificationEvent`; placement type, heightmap and predicate with `And`, `Or` and `Replace` as `MergedSpawnPredicate`; the predicate gets no spawn reason or random source; test `a_spawn_placement_rejects_a_position` |

#### `event.entity.item` (3: 3 supported)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| ItemEvent (abstract) | game | both | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported |
| ItemExpireEvent | game | both | no | - | supported | `ItemDespawnEvent` | fired in `entity/item.rs`; WIT v0.2 |
| ItemTossEvent | game | both | yes | - | supported | `PlayerDropItemEvent` | fired in `entity/player.rs`; WIT v0.2 |

#### `event.entity.living` (48: 46 supported, 2 planned)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| AnimalTameEvent | game | both | yes | - | supported | `EntityTameEvent` | fired in `entity/mob/mod.rs`; WIT v0.2 |
| ArmorHurtEvent | game | both | yes | - | supported | `PlayerItemDamageEvent` | fired in `entity/player.rs`; WIT v0.2; players only |
| BabyEntitySpawnEvent | game | both | yes | - | supported | `EntityBreedEvent` | fired in `entity/mob/mod.rs`; WIT v0.2 |
| EffectParticleModificationEvent | game | both | no | - | supported | `EffectParticleModificationEvent` (`pumpkin_core::plugin::living`) | fired for each active effect in `LivingEntity::sync_effect_particles` (vanilla `updateSynchronizedMobEffectParticles`), outside the effect lock; `visible` and `particle_color` apply; gap: Pumpkin sends the particle `minecraft:entity_effect` only, so the particle options are its ARGB colour; WIT v0.2, test `a_plugin_hides_an_effect_particle_and_refuses_an_effect` (`crates/pumpkin-wasm-host-v0_2/tests`); test `change_a_settable_field_effect_particle_modification_event` |
| EndermanAngerEvent | game | both | yes | - | supported | `EndermanAngerEvent` | fired in `entity/mob/enderman.rs` at the end of `is_player_staring` (vanilla `isBeingStaredBy`) when the stare and the line of sight hold; a cancel makes the stare not count, for the chase and the teleport goals; test `cancel_enderman_anger_event`; WIT v0.2 |
| FinalizeSpawnEvent | game | server | yes | - | supported | `CreatureSpawnEvent` | fired in `world/mod.rs`; WIT v0.2 |
| LivingBreatheEvent | game | both | no | - | supported | `EntityAirChangeEvent` | fired in `entity/breath.rs`; WIT v0.2 |
| LivingChangeTargetEvent | game | both | yes | - | supported | `EntityTargetEvent` | fired in `entity/mob/mod.rs`; WIT v0.2; EntityTargetLivingEntityEvent for living targets |
| LivingConversionEvent (abstract) | game | both | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported |
| LivingConversionEvent.Post | game | both | no | - | supported | `LivingConversionEventPost` (`pumpkin_core::plugin::living`) | fired before the new mob joins the world in the conversions of a hoglin to a zoglin and of a piglin and a piglin brute to a zombified piglin; gap: the other vanilla conversions (zombie villager, drowned, stray, witch, mooshroom, tadpole and the others) are not in Pumpkin; WIT v0.2, test `a_plugin_cancels_a_destroy_block_and_receives_a_conversion` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `one_event_per_fire_site_living_conversion_event_post`, `one_event_per_fire_site_living_conversion_event_post_of_a_piglin` |
| LivingConversionEvent.Pre | game | both | yes | - | supported | `EntityTransformEvent` | fired in `entity/mob/mod.rs`; WIT v0.2 |
| LivingDamageEvent (abstract) | game | both | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported |
| LivingDamageEvent.Post | game | both | no | - | supported | `LivingDamageEventPost` (`pumpkin_core::plugin::living`) | fired at the end of `LivingEntity::damage_with_context` after the health changed, also after the death; `source` (damage type, direct and causing entity, position), the inflicted and health damage, the shield damage, the hurt cooldown and the reductions; gap: `magic` holds `ENCHANTMENTS` and `MOB_EFFECTS` together, a shield that blocks the whole damage ends the hurt before the event (NeoForge fires it with the blocked damage), and only a mob or a player fires it (see the row of `EntityBase::damage` in "Mutators and their events"); WIT v0.2, test `a_plugin_changes_the_damage_of_a_living_damage_pre_and_reads_the_post` (`crates/pumpkin-wasm-host-v0_2/tests`); test `change_a_settable_field_living_damage_event_pre` |
| LivingDamageEvent.Pre | game | both | no | - | supported | `LivingDamageEventPre` (`pumpkin_core::plugin::living`) | fired in `LivingEntity::damage_with_context` after the armor, the effects and the hurt cooldown and before the absorption; `new_damage` applies, and a damage that is not finite keeps the damage before the event; gap: no reduction modifiers of `DamageContainer`, and a shield that blocks the whole damage ends the hurt before the event (NeoForge fires it with 0 outside the hurt cooldown); WIT v0.2, test `a_plugin_changes_the_damage_of_a_living_damage_pre_and_reads_the_post` (`crates/pumpkin-wasm-host-v0_2/tests`); test `change_a_settable_field_living_damage_event_pre` |
| LivingDeathEvent | game | both | yes | - | supported | `EntityDeathEvent` | fired in `entity/living.rs`; WIT v0.2; PlayerDeathEvent for players |
| LivingDestroyBlockEvent | game | both | yes | - | supported | `LivingDestroyBlockEvent` (`pumpkin_core::plugin::living`) | fired for each block that a wither breaks around it and in `BreakDoorGoal::can_start` (NeoForge `CommonHooks.canEntityDestroy`); a cancel keeps the block; gap: the ender dragon breaks no blocks in Pumpkin; WIT v0.2, test `a_plugin_cancels_a_destroy_block_and_receives_a_conversion` (`crates/pumpkin-wasm-host-v0_2/tests`); test `cancel_living_destroy_block_event` |
| LivingDropsEvent | game | both | yes | - | supported | `EntityDeathEvent` | fired in `entity/living.rs`; WIT v0.2; drop list on the death event |
| LivingDrownEvent | game | both | yes | - | supported | `LivingDrownEvent` (`pumpkin_core::plugin::living`) | fired in each tick that a player is out of air under water (`BreathManager::tick`); `drowning` is `true` in the tick of the drowning damage; `drowning`, `damage_amount` and `bubble_count` apply, and a cancel deals no damage; gap: Pumpkin ticks the air of players only, and `bubble_count` has no effect on the server, as in vanilla; WIT v0.2, test `a_plugin_changes_a_knockback_a_fall_and_a_drown` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `change_a_settable_field_living_drown_event`, `cancel_living_drown_event` |
| LivingEntityUseItemEvent | game | both | no | - | supported | `LivingEntityUseItemEvent` (`pumpkin_core::plugin::living`) | concrete parent: a `ParentEventHandler` gets `LivingEntityUseItemEventStart`, `Tick` and `Stop`; gap: `Finish` is `PlayerItemConsumeEvent`, which does not reach the parent; test `cancel_living_entity_use_item_event_start` |
| LivingEntityUseItemEvent.Finish | game | both | no | - | supported | `PlayerItemConsumeEvent` | fired in `net/java/play/use_item.rs`; WIT v0.2; players only |
| LivingEntityUseItemEvent.Start | game | both | yes | - | supported | `LivingEntityUseItemEventStart` (`pumpkin_core::plugin::living`) | fired in `LivingEntity::set_active_hand` (vanilla `startUsingItem`) for players and mobs, only when no use runs, as vanilla starts only then (a brush calls `set_active_hand` again to refresh its use); `duration` applies, and a cancel or a negative duration starts no use; WIT v0.2, test `a_plugin_sets_the_durations_of_the_item_use_events_and_cancels_them` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `cancel_living_entity_use_item_event_start`, `one_event_per_fire_site_living_entity_use_item_event_start` |
| LivingEntityUseItemEvent.Stop | game | both | yes | - | supported | `LivingEntityUseItemEventStop` (`pumpkin_core::plugin::living`) | fired in `Player::release_using_item` (vanilla `releaseUsingItem`) when the Java or Bedrock client releases the item; a cancel skips the release of the item, so a bow shoots nothing, and the use stops; WIT v0.2, test `a_plugin_sets_the_durations_of_the_item_use_events_and_cancels_them` (`crates/pumpkin-wasm-host-v0_2/tests`); test `cancel_living_entity_use_item_event_stop` |
| LivingEntityUseItemEvent.Tick | game | both | yes | - | supported | `LivingEntityUseItemEventTick` (`pumpkin_core::plugin::living`) | fired in the living tick before the item ticks; `duration` becomes the remaining use ticks and a cancel sets them to -1; with no remaining tick a consumable finishes in that tick and a crossbow keeps its use, as vanilla keeps the use of a `useOnRelease` item; gap: another item stops being used, because the server finishes only consumables, and the event no longer fires for a crossbow with no remaining tick; WIT v0.2, test `a_plugin_sets_the_durations_of_the_item_use_events_and_cancels_them` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `change_a_settable_field_living_entity_use_item_event_tick`, `cancel_living_entity_use_item_event_tick`, `living_entity_use_item_event_tick_of_zero_on_other_items` |
| LivingEquipmentChangeEvent | game | both | no | - | planned | equipment slots in `LivingEntity` | Pumpkin has no equipment diff in the living tick (vanilla `collectEquipmentChanges`): 35 call sites send a change through `send_equipment_changes` without the previous stack |
| LivingEvent (abstract) | game | both | no | - | planned | shared fields of 98 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 83 supported, 8 planned, 5 not supported, 2 client-only |
| LivingEvent.LivingJumpEvent | game | both | no | - | supported | `LivingEventLivingJumpEvent` (`pumpkin_core::plugin::living`) | fired after a mob jumps in `LivingEntity::jump` (vanilla `jumpFromGround`) and in `Player::jump`, which the server calls for the jump that the movement packets show; gap: the client already moved the player, so a handler cannot change the jump, and the jumps of horses, camels and slimes are not in Pumpkin; WIT v0.2, test `a_plugin_receives_a_jump_and_changes_a_visibility` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `one_event_per_fire_site_living_jump_event`, `one_event_per_fire_site_living_jump_event_of_a_mob` |
| LivingEvent.LivingVisibilityEvent | game | both | no | - | supported | `LivingEventLivingVisibilityEvent` (`pumpkin_core::plugin::living`) | fired in `LivingEntity::visibility_percent` (vanilla `getVisibilityPercent`) for the target selection of mobs and piglins; `visibility_modifier` applies, at least 0 and at most 10; WIT v0.2, test `a_plugin_receives_a_jump_and_changes_a_visibility` (`crates/pumpkin-wasm-host-v0_2/tests`); test `change_a_settable_field_living_visibility_event` |
| LivingExperienceDropEvent | game | both | yes | - | supported | `LivingExperienceDropEvent` (`pumpkin_core::plugin::living`) | fired in `LivingEntity::on_death` before the experience drop of a mob that a player killed; `dropped_experience` applies, and a cancel drops none; `attacking_player` is the killer or the player that hurt the entity last; gap: the ender dragon drops no experience in Pumpkin, and a player fires nothing, because Pumpkin drops experience only for a mob that a player killed; WIT v0.2, test `a_plugin_cancels_an_experience_drop_and_replaces_a_projectile` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `cancel_living_experience_drop_event`, `change_a_settable_field_living_experience_drop_event` |
| LivingFallEvent | game | both | yes | - | supported | `LivingFallEvent` (`pumpkin_core::plugin::living`) | fired in `LivingEntity::handle_fall_damage` after the flight check and the fall statistic of a player; `distance` and `damage_multiplier` apply, and a cancel deals no fall damage; gap: the fall distance of a player comes from its client, and an entity immune to fall damage fires nothing; WIT v0.2, test `a_plugin_changes_a_knockback_a_fall_and_a_drown` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `change_a_settable_field_living_fall_event`, `cancel_living_fall_event` |
| LivingGetProjectileEvent | game | both | no | - | supported | `LivingGetProjectileEvent` (`pumpkin_core::plugin::living`) | fired by `living_events::player_projectile` (vanilla `Player.getProjectile`) for the use and the release of a bow and the use and the load of a crossbow, and for the bow and crossbow attacks of mobs (`Monster.getProjectile`); `projectile` applies; gap: a projectile that a handler sets is a copy, so the server takes nothing from the inventory for it; WIT v0.2, test `a_plugin_cancels_an_experience_drop_and_replaces_a_projectile` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `change_a_settable_field_living_get_projectile_event`, `change_a_settable_field_living_get_projectile_event_of_a_crossbow` |
| LivingHealEvent | game | both | yes | - | supported | `EntityRegainHealthEvent` | fired at the start of `LivingEntity::heal`, for every call, also with an amount of 0 and for the dead; `amount` is settable and a value that is not finite keeps the amount before the event; a cancel heals nothing; then an amount that is not above 0 heals nothing (`if (heal <= 0) return`), so the negative amount of an overflowing instant health amplifier takes no health, unlike vanilla; WIT v0.2; tests `change_a_settable_field_living_heal_event`, `living_heal_event_fires_for_every_heal_and_heals_only_a_positive_amount` |
| LivingIncomingDamageEvent | game | both | yes | - | supported | `EntityDamageEvent` | fired in `entity/item.rs`, `entity/living.rs`; WIT v0.2; EntityDamageByEntityEvent and EntityDamageByBlockEvent carry the source |
| LivingKnockBackEvent | game | both | yes | - | supported | `LivingKnockBackEvent` (`pumpkin_core::plugin::living`) | fired before the knockback resistance for the knockback of a damage and of a player attack (`combat::handle_knockback`); a mob that replaces the knockback (`Mob::mob_knockback`, the sulfur cube with a body item) fires no event, as vanilla `SulfurCube.knockback` does not call `super.knockback`, where NeoForge posts the event; `strength`, `ratio_x` and `ratio_z` apply, and a negative or not finite strength or a ratio that is not finite keeps the values before the event; a cancel gives no knockback; upstream `EntityKnockbackEvent` stays unfired; gap: Pumpkin knocks back before the health changes, where vanilla runs `actuallyHurt` first; WIT v0.2, test `a_plugin_changes_a_knockback_a_fall_and_a_drown` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `cancel_living_knock_back_event`, `change_a_settable_field_living_knock_back_event`, `a_plugin_that_writes_back_a_nan_knockback_keeps_the_vanilla_knockback` (`crates/pumpkin-wasm-host-v0_2/tests`) |
| LivingShieldBlockEvent | game | both | no | - | supported | `LivingShieldBlockEvent` (`pumpkin_core::plugin::living`) | fired in `LivingEntity::damage_with_context` for a damage to an entity that holds up a shield, also for a damage that bypasses the shield (`original_blocked` is `false`, the blocked damage follows the facing of the shield), not for a damage of 0; `blocked`, `blocked_damage` and `shield_damage` apply, and the rest of a partly blocked damage goes on; gap: it also fires for an arrow with piercing, where vanilla blocks nothing before the event; WIT v0.2, test `a_plugin_unblocks_a_shield_and_sets_its_durability_loss` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `change_a_settable_field_living_shield_block_event`, `change_a_settable_field_living_shield_block_event_partly` |
| LivingSwapItemsEvent (abstract) | game | both | no | - | supported | shared fields of 1 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 1 supported |
| LivingSwapItemsEvent.Hands | game | both | yes | - | supported | `PlayerSwapHandItemsEvent` | fired in `entity/player.rs`; WIT v0.2; players only |
| LivingUseTotemEvent | game | both | yes | - | supported | `EntityResurrectEvent` | fired in `entity/living.rs`; WIT v0.2 |
| MobDespawnEvent | game | server | no | setResult(Result) | supported | `MobDespawnEvent` | fired at the start of `MobEntity::check_despawn`, once per mob and tick, also for a persistent mob; `result` is settable: `Allow` removes the mob, `Deny` skips the check and resets `no_action_time` (`mob.setNoActionTime(0)`), `Default` runs it; tests `change_a_settable_field_mob_despawn_event`, `deny_mob_despawn_event`; WIT v0.2 |
| MobEffectEvent (abstract) | game | both | no | - | supported | shared fields of 4 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 4 supported |
| MobEffectEvent.Added | game | both | no | - | supported | `EntityPotionEffectEvent` | fired in `entity/living.rs`; WIT v0.2; action added |
| MobEffectEvent.Applicable | game | both | no | setResult(Result) | supported | `MobEffectEventApplicable` (`pumpkin_core::plugin::living`) | fired in `LivingEntity::add_effect` before `MobEffectEvent.Added`; `result` applies; gap: Pumpkin knows no source entity of an effect, so `effect_source` is `None`, and an entity that is not in its world yet fires nothing (vanilla fires it); WIT v0.2, test `a_plugin_hides_an_effect_particle_and_refuses_an_effect` (`crates/pumpkin-wasm-host-v0_2/tests`); test `change_a_settable_field_mob_effect_event_applicable` |
| MobEffectEvent.Expired | game | both | yes | - | supported | `EntityPotionEffectEvent` | fired in `entity/living.rs`; WIT v0.2; action expired |
| MobEffectEvent.Remove | game | both | yes | - | supported | `EntityPotionEffectEvent` | fired in `entity/living.rs`; WIT v0.2; action removed |
| MobSpawnEvent (abstract) | game | server | no | - | supported | shared fields of 3 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 3 supported |
| MobSpawnEvent.PositionCheck | game | server | no | setResult(Result) | supported | `MobSpawnEventPositionCheck` | fired for a created mob in the natural spawner and the chunk generation spawns (`world/natural_spawner.rs`), in `spawn_util::try_spawn_mob` (the warden of a sculk shrieker) and in the mob spawner block, with the spawn type and the spawner position; `result` is settable; `Default` keeps the check of Pumpkin at the site: the obstruction check of the summon, and none elsewhere, because Pumpkin has no `Mob.checkSpawnRules`; test `change_a_settable_field_mob_spawn_event_position_check`; WIT v0.2 |
| MobSpawnEvent.SpawnPlacementCheck | game | server | no | setResult(Result) | supported | `MobSpawnEventSpawnPlacementCheck` | fired at the end of `check_spawn_rules` (vanilla `SpawnPlacements.checkSpawnRules`) for the natural spawner and the chunk generation spawns, with the result of the spawn rules as `default_result`; `result` is settable; no random source; the peaceful check of Pumpkin is inside the rules, so it reaches the handler, where vanilla returns before the event; gap: the mob spawner block and zombie reinforcements read no spawn rules in Pumpkin; test `change_a_settable_field_mob_spawn_event_spawn_placement_check`; WIT v0.2 |
| MobSplitEvent | game | both | yes | - | supported | `MobSplitEvent` | fired in `entity/mob/slime.rs` for a dead slime or magma cube before the children spawn; `children` is settable, and the server spawns only new entities of the list; a cancel spawns no child; Pumpkin's `SlimeSplitEvent` still fires nowhere; tests `change_a_settable_field_mob_split_event`, `cancel_mob_split_event`; WIT v0.2, where a handler keeps a subset of the children |
| SpawnClusterSizeEvent | game | both | no | - | supported | `SpawnClusterSizeEvent` | fired in `spawn_category_for_position` after each natural spawn, with the mob and the limit per chunk of its type (vanilla `getMaxSpawnClusterSize`); `size` is settable; gap: the mob is not in the world yet, because the natural spawner of Pumpkin adds the pack after the loop, where vanilla fires after `addFreshEntityWithPassengers`; test `change_a_settable_field_spawn_cluster_size_event`; WIT v0.2 |

#### `event.entity.player` (68: 56 supported, 7 planned, 5 not supported)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| AdvancementEvent (abstract) | game | both | no | - | planned | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 1 supported, 1 planned |
| AdvancementEvent.AdvancementEarnEvent | game | both | no | - | supported | `PlayerAdvancementDoneEvent` | fired in `entity/player/advancement.rs`; WIT v0.2 |
| AdvancementEvent.AdvancementProgressEvent | game | both | no | - | planned | advancement criteria tracking | `PlayerAdvancements::award` and `revoke` run under the advancement lock, and `finish_award` (outside it) gets no criterion name and runs only for a completed advancement; the event needs a post-lock step for each grant and revoke |
| AnvilCraftEvent (abstract) | game | both | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported |
| AnvilCraftEvent.Post | game | both | no | - | supported | `AnvilCraftEventPost` | fired by `player_events::fire_anvil_take` after the slot click released the screen-handler lock, after the levels are paid, the inputs are used and the anvil update event fired; not fired after a cancelled Pre, a closed anvil or a changed input; WIT v0.2, test `a_plugin_cancels_an_anvil_take_and_reads_the_post` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `cancel_anvil_craft_event_pre`, `anvil_craft_events_handler_closes_the_screen_or_changes_the_inputs` |
| AnvilCraftEvent.Pre | game | both | yes | - | supported | `AnvilCraftEventPre` | the anvil result slot records the take when a handler listens (`InventoryPlayer::listens_to_anvil_craft`), and `player_events::fire_anvil_take` fires the event after the slot click released the screen-handler lock, before the cost is paid; a cancel skips the end of the take (levels, inputs, anvil damage) as `AnvilMenu#onTake` returns, and the handler does that work through `current_screen_handler`; a handler that closes the anvil or changes an input gets the same result; WIT v0.2, test `a_plugin_cancels_an_anvil_take_and_reads_the_post` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `cancel_anvil_craft_event_pre`, `anvil_craft_events_handler_closes_the_screen_or_changes_the_inputs` |
| ArrowLooseEvent | game | both | yes | - | supported | `ArrowLooseEvent` | fired in `BowItem::release_bow` for players; `charge` written back, a cancel shoots nothing; WIT v0.2, test `a_plugin_sets_the_action_of_a_nock_and_the_charge_of_a_loose` (`crates/pumpkin-wasm-host-v0_2/tests`); test `change_a_settable_field_arrow_loose_event` |
| ArrowNockEvent | game | both | yes | setAction(InteractionResult) | supported | `ArrowNockEvent` | fired in the bow `normal_use`; a cancel or a set `action` stops the draw, Pumpkin's item use returns no result; WIT v0.2, test `a_plugin_sets_the_action_of_a_nock_and_the_charge_of_a_loose` (`crates/pumpkin-wasm-host-v0_2/tests`); test `cancel_arrow_nock_event` |
| AttackEntityEvent | game | both | yes | - | supported | `EntityDamageByEntityEvent` | fired in `entity/living.rs`; WIT v0.2; fires later, after the damage is computed |
| BonemealEvent | game | both | yes | - | supported | `BlockFertilizeEvent` | fired in `block/registry.rs`; WIT v0.2 |
| CanContinueSleepingEvent | game | both | no | - | planned | sleep checks in the `Player` tick | `Player::tick` has no check that wakes a sleeping player (vanilla `Player.tick` wakes the player at day, `LivingEntity.tick` when the bed is gone), so there is no decision for the event to change |
| CanPlayerSleepEvent | game | server | no | - | supported | `PlayerBedEnterEvent` | fired in `block/blocks/bed.rs`, `block/blocks/straw_bed.rs`; WIT v0.2 |
| ClientInformationUpdatedEvent | game | server | no | - | supported | `ClientInformationUpdatedEvent` | fired in `net/java/play/client_information.rs` after the settings are stored; Java only; WIT v0.2, test `a_plugin_reads_a_trade_and_the_client_information` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_client_information_updated_event` |
| CriticalHitEvent | game | both | no | - | supported | `CriticalHitEvent` | fired in `Player::attack`; `critical_hit`, `damage_multiplier`, `disable_sweep` written back; the sprint knockback and the mace smash keep the vanilla attack type, only the sound and the particle follow the event; Pumpkin's `AttackType::new` never marks a falling mace hit critical (vanilla does), so `vanilla_critical` is `false` there; WIT v0.2, test `a_plugin_changes_a_critical_hit_and_stops_a_sweep` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `change_a_settable_field_critical_hit_event`, `forced_critical_hit_keeps_the_sprint_knockback_and_the_mace_smash` |
| CustomClickActionEvent | game | server | yes | - | supported | `DialogClickActionEvent` | fired in `net/java/mod.rs`; WIT v0.2; custom click actions from dialogs |
| FluidTooltipEvent | game | both | no | - | not supported | - | tooltip lines are built by the client; Pumpkin never builds them |
| ItemEntityPickupEvent (abstract) | game | both | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported |
| ItemEntityPickupEvent.Post | game | both | no | - | supported | `ItemEntityPickupEventPost` | fired in `ItemEntity::on_player_collision` after the items moved; WIT v0.2, test `a_plugin_cancels_an_orb_pickup_and_forces_an_item_pickup` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_item_entity_pickup_events` |
| ItemEntityPickupEvent.Pre | game | both | no | setCanPickup(TriState) | supported | `ItemEntityPickupEventPre` | fired in `ItemEntity::on_player_collision` for a living player that is not a spectator, before the pickup delay check; `can_pickup` (`TriState`) applied; `EntityPickupItemEvent` fires after the insert and cannot stop it; WIT v0.2, test `a_plugin_cancels_an_orb_pickup_and_forces_an_item_pickup` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_item_entity_pickup_events` |
| ItemFishedEvent | game | both | yes | - | supported | `PlayerFishEvent` | fired in `net/java/play/use_item.rs`; WIT v0.2 |
| ItemTooltipEvent | game | both | no | - | not supported | - | tooltip lines are built by the client; Pumpkin never builds them |
| PermissionsChangedEvent | game | server | yes | - | supported | `PermissionsChangedEvent` (`pumpkin_core::plugin::player`) | fired by `plugin::server_events::permission_change_cancelled` before the op list changes, in the op and deop commands, in `op_player` and `deop_player` of the v0.1 and v0.2 hosts and in the operator methods of the server management protocol (`operators/add`, `remove`, `clear` and `set`, `net/management/handlers.rs`), for an online player and only when the level changes, as `EventHooks.onPermissionChanged`; `old_level` is the level of the op list entry; a cancel leaves the op list and the level of the player; `Player::change_permission_lvl`, which the permission level call of the v0.1 and v0.2 hosts uses, fires it with the level of the player as the old level, because that call does not touch the op list (NeoForge has no such call); tests `permissions_changed_event_of_op_and_deop`, `cancel_permissions_changed_event`, `the_management_operator_methods_fire_permissions_changed_event`, `change_permission_lvl_fires_permissions_changed_event`; WIT v0.2, test `a_plugin_reads_the_levels_of_a_permission_change_and_cancels_it` |
| PlayerContainerEvent | game | both | no | - | supported | `PlayerContainerEvent` | concrete parent: a `ParentEventHandler` gets `InventoryOpenEvent` and `InventoryCloseEvent`; test `handler_on_a_parent_player_container_event` |
| PlayerContainerEvent.Close | game | both | no | - | supported | `InventoryCloseEvent` | fired in `entity/player.rs`; WIT v0.2 |
| PlayerContainerEvent.Open | game | both | no | - | supported | `InventoryOpenEvent` | fired in `entity/player.rs`; WIT v0.2 |
| PlayerDestroyItemEvent | game | both | no | - | supported | `PlayerItemBreakEvent` | fired in `entity/player.rs`; WIT v0.2 |
| PlayerEnchantItemEvent | game | both | no | - | supported | `PlayerEnchantItemEvent` | fired by `container_events::on_button_click` after the enchantments apply and before the lapis is consumed, after the screen-handler lock is released; the table writes `enchanted_item` back to slot 0 when slot 0 still holds the enchanted item, and consumes the lapis cost from slot 1 while it holds lapis (up to what it holds), so a handler that changes the lapis count still pays and a handler that closes the table keeps the lapis; WIT v0.2, test `a_plugin_changes_the_stacks_of_a_brew_and_reads_an_enchant` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `one_event_per_fire_site_enchantment_events`, `enchanting_events_handler_closes_the_screen`, `player_enchant_item_event_lapis_change_still_pays` |
| PlayerEvent (abstract) | game | both | no | - | planned | shared fields of 60 subclasses | abstract: the bus rejects listeners on abstract events (EventBus 8.0.5 `addToListeners`); subclasses: 48 supported, 5 planned, 5 not supported, 2 client-only |
| PlayerEvent.BreakSpeed | game | both | yes | - | supported | `PlayerEventBreakSpeed` | fired in `block::calc_block_breaking` on every mining step; `new_speed` written back, a cancel gives -1; the client also predicts break speed; WIT v0.2, test `a_plugin_slows_the_mining_and_refuses_the_harvest_of_a_block` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_break_speed_and_harvest_check` |
| PlayerEvent.Clone | game | both | no | - | planned | player respawn | `World::respawn_player` and the End exit portal keep the same `Arc<Player>`, so there is no old player to pair with a new one; the attachment copy rules of the event run on that player after the respawn position event and in the respawn world, and on the End exit (`ironpumpkin_mods::attachments`) |
| PlayerEvent.HarvestCheck | game | both | no | - | supported | `PlayerEventHarvestCheck` | fired in `Player::can_harvest` (mining speed and drops); `can_harvest` written back; WIT v0.2, test `a_plugin_slows_the_mining_and_refuses_the_harvest_of_a_block` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_break_speed_and_harvest_check` |
| PlayerEvent.ItemCraftedEvent | game | both | no | - | supported | `CraftItemEvent` | fired in `entity/player.rs`; WIT v0.2 |
| PlayerEvent.ItemSmeltedEvent | game | both | no | - | supported | `FurnaceExtractEvent` | fired in `entity/player.rs`; WIT v0.2 |
| PlayerEvent.ItemSmithedEvent | game | both | no | - | planned | `SmithItemEvent` | Pumpkin fires `SmithItemEvent` in `entity/player.rs` with the player and the recipe id only, and a handler can cancel it; NeoForge passes the input container and the result, fires it before the inputs shrink, and has no cancel |
| PlayerEvent.LoadFromFile | game | both | no | - | supported | `PlayerEventLoadFromFile` | fired in `Server::add_player` after the data load; `player_file(suffix)`; WIT v0.2, test `a_plugin_receives_the_tracking_and_the_player_file_events` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_player_file_events` |
| PlayerEvent.NameFormat | game | both | no | - | supported | `PlayerEventNameFormat` | fired in `Player::get_display_name` when no name is set through the API; not cached, fires on every read; WIT v0.2, test `a_plugin_renames_a_player_in_the_chat_and_in_the_tab_list` (`crates/pumpkin-wasm-host-v0_2/tests`); test `change_a_settable_field_name_format_events` |
| PlayerEvent.PlayerChangeGameModeEvent | game | both | yes | - | supported | `PlayerGamemodeChangeEvent` | fired in `entity/player.rs`; WIT v0.2 |
| PlayerEvent.PlayerChangedDimensionEvent | game | both | no | - | supported | `PlayerChangedWorldEvent` | fired in `entity/player.rs`; WIT v0.2 |
| PlayerEvent.PlayerLoggedInEvent | game | both | no | - | supported | `PlayerJoinEvent` | fired in `world/mod.rs`; WIT v0.2 |
| PlayerEvent.PlayerLoggedOutEvent | game | both | no | - | supported | `PlayerLeaveEvent` | fired in `world/mod.rs`; WIT v0.2 |
| PlayerEvent.PlayerRespawnEvent | game | both | no | - | supported | `PlayerRespawnEvent` | fired in `world/mod.rs`; WIT v0.2 |
| PlayerEvent.SaveToFile | game | both | no | - | supported | `PlayerEventSaveToFile` | fired in `ServerPlayerData` on leave, periodic save and `save_all_players` while saving is enabled, with the player handle of the caller (a leaving player is no longer in the player list); the periodic save fires before the asynchronous write and does not report whether the write succeeds; WIT v0.2, test `a_plugin_receives_the_tracking_and_the_player_file_events` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `one_event_per_fire_site_player_file_events`, `one_event_per_fire_site_logout_save_and_stop_tracking` |
| PlayerEvent.StartTracking | game | both | no | - | supported | `PlayerEventStartTracking` | fired in `TrackedEntity::add_pairing`, after the iteration of the entity map (`player_events::Deferral`); WIT v0.2, test `a_plugin_receives_the_tracking_and_the_player_file_events` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `one_event_per_fire_site_tracking_events`, `tracking_events_fire_after_the_entity_map_iteration` |
| PlayerEvent.StopTracking | game | both | no | - | supported | `PlayerEventStopTracking` | fired in `TrackedEntity::remove_player`, `forget_player` and `broadcast_removed`, after the iteration of the entity map; a player that leaves the world (logout, cross-dimension respawn) gets it through `player_events::untrack_leaving_player`; WIT v0.2, test `a_plugin_receives_the_tracking_and_the_player_file_events` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `one_event_per_fire_site_tracking_events`, `tracking_events_fire_after_the_entity_map_iteration`, `one_event_per_fire_site_logout_save_and_stop_tracking` |
| PlayerEvent.TabListNameFormat | game | both | no | - | supported | `PlayerEventTabListNameFormat` | fired in `Player::get_tab_list_name` when no name is set through the API; not cached, fires on every read (NeoForge caches it until `refreshTabListName`); `set_tab_list_name` sends the change; WIT v0.2, test `a_plugin_renames_a_player_in_the_chat_and_in_the_tab_list` (`crates/pumpkin-wasm-host-v0_2/tests`); test `change_a_settable_field_name_format_events` |
| PlayerFlyableFallEvent | game | both | no | - | supported | `PlayerFlyableFallEvent` | fired in `LivingEntity::handle_fall_damage` for a player that may fly; nothing read back; `distance` is the fall distance (NeoForge 26.3 passes the multiplier twice); WIT v0.2, test `a_plugin_receives_a_flyable_fall_and_a_hotbar_switch` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_player_flyable_fall_event` |
| PlayerInteractEvent (abstract) | game | both | no | - | supported | shared fields of 6 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 4 supported, 2 not supported |
| PlayerInteractEvent.EntityInteract | game | both | yes | setCancellationResult(InteractionResult) | supported | `PlayerInteractEntityEvent` | fired in `net/bedrock/play/inventory_action.rs`, `net/java/play/interact.rs`; WIT v0.2 |
| PlayerInteractEvent.LeftClickBlock | game | both | yes | setUseBlock(TriState), setUseItem(TriState) | supported | `PlayerInteractEvent` | fired in `net/java/play/player_action.rs` first for each dig action (start, stop, abort), before the reach check, as `ServerPlayerGameMode.handleBlockBreakAction` (`interact_events::left_click_block`); fields `left_click_action`, `face`, `hand` (always the main hand) and `clicked_pos`; `use_block` set to `False` skips the block attack at the dig start, and `use_item` has no server effect, as in NeoForge; a cancel stops the action: a cancelled start starts no dig, the finish of the dig that the client still sends breaks nothing (a finish breaks only at the position of the started dig, as STOP_DESTROY_BLOCK breaks only at `destroyPos`), and the client gets the block back; a cancel does not set `use_block` and `use_item` to `False` as NeoForge `setCanceled` does, which only a later handler that receives cancelled events sees; a swing that hits a block fires nothing; no Bedrock code fires it; WIT v0.2 with the NeoForge fields, `use_block`, `use_item` and `cancellation_result` written back, test `a_plugin_receives_the_dig_action_of_a_left_click_block` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `left_click_block_fires_at_the_dig_start` and `finished_digging_breaks_only_the_started_dig`; parity trace slice 2 |
| PlayerInteractEvent.LeftClickEmpty | game | both | no | - | not supported | - | only the client knows about a click on air; vanilla sends only the swing |
| PlayerInteractEvent.RightClickBlock | game | both | yes | setUseBlock(TriState), setUseItem(TriState), setCancellationResult(InteractionResult) | supported | `PlayerInteractEvent` | fired in `net/java/play/use_item_on.rs` (a use on a block, also in spectator) before the use phases (`interact_events::right_click_block`); fields `hand`, `face`, `hit_vector` (world coordinates), `clicked_pos` and `item_stack()`; `use_block` and `use_item` decide the phases as `ServerPlayerGameMode.useItemOn` of NeoForge: `use_block` set to `True` uses the block also for a sneaking player and `False` skips the block use; `use_item` set to `False` skips both item phases and `True` runs the item phase after the block also for an empty hand; a cancel skips the use, sends the block back and ends with `cancellation_result`: `Success` swings the arm for the other players and `SuccessServer` for every player, as `handleUseItemOn`; no Bedrock code fires it; WIT v0.2 with the NeoForge fields, `use_block`, `use_item` and `cancellation_result` written back, test `a_plugin_sets_the_decisions_of_a_right_click_block` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `right_click_block_and_use_item_on_block_fire_each_phase`, `use_block_and_use_item_denials_follow_common_hooks` and `a_success_cancellation_result_swings_the_arm_without_the_action`; parity trace slice 2 |
| PlayerInteractEvent.RightClickEmpty | game | both | no | - | not supported | - | only the client knows about an empty-hand click on air |
| PlayerInteractEvent.RightClickItem | game | both | yes | setCancellationResult(InteractionResult) | supported | `PlayerInteractEvent` | fired in `net/java/play/use_item.rs` before the item use, for each use of a non-empty hand by a player who is not a spectator, whatever the ray of the player hits, as `ServerPlayerGameMode.useItem` (`interact_events::right_click_item`), and in `net/bedrock/play/inventory_action.rs` (a Bedrock use of an item); fields `hand` and `item_stack()`; on Java a cancel skips the use (also the equip and the start of the use) and ends with `cancellation_result`, which swings the arm as for `RightClickBlock`; on Java it does not fire while the item is on cooldown, where vanilla returns before the event, and the Bedrock path still fires it then; WIT v0.2 with the NeoForge fields; action right click air; tests `a_success_cancellation_result_swings_the_arm_without_the_action`, `right_click_item_skips_an_item_on_cooldown` |
| PlayerNegotiationEvent | game | both | no | - | planned | phase 3 configuration-task queue in `net/java/pending.rs` | delays login until async work ends; no fire site yet: it can queue its pending work as a `ConfigurationTask::Mod` task, the dynamic task of task 2.11 |
| PlayerRespawnPositionEvent | game | server | no | - | supported | `PlayerSpawnLocationEvent` | fired in `World::respawn_player`; position, yaw, pitch, dimension and `copy_original_spawn_position` written back; a cancel keeps the vanilla position and the respawn point (NeoForge has no cancel); WIT v0.2 sees the position only; test `change_a_settable_field_respawn_position_event` |
| PlayerSetSpawnEvent | game | server | yes | - | supported | `PlayerSpawnChangeEvent` | fired in `entity/player.rs`; WIT v0.2 |
| PlayerSpawnPhantomsEvent | game | both | no | setResult(Result) | not supported | - | Pumpkin has no phantom spawner |
| PlayerSwitchHotbarSlotEvent (abstract) | game | both | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported |
| PlayerSwitchHotbarSlotEvent.Post | game | both | no | - | supported | `PlayerSwitchHotbarSlotEventPost` | fired in `net/java/play/set_held_item.rs`, `net/bedrock/play/mob_equipment.rs` after the change, only when the slot changed; WIT v0.2, test `a_plugin_receives_a_flyable_fall_and_a_hotbar_switch` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_switch_hotbar_slot_event_post` |
| PlayerSwitchHotbarSlotEvent.Pre | game | both | yes | - | supported | `PlayerItemHeldEvent` | fired in `net/bedrock/play/mob_equipment.rs`, `net/java/play/set_held_item.rs` only when the slot changes, as `handleSetCarriedItem`; WIT v0.2; test `switch_hotbar_slot_event_pre_fires_only_on_a_change` |
| PlayerWakeUpEvent | game | both | no | - | supported | `PlayerBedLeaveEvent` | fired in `entity/player.rs`; WIT v0.2 |
| PlayerXpEvent (abstract) | game | both | no | - | supported | shared fields of 3 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 3 supported |
| PlayerXpEvent.LevelChange | game | both | yes | - | supported | `PlayerLevelChangeEvent` | fired in `entity/player.rs`; WIT v0.2 |
| PlayerXpEvent.PickupXp | game | both | yes | - | supported | `PlayerXpEventPickupXp` | fired in `ExperienceOrbEntity::on_player_collision` before the pickup delay is set; a cancel leaves the orb; WIT v0.2, test `a_plugin_cancels_an_orb_pickup_and_forces_an_item_pickup` (`crates/pumpkin-wasm-host-v0_2/tests`); test `cancel_pickup_xp_event` |
| PlayerXpEvent.XpChange | game | both | yes | - | supported | `PlayerExpChangeEvent` | fired in `entity/player.rs`; WIT v0.2 |
| SweepAttackEvent | game | both | yes | - | supported | `SweepAttackEvent` | fired in `Player::attack` after `CriticalHitEvent`; `sweeping` written back, also on cancel; WIT v0.2, test `a_plugin_changes_a_critical_hit_and_stops_a_sweep` (`crates/pumpkin-wasm-host-v0_2/tests`); test `cancel_sweep_attack_event` |
| TradeWithVillagerEvent | game | both | no | - | supported | `TradeWithVillagerEvent` | recorded in `complete_trade` of the villager and the wandering trader, fired after the slot click released the trade screen-handler lock (`player_events::Deferral`); WIT v0.2, test `a_plugin_reads_a_trade_and_the_client_information` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_trade_with_villager_event` (a villager and a wandering trader) |
| UseItemOnBlockEvent | game | both | yes | setCancellationResult(InteractionResult) | supported | `UseItemOnBlockEvent` | fired in `net/java/play/use_item_on.rs` after `RightClickBlock`, once for each phase that runs (`ItemBeforeBlock`, `Block`, `ItemAfterBlock`), as the NeoForge patches of `ItemStack.onItemUseFirst`, `BlockStateBase.useItemOn` and `ItemStack.useOn` (`interact_events::use_item_on_block`); fields `hand`, `item` (a copy at the start of the phase), `pos`, `face`, `hit_vector` and `phase`; a cancel skips the phase and gives `cancellation_result` (`TryWithEmptyHand` by default): in the item before block phase any result but `Pass` ends the use; in the block phase the result takes the place of the block result, and `TryWithEmptyHand` goes on to the use without the item; in the item after block phase the item use and the block placement do not run; Pumpkin items have no use before the block, so that phase only fires the event; the arm swings for `Success` and `SuccessServer` as for `RightClickBlock`, except for a block phase result, which swings only for `SuccessServer`, as every Pumpkin block use does; WIT v0.2 (`neoforge-events.wit`), `cancellation_result` written back, test `a_plugin_cancels_use_item_on_block_and_sets_its_result` (`crates/pumpkin-wasm-host-v0_2/tests`); no Bedrock code fires it; tests `right_click_block_and_use_item_on_block_fire_each_phase`, `use_block_and_use_item_denials_follow_common_hooks` and `cancel_use_item_on_block_event_ends_its_phase`; parity trace slice 2 |

#### `event.level` (41: 30 supported, 10 planned, 1 not supported)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| AlterGroundEvent | game | both | no | - | planned | engine extension point (M5) | M5 engine extension: tree decorator internals of world generation; Pumpkin exposes no decorator hooks |
| BlockDropsEvent | game | server | yes | - | supported | `BlockDropItemEvent` | fired in `block/mod.rs`; WIT v0.2 |
| BlockEvent (abstract) | game | both | no | - | planned | shared fields of 17 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 15 supported, 2 planned |
| BlockEvent.BlockToolModificationEvent | game | both | yes | - | supported | `BlockEventBlockToolModificationEvent` (`pumpkin_core::plugin::level`) | fired by `level_events::tool_modification` at the sites of NeoForge 26.3: `Ignition::ignite_block` (flint and steel, fire charge; ability `firestarter_light`), `Ignition::ignite_dispensed` (a dispenser of flint and steel, after the fire placement and before a TNT primes, as `FlintAndSteelDispenseItemBehavior`) and the shears use on a growing plant (`shears_trim`); NeoForge 26.3 fires nothing for axes, hoes and shovels, which use the data-driven block transformers; `final_state` settable (`None` is no change; the unchanged state keeps the vanilla change); a cancel is no change, so a fire starter places fire next to the block; WIT v0.2 (`neoforge-events.wit`), `final_state` written back, tests `a_plugin_sets_the_final_state_of_a_tool_modification`, `a_plugin_cancels_a_tool_modification` (`crates/pumpkin-wasm-host-v0_2/tests`); gap: no `getContext`, only the player, the tool and the position; the dispenser of shears (`shears_harvest`) fires nothing; tests `one_event_per_fire_site_block_tool_modification_event`, `cancel_block_tool_modification_event`, `change_a_settable_field_block_tool_modification_event`, `dispensed_flint_and_steel_fires_the_tool_event_after_the_fire` |
| BlockEvent.EntityMultiPlaceEvent | game | both | yes | - | supported | `BlockMultiPlaceEvent` | fired by `level_events::player_place` in `BlockRegistry::place_block` after `BlockPlaceEvent` and before the blocks change, for a bed, a door, a tall plant or a small dripleaf, with `placed_blocks` and `replaced_blocks`; a cancel places no block; NeoForge posts one `EntityMultiPlaceEvent`, which an `EntityPlaceEvent` listener also gets, after the placement and restores the snapshots on a cancel; WIT v0.2 with `replaced_blocks`, the cancel written back, test `a_plugin_receives_the_replaced_blocks_of_a_multi_place` (`crates/pumpkin-wasm-host-v0_2/tests`); test `cancel_entity_multi_place_event` |
| BlockEvent.EntityPlaceEvent | game | both | yes | - | supported | `BlockPlaceEvent`, `BlockEventEntityPlaceEvent` (`pumpkin_core::plugin::level`) | a player placement fires `BlockPlaceEvent` in `BlockRegistry::place_block` with `placed_state` and `replaced_state`; an enderman placement fires `BlockEventEntityPlaceEvent` in `PlaceBlockGoal::tick` with the entity, because `BlockPlaceEvent` needs a player; both fire before the block changes, where NeoForge fires a player placement after it and restores the snapshot on a cancel; NeoForge 26.3 builds the enderman snapshot at the block below the target, IronPumpkin gives the target; gap: NeoForge `ItemStack.useOn` sends every server-side item use on a block except a bucket through `CommonHooks.onPlaceItemIntoWorld`, so the fire of a flint and steel or a fire charge, the axe, hoe and shovel transforms, bone meal and the shears trim also post `EntityPlaceEvent` or `EntityMultiPlaceEvent`, and Pumpkin fires `BlockPlaceEvent` for block items only; WIT v0.2 with the states, and `BlockEventEntityPlaceEvent` in `neoforge-events.wit`, tests `a_plugin_receives_the_states_of_a_block_place` and `world_place_block_by_an_entity_reaches_plugins_and_mods` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `one_event_per_fire_site_entity_place_event`, `cancel_entity_place_event`, `one_event_per_fire_site_entity_place_event_of_an_entity`, `cancel_entity_place_event_of_an_entity`; parity trace slice 2 |
| BlockEvent.FarmlandTrampleEvent | game | both | yes | - | planned | farmland trample in the farmland `BlockBehaviour` | no fire site: Pumpkin does not port `FarmBlock.fallOn`, so farmland is never trampled; EntityChangeBlockEvent fires only for the enderman block pickup |
| BlockEvent.FluidPlaceBlockEvent | game | both | yes | - | supported | `BlockFormEvent` | fired in `block/fluid/lava.rs`; WIT v0.2; lava and water forming stone, cobblestone or obsidian |
| BlockEvent.NeighborNotifyEvent | game | both | yes | - | supported | `BlockPhysicsEvent` | fired in `world/mod.rs`; WIT v0.2; neighbour update |
| BlockEvent.PortalSpawnEvent | game | both | yes | - | supported | `PortalCreateEvent` | fired in `item/items/ignite/fire_charge.rs`, `item/items/ignite/flint_and_steel.rs`; WIT v0.2; fired from flint and steel and fire charge only |
| BlockGrowFeatureEvent | game | both | yes | - | supported | `StructureGrowEvent` | fired in `block/blocks/plant/mangrove_propagule.rs`, `block/blocks/plant/mushroom_plant.rs`, `block/blocks/plant/sapling.rs`; WIT v0.2 |
| ChunkDataEvent (abstract) | game | both | no | - | planned | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 planned |
| ChunkDataEvent.Load | game | both | no | - | planned | chunk deserialization in `pumpkin-world` | M5 engine extension: the chunk I/O runs on the `pumpkin-world` I/O threads, which have no hook into the event bus, and Pumpkin has no `SerializableChunkData` for a mod to read or extend; `ChunkEvent.Load` is `ChunkLoad` |
| ChunkDataEvent.Save | game | both | no | - | planned | chunk serialization in `pumpkin-world` | M5 engine extension: the chunk I/O runs on the `pumpkin-world` I/O threads, which have no hook into the event bus, and Pumpkin has no `SerializableChunkData` for a mod to extend; `ChunkSave` is defined but never fired |
| ChunkEvent (abstract) | game | both | no | - | planned | shared fields of 4 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported, 2 planned |
| ChunkEvent.Load | game | both | no | - | supported | `ChunkLoad` | fired in `World::update_active_chunks` (the world tick) for each chunk that joined the loaded chunks of the world since the last tick; `cancelled` has no effect, the chunk is loaded; gap: no `isNewChunk`, Pumpkin does not report whether it generated or read the chunk; WIT v0.2 with the chunk, test `a_plugin_reads_the_chunk_of_a_chunk_load` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_chunk_load_and_unload_events` |
| ChunkEvent.Unload | game | both | no | - | supported | `ChunkUnloadEvent` | fired through `World::unload_chunk` in `World::update_active_chunks` (the world tick) for each chunk that left the loaded chunks of the world since the last tick; a chunk that loaded and left again since the last tick fires neither `ChunkLoad` nor `ChunkUnloadEvent`; `cancelled` has no effect; WIT v0.2; tests `one_event_per_fire_site_chunk_load_and_unload_events`, `a_chunk_loaded_and_unloaded_within_one_tick_fires_no_chunk_event` |
| ChunkTicketLevelUpdatedEvent | game | server | no | - | planned | engine extension point (M5) | M5 engine extension: ticket levels are internal to the pumpkin-world chunk system |
| ChunkWatchEvent (abstract) | game | server | no | - | supported | shared fields of 3 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 3 supported |
| ChunkWatchEvent.Sent | game | server | no | - | supported | `ChunkSend` | fired in `net/bedrock/mod.rs`, `net/java/mod.rs`, `world/mod.rs`; WIT v0.2 |
| ChunkWatchEvent.UnWatch | game | server | no | - | supported | `ChunkWatchEventUnWatch` (`pumpkin_core::plugin::level`) | fired once per chunk in `chunker::update_position` for the chunks that leave the view, and for each chunk of the view when the player changes world (`Player::teleport_world`, a respawn in another dimension) and when the player leaves (`Player::remove`), as `ChunkMap.dropChunk`; WIT v0.2, test `a_plugin_receives_a_chunk_unwatch` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_chunk_watch_event_unwatch` |
| ChunkWatchEvent.Watch | game | server | no | - | supported | `ChunkSend` | fired in `net/bedrock/mod.rs`, `net/java/mod.rs`, `world/mod.rs`; WIT v0.2 |
| ExplosionEvent (abstract) | game | server | no | - | supported | shared fields of 3 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 3 supported |
| ExplosionEvent.Detonate | game | server | no | - | supported | `ExplosionEventDetonate` (`pumpkin_core::plugin::level`) | fired in `Explosion::explode` after the explosion found its blocks and entities and before the damage, as `ServerExplosion.hurtEntities`; `affected_blocks` and `affected_entities` settable; the explosion finds its blocks before the damage, as vanilla; WIT v0.2, both lists written back, test `a_plugin_changes_the_blocks_and_entities_of_a_detonation` (`crates/pumpkin-wasm-host-v0_2/tests`); test `change_a_settable_field_explosion_event_detonate` |
| ExplosionEvent.Start | game | server | yes | - | supported | `EntityExplodeEvent` | fired in `World::run_explosion` before the explosion, with `world`, the center (`position`) and the power (`yield_rate`); a cancel stops the explosion; `entity_id` is 0, `World::explode` has no source; gap: no `ServerExplosion` (source, damage source, block interaction); WIT v0.2 with the world, test `a_plugin_receives_the_world_of_an_explosion_start_and_cancels_it` (`crates/pumpkin-wasm-host-v0_2/tests`); test `cancel_explosion_event_start` |
| ExplosionKnockbackEvent | game | server | no | - | supported | `ExplosionKnockbackEvent` (`pumpkin_core::plugin::level`) | fired in `Explosion::damage_entities` for each entity that the explosion pushes, before the push; `knockback` settable; WIT v0.2, `knockback` written back, test `a_plugin_sets_the_knockback_of_an_explosion` (`crates/pumpkin-wasm-host-v0_2/tests`); test `change_a_settable_field_explosion_knockback_event` |
| GameRuleChangedEvent | game | server | no | - | supported | `GameRuleChangedEvent` (`pumpkin_core::plugin::level`) | fired after the `/gamerule` command (the rules of the server) and after `World::set_game_rule` (the rules of one world), with `rule` and `value`; WIT v0.2, test `a_plugin_receives_the_rule_and_value_of_a_game_rule_change` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_game_rule_changed_event` |
| LevelEvent (abstract) | game | both | no | - | planned | shared fields of 11 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 8 supported, 3 planned |
| LevelEvent.CreateSpawnPosition | game | both | yes | - | planned | world spawn selection at world creation | no fire site: Pumpkin picks the spawn in `Server::new` (`LevelData::from_world_generator`) before the server and its worlds exist, so no handler can get the level |
| LevelEvent.Load | game | both | no | - | supported | `WorldLoadEvent` | fired in `server/mod.rs`; WIT v0.2 |
| LevelEvent.PotentialSpawns | game | both | yes | - | supported | `LevelEventPotentialSpawns` (`pumpkin_core::plugin::level`) | fired in `natural_spawner::get_random_spawn_mob_at` and `can_spawn_mob_at`, the two users of `NaturalSpawner.mobsAt`; `spawns` settable, a new entry is a `&'static Spawner`; a cancel spawns nothing; gap: the biome lists of Pumpkin have no weights and no nether fortress list; WIT v0.2, `spawns` written back: a v0.2 entry names its entity type; an entry with an unknown type, a minimum below 1, a maximum below the minimum or above the mob cap of the category is dropped; the host reads at most 256 entries and leaks each new entry once, at most 256 in all; tests `a_plugin_changes_and_cancels_the_potential_spawns`, `added_spawn_entries_are_reused_and_capped` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `change_a_settable_field_level_event_potential_spawns`, `cancel_level_event_potential_spawns` |
| LevelEvent.Save | game | both | no | - | supported | `WorldSaveEvent` | fired in `world/mod.rs`; WIT v0.2 |
| LevelEvent.Unload | game | both | no | - | supported | `WorldUnloadEvent` | fired in `world/mod.rs`; WIT v0.2 |
| ModifyCustomSpawnersEvent | game | server | no | - | not supported | - | Pumpkin has no custom spawner list (phantom, patrol, cat, trader, siege) to modify |
| NoteBlockEvent (abstract) | game | both | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported |
| NoteBlockEvent.Change | game | both | yes | - | supported | `NoteBlockEventChange` (`pumpkin_core::plugin::level`) | fired in the note block `normal_use` before the note changes; `note` settable, the note id 0 to 24 of `getVanillaNoteId` (the `Note` and `Octave` enums are not ported); a cancel keeps the note and fails the use; WIT v0.2, `note` written back, test `a_plugin_tunes_and_cancels_a_note_block_change` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `one_event_per_fire_site_note_block_event_change`, `cancel_note_block_event_change`, `change_a_settable_field_note_block_event_change` |
| NoteBlockEvent.Play | game | both | yes | - | supported | `NotePlayEvent` | fired in `block/blocks/note.rs`; WIT v0.2 |
| PistonEvent (abstract) | game | both | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported |
| PistonEvent.Post | game | both | no | - | supported | `PistonEventPost` (`pumpkin_core::plugin::level`) | fired at the end of the piston synced block event after an extend or a retract moved the blocks; WIT v0.2, test `a_plugin_receives_a_piston_post` (`crates/pumpkin-wasm-host-v0_2/tests`); gap: no `getStructureHelper`; test `one_event_per_fire_site_piston_event_post` |
| PistonEvent.Pre | game | both | yes | - | supported | `BlockPistonExtendEvent` | fired in `block/blocks/piston/piston.rs`; WIT v0.2; BlockPistonRetractEvent for retraction |
| SleepFinishedTimeEvent | game | server | yes | - | supported | `TimeSkipEvent` | fired in `world/mod.rs`; WIT v0.2; night skip reason |

#### `event.level.block` (5: 5 supported)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| BreakBlockEvent | game | both | yes | - | supported | `BlockBreakEvent` | fired in `World::break_block` and `World::destroy_block` (vanilla `Level.destroyBlock`, with the player that owns the breaker in `player`) before the block changes; NeoForge posts no `BreakBlockEvent` from `Level.destroyBlock`, so a break by a projectile, a mob or a machine fires one only on IronPumpkin; with `player`, `state`, `block_position`, `exp` and `drop`; `exp` is the experience that the break drops when it drops items and is settable (NeoForge 26.3 sets it in `BlockDropsEvent`); Pumpkin also fires it without a player where NeoForge posts nothing: a block that loses its support, a piston that breaks a block, a bed or a respawn anchor that explodes, a hatched egg, a sheep that eats grass, and a machine or a mob without a player owner, which a `FakePlayer` does in NeoForge; a port reads `player == None` as such a break; gap: no `notifyClient` (the Java client always gets the block back), and no pre-cancelled event for a held item that cannot break the block or a game master block; WIT v0.2 with the state, test `a_plugin_receives_the_state_of_a_block_break_and_sets_its_drops` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `one_event_per_fire_site_break_block_event`, `cancel_break_block_event`, `change_a_settable_field_break_block_event`, `a_mod_cancels_the_break_of_another_mods_block` (`crates/ironpumpkin-mods/tests`); parity trace slice 2 |
| CreateFluidSourceEvent | game | server | no | - | supported | `CreateFluidSourceEvent` (`pumpkin_core::plugin::level`) | fired in `FlowingFluid::get_new_liquid` and `check_infinite_source_formation` for each source next to a flowing fluid; `can_convert` settable; Pumpkin also checks a new source when the fluid spreads, so the event fires more often than in NeoForge; WIT v0.2, `can_convert` written back, test `a_plugin_sets_can_convert_of_create_fluid_source` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `one_event_per_fire_site_create_fluid_source_event`, `change_a_settable_field_create_fluid_source_event` |
| CropGrowEvent (abstract) | game | both | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported |
| CropGrowEvent.Post | game | both | no | - | supported | `CropGrowEventPost` (`pumpkin_core::plugin::level`) | partial: fired in `CropBlockBase::random_tick` after the growth, where `CropGrowEvent.Pre` (`BlockGrowEvent`) fires, so only for wheat, carrots, potatoes, beetroots and the torchflower; gap: NeoForge also fires it for stems, sweet berry bushes, sugar cane, nether wart, growing plant heads (kelp, vines), cactus, bamboo, chorus flowers and cocoa; WIT v0.2, test `a_plugin_receives_a_crop_grow_post` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_crop_grow_event_post` |
| CropGrowEvent.Pre | game | both | no | setResult(Result) | supported | `BlockGrowEvent` | partial: fired in `CropBlockBase::random_tick` (`block/blocks/plant/crop/mod.rs`), so only for wheat, carrots, potatoes, beetroots and the torchflower, and only after a successful growth roll; NeoForge fires it on every eligible random tick before the roll, and the result `GROW` forces the growth; gap: NeoForge also fires it for stems, sweet berry bushes, sugar cane, nether wart, growing plant heads (kelp, vines), cactus, bamboo, chorus flowers and cocoa; WIT v0.2; NeoForge uses a result; Pumpkin uses cancel |

#### `event.server` (7: 6 supported, 1 planned)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| RegisterRpcSchemaEvent | game | server | no | - | planned | management JSON-RPC server (`net/management/rpc.rs`) | M5 engine extension: the server answers `rpc.discover` with the fixed schema file `assets/json-rpc-api-schema.json`, and no hook adds a schema component |
| ServerAboutToStartEvent | game | server | no | - | supported | `ServerAboutToStartEvent` | fired in `Server::new` after the mod lifecycle and the synced config load, after the content freeze and before the worlds load, as `ServerLifecycleHooks.handleServerAboutToStart` before `loadLevel`; native mods only, because the Wasm plugins load later; gap: the datapack recipes and tags are not loaded yet (`datapack_manager.load_all` runs after the worlds), while NeoForge has them in the `WorldStem`; boot test `neoforge_lifecycle_events`; parity trace |
| ServerLifecycleEvent (abstract) | game | server | no | - | supported | shared fields of 5 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 5 supported |
| ServerStartedEvent | game | server | no | - | supported | `ServerLoadEvent` | fired in `lib.rs` after the plugins load and before the ticker starts, as on `NeoForge`; test `neoforge_lifecycle_events` (`crates/ironpumpkin-test-mod/tests`); WIT v0.2 |
| ServerStartingEvent | game | server | no | - | supported | `ServerStartingEvent` | fired in `lib.rs` after the worlds load and before the ticker starts; Wasm plugins load later; test `neoforge_lifecycle_events` (`crates/ironpumpkin-test-mod/tests`) |
| ServerStoppedEvent | game | server | no | - | supported | `ServerStoppedEvent` | fired in `lib.rs` after the worlds save; Wasm plugins are unloaded before it; test `neoforge_lifecycle_events` (`crates/ironpumpkin-test-mod/tests`) |
| ServerStoppingEvent | game | server | no | - | supported | `ServerStoppingEvent` | fired in `lib.rs` when the server stops accepting connections, after the ticker thread exits (bound 30 s: a tick that runs longer logs a warning and the shutdown continues); test `neoforge_lifecycle_events` (`crates/ironpumpkin-test-mod/tests`) |

#### `event.tick` (12: 12 supported)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| EntityTickEvent (abstract) | game | both | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported |
| EntityTickEvent.Post | game | both | no | - | supported | `EntityTickEventPost` (`pumpkin_core::plugin::level`) | fired in `World::tick` after an entity that is not a player ticked; the entities tick in parallel, so handlers of two entities can run at the same time; WIT v0.2, one event per entity and tick (section "Performance guardrail"), test `a_plugin_receives_the_entity_and_player_tick_events` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_entity_tick_events` |
| EntityTickEvent.Pre | game | both | yes | - | supported | `EntityTickEventPre` (`pumpkin_core::plugin::level`) | fired in `World::tick` before an entity that is not a player ticks; a cancel skips the tick, the age and `EntityTickEventPost`; a rider ages and loses its motion before the event, so a cancel keeps both, as NeoForge fires the event in `rideTick` after `ServerLevel.tickPassenger` ran `commonTick`; a player fires `PlayerTickEvent` only (NeoForge also fires `EntityTickEvent` for a `ServerPlayer`); WIT v0.2, one event per entity and tick (section "Performance guardrail"), test `a_plugin_cancels_an_entity_tick` (`crates/pumpkin-wasm-host-v0_2/tests`); tests `one_event_per_fire_site_entity_tick_events`, `cancel_entity_tick_event_pre`, `cancel_entity_tick_event_pre_of_a_rider_keeps_its_age` |
| LevelTickEvent (abstract) | game | both | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported |
| LevelTickEvent.Post | game | both | no | - | supported | `LevelTickEventPost` | fired in `server/mod.rs` after each world ticks, also while the game is frozen (once per world, in sequence); in a normal tick the worlds tick in parallel, so handlers of two worlds can run at the same time; WIT v0.2, test `a_plugin_receives_the_level_tick_events` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_level_tick_events` and `neoforge_lifecycle_events` (`crates/ironpumpkin-test-mod/tests`) |
| LevelTickEvent.Pre | game | both | no | - | supported | `LevelTickEventPre` | fired in `server/mod.rs` before each world ticks, also while the game is frozen (once per world, in sequence); in a normal tick the worlds tick in parallel, so handlers of two worlds can run at the same time; WIT v0.2, test `a_plugin_receives_the_level_tick_events` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_level_tick_events` and `neoforge_lifecycle_events` (`crates/ironpumpkin-test-mod/tests`) |
| PlayerTickEvent (abstract) | game | both | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported |
| PlayerTickEvent.Post | game | both | no | - | supported | `PlayerTickEventPost` (`pumpkin_core::plugin::level`) | fired at the end of `Player::tick`; WIT v0.2, one event per player and tick (section "Performance guardrail"), test `a_plugin_receives_the_entity_and_player_tick_events` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_player_tick_events`; parity trace slice 2 |
| PlayerTickEvent.Pre | game | both | no | - | supported | `PlayerTickEventPre` (`pumpkin_core::plugin::level`) | fired in `Player::tick` after the packets of the tick; the players tick in parallel; WIT v0.2, one event per player and tick (section "Performance guardrail"), test `a_plugin_receives_the_entity_and_player_tick_events` (`crates/pumpkin-wasm-host-v0_2/tests`); test `one_event_per_fire_site_player_tick_events`; parity trace slice 2 |
| ServerTickEvent (abstract) | game | server | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 supported |
| ServerTickEvent.Post | game | server | no | - | supported | `ServerTickEndEvent` | fired in `server/ticker.rs`; `has_time()`; test `neoforge_lifecycle_events` (`crates/ironpumpkin-test-mod/tests`); WIT v0.2 |
| ServerTickEvent.Pre | game | server | no | - | supported | `ServerTickStartEvent` | fired in `server/ticker.rs`; `has_time()`; test `neoforge_lifecycle_events` (`crates/ironpumpkin-test-mod/tests`); WIT v0.2 |

#### `event.village` (1: 1 not supported)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| VillageSiegeEvent | game | both | yes | - | not supported | - | Pumpkin has no village siege spawner |

#### `fml.event.config` (4: 4 supported)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| ModConfigEvent | mod | both | no | - | supported | `NativeMod::config_loading`, `config_reloading`, `config_unloading` | concrete parent of the config events; the Rust API has the three hooks and no parent handler, so a listener on the parent implements the three hooks; tests of the three hooks below |
| ModConfigEvent.Loading | mod | both | no | - | supported | `NativeMod::config_loading` | called once per config after it loads: Startup inside `register_config`, Local before `common_setup`, Synced after `load_complete`; the hooks of all mods run one at a time; tests `local_configs_load_before_common_setup_and_synced_after_load_complete` and `neoforge_lifecycle_events` (`crates/ironpumpkin-test-mod/tests`) |
| ModConfigEvent.Reloading | mod | both | no | - | supported | `NativeMod::config_reloading` | called after a reload that the poll finds (`mod_config_reload_ticks`) and after `ModConfig::save`; a `save` inside a hook of the same config does not call the hook again; tests `save_calls_the_reloading_hook_once_even_when_the_hook_saves`, `an_edit_reloads_within_the_interval_and_changes_the_synced_body` and `neoforge_client_syncs_custom_content` (`crates/ironpumpkin-test-mod/tests`) |
| ModConfigEvent.Unloading | mod | both | no | - | supported | `NativeMod::config_unloading` | called for each Synced config after the server stopped and `ServerStoppedEvent` fired, as `ServerLifecycleHooks.handleServerStopped` calls `ConfigTracker.unloadConfigs(SYNCED)`; the values are gone inside the hook; Local and Startup configs never unload, as in NeoForge; a config that never loaded is skipped; tests `a_stopped_server_unloads_the_synced_configs_and_calls_the_hook_for_each` (`config.rs`) and `second_server` (`crates/ironpumpkin-test-mod/tests`, the real stop path) |

#### `fml.event.lifecycle` (8: 8 supported)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| FMLCommonSetupEvent | mod | both | no | - | supported | `NativeMod::common_setup` | called for every mod in load order after the last `init`, with the Local configs loaded; tests `lifecycle_runs_in_neoforge_order_on_each_mod_and_delivers_imc`, `local_configs_load_before_common_setup_and_synced_after_load_complete` and `neoforge_lifecycle_events` (`crates/ironpumpkin-test-mod/tests`) |
| FMLConstructModEvent | mod | both | no | - | supported | `NativeMod::init` | `init_mods` calls `init` of each mod in load order, with the `ModInit` registration API; tests `init_runs_in_dependency_order` and `neoforge_lifecycle_events` (`crates/ironpumpkin-test-mod/tests`) |
| FMLDedicatedServerSetupEvent | mod | server | no | - | supported | `NativeMod::dedicated_server_setup` | called for every mod in load order after `common_setup`; test `lifecycle_runs_in_neoforge_order_on_each_mod_and_delivers_imc` |
| FMLLoadCompleteEvent | mod | both | no | - | supported | `NativeMod::load_complete` | called for every mod in load order after the IMC steps; the Synced configs load right after it; tests `lifecycle_runs_in_neoforge_order_on_each_mod_and_delivers_imc`, `local_configs_load_before_common_setup_and_synced_after_load_complete` and `neoforge_lifecycle_events` (`crates/ironpumpkin-test-mod/tests`) |
| InterModEnqueueEvent | mod | both | no | - | supported | `NativeMod::inter_mod_enqueue` (`InterModEnqueue::send`) | a send to a mod that is not loaded returns false; test `lifecycle_runs_in_neoforge_order_on_each_mod_and_delivers_imc` |
| InterModProcessEvent | mod | both | no | - | supported | `NativeMod::inter_mod_process` (`InterModProcess::get_messages`) | the target mod lists the messages that were sent to it; test `lifecycle_runs_in_neoforge_order_on_each_mod_and_delivers_imc` |
| ModLifecycleEvent (abstract) | mod | both | no | - | supported | shared fields of 7 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 6 supported, 1 client-only |
| ParallelDispatchEvent (abstract) | mod | both | no | - | supported | shared fields of 7 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 6 supported, 1 client-only |

#### Events in other packages (19: 9 supported, 6 planned, 4 not supported)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| RegisterCapabilitiesEvent (`capabilities`) | mod | both | no | - | supported | `NativeMod::register_capabilities` with `ironpumpkin_mods::capabilities::RegisterCapabilitiesEvent` | called once per `EventPriority` phase for every mod in load order, host first, after `dedicated_server_setup` and before `ExtendPoiTypesEvent` (`RegistrationEvents.init`); closes after the last phase; tests `every_mod_gets_each_phase_in_order_after_the_host`, `registration_closes_after_the_event` |
| RegisterTicketControllersEvent (`common.world.chunk`) | mod | both | no | - | planned | engine extension point (M5) | M5 engine extension: per-mod forced chunk tickets are not exposed by the pumpkin-world chunk system |
| ExtendPoiTypesEvent (`common.world.poi`) | mod | both | no | - | supported | `NativeMod::extend_poi_types` with `ironpumpkin_mods::registration::ExtendPoiTypesEvent`; `content::poi::poi_type_for_state` gives the POI storage the type of the added blocks | fires after `dedicated_server_setup`; `add_block_to_poi` adds every state of a block to a job site POI type; `addStatesToPoi` with a part of the states has no counterpart, and a POI type without a profession logs a warning; tests `each_registration_event_fires_once_at_its_neoforge_position`, `the_poi_storage_records_the_mod_poi` |
| GatherDataEvent (abstract) (`data.event`) | mod | both | no | - | not supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 2 not supported |
| GatherDataEvent.Client (`data.event`) | mod | both | no | - | not supported | - | data generation runs at build time |
| GatherDataEvent.Server (`data.event`) | mod | both | no | - | not supported | - | data generation runs at build time; ported mods ship the generated files |
| XpOrbTargetingEvent (`entity`) | game | both | no | - | planned | experience orb entity | the vanilla code path is missing: Pumpkin's experience orb never follows a player (no `followNearbyPlayer`), so there is no fire site |
| RegisterCauldronFluidContentEvent (`fluids`) | mod | both | no | - | planned | engine extension point (M5) | M5 engine extension: part of the NeoForge transfer API, which has no Pumpkin equivalent |
| RegisterConfigurationTasksEvent (`network.event`) | mod | server | no | - | supported | `ironpumpkin_mods::network::RegisterConfigurationTasksEvent` (`pumpkin_core::plugin::api::events::network`) | fires once per Java connection after the channel negotiation, when the tasks are queued; `register` takes a `CustomConfigurationTask`; parity trace slice 1, boot test `neoforge_network_with_the_headless_client` |
| RegisterPayloadHandlersEvent (`network.event`) | mod | both | no | - | supported | `ironpumpkin_mods::network::RegisterPayloadHandlersEvent` (`pumpkin_core::plugin::api::events::network`) | fires once after `load_complete` and before the synced configs load, as `NetworkRegistry.setup`; `registrar` gives a `PayloadRegistrar`; parity trace slice 1, tests `lifecycle_runs_in_neoforge_order_on_each_mod_and_delivers_imc`, `neoforge_lifecycle_events` |
| ModifyRegistriesEvent (`registries`) | mod | both | no | - | planned | engine extension point (M5) | M5 engine extension: registry callbacks need mutable registries; Pumpkin registries are generated |
| NewDatapackRegistryEvent (`registries`) | mod | both | no | - | planned | engine extension point (M5) | M5 engine extension: the datapack registry list in `dynamic_registry_loader.rs` is fixed; no custom registry types |
| NewRegistryEvent (`registries`) | mod | both | no | - | planned | engine extension point (M5) | M5 engine extension: new static registries need registry sync for arbitrary types; phase 2 covers block, item and entity type |
| RegisterEvent (`registries`) | mod | both | no | - | supported | `pumpkin_core::plugin::registries::RegisterEvent`; a handler registers through `ironpumpkin_mods::content::RegisterEventExt` | fires after the last `init` and before `common_setup`, for `minecraft:block`, `minecraft:entity_type` and `minecraft:item` only, in that order; test `register_event_fires_once_per_registry_between_init_and_common_setup` |
| DataMapsUpdatedEvent (`registries.datamaps`) | game | both | no | - | supported | `pumpkin_core::plugin::data_maps::DataMapsUpdatedEvent` | fired after each load (startup and `/reload`) for every registry that had a data map file, with cause `ServerReload` |
| RegisterDataMapTypesEvent (`registries.datamaps`) | mod | both | no | - | supported | `NativeMod::register_data_map_types` with `ironpumpkin_mods::data_maps::RegisterDataMapTypesEvent` | called for every mod in load order, host first with the `NeoForgeDataMaps` types, after `RegisterCapabilitiesEvent` and before `ExtendPoiTypesEvent` (`RegistrationEvents.init`); closes after the last mod; tests `register_rejects_a_duplicate_id`, `registration_closes_after_the_event` |
| PermissionGatherEvent (abstract) (`server.permission.events`) | game | server | no | - | supported | shared fields of 2 subclasses | abstract: the bus rejects listeners on abstract events; subclasses: 1 supported, 1 not supported |
| PermissionGatherEvent.Handler (`server.permission.events`) | game | server | no | - | not supported | - | Pumpkin has one permission manager; PlayerPermissionCheckEvent overrides single checks |
| PermissionGatherEvent.Nodes (`server.permission.events`) | game | server | no | - | supported | `Context::register_permission` (API, no event) | API call instead of an event |

#### Client events (189, all client-only)

Every class below lives in the client source set (`src/client/java`) or is posted only from client code, so it never fires on a dedicated server. Status: client-only.

| Event | Package | Bus | Side | Cancel | Status | Reason |
|:--|:--|:--|:--|:--|:--|:--|
| AddClientReloadListenersEvent | client.event | mod | client | no | client-only | client source set |
| AddDebugSubscriptionFlagsEvent | client.event | game | client | no | client-only | client source set |
| AddSectionGeometryEvent | client.event | game | client | no | client-only | client source set |
| CalculateDetachedCameraDistanceEvent | client.event | game | client | no | client-only | client source set |
| CalculatePlayerTurnEvent | client.event | game | client | no | client-only | client source set |
| ClientChatEvent | client.event | game | client | yes | client-only | client source set |
| ClientChatReceivedEvent | client.event | game | client | yes | client-only | client source set |
| ClientChatReceivedEvent.Player | client.event | game | client | yes | client-only | client source set |
| ClientChatReceivedEvent.System | client.event | game | client | yes | client-only | client source set |
| ClientPauseChangeEvent (abstract) | client.event | game | client | no | client-only | client source set |
| ClientPauseChangeEvent.Post | client.event | game | client | no | client-only | client source set |
| ClientPauseChangeEvent.Pre | client.event | game | client | yes | client-only | client source set |
| ClientPlayerChangeGameTypeEvent | client.event | game | client | no | client-only | client source set |
| ClientPlayerNetworkEvent (abstract) | client.event | game | client | no | client-only | client source set |
| ClientPlayerNetworkEvent.Clone | client.event | game | client | no | client-only | client source set |
| ClientPlayerNetworkEvent.LoggingIn | client.event | game | client | no | client-only | client source set |
| ClientPlayerNetworkEvent.LoggingOut | client.event | game | client | no | client-only | client source set |
| ClientResourceLoadFinishedEvent | client.event | game | client | no | client-only | client source set |
| ClientTickEvent (abstract) | client.event | game | client | no | client-only | client source set |
| ClientTickEvent.Post | client.event | game | client | no | client-only | client source set |
| ClientTickEvent.Pre | client.event | game | client | no | client-only | client source set |
| ComputeFovModifierEvent | client.event | game | client | no | client-only | client source set |
| ConfigureMainRenderTargetEvent | client.event | mod | client | no | client-only | client source set |
| CustomizeGuiOverlayEvent (abstract) | client.event | game | client | no | client-only | client source set |
| CustomizeGuiOverlayEvent.BossEventProgress | client.event | game | client | yes | client-only | client source set |
| CustomizeGuiOverlayEvent.Chat | client.event | game | client | no | client-only | client source set |
| EntityRenderersEvent (abstract) | client.event | mod | client | no | client-only | client source set |
| EntityRenderersEvent.AddLayers | client.event | mod | client | no | client-only | client source set |
| EntityRenderersEvent.CreateSkullModels | client.event | mod | client | no | client-only | client source set |
| EntityRenderersEvent.RegisterLayerDefinitions | client.event | mod | client | no | client-only | client source set |
| EntityRenderersEvent.RegisterRenderers | client.event | mod | client | no | client-only | client source set |
| ExtractBlockOutlineRenderStateEvent | client.event | game | client | yes | client-only | client source set |
| ExtractBlockScreenEffectEvent | client.event | game | client | yes | client-only | client source set |
| ExtractLevelRenderStateEvent | client.event | game | client | no | client-only | client source set |
| FlipFrameEvent | client.event | game | client | no | client-only | client source set |
| FrameGraphSetupEvent | client.event | game | client | no | client-only | client source set |
| GatherEffectScreenTooltipsEvent | client.event | game | client | no | client-only | client source set |
| InitializeClientRegistriesEvent | client.event | mod | client | no | client-only | client source set |
| InputEvent (abstract) | client.event | game | client | no | client-only | client source set |
| InputEvent.InteractionKeyMappingTriggered | client.event | game | client | yes | client-only | client source set |
| InputEvent.Key | client.event | game | client | no | client-only | client source set |
| InputEvent.MouseButton (abstract) | client.event | game | client | no | client-only | client source set |
| InputEvent.MouseButton.Post | client.event | game | client | no | client-only | client source set |
| InputEvent.MouseButton.Pre | client.event | game | client | yes | client-only | client source set |
| InputEvent.MouseScrollingEvent | client.event | game | client | yes | client-only | client source set |
| ModelEvent (abstract) | client.event | game | client | no | client-only | client source set |
| ModelEvent.BakingCompleted | client.event | mod | client | no | client-only | client source set |
| ModelEvent.ModifyBakingResult | client.event | mod | client | no | client-only | client source set |
| ModelEvent.RegisterLoaders | client.event | mod | client | no | client-only | client source set |
| ModelEvent.RegisterStandalone | client.event | mod | client | no | client-only | client source set |
| MovementInputUpdateEvent | client.event | game | client | no | client-only | client source set |
| PlayerHeartTypeEvent | client.event | game | client | no | client-only | client source set |
| PrepareRenderBuffersEvent | client.event | game | client | no | client-only | client source set |
| RecipesReceivedEvent | client.event | game | client | no | client-only | client source set |
| RegisterBlockModelsEvent | client.event | mod | client | no | client-only | client source set |
| RegisterBlockStateModels | client.event | mod | client | no | client-only | client source set |
| RegisterClientCommandsEvent | client.event | game | client | no | client-only | client source set |
| RegisterClientTooltipComponentFactoriesEvent | client.event | mod | client | no | client-only | client source set |
| RegisterColorHandlersEvent (abstract) | client.event | mod | client | no | client-only | client source set |
| RegisterColorHandlersEvent.BlockTintSources | client.event | mod | client | no | client-only | client source set |
| RegisterColorHandlersEvent.ColorResolvers | client.event | mod | client | no | client-only | client source set |
| RegisterColorHandlersEvent.ItemTintSources | client.event | mod | client | no | client-only | client source set |
| RegisterConditionalItemModelPropertyEvent | client.event | mod | client | no | client-only | client source set |
| RegisterCustomEnvironmentEffectRendererEvent | client.event | mod | client | no | client-only | client source set |
| RegisterDebugEntriesEvent | client.event | mod | client | no | client-only | client source set |
| RegisterDebugRenderersEvent | client.event | mod | client | no | client-only | client source set |
| RegisterDimensionTransitionScreenEvent | client.event | mod | client | no | client-only | client source set |
| RegisterEntitySpectatorShadersEvent | client.event | mod | client | no | client-only | client source set |
| RegisterFeatureRenderersEvent | client.event | game | client | no | client-only | client source set |
| RegisterFluidModelsEvent | client.event | mod | client | no | client-only | client source set |
| RegisterGuiLayersEvent | client.event | mod | client | no | client-only | client source set |
| RegisterItemDecorationsEvent | client.event | mod | client | no | client-only | client source set |
| RegisterItemModelsEvent | client.event | mod | client | no | client-only | client source set |
| RegisterJsonAnimationTypesEvent | client.event | mod | client | no | client-only | client source set |
| RegisterKeyMappingsEvent | client.event | mod | client | no | client-only | client source set |
| RegisterMenuScreensEvent | client.event | mod | client | no | client-only | client source set |
| RegisterParticleGroupsEvent | client.event | mod | client | no | client-only | client source set |
| RegisterParticleProvidersEvent | client.event | mod | client | no | client-only | client source set |
| RegisterPictureInPictureRenderersEvent | client.event | mod | client | no | client-only | client source set |
| RegisterPresetEditorsEvent | client.event | mod | client | no | client-only | client source set |
| RegisterRangeSelectItemModelPropertyEvent | client.event | mod | client | no | client-only | client source set |
| RegisterRecipeBookSearchCategoriesEvent | client.event | mod | client | no | client-only | client source set |
| RegisterRenderPipelinesEvent | client.event | mod | client | no | client-only | client source set |
| RegisterSelectItemModelPropertyEvent | client.event | mod | client | no | client-only | client source set |
| RegisterSpecialModelRendererEvent | client.event | mod | client | no | client-only | client source set |
| RegisterSpriteSourcesEvent | client.event | mod | client | no | client-only | client source set |
| RegisterTextureAtlasesEvent | client.event | mod | client | no | client-only | client source set |
| RenderArmEvent | client.event | game | client | yes | client-only | client source set |
| RenderFrameEvent (abstract) | client.event | game | client | no | client-only | client source set |
| RenderFrameEvent.Post | client.event | game | client | no | client-only | client source set |
| RenderFrameEvent.Pre | client.event | game | client | no | client-only | client source set |
| RenderGuiEvent (abstract) | client.event | game | client | no | client-only | client source set |
| RenderGuiEvent.Post | client.event | game | client | no | client-only | client source set |
| RenderGuiEvent.Pre | client.event | game | client | yes | client-only | client source set |
| RenderGuiLayerEvent (abstract) | client.event | game | client | no | client-only | client source set |
| RenderGuiLayerEvent.Post | client.event | game | client | no | client-only | client source set |
| RenderGuiLayerEvent.Pre | client.event | game | client | yes | client-only | client source set |
| RenderHandEvent | client.event | game | client | yes | client-only | client source set |
| RenderItemInFrameEvent | client.event | game | client | yes | client-only | client source set |
| RenderLevelStageEvent (abstract) | client.event | game | client | no | client-only | client source set |
| RenderLevelStageEvent.AfterLevel | client.event | game | client | no | client-only | client source set |
| RenderLevelStageEvent.AfterOpaqueBlocks | client.event | game | client | no | client-only | client source set |
| RenderLevelStageEvent.AfterOpaqueFeatures | client.event | game | client | no | client-only | client source set |
| RenderLevelStageEvent.AfterSky | client.event | game | client | no | client-only | client source set |
| RenderLevelStageEvent.AfterTranslucentBlocks | client.event | game | client | no | client-only | client source set |
| RenderLevelStageEvent.AfterTranslucentFeatures | client.event | game | client | no | client-only | client source set |
| RenderLevelStageEvent.AfterTranslucentParticles | client.event | game | client | no | client-only | client source set |
| RenderLevelStageEvent.AfterWeather | client.event | game | client | no | client-only | client source set |
| RenderLevelStageEvent.OitTranslucent | client.event | game | client | no | client-only | client source set |
| RenderLivingEvent (abstract) | client.event | game | client | no | client-only | client source set |
| RenderLivingEvent.Post | client.event | game | client | no | client-only | client source set |
| RenderLivingEvent.Pre | client.event | game | client | yes | client-only | client source set |
| RenderNameTagEvent (abstract) | client.event | game | client | no | client-only | client source set |
| RenderNameTagEvent.CanRender | client.event | game | client | no | client-only | client source set |
| RenderNameTagEvent.DoRender | client.event | game | client | yes | client-only | client source set |
| RenderPlayerEvent (abstract) | client.event | game | client | no | client-only | client source set |
| RenderPlayerEvent.Post | client.event | game | client | no | client-only | client source set |
| RenderPlayerEvent.Pre | client.event | game | client | yes | client-only | client source set |
| RenderTooltipEvent (abstract) | client.event | game | client | no | client-only | client source set |
| RenderTooltipEvent.GatherComponents | client.event | game | client | yes | client-only | client source set |
| RenderTooltipEvent.Pre | client.event | game | client | yes | client-only | client source set |
| RenderTooltipEvent.Texture | client.event | game | client | no | client-only | client source set |
| ScreenEvent (abstract) | client.event | game | client | no | client-only | client source set |
| ScreenEvent.CharacterTyped (abstract) | client.event | game | client | no | client-only | client source set |
| ScreenEvent.CharacterTyped.Post | client.event | game | client | no | client-only | client source set |
| ScreenEvent.CharacterTyped.Pre | client.event | game | client | yes | client-only | client source set |
| ScreenEvent.Closing | client.event | game | client | no | client-only | client source set |
| ScreenEvent.Init (abstract) | client.event | game | client | no | client-only | client source set |
| ScreenEvent.Init.Post | client.event | game | client | no | client-only | client source set |
| ScreenEvent.Init.Pre | client.event | game | client | yes | client-only | client source set |
| ScreenEvent.KeyInput (abstract) | client.event | game | client | no | client-only | client source set |
| ScreenEvent.KeyPressed (abstract) | client.event | game | client | no | client-only | client source set |
| ScreenEvent.KeyPressed.Post | client.event | game | client | yes | client-only | client source set |
| ScreenEvent.KeyPressed.Pre | client.event | game | client | yes | client-only | client source set |
| ScreenEvent.KeyReleased (abstract) | client.event | game | client | no | client-only | client source set |
| ScreenEvent.KeyReleased.Post | client.event | game | client | yes | client-only | client source set |
| ScreenEvent.KeyReleased.Pre | client.event | game | client | yes | client-only | client source set |
| ScreenEvent.MouseButtonPressed (abstract) | client.event | game | client | no | client-only | client source set |
| ScreenEvent.MouseButtonPressed.Post | client.event | game | client | no | client-only | client source set |
| ScreenEvent.MouseButtonPressed.Pre | client.event | game | client | yes | client-only | client source set |
| ScreenEvent.MouseButtonReleased (abstract) | client.event | game | client | no | client-only | client source set |
| ScreenEvent.MouseButtonReleased.Post | client.event | game | client | no | client-only | client source set |
| ScreenEvent.MouseButtonReleased.Pre | client.event | game | client | yes | client-only | client source set |
| ScreenEvent.MouseDragged (abstract) | client.event | game | client | no | client-only | client source set |
| ScreenEvent.MouseDragged.Post | client.event | game | client | no | client-only | client source set |
| ScreenEvent.MouseDragged.Pre | client.event | game | client | yes | client-only | client source set |
| ScreenEvent.MouseInput (abstract) | client.event | game | client | no | client-only | client source set |
| ScreenEvent.MouseScrolled (abstract) | client.event | game | client | no | client-only | client source set |
| ScreenEvent.MouseScrolled.Post | client.event | game | client | no | client-only | client source set |
| ScreenEvent.MouseScrolled.Pre | client.event | game | client | yes | client-only | client source set |
| ScreenEvent.Opening | client.event | game | client | yes | client-only | client source set |
| ScreenEvent.Preedit (abstract) | client.event | game | client | no | client-only | client source set |
| ScreenEvent.Preedit.Post | client.event | game | client | no | client-only | client source set |
| ScreenEvent.Preedit.Pre | client.event | game | client | yes | client-only | client source set |
| ScreenEvent.Render (abstract) | client.event | game | client | no | client-only | client source set |
| ScreenEvent.Render.Background | client.event | game | client | no | client-only | client source set |
| ScreenEvent.Render.Foreground | client.event | game | client | no | client-only | client source set |
| ScreenEvent.Render.Post | client.event | game | client | no | client-only | client source set |
| ScreenEvent.Render.Pre | client.event | game | client | yes | client-only | client source set |
| ScreenEvent.RenderInventoryMobEffects | client.event | game | client | yes | client-only | client source set |
| ScreenshotEvent | client.event | game | client | yes | client-only | client source set |
| SelectMusicEvent | client.event | game | client | yes | client-only | client source set |
| SubmitCustomGeometryEvent | client.event | game | client | no | client-only | client source set |
| TextureAtlasStitchedEvent | client.event | mod | client | no | client-only | client source set |
| ToastAddEvent | client.event | game | client | yes | client-only | client source set |
| ViewportEvent (abstract) | client.event | game | client | no | client-only | client source set |
| ViewportEvent.ComputeCameraAngles | client.event | game | client | no | client-only | client source set |
| ViewportEvent.ComputeFogColor | client.event | game | client | no | client-only | client source set |
| ViewportEvent.ComputeFov | client.event | game | client | no | client-only | client source set |
| ViewportEvent.RenderFog | client.event | game | client | no | client-only | client source set |
| WindowResizeEvent | client.event | game | client | no | client-only | client source set |
| ClientLifecycleEvent (abstract) | client.event.lifecycle | game | client | no | client-only | client source set |
| ClientStartedEvent | client.event.lifecycle | game | client | no | client-only | client source set |
| ClientStoppedEvent | client.event.lifecycle | game | client | no | client-only | client source set |
| ClientStoppingEvent | client.event.lifecycle | game | client | no | client-only | client source set |
| PlaySoundEvent | client.event.sound | game | client | no | client-only | client source set |
| PlaySoundSourceEvent | client.event.sound | game | client | no | client-only | client source set |
| PlayStreamingSourceEvent | client.event.sound | game | client | no | client-only | client source set |
| SoundEngineLoadEvent | client.event.sound | mod | client | no | client-only | client source set |
| SoundEvent (abstract) | client.event.sound | game | client | no | client-only | client source set |
| SoundEvent.SoundSourceEvent (abstract) | client.event.sound | game | client | no | client-only | client source set |
| RegisterClientExtensionsEvent | client.extensions.common | mod | client | no | client-only | client source set |
| RegisterGameRuleEntryFactoryEvent | client.gamerules | mod | client | no | client-only | client source set |
| RegisterMapDecorationRenderersEvent | client.gui.map | mod | client | no | client-only | client source set |
| RegisterClientPayloadHandlersEvent | client.network.event | mod | client | no | client-only | client source set |
| RegisterPipelineModifiersEvent | client.pipeline | mod | client | no | client-only | client source set |
| RegisterRenderStateModifiersEvent | client.renderstate | mod | client | no | client-only | client source set |
| TagsUpdatedEvent.ClientPacketReceived | event | game | client | no | client-only | posted only from client code |
| FMLClientSetupEvent | fml.event.lifecycle | mod | client | no | client-only | FML client dist |

### Registries

#### Registration API (`registries`)

| API | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `BaseMappedRegistry` | class, 10 public methods | both | not supported | - | internal (`@ApiStatus.Internal`) |
| `DataMapLoader` | class, 3 public methods | both | supported | `ironpumpkin_mods::data_maps` loader, run after `DatapackManager::load_all` at startup and on `/reload` | a registry with no data map file keeps its values, as `apply` leaves it |
| `DataPackRegistriesHooks` | class, 7 public methods | both | not supported | - | internal (`@ApiStatus.Internal`) |
| `DatapackRegistryBuilder` | class, 5 public methods | both | planned | engine extension point (M5) | M5 engine extension: builds new datapack registries; see NewDatapackRegistryEvent |
| `DeferredBlock` | class, 5 public methods | both | supported | no handle type: the mod keeps the name and looks it up after the freeze with `Block::from_name` or `ironpumpkin_mods::registry` | the lookups answer for vanilla and mod entries; test `lookups_round_trip_vanilla_and_mod_entries` |
| `DeferredHolder` | class, 16 public methods | both | supported | no handle type: the mod keeps the name and looks it up after the freeze with `ironpumpkin_mods::registry` (`id_of`, `key_of`, `is_in_tag`) | the lookups answer for vanilla and mod entries; test `lookups_round_trip_vanilla_and_mod_entries` |
| `DeferredItem` | class, 5 public methods | both | supported | no handle type: the mod keeps the name and looks it up after the freeze with `Item::from_registry_key` or `ironpumpkin_mods::registry` | the lookups answer for vanilla and mod entries; test `lookups_round_trip_vanilla_and_mod_entries` |
| `DeferredRegister#create` | method, 3 overloads | both | supported | `ModInit`, one handle for every backed registry | block, item, entity type, sound event, mob effect, attribute and data component type; tests `a_mod_registers_a_block_and_an_item_that_the_registry_holds`, `the_entries_bind_after_the_vanilla_ids` |
| `DeferredRegister#createItems` | method, 1 overload | both | supported | `ModInit::register_item` with `content::ItemBuilder` | test `a_mod_registers_a_block_and_an_item_that_the_registry_holds` |
| `DeferredRegister#createBlocks` | method, 1 overload | both | supported | `ModInit::register_block` with `content::BlockBuilder` | test `a_mod_registers_a_block_and_an_item_that_the_registry_holds` |
| `DeferredRegister#createDataComponents` | method, 1 overload | both | supported | `ModInit::register_data_component_type` with `data_components::DataComponentTypeBuilder` | item stacks store mod components next to the generated vanilla ones; tests `a_mod_component_on_an_item_survives_a_save_and_reaches_a_synced_client`, `persistent_mod_components_survive_a_save_and_load` |
| `DeferredRegister#createEntities` | method, 1 overload | both | supported | `ModInit::register_entity_type` with `content::EntityTypeBuilder` | test `a_mod_registers_an_entity_type_with_its_factory` |
| `DeferredRegister#register` | method, 3 overloads | both | supported | `ModInit::register_block`, `register_item`, `register_entity_type`, `register_sound_event`, `register_mob_effect`, `register_attribute`, `register_data_component_type` | tests `a_mod_registers_a_block_and_an_item_that_the_registry_holds`, `a_mod_registers_an_entity_type_with_its_factory`, `the_entries_bind_after_the_vanilla_ids` |
| `DeferredRegister#makeRegistry` | method, 1 overload | both | planned | engine extension point (M5) | M5 engine extension: creates a new registry; see NewRegistryEvent |
| `DeferredRegister#getRegistry` | method, 1 overload | both | supported | `ModInit::registry`, a `registry::Registry` handle | the handle has the lookups of `ironpumpkin_mods::registry` (`contains`, `id_of`, `key_of`, `keys`, `is_in_tag`, `tag_entries`); a registry that `makeRegistry` would create does not exist (M5); test `the_registry_handle_answers_the_lookups_of_the_registry_module` |
| `DeferredRegister#createTagKey` | method, 2 overloads | both | supported | a tag is its name: `content::TagBuilder`, `registry::is_in_tag`, `registry::tag_entries` | test `tag_lookups_answer_for_generated_and_mod_tags` |
| `DeferredRegister#addAlias` | method, 1 overload | both | planned | aliases in the `frozen_registry` snapshot | M5 engine extension: the content registry has no alias map, so a lookup of an old name does not resolve, and the snapshot sends no aliases (`RegistrySnapshot::aliases` stays empty) |
| `DeferredRegister#getEntries` | method, 1 overload | both | supported | `ModInit::entries` inside `init`, `registry::mod_entries` after it | the names that the mod registered through `ModInit` in a registry, in registration order, without tags and without failed registrations; one list per mod and registry, so two registers of one registry give one merged list; content that a `RegisterEvent` handler registers is not listed; tests `entries_list_what_the_mod_registered_in_registration_order`, `a_mod_reads_its_entries_in_common_setup` |
| `DeferredRegister#getRegistryKey` | method, 1 overload | both | supported | `ModInit::registry_key`, the `ContentKind` of the registry | a registry is its `ContentKind`, so the key is the kind that the lookups take; test `the_registry_getters_name_each_registry` |
| `DeferredRegister#getRegistryName` | method, 1 overload | both | supported | `ModInit::registry_name` | `minecraft:block`, `minecraft:item`, `minecraft:entity_type`, `minecraft:sound_event`, `minecraft:mob_effect`, `minecraft:attribute` and `minecraft:data_component_type`; test `the_registry_getters_name_each_registry` |
| `DeferredRegister#getNamespace` | method, 1 overload | both | supported | `ModInit::namespace` | the `namespace` of `neoforge.mods.toml`, or the mod id; one per mod, not per `DeferredRegister`; after `init`, `ModList` gives it as `ModInfo::namespace`; the registrations take full names, so the mod puts the namespace in front itself; test `the_namespace_is_the_namespace_of_the_mods_toml_or_the_mod_id` |
| `DeferredRegister.Blocks#register` | method, 2 overloads | both | supported | `ModInit::register_block` with `content::BlockBuilder` | test `a_mod_registers_a_block_and_an_item_that_the_registry_holds` |
| `DeferredRegister.Blocks#registerBlock` | method, 3 overloads | both | supported | `ModInit::register_block` with `BlockBuilder::behaviour` | tests `a_mod_registers_a_block_and_an_item_that_the_registry_holds`, `the_block_registry_runs_the_behaviour_of_a_custom_block` |
| `DeferredRegister.Blocks#registerSimpleBlock` | method, 3 overloads | both | supported | `ModInit::register_block` with `BlockBuilder::new` | test `a_mod_registers_a_block_and_an_item_that_the_registry_holds` |
| `DeferredRegister.DataComponents#registerComponentType` | method, 1 overload | both | supported | `ModInit::register_data_component_type` with `DataComponentTypeBuilder::<T>::new`; the codec is the value type `T` (`pumpkin_codecs` `Encode` and `Decode`) | tests `a_mod_component_on_an_item_survives_a_save_and_reaches_a_synced_client`, `persistent_mod_components_survive_a_save_and_load` |
| `DeferredRegister.Entities#registerEntityType` | method, 2 overloads | both | supported | `ModInit::register_entity_type` with `content::EntityTypeBuilder` | test `a_mod_registers_an_entity_type_with_its_factory` |
| `DeferredRegister.Items#register` | method, 2 overloads | both | supported | `ModInit::register_item` with `content::ItemBuilder` | test `a_mod_registers_a_block_and_an_item_that_the_registry_holds` |
| `DeferredRegister.Items#registerSimpleBlockItem` | method, 6 overloads | both | supported | `ModInit::register_item` with `ItemBuilder::places` | test `a_mod_registers_a_block_and_an_item_that_the_registry_holds` |
| `DeferredRegister.Items#registerItem` | method, 3 overloads | both | supported | `ModInit::register_item` with `ItemBuilder::behaviour` | tests `a_mod_registers_a_block_and_an_item_that_the_registry_holds`, `the_item_registry_runs_the_behaviour_of_a_custom_item` |
| `DeferredRegister.Items#registerSimpleItem` | method, 3 overloads | both | supported | `ModInit::register_item` with `ItemBuilder::new` | test `a_mod_registers_a_block_and_an_item_that_the_registry_holds` |
| `GameData` | class, 9 public methods | both | not supported | - | internal (`@ApiStatus.Internal`) |
| `IRegistryExtension` | interface, 12 public methods | both | planned | `ironpumpkin_mods::registry` (`contains`, `id_of`, `key_of` for `getKey` and `getKeyOrNull`, `keys`), `data_maps::DataMapType::get_data` for `getDataMap` | M5 engine extension: partial; `addAlias`, `resolve`, `getMaxId`, `doesSync` and `addCallback` have no counterpart because the content registry has no alias map, no id limit and no registration callbacks |
| `NeoForgeRegistries` | class, 0 public methods | both | planned | the registry names of the NeoForge registries table | M5 engine extension: 2 of 10 registries are supported (`attachment_types`, `global_loot_modifier_serializers`); the others have no registration API, see the NeoForge registries table |
| `NeoForgeRegistries.Keys` | class, 0 public methods | both | planned | the registry names of the NeoForge registries table | M5 engine extension: 2 of 12 keys are supported (`attachment_types`, `global_loot_modifier_serializers`); the others have no registration API, see the NeoForge registries table |
| `NeoForgeRegistriesSetup` | class, 1 public method | both | not supported | - | internal (`@ApiStatus.Internal`) |
| `RegisterEvent.RegisterHelper#register` | method, 2 overloads | both | supported | `ironpumpkin_mods::content::RegisterEventExt` (`register_block`, `register_item`, `register_entity_type`) | registers only on the fire of its registry, as `RegisterEvent#register` does; test `register_event_fires_once_per_registry_between_init_and_common_setup` |
| `RegistryBuilder` | class, 11 public methods | both | planned | engine extension point (M5) | M5 engine extension: builds new registries; see NewRegistryEvent |
| `RegistryManager` | class, 13 public methods | both | not supported | - | internal (`@ApiStatus.Internal`) |
| `RegistryManager.SnapshotType` | enum, 0 public methods | both | not supported | - | member of `RegistryManager`, which is `@ApiStatus.Internal`; `SYNC_TO_CLIENT` is what `SYNCED_REGISTRIES` sends, and `FULL` is the Java backup that reverts a registry |
| `RegistrySnapshot` | class, 3 public methods | both | supported | `pumpkin_protocol::java::neoforge::RegistrySnapshot` (`ids`, `aliases`) | the ids of one synced registry; `aliases` stays empty (see `DeferredRegister#addAlias`), and `getFullBackup` is the Java revert backup; tests `frozen_registry_decodes_java_bytes`, `frozen_registry_writes_java_bytes`; boot test `neoforge_client_syncs_custom_content` |

#### Registries NeoForge adds (`NeoForgeRegistries.Keys`, plus `registry_order.json` entries in `neoforge:`)

| Registry | Constant | Kind | Synced | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| `neoforge:entity_data_serializers` | `ENTITY_DATA_SERIALIZERS` | static | yes | both | planned | engine extension point (M5) | M5 engine extension: custom entity data serializers; phase 3 syncs only the built-in entries |
| `neoforge:global_loot_modifier_serializers` | `GLOBAL_LOOT_MODIFIER_SERIALIZERS` | static | no | both | supported | `ModInit::register_loot_modifier_type` (`ironpumpkin_mods::loot`) | the registry is the set of modifier types, each a callback that decodes a data pack file; no `RegisterEvent` fires for it and it is not synced; tests `a_duplicate_type_fails`, `modifiers_load_from_world_data_packs_and_change_the_rolls` (`crates/ironpumpkin-mods/tests`) |
| `neoforge:biome_modifier_serializers` | `BIOME_MODIFIER_SERIALIZERS` | static | no | both | planned | engine extension point (M5) | M5 engine extension: biome modifiers change worldgen data; Pumpkin worldgen is Rust code |
| `neoforge:structure_modifier_serializers` | `STRUCTURE_MODIFIER_SERIALIZERS` | static | no | both | planned | engine extension point (M5) | M5 engine extension: structure modifiers change worldgen data; Pumpkin worldgen is Rust code |
| `neoforge:fluid_type` | `FLUID_TYPES` | static | yes | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin fluids are the generated vanilla set; phase 3 syncs only the built-in entries |
| `neoforge:holder_set_type` | `HOLDER_SET_TYPES` | static | yes | both | planned | engine extension point (M5) | M5 engine extension: custom holder set codecs in data files; the datapack loader reads vanilla holder sets only |
| `neoforge:ingredient_serializer` | `INGREDIENT_TYPES` | static | yes | both | planned | engine extension point (M5) | M5 engine extension: custom ingredient types; the recipe loader reads vanilla ingredients only |
| `neoforge:fluid_ingredient_type` | `FLUID_INGREDIENT_TYPES` | static | yes | both | planned | engine extension point (M5) | M5 engine extension: fluid ingredients belong to the NeoForge fluid API, which has no Pumpkin equivalent |
| `neoforge:condition_codecs` | `CONDITION_CODECS` | static | no | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions` |
| `neoforge:attachment_types` | `ATTACHMENT_TYPES` | static | no | both | supported | `ModInit::register_attachment_type` (`ironpumpkin_mods::attachments`) | frozen after the last `RegisterEvent`; no `RegisterEvent` fires for it; a name in the `minecraft` namespace fails with `AttachmentError::ReservedNamespace`, which `NeoForge` accepts |
| `neoforge:biome_modifier` | `BIOME_MODIFIERS` | datapack | no | both | planned | engine extension point (M5) | M5 engine extension: datapack registry of biome modifiers; Pumpkin worldgen is Rust code |
| `neoforge:structure_modifier` | `STRUCTURE_MODIFIERS` | datapack | no | both | planned | engine extension point (M5) | M5 engine extension: datapack registry of structure modifiers; Pumpkin worldgen is Rust code |
| `neoforge:synced_attachment_types` | `AttachmentSync.SYNCED_ATTACHMENT_TYPES` | static | yes | both | supported | the types with a sync handler, in registration order (`plugin::attachments::SYNCED_ATTACHMENT_TYPES`) | sent by the registry sync, so the ids of `neoforge:sync_attachments` are those of the server |

#### Static registries a `DeferredRegister` can target (95 vanilla, from `registry_order.json`)

Synced = listed in `NeoForgeRegistriesSetup.VANILLA_SYNC_REGISTRIES`, so a NeoForge client expects the M3 NeoForge handshake and registry sync `frozen_registry` snapshot for it.

| Registry | Synced | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `attribute` | yes | both | supported | `ModInit::register_attribute`, `RegisterAttribute::register_attribute` with `entity_attributes::AttributeBuilder`; values through `ironpumpkin_mods::entity_attributes` | ids after the vanilla attributes and the three `NeoForgeMod` attributes, in the `frozen_registry` sync; vanilla clients never get a mod attribute; tests `a_mod_attribute_on_a_mod_entity_has_its_base_value_and_accepts_modifiers`, `neoforge_attributes_take_the_ids_of_a_neoforge_server` |
| `data_component_type` | yes | both | supported | `ModInit::register_data_component_type`, `RegisterDataComponentType::register_data_component_type`; values through `ironpumpkin_mods::data_components` | item stacks store mod components next to the generated vanilla ones, in the `frozen_registry` sync; vanilla clients never receive them; the server writes a value in the NBT form of its codec, so the Java half must keep the default stream codec of the type (`ByteBufCodecs.fromCodecWithRegistries`): a type with its own `networkSynchronized` stream codec disconnects the client; tests `a_mod_component_on_an_item_survives_a_save_and_reaches_a_synced_client`, `persistent_mod_components_survive_a_save_and_load` |
| `particle_type` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `game_event` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `sound_event` | yes | both | supported | `ModInit::register_sound_event`, `RegisterSoundEvent::register_sound_event`; played with `sound_events::play_sound` | the sound packet carries the sound inline by name, so clients need no registry entry and the registry is not in the `frozen_registry` sync; the id of a mod sound event from `registry::id_of` is known to no client; test `a_played_mod_sound_reaches_the_players_near_the_position` |
| `fluid` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `mob_effect` | yes | both | supported | `ModInit::register_mob_effect`, `RegisterMobEffect::register_mob_effect` with `mob_effects::MobEffectBuilder` and a `MobEffect` behaviour; applied through `ironpumpkin_mods::mob_effects` | ids after the vanilla effects, in the `frozen_registry` sync; vanilla clients never get the effect packets of a mod effect; test `a_mod_effect_applied_to_a_player_ticks_and_expires` |
| `block` | yes | both | supported | `ModInit::register_block`, `RegisterEventExt::register_block`; lookups in `ironpumpkin_mods::registry` | tests `a_mod_registers_a_block_and_an_item_that_the_registry_holds`, `register_event_fires_once_per_registry_between_init_and_common_setup`, `lookups_round_trip_vanilla_and_mod_entries` |
| `debug_subscription` | yes | both | not supported | - | debug renderer subscriptions; development only |
| `entity_type` | yes | both | supported | `ModInit::register_entity_type`, `RegisterEventExt::register_entity_type`; lookups in `ironpumpkin_mods::registry` | tests `a_mod_registers_an_entity_type_with_its_factory`, `register_event_fires_once_per_registry_between_init_and_common_setup`, `lookups_round_trip_vanilla_and_mod_entries` |
| `item` | yes | both | supported | `ModInit::register_item`, `RegisterEventExt::register_item`; lookups in `ironpumpkin_mods::registry` | tests `a_mod_registers_a_block_and_an_item_that_the_registry_holds`, `register_event_fires_once_per_registry_between_init_and_common_setup`, `lookups_round_trip_vanilla_and_mod_entries` |
| `potion` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `block_entity_type` | yes | both | planned | block entities (phase 6) | M5 engine extension: no registration API for a new block entity type; `BlockEntityTypeAddBlocksEvent` adds mod blocks to the vanilla types, and the block entity reader is a fixed table |
| `custom_stat` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `chunk_status` | no | both | planned | engine extension point (M5) | M5 engine extension: world generation stages are internal to pumpkin-world |
| `rule_test` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `rule_block_entity_modifier` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `pos_rule_test` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `menu` | yes | both | planned | menus and `advanced_open_screen` (phase 6) | M5 engine extension: no registration API for a mod menu type; `ironpumpkin_mods::menus` opens the vanilla menu types |
| `recipe_type` | yes | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin crafts with a fixed set of recipe types; custom types need code in the recipe loader |
| `recipe_serializer` | yes | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin crafts with a fixed set of recipe types; custom types need code in the recipe loader |
| `position_source_type` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `command_argument_type` | yes | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin command arguments are a fixed set in pumpkin-command; clients need the type to parse the tree |
| `stat_type` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `villager_type` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `villager_profession` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `point_of_interest_type` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `memory_module_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `sensor_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `activity` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `loot_pool_entry_type` | no | both | planned | engine extension point (M5) | M5 engine extension: loot functions, conditions and entry types are a fixed set in `loot_table_loader.rs` |
| `loot_function_type` | no | both | planned | engine extension point (M5) | M5 engine extension: loot functions, conditions and entry types are a fixed set in `loot_table_loader.rs` |
| `loot_condition_type` | no | both | planned | engine extension point (M5) | M5 engine extension: loot functions, conditions and entry types are a fixed set in `loot_table_loader.rs` |
| `context_float_provider_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `context_int_provider_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `loot_nbt_provider_type` | no | both | planned | engine extension point (M5) | M5 engine extension: loot functions, conditions and entry types are a fixed set in `loot_table_loader.rs` |
| `loot_score_provider_type` | no | both | planned | engine extension point (M5) | M5 engine extension: loot functions, conditions and entry types are a fixed set in `loot_table_loader.rs` |
| `float_provider_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `int_provider_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `height_provider_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `block_predicate_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `worldgen/carver_type` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/feature_type` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/structure_placement` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/structure_piece` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/structure_type` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/placement_modifier_type` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/block_state_provider_type` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/foliage_placer_type` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/trunk_placer_type` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/root_placer_type` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/tree_decorator_type` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/feature_size_type` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/biome_source` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/chunk_generator` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/material_condition_type` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/material_rule_type` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/density_function_type` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/structure_processor` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/structure_pool_element` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `worldgen/pool_alias_binding` | no | both | planned | engine extension point (M5) | M5 engine extension: Pumpkin world generation is Rust code; plugins can hook generation phases (`handle-generate-phase`) but not add worldgen types |
| `creative_mode_tab` | no | both | not supported | `ModInit::register_creative_mode_tab` accepts the entry, logs one line and ignores it | creative tabs are client UI; the server never reads them; test `a_creative_mode_tab_is_accepted_and_ignored` |
| `trigger_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `number_format_type` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `game_rule` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `entity_sub_predicate_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `data_component_predicate_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `map_decoration_type` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `enchantment_effect_component_type` | no | both | planned | engine extension point (M5) | M5 engine extension: enchantment effect types are a fixed set in Pumpkin |
| `enchantment_level_based_value_type` | no | both | planned | engine extension point (M5) | M5 engine extension: enchantment effect types are a fixed set in Pumpkin |
| `enchantment_entity_effect_type` | no | both | planned | engine extension point (M5) | M5 engine extension: enchantment effect types are a fixed set in Pumpkin |
| `enchantment_location_based_effect_type` | no | both | planned | engine extension point (M5) | M5 engine extension: enchantment effect types are a fixed set in Pumpkin |
| `enchantment_value_effect_type` | no | both | planned | engine extension point (M5) | M5 engine extension: enchantment effect types are a fixed set in Pumpkin |
| `enchantment_provider_type` | no | both | planned | engine extension point (M5) | M5 engine extension: enchantment effect types are a fixed set in Pumpkin |
| `consume_effect_type` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `recipe_display` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `slot_display` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `recipe_book_category` | yes | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `ticket_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `incoming_rpc_methods` | no | both | planned | management JSON-RPC server (`net/management/rpc.rs`) | M5 engine extension: the management server dispatches a fixed set of methods; no hook adds one |
| `outgoing_rpc_methods` | no | both | planned | management JSON-RPC server (`net/management/rpc.rs`) | M5 engine extension: the management server sends a fixed set of notifications; no hook adds one |
| `test_environment_definition_type` | no | both | not supported | - | game test framework |
| `test_instance_type` | no | both | not supported | - | game test framework |
| `spawn_condition_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `dialog_type` | no | both | not supported | - | dialog codec types are fixed; plugins show dialogs through `java-dialogs.wit` |
| `dialog_action_type` | no | both | not supported | - | dialog codec types are fixed; plugins show dialogs through `java-dialogs.wit` |
| `input_control_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `dialog_body_type` | no | both | not supported | - | dialog codec types are fixed; plugins show dialogs through `java-dialogs.wit` |
| `permission_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `permission_check_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `environment_attribute` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `attribute_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `slot_source_type` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `context_key_set` | no | both | planned | engine extension point (M5) | M5 engine extension: no registration API; the phase 2 content registry covers block, item and entity type only |
| `test_function` | no | both | not supported | - | game test framework |

#### Other registry packages

| Package | Public types | Types | Side | Status | Reason |
|:--|:--|:--|:--|:--|:--|
| `registries.callback` | 4 | AddCallback, BakeCallback, ClearCallback, RegistryCallback | both | planned | M5 engine extension: custom registry callbacks need mutable registries |
| `registries.holdersets` | 11 | AndHolderSet, AndHolderSet.Type, AnyHolderSet, AnyHolderSet.Type, CompositeHolderSet, HolderSetType, ICustomHolderSet, NotHolderSet, NotHolderSet.Type, OrHolderSet, OrHolderSet.Type | both | planned | M5 engine extension: custom holder set types in data files; the datapack loader reads vanilla holder sets only |

### Capabilities

The `capabilities` and `transfer` modules of `ironpumpkin-mods` (task 2.7, #153). The transfer types (`ResourceHandler`, `ItemResource`, `FluidResource`, `EnergyHandler`, `ItemAccess`, `Transaction`, `SnapshotJournal`, `RootCommitJournal`) keep their NeoForge names in `ironpumpkin_mods::transfer`. A transaction belongs to its thread and is `!Send`; a journal belongs to the first root that touched it until that root closes; a rollback of a Pumpkin inventory slot gives back only what the transaction moved. Porting guide: `docs/porting-from-neoforge/capabilities.md`.

#### Built-in capabilities (`Capabilities`)

| Capability | Kind | Type and context | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|
| `Capabilities.Energy.BLOCK` | BlockCapability | EnergyHandler, @Nullable Direction | both | supported | `capabilities::energy::BLOCK` | no vanilla block provides energy; mods provide it (`SimpleEnergyHandler`) |
| `Capabilities.Energy.ENTITY` | EntityCapability | EnergyHandler, @Nullable Direction | both | supported | `capabilities::energy::ENTITY` | no vanilla entity provides energy; mods provide it |
| `Capabilities.Energy.ITEM` | ItemCapability | EnergyHandler, ItemAccess | both | supported | `capabilities::energy::ITEM` | no vanilla item provides energy; mods provide it |
| `Capabilities.Fluid.BLOCK` | BlockCapability | ResourceHandler<FluidResource>, @Nullable Direction | both | supported | `capabilities::fluid::BLOCK` | mods provide it (`FluidStacksResourceHandler`); no vanilla provider yet: the cauldron needs fluid storage, which Pumpkin lacks |
| `Capabilities.Fluid.ENTITY` | EntityCapability | ResourceHandler<FluidResource>, @Nullable Direction | both | supported | `capabilities::fluid::ENTITY` | mods provide it; vanilla has no provider |
| `Capabilities.Fluid.ITEM` | ItemCapability | ResourceHandler<FluidResource>, ItemAccess | both | supported | `capabilities::fluid::ITEM` | mods provide it; no vanilla provider yet: the bucket handler (`BucketResourceHandler`) needs the fluid storage that Pumpkin lacks |
| `Capabilities.Item.BLOCK` | BlockCapability | ResourceHandler<ItemResource>, @Nullable Direction | both | supported | `capabilities::item::BLOCK` over block entity `Inventory` (`transfer::inventory`) | vanilla providers for chests (double chests combined), the `WorldlyContainer` faces of furnaces, brewing stands and shulker boxes, and the other containers of `registerVanillaProviders`; the composter has none (Pumpkin has no composter container); hoppers, droppers, crafters and hopper minecarts ask it after the block container and before the container entities, as `HopperBlockEntity.getContainerOrHandlerAt` does; tests `a_crafter_dispenses_into_the_item_capability_of_a_block`, `a_hopper_minecart_pulls_from_the_item_capability_of_a_block`, `a_chest_has_27_slots_and_a_double_chest_54_with_the_right_half_first`, `a_furnace_exposes_the_vanilla_slots_of_each_face`, `the_block_capability_wins_over_a_chest_minecart_for_a_hopper_and_a_dropper`, `a_hopper_pulls_from_the_block_capability_before_a_chest_minecart` |
| `Capabilities.Item.ENTITY` | EntityCapability | ResourceHandler<ItemResource>, @Nullable Void | both | supported | `capabilities::item::ENTITY` | vanilla provider for players; chest boats, chest and hopper minecarts, horses and the hands and armor of living entities have none yet: Pumpkin keeps their inventories private to their modules |
| `Capabilities.Item.ENTITY_AUTOMATION` | EntityCapability | ResourceHandler<ItemResource>, @Nullable Direction | both | supported | `capabilities::item::ENTITY_AUTOMATION` | mods provide it; no vanilla provider yet: Pumpkin hoppers do not reach entities (`getEntityContainer`) |
| `Capabilities.Item.ITEM` | ItemCapability | ResourceHandler<ItemResource>, ItemAccess | both | supported | `capabilities::item::ITEM` | mods provide it; no vanilla provider yet: the shulker box and bundle items need a handler over their container component |

#### Capability API

| API | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `BaseCapability` | class, 3 public methods | both | supported | `name` of the capability keys | `typeClass` and `contextClass` are the type parameters `T` and `C` |
| `BlockCapability` | class, 7 public methods | both | supported | `capabilities::BlockCapability` (`create`, `create_sided`, `create_void`, `get_capability`, `is_proxyable`, `capabilities::get_all_proxyable`) | `getAllProxyable` returns names; test `creating_a_name_with_other_types_fails_with_type_mismatch` |
| `BlockCapabilityCache` | class, 6 public methods | both | supported | `capabilities::BlockCapabilityCache` (`create`, `create_with_listener`) | `getCapability` returns `Err(CapabilityError::InvalidCache)` where NeoForge throws; tests `a_cache_listener_runs_when_the_block_entity_goes`, `a_chunk_unload_invalidates_its_caches` |
| `Capabilities` | class, 0 public methods | both | supported | the modules `capabilities::item`, `fluid` and `energy` | holder of the constants in the capability table |
| `Capabilities.Energy` | class, 0 public methods | both | supported | `capabilities::energy` | holder of the constants in the capability table |
| `Capabilities.Fluid` | class, 0 public methods | both | supported | `capabilities::fluid` | holder of the constants in the capability table |
| `Capabilities.Item` | class, 0 public methods | both | supported | `capabilities::item` | holder of the constants in the capability table |
| `CapabilityHooks` | class, 7 public methods | both | not supported | - | internal (`@ApiStatus.Internal`); the host steps run in `RegisterCapabilitiesEvent`, the engine calls go through `pumpkin_core::plugin::capabilities` |
| `CapabilityListenerHolder` | class, 4 public methods | both | not supported | - | internal (`@ApiStatus.Internal`); `register_capability_listener` and `invalidate_capabilities` are the public part |
| `CapabilityRegistry` | class, 2 public methods | both | supported | the `create` functions of the capability keys | one registry per kind of key, with the built-in names first |
| `CapabilityRegistry.CapabilityConstructor` | interface, 1 public method | both | supported | the `create` functions of the capability keys | Rust generics replace the constructor |
| `EntityCapability` | class, 5 public methods | both | supported | `capabilities::EntityCapability` (`create`, `create_sided`, `create_void`, `get_capability`) | `Entity.getCapability` is `get_capability(&entity, context)` |
| `IBlockCapabilityProvider` | interface, 1 public method | both | supported | closure `Fn(&Arc<World>, &BlockPos, &BlockState, Option<&Arc<dyn BlockEntity>>, C)` (`capabilities::BlockProvider`) | test `a_provider_of_one_mod_answers_the_lookup_of_another` |
| `ICapabilityInvalidationListener` | interface, 1 public method | both | supported | `capabilities::CapabilityInvalidationListener` | the host invalidates on block entity placement and removal, chest type changes and chunk load and unload; a plain block calls `invalidate_capabilities` |
| `ICapabilityProvider` | interface, 1 public method | both | supported | closures `capabilities::BlockEntityProvider`, `EntityProvider` and `ItemProvider` | - |
| `ItemCapability` | class, 4 public methods | both | supported | `capabilities::ItemCapability` (`create`, `create_void`, `get_capability`, `get_capability_from`) | `ItemAccess.getCapability` is `get_capability_from(&access)` |
| `RegisterCapabilitiesEvent#registerBlock` | method, 1 overload | both | supported | `RegisterCapabilitiesEvent::register_block` | blocks by registry name; event row: see `capabilities` in the event tables |
| `RegisterCapabilitiesEvent#registerBlockEntity` | method, 1 overload | both | supported | `RegisterCapabilitiesEvent::register_block_entity` | vanilla block entity types by name; mod block entity types wait for the `block_entity_type` registry |
| `RegisterCapabilitiesEvent#isBlockRegistered` | method, 1 overload | both | supported | `RegisterCapabilitiesEvent::is_block_registered` | counts block entity providers through the block entity type of a vanilla block |
| `RegisterCapabilitiesEvent#setProxyable` | method, 1 overload | both | supported | `RegisterCapabilitiesEvent::set_proxyable` | test `unknown_targets_and_proxyable_conflicts_fail` |
| `RegisterCapabilitiesEvent#setNonProxyable` | method, 1 overload | both | supported | `RegisterCapabilitiesEvent::set_non_proxyable` | test `unknown_targets_and_proxyable_conflicts_fail` |
| `RegisterCapabilitiesEvent#registerEntity` | method, 1 overload | both | supported | `RegisterCapabilitiesEvent::register_entity` | entity types by registry name |
| `RegisterCapabilitiesEvent#isEntityRegistered` | method, 1 overload | both | supported | `RegisterCapabilitiesEvent::is_entity_registered` | - |
| `RegisterCapabilitiesEvent#registerItem` | method, 1 overload | both | supported | `RegisterCapabilitiesEvent::register_item` | items by registry name |
| `RegisterCapabilitiesEvent#isItemRegistered` | method, 1 overload | both | supported | `RegisterCapabilitiesEvent::is_item_registered` | - |

### Attachments

#### Attachment API (`attachment`)

| API | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `AttachmentHolder` | class, 5 public methods | both | supported | `attachments::AttachmentAccess` on `Entity`, `World`, `attachments::block_entity` and `attachments::chunk` | the storage is `pumpkin_world::attachments::Attachments`, with no map until the first attachment |
| `AttachmentHolder.AsField` | class, 2 public methods | both | supported | `attachments::ChunkAttachments` over the `attachments` field of `ChunkData` | the chunk holder; a proto chunk carries the attachments of the chunk it resumed from, so the relight downgrade keeps them |
| `AttachmentInternals` | class, 4 public methods | both | not supported | - | internal (`@ApiStatus.Internal`) |
| `AttachmentSync` | class, 9 public methods | both | not supported | - | internal (`@ApiStatus.Internal`) |
| `AttachmentSyncHandler` | interface, 3 public methods | both | supported | `attachments::AttachmentSyncHandler` | `sendToPlayer` and `write`; `read` runs on the client |
| `AttachmentType#builder` | method, 2 overloads | both | supported | `AttachmentTypeBuilder::new`, `AttachmentTypeBuilder::with_constructor` | the default value or the default constructor that gets the holder |
| `AttachmentType#serializable` | method, 2 overloads | both | supported | `AttachmentTypeBuilder::serializable`, `serializable_with_constructor` with `attachments::NbtSerializable` | persisted under the `neoforge:attachments` NBT key |
| `AttachmentType.Builder#serialize` | method, 3 overloads | both | supported | `AttachmentTypeBuilder::serialize`, `serialize_codec`, `serialize_codec_if` | persisted under the `neoforge:attachments` NBT key; a second serializer does not compile |
| `AttachmentType.Builder#copyOnDeath` | method, 1 overload | both | supported | `AttachmentTypeBuilder::copy_on_death` | applied by `World::respawn_player` after the respawn position event, and by the mob conversions; only on a builder with a serializer |
| `AttachmentType.Builder#copyHandler` | method, 1 overload | both | supported | `AttachmentTypeBuilder::copy_handler` | only on a builder with a serializer |
| `AttachmentType.Builder#sync` | method, 3 overloads | both | supported | `AttachmentTypeBuilder::sync`, `sync_with`, `sync_to` | the encoder of the stream codec; `neoforge:sync_attachments` |
| `AttachmentType.Builder#build` | method, 1 overload | both | supported | `ModInit::register_attachment_type` | takes the builder and returns the typed `AttachmentType<T>` |
| `IAttachmentCopyHandler` | interface, 1 public method | both | supported | `attachments::AttachmentCopyHandler` | also implemented by closures |
| `IAttachmentHolder#hasAttachments` | method, 1 overload | both | supported | `AttachmentAccess::has_attachments` | - |
| `IAttachmentHolder#hasData` | method, 2 overloads | both | supported | `AttachmentAccess::has_data` | - |
| `IAttachmentHolder#getData` | method, 2 overloads | both | supported | `AttachmentAccess::get_data`, `AttachmentAccess::with_data` | `get_data` returns a clone; a change in place goes through `with_data`, which marks a block entity or a chunk changed, or a shared value; each type has its own lock, and a use of the type of the running `with_data` panics |
| `IAttachmentHolder#getExistingData` | method, 2 overloads | both | supported | `AttachmentAccess::get_existing_data` | - |
| `IAttachmentHolder#getExistingDataOrNull` | method, 2 overloads | both | supported | `AttachmentAccess::get_existing_data` | `Option` covers both |
| `IAttachmentHolder#syncData` | method, 2 overloads | both | supported | `AttachmentAccess::sync_data` | does nothing for a type without a sync handler; the sync handler chooses the players with no lock held |
| `IAttachmentSerializer` | interface, 2 public methods | both | supported | `attachments::AttachmentSerializer`, `attachments::NbtSerializer` | the NBT compound of the attachment is the `ValueInput` and `ValueOutput` |
| `LevelAttachmentsSavedData` | class, 2 public methods | both | not supported | - | internal (`@ApiStatus.Internal`) |

#### Attachment holders (from patches and `injected-interfaces.json`)

| Minecraft class | How | Side | Status | Pumpkin type | Reason |
|:--|:--|:--|:--|:--|:--|
| `BlockEntity` | patch: extends AttachmentHolder | both | supported | `attachments::block_entity` over `World::attachments` | stored per world and position with the identity of the block entity; saved in its NBT by `World::save_block_entities` |
| `ChunkAccess` | injected IAttachmentHolder | both | supported | `ChunkData::attachments` (pumpkin-world `chunk/mod.rs`) | saved at the top level of the chunk NBT; the Pump format does not carry it |
| `Entity` | patch: extends AttachmentHolder | both | supported | `Entity::attachments` (pumpkin-core `entity/mod.rs`) | saved in the entity NBT, the player data included |
| `Level` | patch: extends AttachmentHolder | both | supported | `World::attachments` (pumpkin-core `world/mod.rs`) | saved to `data/neoforge/data_attachments.dat` of the dimension |
| `LevelChunk` | injected IAttachmentHolder | both | supported | `attachments::chunk` over `ChunkData::attachments` | the loaded chunk |

### Data maps

#### Data map API (`registries.datamaps`)

| API | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `AdvancedDataMapType` | class, 5 public methods | both | supported | `ironpumpkin_mods::data_maps::AdvancedDataMapType` (`builder`, `no_tags_reason`, dereferences to `DataMapType`) | the merger and the remover type are generic parameters of the builder; test `loader_reads_the_three_removal_forms_of_an_advanced_type` |
| `AdvancedDataMapType.Builder` | class, 5 public methods | both | supported | `AdvancedDataMapTypeBuilder` (`synced`, `merger`, `remover`, `disable_tag_support`, `build`) | `disable_tag_support` panics on a blank reason or a second call, as NeoForge throws; test `a_type_without_tag_support_drops_tag_values_but_not_tag_removals` |
| `DataMapEntry` | record, 1 public method | both | supported | the loader reads `{"value": .., "replace": ..}` or the bare value | internal to `data_maps::loader`; an entry with `neoforge:conditions` is skipped with a warning, because IronPumpkin evaluates no load conditions yet |
| `DataMapEntry.Removal` | record, 1 public method | both | supported | the loader reads a key list, and for an advanced type a list of `{"key", "remover"}` objects or a map from key to remover | internal to `data_maps::loader`; test `loader_reads_the_three_removal_forms_of_an_advanced_type` |
| `DataMapFile` | record, 1 public method | both | supported | the loader reads `replace`, `values` and `remove` of `data/<namespace>/data_maps/<registry>/<path>.json` from every enabled data pack | internal to `data_maps::loader`; the files of mod data packs load once those packs are enabled data packs |
| `DataMapType` | class, 5 public methods | both | supported | `ironpumpkin_mods::data_maps::DataMapType<T>` (`builder`, `id`, `registry_key`, `is_synced`, `mandatory_sync`, `get_data`, `with_data`, `data_map` with a `DataMapView`) | the codec is the value type (`Encode` + `Decode` of `pumpkin_codecs`); `Encode` writes the form of the network codec |
| `DataMapType.Builder` | class, 2 public methods | both | supported | `DataMapTypeBuilder` (`synced(mandatory)`, `build`) | a synced map sends the form that the value type encodes, so a separate network codec is the `Encode` of the value type |
| `DataMapValueMerger` | interface, 8 public methods | both | supported | trait `DataMapValueMerger<T>`; `DefaultMerger`, `ListMerger`, `SetMerger`, `MapMerger`, `WrappedMerger` | test `loader_applies_files_in_pack_order_with_merger_replace_and_removals` |
| `DataMapValueRemover` | interface, 1 public method | both | supported | trait `DataMapValueRemover<T>`: `None` removes the value | the remover type is `Decode`, the codec of the remover objects |
| `DataMapValueRemover.Default` | class, 3 public methods | both | supported | `DefaultRemover` | its codec reads any map, as `MapCodec.unitCodec` used as a `Codec` |
| `DataMapsUpdatedEvent.UpdateCause` | enum, 0 public methods | both | supported | `pumpkin_core::plugin::data_maps::UpdateCause` | the server fires only `ServerReload` |
| `IWithData` | interface, 1 public method | both | supported | `DataMapType::get_data(entry)` | the entry is named by its id; Pumpkin holders have no `getData` |

#### Built-in data maps (`NeoForgeDataMaps`, 11)

| Data map | Registry | Value | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|
| `ACCEPTABLE_VILLAGER_DISTANCES` | EntityType<?> | AcceptableVillagerDistance | both | supported | `ironpumpkin_mods::data_maps::builtin::ACCEPTABLE_VILLAGER_DISTANCES` | registered, loaded and synced; read by the hostile sensor of villagers with the vanilla distance table as fallback, as `VillagerHostilesSensor` does; the defaults are the vanilla table; tests `a_data_map_distance_makes_an_entity_hostile_and_vanilla_stays_the_fallback`, `a_pack_changes_the_villager_distance` |
| `BLOCK_TRANSFORM_APPENDERS` | BlockTransformer | BlockTransformAppender | both | supported | `ironpumpkin_mods::data_maps::builtin::BLOCK_TRANSFORM_APPENDERS` | registered, loaded and synced; entries join the axe, hoe and shovel transformers after the vanilla entries (`DataMapHooks.appendDatamapTransformers`); a transform with a provider other than `rule_based`, `simple` or `copy_properties`, a predicate other than `true`, `matching_blocks`, `matching_block_tag` or `all_of`, block state properties, or a mod sound is ignored with a warning, and equal transforms share one entry across reloads; no defaults, as NeoForge ships none; tests `a_pack_appends_a_transform_to_a_block_transformer`, `appenders_and_transformables_extend_the_block_transformer_after_the_vanilla_entries` |
| `MONSTER_ROOM_MOBS` | EntityType<?> | MonsterRoomMob | both | supported | `ironpumpkin_mods::data_maps::builtin::MONSTER_ROOM_MOBS` | registered, loaded and synced; read by the dungeon feature of pumpkin-world as a weighted pick (`MonsterRoomHooks`); the default weights keep the vanilla pick, so a seed generates the same dungeons; an empty map keeps the vanilla pick, where NeoForge throws; tests `a_pack_changes_the_monster_room_mobs`, `a_data_pack_weight_changes_the_spawner_mob`, `the_default_data_map_keeps_the_vanilla_pick_for_a_seed` |
| `OXIDIZABLES` | Block | Oxidizable | both | supported | `ironpumpkin_mods::data_maps::builtin::OXIDIZABLES` | registered, loaded and synced; read by the next and previous copper stage (`DataMapHooks.getNextOxidizedStage`); `generate_block_transform` adds a scrape to the axe; defaults from the vanilla weathering data, test `oxidizables_give_the_next_stage_of_copper`; a block outside the copper family gets no random tick, because the engine ticks only blocks with a behaviour; the age and the weathering copper check stay on the vanilla copper family, and the walk back has a cap; tests `a_pack_changes_the_oxidation_of_a_block`, `a_replacing_file_hides_the_defaults`, `a_cyclic_oxidizables_map_cannot_hang_the_walks`, `a_non_copper_entry_does_not_make_a_block_weathering_copper`, `a_pack_that_removes_the_first_link_keeps_the_age_of_the_next_block` |
| `PARROT_IMITATIONS` | EntityType<?> | ParrotImitation | both | supported | `ironpumpkin_mods::data_maps::builtin::PARROT_IMITATIONS` | registered, loaded and synced; read by the nearby imitation and the ambient sound of a parrot (`Parrot.getImitatedSound`); the vanilla table still names the types; only vanilla sounds play; tests `a_data_map_entry_adds_a_type_and_sets_its_sound`, `a_pack_changes_the_parrot_imitations` |
| `RAID_HERO_GIFTS` | VillagerProfession | RaidHeroGift | both | supported | `ironpumpkin_mods::data_maps::builtin::RAID_HERO_GIFTS` | registered, loaded and synced; read by `GiveGiftToHero` with the vanilla gift table as fallback; tests `a_data_map_gift_replaces_the_vanilla_table_but_not_the_baby_gift`, `a_pack_changes_the_hero_gifts` |
| `TRANSFORMABLES` | Block | Transformable | both | supported | `ironpumpkin_mods::data_maps::builtin::TRANSFORMABLES` | registered, loaded and synced; the transforms join the transformer that they name after the generated ones; tag targets are dropped by the loader; no defaults; tests `a_pack_gives_a_block_a_transform_and_drops_what_the_entry_cannot_hold` |
| `VIBRATION_FREQUENCIES` | GameEvent | VibrationFrequency | both | supported | partial: `ironpumpkin_mods::data_maps::builtin::VIBRATION_FREQUENCIES` | registered, loaded and synced; `game_event_frequency` reads the map and the defaults are the vanilla frequencies, but no engine behaviour calls it: sculk sensors are not game event listeners in Pumpkin, and the vibration system is not ported; tests `a_data_map_frequency_sets_the_vibration_and_a_missing_event_has_none`, `a_pack_changes_the_vibration_frequencies` |
| `VILLAGER_COMPOSTABLES` | Item | VillagerCompostable | both | supported | `ironpumpkin_mods::data_maps::builtin::VILLAGER_COMPOSTABLES` | registered, loaded and not synced; read by the villager that works at a composter, with no vanilla fallback, as `WorkAtComposter` does; tests `a_data_map_item_is_compostable_by_villagers_and_vanilla_has_no_fallback`, `a_pack_changes_what_villagers_compost` |
| `VILLAGER_TYPES` | Biome | BiomeVillagerType | both | supported | `ironpumpkin_mods::data_maps::builtin::VILLAGER_TYPES` | registered, loaded and synced; read when a villager takes the type of its biome (`VillagerType.byBiome`) with the vanilla biome table as fallback; tests `a_data_map_type_replaces_the_vanilla_type_of_a_biome`, `a_pack_changes_the_villager_type_of_a_biome` |
| `WAXABLES` | Block | Waxable | both | supported | `ironpumpkin_mods::data_maps::builtin::WAXABLES` | registered, loaded and synced; read by the honeycomb (`DataMapHooks.getBlockWaxed`); `generate_block_transform` adds a wax off to the axe; defaults from the vanilla waxing data; tests `a_waxable_with_generate_block_transform_gives_the_axe_a_wax_off`, `a_pack_changes_the_wax_of_a_block` |

| Package | Public types | Types | Side | Status | Reason |
|:--|:--|:--|:--|:--|:--|
| `registries.datamaps.builtin` | 12 | AcceptableVillagerDistance, BiomeVillagerType, BlockTransformAppender, MonsterRoomMob, Oxidizable, ParrotImitation, RaidHeroGift, Transformable, Transformable.Remover, VibrationFrequency, VillagerCompostable, Waxable | both | supported | value types in `ironpumpkin_mods::data_maps::builtin`; a block transform is kept as its JSON object |

### Config

#### Config API (`common.ModConfigSpec`, `common.config`, FML `config`)

| API | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `ModConfigSpec` | class, 15 public methods | both | supported | `ConfigSpec` | `ConfigSpec::builder()` builds the spec; tests `the_default_file_is_modid_type_toml_in_the_config_folder`, `an_out_of_range_value_is_clamped_and_the_file_rewritten` and `set_and_save_write_the_key_under_its_section`; gap: a value is read by path with `ModConfig::get::<T>`, not through a typed handle |
| `ModConfigSpec.BooleanValue` | class, 3 public methods | both | supported | `ModConfig::get::<bool>` | read by path, no typed handle; tests `a_boolean_accepts_the_strings_true_and_false` and `set_and_save_write_the_key_under_its_section` |
| `ModConfigSpec.Builder#define` | method, 12 overloads | both | supported | `ConfigSpecBuilder::define`, `define_validated` | tests `a_boolean_accepts_the_strings_true_and_false`, `a_path_defined_twice_is_an_invalid_spec` |
| `ModConfigSpec.Builder#defineInRange` | method, 16 overloads | both | supported | `ConfigSpecBuilder::define_in_range` | for `i32`, `i64` and `f64`; a number out of range is clamped; test `an_out_of_range_value_is_clamped_and_the_file_rewritten` |
| `ModConfigSpec.Builder#defineInList` | method, 4 overloads | both | supported | `ConfigSpecBuilder::define_in_list` | a value that is not in the list gets the default, which must be in the list; test `define_in_list_keeps_a_listed_value_and_corrects_any_other` |
| `ModConfigSpec.Builder#defineList` | method, 9 overloads | both | supported | `ConfigSpecBuilder::define_list` | elements that fail are removed, and a list with no element left gets the default; test `enums_lists_and_restart_values_follow_the_spec` |
| `ModConfigSpec.Builder#defineListAllowEmpty` | method, 8 overloads | both | supported | `ConfigSpecBuilder::define_list_allow_empty` | the correction of `define_list`: an empty list in the file stays empty, a missing key gets the default; NeoForge `correct` runs only when `test` fails, so the size range of the two list methods only limits the config screen; test `define_list_allow_empty_keeps_an_empty_list_and_defaults_a_missing_key` |
| `ModConfigSpec.Builder#defineEnum` | method, 20 overloads | both | supported | `ConfigSpecBuilder::define_enum` | the type implements `ConfigEnum`; test `enums_lists_and_restart_values_follow_the_spec` |
| `ModConfigSpec.Builder#comment` | method, 2 overloads | both | supported | `ConfigSpecBuilder::comment` | each line of the comment is one `#` line; test `a_comment_with_line_breaks_writes_one_comment_line_each` |
| `ModConfigSpec.Builder#translation` | method, 1 overload | both | supported | `ConfigSpecBuilder::translation` | read back with `ConfigSpec::translation`; test `enums_lists_and_restart_values_follow_the_spec` |
| `ModConfigSpec.Builder#worldRestart` | method, 1 overload | both | supported | `ConfigSpecBuilder::world_restart` | `get` keeps the loaded value until its server stops (`NeoForgeMod.serverStopping` calls `resetCaches(RestartType.WORLD)`), then caches the value of the file at the next `get`; tests `enums_lists_and_restart_values_follow_the_spec` and `a_second_server_reuses_the_mods_loaded_by_the_first` |
| `ModConfigSpec.Builder#gameRestart` | method, 1 overload | both | supported | `ConfigSpecBuilder::game_restart` | a synced config cannot have such a value; test `a_synced_config_cannot_need_a_game_restart` |
| `ModConfigSpec.Builder#push` | method, 2 overloads | both | supported | `ConfigSpecBuilder::push` | test `set_and_save_write_the_key_under_its_section` |
| `ModConfigSpec.Builder#pop` | method, 2 overloads | both | supported | `ConfigSpecBuilder::pop` | test `set_and_save_write_the_key_under_its_section` |
| `ModConfigSpec.Builder#configure` | method, 1 overload | both | supported | `ConfigSpec::configure` | runs the closure on a new builder and returns its result with the built spec, as the `Pair`; test `configure_returns_the_builder_result_and_the_spec` |
| `ModConfigSpec.Builder#build` | method, 1 overload | both | supported | `ConfigSpecBuilder::build` | returns a `Result`; `InvalidSpec` for a malformed definition; test `a_path_defined_twice_is_an_invalid_spec` |
| `ModConfigSpec.ConfigValue` | class, 10 public methods | both | supported | `ModConfig::get`, `set`, `save`, `get_default` | the value methods are on `ModConfig`, addressed by path; tests `set_and_save_write_the_key_under_its_section`, `an_out_of_range_value_is_clamped_and_the_file_rewritten` |
| `ModConfigSpec.DoubleValue` | class, 2 public methods | both | supported | `ModConfig::get::<f64>` | read by path, no typed handle; test `enums_lists_and_restart_values_follow_the_spec` |
| `ModConfigSpec.EnumValue` | class, 1 public method | both | supported | `ConfigEnum`, `ModConfig::get::<E>` | read by path, no typed handle; test `enums_lists_and_restart_values_follow_the_spec` |
| `ModConfigSpec.IntValue` | class, 2 public methods | both | supported | `ModConfig::get::<i32>` | read by path, no typed handle; a number out of range is clamped, also a TOML integer that does not fit in an `i32`, as `Range.correct` compares `doubleValue()`; test `an_i32_value_is_read_and_corrected_into_its_range` |
| `ModConfigSpec.ListValueSpec` | class, 3 public methods | both | not supported | - | its members (`getNewElementSupplier`, `testElement`, `getSizeRange`) serve the config screen only ("Only used by the UI!"); the list is read with `ModConfig::get::<Vec<T>>` |
| `ModConfigSpec.LongValue` | class, 2 public methods | both | supported | `ModConfig::get::<i64>` | read by path, no typed handle; test `an_out_of_range_value_is_clamped_and_the_file_rewritten` |
| `ModConfigSpec.Range` | class, 7 public methods | both | supported | `config::Range` | `new`, `min`, `max`, `test`, `correct` and the `Range:` comment text through `Display`; `Range::new` returns `None` where `Range.of` throws; `define_in_range` uses it; test `a_range_tests_corrects_and_prints_its_bounds` |
| `ModConfigSpec.RestartType` | enum, 1 public method | both | supported | `config::RestartType` (`None`, `World`, `Game`) | set with `world_restart` and `game_restart`, read with `ValueSpec::restart_type`; test `a_value_spec_gives_its_comment_default_restart_and_checks` |
| `ModConfigSpec.ValueSpec` | class, 8 public methods | both | supported | `config::ValueSpec`, `ConfigSpec::value_spec` | `path`, `comment`, `translation_key`, `restart_type`, `default_value`, `test` and `correct`; `getRange` and `getClazz` have no counterpart because the kind and the range are inside `test` and `correct`; test `a_value_spec_gives_its_comment_default_restart_and_checks` |
| `NeoForgeLocalConfig` | class, 0 public methods | both | not supported | - | NeoForge own settings; Pumpkin has its own `pumpkin.toml` |
| `NeoForgeSyncedConfig` | class, 0 public methods | both | not supported | - | NeoForge own settings; Pumpkin has its own `pumpkin.toml` |
| `ConfigTracker` | class, 7 public methods | both | not supported | - | internal (`@ApiStatus.Internal`) |
| `IConfigSpec` | interface, 5 public methods | both | not supported | - | a mod implements it to plug its own spec type into FML; `ConfigSpec` is the only spec, and `register_config` takes it; a port that only uses `ModConfigSpec` compiles against `ConfigSpec` |
| `IConfigSpec.ILoadedConfig` | interface, 2 public methods | both | not supported | - | it wraps the `NightConfig` `CommentedConfig` of a loaded file (`config`, `save`); a mod reads and writes values with `ModConfig::get`, `set` and `save` |
| `ModConfig` | class, 6 public methods | both | supported | `ModConfig` | `mod_id`, `config_type`, `file_name`, `path`, `spec`, `get`, `set`, `save` and `get_default`; tests `the_default_file_is_modid_type_toml_in_the_config_folder`, `set_and_save_write_the_key_under_its_section` |
| `ModConfig.Type` | enum, 1 public method | both | supported | `ConfigType::{Local, Synced, Startup}` | `CLIENT` never loads on a server (see the client-only row); tests in the config type table |
| `ModConfigs` | class, 4 public methods | both | supported | `config::ModConfigs` | `get_mod_configs`, `get_config_file_names`, `get_config_set` and `get_file_map` read the configs that `register_config` registered; test `mod_configs_lists_the_configs_of_a_mod_by_type_and_file` |

#### Config types (`ModConfig.Type` in FancyModLoader 12.0.8)

| Type | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|
| `ModConfig.Type.LOCAL` | both | supported | `ConfigType::Local`, file `config/<modid>-local.toml` | one copy per side, never synced; loads after the last `init` and before `common_setup`; test `local_configs_load_before_common_setup_and_synced_after_load_complete` |
| `ModConfig.Type.CLIENT` | client | client-only | - | client-only config, never loaded on a dedicated server |
| `ModConfig.Type.SYNCED` | both | supported | `ConfigType::Synced`, file `config/<modid>-synced.toml`, `neoforge:config_file` sync through a live source | the server sends it to NeoForge clients in the configuration phase; loads after `load_complete`; a reload reaches the next client, not a client already in play; tests `an_edit_reloads_within_the_interval_and_changes_the_synced_body` and `neoforge_client_syncs_custom_content` (`crates/ironpumpkin-test-mod/tests`) |
| `ModConfig.Type.STARTUP` | both | supported | `ConfigType::Startup`, file `config/<modid>-startup.toml` | loads inside `ModInit::register_config`, so `init` reads it; test `init_reads_a_startup_config_from_the_file` |

Config events (`ModConfigEvent.Loading`, `Reloading`, `Unloading`) are in the `fml.event.config` event table. Config sync is the `SyncConfig` configuration task and the `neoforge:config_file` payload in the networking section.

### Networking

#### Payload registration and sending

| API | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `PayloadRegistrar#playToClient` | method, 2 overloads | both | supported | `ironpumpkin_mods::network::PayloadRegistrar::play_to_client` (a channel id, raw bytes) | no server handler, as on the server side of NeoForge; tests `play_to_server_takes_part_in_the_negotiation_with_the_registrar_version` and `registration_refuses_what_neoforge_refuses_and_closes_after_the_step` |
| `PayloadRegistrar#playToServer` | method, 1 overload | both | supported | `ironpumpkin_mods::network::PayloadRegistrar::play_to_server` with a handler closure | the payload is raw bytes, the mod decodes it; tests `play_to_server_takes_part_in_the_negotiation_with_the_registrar_version` and `registration_refuses_what_neoforge_refuses_and_closes_after_the_step`, `a_play_payload_reaches_the_handler_of_its_channel_once` |
| `PayloadRegistrar#playBidirectional` | method, 2 overloads | both | supported | `ironpumpkin_mods::network::PayloadRegistrar::play_bidirectional` with a handler closure | the client handler stays on the client; boot test `neoforge_network_with_the_headless_client` (channel `test-mod:echo`) |
| `PayloadRegistrar#configurationToClient` | method, 2 overloads | both | supported | `ironpumpkin_mods::network::PayloadRegistrar::configuration_to_client` | boot test `neoforge_network_with_the_headless_client` (channel `test-mod:greeting`) |
| `PayloadRegistrar#configurationToServer` | method, 1 overload | both | supported | `ironpumpkin_mods::network::PayloadRegistrar::configuration_to_server` with a handler closure | the handler runs on the connection task: the configuration phase has no world tick; test `common_channels_register_in_both_phases_and_configuration_channels_in_one` |
| `PayloadRegistrar#configurationBidirectional` | method, 2 overloads | both | supported | `ironpumpkin_mods::network::PayloadRegistrar::configuration_bidirectional` with a handler closure | test `a_finish_request_while_no_mod_task_waits_disconnects_the_client`, boot test `neoforge_network_with_the_headless_client` (channel `test-mod:licence`) |
| `PayloadRegistrar#commonToClient` | method, 2 overloads | both | supported | `ironpumpkin_mods::network::PayloadRegistrar::common_to_client` | registers the channel for play and configuration, as NeoForge does; test `common_channels_register_in_both_phases_and_configuration_channels_in_one` |
| `PayloadRegistrar#commonToServer` | method, 1 overload | both | supported | `ironpumpkin_mods::network::PayloadRegistrar::common_to_server` with a handler closure | registers the channel for play and configuration, as NeoForge does; test `common_channels_register_in_both_phases_and_configuration_channels_in_one` |
| `PayloadRegistrar#commonBidirectional` | method, 2 overloads | both | supported | `ironpumpkin_mods::network::PayloadRegistrar::common_bidirectional` with a handler closure | registers the channel for play and configuration, as NeoForge does; test `common_channels_register_in_both_phases_and_configuration_channels_in_one` |
| `PayloadRegistrar#executesOn` | method, 1 overload | both | supported | `ironpumpkin_mods::network::PayloadRegistrar::executes_on(HandlerThread)` | `Main` is the world tick of the sender, `Network` the connection task; test `a_network_thread_handler_runs_on_the_read_loop_and_a_main_handler_waits_for_the_tick` |
| `PayloadRegistrar#versioned` | method, 1 overload | both | supported | `ironpumpkin_mods::network::PayloadRegistrar::versioned`, the version in the `neoforge:network` negotiation | test `play_to_server_takes_part_in_the_negotiation_with_the_registrar_version` |
| `PayloadRegistrar#optional` | method, 1 overload | both | supported | `ironpumpkin_mods::network::PayloadRegistrar::optional`, the optional flag in the `neoforge:network` negotiation | an optional channel is readable without negotiation, as `hasAdhocChannel`; tests `play_to_server_takes_part_in_the_negotiation_with_the_registrar_version`, `a_payload_on_a_required_channel_the_client_did_not_negotiate_disconnects_it` |
| `IPayloadContext#listener` | method, 1 overload | both | not supported | - | returns the Java packet listener; the native API gives the player instead |
| `IPayloadContext#connection` | method, 1 overload | both | not supported | - | returns the Netty connection object |
| `IPayloadContext#player` | method, 1 overload | both | supported | `ironpumpkin_mods::network::PayloadContext::player` | `None` in the configuration phase, where NeoForge throws; test `a_play_payload_reaches_the_handler_of_its_channel_once` |
| `IPayloadContext#reply` | method, 1 overload | both | supported | `ironpumpkin_mods::network::PayloadContext::reply` | fails with `NetworkError::ChannelNotNegotiated` where `checkPacket` throws; test `a_reply_reaches_the_client_of_the_sender` |
| `IPayloadContext#disconnect` | method, 1 overload | both | supported | `ironpumpkin_mods::network::PayloadContext::disconnect` | applied after the handler returns; test `a_handler_disconnects_the_sender` |
| `IPayloadContext#enqueueWork` | method, 2 overloads | both | supported | `ironpumpkin_mods::network::PayloadContext::enqueue_work`, which returns a `PayloadWork` | a handler on `HandlerThread::Network` hands the closure to the world tick of the player, which runs it in order with the packets of that player; a `Main` handler runs it at once, as NeoForge does; in the configuration phase NeoForge schedules it on the server thread and IronPumpkin runs it at once on the connection task, because no world tick exists; the path follows the handler thread of the channel; a task counts as a packet in the cap of 64 per tick; a closure that panics is logged, does not stop the server and gives `None`; a player who leaves drops its queued tasks and the work gives `None`; tests `a_network_handler_hands_work_to_the_tick_in_order_with_the_packets`, `a_main_handler_runs_its_work_at_once` |
| `IPayloadContext#flow` | method, 1 overload | both | supported | `ironpumpkin_mods::network::PayloadContext::flow` | always `PacketFlow::Serverbound` on the server; tests `a_play_payload_reaches_the_handler_of_its_channel_once`, `a_finish_request_while_no_mod_task_waits_disconnects_the_client` |
| `IPayloadContext#protocol` | method, 1 overload | both | supported | `ironpumpkin_mods::network::PayloadContext::protocol` | tests `a_play_payload_reaches_the_handler_of_its_channel_once`, `a_finish_request_while_no_mod_task_waits_disconnects_the_client` |
| `IPayloadContext#handle` | method, 2 overloads | both | not supported | - | re-injects a vanilla packet into the listener; Pumpkin has no re-injection path |
| `IPayloadContext#finishCurrentTask` | method, 1 overload | both | supported | `ironpumpkin_mods::network::PayloadContext::finish_current_task` | a request for a task that is not current disconnects with `disconnect.genericReason` and the text `Internal Exception: java.lang.IllegalStateException: Unexpected request for task finish, current task: X, requested: Y`; NeoForge disconnects with that text for a `Network` handler, and for the default `Main` handler it only logs the exception and the task keeps waiting; tests `a_finish_request_while_no_mod_task_waits_disconnects_the_client`, `a_configuration_payload_on_an_unregistered_channel_reaches_the_plugin_event`, boot test `neoforge_network_with_the_headless_client` |
| `IPayloadContext#channelHandlerContext` | method, 1 overload | both | not supported | - | Netty internals |
| `PacketDistributor#sendToPlayer` | method, 1 overload | both | supported | `ironpumpkin_mods::network::send_to_player` | fails with `NetworkError::ChannelNotNegotiated` for a client without the channel; test `send_to_player_fails_for_a_vanilla_client_and_broadcasts_skip_it` |
| `PacketDistributor#sendToPlayersInDimension` | method, 1 overload | both | supported | `ironpumpkin_mods::network::send_to_players_in_dimension` | skips a client without the channel and returns the count; test `send_to_player_fails_for_a_vanilla_client_and_broadcasts_skip_it` |
| `PacketDistributor#sendToPlayersNear` | method, 1 overload | both | supported | `ironpumpkin_mods::network::send_to_players_near` (`World::for_each_player_near`) | the players of the world strictly closer than the radius, except the excluded one, as `PlayerList.broadcast`; skips a client without the channel and returns the count; test `send_to_players_near_skips_the_excluded_the_far_and_the_vanilla` |
| `PacketDistributor#sendToAllPlayers` | method, 1 overload | both | supported | `ironpumpkin_mods::network::send_to_all_players` | skips a client without the channel and returns the count; test `send_to_player_fails_for_a_vanilla_client_and_broadcasts_skip_it` |
| `PacketDistributor#sendToPlayersTrackingEntity` | method, 1 overload | both | supported | `ironpumpkin_mods::network::send_to_players_tracking_entity` (`TrackedEntity::java_viewers` of the entity tracker) | the viewers of the entity, not the entity; an untracked entity has none; skips a client without the channel and returns the count; test `send_to_players_tracking_entity_reaches_the_viewers_only` |
| `PacketDistributor#sendToPlayersTrackingEntityAndSelf` | method, 1 overload | both | supported | `ironpumpkin_mods::network::send_to_players_tracking_entity_and_self` | the viewers and the entity when it is a player; an untracked entity gets nothing, as in `ChunkMap`; test `send_to_players_tracking_entity_and_self_adds_the_entity_when_it_is_a_player` |
| `PacketDistributor#sendToPlayersTrackingChunk` | method, 1 overload | both | supported | `ironpumpkin_mods::network::send_to_players_tracking_chunk` (`World::for_each_player_tracking_chunk`) | the players with the chunk in their view that the server sent, as `ChunkMap.getPlayers(pos, false)`; test `send_to_players_tracking_chunk_needs_the_chunk_in_view_and_held` |

#### Payloads (`network/payload`, 25 channel ids)

Codec = a codec for the channel exists in `crates/pumpkin-protocol/src/java/neoforge/` (NeoForge-aware networking, #8), or in the module of `ironpumpkin-mods` that sends the payload.

| Channel | Class | Codec | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|
| `neoforge:advanced_add_entity` | AdvancedAddEntityPayload | no | both | planned | spawn data of a mod entity type (`IEntityWithComplexSpawn`) | M5 engine extension: no codec yet and nothing sends it; the spawn packet of a mod entity type has no hook for extra data |
| `neoforge:advanced_container_set_data` | AdvancedContainerSetDataPayload | no | both | planned | 32-bit data slots of a mod menu | M5 engine extension: no codec yet; `ironpumpkin_mods::menus` sends the data slots of a vanilla menu type as 16-bit values, as vanilla does |
| `neoforge:advanced_open_screen` | AdvancedOpenScreenPayload | no | both | planned | extra data of `openMenu` and mod menu types | M5 engine extension: no codec yet; `menus::open_menu` opens vanilla menu types, which the client builds itself, and a mod menu type has no registration API |
| `neoforge:auxiliary_light_data` | AuxiliaryLightDataPayload | no | both | planned | engine extension point (M5) | M5 engine extension: no codec yet; per-position light from block entities (AuxiliaryLightManager); Pumpkin lighting has no such hook |
| `c:register` | CommonRegisterPayload | yes | both | supported | `ConfigurationTask::CommonRegister`; codec in `pumpkin-protocol/src/java/neoforge/common.rs` | tests `register_decodes_java_bytes`, `register_writes_java_bytes`, `common_register_reply_keeps_the_client_play_channels`; boot test `neoforge_client_syncs_custom_content` |
| `c:version` | CommonVersionPayload | yes | both | supported | `ConfigurationTask::CommonVersion`; codec in `pumpkin-protocol/src/java/neoforge/common.rs` | tests `version_decodes_java_bytes`, `version_writes_java_bytes`, `common_version_reply_with_version_one_is_accepted`, `common_version_reply_without_version_one_disconnects` |
| `neoforge:config_file` | ConfigFilePayload | yes | both | supported | `ConfigurationTask::SyncConfig`; codec in `pumpkin-protocol/src/java/neoforge/config_file.rs` | tests `decodes_java_bytes`, `writes_java_bytes` (`config_file.rs`), `neoforge_config_is_the_captured_file`, `mod_configs_follow_the_neoforge_config`; boot test `neoforge_client_syncs_custom_content` |
| `neoforge:extensible_enum_ack` | ExtensibleEnumAcknowledgePayload | yes | both | supported | `ConfigurationTask::ExtensibleEnums`; codec in `pumpkin-protocol/src/java/neoforge/extensible_enums.rs` | client acknowledgement, empty; the task waits for it; tests `ack_is_empty`, `each_modded_task_waits_for_its_reply` |
| `neoforge:extensible_enum_data` | ExtensibleEnumDataPayload | yes | both | supported | `ConfigurationTask::ExtensibleEnums`; codec in `pumpkin-protocol/src/java/neoforge/extensible_enums.rs` | the 9 networked enums without extension data; tests `enum_entries_equal_the_captured_payload`, `decodes_captured_payload`, `writes_captured_payload_back` |
| `neoforge:feature_flags_ack` | FeatureFlagAcknowledgePayload | yes | both | supported | `ConfigurationTask::FeatureFlags`; codec in `pumpkin-protocol/src/java/neoforge/feature_flags.rs` | client acknowledgement, empty; the task waits for it; tests `ack_is_empty`, `each_modded_task_waits_for_its_reply` |
| `neoforge:feature_flags` | FeatureFlagDataPayload | yes | both | supported | `ConfigurationTask::FeatureFlags`; codec in `pumpkin-protocol/src/java/neoforge/feature_flags.rs` | the server sends the empty set; tests `the_default_payload_is_the_captured_empty_payload`, `decodes_captured_payload`, `writes_java_bytes` |
| `neoforge:frozen_registry` | FrozenRegistryPayload | yes | both | supported | `ConfigurationTask::RegistrySync`; codec in `pumpkin-protocol/src/java/neoforge/registry.rs` | one snapshot per synced registry; tests `frozen_registry_decodes_java_bytes`, `frozen_registry_writes_java_bytes`; boot test `neoforge_client_syncs_custom_content` (ids of the mod content and the echo before the known packs) |
| `neoforge:frozen_registry_sync_completed` | FrozenRegistrySyncCompletedPayload | yes | both | supported | `ConfigurationTask::RegistrySync`; codec in `pumpkin-protocol/src/java/neoforge/registry.rs` | no body; the client echoes it; tests `known_packs_wait_for_the_registry_sync_echo`; boot test `neoforge_client_syncs_custom_content` (ids of the mod content and the echo before the known packs) |
| `neoforge:frozen_registry_sync_start` | FrozenRegistrySyncStartPayload | yes | both | supported | `ConfigurationTask::RegistrySync`; codec in `pumpkin-protocol/src/java/neoforge/registry.rs` | the names of the synced registries; tests `sync_start_decodes_java_bytes`, `sync_start_writes_java_bytes`; boot test `neoforge_client_syncs_custom_content` (ids of the mod content and the echo before the known packs) |
| `neoforge:known_registry_data_maps` | KnownRegistryDataMapsPayload | yes | both | supported | `pumpkin_protocol::java::neoforge::KnownRegistryDataMapsPayload`, built from the registered data map types | every registry with data map types and its synced types; test `builtin_types_match_the_known_data_maps_of_neoforge` |
| `neoforge:known_registry_data_maps_reply` | KnownRegistryDataMapsReplyPayload | yes | both | supported | `pumpkin_protocol::java::neoforge::KnownRegistryDataMapsReplyPayload`, kept per connection | client reply; the server sends the values of the maps it names |
| `minecraft:register` | MinecraftRegisterPayload | yes | both | supported | `PlayerRegisterChannelEvent` | parsed in play; in configuration with `detect_neoforge_clients` |
| `minecraft:unregister` | MinecraftUnregisterPayload | yes | both | supported | `PlayerUnregisterChannelEvent` | parsed in play; in configuration with `detect_neoforge_clients` |
| `neoforge:network` | ModdedNetworkPayload | yes | both | supported | sent after the channel negotiation (`initialize_neoforge_connection`); codec in `pumpkin-protocol/src/java/neoforge/network.rs` | tests `network_decodes_java_bytes`, `network_writes_java_bytes`, `negotiate_agrees_on_the_same_version`; the expected sequences `pumpkin-neoforge.txt` and `pumpkin-neoforge-test-mod.txt` hold it, checked by boot test `neoforge_client_syncs_custom_content` |
| `neoforge:register` | ModdedNetworkQueryPayload | yes | both | supported | NeoForge client detection (#9, `net/java/pending.rs`) | probe and channel list |
| `neoforge:modded_network_setup_failed` | ModdedNetworkSetupFailedPayload | yes | both | supported | sent when the channel negotiation fails, before the disconnect; codec in `pumpkin-protocol/src/java/neoforge/network.rs` | tests `setup_failed_decodes_java_bytes`, `setup_failed_writes_java_bytes`, `negotiate_fails_on_a_missing_required_channel`, `a_neoforge_client_without_a_required_mod_channel_fails_the_negotiation` (`crates/pumpkin-core/tests`) |
| `neoforge:recipe_content` | RecipeContentPayload | no | both | not supported | - | no codec yet; not in the phase 3 scope; NeoForge clients then lack full recipe data for recipe viewer mods |
| `neoforge:registry_data_map_sync` | RegistryDataMapSyncPayload | yes | both | supported | `pumpkin_protocol::java::neoforge::RegistryDataMapSyncPayload`, sent by `ironpumpkin_mods::data_maps` on join and after `/reload` | one payload per registry with the synced maps the client named; tests `sync_writes_java_bytes`, `sync_sends_the_synced_maps_the_client_knows` |
| `neoforge:split` | SplitPacketPayload | yes | both | supported | split of server packets above the limit and join of client parts; codec in `pumpkin-protocol/src/java/neoforge/split.rs` | both only with a declared channel; tests `splits_a_9_mib_packet`, `never_splits_without_the_channel`, `joins_three_parts_once`, `split_packets_are_joined_only_with_detection_and_a_declared_channel` |
| `neoforge:sync_attachments` | SyncAttachmentsPayload | yes | both | supported | `ironpumpkin_mods::attachments`; the codec is `attachments/sync.rs`, not in `pumpkin-protocol` | sent to a client that negotiated the channel, on a change of a synced attachment and on the first sight of the holder; tests `payload_matches_the_neoforge_codec`, `a_synced_change_reaches_neoforge_clients_only`, `a_new_viewer_gets_the_synced_values` |

#### Configuration tasks and other network types

| Type | Package | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|
| `CheckExtensibleEnums` | network.configuration | configuration task | both | supported | `ConfigurationTask::ExtensibleEnums` with `configuration_payloads::extensible_enum_data` | sends the 9 networked enums of `NeoForge` 26.3 without extension data and waits for the client acknowledgement; a mod that extends an enum stays excluded (the `enumExtensions` key is not supported); tests `enum_entries_equal_the_captured_payload`, `each_modded_task_waits_for_its_reply`, `neoforge_client_gets_the_modded_tasks_after_the_resource_pack`; boot test `neoforge_client_syncs_custom_content` (`crates/ironpumpkin-test-mod/tests`) |
| `CheckExtensibleEnums.EnumEntry` | network.configuration | record | both | supported | `pumpkin_protocol::java::neoforge::EnumEntry` | one networked enum: class name, `NetworkCheck` and no extension data; tests `decodes_captured_payload`, `writes_captured_payload_back` (`extensible_enums.rs`) |
| `CheckExtensibleEnums.ExtensionData` | network.configuration | record | both | not supported | - | Java-only: member of `CheckExtensibleEnums`, which is `@ApiStatus.Internal`; the server never sends it because extensible enums stay excluded; the codec `pumpkin_protocol::java::neoforge::ExtensionData` reads and writes it, tests `extended_decodes_java_bytes`, `extended_writes_java_bytes` |
| `CheckFeatureFlags` | network.configuration | configuration task | both | supported | `ConfigurationTask::FeatureFlags` | sends the empty set of modded feature flags and waits for the client acknowledgement; mod feature flags stay excluded (the `featureFlags` key is not supported); tests `the_default_payload_is_the_captured_empty_payload`, `each_modded_task_waits_for_its_reply`, `neoforge_connection_without_the_check_channels_skips_their_tasks`; boot test `neoforge_client_syncs_custom_content` |
| `CommonRegisterTask` | network.configuration | configuration task | both | supported | `ConfigurationTask::CommonRegister` | sends `c:register` version 1 with the optional server-bound play channels and keeps the channels of the client; tests `common_register_reply_keeps_the_client_play_channels`, `common_tasks_need_both_common_channels`; boot test `neoforge_client_syncs_custom_content` compares the body with run (b) of the capture |
| `CommonVersionTask` | network.configuration | configuration task | both | supported | `ConfigurationTask::CommonVersion` | sends the supported common versions (1) and disconnects a client that has none; tests `common_version_reply_with_version_one_is_accepted`, `common_version_reply_without_version_one_disconnects`, `malformed_common_version_reply_is_an_error`, `the_version_kick_lists_the_supported_versions` |
| `ICustomConfigurationTask` | network.configuration | interface | both | supported | `ironpumpkin_mods::network::CustomConfigurationTask` with `TaskContext` | the tasks of the mods run after the tasks of NeoForge, in registration order; tests `mod_tasks_run_after_the_neoforge_tasks_in_registration_order`, `a_mod_task_blocks_the_finish_until_its_finish_request`, boot test `neoforge_network_with_the_headless_client` |
| `RegistryDataMapNegotiation` | network.configuration | configuration task | both | supported | `pumpkin_core::plugin::data_maps`: the known maps for a `NeoForge` client, the refusal of a vanilla client and of a `NeoForge` client without the channel when a map is `mandatory_sync`; a mandatory map turns on the client detection | the `NeoForge` client checks the mandatory maps and refuses itself, as on `NeoForge`; test `a_vanilla_client_is_refused_only_for_mandatory_maps` |
| `SyncConfig` | network.configuration | configuration task | both | supported | `ConfigurationTask::SyncConfig` with `configuration_payloads::config_files` | sends `neoforge-synced.toml` and the file of each synced config of the native mods, in mod id order, without a reply; tests `neoforge_config_is_the_captured_file`, `mod_configs_follow_the_neoforge_config`; boot test `neoforge_client_syncs_custom_content` (an edited file reaches the next client) |
| `SyncRegistries` | network.configuration | configuration task | both | supported | `ConfigurationTask::RegistrySync` with `SYNCED_REGISTRIES` | sends the start payload, one snapshot per synced registry and the completed payload, and the known packs wait for the client echo; tests `neoforge_connection_syncs_registries_before_the_known_packs`, `known_packs_wait_for_the_registry_sync_echo`; boot test `neoforge_client_syncs_custom_content` |
| `IPayloadHandler` | network.handling | interface | both | supported | `ironpumpkin_mods::network::PayloadHandler`, a closure `Fn(&[u8], &mut PayloadContext)` | per-channel dispatch; test `a_play_payload_reaches_the_handler_of_its_channel_once` |
| `MainThreadPayloadHandler` | network.handling | record | both | supported | `ironpumpkin_mods::network::HandlerThread::Main` | the world tick of the sender in play, the connection task in configuration; test `a_network_thread_handler_runs_on_the_read_loop_and_a_main_handler_waits_for_the_tick` |
| `QueuedPacket` | network.handling | interface | both | not supported | - | internal (`@ApiStatus.Internal`); phase 3 reimplements the wire behaviour, see the phase 1 design |
| `QueuedPacket.CustomPayload` | network.handling | record | both | not supported | - | internal packet queue record |
| `ServerPayloadContext` | network.handling | record | both | not supported | - | internal (`@ApiStatus.Internal`); phase 3 reimplements the wire behaviour, see the phase 1 design |
| `ChannelAttributes` | network.registration | class | both | not supported | - | internal (`@ApiStatus.Internal`); phase 3 reimplements the wire behaviour, see the phase 1 design |
| `HandlerThread` | network.registration | enum | both | supported | `ironpumpkin_mods::network::HandlerThread` | test `a_network_thread_handler_runs_on_the_read_loop_and_a_main_handler_waits_for_the_tick` |
| `ModdedConfigurationPayloadRegistration` | network.registration | record | both | not supported | - | internal (`@ApiStatus.Internal`); phase 3 reimplements the wire behaviour, see the phase 1 design |
| `ModdedPlayPayloadRegistration` | network.registration | record | both | not supported | - | internal (`@ApiStatus.Internal`); phase 3 reimplements the wire behaviour, see the phase 1 design |
| `NetworkChannel` | network.registration | record | both | not supported | - | internal (`@ApiStatus.Internal`); phase 3 reimplements the wire behaviour, see the phase 1 design |
| `NetworkPayloadSetup` | network.registration | record | both | not supported | - | internal (`@ApiStatus.Internal`); phase 3 reimplements the wire behaviour, see the phase 1 design |
| `NetworkRegistry` | network.registration | class | both | not supported | - | internal (`@ApiStatus.Internal`); phase 3 reimplements the wire behaviour, see the phase 1 design |
| `PayloadRegistration` | network.registration | record | both | not supported | - | internal (`@ApiStatus.Internal`); phase 3 reimplements the wire behaviour, see the phase 1 design |
| `ConfigSync` | network | class | both | not supported | - | internal (`@ApiStatus.Internal`); phase 3 reimplements the wire behaviour, see the phase 1 design |
| `ConfigurationInitialization` | network | class | both | not supported | - | internal (`@ApiStatus.Internal`); phase 3 reimplements the wire behaviour, see the phase 1 design |
| `DualStackUtils` | network | class | both | not supported | - | IPv4 and IPv6 address helpers for the client connect screen |
| `IContainerFactory` | network | interface | both | planned | menus and `advanced_open_screen` | M5 engine extension: builds a menu from the extra data of `neoforge:advanced_open_screen`; `ironpumpkin_mods::menus` opens vanilla menu types only, and a mod menu type has no registration API |

#### Other network packages

| Package | Public types | Types | Side | Status | Reason |
|:--|:--|:--|:--|:--|:--|
| `network.bundle` | 1 | BundlePacketUtils | both | not supported | bundle packet helpers of the Java pipeline |
| `network.codec` | 1 | NeoForgeStreamCodecs | both | not supported | Java stream codec helpers; the native API uses pumpkin-protocol codecs |
| `network.connection` | 2 | ConnectionType, ConnectionUtils | both | not supported | connection type lookups; phase 1 records the connection type in `pending.rs` |
| `network.filters` | 6 | DynamicChannelHandler, GenericPacketSplitter, GenericPacketSplitter.RemoteCompatibility, NetworkFilters, VanillaConnectionNetworkFilter, VanillaPacketFilter | both | not supported | Netty pipeline filters for vanilla connections |
| `network.handlers` | 1 | ServerPayloadHandler | both | not supported | NeoForge own payload handlers |
| `network.negotiation` | 5 | NegotiableNetworkComponent, NegotiatedNetworkComponent, NegotiationResult, NetworkComponentNegotiator, NetworkComponentNegotiator.ComponentNegotiationResult | both | not supported | Java negotiation code; #31 reimplements the rules in Rust |

### Lifecycle, entry points and metadata

FML lifecycle events (`FMLConstructModEvent`, `FMLCommonSetupEvent`, `FMLDedicatedServerSetupEvent`, `FMLClientSetupEvent`, `FMLLoadCompleteEvent`, `InterModEnqueueEvent`, `InterModProcessEvent`) are in the `fml.event.lifecycle` event table. FancyModLoader version: 12.0.8.

#### Entry point annotations

| Annotation member | Type | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `@Mod(value)` | String | both | supported | `NativeMod` entry point (`register_mod!`) with the mod id | the mod id is `NativeMod::id` or the `modId` of `mods_toml`; tests `collects_registered_mods_in_id_order_and_runs_init`, `the_mods_toml_decides_the_metadata` |
| `@Mod(dist)` | Dist[], default `{ Dist.CLIENT, Dist.DEDICATED_SERVER }` | both | supported | `mod_list::dist()` | always `Dist::DedicatedServer`; client-dist entry points never run; test `mod_list_answers_like_neoforge` |
| `@Mod(depends)` | String[], default `{}` | both | supported | `mods_toml::ModInfo::dependencies` | ids, types, ranges, ordering and side; test `parses_the_client_mod_descriptor` |
| `@EventBusSubscriber(value)` | Dist[], default `{ Dist.CLIENT, Dist.DEDICATED_SERVER }` | both | not supported | - | the annotation drives the scan of `IEventBus#register`; Rust has no scan, and a client-dist handler has no server counterpart because `dist()` is always `DedicatedServer` |
| `@EventBusSubscriber(modid)` | String, default `""` | both | not supported | - | the annotation drives the scan of `IEventBus#register`; Rust has no scan, and every handler of `ModInit::register_event` belongs to the mod of that `ModInit` |
| `@SubscribeEvent(priority)` | EventPriority, default `EventPriority.NORMAL` | both | supported | `ModInit::register_event` `priority` argument (`EventPriority`, same five levels) | blocking handlers run before non-blocking ones, each from Highest to Lowest, in registration order inside one priority; tests `priority_order`, `blocking_before_non_blocking` |
| `@SubscribeEvent(receiveCanceled)` | boolean, default `false` | both | supported | `ModInit::register_event` `receive_canceled` argument | a handler without the flag does not run for a cancelled event; `Context::register_event` (Wasm) always receives cancelled events; tests `receive_canceled`, `register_and_register_event_receive_cancelled_events` |

#### Event bus and mod container (eventbus 8.0.5, FancyModLoader 12.0.8)

| API | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IEventBus#register` | method, 1 overload | both | not supported | - | it scans an object for `@SubscribeEvent` methods; Rust has no annotation scan, so a mod calls `ModInit::register_event` once for each handler |
| `IEventBus#addListener` | method, 8 overloads | both | supported | `ModInit::register_event` (native), `ModInit::register_parent_event` (parent class), `Context::register_event` (Wasm) | each NeoForge event type is a Pumpkin event or maps to one; tests `priority_order`, `blocking_before_non_blocking`, `receive_canceled`, `handler_on_a_parent` |
| `IEventBus#unregister` | method, 1 overload | both | not supported | - | Pumpkin plugins register handlers for the plugin lifetime |
| `IEventBus#post` | method, 2 overloads | both | supported | `PluginManager::fire`, `fire_blocking` (`server.plugin_manager`) | a mod defines its own event with `impl Payload` and posts it; the handlers of every mod receive it by priority; test `a_mod_posts_its_own_event_and_another_mod_handles_it` (`crates/ironpumpkin-mods/tests/mod_event_post.rs`) |
| `IEventBus#start` | method, 1 overload | both | not supported | - | internal FML bus start |
| `EventPriority` | enum: HIGHEST, HIGH, NORMAL, LOW, LOWEST | both | supported | WIT `event-priority` | same five levels |
| `ModContainer#getModId` | method, 1 overload | both | supported | plugin-metadata `name` (`metadata.wit`) | available today |
| `ModContainer#getNamespace` | method, 1 overload | both | supported | `mods_toml::ModInfo::namespace` | the `namespace` key, or the mod id; test `mod_list_answers_like_neoforge` |
| `ModContainer#getModInfo` | method, 1 overload | both | supported | plugin-metadata (`metadata.wit`) | available today |
| `ModContainer#getCustomExtension` | method, 1 overload | both | client-only | - | extension points are client screens (config screen factory) |
| `ModContainer#registerExtensionPoint` | method, 2 overloads | both | client-only | - | extension points are client screens (config screen factory) |
| `ModContainer#registerConfig` | method, 2 overloads | both | supported | `ModInit::register_config`, `ModInit::register_config_with_file_name` | tests `the_default_file_is_modid_type_toml_in_the_config_folder`, `a_second_config_with_the_same_file_is_a_duplicate` |
| `ModContainer#getEventBus` | method, 1 overload | both | not supported | - | a mod has no bus object: the lifecycle events are `NativeMod` methods, and a game event handler registers with `ModInit::register_event` |
| `ModContainer#acceptEvent` | method, 2 overloads | both | not supported | - | internal FML dispatch |
| `ModList#of` | method, 1 overload | both | not supported | - | mod files and scan data describe Java jars |
| `ModList#get` | method, 1 overload | both | supported | `ModList::get()` | returns the process-wide list that `init_mods` builds; test `mod_list_get_lists_the_mods_that_init_mods_loaded` (`crates/ironpumpkin-mods/tests/mod_event_post.rs`) |
| `ModList#getModFiles` | method, 1 overload | both | not supported | - | mod files and scan data describe Java jars |
| `ModList#getModFileById` | method, 1 overload | both | not supported | - | mod files and scan data describe Java jars |
| `ModList#getModContainerById` | method, 1 overload | both | supported | `ModList::get_mod_container_by_id` | test `mod_list_answers_like_neoforge` |
| `ModList#getMods` | method, 1 overload | both | supported | `ModList::get_mods` | test `mod_list_answers_like_neoforge` |
| `ModList#isLoaded` | method, 1 overload | both | supported | `ModList::is_loaded` | the system mods `minecraft` and `neoforge` count as loaded; test `mod_list_answers_like_neoforge` |
| `ModList#size` | method, 1 overload | both | supported | `ModList::size` | test `mod_list_answers_like_neoforge` |
| `ModList#getAllScanData` | method, 1 overload | both | not supported | - | mod files and scan data describe Java jars |
| `ModList#forEachModFile` | method, 1 overload | both | not supported | - | mod files and scan data describe Java jars |
| `ModList#applyForEachModFile` | method, 1 overload | both | not supported | - | mod files and scan data describe Java jars |
| `ModList#applyForEachModFileAlphabetical` | method, 1 overload | both | not supported | - | mod files and scan data describe Java jars |
| `ModList#forEachModContainer` | method, 1 overload | both | supported | `ModList::get_mods` (iterate) | no callback method: iterate the slice; test `mod_list_answers_like_neoforge` covers the slices |
| `ModList#getSortedMods` | method, 1 overload | both | supported | `ModList::get_sorted_mods` | test `mod_list_answers_like_neoforge` |
| `ModList#forEachModInOrder` | method, 1 overload | both | supported | `ModList::get_sorted_mods` (iterate) | no callback method: iterate the slice; test `mod_list_answers_like_neoforge` covers the slices |
| `ModList#applyForEachModContainer` | method, 1 overload | both | supported | `ModList::get_mods` (iterate) | no callback method: iterate the slice; test `mod_list_answers_like_neoforge` covers the slices |
| `ModList#clear` | method, 1 overload | both | not supported | - | mod files and scan data describe Java jars |

#### `neoforge.mods.toml` keys (44, every `getConfigElement` and `getConfigList` call in FML and NeoForge)

| Section | Key | Read by | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|
| [[mods]] | `enumExtensions` | FML `FMLLoader` | both | not supported | - | extensible enums stay excluded (risk: feature flags and extensible enums) |
| file | `modLoader` | FML `ModFileInfo` | both | not supported | - | selects the Java language loader; a ported plugin is Rust |
| file | `loaderVersion` | FML `ModFileInfo` | both | not supported | - | FML version range |
| file | `license` | FML `ModFileInfo` | both | supported | `mods_toml::parse` | required: a missing or blank license stops the server with "Missing license", as FML `ModFileInfo` does; the value is not kept; test `a_missing_or_blank_license_is_rejected` |
| file | `showAsResourcePack` | FML `ModFileInfo` | both | not supported | - | pack listing in the client UI |
| file | `showAsDataPack` | FML `ModFileInfo` | both | not supported | - | pack listing in the client UI |
| file | `services` | FML `ModFileInfo` | both | not supported | - | Java ServiceLoader declarations |
| file | `properties` | FML `ModFileInfo` | both | supported | `mods_toml::ModInfo::file_properties` | the `[properties]` table of the file, as a TOML table; test `properties_are_kept_for_the_file` |
| file | `issueTrackerURL` | FML `ModFileInfo`, NeoForge `CrashReportExtender` | both | supported | `mods_toml::ModInfo::file_issue_tracker_url`, `ModInfo::issues_url` | file level, parsed as a URL: a blank or placeholder value is absent and a value without a scheme stops the load, as `StringUtils.toURL`; `issues_url` returns the key of the mod, else the key of the file; test `issue_tracker_urls_are_read_at_file_and_mod_level` |
| file | `mods` | FML `ModFileInfo` | both | supported | `NativeMod::mods_toml`, `mods_toml::parse` | a crate that registers several mods with `register_mod!` gives each the same text, and each picks the `[[mods]]` entry with its id, or the only entry; test `picks_the_entry_of_the_mod` |
| file | `mixins` | FML `ModFileParser` | both | not supported | - | Mixin configuration; Java bytecode patching |
| [[mixins]] | `config` | FML `ModFileParser` | both | not supported | - | Mixin configuration; Java bytecode patching |
| [[mixins]] | `requiredMods` | FML `ModFileParser` | both | not supported | - | Mixin configuration; Java bytecode patching |
| [[mixins]] | `behaviorVersion` | FML `ModFileParser` | both | not supported | - | Mixin configuration; Java bytecode patching |
| file | `accessTransformers` | FML `ModFileParser` | both | not supported | - | access transformers; Java bytecode patching |
| [[accessTransformers]] | `file` | FML `ModFileParser` | both | not supported | - | access transformers; Java bytecode patching |
| [[mods]] | `modId` | FML `ModInfo` | both | supported | `mods_toml::ModInfo::mod_id` | read by `mods_toml::parse`, with the charset of `ModInfo.VALID_MODID`; tests `mod_id_charset_follows_fml`, `rejects_invalid_descriptors` |
| [[mods]] | `namespace` | FML `ModInfo` | both | supported | `mods_toml::ModInfo::namespace` | the registry namespace, the mod id by default, with the charset of `ModInfo.VALID_NAMESPACE`; test `applies_the_fml_defaults` |
| [[mods]] | `version` | FML `ModInfo` | both | supported | `mods_toml::ModInfo::version` | read by `mods_toml::parse`; `${file.jarVersion}` becomes `NativeMod::version`; test `applies_the_fml_defaults` |
| [[mods]] | `displayName` | FML `ModInfo` | both | supported | `mods_toml::ModInfo::display_name` | the mod id by default; test `applies_the_fml_defaults` |
| [[mods]] | `description` | FML `ModInfo` | both | supported | `mods_toml::ModInfo::description` | read by `mods_toml::parse`; "MISSING DESCRIPTION" by default; test `applies_the_fml_defaults` |
| [[mods]] | `logoFile` | FML `ModInfo`, NeoForge `DefaultModDisplayInfo`, NeoForge `LogoFileWarningsHandler` | client | client-only | - | mod list screen |
| [[mods]] | `logoBlur` | FML `ModInfo` | client | client-only | - | mod list screen |
| [[mods]] | `updateJSONURL` | FML `ModInfo` | both | not supported | - | NeoForge version checker |
| [[mods]] | `modUrl` | FML `ModInfo`, NeoForge `DefaultModDisplayInfo` | both | supported | `mods_toml::ModInfo::mod_url` | a blank or placeholder value is absent; test `mod_url_and_url_rules_follow_fml` |
| [[mods]] | `dependencies` | FML `ModInfo` | both | supported | `mods_toml::ModInfo::dependencies` | the `[[dependencies.<modid>]]` entries of the mod; tests `parses_the_client_mod_descriptor`, `a_bad_mods_toml_id_or_dependency_stops_the_load` |
| [[mods]] | `features` | FML `ModInfo` | both | not supported | - | environment checks (Java version, OpenGL version) |
| [[mods]] | `modproperties` | FML `ModInfo` | both | supported | `mods_toml::ModInfo::mod_properties` | the `[modproperties.<modId>]` table of the file; test `modproperties_are_read_for_the_mod_id` |
| [[dependencies.<modid>]] | `modId` | FML `ModInfo.ModVersion` | both | supported | `mods_toml::ModDependency::mod_id` | test `parses_the_client_mod_descriptor` |
| [[dependencies.<modid>]] | `type` | FML `ModInfo.ModVersion` | both | supported | `mods_toml::ModDependency::dependency_type` | required, optional, incompatible or discouraged, case-insensitive; test `checks_version_ranges_types_and_sides` |
| [[dependencies.<modid>]] | `reason` | FML `ModInfo.ModVersion` | both | supported | `mods_toml::ModDependency::reason` | added to the error of an incompatible dependency; test `checks_version_ranges_types_and_sides` |
| [[dependencies.<modid>]] | `versionRange` | FML `ModInfo.ModVersion` | both | supported | `mods_toml::ModDependency::version_range` | Maven ranges, as `ModSorter.verifyDependencyVersions` reads them; tests `checks_version_ranges_types_and_sides`, `ranges_contain_like_maven` |
| [[dependencies.<modid>]] | `ordering` | FML `ModInfo.ModVersion` | both | supported | `mods_toml::ModDependency::ordering` | BEFORE and AFTER order `init` and the lifecycle methods, the mod id breaks ties; tests `before_and_after_order_three_mods`, `id_breaks_ties_and_none_adds_no_order` |
| [[dependencies.<modid>]] | `side` | FML `ModInfo.ModVersion` | both | supported | `mods_toml::ModDependency::side` | a CLIENT dependency is ignored on the server; test `checks_version_ranges_types_and_sides` |
| [[dependencies.<modid>]] | `referralUrl` | FML `ModInfo.ModVersion` | both | supported | `mods_toml::ModDependency::referral_url` | test `referral_url_is_read_on_a_dependency` |
| [[mods]] | `authors` | NeoForge `DefaultModDisplayInfo` | both | supported | `mods_toml::ModInfo::authors` | read by `mods_toml::parse`; `parses_the_client_mod_descriptor` covers a descriptor without the key only |
| [[mods]] | `credits` | NeoForge `DefaultModDisplayInfo` | client | client-only | - | mod list screen |
| [[mods]] | `licenseURL` | NeoForge `DefaultModDisplayInfo` | client | client-only | - | mod list screen |
| [[mods]] | `bannerFile` | NeoForge `DefaultModDisplayInfo`, NeoForge `LogoFileWarningsHandler` | client | client-only | - | mod list screen |
| [[mods]] | `iconFile` | NeoForge `DefaultModDisplayInfo`, NeoForge `LogoFileWarningsHandler` | client | client-only | - | mod list screen |
| [[mods]] | `iconBlur` | NeoForge `DefaultModDisplayInfo` | client | client-only | - | mod list screen |
| [[mods]] | `displayURL` | NeoForge `DefaultModDisplayInfo` | client | client-only | - | mod list screen |
| [[mods]] | `issueTrackerURL` | NeoForge `DefaultModDisplayInfo` | both | supported | `mods_toml::ModInfo::issue_tracker_url`, `ModInfo::issues_url` | mod level; kept as a plain string, as FML never parses it as a URL; test `issue_tracker_urls_are_read_at_file_and_mod_level` |
| [[mods]] | `featureFlags` | NeoForge `FeatureFlagLoader` | both | not supported | - | mod feature flags stay excluded (risk: feature flags and extensible enums) |

### Loot modifiers, loot conditions and data conditions

| API | Package | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|
| `AddTableLootModifier` | common.loot | class, 2 public methods | both | supported | built-in modifier type `neoforge:add_table` of `ironpumpkin_mods::loot`, rolled with `LootContext::roll_loot_table_raw` | task 2.16 (#166) |
| `CanItemPerformAbility` | common.loot | class, 4 public methods | both | planned | engine extension point (M5) | M5 engine extension: item abilities (tool actions) have no Pumpkin equivalent |
| `IGlobalLootModifier` | common.loot | interface, 3 public methods | both | supported | `ironpumpkin_mods::loot::GlobalLootModifier`; the codec is the decode callback of `ModInit::register_loot_modifier_type` | task 2.16 (#166) |
| `LootModifier` | common.loot | class, 2 public methods | both | supported | the host reads `condition` and `priority` and calls `GlobalLootModifier::apply` when the condition passes | task 2.16 (#166) |
| `LootModifierManager` | common.loot | class, 3 public methods | both | supported | loader for `data/<ns>/loot_modifiers/<path>.json` in `DatapackManager`, on startup and `/reload` | task 2.16 (#166) |
| `LootTableIdCondition` | common.loot | class, 3 public methods | both | supported | condition `neoforge:loot_table_id` in modifier conditions | task 2.16 (#166); modifier conditions only, not inside loot tables |
| `LootTableIdCondition.Builder` | common.loot | class, 1 public method | both | not supported | - | builds the condition in Java datagen or `LootTableLoadEvent` code; a data file writes the JSON condition `neoforge:loot_table_id` |
| `NeoForgeLootContextParams` | common.loot | class, 0 public methods | both | planned | engine extension point (M5) | M5 engine extension: extra loot context key (queried stack); Pumpkin loot context is fixed |
| `AlwaysCondition` | common.conditions | class, 3 public methods | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `AndCondition` | common.conditions | record, 3 public methods | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `ConditionContext` | common.conditions | class, 5 public methods | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `ConditionalOps` | common.conditions | class, 7 public methods | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `FeatureFlagsEnabledCondition` | common.conditions | record, 2 public methods | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `ICondition` | common.conditions | interface, 9 public methods | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `ICondition.IContext` | common.conditions | interface, 4 public methods | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `ModLoadedCondition` | common.conditions | record, 3 public methods | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `NeoForgeConditions` | common.conditions | class, 19 public methods | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `NeverCondition` | common.conditions | class, 3 public methods | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `NotCondition` | common.conditions | record, 3 public methods | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `OrCondition` | common.conditions | record, 3 public methods | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `RegisteredCondition` | common.conditions | record, 2 public methods | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `TagEmptyCondition` | common.conditions | record, 2 public methods | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `WithConditions` | common.conditions | record, 1 public method | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `WithConditions.Builder` | common.conditions | class, 4 public methods | both | planned | engine extension point (M5) | M5 engine extension: the datapack loader does not read `neoforge:conditions`; data with conditions loads as if they were absent |
| `GlobalLootModifierProvider` | common.data | class | both | not supported | - | data generation of loot modifiers |

`LootTableLoadEvent`, `EnchantedBlockLootEvent` and `EnchantedEntityLootEvent` are in the event tables. Pumpkin fires `LootGenerateEvent` (fired, in WIT v0.2) after a loot table rolls.

The global loot modifiers (task 2.16, #166) keep the NeoForge 26.3 behaviour with these deviations:

- Modifiers with the same `priority` run by modifier id. NeoForge leaves their order undefined (the order of a `HashMap`), so a fixed order breaks no mod and keeps the loot the same on each start.
- `neoforge:loot_table_id` works in the `condition` of a modifier, also inside `all_of`, `any_of`, `inverted` and a named predicate. The loot table parser of Pumpkin does not know it, so it does not work inside a loot table.
- `GlobalLootModifier::apply` has no error path: a modifier that cannot act returns the list unchanged. A panic in `apply` or in a decoder is caught and logged by modifier id, as NeoForge catches an exception. After a panic the next modifier gets a copy of the loot from before the failed one; NeoForge goes on with the list object, which the failed modifier may have changed.
- `LootTableHandle` in the upstream file `world/loot.rs` became a struct with the table and its id, and the roll in `world/loot/generation.rs` borrows its random source. The modifiers need the table id at the roll, and the roll and the modifiers share one random source as in NeoForge. An upstream merge that touches these lines fails at compile time and is cheap to fix.
- `neoforge:conditions` in a modifier file is ignored and the file loads, as the data conditions row states. NeoForge skips the file when a condition fails.
- The modifiers run on every roll of a table by its id, through `generate_loot_from_handle`, as `CommonHooks.modifyLoot` runs in `LootTable.getRandomItems`. The rolls of Pumpkin that take a generated table directly (`generate_loot`, `generate_loot_with_context`) have no table id and run no modifiers.

### Extension interfaces NeoForge adds to `net.minecraft`

One row per public method name of each interface, from `net/neoforged/neoforge/common/extensions` (overloads are counted in the Kind column). Targets come from `src/main/resources/META-INF/injected-interfaces.json`. `python3 -I tools/openspec-sync/extension_table.py` writes these tables and the family of `mapping-table.md` from the NeoForge sources and `tools/openspec-sync/extension_rows.tsv`: edit the TSV, not the tables. A `supported` row names the Pumpkin trait method or function and a test. A `planned` row with the prefix `M5 engine extension:` waits for an engine call site; the other `planned` rows wait for a mod API accessor.

400 public methods in 57 interfaces: 366 default, 32 abstract, 2 static. They form 371 rows, one per method name. 2 more rows are types without a public method. The two largest, `IBlockExtension` and `IBlockStateExtension`, hold the block hooks that ported blocks override most.

#### `BootstrapContextAccessExtension` (target `BootstrapContextAccess`; 1 row: 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `BootstrapContextAccessExtension#holderLookup` | default method | both | not supported | - | data generation of datapack registries |

#### `BootstrapContextExtension` (target `BootstrapContext`; 1 row: 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `BootstrapContextExtension#register` | default method | both | not supported | - | data generation of datapack registries |

#### `ContainerExtension` (target `Container`; 2 rows: 2 planned)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `ContainerExtension#setItem` | default method | both | planned | `Inventory` trait (pumpkin-inventory), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `ContainerExtension#onTransfer` | default method | both | planned | `Inventory` trait (pumpkin-inventory), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |

#### `GameTestHelperExtension` (target `GameTestHelper`; 3 rows: 3 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `GameTestHelperExtension#fail` | default method, 2 overloads | both | not supported | - | game test framework |
| `GameTestHelperExtension#getCapability` | default method | both | not supported | - | game test framework |
| `GameTestHelperExtension#requireCapability` | default method | both | not supported | - | game test framework |

#### `IAbstractBoatExtension` (target `AbstractBoat`; 1 row: 1 planned)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IAbstractBoatExtension#canBoatInFluid` | default method, 2 overloads | both | planned | boat entity, new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |

#### `IAdvancementBuilderExtension` (target `Advancement.Builder`; 1 row: 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IAdvancementBuilderExtension#save` | default method, 2 overloads | both | not supported | - | data generation of advancements |

#### `IAttributeExtension` (target `Attribute`; 7 rows: 7 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IAttributeExtension#toValueComponent` | default method | both | not supported | - | formats attribute values for tooltips; display only |
| `IAttributeExtension#toComponent` | default method | both | not supported | - | formats attribute values for tooltips; display only |
| `IAttributeExtension#getDebugInfo` | default method | both | not supported | - | formats attribute values for tooltips; display only |
| `IAttributeExtension#getBaseId` | default method | both | not supported | - | formats attribute values for tooltips; display only |
| `IAttributeExtension#toBaseComponent` | default method | both | not supported | - | formats attribute values for tooltips; display only |
| `IAttributeExtension#getMergedStyle` | abstract method | both | not supported | - | formats attribute values for tooltips; display only |
| `IAttributeExtension#isNullOrAddition` | static method | both | not supported | - | formats attribute values for tooltips; display only |

#### `IBaseRailBlockExtension` (target `BaseRailBlock`; 4 rows: 4 planned)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IBaseRailBlockExtension#isFlexibleRail` | abstract method | both | planned | rail `BlockBehaviour`, new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBaseRailBlockExtension#canMakeSlopes` | default method | both | planned | rail `BlockBehaviour`, new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBaseRailBlockExtension#getRailDirection` | abstract method | both | planned | rail `BlockBehaviour`, new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBaseRailBlockExtension#isValidRailShape` | default method | both | planned | rail `BlockBehaviour`, new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |

#### `IBlockEntityExtension` (target `BlockEntity`; 9 rows: 1 supported, 3 planned, 4 client-only, 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IBlockEntityExtension#onDataPacket` | default method | client | client-only | - | the client calls it when it receives block entity data or renders the block entity |
| `IBlockEntityExtension#handleUpdateTag` | default method | client | client-only | - | the client calls it when it receives block entity data or renders the block entity |
| `IBlockEntityExtension#getPersistentData` | abstract method | both | not supported | `attachments::block_entity` | design difference: data attachments hold the persistent data of a block entity |
| `IBlockEntityExtension#onChunkUnloaded` | default method | both | planned | `BlockEntity` trait (pumpkin-core `block/entities/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockEntityExtension#onLoad` | default method | both | planned | `BlockEntity` trait (pumpkin-core `block/entities/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockEntityExtension#requestModelDataUpdate` | default method | client | client-only | - | the client calls it when it receives block entity data or renders the block entity |
| `IBlockEntityExtension#getModelData` | default method | client | client-only | - | the client calls it when it receives block entity data or renders the block entity |
| `IBlockEntityExtension#invalidateCapabilities` | default method | both | supported | `ironpumpkin_mods::capabilities::invalidate_capabilities` | a block entity that goes or arrives invalidates the caches of its position; a plain block calls the function itself |
| `IBlockEntityExtension#applyStructureRotation` | default method | both | planned | `BlockEntity` trait (pumpkin-core `block/entities/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |

#### `IBlockExtension` (target `Block`; 58 rows: 51 planned, 6 client-only, 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IBlockExtension#getFriction` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `entity/living.rs:2229` and no hook that a mod can override |
| `IBlockExtension#hasDynamicLightEmission` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the server calls it in vanilla (`ChunkAccess.java:321`, to find light sources); the Pumpkin light engine reads the state luminance at `pumpkin-world/src/lighting/engine.rs:537` and has no hook |
| `IBlockExtension#getLightEmission` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `pumpkin-world/src/lighting/engine.rs:537` and no hook that a mod can override |
| `IBlockExtension#ignitedByLava` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#isLadder` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `entity/living.rs:2511` and no hook that a mod can override |
| `IBlockExtension#makesOpenTrapdoorAboveClimbable` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#isBurning` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#canHarvestBlock` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `block/mod.rs:622` and no hook that a mod can override |
| `IBlockExtension#onDestroyedByPlayer` | default method | both | planned | `BlockBehaviour::broken` | M5 engine extension: `BlockBehaviour::broken` runs after the engine removed the block; NeoForge calls the method in place of the removal |
| `IBlockExtension#onDestroyedByPushReaction` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#getRespawnPosition` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#getExplosionResistance` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `world/explosion.rs:68` and no hook that a mod can override |
| `IBlockExtension#getCloneItemStack` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `net/java/play/pick_item.rs` and no hook that a mod can override |
| `IBlockExtension#addLandingEffects` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#addRunningEffects` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#playFallSound` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#playStepSound` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#canSustainPlant` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#onTreeGrow` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#isFertile` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#getExpDrop` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#rotate` | default method | both | planned | `BlockBehaviour::rotate` | M5 engine extension: `BlockBehaviour::rotate` is the vanilla state rotation and takes no level or position; `rotate(state, level, pos, rotation)` has no call site |
| `IBlockExtension#getEnchantPowerBonus` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `block/blocks/enchanting_table.rs:105` and no hook that a mod can override |
| `IBlockExtension#onNeighborChange` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#shouldCheckWeakPower` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#getWeakChanges` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#getSoundType` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `entity/living.rs:2849` and no hook that a mod can override |
| `IBlockExtension#getBeaconColorMultiplier` | default method | both | not supported | - | sets the colour of the beacon beam; display only |
| `IBlockExtension#getStateAtViewpoint` | default method | client | client-only | - | only the client calls it (`Camera.java`), to pick the fog of the camera block |
| `IBlockExtension#getBlockPathType` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#getAdjacentBlockPathType` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#isSlimeBlock` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `block/blocks/piston/mod.rs:81` and no hook that a mod can override |
| `IBlockExtension#isStickyBlock` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `block/blocks/piston/mod.rs:81` and no hook that a mod can override |
| `IBlockExtension#canStickTo` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `block/blocks/piston/mod.rs:85` and no hook that a mod can override |
| `IBlockExtension#getFlammability` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `block/blocks/fire/fire.rs:37` and no hook that a mod can override |
| `IBlockExtension#isFlammable` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `block/blocks/fire/fire.rs:37` and no hook that a mod can override |
| `IBlockExtension#onCaughtFire` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#getFireSpreadSpeed` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `block/blocks/fire/fire.rs:37` and no hook that a mod can override |
| `IBlockExtension#isFireSource` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#canEntityDestroy` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `entity/boss/wither.rs:170` and no hook that a mod can override |
| `IBlockExtension#canDropFromExplosion` | default method | both | planned | `BlockBehaviour::should_drop_items_on_explosion` | M5 engine extension: the engine calls the hook in `world/explosion.rs`, but the hook takes no state, position or explosion, and no test covers it |
| `IBlockExtension#onBlockExploded` | default method | both | planned | `BlockBehaviour::explode` | M5 engine extension: the engine calls `explode` after it removes the block; NeoForge calls `onBlockExploded` in place of the removal |
| `IBlockExtension#collisionExtendsVertically` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#shouldDisplayFluidOverlay` | default method | client | client-only | - | only the client calls it: dynamic light, face culling or the rendered appearance |
| `IBlockExtension#getToolModifiedState` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#isScaffolding` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `entity/living.rs:1893` and no hook that a mod can override |
| `IBlockExtension#hidesNeighborFace` | default method | client | client-only | - | only the client calls it: dynamic light, face culling or the rendered appearance |
| `IBlockExtension#supportsExternalFaceHiding` | default method | client | client-only | - | only the client calls it: dynamic light, face culling or the rendered appearance |
| `IBlockExtension#onBlockStateChange` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#canBeHydrated` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#getMapColor` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `world/map.rs:116` and no hook that a mod can override |
| `IBlockExtension#getAppearance` | default method | client | client-only | - | only the client calls it: dynamic light, face culling or the rendered appearance |
| `IBlockExtension#getPistonPushReaction` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#isEmpty` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#getBubbleColumnDirection` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#shouldHideAdjacentFluidFace` | default method | client | client-only | - | only the client calls it: dynamic light, face culling or the rendered appearance |
| `IBlockExtension#getBounceRestitution` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockExtension#getRelocability` | default method | both | planned | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |

#### `IBlockGetterExtension` (target `BlockGetter`; 2 rows: 1 planned, 1 client-only)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IBlockGetterExtension#getAuxLightManager` | default method, 2 overloads | both | planned | `World` light engine | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IBlockGetterExtension#getModelData` | default method | client | client-only | - | model data is a client rendering feature |

#### `IBlockStateExtension` (target `BlockState`; 55 rows: 48 planned, 6 client-only, 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IBlockStateExtension#getFriction` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#getFriction`, which has no hook; the engine code is at `entity/living.rs:2229` |
| `IBlockStateExtension#hasDynamicLightEmission` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: the server calls it in vanilla (`ChunkAccess.java:321`, to find light sources); the Pumpkin light engine reads the state luminance at `pumpkin-world/src/lighting/engine.rs:537` and has no hook |
| `IBlockStateExtension#getLightEmission` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#getLightEmission`, which has no hook; the engine code is at `pumpkin-world/src/lighting/engine.rs:537` |
| `IBlockStateExtension#ignitedByLava` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#isLadder` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#isLadder`, which has no hook; the engine code is at `entity/living.rs:2511` |
| `IBlockStateExtension#canHarvestBlock` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#canHarvestBlock`, which has no hook; the engine code is at `block/mod.rs:622` |
| `IBlockStateExtension#onDestroyedByPlayer` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#onDestroyedByPlayer`, which has no hook that replaces the removal |
| `IBlockStateExtension#onDestroyedByPushReaction` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#getRespawnPosition` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#getExplosionResistance` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#getExplosionResistance`, which has no hook; the engine code is at `world/explosion.rs:68` |
| `IBlockStateExtension#getCloneItemStack` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#getCloneItemStack`, which has no hook; the engine code is at `net/java/play/pick_item.rs` |
| `IBlockStateExtension#addLandingEffects` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#addRunningEffects` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#playFallSound` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#playStepSound` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#canSustainPlant` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#onTreeGrow` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#isFertile` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#getExpDrop` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#rotate` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#rotate`, which has no hook with a level and a position |
| `IBlockStateExtension#getEnchantPowerBonus` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#getEnchantPowerBonus`, which has no hook; the engine code is at `block/blocks/enchanting_table.rs:105` |
| `IBlockStateExtension#onNeighborChange` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#shouldCheckWeakPower` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#getWeakChanges` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#getSoundType` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#getSoundType`, which has no hook; the engine code is at `entity/living.rs:2849` |
| `IBlockStateExtension#getBeaconColorMultiplier` | default method | both | not supported | - | sets the colour of the beacon beam; display only |
| `IBlockStateExtension#getStateAtViewpoint` | default method | client | client-only | - | only the client calls it (`Camera.java`), to pick the fog of the camera block |
| `IBlockStateExtension#isSlimeBlock` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#isSlimeBlock`, which has no hook; the engine code is at `block/blocks/piston/mod.rs:81` |
| `IBlockStateExtension#isStickyBlock` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#isStickyBlock`, which has no hook; the engine code is at `block/blocks/piston/mod.rs:81` |
| `IBlockStateExtension#canStickTo` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#canStickTo`, which has no hook; the engine code is at `block/blocks/piston/mod.rs:85` |
| `IBlockStateExtension#getFlammability` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#getFlammability`, which has no hook; the engine code is at `block/blocks/fire/fire.rs:37` |
| `IBlockStateExtension#isFlammable` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#isFlammable`, which has no hook; the engine code is at `block/blocks/fire/fire.rs:37` |
| `IBlockStateExtension#onCaughtFire` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#getFireSpreadSpeed` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#getFireSpreadSpeed`, which has no hook; the engine code is at `block/blocks/fire/fire.rs:37` |
| `IBlockStateExtension#isFireSource` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#canEntityDestroy` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#canEntityDestroy`, which has no hook; the engine code is at `entity/boss/wither.rs:170` |
| `IBlockStateExtension#isBurning` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#getBlockPathType` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#getAdjacentBlockPathType` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#canDropFromExplosion` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#onBlockExploded` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#collisionExtendsVertically` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#shouldDisplayFluidOverlay` | default method | client | client-only | - | only the client calls it: dynamic light, face culling or the rendered appearance |
| `IBlockStateExtension#getToolModifiedState` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#isScaffolding` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to `IBlockExtension#isScaffolding`, which has no hook; the engine code is at `entity/living.rs:1893` |
| `IBlockStateExtension#hidesNeighborFace` | default method | client | client-only | - | only the client calls it: dynamic light, face culling or the rendered appearance |
| `IBlockStateExtension#supportsExternalFaceHiding` | default method | client | client-only | - | only the client calls it: dynamic light, face culling or the rendered appearance |
| `IBlockStateExtension#onBlockStateChange` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#canBeHydrated` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#getAppearance` | default method | client | client-only | - | only the client calls it: dynamic light, face culling or the rendered appearance |
| `IBlockStateExtension#isEmpty` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#getBubbleColumnDirection` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#shouldHideAdjacentFluidFace` | default method | client | client-only | - | only the client calls it: dynamic light, face culling or the rendered appearance |
| `IBlockStateExtension#getBounceRestitution` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IBlockStateExtension#getRelocability` | default method | both | planned | `BlockState` forwarding to `BlockBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |

#### `IBucketPickupExtension` (target `BucketPickup`; 1 row: 1 planned)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IBucketPickupExtension#getPickupSound` | default method | both | planned | bucket `ItemBehaviour` (pumpkin-core `item/items/bucket.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |

#### `IClientCommonPacketListenerExtension` (target `ClientCommonPacketListener`; 2 rows: 2 client-only)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IClientCommonPacketListenerExtension#send` | default method | client | client-only | - | client packet listener; a dedicated server never runs it |
| `IClientCommonPacketListenerExtension#disconnect` | default method | client | client-only | - | client packet listener; a dedicated server never runs it |

#### `ICommandSourceExtension` (target `CommandSource`; 1 row: 1 planned)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `ICommandSourceExtension#getGameProfile` | default method | both | planned | `CommandSender` (pumpkin-core `command/mod.rs`) | the Pumpkin type has the data; no mod API reads it yet |

#### `ICommandSourceStackExtension` (target `CommandSourceStack`; 4 rows: 4 planned)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `ICommandSourceStackExtension#getScoreboard` | default method | both | planned | `CommandSender`, scoreboard of the server | the Pumpkin type has the data; no mod API reads it yet |
| `ICommandSourceStackExtension#getAdvancement` | default method | both | planned | `CommandSender`, advancement lookup | the Pumpkin type has the data; no mod API reads it yet |
| `ICommandSourceStackExtension#getUnsidedLevel` | default method | both | planned | `CommandSender` world | the Pumpkin type has the data; no mod API reads it yet |
| `ICommandSourceStackExtension#getSourceProfile` | default method | both | planned | `CommandSender` (pumpkin-core `command/mod.rs`) | the Pumpkin type has the data; no mod API reads it yet |

#### `ICommonPacketListener` (not injected (used through a cast or a patch); 6 rows: 2 supported, 2 planned, 2 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `ICommonPacketListener#send` | abstract method, 2 overloads | both | planned | `pumpkin_core` payload send of the connection | M5 engine extension: a mod sends a payload only through the registrar and the player helpers; the `Packet` overload is Java-only |
| `ICommonPacketListener#disconnect` | abstract method | both | planned | `JavaClient` kick (pumpkin-core `net/java/mod.rs`) | M5 engine extension: no mod API disconnects a connection with a text component |
| `ICommonPacketListener#getConnection` | abstract method | both | not supported | - | Java-only: returns the Netty connection object |
| `ICommonPacketListener#getPacketProcessor` | abstract method | both | not supported | - | Java-only: returns the Netty packet processor |
| `ICommonPacketListener#hasChannel` | default method, 3 overloads | both | supported | `NegotiatedChannels::has_channel` (pumpkin-core `plugin/network_events.rs`) | reads the negotiated channels, the common channels and the ad hoc channels of the connection |
| `ICommonPacketListener#getConnectionType` | abstract method | both | supported | `NegotiatedChannels::connection_type` | the connection is `NeoForge`, `Vanilla` or `Other` after the negotiation |

#### `IDataComponentHolderExtension` (target `DataComponentHolder`; 1 row: 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IDataComponentHolderExtension#addToTooltip` | default method, 2 overloads | both | not supported | - | tooltip text is built for display |

#### `IDataComponentMapBuilderExtensions` (target `DataComponentMap.Builder`; 1 row: 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IDataComponentMapBuilderExtensions#set` | default method | both | not supported | `ItemBuilder` component calls | Java-only: a `Supplier` overload of the builder `set` |

#### `IDispensibleContainerItemExtension` (target `DispensibleContainerItem`; 1 row: 1 planned)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IDispensibleContainerItemExtension#emptyContents` | default method | both | planned | bucket `ItemBehaviour` (pumpkin-core `item/items/bucket.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |

#### `IEntityExtension` (target `Entity`; 29 rows: 27 planned, 1 client-only, 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IEntityExtension#captureDrops` | abstract method, 2 overloads | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#getPersistentData` | abstract method | both | not supported | `Entity::attachments` | design difference: data attachments hold the persistent data of an entity |
| `IEntityExtension#shouldRiderSit` | default method | client | client-only | - | only the client calls it (`HumanoidMobRenderer.java:62`), for the sitting pose |
| `IEntityExtension#canRiderInteract` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#canBeRiddenUnderFluidType` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#canTrample` | abstract method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `block/blocks/farmland.rs:73` and no hook that a mod can override |
| `IEntityExtension#getClassification` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#isAddedToLevel` | abstract method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#onAddedToLevel` | abstract method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#onRemovedFromLevel` | abstract method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#revive` | abstract method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#isMultipartEntity` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#getParts` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#getFluidTypeHeight` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#getMaxHeightFluidType` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#isInFluidType` | default method, 4 overloads | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#getFirstEyeInFluidType` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#isEyeInFluidType` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#canStartSwimming` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#getFluidMotionScale` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#isPushedByFluid` | default method | both | planned | `EntityBase::is_pushed_by_fluids` | M5 engine extension: the hook takes no fluid type, because Pumpkin has no fluid types |
| `IEntityExtension#canSwimInFluidType` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#canFluidExtinguish` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#getFluidFallDistanceModifier` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#canHydrateInFluidType` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#getSoundFromFluidType` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#sendPairingData` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#copyAttachmentsFrom` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IEntityExtension#getBlockBounciness` | default method | both | planned | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `entity/mod.rs:3430` and no hook that a mod can override |

#### `IFallableExtension` (target `Fallable`; 1 row: 1 planned)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IFallableExtension#fallingTick` | default method | both | planned | falling block `BlockBehaviour`, new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |

#### `IFluidExtension` (target `Fluid`; 9 rows: 8 planned, 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IFluidExtension#getExplosionResistance` | default method | both | planned | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IFluidExtension#getFluidType` | abstract method | both | planned | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IFluidExtension#canConvertToSource` | default method | both | planned | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IFluidExtension#supportsBoating` | default method | both | planned | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IFluidExtension#getBlockPathType` | default method | both | planned | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IFluidExtension#getAdjacentBlockPathType` | default method | both | planned | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IFluidExtension#canHydrate` | default method | both | planned | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IFluidExtension#canExtinguish` | default method | both | planned | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IFluidExtension#appendHoverText` | default method | both | not supported | - | tooltip text for display |

#### `IFluidStateExtension` (target `FluidState`; 8 rows: 8 planned)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IFluidStateExtension#getExplosionResistance` | default method | both | planned | `FluidState` forwarding to `FluidBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IFluidStateExtension#getFluidType` | default method | both | planned | `FluidState` forwarding to `FluidBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IFluidStateExtension#canConvertToSource` | default method | both | planned | `FluidState` forwarding to `FluidBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IFluidStateExtension#supportsBoating` | default method | both | planned | `FluidState` forwarding to `FluidBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IFluidStateExtension#getBlockPathType` | default method | both | planned | `FluidState` forwarding to `FluidBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IFluidStateExtension#getAdjacentBlockPathType` | default method | both | planned | `FluidState` forwarding to `FluidBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IFluidStateExtension#canHydrate` | default method | both | planned | `FluidState` forwarding to `FluidBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IFluidStateExtension#canExtinguish` | default method | both | planned | `FluidState` forwarding to `FluidBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |

#### `IFriendlyByteBufExtension` (target `FriendlyByteBuf`; 5 rows: 5 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IFriendlyByteBufExtension#readArray` | default method | both | not supported | - | Java-only: helpers of the Java wire buffer; a payload in `ironpumpkin_mods::network` is a Rust type with its own codec |
| `IFriendlyByteBufExtension#writeArray` | default method | both | not supported | - | Java-only: helpers of the Java wire buffer; a payload in `ironpumpkin_mods::network` is a Rust type with its own codec |
| `IFriendlyByteBufExtension#writeByte` | default method | both | not supported | - | Java-only: helpers of the Java wire buffer; a payload in `ironpumpkin_mods::network` is a Rust type with its own codec |
| `IFriendlyByteBufExtension#readMap` | default method | both | not supported | - | Java-only: helpers of the Java wire buffer; a payload in `ironpumpkin_mods::network` is a Rust type with its own codec |
| `IFriendlyByteBufExtension#writeMap` | default method | both | not supported | - | Java-only: helpers of the Java wire buffer; a payload in `ironpumpkin_mods::network` is a Rust type with its own codec |

#### `IHolderExtension` (target `Holder`; 5 rows: 3 supported, 2 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IHolderExtension#getDelegate` | default method | both | not supported | `registry::key_of`, `registry::id_of` | Java-only: IronPumpkin has no holder object; the mod keeps the name |
| `IHolderExtension#unwrapLookup` | default method | both | not supported | `registry::key_of`, `registry::id_of` | Java-only: IronPumpkin has no holder object; the mod keeps the name |
| `IHolderExtension#getKey` | default method | both | supported | `ironpumpkin_mods::registry::key_of` (a holder is a name in IronPumpkin) | answers for vanilla and mod entries; the mod already holds the name, and `key_of` maps an id to it |
| `IHolderExtension#key` | default method | both | supported | `ironpumpkin_mods::registry::key_of` (a holder is a name in IronPumpkin) | answers for vanilla and mod entries; the mod already holds the name, and `key_of` maps an id to it |
| `IHolderExtension#keyOrNull` | default method | both | supported | `ironpumpkin_mods::registry::key_of` (a holder is a name in IronPumpkin) | answers for vanilla and mod entries; the mod already holds the name, and `key_of` maps an id to it |

#### `IHolderLookupProviderExtension` (target `HolderLookup.Provider`; 2 rows: 2 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IHolderLookupProviderExtension#holderOrThrow` | default method | both | not supported | `registry::contains`, `registry::id_of` | Java-only: returns a holder object; the mod asks the registry by name |
| `IHolderLookupProviderExtension#holder` | default method | both | not supported | `registry::contains`, `registry::id_of` | Java-only: returns a holder object; the mod asks the registry by name |

#### `IHolderSetExtension` (target `HolderSet`; 3 rows: 3 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IHolderSetExtension#addInvalidationListener` | default method | both | not supported | `registry::tag_entries` | Java-only: a holder set is a tag name, and `tag_entries` answers its members |
| `IHolderSetExtension#serializationType` | default method | both | not supported | `registry::tag_entries` | Java-only: a holder set is a tag name, and `tag_entries` answers its members |
| `IHolderSetExtension#isImmediatelyResolvable` | default method | both | not supported | `registry::tag_entries` | Java-only: a holder set is a tag name, and `tag_entries` answers its members |

#### `IItemExtension` (target `Item`; 46 rows: 42 planned, 3 client-only, 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IItemExtension#getDefaultAttributeModifiers` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#onDroppedByPlayer` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#getHighlightTip` | default method | client | client-only | - | only the client calls it, for the item name above the hotbar |
| `IItemExtension#onItemUseFirst` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#isPiglinCurrency` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `entity/mob/piglin.rs:402` and no hook that a mod can override |
| `IItemExtension#makesPiglinsNeutral` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#isCombineRepairable` | abstract method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#getXpRepairRatio` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `entity/player.rs:6759` and no hook that a mod can override |
| `IItemExtension#onStopUsing` | default method | both | planned | `ItemBehaviour::on_stopped_using` | M5 engine extension: `ItemBehaviour::on_stopped_using` is the vanilla `releaseUsing`; NeoForge calls `onStopUsing` on every stop (`LivingEntity.java:3750`), also when the item is not released |
| `IItemExtension#onLeftClickEntity` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#getCraftingRemainder` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `pumpkin-data/src/item_stack/mod.rs:358` and no hook that a mod can override |
| `IItemExtension#getEntityLifespan` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `entity/item.rs:92` and no hook that a mod can override |
| `IItemExtension#hasCustomEntity` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#createEntity` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#onEntityItemUpdate` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#doesSneakBypassUse` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#canEquip` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `pumpkin-data/src/data_component_impl/combat.rs:670` and no hook that a mod can override |
| `IItemExtension#getEquipmentSlot` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `entity/mob/equipment.rs:809` and no hook that a mod can override |
| `IItemExtension#onEntitySwing` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#getDamage` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#getMaxDamage` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `pumpkin-data/src/item_stack/mod.rs:363` and no hook that a mod can override |
| `IItemExtension#isDamaged` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#setDamage` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#canPerformAction` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#getMaxStackSize` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `pumpkin-data/src/item_stack/mod.rs:351` and no hook that a mod can override |
| `IItemExtension#isPrimaryItemFor` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#supportsEnchantment` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#getEnchantmentLevel` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#getAllEnchantments` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#shouldCauseReequipAnimation` | default method | client | client-only | - | only the client calls it, for the held item animation |
| `IItemExtension#shouldCauseBlockBreakReset` | default method | client | client-only | - | only the client calls it, to reset the block breaking progress |
| `IItemExtension#canContinueUsing` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#getCreatorModId` | default method | both | not supported | - | names the mod of an item for tooltips; display only |
| `IItemExtension#onAnimalArmorTick` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#damageItem` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#onDestroyed` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#isGazeDisguise` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `entity/mob/enderman.rs:371` and no hook that a mod can override |
| `IItemExtension#canWalkOnPowderedSnow` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `block/blocks/powder_snow.rs:26` and no hook that a mod can override |
| `IItemExtension#isDamageable` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#getSweepHitBox` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `entity/player.rs:1848` and no hook that a mod can override |
| `IItemExtension#isNotReplaceableByPickAction` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#canGrindstoneRepair` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#canBeHurtBy` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: the engine has the vanilla code at `pumpkin-data/src/item_stack/mod.rs:450` and no hook that a mod can override |
| `IItemExtension#applyEnchantments` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#canFitInsideContainerItems` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IItemExtension#onGlideDamage` | default method | both | planned | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |

#### `IItemPropertiesExtensions` (target `Item.Properties`; 1 row: 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IItemPropertiesExtensions#component` | default method | both | not supported | `ItemBuilder` component calls | Java-only: a `Supplier` overload of the builder `component` |

#### `IItemStackExtension` (target `ItemStack`; 29 rows: 1 supported, 26 planned, 2 client-only)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IItemStackExtension#onItemUseFirst` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#shouldCauseBlockBreakReset` | default method | client | client-only | - | only the client calls it, to reset the block breaking progress |
| `IItemStackExtension#isPrimaryItemFor` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#supportsEnchantment` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#getAllEnchantments` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#getEquipmentSlot` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to `IItemExtension#getEquipmentSlot`, which has no hook; the engine code is at `entity/mob/equipment.rs:809` |
| `IItemStackExtension#onEntitySwing` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#onStopUsing` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to `IItemExtension#onStopUsing`, which has no hook for every stop |
| `IItemStackExtension#getEntityLifespan` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to `IItemExtension#getEntityLifespan`, which has no hook; the engine code is at `entity/item.rs:92` |
| `IItemStackExtension#onEntityItemUpdate` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#getXpRepairRatio` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to `IItemExtension#getXpRepairRatio`, which has no hook; the engine code is at `entity/player.rs:6759` |
| `IItemStackExtension#onAnimalArmorTick` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#canEquip` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to `IItemExtension#canEquip`, which has no hook; the engine code is at `pumpkin-data/src/data_component_impl/combat.rs:670` |
| `IItemStackExtension#onDroppedByPlayer` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#getHighlightTip` | default method | client | client-only | - | only the client calls it, for the item name above the hotbar |
| `IItemStackExtension#doesSneakBypassUse` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#isCombineRepairable` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#isPiglinCurrency` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to `IItemExtension#isPiglinCurrency`, which has no hook; the engine code is at `entity/mob/piglin.rs:402` |
| `IItemStackExtension#makesPiglinsNeutral` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#isGazeDisguise` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to `IItemExtension#isGazeDisguise`, which has no hook; the engine code is at `entity/mob/enderman.rs:371` |
| `IItemStackExtension#canWalkOnPowderedSnow` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to `IItemExtension#canWalkOnPowderedSnow`, which has no hook; the engine code is at `block/blocks/powder_snow.rs:26` |
| `IItemStackExtension#getSweepHitBox` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to `IItemExtension#getSweepHitBox`, which has no hook; the engine code is at `entity/player.rs:1848` |
| `IItemStackExtension#onDestroyed` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#isNotReplaceableByPickAction` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#canGrindstoneRepair` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#getCapability` | default method, 2 overloads | both | supported | `ironpumpkin_mods::capabilities::ItemCapability::get_capability` | an empty stack has no capability; `get_capability_from` answers for an `ItemAccess` |
| `IItemStackExtension#getAttributeModifiers` | default method | both | planned | `ItemStack` attribute modifier component | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#canFitInsideContainerItems` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `IItemStackExtension#onGlideDamage` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |

#### `ILevelExtension` (target `Level`; 7 rows: 2 supported, 1 client-only, 4 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `ILevelExtension#getMaxEntityRadius` | abstract method | both | not supported | - | Java-only: the size of the entity lookup of the Java engine |
| `ILevelExtension#increaseMaxEntityRadius` | abstract method | both | not supported | - | Java-only: the size of the entity lookup of the Java engine |
| `ILevelExtension#getModelDataManager` | default method | client | client-only | - | model data is a client rendering feature |
| `ILevelExtension#getCapability` | default method, 4 overloads | both | supported | `ironpumpkin_mods::capabilities::BlockCapability::get_capability` | four overloads; the lookup goes through the providers of the mods and of vanilla blocks |
| `ILevelExtension#invalidateCapabilities` | default method, 2 overloads | both | supported | `ironpumpkin_mods::capabilities::invalidate_capabilities` and `invalidate_capabilities_in_chunk` | a plain block calls it; the host calls it for chunk loads and unloads |
| `ILevelExtension#getDescriptionKey` | default method | both | not supported | - | display name of the dimension |
| `ILevelExtension#getDescription` | default method | both | not supported | - | display name of the dimension |

#### `ILevelReaderExtension` (target `LevelReader`; 3 rows: 1 planned, 2 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `ILevelReaderExtension#isAreaLoaded` | default method | both | planned | `World` chunk loading state | the Pumpkin type has the data; no mod API reads it yet |
| `ILevelReaderExtension#holderOrThrow` | default method | both | not supported | `registry::contains`, `registry::id_of` | Java-only: returns a holder object; the mod asks the registry by name |
| `ILevelReaderExtension#holder` | default method | both | not supported | `registry::contains`, `registry::id_of` | Java-only: returns a holder object; the mod asks the registry by name |

#### `ILivingEntityExtension` (target `LivingEntity`; 7 rows: 6 planned, 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `ILivingEntityExtension#self` | default method | both | not supported | - | Java-only: returns the receiver of the interface |
| `ILivingEntityExtension#canSwimInFluidType` | default method | both | planned | `LivingEntity` (pumpkin-core `entity/living.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `ILivingEntityExtension#jumpInFluid` | default method | both | planned | `LivingEntity` (pumpkin-core `entity/living.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `ILivingEntityExtension#sinkInFluid` | default method | both | planned | `LivingEntity` (pumpkin-core `entity/living.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `ILivingEntityExtension#canDrownInFluidType` | default method | both | planned | `LivingEntity` (pumpkin-core `entity/living.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `ILivingEntityExtension#moveInFluid` | default method | both | planned | `LivingEntity` (pumpkin-core `entity/living.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `ILivingEntityExtension#onDamageTaken` | default method | both | planned | `LivingEntity` (pumpkin-core `entity/living.rs`), new hook | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |

#### `IMenuProviderExtension` (target `MenuProvider`; 2 rows: 2 planned)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IMenuProviderExtension#shouldTriggerClientSideContainerClosingOnOpen` | default method | both | planned | `menus::MenuDefinition` | M5 engine extension: no Pumpkin hook exists for this method and the engine has no call site |
| `IMenuProviderExtension#writeClientSideData` | default method, 2 overloads | both | planned | `menus::MenuDefinition` | M5 engine extension: the extra data of a mod menu waits for `neoforge:advanced_open_screen` |

#### `IMenuTypeExtension` (target `MenuType`; 1 row: 1 planned)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IMenuTypeExtension#create` | abstract and static method, 2 overloads | both | planned | `registration.menu-type-definition.extra-data` | M5 engine extension: mod menu types with extra data wait for `neoforge:advanced_open_screen` |

#### `IMobEffectExtension` (target `MobEffect`; 1 row: 1 client-only)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IMobEffectExtension#getSortOrder` | default method | client | client-only | - | sorts the effects in GUIs; the server never calls it |

#### `IOwnedSpawner` (target `BaseSpawner, TrialSpawner`; 1 row: 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IOwnedSpawner#getOwner` | abstract method | both | not supported | - | finds the owner entity of a spawner for spawn events; Pumpkin spawners have no owner link |

#### `IPacketFlowExtension` (target `PacketFlow`; 4 rows: 4 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IPacketFlowExtension#self` | default method | both | not supported | - | Java-only: returns the receiver of the interface |
| `IPacketFlowExtension#isClientbound` | default method | both | not supported | `ironpumpkin_mods::network::PacketFlow` match | Java-only: helper of the Java enum; a port matches the Rust `PacketFlow` directly |
| `IPacketFlowExtension#isServerbound` | default method | both | not supported | `ironpumpkin_mods::network::PacketFlow` match | Java-only: helper of the Java enum; a port matches the Rust `PacketFlow` directly |
| `IPacketFlowExtension#getReceptionSide` | default method | both | not supported | `ironpumpkin_mods::network::PacketFlow` match | Java-only: helper of the Java enum; a port matches the Rust `PacketFlow` directly |

#### `IPlayerExtension` (target `Player`; 4 rows: 1 supported, 2 planned, 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IPlayerExtension#isCloseEnough` | default method | both | planned | `Player` (pumpkin-core `entity/player.rs`) | the Pumpkin type has the data; no mod API reads it yet |
| `IPlayerExtension#openMenu` | default method, 2 overloads | both | planned | `ironpumpkin_mods::menus::open_menu` | M5 engine extension: `openMenu(provider)` works through `open_menu`; the overloads with extra data or a position wait for `neoforge:advanced_open_screen` |
| `IPlayerExtension#mayFly` | default method | both | supported | `Player::may_fly` (pumpkin-core `entity/player/ironpumpkin.rs`, public through `ironpumpkin_mods::entity`) | true for the allow-flying ability or the `neoforge:creative_flight` attribute; the engine calls it in `request_flying` and `stop_flying_unless_may_fly` |
| `IPlayerExtension#isFakePlayer` | default method | both | not supported | - | design difference: Pumpkin has no fake player type, so a player is never a fake player |

#### `IPlayerListExtension` (target `PlayerList`; 3 rows: 2 planned, 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IPlayerListExtension#self` | default method | both | not supported | - | Java-only: returns the receiver of the interface |
| `IPlayerListExtension#broadcastAll` | default method, 2 overloads | both | planned | `ironpumpkin_mods::network` payload send helpers | M5 engine extension: the payload send helpers of the mod API are not complete |
| `IPlayerListExtension#broadcast` | default method, 2 overloads | both | planned | `ironpumpkin_mods::network` payload send helpers | M5 engine extension: the payload send helpers of the mod API are not complete |

#### `IRecipeOutputExtension` (target `RecipeOutput`; 2 rows: 2 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IRecipeOutputExtension#accept` | abstract method | both | not supported | - | data generation of recipes |
| `IRecipeOutputExtension#withConditions` | default method | both | not supported | - | data generation of recipes |

#### `IServerChunkCacheExtension` (target `ServerChunkCache`; 3 rows: 2 planned, 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IServerChunkCacheExtension#self` | default method | both | not supported | - | Java-only: returns the receiver of the interface |
| `IServerChunkCacheExtension#sendToTrackingPlayersAndSelf` | default method | both | planned | `ironpumpkin_mods::network` payload send helpers | M5 engine extension: the payload send helpers of the mod API are not complete |
| `IServerChunkCacheExtension#sendToTrackingPlayers` | default method | both | planned | `ironpumpkin_mods::network` payload send helpers | M5 engine extension: the payload send helpers of the mod API are not complete |

#### `IServerCommonPacketListenerExtension` (target `ServerCommonPacketListener`; 1 row: 1 planned)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IServerCommonPacketListenerExtension#send` | abstract and default method, 3 overloads | both | planned | `ironpumpkin_mods::network` payload send helpers | M5 engine extension: the payload send helpers of the mod API are not complete; the overload with a Netty `ChannelFutureListener` is Java-only |

#### `IServerConfigurationPacketListenerExtension` (target `ServerConfigurationPacketListener`; 1 row: 1 supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IServerConfigurationPacketListenerExtension#finishCurrentTask` | abstract method | both | supported | `ironpumpkin_mods::network::PayloadContext::finish_current_task` | a request for a task that is not current disconnects the client |

#### `IServerGamePacketListenerExtension` (target `ServerGamePacketListener`; 1 row: 1 planned)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IServerGamePacketListenerExtension#sendBundled` | default method, 2 overloads | both | planned | `ironpumpkin_mods::network` payload send helpers | M5 engine extension: the payload send helpers of the mod API are not complete |

#### `ITagAppenderExtension` (target `TagAppender`; 5 rows: 5 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `ITagAppenderExtension#addTags` | default method | both | not supported | - | data generation of tags |
| `ITagAppenderExtension#addOptionalTags` | default method | both | not supported | - | data generation of tags |
| `ITagAppenderExtension#add` | abstract method | both | not supported | - | data generation of tags |
| `ITagAppenderExtension#replace` | abstract and default method, 2 overloads | both | not supported | - | data generation of tags |
| `ITagAppenderExtension#remove` | abstract and default method, 4 overloads | both | not supported | - | data generation of tags |

#### `ITagBuilderExtension` (target `TagBuilder`; 2 rows: 2 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `ITagBuilderExtension#removeElement` | default method | both | not supported | - | data generation of tags |
| `ITagBuilderExtension#removeTag` | default method | both | not supported | - | data generation of tags |

#### `ITransformationExtension` (target `Transformation`; 7 rows: 7 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `ITransformationExtension#isIdentity` | default method | both | not supported | - | model transformation math for client models |
| `ITransformationExtension#transformPosition` | default method | both | not supported | - | model transformation math for client models |
| `ITransformationExtension#transformNormal` | default method | both | not supported | - | model transformation math for client models |
| `ITransformationExtension#rotateTransform` | default method | both | not supported | - | model transformation math for client models |
| `ITransformationExtension#blockCenterToCorner` | default method | both | not supported | - | model transformation math for client models |
| `ITransformationExtension#blockCornerToCenter` | default method | both | not supported | - | model transformation math for client models |
| `ITransformationExtension#applyOrigin` | default method | both | not supported | - | model transformation math for client models |

#### `ItemInstanceExtension` (target `ItemInstance`; 4 rows: 4 planned)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `ItemInstanceExtension#canPerformAction` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `ItemInstanceExtension#getTagEnchantments` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `ItemInstanceExtension#getEnchantmentLevel` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to the hook of the block, item or fluid of the same name, which has no call site |
| `ItemInstanceExtension#getCraftingRemainder` | default method | both | planned | `ItemStack` forwarding to `ItemBehaviour`, new hook | M5 engine extension: forwards to `IItemExtension#getCraftingRemainder`, which has no hook; the engine code is at `pumpkin-data/src/item_stack/mod.rs:358` |

#### `LootTableSubProviderContextExtension` (target `LootTableSubProvider.Context`; 1 row: 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `LootTableSubProviderContextExtension#withConditions` | default method | both | not supported | - | data generation of loot tables |

#### `PackMetadataResourcesExtension` (target `PackMetadataResources`; 1 row: 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `PackMetadataResourcesExtension#isHidden` | default method | both | not supported | - | pack resource internals |

#### `PendingTagsExtension` (target `Registry.PendingTags`; 1 row: 1 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `PendingTagsExtension#contents` | abstract method | both | not supported | - | internal tag loading state |

#### `TooltipFlagExtension` (target `TooltipFlag`; 4 rows: 4 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `TooltipFlagExtension#hasControlDown` | default method | both | not supported | - | tooltip display only |
| `TooltipFlagExtension#hasShiftDown` | default method | both | not supported | - | tooltip display only |
| `TooltipFlagExtension#hasAltDown` | default method | both | not supported | - | tooltip display only |
| `TooltipFlagExtension#shouldDisplayAllInformation` | default method | both | not supported | - | tooltip display only |

#### `TypedInstanceExtension` (target `TypedInstance`; 1 row: 1 supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `TypedInstanceExtension#getData` | default method | both | supported | `ironpumpkin_mods::data_maps::DataMapType::get_data` | a data map entry is read by registry name |

#### `ValueInputExtension` (target `ValueInput`; 3 rows: 3 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `ValueInputExtension#keySet` | default method | both | not supported | `NbtCompound` in `EntityBase::read_custom_nbt` | Java-only: reader of the Java codec layer; Rust hooks read an `NbtCompound` |
| `ValueInputExtension#readChild` | default method | both | not supported | `NbtCompound` in `EntityBase::read_custom_nbt` | Java-only: reader of the Java codec layer; Rust hooks read an `NbtCompound` |
| `ValueInputExtension#rawChildOrEmpty` | default method | both | not supported | `NbtCompound` in `EntityBase::read_custom_nbt` | Java-only: reader of the Java codec layer; Rust hooks read an `NbtCompound` |

#### `ValueOutputExtension` (target `ValueOutput`; 2 rows: 2 not supported)

| Method | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `ValueOutputExtension#store` | default method | both | not supported | `NbtCompound` in `EntityBase::write_custom_nbt` | Java-only: writer of the Java codec layer; Rust hooks write an `NbtCompound` |
| `ValueOutputExtension#putChild` | default method | both | not supported | `NbtCompound` in `EntityBase::write_custom_nbt` | Java-only: writer of the Java codec layer; Rust hooks write an `NbtCompound` |

#### Types without a public method (2: 2 not supported)

| Interface | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `IEnchantmentExtension` | interface, no public method | both | not supported | - | declares no methods in 26.3 |
| `IHolderSetExtension.SerializationType` | interface, no public method | both | not supported | - | enum of the Java holder set serialization; a holder set is a tag name in `registry::tag_entries` |

### Client-only subsystems (families with counts)

| Family | Public types | Packages | Side | Status | Reason |
|:--|:--|:--|:--|:--|:--|
| client event helper types | 6 | `client.event` 6 | client | client-only | client source set (`src/client/java`); never loaded on a dedicated server |
| rendering | 62 | `client` 32, `client.color.item` 1, `client.entity.animation` 2, `client.entity.animation.json` 5, `client.fluid` 3, `client.pipeline` 3, `client.renderstate` 4, `client.stencil` 3, `client.submit` 4, `client.textures` 4, `client.util` 1 | client | client-only | client source set (`src/client/java`); never loaded on a dedicated server |
| model loaders | 74 | `client.model` 18, `client.model.ao` 1, `client.model.block` 5, `client.model.generators.blockstate` 5, `client.model.generators.loaders` 3, `client.model.generators.template` 8, `client.model.item` 6, `client.model.obj` 9, `client.model.pipeline` 3, `client.model.quad` 9, `client.model.standalone` 7 | client | client-only | client source set (`src/client/java`); never loaded on a dedicated server |
| screens and GUI | 48 | `client.config` 1, `client.gamerules` 4, `client.gui` 24, `client.gui.map` 2, `client.gui.modlist` 6, `client.gui.widget` 8, `client.loading` 1, `client.loading.earlydisplay` 2 | client | client-only | client source set (`src/client/java`); never loaded on a dedicated server |
| key mappings and input | 4 | `client.settings` 4 | client | client-only | client source set (`src/client/java`); never loaded on a dedicated server |
| client networking | 3 | `client.network` 1, `client.network.handling` 1, `client.network.registration` 1 | client | client-only | client source set (`src/client/java`); never loaded on a dedicated server |
| client extension interfaces | 28 | `client.extensions` 22, `client.extensions.common` 6 | client | client-only | client source set (`src/client/java`); never loaded on a dedicated server |
| client data generation | 4 | `client.data` 2, `client.data.internal` 2 | client | client-only | client source set (`src/client/java`); never loaded on a dedicated server |
| client registries and resources | 5 | `client.internal` 2, `client.registries` 1, `client.resources` 2 | client | client-only | client source set (`src/client/java`); never loaded on a dedicated server |

Client events (189) are counted in the event tables, not here.

### Other packages

#### Packages outside the families above (one row per package)

The root `common` package mixes families, so its types have one row each in the next table.

| Package | Public types | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `common.advancements.critereon` | 5 | both | planned | engine extension point (M5) | M5 engine extension: custom advancement predicates; Pumpkin advancement criteria are a fixed set. Types: ItemAbilityPredicate, PiglinCurrencyItemPredicate, PiglinNeutralArmorEntityPredicate, SnowBootsEntityPredicate, TridentEntityPredicate |
| `common.command` | 2 | both | planned | engine extension point (M5) | M5 engine extension: custom entity selector types; selectors are parsed by pumpkin-command. Types: EntitySelectorManager, IEntitySelectorType |
| `common.crafting` | 14 | both | planned | engine extension point (M5) | M5 engine extension: custom ingredient types; the recipe loader reads vanilla ingredients only. Types: BlockTagIngredient, CompoundIngredient, ConditionalRecipeOutput, CustomDisplayIngredient, DataComponentIngredient, DifferenceIngredient, ICustomIngredient, IRecipeContainer, ... |
| `common.damagesource` | 5 | both | planned | damage pipeline data on `EntityDamageEvent` | DamageContainer carries the reductions. Types: DamageContainer, DamageContainer.Reduction, IDeathMessageProvider, IReductionFunction, IScalingFunction |
| `common.data` | 18 | both | not supported | - | data generation providers; ported mods ship the generated files. Types: BlockTagCopyingItemTagProvider, BlockTagsProvider, ConditionalLootTableSubProviderContext, DataMapProvider, DataMapProvider.AdvancedBuilder, DataMapProvider.Builder, DatapackBuiltinEntriesProvider, GeneratingOverlayMetadataSection, ... |
| `common.data.fixes` | 1 | both | not supported | - | internal DataFixer additions. Types: NeoForgeEntityLegacyAttributesFix |
| `common.data.internal` | 16 | both | not supported | - | internal NeoForge data generation. Types: NeoForgeAdvancementProvider, NeoForgeBiomeTagsProvider, NeoForgeBlockTagsProvider, NeoForgeDamageTypeTagsProvider, NeoForgeDataMapsProvider, NeoForgeEnchantmentTagsProvider, NeoForgeEntityTypeTagsProvider, NeoForgeFluidTagsProvider, ... |
| `common.enums` | 1 | both | not supported | - | bubble column direction for IBlockExtension; follows that interface. Types: BubbleColumnDirection |
| `common.ticket` | 5 | both | planned | engine extension point (M5) | M5 engine extension: per-mod chunk tickets; not exposed by the pumpkin-world chunk system. Types: AABBTicket, ChunkTicketManager, ITicketGetter, ITicketManager, SimpleTicket |
| `common.tooltip` | 3 | both | not supported | - | tooltip display only. Types: ItemTooltipHandler, TooltipAppender, TooltipLocation |
| `common.util` | 46 | both | not supported | - | Java helpers (Lazy, maps, codecs, FakePlayer); Rust std covers the helpers, Pumpkin has no fake players. Types: AttributeTooltipContext, AttributeUtil, BlockRelocability, BlockRelocability.Multiblock, BlockRelocability.No, BlockRelocability.Yes, BlockSnapshot, CenterChunkPosComparator, ... |
| `common.util.flag` | 1 | both | not supported | - | mod feature flags stay excluded. Types: FeatureFlagLoader |
| `common.util.strategy` | 2 | both | not supported | - | Java hash strategies. Types: BasicStrategy, IdentityStrategy |
| `common.world` | 35 | both | planned | engine extension point (M5) | M5 engine extension: biome and structure modifiers and their builders; Pumpkin worldgen is Rust code. Types: AuxiliaryLightManager, BiomeGenerationSettingsBuilder, BiomeModifier, BiomeModifier.Phase, BiomeModifiers, BiomeModifiers.AddCarversBiomeModifier, BiomeModifiers.AddFeaturesBiomeModifier, BiomeModifiers.AddSpawnCostsBiomeModifier, ... |
| `common.world.chunk` | 7 | both | planned | engine extension point (M5) | M5 engine extension: forced chunk tickets per mod. Types: ForcedChunkManager, ForcedChunkManager.OwnedChunks, ForcedChunkManager.TicketTracker, LoadingValidationCallback, TicketController, TicketHelper, TicketSet |
| `common.world.poi` | 2 | both | planned | POI storage (pumpkin-world `poi/mod.rs`) | adds block states to existing POI types. Types: PoiStateSet, PoiTypeExtender |
| `data.event` | 6 | both | not supported | - | data generation. Types: DataGeneratorConfig, GatherDataEvent.GatherDataEventGenerator, PackGenerator, PackGenerator.DataProviderFromOutput, PackGenerator.DataProviderFromOutputLookup, PackGenerator.ItemTagsProvider |
| `data.loading` | 1 | both | not supported | - | data generation. Types: DatagenModLoader |
| `entity` | 2 | both | planned | `advanced_add_entity` spawn data (phase 3) | IEntityWithComplexSpawn; PartEntity (multipart entities) has no target. Types: IEntityWithComplexSpawn, PartEntity |
| `fluids` | 18 | both | planned | engine extension point (M5) | M5 engine extension: FluidType, FluidStack and flowing fluids; Pumpkin fluids are the generated vanilla set. Types: BaseFlowingFluid, BaseFlowingFluid.Flowing, BaseFlowingFluid.Properties, BaseFlowingFluid.Source, CauldronFluidContent, FluidInstance, FluidInteractionRegistry, FluidInteractionRegistry.FluidInteraction, ... |
| `fluids.crafting` | 10 | both | planned | engine extension point (M5) | M5 engine extension: fluid ingredients. Types: CompoundFluidIngredient, CustomDisplayFluidIngredient, DataComponentFluidIngredient, DifferenceFluidIngredient, FluidIngredient, FluidIngredientCodecs, FluidIngredientType, IntersectionFluidIngredient, ... |
| `fluids.crafting.display` | 5 | both | not supported | - | fluid slot displays for recipe viewers. Types: FluidSlotDisplay, FluidStackContentsFactory, FluidStackSlotDisplay, FluidTagSlotDisplay, ForFluidStacks |
| `gametest` | 2 | both | not supported | - | game test framework. Types: BlockPosValueConverter, GameTestHooks |
| `internal` | 9 | both | not supported | - | internal. Types: BrandingControl, CommonModLoader, DataComponentModifiers, LogoFileWarningsHandler, NeoForgeBindings, NeoForgeProxy, NeoForgeVersionCheck, NonExhaustiveEnum, ... |
| `junit` | 1 | both | not supported | - | test bootstrap. Types: JUnitMain |
| `logging` | 3 | both | not supported | - | internal crash report and packet dump helpers. Types: CrashReportExtender, PacketDump, ThreadInfoUtil |
| `mixins` | 2 | both | not supported | - | internal. Types: BlockEntityTypeAccessor, MappedRegistryAccessor |
| `model.data` | 4 | both | not supported | - | model data for client rendering. Types: ModelData, ModelData.Builder, ModelDataManager, ModelProperty |
| `resource` | 10 | both | planned | datapack reload hook (`datapack-manager.reload`) | reload listener ordering. Types: ContextAwareReloadListener, EmptyPackResources, EmptyPackResources.EmptyResourcesSupplier, JarContentsPackResources, JarContentsPackResources.JarContentsResourcesSupplier, ListenerKey, NeoForgeReloadListeners, ReloadListenerSort, ... |
| `server` | 2 | both | not supported | - | internal server lifecycle hooks. Types: LanguageHook, ServerLifecycleHooks |
| `server.command` | 7 | both | not supported | - | the `/neoforge` command tree. Types: CommandHelper, CommandUtils, EnumArgument, EnumArgument.Info, EnumArgument.Info.Template, ModIdArgument, NeoForgeCommand |
| `server.command.generation` | 5 | both | not supported | - | chunk pre-generation command. Types: CoarseOnionIterator, GenerationBar, GenerationTask, GenerationTask.Listener, OnionIterator |
| `server.console` | 1 | both | not supported | - | NeoForge console. Types: TerminalHandler |
| `server.dedicated` | 1 | both | not supported | - | NeoForge dedicated server hooks. Types: DedicatedLanServerPinger |
| `server.jsonrpc` | 5 | both | not supported | - | internal NeoForge methods for the management JSON-RPC server. Types: IMinecraftApiExtension, ModRecord, NeoForgeRpcMethods, NeoForgeSchemas, RegistryInfo |
| `server.loading` | 1 | both | not supported | - | server mod loading. Types: ServerModLoader |
| `server.permission` | 1 | both | supported | `Context::player_has_permission` (API) | PermissionAPI.getPermission. Types: PermissionAPI |
| `server.permission.exceptions` | 1 | both | not supported | - | permission handler errors. Types: UnregisteredPermissionException |
| `server.permission.handler` | 3 | both | not supported | - | custom permission handlers; Pumpkin has one permission manager. Types: DefaultPermissionHandler, IPermissionHandler, IPermissionHandlerFactory |
| `server.permission.nodes` | 6 | both | planned | `Permission` (`permission.wit`) | typed nodes (int, string) and dynamic contexts. Types: PermissionDynamicContext, PermissionDynamicContextKey, PermissionNode, PermissionNode.PermissionResolver, PermissionType, PermissionTypes |
| `server.timings` | 2 | both | not supported | - | NeoForge tick timing statistics. Types: ObjectTimings, TimeTracker |
| `transfer` | 13 | both | planned | `ironpumpkin_mods::transfer` | landed: `ResourceHandler` (indexed `insert_at` and `extract_at`), `CombinedResourceHandler`, `ResourceStacksResourceHandler`, `is_full` (`ResourceHandlerUtil.isFull`); planned: the delegating, empty, infinite, ranged and voiding handlers and the other `ResourceHandlerUtil` functions. Types: CombinedResourceHandler, DelegatingResourceHandler, EmptyResourceHandler, IndexModifier, InfiniteResourceHandler, ItemAccessResourceHandler, RangedResourceHandler, ResourceHandler, ... |
| `transfer.access` | 2 | both | planned | `ironpumpkin_mods::transfer::ItemAccess` | landed: `ItemAccess` and `StackItemAccess` (`ItemAccess.forStack`); planned: the player, handler and one-by-one accesses. Types: HandlerItemAccess, ItemAccess |
| `transfer.energy` | 9 | both | planned | `ironpumpkin_mods::transfer::EnergyHandler` | landed: `EnergyHandler`, `SimpleEnergyHandler`; planned: the delegating, empty, infinite, limiting, voiding and item access handlers; no vanilla energy providers. Types: DelegatingEnergyHandler, EmptyEnergyHandler, EnergyHandler, EnergyHandlerUtil, InfiniteEnergyHandler, ItemAccessEnergyHandler, LimitingEnergyHandler, SimpleEnergyHandler, ... |
| `transfer.fluid` | 7 | both | planned | `ironpumpkin_mods::transfer::FluidResource` | landed: `FluidResource` (Pumpkin fluids have no components), `FluidStacksResourceHandler`, `BUCKET_VOLUME`; planned: the bucket, cauldron and item access handlers, which need fluid storage that Pumpkin lacks. Types: BucketResourceHandler, CauldronWrapper, DispenseFluidContainer, FluidResource, FluidStacksResourceHandler, FluidUtil, ItemAccessFluidHandler |
| `transfer.item` | 16 | both | planned | `ironpumpkin_mods::transfer` over `Inventory` (pumpkin-inventory) | landed: `ItemResource`, `ItemStacksResourceHandler`, `VanillaContainerWrapper` (`transfer::inventory::InventoryWrapper`), `WorldlyContainerWrapper` (`SidedInventoryWrapper`), `ItemUtil.insertItemReturnRemaining`, `VanillaInventoryCodeHooks` for hoppers and droppers; planned: the bundle, composter, player inventory, equipment and item access handlers. Types: BundleItemHandler, CarriedSlotWrapper, ComposterWrapper, ContainerOrHandler, ItemAccessItemHandler, ItemResource, ItemStackResourceHandler, ItemStacksResourceHandler, ... |
| `transfer.resource` | 4 | both | planned | `ironpumpkin_mods::transfer::{Resource, ResourceStack}` | landed: `Resource`, `ResourceStack`; planned: `DataComponentHolderResource` and `RegisteredResource`, which wait for component access on resources. Types: DataComponentHolderResource, RegisteredResource, Resource, ResourceStack |
| `transfer.transaction` | 5 | both | supported | `ironpumpkin_mods::transfer::{Transaction, TransactionContext, Journal, SnapshotJournal, RootCommitJournal, Lifecycle}` | one root per thread; `Transaction` is `!Send` and a nested one borrows its parent; a journal belongs to the first root that touched it until that root closes; callbacks are panic safe; tests in `transfer::transaction`. Types: RootCommitJournal, SnapshotJournal, Transaction, Transaction.Lifecycle, TransactionContext |
| `world.inventory` | 1 | both | planned | menus (phase 6) | Types: StackCopySlot |

#### Types in the root `common` package

| Type | Kind | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|
| `BooleanAttribute` | class, 3 public methods | both | supported | `entity_attributes::AttributeBuilder::boolean`, `AttributeRange::Boolean` | `sanitizeValue` as in `NeoForge`; the tooltip methods are display only; tests `ranged_and_boolean_attributes_sanitize_like_neoforge`, `creative_flight_lets_a_survival_player_fly` |
| `CommonHooks` | class, 97 public methods | both | not supported | - | internal hook class called from patches |
| `CommonHooks.BiomeCallbackFunction` | interface, 1 public method | both | not supported | - | internal hook class called from patches |
| `CreativeModeTabRegistry` | class, 5 public methods | both | not supported | - | creative tabs are client UI |
| `DataMapHooks` | class, 6 public methods | both | not supported | - | internal hook class called from patches |
| `FarmlandWaterManager` | class, 3 public methods | both | planned | engine extension point (M5) | M5 engine extension: water sources for farmland outside the vanilla range; no Pumpkin hook |
| `IMinecartCollisionHandler` | interface, 4 public methods | both | planned | engine extension point (M5) | M5 engine extension: custom minecart collision; no Pumpkin hook |
| `IOUtilities` | class, 4 public methods | both | not supported | - | Java file helpers |
| `IOUtilities.WriteCallback` | interface, 1 public method | both | not supported | - | Java file helpers |
| `IShearable` | interface, 3 public methods | both | planned | shears and the `Shearable` trait of the mobs | M5 engine extension: NeoForge calls `isShearable` and `onSheared` of any block or entity from the shears item; Pumpkin shears vanilla mobs through the `Shearable` trait of each mob, and a mod block has no hook |
| `ItemAbilities` | 20 constants | both | planned | engine extension point (M5) | M5 engine extension: item abilities (tool actions); no Pumpkin equivalent |
| `ItemAbility` | class, 4 public methods | both | planned | engine extension point (M5) | M5 engine extension: item abilities (tool actions); no Pumpkin equivalent |
| `LenientUnboundedMapCodec` | class, 8 public methods | both | not supported | - | Java codec helper |
| `ModConfigSpec` | class, 15 public methods | both | supported | `ConfigSpec` | see the config section; tests `the_default_file_is_modid_type_toml_in_the_config_folder`, `an_out_of_range_value_is_clamped_and_the_file_rewritten` and `set_and_save_write_the_key_under_its_section` |
| `MonsterRoomHooks` | class, 2 public methods | both | not supported | - | internal hook class called from patches |
| `MutableDataComponentHolder` | interface, 14 public methods | both | supported | `ItemStack::{get_data_component, set_data_component, remove_data_component, has_data_component}` for vanilla components; `data_components::DataComponentType::{get, set, remove, has}` for mod components | the holder of NeoForge is the item stack; `update`, `copyFrom`, `applyComponents` and `applyComponentsAndValidate` are not provided, a port reads and then writes; tests `a_component_is_set_read_and_removed_over_the_default_of_the_item` (`pumpkin-data`), `a_mod_component_on_an_item_survives_a_save_and_reaches_a_synced_client` |
| `NeoForge` | class, 0 public methods | both | supported | `ModInit::register_event` (`NeoForge.EVENT_BUS`) | the class has one constant, the game bus; handlers register through `ModInit::register_event` and `register_parent_event`; tests `priority_order`, `blocking_before_non_blocking`, `receive_canceled`, `handler_on_a_parent` (`crates/pumpkin-core/tests/neoforge_event_bus.rs`) |
| `NeoForgeBuildType` | enum, 0 public methods | both | not supported | - | internal (`@ApiStatus.Internal`) |
| `NeoForgeEventHandler` | class, 11 public methods | both | not supported | - | internal (`@ApiStatus.Internal`) |
| `NeoForgeMod` | class, 12 public methods | both | not supported | - | NeoForge own mod class; its registered entries are in the registry tables |
| `OnlyInWarningsHandler` | class, 0 public methods | both | not supported | - | internal (`@ApiStatus.Internal`) |
| `PercentageAttribute` | class, 1 public method | both | supported | `entity_attributes::AttributeBuilder::ranged` | a ranged attribute; the percentage format is display only; tests `ranged_and_boolean_attributes_sanitize_like_neoforge`, `swim_speed_scales_the_swim_speed_of_a_mob_in_water` |
| `SoundAction` | class, 3 public methods | both | planned | engine extension point (M5) | M5 engine extension: fluid sound actions of the NeoForge fluid API |
| `SoundActions` | 4 constants | both | planned | engine extension point (M5) | M5 engine extension: fluid sound actions of the NeoForge fluid API |
| `SpecialPlantable` | interface, 3 public methods | both | planned | engine extension point (M5) | M5 engine extension: custom plantable items for villagers and dispensers; no Pumpkin hook |
| `Tags` | class, 0 public methods | both | planned | tag names: `registry::is_in_tag`, `registry::tag_entries` over the generated `c:` and `neoforge:` tags | holder rule: 5 of 10 nested classes are supported (`Enchantments`, `EntityTypes`, `Fluids`, `Potions`, `Structures`); the other 5 miss tag ids, listed in their rows; tag names resolve through `registry::is_in_tag` and `registry::tag_entries`, test `tag_lookups_answer_for_generated_and_mod_tags` |
| `Tags.Biomes` | 94 constants, 94 tag ids (94 `c:`, 0 `neoforge:`) | both | planned | add the missing ids to `assets/conventional_tags` and regenerate | 1 of 94 ids missing: `c:primary_wood_type/poplar` |
| `Tags.Blocks` | 138 constants, 137 tag ids (132 `c:`, 5 `neoforge:`) | both | planned | add the missing ids to `assets/conventional_tags` and regenerate | 5 of 137 ids missing: `neoforge:enderman_place_on_blacklist`, `neoforge:needs_wood_tool`, `neoforge:needs_gold_tool`, `neoforge:needs_netherite_tool`, `neoforge:villager_farmlands`; 1 constant is built another way (for example `DyeColor.getTag()`), not checked |
| `Tags.DamageTypes` | 7 constants, 7 tag ids (0 `c:`, 7 `neoforge:`) | both | planned | add the missing ids to `assets/conventional_tags` and regenerate | 7 of 7 ids missing: `neoforge:is_magic`, `neoforge:is_poison`, `neoforge:is_wither`, `neoforge:is_environment`, `neoforge:is_physical`, ... |
| `Tags.Enchantments` | 7 constants, 7 tag ids (7 `c:`, 0 `neoforge:`) | both | supported | conventional tags in pumpkin-data (`generated/tag.rs`, from `assets/conventional_tags`) | all ids present |
| `Tags.EntityTypes` | 6 constants, 6 tag ids (6 `c:`, 0 `neoforge:`) | both | supported | conventional tags in pumpkin-data (`generated/tag.rs`, from `assets/conventional_tags`) | all ids present |
| `Tags.Fluids` | 12 constants, 12 tag ids (12 `c:`, 0 `neoforge:`) | both | supported | conventional tags in pumpkin-data (`generated/tag.rs`, from `assets/conventional_tags`) | all ids present |
| `Tags.Items` | 276 constants, 258 tag ids (254 `c:`, 4 `neoforge:`) | both | planned | add the missing ids to `assets/conventional_tags` and regenerate | 4 of 258 ids missing: `neoforge:enchanting_fuels`, `neoforge:piglin_usable_crossbows`, `neoforge:pillager_usable_crossbows`, `neoforge:skeleton_usable_bows`; 18 constants are built another way (for example `DyeColor.getTag()`), not checked |
| `Tags.Potions` | 1 constants, 1 tag ids (1 `c:`, 0 `neoforge:`) | both | supported | conventional tags in pumpkin-data (`generated/tag.rs`, from `assets/conventional_tags`) | all ids present |
| `Tags.Structures` | 2 constants, 2 tag ids (2 `c:`, 0 `neoforge:`) | both | supported | conventional tags in pumpkin-data (`generated/tag.rs`, from `assets/conventional_tags`) | all ids present |
| `Tags.WorldClocks` | 2 constants, 2 tag ids (0 `c:`, 2 `neoforge:`) | both | planned | add the missing ids to `assets/conventional_tags` and regenerate | 2 of 2 ids missing: `neoforge:ignores/pause_command`, `neoforge:ignores/advance_time_rule` |
| `TranslatableEnum` | interface, 1 public method | both | not supported | - | client display names |
| `UsernameCache` | class, 3 public methods | both | not supported | - | NeoForge username cache file; Pumpkin keeps its own user cache (`server.data.user_cache`) |

## Risks / Trade-offs

- [The side column is a heuristic. An event built in `EventHooks` or `CommonHooks` counts as `both` even when only one side calls the hook.] -> Status does not depend on `both` versus `server`; a port in M4 Native API with NeoForge semantics checks the call site in the NeoForge source before it maps the event.
- [A `supported` row can still differ in detail: payload fields, cancel semantics, fire order.] -> The mapping table has a test per supported row, and the reason column names the known gaps.
- [Pumpkin adds or starts firing events upstream, so statuses go stale.] -> Rerun the generator after each `git merge upstream/master`. Downgrades from unfired events show up on their own.
- [NeoForge 26.3.x moves on: classes, payloads and registries change per release.] -> Rerun against the new commit and diff the tables.
- [Method rows group overloads. A port can need one overload that the native API lacks.] -> The mapping table lists overloads where a ported mod uses them.
- [Most rows are `planned`.] -> That is the gap that M4 Native API with NeoForge semantics and M5 Modded gameplay parity close. A row with the reason prefix `M5 engine extension:` waits for an extension point of the engine. Pick the order from `modpack-usage.md` (#46), not from this list.
- [The internal hooks that fire NeoForge events touch upstream functions, so `git merge upstream/master` can conflict there.] -> The hooks are host code in bounded modules, and each event has a test; a merge conflict moves the hook, and the API version of mods does not change.
- [A fire site on a hot path (per entity, per tick) slows the server.] -> The performance guardrail: the handler check comes before the event construction, and each wave that adds fire sites runs the world benches and the dispatch bench with zero listeners.
- [Source patches break on every IronPumpkin update that changes the lines they touch.] -> A patch names its commit, and the build fails with the mod, the patch and both commits before it compiles. The mod author writes the patch again, or the pack stays on the old commit. An event that replaces a common patch removes the cost for every mod.
- [Two mods patch the same lines.] -> The build fails and names both mods and the file. The pack maintainer orders the patches, shares one patch, or proposes an event.
- [Mods use source patches where an event exists.] -> The justification file says why a NeoForge event or API is not enough, and a pack maintainer reads it before adding the mod.
- [A native mod can crash or corrupt the server: no sandbox.] -> The same trust model as Java mods. A pack lists its mods in source form, and untrusted code goes to the Wasm channel.
- [The GPL-3.0 constraint excludes ports of mods whose licence forbids it.] -> Such a mod can go to the Wasm channel where its hooks are coarse enough and it needs no patch, or stays unported.
- [A pack needs its own binary, so a mod change means a rebuild.] -> The blueprint GitHub Action builds per tag with a cached `target/`; plugins that need hot reload stay Wasm.

## Open Questions

- Which planned rows M4 Native API with NeoForge semantics implements first. `modpack-usage.md` (#46) answers it; the answer changes the task order, not this inventory.
- Which events beyond the NeoForge inventory enter the API first. The justification files of real patches answer it; the answer adds events, not primitives.
