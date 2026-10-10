# Spec Delta

## Purpose

Defines the content and the reply waits of the NeoForge configuration payloads after the registry sync, the splitting of large packets for NeoForge clients, and the configuration disconnect reason, as a NeoForge 26.3 server sends them.

## ADDED Requirements

### Requirement: Common version
The common version task SHALL send `c:version` with the version list `[1]` and wait for the client's `c:version`. The server SHALL disconnect the client when the client's list does not contain 1. Status: planned (T7).

#### Scenario: Matching version
- **WHEN** the client answers `c:version` with `[1]`
- **THEN** the common register task starts

#### Scenario: No common version
- **WHEN** the client answers `c:version` with `[2]`
- **THEN** the server disconnects the client

### Requirement: Common register
The common register task SHALL send `c:register` with version 1, protocol `play` and the optional play channels of the server channel registry that accept serverbound payloads. It SHALL wait for the client's `c:register` and keep the client's channel set on the connection. Status: planned (T7).

#### Scenario: Client play channels
- **WHEN** the client answers `c:register` with 9 play channels
- **THEN** the connection keeps those 9 channels and the sync config task starts

### Requirement: NeoForge synced config
The sync config task SHALL send one `neoforge:config_file` with the file name `neoforge-synced.toml` and the bytes of the file captured from the NeoForge 26.3.0.64-beta server. The task SHALL NOT wait for a reply. Status: planned (T7).

#### Scenario: NeoForge file
- **WHEN** a NeoForge client that declared `neoforge:config_file` runs the sync config task
- **THEN** the first `neoforge:config_file` names `neoforge-synced.toml` and holds the 1168 bytes of the run (b) capture

### Requirement: Native mod synced configs
A native mod SHALL be able to register synced config bytes with a file name during its `init`. The sync config task SHALL send one `neoforge:config_file` per registered file after the NeoForge file, in mod id order. Status: planned (T7).

#### Scenario: Mod config
- **WHEN** a native mod registers the synced config `testmod-server.toml` with 12 bytes
- **THEN** the client receives a second `neoforge:config_file` with that name and those bytes

### Requirement: Data map negotiation
For a NeoForge connection the server SHALL send `neoforge:known_registry_data_maps` with the registries and data map ids of the NeoForge 26.3.0.64-beta capture, every one with `mandatory = false`, and wait for `neoforge:known_registry_data_maps_reply`. It SHALL keep the client's data map ids on the connection. Status: planned (T7).

#### Scenario: Empty reply
- **WHEN** the client answers with an empty map
- **THEN** the connection keeps no data map id and the extensible enum check starts

### Requirement: Extensible enum check
For a NeoForge connection the server SHALL send `neoforge:extensible_enum_data` with the 9 enum entries of the NeoForge 26.3.0.64-beta capture, each without extension data, and wait for `neoforge:extensible_enum_ack`. Status: planned (T7).

#### Scenario: Acknowledgement
- **WHEN** the client sends `neoforge:extensible_enum_ack`
- **THEN** the feature flag check starts

### Requirement: Feature flag check
For a NeoForge connection the server SHALL send `neoforge:feature_flags` with an empty set and wait for `neoforge:feature_flags_ack`. Status: planned (T7).

#### Scenario: Acknowledgement
- **WHEN** the client sends `neoforge:feature_flags_ack`
- **THEN** the server sends `finish_configuration`

### Requirement: Modded checks on other connections
On a connection of type Other, the data map negotiation, the extensible enum check and the feature flag check SHALL finish without sending a payload. Status: planned (T7).

#### Scenario: Vanilla client
- **WHEN** a vanilla client joins with detection on and only client-optional mods loaded
- **THEN** it receives no `neoforge:known_registry_data_maps`, `neoforge:extensible_enum_data` or `neoforge:feature_flags`

### Requirement: Codecs from the capture
pumpkin-protocol SHALL decode and write `neoforge:known_registry_data_maps` and its reply, `neoforge:extensible_enum_data` and its acknowledgement, and `neoforge:feature_flags_ack`, with bounded counts and no trailing bytes. Status: implemented (#87).

#### Scenario: Captured bytes
- **WHEN** the codecs decode the bodies of these payloads from the run (b) capture
- **THEN** the data map payload has 7 registries with every entry `mandatory = false`, the enum payload has 9 entries with 6 `CLIENTBOUND` and 3 `BIDIRECTIONAL`, and writing the decoded values gives the same bytes

### Requirement: Split serverbound packets
When a client sends `neoforge:split` parts in configuration or play, the server SHALL join the parts from the first part (state 1) to the last part (state 2) and decode the result as one packet of the current protocol. A part out of order, or a joined size above 8 MiB, SHALL disconnect the client. Status: planned (T10).

#### Scenario: Three parts
- **WHEN** a client sends a packet as three `neoforge:split` parts with states 1, 0 and 2
- **THEN** the server handles the joined packet once

#### Scenario: Joined size above the cap
- **WHEN** a client sends `neoforge:split` parts whose joined size exceeds 8 MiB
- **THEN** the server disconnects the client before it decodes the packet

### Requirement: Split clientbound packets
For a client that declared `neoforge:split`, the server SHALL send an encoded packet larger than the limit as `neoforge:split` parts. The limit SHALL be 8 MiB uncompressed with compression on and 2 MiB without compression. For other clients the size check SHALL stay as before. Status: planned (T10).

#### Scenario: Large packet
- **WHEN** a packet of 9 MiB goes to a client with `neoforge:split` and compression on
- **THEN** the client receives parts with states 1, 0 and 2 whose payloads join to the encoded packet

#### Scenario: Small packet
- **WHEN** a packet of 71788 bytes goes to the same client
- **THEN** the server sends it unsplit

### Requirement: Configuration disconnect component
The configuration disconnect packet SHALL carry the reason as a text component in network NBT, as the decompiled vanilla 26.3 server writes it, so that a translatable reason keeps its key, fallback and arguments. Status: implemented (#89).

#### Scenario: Translatable kick
- **WHEN** the server kicks a client in configuration with a translatable reason and one argument
- **THEN** the client decodes a component with that translate key and argument

### Requirement: Plain string arguments in components
The text component NBT reader SHALL accept a `with` list of plain strings. Status: implemented (#89).

#### Scenario: NeoForge incompatible reason
- **WHEN** the headless client reads the run (c) disconnect `{translate:"multiplayer.disconnect.incompatible", with:["NeoForge 26.3.0.64-beta"]}`
- **THEN** it records the key and the argument and not `<unparsed reason>`
