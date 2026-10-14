# Proposal

## Why

A NeoForge 26.3 client with the same mods as the server cannot see custom blocks, items and entity types on IronPumpkin today: the server sends no registry sync, so every client gets the display ids of phase 2, and a vanilla client joins a server whose mods need the client side. The owner decided to stay on Minecraft 26.3 and port mods and clients to it, so phase M3 reproduces the NeoForge 26.3.x configuration handshake for that version only.

Phase goal: a NeoForge 26.3 client whose mods match the server's native mods joins IronPumpkin, syncs the content ids, and sees custom content with its real ids; a vanilla client is kicked when a mod requires the client side, and otherwise joins with display ids as today.

## What Changes

- A configuration task queue on the pending connection. The registry sync, the vanilla known packs step, the resource pack step and the NeoForge configuration tasks run as tasks in the NeoForge order, after the phase 1 probe. Each task finishes at once or waits for one named client reply. The order follows vanilla 26.3: `update_enabled_features` before the queue, the resource pack after known packs. With `detect_neoforge_clients = false` and no resource pack the byte stream does not change. Client-required mods force detection on.
- A registry sync task for NeoForge connections: `frozen_registry_sync_start`, one `frozen_registry` per synced registry (`minecraft:block`, `minecraft:item`, `minecraft:entity_type`), `frozen_registry_sync_completed`, and a wait for the client echo. The snapshots come from a new public accessor of the frozen content tables.
- A client gate: native mods declare `client_required` (default true), the server keeps the native mod list at runtime, and a client of connection type Other is kicked with the NeoForge "install NeoForge" text when a client-required mod is loaded.
- Per-client content encoding: the connection type, the negotiated channels and a content id mode (`Real` or `Display`) go into play. Packet serialization takes the mode, the id mappers return real ids in `Real` mode, broadcasts serialize once per version and mode, and `CUpdateTags` carries custom members and mod tags to `Real` clients.
- The other NeoForge configuration payloads: `c:version`, `c:register`, `neoforge:config_file` (NeoForge's own synced config and native mod synced configs), `neoforge:known_registry_data_maps`, `neoforge:extensible_enum_data`, `neoforge:feature_flags`, with their reply waits, the missing codecs, and `neoforge:split` in both directions.
- The configuration disconnect carries a text component, as vanilla 26.3 writes it, so translatable kick reasons with arguments reach the client. If the decompiled vanilla server confirms it, it is an upstream Pumpkin bug with its own commit.
- The headless client asserts the full M3 sequence, the registry contents and expected disconnects; a boot test in the root workspace runs it against a server with a test mod that registers a block, an item and an entity type.

Out of scope: syncing the other 32 registries of the NeoForge capture, `neoforge:registry_data_map_sync`, `neoforge:advanced_add_entity`, mod configuration specs (M4 `mod-config`), mod network channels (M4 `network`), Bedrock clients.

## Capabilities

### New Capabilities

- `configuration-task-queue`: the ordered configuration tasks of a Java connection, the one-reply-per-task rule and the vanilla byte stream guarantee.
- `neoforge-registry-sync`: the frozen registry sync for NeoForge connections: the synced registries, the snapshot content and the echo wait.
- `neoforge-client-gate`: which clients the server accepts: the `client_required` flag, the runtime native mod list and the kick of Other connections.
- `per-client-content-encoding`: the content id mode of a connection, real ids or display ids in every egress, the broadcast grouping and the tags per client.
- `neoforge-configuration-payloads`: the content and the reply waits of `c:version`, `c:register`, `neoforge:config_file`, the data maps, the extensible enums and the feature flags, `neoforge:split`, and the configuration disconnect component.

### Modified Capabilities

- `content-registry`: a public accessor of the frozen tables and a registry snapshot per synced registry (ADDED requirements only).

## Impact

- Crates: pumpkin-core (`net/java/pending.rs`, `net/java/neoforge.rs`, `net/java/login/`, `net/java/mod.rs`, `net/mod.rs`, `entity/player.rs`, `entity/mod.rs`, `world/mod.rs`, `net/java/chunk_data/`, `plugin/startup.rs`), pumpkin-protocol (codecs, `ClientPacket::write_packet_data`, `CConfigDisconnect`, `CUpdateTags`, the item stack and data component codecs, packet encoder and decoder), pumpkin-data (`dynamic.rs`), pumpkin-world (`chunk/palette.rs`), pumpkin-util (`text/mod.rs`), ironpumpkin-mods (`NativeMod`, `ModInit`), `tools/pumpkin-neoforge-client`, `examples/modpack`.
- `ClientPacket::write_packet_data` and the serialization helpers take a key that carries the protocol version and the content id mode. Every packet implementation that writes a content id changes its signature. This adds upstream merge friction in pumpkin-protocol.
- `NativeMod` gains a defaulted method and `ModInit` gains `synced_config`: additive for mod crates.
- `startup::set_native_mod_ids` is replaced by `startup::set_native_mods`.
- With `detect_neoforge_clients = false` and no resource pack the configuration byte stream stays as today. A server with a resource pack sends it after known packs, as vanilla does.

## Verification

- pumpkin-protocol tests decode the bytes of the run (b) capture of a NeoForge 26.3.0.64-beta server for every new codec.
- The headless client in NeoForge mode against a booted IronPumpkin matches run (b) in channel order from `neoforge:frozen_registry_sync_start` to `finish_configuration`, with the registries limited to the three synced ones, and its registry expectations pass.
- The boot test `neoforge_client_syncs_custom_content` boots a server with that test mod and asserts the custom names in the block, item and entity type snapshots with ids at the generated counts and above.
- The headless client in vanilla mode is kicked with `neoforge.network.negotiation.failure.vanilla.client.not_supported` when a client-required mod is loaded.
- Owner play test: a real NeoForge 26.3 client with a Java test mod joins and sees the custom block with its real model; a vanilla client gets the "install NeoForge" screen.
