# Spec Delta

## Purpose

Defines how the server encodes custom content ids per client: real ids for NeoForge clients that synced the registries, display ids for every other client, in every packet and in the tags.

## ADDED Requirements

### Requirement: Connection state in play
The connection type, the negotiated channel setup and the content id mode of a connection SHALL stay available in play for the player of that connection. Status: implemented (#90).

#### Scenario: NeoForge player
- **WHEN** a NeoForge client finishes the configuration with the channel map `channels/neoforge-26.3.toml`
- **THEN** its player has connection type NeoForge and a channel setup with 11 configuration and 9 play channels

### Requirement: Content id mode
A connection SHALL use the `Real` content id mode only when it is a NeoForge connection and the client echoed `neoforge:frozen_registry_sync_completed`. Every other connection SHALL use the `Display` mode. Status: implemented (#90, #95).

#### Scenario: Vanilla client
- **WHEN** a vanilla client joins with only client-optional mods loaded
- **THEN** its mode is `Display`

#### Scenario: NeoForge client
- **WHEN** a NeoForge client completes the registry sync
- **THEN** its mode is `Real`

### Requirement: Real ids for real-mode clients
In `Real` mode every packet SHALL write the real id of a custom block state, block, item or entity type. In `Display` mode it SHALL write the id of the display entry. Generated ids SHALL be the same in both modes. The real id of a block, item or entity type SHALL be its server id. Status: implemented (#95).

#### Scenario: Chunk with a custom block
- **WHEN** a chunk holds a custom block whose display is the redstone lamp, and it is sent to a `Real` client and to a `Display` client
- **THEN** the `Real` client's palette holds the custom state id at or above 35723, and the `Display` client's palette holds the redstone lamp state id

#### Scenario: Custom item stack
- **WHEN** a player with the only custom item in the inventory is a `Real` client
- **THEN** the inventory packet writes item id 1658 for the stack

#### Scenario: Custom entity
- **WHEN** a custom entity type spawns near a `Real` client and a `Display` client
- **THEN** the `Real` client's spawn packet carries the custom entity type id at or above 161, and the `Display` client's carries the display type id

### Requirement: Real block state ids are client-side ids
The real id of a custom block state SHALL be the id the NeoForge client gives it. The client numbers block states itself after the registry sync, appending the states of each synced block in block id order. Placeholder blocks are not synced, so their states are absent on the client and the states of later custom blocks shift down. Status: implemented (#95).

#### Scenario: Placeholder block before a custom block
- **WHEN** a placeholder block sorts before the custom block `test:lamp` and a chunk with `test:lamp` is sent to a `Real` client
- **THEN** the palette holds the state id 35723 for the first state of `test:lamp`, the generated state count, not its server state id

### Requirement: Placeholders keep their display id
A placeholder block, block state, item or entity type SHALL be written with the id of its display entry in both modes, because no client knows its real id: the registry sync leaves placeholders out. Status: implemented (#95).

#### Scenario: Placeholder item in both modes
- **WHEN** a stack of a placeholder item whose display is the emerald is sent to a `Real` client and to a `Display` client
- **THEN** both packets carry the emerald item id

### Requirement: Every content id egress follows the mode
The mode SHALL apply to the chunk palettes, block updates, multi block updates, block events, level events, entity metadata block states, block particles, entity spawns, item stacks, data components that hold blocks, items or entity types, and the recipe book. Status: implemented (#95).

#### Scenario: Block update
- **WHEN** a custom block is placed near a `Real` client
- **THEN** the block update carries the real state id

### Requirement: Serialization per version and mode
A broadcast SHALL serialize a packet once per group of recipients with the same protocol version and content id mode, and SHALL send each group its own bytes. Status: implemented (#95).

#### Scenario: Mixed recipients
- **WHEN** a block update for a custom block is broadcast to two `Real` clients and one `Display` client
- **THEN** the packet is serialized twice, and both `Real` clients receive the same bytes

### Requirement: Tags for display clients
A `Display` client SHALL receive in `update_tags` the generated tag lists with generated ids only, as before this change. Status: implemented (#95).

#### Scenario: Display tags
- **WHEN** a vanilla client receives `update_tags` on a server with custom content
- **THEN** no tag holds an id at or above the generated count, and no mod tag is listed

### Requirement: Tags for real-mode clients
A `Real` client SHALL receive in `update_tags` the generated tag lists plus the custom ids of each generated tag, and every mod-defined block, item and entity type tag with its members. Status: implemented (#95).

#### Scenario: Custom member and mod tag
- **WHEN** a custom block lists `minecraft:mineable/pickaxe` and a mod registers the block tag `test:lamps` with that block
- **THEN** the `Real` client's `minecraft:mineable/pickaxe` tag holds the block's real id, and the tag `test:lamps` is present with that id
