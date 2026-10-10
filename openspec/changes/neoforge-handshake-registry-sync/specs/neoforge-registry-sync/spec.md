# Spec Delta

## Purpose

Defines the frozen registry sync that IronPumpkin sends to NeoForge clients, so that a client with the same mods uses the server's numeric ids for blocks, items and entity types.

## ADDED Requirements

### Requirement: Synced registries
The server SHALL sync exactly the registries `minecraft:block`, `minecraft:item` and `minecraft:entity_type`. `neoforge:frozen_registry_sync_start` SHALL list exactly these registries, and the server SHALL send one `neoforge:frozen_registry` for each listed registry, then `neoforge:frozen_registry_sync_completed`. Status: planned (T6).

#### Scenario: Registry list
- **WHEN** a NeoForge client runs the registry sync
- **THEN** `neoforge:frozen_registry_sync_start` lists 3 registries, and the client receives 3 `neoforge:frozen_registry` payloads with the same names before `neoforge:frozen_registry_sync_completed`

### Requirement: Snapshot ids
A snapshot SHALL map every id of the registry to its name: the generated entries with ids from 0 to the generated count minus one, then the custom entries in allocation order. Placeholder entries (content of a mod the server no longer has) SHALL be left out, so their ids are gaps. Status: planned (T6).

#### Scenario: Server without mods
- **WHEN** no native mod registers content
- **THEN** the block snapshot has 1286 ids from `minecraft:air` to `minecraft:firefly_bush`, the item snapshot 1658 ids from `minecraft:air` to `minecraft:ominous_bottle`, and the entity type snapshot 161 ids from `minecraft:acacia_boat` to `minecraft:fishing_bobber`, as in the NeoForge 26.3.0.64-beta capture

#### Scenario: Custom block, item and entity type
- **WHEN** the test mod of the boot test `neoforge_client_syncs_custom_content` registers one block, one item and one entity type
- **THEN** the block snapshot maps id 1286, the item snapshot id 1658 and the entity type snapshot id 161 to the namespaced names of these entries

#### Scenario: Placeholder
- **WHEN** the content manifest names a custom block whose mod is missing
- **THEN** the block snapshot has no entry for the placeholder's id, and the ids before and after it are present

### Requirement: Namespaced names and empty aliases
Every name in a snapshot SHALL be namespaced: generated entries carry the `minecraft` namespace. The alias map of every snapshot SHALL be empty. Status: planned (T6).

#### Scenario: Generated name
- **WHEN** the block snapshot is decoded
- **THEN** id 0 maps to `minecraft:air`, not to `air`, and the alias count is 0

### Requirement: No block state snapshot
The server SHALL NOT send a snapshot of block states. The client rebuilds state ids from the block order, and the content registry lays out custom states the same way. Status: planned (T6).

#### Scenario: Registry names
- **WHEN** the sync payloads are recorded
- **THEN** no `neoforge:frozen_registry` payload names a registry other than block, item and entity type

### Requirement: Echo wait
After `neoforge:frozen_registry_sync_completed` the server SHALL wait for the client's echo of `neoforge:frozen_registry_sync_completed` before the known packs task starts. A client that disconnects during the wait SHALL be logged with its name and the registry sync task. Status: planned (T6).

#### Scenario: Echo
- **WHEN** the client echoes `neoforge:frozen_registry_sync_completed`
- **THEN** the server sends `select_known_packs`

### Requirement: No sync for other clients
A connection of type Other SHALL NOT receive any `neoforge:frozen_registry_sync_start`, `neoforge:frozen_registry` or `neoforge:frozen_registry_sync_completed` payload. Status: planned (T6).

#### Scenario: Vanilla client
- **WHEN** a vanilla client joins with detection on and only client-optional mods loaded
- **THEN** it receives no registry sync payload and reaches play
