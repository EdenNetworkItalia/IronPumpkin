# Spec Delta

## Purpose

Defines which clients the server accepts at the configuration handshake when native mods are loaded: the client-required flag of a mod, the native mod list at runtime, and the kick of clients that cannot have the required mods.

## ADDED Requirements

### Requirement: Client-required by default
A native mod SHALL be client-required unless it opts out. `NativeMod::client_required` SHALL return true when the mod does not override it. Status: implemented (#88).

#### Scenario: Default
- **WHEN** a native mod implements only `id`, `display_name`, `version` and `init`
- **THEN** the native mod list marks it client-required

#### Scenario: Opt out
- **WHEN** a native mod overrides `client_required` to return false
- **THEN** the native mod list marks it client-optional

### Requirement: Native mod list at runtime
The server SHALL receive the native mod list at startup, before the content freeze, with the id, the display name, the version and the client-required flag of each mod, sorted by id. The list SHALL be readable while the server runs. Status: implemented (#88).

#### Scenario: One native mod
- **WHEN** a server boots with one native mod `test-mod` that does not override `client_required`
- **THEN** the native mod list has one entry `test-mod` with its display name, its version and `client_required = true`

### Requirement: Kick of other clients
With detection on, the server SHALL disconnect a connection of type Other after the probe pong and before the brand when at least one client-required native mod is loaded. The reason SHALL be the translatable `neoforge.network.negotiation.failure.vanilla.client.not_supported` with the argument `26.3.0.64-beta`. Status: implemented (#94).

#### Scenario: Vanilla client and a client-required mod
- **WHEN** a vanilla client joins a server with a client-required native mod and `detect_neoforge_clients = true`
- **THEN** it receives no `minecraft:brand` and a configuration disconnect with the translate key `neoforge.network.negotiation.failure.vanilla.client.not_supported` and the argument `26.3.0.64-beta`

#### Scenario: Headless client expects the kick
- **WHEN** the headless client runs in vanilla mode with `--expect-disconnect neoforge.network.negotiation.failure.vanilla.client.not_supported` against a server with a client-required native mod
- **THEN** it exits with 0

### Requirement: Kick fallback text
A translatable text component SHALL carry an optional fallback, written to JSON and NBT only when it is set, in the vanilla shape. The kick reason SHALL carry the fallback "You are trying to connect to a server that is running NeoForge, but you are not. Please install NeoForge Version: %s to connect to this server.", so that a vanilla client without the translation key shows the text with the version. Status: implemented (#94).

#### Scenario: Fallback
- **WHEN** the disconnect reason of the kick is decoded
- **THEN** its `fallback` field equals the NeoForge text and its `with` list holds the version string

### Requirement: Kick log
The server SHALL log the kick at info level with the player name and the ids of the client-required mods that caused it. Status: implemented (#94).

#### Scenario: Log line
- **WHEN** a vanilla client is kicked by a server whose only client-required mod is `test-mod`
- **THEN** the log has one info line with the player name and `test-mod`

### Requirement: Client-required mods force detection on
When at least one loaded native mod is client-required and `detect_neoforge_clients` is false, the server SHALL log a warning at startup that names those mods, and SHALL run as if `detect_neoforge_clients` were true. Status: implemented (#94).

#### Scenario: Detection off in the config
- **WHEN** a server with the client-required mod `test-mod` starts with `detect_neoforge_clients = false`
- **THEN** the log has one warning that names `test-mod`, and a vanilla client that joins receives the probe and the kick

#### Scenario: Only client-optional mods
- **WHEN** a server whose only native mod is client-optional starts with `detect_neoforge_clients = false`
- **THEN** no warning is logged and no client receives the probe

### Requirement: Client-optional mods accept vanilla clients
When every loaded native mod is client-optional, a connection of type Other SHALL join as before, with display ids and no registry sync. Status: implemented (#94).

#### Scenario: Server-only mod
- **WHEN** a vanilla client joins a server whose only native mod returns false from `client_required`
- **THEN** the client reaches play and receives the display ids of the custom content

### Requirement: NeoForge clients are checked by the registry sync
The server SHALL NOT check the mod list of a NeoForge connection, because the NeoForge handshake does not exchange it. A NeoForge client that lacks a client-required mod SHALL receive the mod's names in the registry snapshots. Status: implemented (#94).

#### Scenario: Client without the mod
- **WHEN** a real NeoForge 26.3 client without the example content mod joins the example modpack
- **THEN** the server sends the snapshots, and the client disconnects with `neoforge.network.registries.sync.server-with-unknown-keys`
