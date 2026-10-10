# Design

## Context

See proposal.md for the motivation. This section keeps the reference material implementers need: the phase 1 handshake that M3 extends, the NeoForge 26.3.x configuration flow, the wire capture of a real NeoForge server, the client rules for registry snapshots and vanilla clients, and the files each task touches. File paths and Java class names are the entry points into the sources.

Base: IronPumpkin `master` at `40dcf628f`, Minecraft Java 26.3. NeoForge reference: branch `26.3.x`, commit `89d5fc58cc3266c205fda70848c285fe5b63ea56` (2026-10-09). Ground truth for the wire: a NeoForge 26.3.0.64-beta dedicated server (port 25599, `online-mode=false`), recorded with `tools/pumpkin-neoforge-client` on 2026-10-10. Java paths below are under `src/main/java/net/neoforged/neoforge/` unless they start with `patches/` or `src/client/`; "NR" is `network/registration/NetworkRegistry.java`.

The phase 1 change `neoforge-aware-networking` is not archived: it waits for the owner play test. Its capabilities `neoforge-payloads`, `connection-lifecycle` and `neoforge-test-client` are not in `openspec/specs/` yet. This change keeps the configuration disconnect requirement in `neoforge-configuration-payloads`, and tracks the headless client changes in the configuration disconnect task (T4) and the end-to-end task (T11) without a delta spec.

### Phase 1 baseline

The handshake is linear and reactive. Each handler sends the next packets inline with `send_packet_now`; the only wait state is the bool `neoforge_probe_pending`.

- `handle_login_acknowledged` (`crates/pumpkin-core/src/net/java/login/login_acknowledged.rs:6`) sets the state to Config. With `detect_neoforge_clients` on it calls `send_neoforge_probe` (`net/java/neoforge.rs:500`): `minecraft:unregister`, `minecraft:register` (`BUILTIN_CHANNELS`), an empty `neoforge:register`, `CConfigPing(0)`. Off: `run_configuration` directly.
- The client's `neoforge:register` goes `handle_plugin_message` (`pending.rs:615`) -> `handle_neoforge_payload` (`neoforge.rs:545`) -> `ClientChannels::handle_payload` (`neoforge.rs:193`) -> `initialize_neoforge_connection` (`neoforge.rs:598`). It negotiates configuration then play with `negotiate` (`neoforge.rs:371`). Failure: `neoforge:modded_network_setup_failed`, then a kick with `multiplayer.disconnect.incompatible` and the argument `NeoForge {CURRENT_MC_VERSION}`. Success: `neoforge:network`, `minecraft:register`, and `PendingConnection.payload_setup`.
- The pong (`pending.rs:564`) goes to `handle_neoforge_probe_pong` (`neoforge.rs:575`), id 0 only while pending. A client that is still Other runs `initialize_other_connection` (`neoforge.rs:662`). It kicks with a plain text that uses `CURRENT_MC_VERSION` as the NeoForge version when a server channel is required, and otherwise sends `minecraft:register`. Then `run_configuration` runs.
- `run_configuration` (`login_acknowledged.rs:22`) sends the brand (`server.get_branding()`), `CConfigServerLinks` when enabled, then `CConfigAddResourcePack` or `send_known_packs` (`login_acknowledged.rs:109`). The resource pack path continues in `handle_resource_pack_response` (`pending.rs:698`). `send_known_packs` sends `CFeatureFlags`, then `CKnownPacks`.
- `SKnownPacks` (`pending.rs:545`) -> `handle_known_packs` (`login/known_packs.rs:5`): one `CRegistryData` per `Registry::get_synced` entry, `CUpdateTags` for the non-empty `RegistryKey::NETWORK_KEYS`, `CFinishConfig`.
- `SAcknowledgeFinishConfig` (`pending.rs:532`) sets the state to Play and returns `PacketHandlerResult::ReadyToPlay(profile, config)` (`net/mod.rs:119`). `JavaClient::from_pending` (`net/java/mod.rs:141`) copies none of `client_channels`, `payload_setup` or the connection type.
- `handle_plugin_message` handles `minecraft:brand` (`read_brand`, `pending.rs:62`) and, with detection on, `neoforge:register`, `minecraft:register` and `minecraft:unregister`. `c:version`, `c:register`, echoes and acknowledgements reach the debug log "unknown configuration channel". `fire_config_custom_payload` (`pending.rs:644`) then fires the plugin event for every payload.
- `kick` (`pending.rs:295`) sends `CConfigDisconnect::new(&reason.get_text())`, which flattens a translatable to en_us plain text. `CConfigDisconnect` (`crates/pumpkin-protocol/src/java/client/config/config_disconnect.rs:26`) writes `write_string`; the play disconnect uses `write_component`.
- `HANDSHAKE_IDLE_TIMEOUT` is 30 s (`pending.rs:74`), reset per packet.
- `NetworkRegistry` (`neoforge.rs:233`, `Server.network_registry` at `server/mod.rs:113`) is seeded with the 18 optional `NEOFORGE_PAYLOADS` (`neoforge.rs:77`). `neoforge:split` is one of them, for play and configuration, with no flow.
- Codecs in `crates/pumpkin-protocol/src/java/neoforge/`: `common.rs` (`CommonVersionPayload`, `CommonRegisterPayload`), `config_file.rs`, `feature_flags.rs` (`FeatureFlagDataPayload`), `network.rs`, `register.rs`, `registry.rs` (`FrozenRegistrySyncStartPayload`, `RegistrySnapshot` with `ids: BTreeMap<i32, Identifier>` and `aliases`, `FrozenRegistryPayload`, `FrozenRegistrySyncCompletedPayload`), `split.rs` (`SplitPacketPayload`, raw slice only). `decode_exact` (`mod.rs:131`) rejects trailing bytes; `read_count` (`mod.rs:150`) rejects negative counts and counts above the remaining bytes. Missing: `known_registry_data_maps` and its reply, `extensible_enum_data` and its ack, `feature_flags_ack`.

### NeoForge configuration flow

`ServerConfigurationPacketListenerImpl.startConfiguration` is patched (`patches/net/minecraft/server/network/ServerConfigurationPacketListenerImpl.java.patch:6-16`) to send the probe and `ClientboundPingPacket(0)`. The vanilla body becomes `runConfiguration()` and runs on pong 0 (patch:48-58). `runConfiguration()` sends the brand, the server links and `UpdateEnabledFeatures`, then fills the task queue:

1. Early task `neoforge:sync_registries` (`SyncRegistries`), from `ConfigurationInitialization.configureEarlyTasks` (`network/ConfigurationInitialization.java:37-44`, patch:23-24). Condition: the client has the channels `frozen_registry_sync_start`, `frozen_registry` and `frozen_registry_sync_completed`, and the connection is not in-memory.
2. Vanilla `SynchronizeRegistriesTask`: known packs, registry data, tags (patch:25-26).
3. Vanilla `addOptionalTasks()`: code of conduct, then `ServerResourcePackConfigurationTask` (patch:29-31). The vanilla order is not verified against the jar.
4. `RegisterConfigurationTasksEvent` tasks, appended at the end of `addOptionalTasks` (patch:33-34). NeoForge's own listener (`ConfigurationInitialization.java:46-62`) adds, in order: `CommonVersionTask` and `CommonRegisterTask` when `hasChannel(c:version) && hasChannel(c:register)`; `SyncConfig` when `hasChannel(neoforge:config_file)`; `RegistryDataMapNegotiation`, `CheckExtensibleEnums`, `CheckFeatureFlags` always. `hasChannel` also accepts the ad hoc channels of the client's `minecraft:register`.
5. `PrepareSpawnTask`, `JoinWorldTask` (vanilla, order not verified). `handleConfigurationFinished` calls `finishCurrentTask(JoinWorldTask.TYPE)` (patch:62-64).

Replies that finish a task:

| Task | Finished by | Source |
|:--|:--|:--|
| sync_registries | client echo of `frozen_registry_sync_completed` | `handlers/ServerPayloadHandler.java:17-19` |
| common_version | client `c:version` | NR:631-645 |
| common_register | client `c:register` | NR:655-667 |
| sync_config | nothing, finishes at once | `configuration/SyncConfig.java` |
| data maps | `known_registry_data_maps_reply` | `registries/RegistryManager.java:265-268` |
| enums | `extensible_enum_ack` | `CheckExtensibleEnums.java:158-160` |
| feature flags | `feature_flags_ack` | `CheckFeatureFlags.java:80-82` |

On an Other connection the three always-registered tasks finish at once or disconnect: `RegistryDataMapNegotiation.run` disconnects only when a mandatory data map exists; `CheckExtensibleEnums.start:57-68` only when an extended clientbound enum exists; `CheckFeatureFlags.start:41-49` only when modded feature flags exist.

Payload formats (Identifier = VarInt-prefixed UTF-8; collections = VarInt count then items; optional = bool then value):

- `neoforge:frozen_registry_sync_start`: `List<Identifier>` (`FrozenRegistrySyncStartPayload.java:26-31`).
- `neoforge:frozen_registry`: `Identifier registryName`; VarInt N, then N x (VarInt id, Identifier key) in ascending id; VarInt M, then M x (Identifier alias, Identifier target) (`FrozenRegistryPayload.java:25-30`, `RegistrySnapshot.java:28-33`).
- `neoforge:frozen_registry_sync_completed`: empty, both directions.
- `c:version`: `List<VarInt>`; the server sends `[1]` (`CommonVersionPayload.java:25-31`). No match: disconnect with a literal "Unsupported common network version" text.
- `c:register`: VarInt version 1, String protocol `play`, `Set<Identifier>` (`CommonRegisterPayload.java:27-34`). The server sends its optional play payloads that flow serverbound; the client replaces its common channel set and replies with its optional clientbound play channels (NR:655-676).
- `neoforge:config_file`: String file name, byte array contents (`ConfigFilePayload.java:27-34`). `ConfigSync.syncAllConfigs` (`network/ConfigSync.java:45-66`) sends one per `ModConfig.Type.SYNCED` config. NeoForge registers one itself (`common/NeoForgeMod.java:586`). The client applies a file only if it knows the name (`ConfigSync.java:131-133`). On a NeoForge connection a SYNCED config that the server never sends stays unloaded on the client (an inference from `ClientNetworkRegistry.java:270`).
- `neoforge:known_registry_data_maps`: map registry key -> `List<{Identifier id, bool mandatory}>` (`KnownRegistryDataMapsPayload.java:22-44`). The client compares mandatory sets (`ClientRegistryManager.java:56-104`) and replies `known_registry_data_maps_reply`: map registry -> collection of its data map ids.
- `neoforge:extensible_enum_data`: VarInt n, then n x (String class name, String network check, Optional `{VarInt vanillaCount, VarInt totalCount, List<String> addedNames}`) (`CheckExtensibleEnums.java:232-262`). The client checks only extended enums and acks with an empty `extensible_enum_ack`.
- `neoforge:feature_flags`: `Set<Identifier>` of modded flags (`FeatureFlagDataPayload.java:19-23`). Equal to the client's modded flags: empty `feature_flags_ack`; else disconnect `neoforge.network.feature_flags.entry_mismatch`.
- `neoforge:split`: byte array (`SplitPacketPayload`). Byte 0 is the state: 1 first, 0 middle, 2 last; the rest is a slice of the fully encoded packet (packet id and body). `GenericPacketSplitter.encode` (`network/filters/GenericPacketSplitter.java:77-130`) splits any packet, only when the remote has `neoforge:split` (169-185). The limit is `CompressionDecoder.MAXIMUM_UNCOMPRESSED_LENGTH` (8 MiB) with a compressor and `MAXIMUM_COMPRESSED_LENGTH` (2 MiB) without. Part size is the limit minus the prefix overhead (`determineMaxPayloadSize`, 187-211). The receiver joins the parts and decodes them with the inbound protocol (134-160). pumpkin-protocol has the same limits as `MAX_PACKET_DATA_SIZE` (8_388_608) and `MAX_PACKET_SIZE` (2_097_152) in `crates/pumpkin-protocol/src/lib.rs:35-36`.

### Run (b): NeoForge client against the NeoForge server

Clientbound order; client replies in brackets.

1. `minecraft:unregister` [`minecraft:register`]
2. `minecraft:register`; the client replies with its own `minecraft:register`, including `c:register` and `c:version`
3. `neoforge:register`, body `00`; the client replies with its query
4. `ping` 0
5. `neoforge:network`, 1173 B: 9 play and 11 configuration channels, all version `1` [`minecraft:register`]
6. `minecraft:register`, 453 B, 18 entries
7. `minecraft:brand` `neoforge`, then `update_enabled_features` [`minecraft:vanilla`]
8. `neoforge:frozen_registry_sync_start`, 880 B, 35 registries
9. 35 x `neoforge:frozen_registry`
10. `neoforge:frozen_registry_sync_completed`, empty; the server waits [echo]
11. `select_known_packs`: `minecraft:core:26.3` and `neoforge:mod/neoforge:26.3.0.64-beta`
12. 32 x `registry_data`, then `update_tags` (75118 B)
13. `c:version` `0101` [`0101`]
14. `c:register` `01 04"play" 01 0e"neoforge:split"` [the client's 9 play channels]
15. `neoforge:config_file`, 1191 B: `neoforge-synced.toml`, 1168 B of TOML; no reply
16. `neoforge:known_registry_data_maps`, 446 B: 7 registries, all `mandatory=false`: block: transformables, oxidizables, waxables; item: none; entity_type: acceptable_villager_distances, monster_room_mobs, parrot_imitations; worldgen/biome: villager_types; game_event: vibration_frequencies; villager_profession: raid_hero_gifts; block_transformer: block_transform_appenders [`00`]
17. `neoforge:extensible_enum_data`, 561 B: 9 entries, all without extension data; 6 `CLIENTBOUND`, 3 `BIDIRECTIONAL` (ItemUseAnimation, FireworkExplosion$Shape, Rarity) [empty ack]
18. `neoforge:feature_flags` `00` [empty ack]
19. `finish_configuration`

The frozen registry payloads have ids 0..n-1 without gaps and 0 aliases. None is split; the largest is 71788 B. The 35 registries (bare names are `minecraft:`):

| registry | ids | first | last | bytes |
|:--|--:|:--|:--|--:|
| block | 1286 | air | firefly_bush | 37020 |
| item | 1658 | air | ominous_bottle | 46928 |
| entity_type | 161 | acacia_boat | fishing_bobber | 3563 |
| sound_event | 1991 | entity.allay.ambient_with_item | block.red_shrub.place | 71788 |
| data_component_type | 122 | custom_data | cushion/color | 3179 |
| particle_type | 128 | angry_villager | sulfur_cube_goo | 3062 |
| custom_stat | 78 | leave_game | interact_with_smithing_table | 2142 |
| command_argument_type | 64 | brigadier:bool | neoforge:modid | 1434 |
| game_event | 61 | block_activate | resonate_15 | 1441 |
| block_entity_type | 49 | furnace | potent_sulfur | 1127 |
| potion | 46 | water | infested | 1125 |
| attribute | 43 | air_drag_modifier | neoforge:gliding_flight | 1204 |
| mob_effect | 40 | speed | breath_of_the_nautilus | 905 |
| map_decoration_type | 40 | player | ocean_ruin_warm | 984 |
| menu | 25 | generic_9x1 | stonecutter | 556 |
| recipe_serializer | 22 | crafting_shaped | smithing_trim | 728 |
| point_of_interest_type | 21 | armorer | lightning_rod | 471 |
| debug_subscription | 16 | dedicated_server_tick_time | game_events | 435 |
| villager_profession | 15 | none | weaponsmith | 332 |
| slot_display | 14 | empty | neoforge:fluid_tag | 319 |
| recipe_book_category | 13 | crafting_building_blocks | campfire | 375 |
| stat_type | 9 | mined | custom | 189 |
| recipe_type | 8 | crafting | brewing | 194 |
| villager_type | 7 | desert | taiga | 149 |
| neoforge:ingredient_serializer | 6 | neoforge:compound | neoforge:custom_display | 162 |
| neoforge:fluid_ingredient_type | 6 | neoforge:simple | neoforge:custom_display | 159 |
| consume_effect_type | 5 | apply_effects | play_sound | 163 |
| fluid | 5 | empty | lava | 117 |
| recipe_display | 5 | crafting_shapeless | smithing | 146 |
| neoforge:holder_set_type | 4 | neoforge:any | neoforge:not | 82 |
| number_format_type | 3 | blank | fixed | 83 |
| neoforge:fluid_type | 3 | empty | lava | 72 |
| position_source_type | 2 | block | entity | 68 |
| neoforge:entity_data_serializers | 0 | - | - | 35 |
| neoforge:synced_attachment_types | 0 | - | - | 35 |

Block 1286, item 1658 and entity_type 161 equal `BlockId::COUNT`, `Item::COUNT` and `EntityType::COUNT` of pumpkin-data. Run (d), a NeoForge client against IronPumpkin `master`, has the same probe and negotiation, and lacks everything from step 8 on except the vanilla packets. Other differences in run (d): brand `Pumpkin`; IronPumpkin sends `server_links`; it offers only `minecraft:core` but sends full NBT in every `registry_data`; `chat_type` has an extra `minecraft:raw`; `damage_type` lacks `neoforge:poison`; `update_tags` is 75587 B.

### Client rules for a registry snapshot

`ClientPayloadHandler` (`src/client/java/net/neoforged/neoforge/client/network/ClientPayloadHandler.java:85-118`) records the registries of `frozen_registry_sync_start`. On `frozen_registry_sync_completed` it disconnects with `neoforge.network.registries.sync.missing` when a listed registry did not arrive. Otherwise it calls `RegistryManager.applySnapshot(map, false)` (`registries/RegistryManager.java:130-202`):

- A registry unknown to the client is ignored when its snapshot is empty. Otherwise the client disconnects with `neoforge.network.registries.sync.failed` and reverts to its frozen ids.
- For a known registry the client clears the int ids and keeps the keys (`patches/.../MappedRegistry.java.patch:155-171`), then maps every server entry. Vanilla ids are remapped too.
- A key the client does not have goes into a missing list. The client disconnects with `neoforge.network.registries.sync.server-with-unknown-keys` and reverts.
- An entry the client has and the server did not send stays registered by key with no int id. This is not an error.
- A synced registry that the server does not send keeps the client's own ids.
- Ids can be sparse. On success the client echoes `frozen_registry_sync_completed`.
- The client rebuilds block state ids by iterating blocks in id order and each block's states in order (`NeoForgeRegistryCallbacks.BlockCallbacks.onBake`). The phase 2 state layout rule produces the same ids when the ported mod declares the same properties and values.

### Rules for vanilla clients

- Detection (`NR.initializeOtherConnection`, NR:383-416): a vanilla client discards the probe payloads and answers the ping. The server negotiates every registered payload against an empty list. Optional payloads drop out. A required payload fails, and the server disconnects with `Component.translatableWithFallback("neoforge.network.negotiation.failure.vanilla.client.not_supported", "You are trying to connect to a server that is running NeoForge, but you are not. Please install NeoForge Version: %s to connect to this server.", version)` (NR:399-400). The vanilla client lacks the key and shows the fallback.
- A mod payload is required unless the mod calls `.optional()` (`PayloadRegistrar.java:28`). Every NeoForge payload is optional.
- A vanilla client gets no registry sync, no `c:*`, no config file, no data maps, no enums and no feature flags (run (a)). Its `registry_data` and `update_tags` bytes equal run (b).
- No NeoForge check looks at registry contents for a vanilla client. Modded ids reach it unchecked; IronPumpkin sends display ids instead.
- The handshake never exchanges mod ids or versions (section F of the source notes). There is no `ModMismatchData` in 26.3.

### Files per task

| Task | Files |
|:--|:--|
| T1 task queue | the vanilla 26.3 order checked on the decompiled server jar first, `crates/pumpkin-core/src/net/java/pending.rs` (`PendingConnection`, `handle_plugin_message`, `SKnownPacks`, resource pack response, finish), `net/java/login/login_acknowledged.rs` (`run_configuration`, `send_known_packs`), `net/java/login/known_packs.rs`, `net/java/neoforge.rs` (reply routing), a new module for the queue under `net/java/` |
| T2 codecs | `crates/pumpkin-protocol/src/java/neoforge/` (new data map and enum files, `feature_flags.rs`, `mod.rs`), `tools/pumpkin-neoforge-client/src/record.rs` (`decode_payload`), `session.rs` (typed replies), `tools/pumpkin-neoforge-client/captures/neoforge-26.3.0.64-beta/` (runs (a), (b) and (c) as gzipped JSONL and a README), hex fixtures next to the codec tests |
| T3 tables, snapshots, mod list | `crates/pumpkin-data/src/dynamic.rs` (`tables()`, snapshot per registry), `crates/ironpumpkin-mods/src/lib.rs` (`NativeMod::client_required`, `init_mods`), `crates/pumpkin-core/src/plugin/startup.rs` (`NativeModInfo`, `set_native_mods`, `native_mods`) |
| T4 disconnect component | `crates/pumpkin-protocol/src/java/client/config/config_disconnect.rs`, `crates/pumpkin-core/src/net/java/pending.rs` (`kick`), `crates/pumpkin-util/src/text/mod.rs` (`with` of plain strings), `tools/pumpkin-neoforge-client/src/{main.rs,lib.rs,record.rs}` (`--expect-disconnect`) |
| T5 connection state in play | `crates/pumpkin-core/src/net/mod.rs` (`ReadyToPlay`), `net/java/mod.rs` (`JavaClient::from_pending`), `entity/player.rs`, `crates/pumpkin-data/src/dynamic.rs` (mode enum) |
| T6 registry sync task | the queue module, `net/java/neoforge.rs` (`SYNCED_REGISTRIES`), `crates/pumpkin-protocol/src/java/neoforge/registry.rs`, the test mod (one block, one item, one entity type) for the boot test |
| T7 modded tasks | the queue module, `net/java/neoforge.rs`, `crates/ironpumpkin-mods/src/lib.rs` (`ModInit::synced_config`), `plugin/startup.rs`, the `neoforge-synced.toml` fixture extracted from run (b) |
| T8 kick | `net/java/neoforge.rs` (`initialize_other_connection`, `EMULATED_NEOFORGE_VERSION`, the negotiation failure argument), `crates/pumpkin-util/src/text/mod.rs` (translate fallback), the startup check that forces detection on |
| T9 per-client encoding | the call sites in the encoding table below |
| T10 split | `crates/pumpkin-protocol/src/java/packet_encoder.rs`, `packet_decoder.rs`, `java/neoforge/split.rs`, `pending.rs` and `net/java/mod.rs` (reassembly) |
| T11 end-to-end | `tools/pumpkin-neoforge-client/expected/`, `src/lib.rs` (registry expectations), the boot test `neoforge_client_syncs_custom_content` in the root workspace with a test mod, `examples/modpack/mods/hello-mod/src/lib.rs` (an entity type for the owner play test) |
| T12 play test | a Java test mod outside the Rust workspace |

## Goals / Non-Goals

**Goals:**

- Reproduce the NeoForge 26.3.x configuration task order and reply waits, so a real NeoForge 26.3 client reaches play.
- Give NeoForge clients the real ids of custom blocks, items and entity types, and keep display ids for every other client.
- Kick vanilla clients the way a NeoForge server does when a mod needs the client side.
- Follow the vanilla 26.3 configuration order. With detection off and no resource pack, the byte stream stays the same, so upstream merges and vanilla deployments are not affected.

**Non-Goals:**

- Syncing the other 32 registries of the capture. M4 registration adds rows to `SYNCED_REGISTRIES` when it registers sound events, mob effects, attributes or data components.
- `neoforge:registry_data_map_sync`, `neoforge:advanced_add_entity`, data maps and spawn data: no M3 content uses them.
- A real `ModConfigSpec` behind the synced configs (M4 `mod-config`) and mod network channels (M4 `network`).
- Changing the brand, the known packs, the `registry_data` content or `server_links` to match NeoForge byte for byte.
- Bedrock egress for custom content.

## Decisions

### D1. A configuration task queue on the pending connection

`PendingConnection` gets a `VecDeque` of configuration tasks. The server sends the brand, the server links and `update_enabled_features`, then starts the queue, as vanilla `startConfiguration` and run (b) do. The queue holds, in order: `SyncRegistries` (NeoForge connections that declared the three frozen registry channels), the known packs task (`CKnownPacks` only; registry data and tags on `SKnownPacks`), the resource pack task (when configured), common version and common register (when the client has the ad hoc channels `c:version` and `c:register`), sync config (when the client has `neoforge:config_file`), data map negotiation, extensible enum check, feature flag check, then finish.

The order of `update_enabled_features`, known packs and the resource pack comes from the NeoForge patch (patch:25-31) and run (b). The queue task checks it first on the decompiled vanilla 26.3 server jar and records the result in its issue. If vanilla confirms, Pumpkin's current order (resource pack first, feature flags after it) is an upstream parity candidate: a note in the issue, and an upstream PR only if the difference has a user-visible effect.

A task either finishes when it has sent its payloads or waits for one named client reply. The queue advances only on that reply. A reply for a task that is not current is a protocol error and kicks. The probe and the pong stay as phase 1 built them. The existing known packs and resource pack steps become tasks. With `detect_neoforge_clients = false` and no resource pack the byte stream stays the same; the headless client vanilla expected file proves it. With a resource pack the order changes to the vanilla order, also with detection off.

Alternative rejected: more wait flags next to `neoforge_probe_pending`. Seven tasks with conditional replies make seven flags and an implicit order spread over the handlers. The queue keeps the order in one place, the way `ServerConfigurationPacketListenerImpl.configurationTasks` does, and M4 configuration tasks from mods (`RegisterConfigurationTasksEvent`) append to it.

Alternative rejected: accept replies in any order. NeoForge finishes a task only through its own reply type, and a reply for another task points at a client bug or a hostile client.

Two rules settled by the wave 1 review: a custom payload reply reaches its task with the payload data (`handle_task_reply(reply, data)`, called between the finish and the next start, as the known packs reply calls `handle_known_packs`), so the common version, common register and data map replies can be read without touching the dispatch; and a modded task whose wait depends on the connection type is not queued for an Other connection at all, the Other-side checks (mandatory data maps, extended clientbound enums, modded feature flags, all empty in M3) live in `initialize_other_connection`.

### D2. Registry sync for block, item and entity type only

IronPumpkin sends `frozen_registry_sync_start` with exactly the registries it snapshots, one `frozen_registry` per registry, then `frozen_registry_sync_completed`, and waits for the echo. In M3 the synced registries are `minecraft:block`, `minecraft:item` and `minecraft:entity_type`. Each snapshot maps every id to a namespaced name: generated entries `0..COUNT` with `minecraft:` names, then custom entries in allocation order. Placeholders (content of a mod the server no longer has, restored from the manifest) are left out, so their ids are gaps; the NeoForge client accepts sparse ids. The alias maps are empty. Block state ids are not synced: the NeoForge client rebuilds them after each bake (`NeoForgeRegistryCallbacks.BlockCallbacks.onBake`), appending the states of every block in registry id order. Placeholder blocks are gaps, so their states are absent on the client and the states of later custom blocks shift down. The `Real` state id of a live custom block is therefore the client-side id: a counter starts at the generated state count, each live custom block in id order takes the next `states` ids, and a placeholder takes none (its states map to its display state).

The list of synced registries is a table (`SYNCED_REGISTRIES`), so M4 registration adds rows without touching the task. The snapshots come from a new public accessor of the frozen content tables in `pumpkin_data::dynamic` (`tables()`). Nothing is copied onto `Server`.

Reason: the NeoForge client keeps its own ids for a synced registry that the server does not send (`RegistryManager.applySnapshot`). These three registries are the only ones where IronPumpkin ids can differ from the client's ids.

Placeholders are left out because the client disconnects on a key it does not have (`server-with-unknown-keys`), and a placeholder is by definition content no client has: with placeholders in the snapshot every NeoForge client would be locked out of a world where a mod was removed, with no end date. The per-client encoding (D4) maps a placeholder id to its display id also in `Real` mode. The three encoded `frozen_registry` payloads are the same for every connection, so the sync task encodes them once after the freeze (`OnceLock`) instead of once per connection.

Alternative rejected: sync all 35 registries of the capture. IronPumpkin has no numeric tables for recipe types, recipe serializers, position source types or any `neoforge:` registry, and the declaration order of `Sound`, `GameEvent` and `WindowType` is not verified against the vanilla registry order. A wrong row remaps vanilla ids on the client and breaks content that works today.

Alternative rejected: keep a copy of the tables on `Server`. The frozen tables are `&'static` and immutable; a second owner adds a field and a lifetime question for no gain.

### D3. Kick clients that cannot have the required mods

`NativeMod` gets `fn client_required(&self) -> bool { true }`: a mod is client-required unless it opts out. `pumpkin-core` receives the mod list at startup through `startup::set_native_mods(Vec<NativeModInfo { id, display_name, version, client_required }>)`, which replaces the ids-only setter, and exposes it through `native_mods()`.

- Connection type Other and at least one client-required mod loaded: kick in `initialize_other_connection` with the translatable `neoforge.network.negotiation.failure.vanilla.client.not_supported`, the NeoForge fallback text and the argument `EMULATED_NEOFORGE_VERSION`. The constant lives in `net/java/neoforge.rs` with the value `26.3.0.64-beta`, the version of the captures. The kick is logged at info with the ids of the mods that caused it. The existing required-channel kick in the same function and the negotiation failure argument use the same constant instead of `CURRENT_MC_VERSION`.
- Connection type NeoForge: the server cannot know the client's mod list, because NeoForge never exchanges it. A client that lacks a required mod disconnects itself on the registry sync with `server-with-unknown-keys`, because the custom names of that mod are in the snapshots. A mod that also registers a required network channel (M4 `network`) is rejected by the phase 1 negotiation.
- A vanilla client with only client-optional mods joins as today, with display ids and no sync.
- A loaded client-required mod forces detection on. At startup, when a native mod is client-required and `detect_neoforge_clients` is false, the server logs a warning that names those mods and runs as if the option were true. Without the probe every client is Other, and the kick could not tell a vanilla client from a NeoForge client.
- The kick needs `translatableWithFallback`. `TextContent::Translate` (`crates/pumpkin-util/src/text/mod.rs:2037`) gets an optional `fallback` field, written to JSON and NBT only when set, in the vanilla shape. It touches an upstream crate, so it is an upstream parity candidate.

Alternative rejected: refuse to start when detection is off and a mod is client-required. It turns a config omission into a failed boot, while forcing the option gives the behaviour the mods declare.

Alternative rejected: `client_required` defaults to false. The owner decided that clients without a required mod are kicked, as on a NeoForge server; server-only mods are the exception and must say so.

Alternative rejected: `ModInit::set_client_required()`. The flag is metadata of the mod, like its id and version, which `NativeMod` already exposes without running `init`.

Alternative rejected: a per-mod flag in `modpack.toml`. The schema is `deny_unknown_fields` (`blueprint/xtask/src/modpack.rs:8-40`), and the mod author, not the pack author, knows whether the client needs the mod.

### D4. Per-client content encoding

`JavaClient` and `Player` keep the connection type, the negotiated `NetworkPayloadSetup` and a content id mode `Real | Display`. `Real` applies only to NeoForge connections that completed the registry sync; every other connection is `Display`. `ReadyToPlay` carries that state out of `PendingConnection`.

The serialization key changes from `JavaMinecraftVersion` to a struct that carries the version and the content id mode and exposes the version. It goes where the version goes today: `write_packet_data`, `serialize_packet`, `serialize_packet_for_version`, `broadcast_java_grouped`, `collect_java_recipients_by_version` and `ChunkSender::encode_batch`. The mappers take the mode and return the real id in `Real` mode. Broadcasts serialize once per (version, mode) group. The mode enum lives in `pumpkin_data::dynamic`, next to the mappers; the key struct lives in pumpkin-protocol next to `ClientPacket` (`crates/pumpkin-protocol/src/lib.rs:251`), which already depends on pumpkin-util and pumpkin-data.

`CUpdateTags` is encoded per client too. `Display` clients get the generated tag lists as today. `Real` clients get the generated lists plus custom members and the mod-defined tags from the content registry (`tag_ids`, `explicit_tags`, `mod_tags`). This is the only way a client learns mod tags.

Alternative rejected: a thread-local or task-local mode. Async tasks interleave on one thread, so the mode of one client leaks into the packet of another.

Alternative rejected: a per-client copy of the mappers. Two code paths for every egress drift apart, and the phase 2 egress rule (one mapper per id type) stops being checkable with a grep.

Encoding call sites (paths in pumpkin-protocol are under `crates/pumpkin-protocol/src/`):

| Egress | Call site | Mapper today | Change |
|:--|:--|:--|:--|
| Block update | `java/client/play/block_update.rs:40` | `java_block_state_id` (`java/client/play/mod.rs:329`) | mapper takes the mode from the key |
| Multi block update | `java/client/play/multi_block_update.rs:77,94,109` | `state_id.to_java_network_id()` called directly | route through `java_block_state_id` with the mode |
| Level event 2001, world event | `java/client/play/level_event.rs:59`, `worldevent.rs:60` | `java_level_event_data` (`mod.rs:349`) | mode |
| Block event | `java/client/play/block_event.rs:53` | `java_block_id` (`mod.rs:338`) | mode |
| Entity metadata block states | `java/client/play/entity_metadata.rs:150` (BLOCK_STATE, OPTIONAL_BLOCK_STATE) | `java_block_state_id` | mode |
| Falling block spawn data | `java/client/play/spawn_entity.rs:221` | `java_block_state_id` | mode |
| Block particles | `particle.rs:181,220`, `entity_metadata.rs:178` | `write_particle_data` (`mod.rs:365`) | mode |
| Chunk palette | `crates/pumpkin-world/src/chunk/palette.rs:727` `convert_network` (single 731, indirect 749, direct 770), written by `crates/pumpkin-core/src/net/java/chunk_data/v1_18.rs:~96` | `BlockStateId::to_java_network_id` | `convert_network` takes the mode; the non-air counts (`palette.rs:890`, `v1_18.rs:88`) use server-side `is_air` and do not change |
| Chunk send | `crates/pumpkin-core/src/entity/player.rs:2830` (`per_player_cache`) -> `ChunkSender::encode_batch` (`chunk_sender.rs:274`) | `CChunkData::write_packet_data(.., &CURRENT_MC_VERSION)` | pass the player's key; the cache is per player, so it needs no grouping |
| Entity spawn | `crates/pumpkin-core/src/entity/mod.rs:2474` `create_spawn_packet` | `entity_type.to_java_network_id()` at packet build time | map at write time from the key, or build the packet per mode |
| Entity type in data components | `codec/data_component.rs:2352` | `EntityType::to_java_network_id` | mode |
| Item stacks | `codec/item_stack_seralizer.rs:546,559,572,628,766`, `codec/data_component.rs:1322` | `Item::to_java_network_id` | mode |
| Block and item id sets | `codec/data_component_impl/mod.rs:225,243` (`IDSetContent::registry_id`) | `Block` and `Item` mappers | mode |
| Recipe book | `java/client/play/recipe_book_add.rs:70,137,214,224,242,783,792,810` | `Item::to_java_network_id` | mode |
| Tags | `java/client/config/update_tags.rs` (`CUpdateTags`), built in `crates/pumpkin-core/src/net/java/login/known_packs.rs` | generated ids only | per-mode content: custom members and mod tags for `Real` |
| Mappers | `crates/pumpkin-data/src/dynamic.rs:1262` (`BlockStateId`), `:1417` (`Block`), `:1627` (`Item`), `:1699` (`EntityType`) | display id for custom ids | take the mode; `Real` returns the id itself |
| Creative echo (ingress) | `crates/pumpkin-data/src/item_stack/mod.rs:745` `ItemStack::is_displayed_as` | accepts the display item for a custom stack | applies to `Display` clients only; a `Real` client sends the custom id |
| Broadcast | `crates/pumpkin-core/src/world/mod.rs:984` `broadcast_java_clients`, `:999` `broadcast_java_grouped`, `:967` `collect_java_recipients_by_version`, `:6867` `broadcast_to_chunk` | one group per version | one group per (version, mode) |
| Entity tracker | `crates/pumpkin-core/src/entity/mod.rs:2929,2976,3013` (`serialize_packet_for_version`) | one serialization | one per (version, mode) |
| Per-recipient send | `crates/pumpkin-core/src/net/java/mod.rs:637` `JavaClient::serialize_packet` | version | the client's key |
| Statistics | `CAwardStats` | statistics of custom content are not sent | no change |

### D5. The other modded configuration payloads

- `c:version`: send `[1]`, wait for the client's list, kick when no version is common. `c:register`: version 1, protocol `play`, the optional play channels of the server channel registry that accept serverbound payloads; wait for the reply and store the client's set on the connection. In M3 the set is `neoforge:split`, which `NEOFORGE_PAYLOADS` (`neoforge.rs:77`) registers for play with no flow, as run (b) sends it.
- `neoforge:config_file`: one payload per synced config. First NeoForge's own `neoforge-synced.toml`, a fixture extracted from the run (b) capture. It is data, not source; the implementer picks `assets/neoforge/neoforge-synced.toml` or the crate's data directory and documents the choice. Then each native mod's synced config bytes, registered through `ModInit::synced_config(file_name, bytes)`. M4 `mod-config` replaces the bytes with a real `ModConfigSpec`. No reply.
- `neoforge:known_registry_data_maps`: send the NeoForge built-in data maps as captured, all `mandatory=false`, from a table in code. Read the reply and keep the client's data map ids on the connection; M4 data maps use them. `neoforge:registry_data_map_sync` is not sent in M3.
- `neoforge:extensible_enum_data`: send the 9 captured entries without extension data, from a table in code; wait for the ack.
- `neoforge:feature_flags`: empty set; wait for the ack.
- The captures go into the repo: runs (a), (b) and (c) as gzipped JSONL under `tools/pumpkin-neoforge-client/captures/neoforge-26.3.0.64-beta/`, with a README line that names the server version, the date and how they were recorded. The codec task extracts the payload bodies it needs as hex fixtures next to the codec tests.
- Missing codecs go into pumpkin-protocol with tests that decode the run (b) bytes: `known_registry_data_maps` and its reply, `extensible_enum_data` and its ack, `feature_flags_ack`. `advanced_add_entity` and `registry_data_map_sync` get no codec in M3: no mod entity carries spawn data and no data map exists before M4.
- `neoforge:split`: serverbound reassembly in the configuration and play handlers; clientbound splitting in the egress when the client declared `neoforge:split` and an encoded packet exceeds the vanilla limit (8 MiB with compression, 2 MiB without). The joined size of serverbound parts is capped at 8 MiB; above it, or on parts out of order, the server disconnects the client. It is the last task of the phase.
- Brand: IronPumpkin keeps `Pumpkin`. A NeoForge client detects the server from `neoforge:register`, not from the brand.
- Known packs: IronPumpkin keeps offering `minecraft:core` only.

Alternative rejected: skip `neoforge:config_file`. On a NeoForge connection the client loads SYNCED configs only from the server (an inference from `ClientNetworkRegistry.java:270`), so NeoForge's own config would stay unloaded on the client.

Alternative rejected: generate `neoforge-synced.toml` from code. The file is NeoForge's data; a fixture captured from the real server is the reference and changes only with the emulated version.

Alternative rejected: send the brand `neoforge`. The brand identifies IronPumpkin, and the NeoForge client detects a NeoForge server from `neoforge:register`.

### D6. The configuration disconnect carries a text component

`CConfigDisconnect` writes the reason as a plain string. Vanilla 26.3 writes a text component (network NBT) in the configuration disconnect packet, as in play. The implementer verifies this against the decompiled vanilla server (`javap` or the decompiled source of `ClientboundDisconnectPacket`), then fixes `CConfigDisconnect` in pumpkin-protocol and `pending.rs::kick`, so translatable reasons with arguments reach the client. If vanilla confirms, it is an upstream Pumpkin bug: own commit, an upstream PR draft in the issue, and an `upstream-pr` issue.

The `TextComponent` NBT deserializer (`crates/pumpkin-util/src/text/mod.rs`) also accepts a `with` list of plain strings. The headless client needs it to read NeoForge's disconnect, which run (c) shows as `{translate:"multiplayer.disconnect.incompatible", with:["NeoForge 26.3.0.64-beta"]}`.

Alternative rejected: keep the plain string and send the fallback text. The client then shows English text for every translatable reason, and the M3 kick could not carry its key, which the headless client asserts.

### D7. Verification

- Headless client: the expected files for the IronPumpkin NeoForge run gain the full M3 sequence. A registry expectation check asserts, per `frozen_registry` payload, the registry name, the id count and the first and last names. `--expect-disconnect <translation key>` covers the kick scenarios. `tests/live_server.rs` stays opt-in through `PUMPKIN_NEOFORGE_CLIENT_SERVER`.
- Boot test `neoforge_client_syncs_custom_content`: in the root workspace, with a test mod crate or a dev-only mod that registers one block, one item and one entity type, built in the shared `target/`. A child process, as in `crates/pumpkin-core/tests/content_manifest.rs`, boots the server, and the headless client runs in NeoForge mode. The test asserts the custom names in the block, item and entity type snapshots, with ids at `COUNT` and above. `examples/modpack` is a separate Cargo workspace with its own `Cargo.lock`; building it is a second server build, so it serves only the owner play test.
- Capture comparison: the IronPumpkin NeoForge run matches run (b) in channel order from `neoforge:frozen_registry_sync_start` to `finish_configuration`, with the registries limited to the three synced ones.
- Owner play test (a `needs-owner` issue at the end of M3): a real NeoForge 26.3 client with a Java test mod that registers the same block, item and entity type as the example content mod joins the example modpack and sees the block with its real model. A vanilla client gets the NeoForge "install NeoForge" screen. An agent builds the Java mod from the NeoForge MDK (gradle, network) if feasible; otherwise the owner builds it.

Alternative rejected: verify against the capture bytes. `registry_data`, the brand and `server_links` differ on purpose (D5), and NeoForge writes maps in hash order. Channel order and decoded registry contents are the stable parts.

## Risks / Trade-offs

- [A real client behaves differently from the source reading, for example on a partial sync of 3 of its 35 synced registries] -> the owner play test and the headless client registry checks; the headless client only checks completeness, not `applySnapshot`.
- [The key change touches every packet that writes a content id, in an upstream crate] -> one key struct passed where the version goes today, so new upstream packets compile against the same parameter; the call-site table above is the review checklist.
- [A broadcast to mixed clients serializes twice] -> one extra serialization per (version, mode) group, only when both modes are present.
- [A mod checks the brand string and expects `neoforge`] -> recorded; no M3 content does it.
- [`neoforge:poison` is missing from `damage_type` and `chat_type` has an extra `minecraft:raw` compared with NeoForge] -> not changed in M3; the owner play test shows whether a NeoForge client needs them.
- [The queue sends the resource pack after known packs, as vanilla does, so the byte stream of a server with a resource pack changes, also with detection off] -> accepted as vanilla parity; the queue task verifies the order on the vanilla jar first.
- [The serverbound configuration custom payload cap stays at 1 MiB (`MAX_PAYLOAD_SIZE`, `crates/pumpkin-protocol/src/java/server/config/plugin_message.rs:8`), not verified against vanilla] -> out of M3 scope.
- [The 30 s idle timeout resets per packet, and a client applying a snapshot is silent] -> the three M3 snapshots are about 87 KB in run (b) sizes; the timeout matters only for M4 rows.
- [The declaration order of `Sound`, `GameEvent` and `WindowType` may differ from the vanilla registry order] -> not used in M3; the M4 task that adds such a row verifies it first.
- [NeoForge marks the payloads `@ApiStatus.Internal`; they change per release] -> `EMULATED_NEOFORGE_VERSION` names the version the tables and the fixture come from; a new version means a new capture.

## Open points for the orchestrator

None.
