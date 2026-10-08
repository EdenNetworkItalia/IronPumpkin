# Design

## Context

See proposal.md for the motivation. This section keeps the reference material implementers need: the Pumpkin baseline that phase 1 builds on, and the NeoForge 26.3.x wire behaviour that phase 1 and phase 3 reproduce. File paths and Java class names are the entry points into the sources.

Base: Pumpkin `master` at `f1c0871f4`, Minecraft Java 26.3 (`CURRENT_MC_VERSION = V_26_3` in `crates/pumpkin-data/src/generated/packet.rs`). NeoForge reference: branch `26.3.x`, commit `89f2d71` (2026-10-06). Ground truth for the wire format: NeoForge 26.3.0.52-beta dedicated server, recorded with `tools/pumpkin-neoforge-client`.

### Pumpkin baseline (upstream `f1c0871f4`, before phase 1)

**Plugin architecture.** One `PluginManager` (`crates/pumpkin-core/src/plugin/mod.rs`, `PLUGIN_API_VERSION = 2`), two loaders:
- Native: `crates/pumpkin-core/src/plugin/loader/native.rs` loads `.so/.dll/.dylib` with `libloading` (symbols `PUMPKIN_API_VERSION`, `METADATA`, `plugin`). Plugins get `Context` (`plugin/api/context.rs`): `register_command`, `register_event`, `register_permission`, `register_service`, `register_plugin_loader`. Nothing registers blocks, items or entities.
- Wasm: component model on wasmtime. Public WIT in `crates/pumpkin-plugin-wit/v0.2/*.wit` (world in `plugin.wit`: `on-load`, `handle-event`, `handle-command`, `handle-ai-goal-*`, `handle-generate-phase`). Hosts `crates/pumpkin-wasm-host-v0_1`, `-v0_2` (one per API version), shared `-common`, umbrella `pumpkin-wasm-host`. SDK `crates/pumpkin-plugin-api` (MIT/Apache), runtime `crates/pumpkin-plugin-runtime`. WIT is mirrored publicly: changes must be additive, regenerated with `cargo run -p pumpkin-codegen -- wit`.
- Existing server-side dynamic registries are the pattern to copy: custom enchantments (`crates/pumpkin-core/src/server/enchantment.rs`), custom recipes, datapack registries (`crates/pumpkin-core/src/data/datapack/dynamic_registry_loader.rs`, merged into `CRegistryData` in `net/java/login/known_packs.rs`).

**Events.** `crates/pumpkin-core/src/plugin/api/events/{block,entity,player,server,world,inventory,...}`; structs with `#[derive(Event)]`, fired via `PluginManager::fire` / `fire_blocking` (`plugin/mod.rs:1200`, `:1244`); mirrored in `event.wit` and `pumpkin-wasm-host-v0_2/src/events/`.

**Registries and generated data.** `tools/pumpkin-codegen` (`block.rs`, `item.rs`, `entity_type.rs`, `registry.rs`, `tag.rs`, ...) turns `assets/` into `crates/pumpkin-data/src/generated/`. The model is static and vanilla-shaped: `Block` (`crates/pumpkin-data/src/blocks.rs`) has `id: BlockId(u16)`, `name: &'static str`, `states: &'static [BlockState]`; lookups `Block::from_registry_key` / `from_id` / `from_state_id` (`generated/block.rs`), `Item::from_registry_key` / `from_id(u16)`, `EntityType::from_name`, all returning `&'static`. `BlockStateId(u16)` is the vanilla network ID written directly into chunk palettes (`crates/pumpkin-world/src/chunk/palette.rs`, `BlockPalette = PalettedContainer<BlockStateId, 16>`). Synced dynamic registries come from `pumpkin_data::registry::Registry::get_synced`. This `&'static` model is the blocker the owner hit: no ID space for content absent from `assets/`.

**Custom payloads and configuration phase.** Packets: `crates/pumpkin-protocol/src/java/{client,server}/config/plugin_message.rs`, `play/custom_payload.rs`. State machine: `crates/pumpkin-core/src/net/java/pending.rs` (`handle_config_packet`; `handle_plugin_message` only understands `minecraft:brand`, anything else is dropped). Sequence after login ack (`net/java/login/login_acknowledged.rs`): brand, server links, resource pack, `CFeatureFlags`, `CKnownPacks`; on `SKnownPacks` (`login/known_packs.rs`): `CRegistryData` per registry, `CUpdateTags`, `CFinishConfig`. No configuration-task queue, no plugin hook in this phase. Play phase: `net/java/mod.rs:1054` turns `SCustomPayload` into `PlayerCustomPayloadEvent` and parses `minecraft:register`/`unregister` into `PlayerRegisterChannelEvent`/`PlayerChannelEvent`; outgoing `Player::send_custom_payload` (`entity/player.rs:85`). Status JSON: `CachedStatus` via `server.get_status()`.

**Maintainer stance** (Pumpkin discussion #99): mod support "only through a custom pumpkin plugin that translates the mod calls"; "adding support for packets is not a problem". Networking hooks could go upstream; the API facade stays in the fork.

### What NeoForge does on the wire (26.3.x)

Sources (under `src/main/java/net/neoforged/neoforge/`): `network/registration/NetworkRegistry.java`, `network/payload/*.java`, `network/configuration/*.java`, `network/ConfigurationInitialization.java`, `registries/RegistryManager.java`, `registries/RegistrySnapshot.java`, `registries/NeoForgeRegistriesSetup.java`; patches `ServerConfigurationPacketListenerImpl.java.patch`, `ClientConfigurationPacketListenerImpl.java.patch`, `ServerStatus.java.patch`. Everything rides on vanilla `custom_payload`; names below are channel IDs.

**Status.** `ServerStatus` JSON gains `"isModded": true` (server-list icon only; no mod list in the ping).

**Server configuration start** (patched `startConfiguration`, before vanilla tasks):
1. `minecraft:unregister` (NUL-separated identifiers): `minecraft:register`, `minecraft:unregister`, optional serverbound play channels.
2. `minecraft:register` with `BUILTIN_PAYLOADS`: `minecraft:register`, `minecraft:unregister`, `neoforge:register`, `neoforge:network`, `neoforge:modded_network_setup_failed`, `c:version`, `c:register`.
3. `neoforge:register` = `ModdedNetworkQueryPayload(Map<ConnectionProtocol, Set<ModdedNetworkQueryComponent>>)`; map key is the enum ordinal (`idMapper`), component `{Identifier id, String version, Optional<PacketFlow>, bool optional}`. Server sends an empty map as a probe.
4. Vanilla `ping` id 0; the server waits for the pong.

**Client** (patched `ClientConfigurationPacketListenerImpl`): on `neoforge:register` it becomes `ConnectionType.NEOFORGE` and replies with its own `neoforge:register` listing all channels its mods registered through `PayloadRegistrar`. If it sees `minecraft:brand` first it falls back to `ConnectionType.OTHER` (vanilla server): `CheckFeatureFlags.handleVanillaServerConnection` disconnects if any mod adds a feature flag, registry sync is skipped, the client keeps its own numeric IDs. With `detect_neoforge_clients` off, IronPumpkin sends the brand first, so NeoForge clients treat it as vanilla and work only while no mod adds registry entries.

**Server negotiation** (`NetworkRegistry.initializeNeoForgeConnection`): it runs when the client's `neoforge:register` arrives; per protocol (configuration, then play), `NetworkComponentNegotiator.negotiate(server, client)`: an optional channel the other side lacks is dropped, a required one fails, matched channels need the same flow when one side sets it and an equal version string. Failure: `neoforge:modded_network_setup_failed` (`Map<Identifier, Component>`, translatable reasons) then disconnect with `multiplayer.disconnect.incompatible`. Success: `neoforge:network` = `ModdedNetworkPayload(NetworkPayloadSetup: Map<protocol, Map<Identifier, NetworkChannel{id, String chosenVersion}>>)`, then `minecraft:register` with builtins plus negotiated configuration channels. After the pong, `runConfiguration()` queues:
- Early (`configureEarlyTasks`, must precede vanilla `SynchronizeRegistriesTask` so tags resolve): `SyncRegistries` sends `neoforge:frozen_registry_sync_start` (`List<Identifier>`), one `neoforge:frozen_registry` per registry (`Identifier, RegistrySnapshot{Map<varint id, Identifier> ids, Map<Identifier, Identifier> aliases}`), then `neoforge:frozen_registry_sync_completed`; the client applies the snapshot (`RegistryManager.applySnapshot`, remaps IDs, disconnects on missing entries) and echoes the completed payload. Synced: `NeoForgeRegistriesSetup.VANILLA_SYNC_REGISTRIES` (`sound_event`, `mob_effect`, `block`, `entity_type`, `item`, `fluid`, `particle_type`, `block_entity_type`, `menu`, `command_argument_type`, `stat_type`, `villager_type`, `villager_profession`, `data_component_type`, ...) plus `neoforge:` registries built with `.sync(true)` (`entity_data_serializers`, `fluid_types`, `holder_set_types`, `ingredient_types`, `fluid_ingredient_types`, attachment types).
- From `RegisterConfigurationTasksEvent`: `c:version` (`List<varint>` = `[1]`) and `c:register` (`{varint 1, string "play", Set<Identifier>}`) when the client has `c:` channels; `SyncConfig` sends `neoforge:config_file {string fileName, byte[] contents}` per `ModConfig.Type.SYNCED`; `RegistryDataMapNegotiation` (`neoforge:known_registry_data_maps` / reply); `CheckExtensibleEnums` (`neoforge:extensible_enum_data` / ack); `CheckFeatureFlags` (`neoforge:feature_flags` `Set<Identifier>` / ack; mismatch disconnects).
- Then vanilla known packs, registry data, tags, finish.

**Play phase.** Negotiated mod channels plus NeoForge internals: `neoforge:advanced_add_entity`, `neoforge:advanced_open_screen`, `neoforge:registry_data_map_sync`, `neoforge:sync_attachments`, `neoforge:split` (payloads over 1 MiB), `neoforge:auxiliary_light_data`. `NetworkRegistry.checkPacket` throws on any non-`minecraft:` payload the peer did not negotiate.

**Versions and the 1.21.1 gap.** NeoForge 21.1.x is for 1.21.1; 26.3.0.x-beta is for 26.3. The porting chain 1.21.1 -> 1.21.2/3 -> 1.21.4 -> 1.21.5 -> 1.21.6 -> ... -> 1.21.11 -> 26.1 -> 26.3 (index at leclowndu93150.github.io/Porting-Primers, upstream primers at docs.neoforged.net/primer) includes: item/equipment/consumables rework and recipes-as-registry (1.21.2), data component getters and tag parsing (1.21.5), NBT access replaced by codecs (1.21.6), transfer API rewrite (21.9), `ResourceLocation` -> `Identifier` and package renames (1.21.11), Java 25 and unobfuscated jars (26.1), rendering rewrites in nearly every step. The wire shape above is stable since 1.20.5, but mods written for NeoForge 21.1 (1.21.1) need a port to 26.3 because Pumpkin speaks one protocol version; the alternative, pinning IronPumpkin to an old Pumpkin commit, loses two years of fixes.

### What phase 1 changed in this baseline

- `handle_plugin_message` in `pending.rs` routes `neoforge:register`, `minecraft:register` and `minecraft:unregister` when `detect_neoforge_clients` is on, logs unknown channels at debug, and fires the configuration payload event for every payload.
- `login_acknowledged.rs` is split into the probe (`crates/pumpkin-core/src/net/java/neoforge.rs`) and `run_configuration`, which is the unchanged vanilla sequence.
- Codecs for every payload listed above live in `crates/pumpkin-protocol/src/java/neoforge/`, except `neoforge:known_registry_data_maps` and `neoforge:extensible_enum_data`.
- The status JSON carries `isModded` behind `advertise_modded` and uses the vanilla key `enforcesSecureChat`.
- `NetworkRegistry` on the server (`server.network_registry`, in `net/java/neoforge.rs`) holds the channels the server speaks, seeded with the NeoForge registrations. The client's `neoforge:register` reply runs `initializeNeoForgeConnection` once, and a later `neoforge:register` kicks the client. On the probe pong a client that is still Other runs `initializeOtherConnection`, then `run_configuration` starts, and the negotiated `NetworkPayloadSetup` is kept on `PendingConnection.payload_setup`.

## Goals / Non-Goals

**Goals:**

- Detect NeoForge clients the way a NeoForge server does, and record what they declare.
- Give plugins and the later handshake work a hook into the configuration phase.
- Keep the default join and status byte-identical to Pumpkin, so upstream merges and vanilla deployments are not affected.

**Non-Goals:**

- A configuration-task queue, registry sync, config sync, feature flag and extensible enum checks.
- Exposing the configuration payload event in `pumpkin-plugin-api`: the SDK still binds WIT v0.1.

## Decisions

### `detect_neoforge_clients` is separate from `advertise_modded`

The probe changes the join sequence of every client: five packets and a ping round trip before the brand. `advertise_modded` only changes the status JSON. A server can want the modded icon without the probe. Both options default to false.

Alternative rejected: one `neoforge` switch. It would couple a cosmetic flag to a change of the join sequence.

### The configuration payload event fires after the built-in handling

`ConfigCustomPayloadEvent` fires after the brand and the NeoForge channel declarations are processed. Handlers then see the channels the payload declared, and they cannot change the vanilla handling. A malformed declaration errors out before the event. The event is not cancellable.

It uses `fire().await`, not `fire_blocking`, because `handle_plugin_message` runs in the async login loop, like ConnectionPacketReceivedEvent and ServerListPingEvent. Handlers answer by queueing payloads on the event; `pending.rs` sends the queue after the handlers ran. A Wasm guest can only append responses; identity and channel fields are read-only.

Alternative rejected: firing before the built-in handling. A handler would see a connection whose declared channels are not recorded yet.

### Ad hoc channels are capped at 1024

`minecraft:register` can declare any number of channels, and NeoForge keeps ad hoc channels for the whole connection (`ServerCommonPacketListenerImpl` patch, `onMinecraftRegister`). The server caps them at `MAX_AD_HOC_CHANNELS = 1024`, the figure Velocity uses for client plugin channels. A register that goes above the cap fails the packet and kicks.

Alternative rejected: collecting ad hoc channels only while the probe is pending. Other platforms register channels after the probe.

### Other settled details

- Codecs reject any count or byte-array length larger than the remaining input, cap pre-allocation at 65536, and are decoded through `decode_exact`, which rejects trailing bytes like the vanilla packet decoder.
- A malformed NeoForge declaration kicks, like a decoder error.
- Only the first pong 0 starts the configuration (`PendingConnection.neoforge_probe_pending`). NeoForge reruns `runConfiguration` on every pong 0; Pumpkin's login loop has no task queue to rerun.
- Maps and sets use BTreeMap and BTreeSet. Java writes HashMap order, so tests compare decoded values, not wire order.

## Risks / Trade-offs

- [A real NeoForge client may still reject the join, because the headless client only simulates it] -> the owner play tests of the detection issue (#9) and the negotiation issue (#31) show the real behaviour.
- [NeoForge marks the payloads `@ApiStatus.Internal`; they can change per release] -> track `26.3.x` commits under `network/payload` and rerun the headless client against a new NeoForge server.
- [The particleStatus fix adds a field to published WIT records] -> called out in the upstream PR draft (#15).
- [Upstream bugs fixed here diverge from Pumpkin until they merge upstream] -> each fix is its own commit with an upstream PR draft in its issue.
