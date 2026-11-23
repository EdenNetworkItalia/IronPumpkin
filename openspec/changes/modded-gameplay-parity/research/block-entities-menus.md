# Block entity types and menu types (phase 6 research)

Read-only research. Paths are relative to the repository root unless they start with `ref/`
(`target/scratch/ref/neoforge-src`, `target/scratch/ref/minecraft-src`). Nothing here is a decision.

## 0. Summary

- Pumpkin has a closed block entity type set. The type name list is generated
  (`BLOCK_ENTITY_TYPES`), the factory is two hand-written `match` blocks on the name
  (`crates/pumpkin-core/src/block/entities/mod.rs:360`, `:545`), and a block state stores the type
  as a `u16` index (`crates/pumpkin-data/src/block_state.rs:41`).
- M2 and M3 already give a mod block the block entity of a VANILLA type
  (`BlockEntityTypeAddBlocksEvent`). A mod cannot define a NEW block entity type.
- The menu side is open for vanilla window types only. A menu has a `WindowType` (a generated
  enum with 25 variants, `crates/pumpkin-data/src/generated/screen.rs:3`), and `COpenScreen`
  writes the enum discriminant as the menu registry id (`crates/pumpkin-core/src/entity/player.rs:7028`).
- Both registries are synced registries on a NeoForge server
  (`ref/neoforge-src/net/neoforged/neoforge/registries/NeoForgeRegistriesSetup.java:40-41`).
  The M3 sync list has 7 rows and neither registry is in it
  (`crates/pumpkin-core/src/net/java/configuration_tasks.rs:58`).
- Demand: 90 of the 98 mods that register blocks also register a `block_entity_type`. 78 mods
  register a `menu`. See section 4.

## 1. Block entity types

### 1.1 How Pumpkin defines a block entity today

| Aspect | Where | Notes |
|:--|:--|:--|
| Trait | `crates/pumpkin-core/src/block/entities/mod.rs:78` `BlockEntity: Any + Send + Sync` | Required: `write_nbt`, `from_nbt` (`Self: Sized`), `resource_location`, `get_position`, `as_any`. All other methods have defaults (tick, inventory, dirty flags, item components, `chunk_data_nbt`). |
| Type id on the wire | `mod.rs:117` `get_id()` | Position of the bare name in `BLOCK_ENTITY_TYPES`, `unwrap_or(0)`. An unknown name sends id 0 (`furnace`). |
| Block to type map | `crates/pumpkin-data/src/block_state.rs:41` `BlockState::block_entity_type: u16` | `u16::MAX` = none. One type per STATE, so one type per block. Vanilla allows a block in the valid set of several types. |
| Custom block to type | `crates/pumpkin-data/src/dynamic_types.rs:74` `add_block_entity_block`; read at the freeze `crates/pumpkin-data/src/dynamic.rs:909-976` | The mod block takes a generated index. Registration after the freeze returns `RegistryFrozen`. |
| Mod API | `crates/ironpumpkin-mods/src/registration.rs:463-500` `BlockEntityTypeAddBlocksEvent::modify`; `crates/ironpumpkin-mods/src/lib.rs:261` | Takes only `minecraft:` names of `BLOCK_ENTITY_TYPES` (`registration.rs:487-499`). Unknown type: warning, no change. |
| Creation on place | `crates/pumpkin-core/src/block/registry.rs:1263-1268` (`on_placed`); `crates/pumpkin-core/src/world/mod.rs:5996-6000` (flag `SKIP_BLOCK_ADDED_CALLBACK`); both call `create_block_entity_for_state` (`entities/mod.rs:513`) | The factory is a `match` on the name string. `structure_block` and `command_block` have a state-dependent constructor (`mod.rs:531-538`). |
| Creation from NBT | `entities/mod.rs:360` `block_entity_from_nbt`: a `match` on the `id` string | Called by chunk wake-up (`world/mod.rs:7293` `get_block_entity`), item `block_entity_data` on place (`block/registry.rs:897`), falling blocks (`entity/falling.rs:151`), Bedrock paths. |
| Live storage | `world/mod.rs:301` `block_entities: DashMap<Vector2<i32>, FxHashMap<BlockPos, Arc<dyn BlockEntity>>>` | Plus `pending_block_entities` (raw NBT) in each chunk. A block entity wakes lazily on first `get_block_entity`. |
| Ticking | `world/mod.rs:1925-1944`: collects the block entities of active chunks, `par_chunks(16)`, `be.tick(self)` | Every live block entity ticks. No per-type ticker, no state argument (NeoForge `BlockEntityTicker` gets level, pos, state). A mod type gets ticking for free with the trait method. |
| NBT save | `world/mod.rs:754-776` `save_block_entities`: `write_internal` (writes `id`, `x`, `y`, `z`, then `write_nbt`), then `PumpkinCustomData`, then `attachments::write_block_entity` | Runs on chunk unload and on the periodic save (`world/mod.rs:1722` flush). |
| NBT load | `world/mod.rs:7293-7330` `get_block_entity`: `block_entity_from_nbt`, then `attachments::read_block_entity` | An unknown `id` returns `None`; the raw NBT stays in the chunk and is written back unchanged (design.md "World data format", open rows). This already protects the data of an unregistered mod type. |
| Client sync, single | `world/mod.rs:7391-7420` `add_block_entity` and `:7518-7540` `update_block_entity`: send `CBlockEntityData(pos, VarInt(get_id()), nbt)` when `chunk_data_nbt()` is `Some` | `chunk_data_nbt` defaults to `None`, so a type syncs nothing unless it overrides it. |
| Client sync, chunk | `crates/pumpkin-core/src/net/java/chunk_data/v1_18.rs:196-218` | Writes the type id from the NBT `id` by the same name lookup (`unwrap_or(0)`), sends the whole saved NBT minus `id`, `x`, `y`, `z`, `LootTable*`, `PumpkinCustomData`, `BukkitValues`. A mod type with private save fields leaks them to the client unless it is filtered. |
| Item component | `crates/pumpkin-protocol/src/codec/data_component.rs:2850-2885` `BlockEntityDataImpl` | Serialises and parses the type id by the same `BLOCK_ENTITY_TYPES` index. A third place with the closed list. |
| Hooks used by capabilities | `crates/ironpumpkin-mods/src/capabilities/mod.rs:1038-1060` `register_block_entity` | Accepts only a vanilla `BLOCK_ENTITY_TYPES` name (`UnknownBlockEntityType` otherwise). |
| Attachments (#168) | `crates/ironpumpkin-mods/src/attachments/hooks.rs:111-140`, `:255-304`; `world/mod.rs:773`, `:7316` | Keyed by the block entity instance, not by the type. Works for a mod type with no change. Block entity attachments sync over `neoforge:sync_attachments` by position. |

Closed lists, counted: the type name is resolved by `BLOCK_ENTITY_TYPES` index in 5 places
(`entities/mod.rs:117`, `v1_18.rs:196`, `data_component.rs:2858` and `:2877`,
`entities/mod.rs:526` and `:553`), and by the factory `match` in 3 places (`mod.rs:360`, `:513`,
`:545`).

### 1.2 What NeoForge defines (the contract to port)

- `BlockEntityType` holds a supplier and a valid block set
  (`ref/minecraft-src/net/minecraft/world/level/block/entity/BlockEntityType.java:15-55`).
  `BlockEntityType.Builder` is GONE in 26.3: a mod calls `new BlockEntityType<>(supplier, blocks...)`
  (constructors at `:20-38`). 112 mods reference the Builder class of 21.1
  (`modpack-usage.md:1121`), so a port of every mod changes this call.
- The registry `minecraft:block_entity_type` is a sync registry
  (`ref/neoforge-src/net/neoforged/neoforge/registries/NeoForgeRegistriesSetup.java:40`,
  comment: "Required for BlockEntity packets"). The ids of the snapshot are the ids that
  `ClientboundBlockEntityDataPacket` and the chunk packet carry.
- `BlockEntityTypeAddBlocksEvent.modify` replaces the valid set of a type
  (`ref/neoforge-src/net/neoforged/neoforge/event/BlockEntityTypeAddBlocksEvent.java:45-60`).
  It runs for mod types too, so a mod can add a block to the type of another mod.
- Sync content: vanilla `getUpdatePacket` and `getUpdateTag`
  (`ref/minecraft-src/net/minecraft/world/level/block/entity/BlockEntity.java:231-235`).

### 1.3 What a registration API for a mod type needs

1. **Registry id.** The type needs a stable id after the generated count, allocated at the freeze in
   name order (as blocks, `dynamic.rs:478` `block_snapshot`), and a name. Saves keep the name only.
2. **Valid blocks.** A set of block names. Today one type per state. A mod type with the valid set
   of one block fits the current `u16`. A block that two types claim needs an error or the first-wins
   rule that `add_block_entity_block` already uses (`dynamic_types.rs:81`).
3. **Factory.** `fn(BlockPos, BlockStateId) -> Arc<dyn BlockEntity>` (NeoForge `BlockEntitySupplier`
   gets pos and state). The pattern exists for entity types: `EntityFactory = fn(Entity) -> Arc<dyn EntityBase>`
   (`crates/pumpkin-core/src/entity/custom.rs:19`), registered with the definition under one lock
   (`custom.rs:26-37`), exposed by `ModInit::register_entity_type` (`crates/ironpumpkin-mods/src/lib.rs:528`).
4. **Load from NBT.** The same factory plus a `read` step, because `from_nbt` has `Self: Sized` and
   cannot be called through `dyn`. The trait needs a `load_nbt(&self, nbt)` method (interior
   mutability, as the vanilla block entities do) or a second `fn(&NbtCompound, BlockPos)` in the
   registration.
5. **Save.** Already `write_nbt`. No change.
6. **Ticking.** Works with the trait `tick`. A NeoForge ticker returns `null` for client side or for
   a non-ticking state; the host cost of an empty `tick` over `par_chunks(16)` is one virtual call per
   block entity per tick. A mod that stores 100k block entities pays that. An opt-in flag on the type
   (`ticks: bool`) avoids the call.
7. **Client sync.** `chunk_data_nbt` plus the type id from the registry. The two id lookups
   (`get_id`, `v1_18.rs:196`) must use the frozen registry instead of `BLOCK_ENTITY_TYPES`.
8. **Wire id on a NeoForge client.** The id in `CBlockEntityData`, in the chunk packet and in
   `BlockEntityDataImpl` must be the id of the `neoforge:frozen_registry` snapshot of
   `minecraft:block_entity_type`.

### 1.4 M4 hooks that exist, and how they combine

| Hook | Where | Effect on a mod type |
|:--|:--|:--|
| `BlockEntityTypeAddBlocksEvent` (#122) | `registration.rs:463`; `lifecycle.rs:91` (runs after the freeze phase list) | Needs `add_blocks` to accept a mod type name. Today `registration.rs:487-499` strips `minecraft:`. |
| Attachments (#168) | `attachments/hooks.rs` | No change. |
| Capabilities (#153) | `capabilities/mod.rs:1038-1060` and `:400`, `:555` (`block_entity_type == u16::MAX` checks) | `register_block_entity` rejects a mod type (`known` check). The capability lookup at `:400` and `:555` reads `block.default_state.block_entity_type` and indexes `BLOCK_ENTITY_TYPES`; both need the dynamic name table. |
| Block entity attachments sync | `hooks.rs:255-304` | Needs the block entity to be live at the position; fine. |

`registerBlockEntity` is called by 103 mods in 493 call sites (`modpack-usage.md:1654`), so the
capability row is the largest user of the type name after the registration itself.

### 1.5 How the M2 content registry extends

The M2 shape is: a pending map, a `register_*` that validates, a `freeze()` that allocates ids
after the generated count in name order, and immutable tables
(`crates/pumpkin-data/src/dynamic.rs:1612` `freeze`, `crates/pumpkin-data/src/dynamic_registries.rs:1-20`).
`ContentKind` has 7 variants (`dynamic.rs:171`). A block entity type is a name, a valid block list
and a factory, with no generated data. It is the same size of problem as `SoundEventDefinition`
(`dynamic_registries.rs:30`), plus the factory that `EntityFactory` shows.

M3 sync: a `ContentTables::*_snapshot` fn and one row in `SYNCED_REGISTRIES`
(`configuration_tasks.rs:58-93`). The array type is `[_; 7]` and `RegistrySyncPayloads::encode`
walks it (`:114`). One new snapshot is one new row. The snapshot must list the 49 generated
vanilla names plus the mod names, as `block_snapshot` does (`dynamic.rs:478`), because
`BLOCK_ENTITY_TYPES` in this checkout is the vanilla list.

### 1.6 Options

**Option A. New registry kind in `pumpkin-data` (`dynamic_types.rs` or `dynamic_registries.rs`), with
factory in `pumpkin-core`.**
- `pumpkin-data`: new `BlockEntityTypeDefinition { name, valid_blocks }`, a `ContentKind::BlockEntityType`
  variant, a `block_entity_type_snapshot`, and an id table. Build the `u16` index of a mod type as
  `BLOCK_ENTITY_TYPES.len() + position`, so the existing `block_entity_type: u16` field stays and
  `u16::MAX` stays "none". No generated file changes.
- `pumpkin-core`: a `custom` module like `entity/custom.rs` with `BlockEntityFactory`; `create_block_entity`
  (`mod.rs:545`) and `block_entity_from_nbt` (`mod.rs:360`) gain a fallthrough arm before `_ => None`.
  `get_id`, `v1_18.rs:196`, `data_component.rs:2858` and `:2877` call one helper
  `block_entity_type_id(name)` that checks the dynamic table when the generated list misses.
- Mod API: `ModInit::register_block_entity_type(def, factory)`, and a `ContentKind` row.
- Upstream merge cost: 3 touched files in upstream hot paths, each with an added arm or a helper call
  (`entities/mod.rs`, `v1_18.rs`, `data_component.rs`). None is generated. The `match` arms sit at the
  end of the function, so a conflict is a one-line context change. Low.
- Performance: the lookup runs only when the generated list misses; vanilla types do not pay. The
  tick path is unchanged. Wire format: ids are the registry snapshot ids; a world saves names.
  Save format: unchanged (`id` string).
- Risk: `BLOCK_ENTITY_TYPES` has 49 names, but the generated `u16` index and the NeoForge client
  registry order differ: the client assigns ids from the snapshot, so the server must send the
  snapshot with the same ids it writes in packets. Block and item already do this.

**Option B. Keep vanilla types only; a mod block entity reuses a vanilla type and keeps its data in
attachments or `custom_block_entity_data`.**
- No new files. Works for data, not for behaviour: ticking and capability lookups then need a
  per-block hook, not a per-type one. 90 mods register a type of their own with their own class
  (`modpack-usage.md:1490`), so this path cannot port them without a rewrite. Not a port.

**Option C. Make the existing two `match` blocks data driven (`BLOCK_ENTITY_FACTORIES` table).**
- Changes 50 arms in an upstream hot file. High upstream conflict cost, no gain for mods.

**Recommendation: Option A.** It is additive, keeps the `u16` state field and the NBT `id`, and reuses
the entity factory pattern and the M3 snapshot row. Open questions for the owner: whether a mod
block entity type may list a vanilla block (NeoForge allows it), and whether the `u16` limit
(65534 indexes, shared with the 49 vanilla types) is enough; it is.

## 2. Menu types

### 2.1 What exists

| Aspect | Where |
|:--|:--|
| Vanilla menu type | `WindowType` enum, 25 variants, `crates/pumpkin-data/src/generated/screen.rs:3` (generated). The `COpenScreen` id is `window_type as i32` (`crates/pumpkin-core/src/entity/player.rs:7028`, `:7198`). Bedrock mapping at `player.rs:542`. |
| Screen handler | `ScreenHandler` trait; `window_type()` returns `Option<WindowType>` (`crates/pumpkin-inventory/src/screen_handler.rs:288`). |
| M4 menus (#154) | `crates/ironpumpkin-mods/src/menus/` (`mod.rs` 618 lines, `handler.rs` 850, `layout.rs`, `scope.rs`, `tests.rs`). `open_menu` builds a `Factory` and calls `player.open_handled_screen` (`menus/mod.rs:538-568`). `ModMenu` sets `Some(definition.window_type)` (`menus/handler.rs:396`). |
| Layout check | `menus/layout.rs:30-60` `vanilla_layout`: the vanilla client builds the menu of a vanilla type itself, so the slot count and data slot count must match. A container content packet with more slots throws on the client (`layout.rs:1-8`). |
| Data slots | `ContainerData` trait with `i32` values (`menus/mod.rs:120-130`). The host sends 16-bit values: `CSetContainerProperty { window_id, property: i16, value: i16 }` (`crates/pumpkin-protocol/src/java/client/play/set_container_property.rs:9-14`), written from `crates/pumpkin-inventory/src/sync_handler.rs:117` and `:203` (`*property as i16`). A value above 32767 wraps. |
| Payload channels | `neoforge:advanced_open_screen` and `neoforge:advanced_container_set_data` are in the channel list the server advertises (`crates/pumpkin-core/src/net/java/neoforge.rs:241`, `:256`), but nothing in `crates` writes them (`rg advanced_open_screen crates` gives only these lines and a doc comment, `menus/mod.rs:10`). |
| Registry sync | `menu` is not in `SYNCED_REGISTRIES` (`configuration_tasks.rs:58-93`). |

### 2.2 What NeoForge does

- `menu` is a sync registry (`NeoForgeRegistriesSetup.java:41`, comment "Required for
  ClientboundOpenScreenPacket").
- `ServerPlayer.openMenu(provider, extraDataWriter)`
  (`ref/minecraft-src/net/minecraft/server/level/ServerPlayer.java:1470-1500`):
  1. writes `provider.writeClientSideData` and the caller's `extraDataWriter` into one buffer;
  2. if the buffer is not empty, sends `neoforge:advanced_open_screen`
     (`AdvancedOpenScreenPayload`, line 1490), else the vanilla `ClientboundOpenScreenPacket`
     (line 1498);
  3. then `initMenu` and posts `PlayerContainerEvent.Open`.
- Payload codec (`ref/neoforge-src/net/neoforged/neoforge/network/payload/AdvancedOpenScreenPayload.java:27-35`):
  `VAR_INT windowId`, `idMapper(BuiltInRegistries.MENU)` (a var int id from the synced registry),
  `ComponentSerialization.STREAM_CODEC name`, `UNBOUNDED_BYTE_ARRAY additionalData`.
  The client wraps `additionalData` in a `RegistryFriendlyByteBuf` and calls
  `menuType.create(windowId, inventory, buf)` (`ClientPayloadHandler.java:143-163`).
- Data slots (`ServerPlayer.java:323-330`): when the connection has the channel
  `neoforge:advanced_container_set_data`, the server sends
  `AdvancedContainerSetDataPayload(byte containerId, short dataId, VAR_INT value)`
  (`AdvancedContainerSetDataPayload.java:25-30`); else the vanilla 16-bit packet. The client handler
  converts it to the vanilla packet (`ClientPayloadHandler.java:181-183`). The initial full sync
  also goes through `sendDataChange`, so the first value also needs the check.
- Registration: `IMenuTypeExtension.create(IContainerFactory)` returns a `MenuType`
  (`ref/neoforge-src/net/neoforged/neoforge/common/extensions/IMenuTypeExtension.java:21-23`). The
  factory takes `(windowId, inventory, RegistryFriendlyByteBuf)` and runs on the CLIENT. The server
  never calls `MenuType.create`; it only needs the type id for the packet.

### 2.3 What a mod menu type needs on the server

1. A registry entry: a name and nothing else. The client factory is client code, the server needs
   no constructor (the menu class on the server is the `MenuLogic` of M4). It is the cheapest of
   the three: the sound event row of `dynamic_registries.rs` is the model.
2. An id in the `neoforge:frozen_registry` snapshot of `minecraft:menu`: vanilla ids 0..24 (the
   `WindowType` order, which is the 26.3 `BuiltInRegistries.MENU` order) plus mod ids in name order.
   One new `SYNCED_REGISTRIES` row and one snapshot fn.
3. `MenuDefinition` takes a menu type that is either a `WindowType` or a mod menu type id
   (`menus/mod.rs` `MenuDefinition::new(WindowType, ..)`). The `layout.rs` check applies to
   vanilla types only; a mod type has no server-side layout limit (the client builds it from the
   mod's own factory).
4. The open packet: `COpenScreen` for a vanilla type with no extra data; for a mod type, or with
   extra data, `neoforge:advanced_open_screen` with the payload above. The registration needs the
   extra data writer (`menus::open_menu` gets an `extra_data: Vec<u8>` or a closure).
   The player must have the channel; a NeoForge client that passed the handshake has it
   (`neoforge.rs:241`).
5. Data slots: send `neoforge:advanced_container_set_data` when the player has the channel,
   for vanilla and mod menus alike (the vanilla packet loses the high 16 bits of a mod value, for
   example a machine energy of 100000). The change is in `sync_handler.rs:117` and `:203`, which
   hold a `CSetContainerProperty`; the screen handler `enqueue_property_packet` signature
   (`player_screen_handler.rs:319`, `crates/pumpkin-inventory/src/screen_handler.rs:191`) takes
   the packet type, so the signature changes or the sender wraps it.
6. The Bedrock mapping (`player.rs:542`) takes a `WindowType`. Out of scope per `config.yaml`; a mod
   type returns the generic container type.

### 2.4 Options

**Option A. Add a mod menu id next to `WindowType` (a `MenuTypeId` newtype: vanilla `WindowType` or
a mod index), threaded through `ScreenHandlerBehaviour`.**
- `ScreenHandlerBehaviour::window_type: Option<WindowType>` (`screen_handler.rs`) is used in 220 lines (`rg window_type crates`, tests and generated files excluded)
  across `pumpkin-inventory`, `player.rs` and `plugin/api/gui.rs`. Changing its type touches
  upstream hot files and the Wasm GUI API. High upstream merge cost.

**Option B. Keep `window_type` for vanilla types and add a second field `mod_menu: Option<u16>` on
`ScreenHandlerBehaviour`.**
- One new field and one branch in the two open-screen sites (`player.rs:7028`, `:7198`). Existing
  call sites do not change. A mod menu has `window_type = None`, and `open_handled_screen`
  currently returns `None` for it (`player.rs:7025` `window_type()?`), so that line changes.
  Upstream merge cost: low to medium (2 files with small hunks). Recommended.

**Option C. Keep the packet logic in `ironpumpkin-mods`: the host sends the payload from the `menus`
module after `open_handled_screen`, and `window_type` stays a vanilla type that the vanilla open
packet suppresses.**
- Needs a hook in `open_handled_screen` to skip `COpenScreen`. Same files as B, so no saving.

**Data slots.** The send site is one function pair (`sync_handler.rs:117`, `:203`). A bounded change:
a helper `send_property(player, sync_id, index, value: i32)` picks the payload or the vanilla packet
by channel. The vanilla packet stays for clients without the channel. Wire risk: the payload
byte order (`byte`, `short`, var int) is fixed by the source above; test against a
`neoforge` capture, as the config says.

**Recommendation: Option B** for the menu type and the helper for data slots. Both are
required by the 78 registering mods and by every mod with a machine that exceeds 32767 in a data
slot. The `neoforge:advanced_open_screen` payload is also needed for any vanilla `WindowType`
that carries extra data (a mod menu of a vanilla type, such as the 79 mods that call
`IMenuTypeExtension.create`).

### 2.5 Interaction with block entity types

A mod menu opens from a block, so `still_valid_at` (`menus/mod.rs:573`) and the container come from
a block entity (`SlotSource::Container(Arc<dyn Inventory>)`, `menus/mod.rs:46`, from
`BlockEntity::get_inventory`, `entities/mod.rs:135`). With a mod block entity type (section 1) the
inventory is the mod's own type behind the `dyn BlockEntity`; no menu change is needed for that.
The common path (a mod block opens its menu with the block position as extra data,
`IPlayerExtension.openMenu(provider, pos)`, `IPlayerExtension.java:59-60`) needs the extra data
writer of 2.3 point 4.

## 3. Order of work

1. Menu registry entry, snapshot row and `advanced_open_screen`: independent of block entities.
2. Block entity type registry: needs the id allocation and the factory.
3. Data slot helper: independent.
4. The capability and `BlockEntityTypeAddBlocksEvent` change for mod types depends on 2.

## 4. Demand (from `modpack-usage.md` and `mapping-table.md`)

Counts are mods of the 414 in the reference modpack that reference the item.

| Item | Mods | Source |
|:--|--:|:--|
| Mods registering a `block_entity_type` | 90 (558 `DeferredRegister` calls, 86 mods use `DeferredRegister`) | `modpack-usage.md:1490`; `mapping-table.md:331` |
| Mods that touch the `block_entity_type` registry | 143 | `modpack-usage.md:1490` |
| Mods registering a `block` | 98 | `modpack-usage.md:1488` |
| `BlockEntity` | 235 | `modpack-usage.md:1030` |
| `BlockEntityType` | 191 | `modpack-usage.md:1049` |
| `BlockEntityType$BlockEntitySupplier` | 144 | `modpack-usage.md:1080` |
| `BlockEntityTicker` | 114 | `modpack-usage.md:1116` |
| `BlockEntityType$Builder` (GONE in 26.3) | 112 | `modpack-usage.md:1121` |
| `ClientboundBlockEntityDataPacket` | 91 | `modpack-usage.md:1141` |
| `RegisterCapabilitiesEvent.registerBlockEntity` | 103 (493 call sites) | `modpack-usage.md:1654`; supported for vanilla types only |
| `BlockEntityTypeAddBlocksEvent` | 8 | `mapping-table.md:137` (supported) |
| Mods registering a `menu` | 78 (366 `DeferredRegister` calls) | `modpack-usage.md:1491`; `mapping-table.md:334` |
| Mods that touch the `menu` registry | 113 | `modpack-usage.md:1491` |
| `AbstractContainerMenu` | 172 | `mapping-table.md:1408`; gap row "Container menus" 176 mods |
| `MenuType` | 157 | `modpack-usage.md:1073` |
| `IContainerFactory` (menu with extra data) | 84 | `mapping-table.md:624` (planned) |
| `IMenuTypeExtension` | 70 | `mapping-table.md:827` (planned) |
| `IMenuTypeExtension.create` call sites | 79 mods, 231 sites | `modpack-usage.md:1757` |
| `RegisterMenuScreensEvent` (client) | 116 | `modpack-usage.md:1473` (client only, no server work) |

Mods in `modpack-usage.md` that need both: `ironchest` (`:408`, BlockEntityType and block states),
`metalbarrels` (`:468`), `stonechest` (`:581`), `extendedae` (`:352`, MenuType, 483 classes),
`refinedstorage_quartz_arsenal` (`:530`), `stepcrafter` (`:580`),
`craftingstation` (`:302`), `routers` (`:551`), `fluxnetworks` (`:363`),
`custommachinerycreate` (`:325`), `createenderstorage` (`:316`). The storage and machine mods
(`ironchest`, `metalbarrels`, `stonechest`) are the smallest cases: one block entity type per
block, one menu type with block position extra data.

## 5. What this report did not measure

- The exact count of mods that need BOTH registries is not in the two documents (the registry
  table is per registry, not per mod). A script over `modpack-usage.md` section 3 would give it.
- No NeoForge 26.3 server capture of `neoforge:frozen_registry` for `minecraft:block_entity_type`
  and `minecraft:menu` was read; the id order of the vanilla part (generated list order vs. the
  26.3 registry order) is assumed equal to `BLOCK_ENTITY_TYPES` and `WindowType` order. The M3
  rows for block, item and entity type use the same assumption (`dynamic.rs:478-514`). A capture
  test should check both lists.
- Performance cost of the tick of an idle mod block entity was not benchmarked.
